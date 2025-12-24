use std::{collections::HashSet, sync::Arc, time::SystemTime};

use ustr::UstrSet;

use crate::{Error, LocatedItem, Location, Player, Print, protocol::Permission};

/// Events from the Archipelago server that clients may want to handle.
///
/// This only encompasses events that can be spontaneously sent by the server.
/// Events that are only ever sent as replies to client requests are represented
/// as [Future]s instead.
pub enum Event {
    /// The client has established a successful connection. This is only emitted
    /// from [Connection.update] and only once, always as the first event.
    Connected,

    /// An event indicating that some information about the room or the current
    /// connection was updated by the server. It contains all the updates that
    /// ocurred.
    Updated(Vec<UpdatedField>),

    /// A message for the client to display to the player.
    Print(Print),

    /// Items have been received from the server (usually another world,
    /// although the specifics of which items will be sent depends on the
    /// [crate::ItemHandling] you pass in [crate::ConnectionOptions]).
    ///
    /// This is typically the first event this will be emitted (after
    /// [Connected], if applicable), and will contain all items the player has
    /// ever received.
    ReceivedItems {
        /// The total number of items the connected player has ever been sent.
        /// See [Synchronizing Items] for details on how to use this to keep the
        /// player's items in sync with the server.
        ///
        /// [Synchronizing Items]: https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md#synchronizing-items
        index: u64,

        /// The items the player has received. These are guaranteed to always be
        /// items for the current player.
        items: Vec<LocatedItem>,
    },

    /// The client has encountered an error.
    ///
    /// Once this event has been emitted, the client should be considered
    /// closed. It will emit no more events and any attempts to send requests
    /// will fail.
    ///
    /// When emitted from [Connection.update], this will be [Error::Elsewhere]
    /// and the actual error will be available from [Connection.state] or
    /// [Connection.into_err].
    Error(Error),

    /// An event sent by other clients in the multiworld. The specific meaning
    /// of this is determined by those clients.
    Bounce {
        /// The set of games this is targeting. If this is `None`, it's not
        /// limited to specific games.
        games: Option<UstrSet>,

        /// The set of all slots this is targeting. If this is `None`, it's not
        /// limited to specific slots.
        slots: Option<HashSet<u32>>,

        /// The set of all tags this is targeting. If this is `None`, it's not
        /// limited to specific tags.
        tags: Option<UstrSet>,

        /// Data attached to the event, if any.
        data: Option<serde_json::Value>,
    },

    /// A death link event, indicating that participating clients should kill
    /// the player because another player died. This is only received if
    /// `"DeathLink"` is passed to [ConnectionOptions.tags].
    DeathLink {
        /// The set of games this is targeting. If this is `None`, it's not
        /// limited to specific games.
        games: Option<UstrSet>,

        /// The set of all slots this is targeting. If this is `None`, it's not
        /// limited to specific slots.
        slots: Option<HashSet<u32>>,

        /// The set of all tags this is targeting. This will always contain at
        /// least `"DeathLink"`.
        tags: UstrSet,

        /// The time the death link was sent, according to the sender. There's
        /// no guarantee that this has any particular relationship to the
        /// current system's time.
        time: SystemTime,

        /// Text to explain the cause of death. This is expected to contain the
        /// name of the player who died.
        cause: Option<String>,

        /// The name of the player who first died. This can be a slot name or a
        /// name from within a multiplayer game.
        source: String,
    },

    /// The value associated with a key in the server's data storage was
    /// updated. This is only emitted after [Client.watch] is called, or if
    /// [Client.set] or [Client.change] is called with `emit_event` set to
    /// `true`.
    KeyChanged {
        /// The name of the key whose value changed.
        key: String,

        /// The value before the change. This is `None` for special server keys.
        old_value: Option<serde_json::Value>,

        /// The value after the change.
        new_value: serde_json::Value,

        /// The player who updated this key.
        player: Arc<Player>,
    },
}

/// An enum that indicates exactly what in a [Client] was updated.
pub enum UpdatedField {
    /// [Client.server_tags] changed.
    ///
    /// This contains the previous tags.
    ServerTags(UstrSet),

    /// [Client.release_permission], [Client.collect_permission], and/or
    /// [Client.remaining_permission] changed.
    ///
    /// This contains the old values for each permission.
    Permissions {
        release: Permission,
        collect: Permission,
        remaining: Permission,
    },

    /// [Client.points_per_hint] and/or [Client.hint_points_per_check] changed.
    ///
    /// This contains the previous values for each field.
    HintEconomy {
        points_per_hint: u64,
        hint_points_per_check: u64,
    },

    /// [Client.hint_points] has changed.
    ///
    /// This contains the previous value for the field.
    HintPoints(u64),

    /// One or more players' aliases have changed.
    ///
    /// This includes the *previous* [Player] structs. Use
    /// [Client.assert_player] to access the new ones.
    Players(Vec<Arc<Player>>),

    /// Additional locations have been checked, usually from a co-op player in
    /// the same slot.
    ///
    /// This includes all newly-checked locations.
    CheckedLocations(Vec<Location>),
}
