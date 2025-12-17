use std::{collections::VecDeque, fmt, io, net::ToSocketAddrs, time::Duration};

use log::*;
use mio::{net::TcpStream, Events, Interest, Poll, Token};
use native_tls::{
    HandshakeError as TlsHandshakeError, MidHandshakeTlsStream, TlsConnector, TlsStream,
};
use thiserror::Error as ThisError;
use tungstenite::client::IntoClientRequest;
use tungstenite::error::{TlsError, UrlError};
use tungstenite::handshake::client::{ClientHandshake, Request};
use tungstenite::handshake::MidHandshake as MidHandshakeWebSocket;
use tungstenite::stream::{MaybeTlsStream, Mode};
use tungstenite::HandshakeError as WsHandshakeError;
use tungstenite::{Message, WebSocket};

use crate::error::{Error, ProtocolError};
use crate::protocol::{ClientMessage, ServerMessage};

/// A WebSocket wrapper that encodes and decodes Archipelago protocol messages.
pub(crate) struct Socket<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// The Mio struct that polls for new events, if we own it. If this is None,
    /// the poll is owned by the caller so that they can wait on multiple
    /// different events at once.
    _poll: Option<Poll>,

    /// The WebSocket that this wraps.
    inner: WebSocket<MaybeTlsStream<TcpStream>>,

    /// The buffer of messages that have yet to be returned, in cases where the
    /// server sends multiple messages at a time.
    messages: VecDeque<ServerMessage<S>>,
}

impl<S> Socket<S>
where
    S: for<'a> serde::de::Deserialize<'a>,
{
    /// Begins the process of establishing a WebSocket connection using the
    /// given [request] (which may be passed as a simple `ws://` or `wss://`
    /// URL).
    ///
    /// This returns a [MidHandshakeSocket] for which
    /// [MidHandshakeSocket.handshake] must be called until it returns a
    /// [Socket].
    pub(crate) fn connect(request: impl IntoClientRequest) -> Result<MidHandshakeSocket, Error> {
        let request = request.into_client_request()?;
        let domain = request
            .uri()
            .host()
            .map(|h| h.to_string())
            .ok_or(UrlError::NoHostName)?;
        let port = request
            .uri()
            .port_u16()
            .or_else(|| match request.uri().scheme_str() {
                Some("wss") => Some(443),
                Some("ws") => Some(80),
                _ => None,
            })
            .ok_or(UrlError::UnsupportedUrlScheme)?;

        log::debug!("Resolving DNS for {domain}...");
        let Some(mut addr) = format!("{domain}:{port}").to_socket_addrs()?.next() else {
            return Err(UrlError::UnableToConnect(format!("{domain}:{port}")).into());
        };
        addr.set_port(port);

        log::debug!("Establishing TCP connection to {addr}...");
        let mut socket = TcpStream::connect(addr)?;
        let poll = Poll::new().map_err(tungstenite::Error::Io)?;
        poll.registry().register(
            &mut socket,
            Token(0),
            Interest::READABLE | Interest::WRITABLE,
        )?;

        let handshake = MidHandshakeSocket {
            poll,
            state: MidHandshakeState::TcpConnecting {
                request,
                domain,
                socket,
            },
        };

        match handshake.force_handshake::<S>(false) {
            // Since multiple handshake steps require remote responses to
            // requests we send out, there's no way the whole process will
            // finish in the initial call.
            Ok(_) => panic!("socket connection completed way too fast"),
            Err(SocketHandshakeError::WouldBlock(handshake)) => Ok(handshake),
            Err(SocketHandshakeError::Failure(err)) => Err(err),
        }
    }

    /// Returns the next message from this socket, if one is available.
    ///
    /// Returns an error if this receives an unexpected message type, either at
    /// the WebSocket or Archipelago levels. Returns `Ok(None)` if there are no
    /// messages to receive. This automatically handles
    /// [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Result<Option<ServerMessage<S>>, Error> {
        if let Some(message) = self.messages.pop_front() {
            return Ok(Some(message));
        }

        loop {
            match self.inner.read() {
                Ok(Message::Text(bytes)) => {
                    debug!("--> {bytes}");
                    // We can overwrite messages without worrying about losing
                    // data because we return early above unless it's empty.
                    self.messages = serde_json::from_str(&bytes).map_err(|error| {
                        ProtocolError::Deserialize {
                            json: bytes.to_string(),
                            error,
                        }
                    })?;
                    // Recurse so we gracefully handle the case where messages
                    // is still empty.
                    return self.try_recv();
                }

                Ok(Message::Binary(bytes)) => {
                    return Err(ProtocolError::BinaryMessage(bytes.to_vec()).into())
                }

                // Other message types like pings, frames, and closes are
                // handled internally by the tungstenite. We can ignore them.
                Ok(_) => {}

                Err(tungstenite::Error::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {
                    // Make sure we reply to any pings or close messages.
                    self.inner.flush()?;
                    return Ok(None);
                }

                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Sends [message] to the server.
    pub(crate) fn send(&mut self, message: ClientMessage) -> Result<(), Error> {
        self.inner
            .send(Message::Text(match serde_json::to_string(&[&message]) {
                Ok(text) => {
                    debug!("<-- {text}");
                    text.into()
                }
                Err(error) => return Err(Error::Serialize { message, error }),
            }))?;
        self.inner.flush()?;
        Ok(())
    }
}

/// The intermediate state of a [Socket] during the process of connecting, doing
/// a TLS handshake if necessary, and then doing a WebSocket handshake.
///
/// This persists the connection state across each step without requiring the
/// thread to block and keep it all on the stack.
pub(crate) struct MidHandshakeSocket {
    poll: Poll,

    /// The current intermediate state of the connection.
    state: MidHandshakeState,
}

impl MidHandshakeSocket {
    /// Continues the handshake process.
    ///
    /// This should be called periodically (often once each frame) to progress
    /// the internal handshake process. Once the process succeeds, this will
    /// return a fully-initialized [Socket].
    pub(crate) fn handshake<S>(mut self) -> Result<Socket<S>, SocketHandshakeError>
    where
        S: for<'a> serde::de::Deserialize<'a>,
    {
        let mut events = Events::with_capacity(1024);

        // Ignore the output here. This is going to time out any time there
        // aren't events immediately available, and the MIO docs say that it
        // may not return `Ok(())` in the future without indicating what it
        // will return. We just check the list of events to see if there are
        // any to process.
        let _ = self.poll.poll(&mut events, Some(Duration::ZERO));

        // If there aren't any events on the socket, there's no point in
        // checking for updates. If there are, we don't care what they are
        // specifically because the only thing we're watching is the
        // underlying socket.
        if events.is_empty() {
            return Err(SocketHandshakeError::WouldBlock(self));
        }

        self.force_handshake(events.into_iter().any(|e| e.is_writable()))
    }

    /// Like [handshake], but doesn't abort early if [poll] has no events.
    fn force_handshake<S>(mut self, writable: bool) -> Result<Socket<S>, SocketHandshakeError>
    where
        S: for<'a> serde::de::Deserialize<'a>,
    {
        use MidHandshakeState::*;
        match self.state {
            // The first state follows the logic recommended by
            // https://docs.rs/mio/latest/mio/net/struct.TcpStream.html#method.connect.
            TcpConnecting {
                request,
                domain,
                socket,
            } => {
                match socket.take_error() {
                    Ok(Some(err)) => Err(err),
                    Ok(None) => Ok(()),
                    Err(err) => Err(err),
                }?;
                match socket.peer_addr() {
                    // TODO: Socket may not be writeable here. Need a better way
                    // to detect that.
                    Ok(_) if writable => match tungstenite::client::uri_mode(request.uri())? {
                        Mode::Plain => {
                            log::debug!("Upgrading to WebSocket...");
                            MidHandshakeSocket {
                                poll: self.poll,
                                state: MidHandshakeState::WsConnecting {
                                    handshake: ClientHandshake::start(
                                        MaybeTlsStream::Plain(socket),
                                        request,
                                        None,
                                    )?,
                                },
                            }
                            .force_handshake(true)
                        }
                        Mode::Tls => {
                            log::debug!("Upgrading to TLS...");
                            let connector =
                                TlsConnector::new().map_err(|e| TlsError::Native(Box::new(e)))?;
                            Self::handle_tls_handshake(
                                self.poll,
                                request,
                                connector.connect(domain.as_str(), socket),
                            )
                        }
                    },
                    Err(err)
                        if err.kind() != io::ErrorKind::NotConnected
                            && err.raw_os_error() != Some(libc::EINPROGRESS) =>
                    {
                        Err(err.into())
                    }
                    _ => {
                        self.state = TcpConnecting {
                            request,
                            domain,
                            socket,
                        };
                        Err(SocketHandshakeError::WouldBlock(self))
                    }
                }
            }

            TlsConnecting { request, handshake } => {
                Self::handle_tls_handshake(self.poll, request, handshake.handshake())
            }

            WsConnecting { handshake } => match handshake.handshake() {
                Ok((inner, response)) => {
                    log::debug!(
                        "WebSocket response: {}\n{:?}",
                        response.status(),
                        response.headers()
                    );

                    Ok(Socket {
                        _poll: Some(self.poll),
                        inner,
                        messages: Default::default(),
                    })
                }
                Err(WsHandshakeError::Interrupted(handshake)) => {
                    self.state = WsConnecting { handshake };
                    Err(SocketHandshakeError::WouldBlock(self))
                }
                Err(WsHandshakeError::Failure(err)) => Err(err.into()),
            },
        }
    }

    /// Returns the next state based on the result of a (potentially in-progress)
    /// TLS handshake.
    fn handle_tls_handshake<S>(
        poll: Poll,
        request: Request,
        result: Result<TlsStream<TcpStream>, TlsHandshakeError<TcpStream>>,
    ) -> Result<Socket<S>, SocketHandshakeError>
    where
        S: for<'a> serde::de::Deserialize<'a>,
    {
        match result {
            Ok(socket) => {
                log::debug!("Upgrading to WebSocket...");
                MidHandshakeSocket {
                    poll,
                    state: MidHandshakeState::WsConnecting {
                        handshake: ClientHandshake::start(
                            MaybeTlsStream::NativeTls(socket),
                            request,
                            None,
                        )?,
                    },
                }
                .force_handshake(true)
            }
            Err(TlsHandshakeError::Failure(err)) => Err(TlsError::Native(Box::new(err)).into()),
            Err(TlsHandshakeError::WouldBlock(handshake)) => {
                Err(SocketHandshakeError::WouldBlock(MidHandshakeSocket {
                    poll,
                    state: MidHandshakeState::TlsConnecting { request, handshake },
                }))
            }
        }
    }
}

/// The current intermediate state of an intermediate connection, minus any data
/// that's common to the entire process.
enum MidHandshakeState {
    /// The initial [TcpStream] connection is being established.
    TcpConnecting {
        /// The request to make to establish the WebSocket connection.
        request: Request,

        /// The domain name of the WebSocket server.
        domain: String,

        /// The (not not yet initialized) socket.
        socket: TcpStream,
    },

    /// A TLS handshake is in progress. This state is skipped for non-TLS URLs.
    TlsConnecting {
        request: Request,

        /// The intermediate state of the TLS handshake.
        handshake: MidHandshakeTlsStream<TcpStream>,
    },

    /// The WebSocket handshake is in progress.
    WsConnecting {
        /// The intermediate state of the WebSocket handshake.
        handshake: MidHandshakeWebSocket<ClientHandshake<MaybeTlsStream<TcpStream>>>,
    },
}

/// Any outcome of running a [Socket] handshake that isn't a fully-initialized
/// socket.
///
/// Despite the name, this doesn't always represent an error. The [WouldBlock]
/// state indicates that the handshake is currently waiting for a remote
/// response, and should be checked later by calling
/// [MidHandshakeSocket.handshake]. The [Failure] state instead represents a
/// true error.
#[derive(ThisError)]
pub(crate) enum SocketHandshakeError {
    /// The handshake is currently waiting for a remote response, and should be
    /// checked later by calling [MidHandshakeSocket.handshake].
    #[error("the handshake process was interrupted")]
    WouldBlock(MidHandshakeSocket),

    /// Something has gone wrong with the connection.
    #[error("{0}")]
    Failure(Error),
}

impl fmt::Debug for SocketHandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            SocketHandshakeError::WouldBlock(_) => write!(f, "{}", self),
            SocketHandshakeError::Failure(err) => err.fmt(f),
        }
    }
}

impl<E> From<E> for SocketHandshakeError
where
    E: Into<Error>,
{
    fn from(value: E) -> Self {
        SocketHandshakeError::Failure(value.into())
    }
}
