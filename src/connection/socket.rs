use std::collections::VecDeque;

use log::*;
use serde::de::DeserializeOwned;
use tungstenite::Message;
use tungstenite::client::IntoClientRequest;

use super::{SocketMode, WebSocket};
use crate::error::{Error, ProtocolError};
use crate::protocol::{ClientMessage, ServerMessage};

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
pub(crate) struct Socket<S: DeserializeOwned> {
    /// The non-blocking WebSocket that this wraps.
    inner: WebSocket,

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
    pub(crate) async fn connect(
        request: impl IntoClientRequest,
        mode: SocketMode,
    ) -> Result<Self, Error> {
        Ok(Self {
            inner: WebSocket::connect(request.into_client_request()?, mode).await?,
            messages: Default::default(),
        })
    }

    /// Returns the next message from this socket, if one is available.
    ///
    /// Returns an error if this receives an unexpected message type, either at
    /// the WebSocket or Archipelago levels. Returns `Ok(None)` if there are no
    /// messages to receive. This automatically handles
    /// [io::ErrorKind::WouldBlock].
    pub(crate) fn try_recv(&mut self) -> Result<Option<ServerMessage<S>>, Error> {
        loop {
            if let Some(message) = self.messages.pop_front() {
                return Ok(Some(message));
            }

            let Some(message) = self.inner.try_recv()? else {
                return Ok(None);
            };

            if let Some(result) = self.handle_message(message) {
                return result.map(Some);
            }
        }
    }

    /// Like [try_recv], but returns a Future that only resolves once a message
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<ServerMessage<S>, Error> {
        loop {
            if let Some(message) = self.messages.pop_front() {
                return Ok(message);
            }

            let message = self.inner.recv_async().await?;
            if let Some(result) = self.handle_message(message) {
                return result;
            }
        }
    }

    /// Handles a single WebSocket message.
    ///
    /// Returns the `Result` to forward to the caller if there is one, or `None`
    /// if this message had no meaningful content and the caller should look for
    /// another one.
    fn handle_message(&mut self, message: Message) -> Option<Result<ServerMessage<S>, Error>> {
        match message {
            Message::Text(bytes) => {
                debug!("--> {bytes}");
                debug_assert!(self.messages.is_empty());
                match serde_json::from_str(&bytes).map_err(|error| ProtocolError::Deserialize {
                    json: bytes.to_string(),
                    error,
                }) {
                    Ok(message) => self.messages = message,
                    Err(err) => return Some(Err(err.into())),
                }

                self.messages.pop_front().map(Ok)
            }

            Message::Binary(bytes) => {
                Some(Err(ProtocolError::BinaryMessage(bytes.to_vec()).into()))
            }

            // Other message types like pings, frames, and closes are handled
            // internally by the tungstenite. We can ignore them.
            _ => None,
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
            }))
    }
}
