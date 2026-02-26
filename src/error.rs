use thiserror::Error as ThisError;
use ustr::{Ustr, UstrSet};

use crate::LocatedItem;

/// The enumeration of all possible errors that can occur in an Archipelago
/// connection.
#[derive(ThisError, Debug)]
pub enum Error {
    /// An error occurred with the underlying WebSocket connection. If the inner
    /// error is [tungstenite::Error::ConnectionClosed], that means that the
    /// connection closed normally.
    #[error("{0}")]
    WebSocket(#[from] tungstenite::Error),

    /// An error occurred with the underlying asynchrony library.
    #[error("{0}")]
    Async(#[from] smol::io::Error),

    /// The Archipelago server rejected the connection.
    #[error("Archipelago refused connection: {}", .0.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", "))]
    ConnectionRefused(Vec<ConnectionError>),

    /// A panic occurred during the connection process.
    #[error("Rust panic during connection process")]
    ConnectionInterrupted,

    /// The caller violated a contract when calling a [Client](crate::Client)
    /// method.
    #[error("{0}")]
    ArgumentError(#[from] ArgumentError),

    /// The Archipelago client provided a message that couldn't be serialized.
    #[error("failed to serialize client message: {0}")]
    Serialize(serde_json::Error),

    /// The Archipelago client sent a package that the server considers invalid.
    #[error("client sent invalid packet: {0}")]
    InvalidPacket(String),

    /// The Archipelago server violated the network protocol (as the client
    /// understands it).
    #[error("Archipelago server violated the expected protocol: {0}")]
    ProtocolError(#[from] ProtocolError),

    /// The client has manually disconnected. This is used when
    /// [Connection::into_err](crate::Connection::into_err) is called when there
    /// was no error, and it's also used as the error value of
    /// [Connection::default](crate::Connection::default).
    #[error("the client ended the connection")]
    ClientDisconnected,

    /// A placeholder used when the full error is available elsewhere. Used when
    /// a future is canceled because the underlying connection failed or in the
    /// events returned by [Connection::update](crate::Connection::update)
    /// because the actual error is stored in
    /// [Connection::state](crate::Connection::state).
    #[error("a full error is available elsewhere")]
    Elsewhere,
}

impl Error {
    /// Returns whether this is a fatal error that indicates that the
    /// Archipelago connection is closed after it's emitted.
    pub fn is_fatal(&self) -> bool {
        !matches!(self, Error::ProtocolError(_) | Error::InvalidPacket(_))
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

/// Errors caused by the user invoking the client incorrectly.
#[derive(ThisError, Debug)]
pub enum ArgumentError {
    /// The game parameter to [Client::connect] or [Connection::new] was `None`
    /// but [ConnectionOptions::tags] didn't contain a tag that would allow
    /// this.
    ///
    /// [Client::connect]: crate::Client::connect
    /// [Connection::new]: crate::Connection::new
    /// [ConnectionOptions::tags]: crate::ConnectionOptions::tags
    #[error(
        "game was None but tags {tags:?} didn't contain \"HintGame\", \"Tracker\", or \"TextOnly\""
    )]
    MissingGame {
        /// The tags that were passed to [ConnectionOptions::tags].
        ///
        /// [ConnectionOptions::tags]: crate::ConnectionOptions::tags
        tags: UstrSet,
    },

    /// The given location ID doesn't correspond to a location in the given
    /// game.
    #[error("{game} doesn't have a location with ID {location}")]
    InvalidLocation {
        /// The non-existent location ID.
        location: i64,

        /// The name of the game in which the location should appear.
        game: Ustr,
    },

    /// The given slot number isn't an actual slot in this multiworld.
    #[error("this multiworld doesn't have a slot {0}")]
    InvalidSlot(u32),
}

/// Errors caused by the Archipelago doing something that violates (our
/// understanding of) the network protocol.
#[derive(ThisError, Debug)]
pub enum ProtocolError {
    /// The server sent a message that couldn't be deserialized.
    ///
    /// This could either mean that the message was syntactically invalid,
    /// or (more likely) that it doesn't match the JSON structure the client
    /// expects.
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

    /// The `Connected` message included an empty players array.
    #[error("Connected message includes no players")]
    EmptyPlayers,

    /// The team and slot numbers for a player don't match anything in the
    /// players list.
    #[error("missing player on slot {slot}, team {team}")]
    MissingPlayer {
        /// The current player's team number.
        team: u32,

        /// The current player's slot number.
        slot: u32,
    },

    /// A player has a slot number that doesn't appear in `Connected.slot_info`.
    #[error("slot {0} is missing from Connected.slot_info")]
    MissingSlotInfo(u32),

    /// The data package for the current game wasn't provided by the server.
    #[error("no data package provided for {0}")]
    MissingGameData(Ustr),

    /// An item has an ID that doesn't appear in its data package.
    #[error("item {id} is missing {game}'s data package")]
    MissingItem {
        /// The ID of the item.
        id: i64,

        /// The name of the game that was expected to have this item ID.
        game: Ustr,
    },

    /// A location has an ID that doesn't appear in its data package.
    #[error("location {id} is missing {game}'s data package")]
    MissingLocation {
        /// The ID of the location.
        id: i64,

        /// The name of the game that was expected to have this location ID.
        game: Ustr,
    },

    /// The server sent us an item whose player ID doesn't match the current
    /// player.
    #[error("server sent {0:?} to this player")]
    ReceivedForeignItem(LocatedItem),

    /// The server sent a response that we didn't request.
    #[error("server sent {0} response that we didn't request")]
    ResponseWithoutRequest(&'static str),
}
