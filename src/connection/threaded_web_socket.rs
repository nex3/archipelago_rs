use std::{io, net::TcpStream, thread, time::Duration};

use log::*;
use native_tls::{HandshakeError as TlsHandshakeError, TlsConnector, TlsStream};
use smol::channel;
use tungstenite::HandshakeError as WsHandshakeError;
use tungstenite::error::{TlsError, UrlError};
use tungstenite::handshake::client::{ClientHandshake, Request};
use tungstenite::stream::{MaybeTlsStream, Mode};
use tungstenite::{self, Message, WebSocket};

use crate::error::Error;

/// The default Archipelago port, used for localhost and potentially other
/// custom servers.
const DEFAULT_PORT: u16 = 38281;

/// The time for the worker thread to wait for an incoming message before
/// checking if there are any outgoing messages to send.
const READ_TIMEOUT: Duration = Duration::from_millis(50);

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
///
/// This runs in a separate thread using blocking IO and sends messages to/from
/// the main thread using channels to simulate a non-blocking API.
pub(crate) struct ThreadedWebSocket {
    /// The receiver for messages to the WebSocket connection.
    sender: channel::Sender<Message>,

    /// The receiver for messages from the WebSocket connection.
    receiver: channel::Receiver<Result<Message, Error>>,
}

impl ThreadedWebSocket {
    /// Begins the process of establishing a WebSocket connection using the
    /// given [request] (which may be passed as a simple `ws://` or `wss://`
    /// URL).
    pub(crate) async fn connect(request: Request) -> Result<Self, Error> {
        let (worker_sender, local_receiver) = channel::unbounded::<Result<Message, Error>>();
        let (local_sender, worker_receiver) = channel::unbounded::<Message>();
        let (connect_sender, connect_receiver) = oneshot::channel::<Result<(), Error>>();
        thread::Builder::new()
            .name("archipelago_rs::ThreadedWebSocketWorker".into())
            .spawn(move || {
                match ThreadedWebSocketWorker::connect(request, worker_sender, worker_receiver) {
                    Ok(mut worker) => {
                        let _ = connect_sender.send(Ok(()));
                        worker.run();
                    }
                    Err(error) => {
                        let _ = connect_sender.send(Err(error));
                    }
                };
            })?;

        match connect_receiver.await {
            Ok(Ok(())) => Ok(ThreadedWebSocket {
                sender: local_sender,
                receiver: local_receiver,
            }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(Error::ThreadKilled),
        }
    }

    /// Returns the next message from this socket, if one is available.
    ///
    /// Returns an error if this receives an unexpected message type, either at
    /// the WebSocket or Archipelago levels. Returns `Ok(None)` if there are no
    /// messages to receive. This automatically handles
    /// [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Result<Option<Message>, Error> {
        match self.receiver.try_recv() {
            Ok(Ok(message)) => Ok(Some(message)),
            Ok(Err(err)) => Err(err),
            Err(channel::TryRecvError::Empty) => Ok(None),
            Err(channel::TryRecvError::Closed) => Err(Error::ThreadKilled),
        }
    }

    /// Like [try_recv], but returns a Future that only resolves once a message
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<Message, Error> {
        match self.receiver.recv().await {
            Ok(result) => result,
            Err(_) => Err(Error::ThreadKilled),
        }
    }

    /// Sends [message] to the server.
    pub(crate) fn send(&mut self, message: Message) -> Result<(), Error> {
        self.sender
            .send_blocking(message)
            .map_err(|_| Error::ThreadKilled)
    }
}

/// The underlying implementation of [ThreadedWebSocket] which runs on a separate
/// worker thread and communicates via a channel.
struct ThreadedWebSocketWorker {
    /// The WebSocket that this wraps.
    inner: WebSocket<MaybeTlsStream<TcpStream>>,

    /// The sender for messages from the WebSocket connection.
    sender: channel::Sender<Result<Message, Error>>,

    /// The receiver for messages to the WebSocket connection.
    receiver: channel::Receiver<Message>,
}

impl ThreadedWebSocketWorker {
    /// Establishes a WebSocket connection using the given [request] (which may
    /// be passed as a simple `ws://` or `wss://` URL).
    fn connect(
        request: Request,
        sender: channel::Sender<Result<Message, Error>>,
        receiver: channel::Receiver<Message>,
    ) -> Result<Self, Error> {
        let domain = request
            .uri()
            .host()
            .map(|h| h.to_string())
            .ok_or(tungstenite::Error::Url(UrlError::NoHostName))?;
        let port = request.uri().port_u16().unwrap_or(DEFAULT_PORT);
        let stream = Self::connect_tcp(&domain, port)?;

        let maybe_tls_stream = match tungstenite::client::uri_mode(request.uri())? {
            Mode::Plain => {
                debug!("Upgrading to WebSocket...");
                MaybeTlsStream::Plain(stream)
            }
            Mode::Tls => {
                debug!("Upgrading to TLS...");
                match Self::try_tls(stream, domain.as_str()) {
                    Ok(stream) => {
                        debug!("Upgrading to WebSocket...");
                        MaybeTlsStream::NativeTls(stream)
                    }
                    Err(Error::WebSocket(tungstenite::Error::Tls(err))) => {
                        debug!(
                            "Upgrading to TLS failed ({err}), attempting plain TCP connection..."
                        );
                        let stream = Self::connect_tcp(&domain, port)?;
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

                    return Ok(ThreadedWebSocketWorker {
                        inner,
                        sender,
                        receiver,
                    });
                }
                Err(WsHandshakeError::Interrupted(new_handshake)) => {
                    handshake = new_handshake;
                }
                Err(WsHandshakeError::Failure(err)) => return Err(err.into()),
            }
        }
    }

    /// Initializes a new TCP connection to the given [domain] and [port].
    fn connect_tcp(domain: impl AsRef<str>, port: u16) -> Result<TcpStream, Error> {
        let domain = domain.as_ref();
        debug!("Establishing TCP connection to {domain}:{port}...");
        TcpStream::connect(format!("{domain}:{port}")).map_err(|err| {
            // Normalize OS errors into tungstenite's error wrapper.
            if let Some(os_err) = err.raw_os_error() {
                tungstenite::Error::Io(io::Error::from_raw_os_error(os_err)).into()
            } else {
                err.into()
            }
        })
    }

    /// Attempts to upgrade `stream` to TLS.
    fn try_tls(stream: TcpStream, domain: impl AsRef<str>) -> Result<TlsStream<TcpStream>, Error> {
        let connector = TlsConnector::new()
            .map_err(|e| tungstenite::Error::Tls(TlsError::Native(Box::new(e))))?;
        match connector.connect(domain.as_ref(), stream) {
            Ok(socket) => {
                debug!("Upgrading to WebSocket...");
                Ok(socket)
            }
            Err(TlsHandshakeError::Failure(err)) => {
                Err(tungstenite::Error::Tls(TlsError::Native(Box::new(err))).into())
            }
            Err(TlsHandshakeError::WouldBlock(_)) => unreachable!(),
        }
    }

    /// Forwards all WebSocket messages from the server to
    /// [ThreadedWebSocketWorker::sender].
    fn run(&mut self) {
        if let Err(err) = match self.inner.get_mut() {
            MaybeTlsStream::Plain(socket) => socket,
            MaybeTlsStream::NativeTls(socket) => socket.get_mut(),
            &mut _ => unreachable!(),
        }
        .set_read_timeout(Some(READ_TIMEOUT))
        {
            let _ = self.sender.send_blocking(Err(err.into()));
            return;
        }

        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    if let Err(err) = self.inner.send(message) {
                        let _ = self.sender.send_blocking(Err(err.into()));
                    }
                }
                Err(channel::TryRecvError::Empty) => {}
                Err(channel::TryRecvError::Closed) => return,
            }

            match self.inner.read() {
                Err(tungstenite::Error::ConnectionClosed) => return,
                Err(tungstenite::Error::Io(err))
                    if matches!(
                        err.kind(),
                        // WouldBlock here is caused by a timeout.
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                message => {
                    if self
                        .sender
                        .send_blocking(message.map_err(|e| e.into()))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
    }
}
