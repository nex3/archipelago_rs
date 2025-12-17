use thiserror::Error as ThisError;

use crate::protocol::ClientMessage;

/// The enumeration of all possible errors that can occur in an Archipelago
/// connection.
#[derive(ThisError, Debug)]
pub enum Error {
    /// An error indicating that the provided URL doesn't have a protocol
    /// (`ws://` or `wss://`).
    #[error("URL \"{0}\" is missing ws:// or wss://")]
    NoProtocolError(String),

    /// An error occurred with the underlying WebSocket connection. If the inner
    /// error is [tungstenite::Error::ConnectionClosed], that means that the
    /// connection closed normally.
    #[error("{0}")]
    WebSocket(tungstenite::Error),

    /// The Archipelago server rejected the connection.
    #[error("Archipelago refused connection: {}", .0.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>().join(", "))]
    ConnectionRefused(Vec<ConnectionError>),

    /// A panic occurred during the connection process.
    #[error("Rust panic during connection process")]
    ConnectionInterrupted,

    /// The Archipelago client provided a message that couldn't be serialized.
    #[error("failed to serialize client message: {error}\n{message:?}")]
    Serialize {
        /// The unencoded message that failed to serialize.
        message: ClientMessage,

        /// The serialization error.
        error: serde_json::Error,
    },

    /// The Archipelago server violated the network protocol (as the client
    /// understands it).
    #[error("Archipelago server violated the expected protocol: {0}")]
    ProtocolError(#[from] ProtocolError),

    /// [Connection::into_err] was called when there was no error.
    #[error("Connection::into_err called before client disconnected")]
    NoError,
}

impl<E> From<E> for Error
where
    E: Into<tungstenite::Error>,
{
    fn from(value: E) -> Self {
        Error::WebSocket(value.into())
    }
}

/// Possible individual errors that can cause an initial Archipelago connection
/// to fail.
#[derive(ThisError, Debug)]
pub enum ConnectionError {
    /// The name provided doesn't match any names on the server.
    #[error("the name provided doesn't match any names on the server")]
    InvalidSlot,

    /// A correctly named slot was found, but the game for it is mismatched.
    #[error("this player isn't playing the expected game")]
    InvalidGame,

    /// This client isn't compatible with the server version.
    #[error(
        "archipelago-rs {} isn't compatible with this Archipelago server",
        env!("CARGO_PKG_VERSION")
    )]
    InvalidVersion,

    /// The password is wrong or was not provided when required.
    #[error("invalid or missing password")]
    InvalidPassword,

    /// Incorrect value type or combination of flags sent for ItemsHandling.
    #[error("invalid ItemsHandling flag")]
    InvalidItemsHandling,

    /// A connection error that's not documented in the Archipelago protocol at
    /// time of writing.
    #[error("{0}")]
    Unknown(String),
}

impl From<String> for ConnectionError {
    fn from(value: String) -> Self {
        use ConnectionError::*;
        match value.as_str() {
            "InvalidSlot" => InvalidSlot,
            "InvalidGame" => InvalidGame,
            "InvalidVersion" => InvalidVersion,
            "InvalidPassword" => InvalidPassword,
            "InvalidItemsHandling" => InvalidItemsHandling,
            _ => Unknown(value),
        }
    }
}

/// Errors caused by the Archipelago doing something that violates (our
/// understanding of) the network protocol.
#[derive(ThisError, Debug)]
pub enum ProtocolError {
    /// The server sent a message that couldn't be deserialized.
    ///
    /// This could either mean that that the message was syntactically invalid,
    /// or (more likely) that it doesn't match the JSON structure the client
    /// expectes.
    #[error("failed to deserialize server message: {error}\n{json}")]
    Deserialize {
        /// The JSON-encoded value of the message we received.
        json: String,

        /// The deserialization error.
        error: serde_json::Error,
    },

    /// The server sent a binary WebSocket message.
    ///
    /// The Archipelago protocol only supports text messages.
    #[error("unexpected binary message")]
    BinaryMessage(Vec<u8>),

    /// The client was expecting a specific response at a specific time and the
    /// server sent something else that was otherwise a valid Archipelago
    /// message.
    #[error("unexpected response {actual}, expected {expected}")]
    UnexpectedResponse {
        /// The ID of the response that was actually received.
        actual: &'static str,

        /// The ID of the response we expected to receive.
        expected: &'static str,
    },

    /// The team and slot numbers for the current player don't match anything in
    /// the players list.
    #[error("Connected packet was missing player on slot {slot}, team {team}")]
    MissingNetworkPlayer {
        /// The current player's team number.
        team: u64,

        /// The current player's slot number.
        slot: u64,
    },
}
