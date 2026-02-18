use tungstenite::{Message, handshake::client::Request};

use super::{NonBlockingWebSocket, ThreadedWebSocket};
use crate::Error;

/// An enum of implementations of a non-blocking web socket.
pub(crate) enum WebSocket {
    NonBlocking(Box<NonBlockingWebSocket>),
    Threaded(ThreadedWebSocket),
}

impl WebSocket {
    /// Establishes a WebSocket connection using the given [request] (which may
    /// be passed as a simple `ws://` or `wss://` URL).
    pub(crate) async fn connect(request: Request, mode: SocketMode) -> Result<Self, Error> {
        Ok(match mode {
            SocketMode::Auto if is_wine::is_wine_lax() => {
                WebSocket::Threaded(ThreadedWebSocket::connect(request).await?)
            }
            SocketMode::NonBlocking | SocketMode::Auto => {
                WebSocket::NonBlocking(Box::new(NonBlockingWebSocket::connect(request).await?))
            }
            SocketMode::Threaded => WebSocket::Threaded(ThreadedWebSocket::connect(request).await?),
        })
    }

    /// Returns the next message from this socket, if one is available.
    ///
    /// Returns an error if this receives an unexpected message type at the
    /// WebSocket level. Returns `Ok(None)` if there are no messages to receive.
    /// This automatically handles [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Result<Option<Message>, Error> {
        match self {
            WebSocket::NonBlocking(inner) => inner.try_recv(),
            WebSocket::Threaded(inner) => inner.try_recv(),
        }
    }

    /// Like [try_recv], but returns a Future that only resolves once a message
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<Message, Error> {
        match self {
            WebSocket::NonBlocking(inner) => inner.recv_async().await,
            WebSocket::Threaded(inner) => inner.recv_async().await,
        }
    }

    /// Sends [message] to the server.
    pub(crate) fn send(&mut self, message: Message) -> Result<(), Error> {
        match self {
            WebSocket::NonBlocking(inner) => inner.send(message),
            WebSocket::Threaded(inner) => inner.send(message),
        }
    }
}

/// Possible ways to run the WebSocket connection.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketMode {
    /// A WebSocket connection that runs on the local thread, using non-blocking
    /// IO to avoid blocking the main thread. This is recommended for most
    /// cases, but it's known to work poorly when running under Wine/Proton.
    NonBlocking,

    /// A WebSocket connection that uses blocking IO on a background thread.
    /// This is somewhat more resource-intensive than [SocketMode::NonBlocking],
    /// but it's less sensitive to OS issues.
    Threaded,

    /// Automatically chooses which type to use based on what we can tell about
    /// the current system.
    #[default]
    Auto,
}
