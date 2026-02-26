#[cfg(unix)]
use std::os::unix::io::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsSocket;
use std::{collections::VecDeque, io, mem, net::TcpStream as SyncTcpStream, sync::Arc};

use log::*;
use native_tls::{HandshakeError as TlsHandshakeError, TlsConnector, TlsStream};
use serde::de::DeserializeOwned;
use smol::{Async, net::TcpStream as AsyncTcpStream};
use tungstenite::HandshakeError as WsHandshakeError;
use tungstenite::client::IntoClientRequest;
use tungstenite::error::{TlsError, UrlError};
use tungstenite::handshake::client::ClientHandshake;
use tungstenite::stream::{MaybeTlsStream, Mode};
use tungstenite::{Message, WebSocket};

use crate::error::{Error, ProtocolError};
use crate::protocol::{ClientMessage, ServerMessage};

/// The default Archipelago port, used for localhost and potentially other
/// custom servers.
const DEFAULT_PORT: u16 = 38281;

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
pub(crate) struct Socket<S: DeserializeOwned + 'static> {
    /// The async wrapper for the TCP stream. We use this to determine when it's
    /// readable and writable.
    async_stream: Arc<Async<SyncTcpStream>>,

    /// The WebSocket that this wraps.
    inner: WebSocket<MaybeTlsStream<SyncTcpStream>>,

    /// The buffer of messages that have yet to be returned, in cases where the
    /// server sends multiple messages at a time.
    messages: VecDeque<Result<ServerMessage<S>, Error>>,
}

impl<S: DeserializeOwned + 'static> Socket<S> {
    /// Begins the process of establishing a WebSocket connection using the
    /// given [request] (which may be passed as a simple `ws://` or `wss://`
    /// URL).
    ///
    /// This returns a [MidHandshakeSocket] for which
    /// [MidHandshakeSocket.handshake] must be called until it returns a
    /// [Socket].
    pub(crate) async fn connect(request: impl IntoClientRequest) -> Result<Self, Error> {
        let request = request.into_client_request()?;
        let domain = request
            .uri()
            .host()
            .map(|h| h.to_string())
            .ok_or(tungstenite::Error::Url(UrlError::NoHostName))?;
        let port = request.uri().port_u16().unwrap_or(DEFAULT_PORT);
        let (stream, mut async_stream) = Self::connect_tcp(&domain, port).await?;

        async_stream.writable().await?;
        let maybe_tls_stream = match tungstenite::client::uri_mode(request.uri())? {
            Mode::Plain => {
                debug!("Upgrading to WebSocket...");
                MaybeTlsStream::Plain(stream)
            }
            Mode::Tls => {
                debug!("Upgrading to TLS...");
                match Self::try_tls(async_stream.as_ref(), stream, domain.as_str()).await {
                    Ok(stream) => {
                        debug!("Upgrading to WebSocket...");
                        MaybeTlsStream::NativeTls(stream)
                    }
                    Err(Error::WebSocket(tungstenite::Error::Tls(err))) => {
                        debug!(
                            "Upgrading to TLS failed ({err}), attempting plain TCP connection..."
                        );
                        let (stream, async_stream_) = Self::connect_tcp(&domain, port).await?;
                        async_stream = async_stream_;
                        debug!("Upgrading to WebSocket...");
                        MaybeTlsStream::Plain(stream)
                    }
                    Err(err) => return Err(err),
                }
            }
        };

        let mut handshake = ClientHandshake::start(maybe_tls_stream, request, None)?;
        loop {
            match handshake.handshake() {
                Ok((inner, response)) => {
                    debug!(
                        "WebSocket response: {}\n{:?}",
                        response.status(),
                        response.headers()
                    );

                    return Ok(Socket {
                        async_stream,
                        inner,
                        messages: Default::default(),
                    });
                }
                Err(WsHandshakeError::Interrupted(new_handshake)) => {
                    handshake = new_handshake;
                    async_stream.readable().await?;
                }
                Err(WsHandshakeError::Failure(err)) => return Err(err.into()),
            }
        }
    }

    /// Initializes a new TCP connection to the given [domain] and [port].
    ///
    /// Returns both the [TcpStream] to use to communicate with the server and
    /// an [Arc<Async<SyncTcpStream>>] which can be used to wait until the
    /// [TcpStream] is writable.
    async fn connect_tcp(
        domain: impl AsRef<str>,
        port: u16,
    ) -> Result<(SyncTcpStream, Arc<Async<SyncTcpStream>>), Error> {
        let domain = domain.as_ref();
        debug!("Establishing TCP connection to {domain}:{port}...");
        let stream = match AsyncTcpStream::connect(format!("{domain}:{port}")).await {
            Ok(stream) => stream,
            Err(err) => {
                // Normalize OS errors into tungstenite's error wrapper.
                return Err(if let Some(os_err) = err.raw_os_error() {
                    tungstenite::Error::Io(io::Error::from_raw_os_error(os_err)).into()
                } else {
                    err.into()
                });
            }
        };
        let async_stream = Arc::<Async<SyncTcpStream>>::from(stream.clone());

        #[cfg(unix)]
        let stream = SyncTcpStream::from(stream.as_fd().try_clone_to_owned()?);
        #[cfg(windows)]
        let stream = SyncTcpStream::from(stream.as_socket().try_clone_to_owned()?);

        Ok((stream, async_stream))
    }

    /// Attempts to upgrade `stream` to TLS.
    async fn try_tls(
        async_stream: &Async<SyncTcpStream>,
        stream: SyncTcpStream,
        domain: impl AsRef<str>,
    ) -> Result<TlsStream<SyncTcpStream>, Error> {
        let connector = TlsConnector::new()
            .map_err(|e| tungstenite::Error::Tls(TlsError::Native(Box::new(e))))?;
        let mut handshake = connector.connect(domain.as_ref(), stream);
        loop {
            match handshake {
                Ok(socket) => {
                    debug!("Upgrading to WebSocket...");
                    return Ok(socket);
                }
                Err(TlsHandshakeError::Failure(err)) => {
                    return Err(tungstenite::Error::Tls(TlsError::Native(Box::new(err))).into());
                }
                Err(TlsHandshakeError::WouldBlock(new_handshake)) => {
                    async_stream.readable().await?;
                    handshake = new_handshake.handshake();
                }
            }
        }
    }

    /// Returns the list of all messages and errors queued in this socket.
    /// Returns an empty list if the socket has received no data since the last
    /// time this was called.
    ///
    /// The list will contain any errors encountered while processing the
    /// incoming data, including errors for unknown messages either at the
    /// Archipelago or WebSocket levels. There's no guarantee that it will
    /// contain only one error. This automatically handles
    /// [io::ErrorKind::WouldBlock].
    pub(crate) fn recv_all(
        &mut self,
    ) -> impl IntoIterator<Item = Result<ServerMessage<S>, Error>> + use<S> {
        self.read_inner();
        mem::take(&mut self.messages)
    }

    /// Returns the next result from this socket, if one is available. Returns
    /// `None` if the socket has received no data since the last time this was
    /// called.
    ///
    /// This returns errors interleaved with messages in the order they were
    /// encountered. It automatically handles [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Option<Result<ServerMessage<S>, Error>> {
        self.read_inner();
        self.messages.pop_front()
    }

    /// Like [try_recv], but returns a Future that only resolves once a result
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<ServerMessage<S>, Error> {
        loop {
            match self.try_recv() {
                Some(result) => return result,
                None => self.async_stream.readable().await?,
            }
        }
    }

    /// Processes any queued messages in [Self::inner] and adds them to
    /// [Self::messages].
    ///
    /// This can't fail because any errors it encounters will be put in
    /// [Self::messages] rather than returned directly.
    fn read_inner(&mut self) {
        // Always check the socket even if we already have messages in
        // [Socket::messages] to ensure that we don't starve it and prevent it
        // from responding to heartbeat pings.
        while self.inner.can_read() {
            match self.inner.read() {
                Ok(Message::Text(bytes)) => {
                    debug!("--> {bytes}");
                    match serde_json::from_str::<Vec<ServerMessage<S>>>(&bytes) {
                        Ok(messages) => self.messages.extend(messages.into_iter().map(Ok)),
                        Err(err) => self.messages.push_back(Err(ProtocolError::Deserialize {
                            json: bytes.to_string(),
                            error: err,
                        }
                        .into())),
                    }
                }

                Ok(Message::Binary(bytes)) => {
                    self.messages
                        .push_back(Err(ProtocolError::BinaryMessage(bytes.to_vec()).into()));
                }

                // Other message types like pings, frames, and closes are
                // handled internally by the tungstenite. We can ignore them.
                Ok(_) => {}

                Err(tungstenite::Error::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {
                    break;
                }

                Err(err) => {
                    let err = Error::from(err);
                    let fatal = err.is_fatal();
                    self.messages.push_back(Err(err));
                    if fatal {
                        break;
                    }
                }
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
                Err(error) => return Err(Error::Serialize(error)),
            }))?;
        self.inner.flush()?;
        Ok(())
    }
}
