#[cfg(unix)]
use std::os::unix::io::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsSocket;
use std::{collections::VecDeque, io, net::TcpStream as SyncTcpStream, sync::Arc};

use log::*;
use native_tls::{HandshakeError as TlsHandshakeError, TlsConnector};
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

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
pub(crate) struct Socket<S: DeserializeOwned> {
    /// The async wrapper for the TCP stream. We use this to determine when it's
    /// readable and writable.
    async_stream: Arc<Async<SyncTcpStream>>,

    /// The WebSocket that this wraps.
    inner: WebSocket<MaybeTlsStream<SyncTcpStream>>,

    /// The buffer of messages that have yet to be returned, in cases where the
    /// server sends multiple messages at a time.
    messages: VecDeque<ServerMessage<S>>,
}

impl<S: DeserializeOwned> Socket<S> {
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
        let port = request
            .uri()
            .port_u16()
            .or_else(|| match request.uri().scheme_str() {
                Some("wss") => Some(443),
                Some("ws") => Some(80),
                _ => None,
            })
            .ok_or(tungstenite::Error::Url(UrlError::UnsupportedUrlScheme))?;

        debug!("Establishing TCP connection to {domain}:{port}...");
        let stream = AsyncTcpStream::connect(format!("{domain}:{port}")).await?;
        let async_stream = Arc::<Async<SyncTcpStream>>::from(stream.clone());

        #[cfg(unix)]
        let stream = SyncTcpStream::from(stream.as_fd().try_clone_to_owned()?);
        #[cfg(windows)]
        let stream = SyncTcpStream::from(stream.as_socket().try_clone_to_owned()?);

        async_stream.writable().await?;
        let maybe_tls_stream = match tungstenite::client::uri_mode(request.uri())? {
            Mode::Plain => {
                debug!("Upgrading to WebSocket...");
                MaybeTlsStream::Plain(stream)
            }
            Mode::Tls => {
                debug!("Upgrading to TLS...");
                let connector = TlsConnector::new()
                    .map_err(|e| tungstenite::Error::Tls(TlsError::Native(Box::new(e))))?;
                let mut handshake = connector.connect(domain.as_str(), stream);
                loop {
                    match handshake {
                        Ok(socket) => {
                            debug!("Upgrading to WebSocket...");
                            break MaybeTlsStream::NativeTls(socket);
                        }
                        Err(TlsHandshakeError::Failure(err)) => {
                            return Err(
                                tungstenite::Error::Tls(TlsError::Native(Box::new(err))).into()
                            );
                        }
                        Err(TlsHandshakeError::WouldBlock(new_handshake)) => {
                            async_stream.readable().await?;
                            handshake = new_handshake.handshake();
                        }
                    }
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
                    return Err(ProtocolError::BinaryMessage(bytes.to_vec()).into());
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

    /// Like [try_recv], but returns a Future that only resolves once a message
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<ServerMessage<S>, Error> {
        loop {
            match self.try_recv()? {
                Some(message) => return Ok(message),
                None => self.async_stream.readable().await?,
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
