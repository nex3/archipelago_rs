use std::{io, net::TcpStream as SyncTcpStream, sync::Arc};

use log::*;
use native_tls::{HandshakeError as TlsHandshakeError, TlsConnector, TlsStream};
use smol::{Async, net::TcpStream as AsyncTcpStream};
use tungstenite::HandshakeError as WsHandshakeError;
use tungstenite::error::{TlsError, UrlError};
use tungstenite::handshake::client::{ClientHandshake, Request};
use tungstenite::stream::{MaybeTlsStream, Mode};
use tungstenite::{Message, WebSocket};

use crate::{error::Error, util};

/// The default Archipelago port, used for localhost and potentially other
/// custom servers.
const DEFAULT_PORT: u16 = 38281;

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
///
/// This runs in the local thread using native non-blocking IO APIs.
pub(crate) struct NonBlockingWebSocket {
    /// The async wrapper for the TCP stream. We use this to determine when it's
    /// readable and writable.
    async_stream: Arc<Async<SyncTcpStream>>,

    /// The WebSocket that this wraps.
    inner: WebSocket<MaybeTlsStream<SyncTcpStream>>,
}

impl NonBlockingWebSocket {
    /// Establishes a WebSocket connection using the given [request] (which may
    /// be passed as a simple `ws://` or `wss://` URL).
    pub(crate) async fn connect(request: Request) -> Result<Self, Error> {
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

                    return Ok(NonBlockingWebSocket {
                        async_stream,
                        inner,
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

        Ok((util::clone_tcp_stream(stream)?, async_stream))
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

    /// Returns the next message from this socket, if one is available.
    ///
    /// Returns an error if this receives an unexpected message type, either at
    /// the WebSocket or Archipelago levels. Returns `Ok(None)` if there are no
    /// messages to receive. This automatically handles
    /// [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Result<Option<Message>, Error> {
        match self.inner.read() {
            Ok(message) => Ok(Some(message)),

            Err(tungstenite::Error::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {
                // Make sure we reply to any pings or close messages.
                self.inner.flush()?;
                Ok(None)
            }

            Err(err) => Err(err.into()),
        }
    }

    /// Like [try_recv], but returns a Future that only resolves once a message
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<Message, Error> {
        loop {
            match self.try_recv()? {
                Some(message) => return Ok(message),
                None => self.async_stream.readable().await?,
            }
        }
    }

    /// Sends [message] to the server.
    pub(crate) fn send(&mut self, message: Message) -> Result<(), Error> {
        self.inner.send(message)?;
        self.inner.flush()?;
        Ok(())
    }
}
