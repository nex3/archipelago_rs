use std::{collections::HashSet, sync::Arc};

use crate::{Error, Location, Player, Print, protocol::Permission};

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
}

/// An enum that indicates exactly what in a [Client] was updated.
pub enum UpdatedField {
    /// [Client.server_tags] changed.
    ///
    /// This contains the previous tags.
    ServerTags(HashSet<String>),

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
