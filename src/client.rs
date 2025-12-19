use std::collections::{HashMap, HashSet};
use std::iter::{FusedIterator, Iterator};
use std::{slice, sync::Arc};

use serde::de::DeserializeOwned;

use crate::{ConnectionOptions, Error, Player, ProtocolError, Socket, protocol::*};

mod death_link_options;

pub use death_link_options::*;

/// The version of the Archipelago server that this client supports.
const VERSION: NetworkVersion = NetworkVersion {
    major: 0,
    minor: 6,
    build: 0,
    class: String::new(),
};

/// The client that talks to the Archipelago server using the Archipelago
/// protocol.
///
/// The generic type [S] is used to deserialize the slot data in the initial
/// [Connected] message. By default, it will decode the slot data as a dynamic
/// JSON blob.
///
/// This isn't currently possible to construct directly. Instead, use
/// [Connection] which represents any possible state a connection can inhabit.
pub struct Client<S: DeserializeOwned = serde_json::Value> {
    socket: Socket<S>,

    // == Session information
    game: String,
    server_version: NetworkVersion,
    generator_version: NetworkVersion,
    server_tags: HashSet<String>,
    password_required: bool,
    permissions: PermissionMap,
    points_per_hint: u64,
    hint_points_per_check: u64,
    hint_points: u64,
    seed_name: String,
    data_package: DataPackageObject,
    player_index: usize,
    players: Vec<Arc<Player>>,
    slot_data: S,
    spectators: Vec<NetworkSlot>,
    groups: Vec<NetworkSlot>,

    /// A map from location IDs for this game to booleans indicating whether or
    /// not they've been checked.
    local_locations_checked: HashMap<i64, bool>,
}

impl<S: DeserializeOwned> Client<S> {
    /// Asynchronously initializes a client connection to an Archipelago server.
    pub async fn connect(
        url: String,
        game: String,
        name: String,
        options: ConnectionOptions,
    ) -> Result<Client<S>, Error> {
        let mut socket = Socket::<S>::connect(url).await?;

        log::debug!("Awaiting RoomInfo...");
        let room_info = match socket.recv_async().await? {
            ServerMessage::RoomInfo(room_info) => room_info,
            message => {
                return Err(ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "RoomInfo",
                }
                .into());
            }
        };

        // TODO: cache data packages and only ask for those that are outdated.
        log::debug!("Awaiting DataPackage...");
        socket.send(ClientMessage::GetDataPackage(GetDataPackage {
            games: None,
        }))?;

        let data_package = match socket.recv_async().await? {
            ServerMessage::DataPackage(DataPackage { data }) => data,
            message => {
                return Err(ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "DataPackage",
                }
                .into());
            }
        };

        log::debug!("Awaiting Connected...");
        let mut version = VERSION.clone();
        version.class = "Version".into();
        socket.send(ClientMessage::Connect(Connect {
            password: options.password,
            game: game.clone(),
            name,
            // Specify something useful here if
            // ArchipelagoMW/Archipelago#998 ever gets sorted out.
            uuid: "".into(),
            version: version.clone(),
            items_handling: options.items_handling.bits(),
            tags: options.tags,
            slot_data: options.slot_data,
        }))?;

        let connected = match socket.recv_async().await? {
            ServerMessage::Connected(connected) => connected,
            ServerMessage::ConnectionRefused(ConnectionRefused { errors }) => {
                return Err(Error::ConnectionRefused(
                    errors.into_iter().map(|e| e.into()).collect(),
                ));
            }
            message => {
                return Err(ProtocolError::UnexpectedResponse {
                    actual: message.type_name(),
                    expected: "Connected",
                }
                .into());
            }
        };

        let client = Client::new(socket, game, room_info, data_package, connected)?;
        log::info!("Archipelago connection initialized succesfully");
        Ok(client)
    }

    /// Creates a new client with all available initial information.
    fn new(
        socket: Socket<S>,
        game: String,
        room_info: RoomInfo,
        data_package: DataPackageObject,
        mut connected: Connected<S>,
    ) -> Result<Self, Error> {
        let total_locations = connected.checked_locations.len() + connected.missing_locations.len();
        let points_per_hint = (total_locations as u64) * u64::from(room_info.hint_cost) / 100;

        let spectators = connected
            .slot_info
            .extract_if(|_, slot| slot.r#type == SlotType::Spectator)
            .map(|(_, slot)| slot)
            .collect();
        let groups = connected
            .slot_info
            .extract_if(|_, slot| slot.r#type == SlotType::Group)
            .map(|(_, slot)| slot)
            .collect::<Vec<_>>();
        let players = connected
            .players
            .into_iter()
            .map(|p| {
                let game = connected
                    .slot_info
                    .get(&p.slot)
                    .or_else(|| groups.iter().find(|s| s.group_members.contains(&p.slot)))
                    .ok_or_else(|| ProtocolError::MissingSlotInfo(p.slot))?
                    .game
                    .clone();
                Ok(Player::hydrate(p, game).into())
            })
            .collect::<Result<Vec<Arc<Player>>, Error>>()?;
        let player_index = players
            .iter()
            .position(|p| p.team() == connected.team && p.slot() == connected.slot)
            .ok_or_else(|| ProtocolError::MissingNetworkPlayer {
                team: connected.team,
                slot: connected.slot,
            })?;

        let mut local_locations_checked = HashMap::with_capacity(total_locations);
        for id in connected.missing_locations {
            local_locations_checked.insert(id, false);
        }
        for id in connected.checked_locations {
            local_locations_checked.insert(id, true);
        }

        Ok(Client {
            socket,
            game,
            server_version: room_info.version,
            generator_version: room_info.generator_version,
            server_tags: room_info.tags,
            password_required: room_info.password_required,
            permissions: room_info.permissions,
            points_per_hint,
            hint_points_per_check: room_info.location_check_points,
            hint_points: connected.hint_points,
            seed_name: room_info.seed_name,
            data_package: data_package,
            player_index,
            players,
            slot_data: connected.slot_data,
            spectators,
            groups,
            local_locations_checked,
        })
    }

    // == Session information

    /// The name of the game that's currently being played.
    pub fn game(&self) -> &str {
        self.game.as_str()
    }

    /// The version of Archipelago which the server is running.
    pub fn server_version(&self) -> &NetworkVersion {
        &self.server_version
    }

    /// The version of Archipelago that generated the multiworld.
    pub fn generator_version(&self) -> &NetworkVersion {
        &self.generator_version
    }

    /// The server's special features or capabilities.
    pub fn server_tags(&self) -> &HashSet<String> {
        &self.server_tags
    }

    /// Whether this Archipelago multiworld requires a password to join.
    pub fn password_required(&self) -> bool {
        self.password_required
    }

    /// The permissions for distributing all items after a player reaches their
    /// goal to other players awaiting them.
    pub fn release_permission(&self) -> Permission {
        self.permissions.release
    }

    /// The permissions for collecting all items after a player reaches their
    /// goal.
    pub fn collect_permission(&self) -> Permission {
        self.permissions.collect
    }

    /// The permissions for a player querying the items remaining in their run.
    pub fn remaining_permission(&self) -> Permission {
        self.permissions.remaining
    }

    /// The number of hint points the player must accumulate in order to access
    /// a single hint.
    pub fn points_per_hint(&self) -> u64 {
        self.points_per_hint
    }

    /// The number of hint points granted for each location a player checks.
    pub fn hint_points_per_check(&self) -> u64 {
        self.hint_points_per_check
    }

    // TODO: Update this as the player checks new locations.
    /// The number of hint points the player currently has.
    pub fn hint_points(&self) -> u64 {
        self.hint_points
    }

    /// The uniquely-identifying name of the generated multiworld.
    ///
    /// If the same multiworld is hosted in multiple rooms, this will be the
    /// same across those rooms.
    pub fn seed_name(&self) -> &str {
        self.seed_name.as_str()
    }

    // TODO: Convert [GameData] into something more usable/intuitive.
    /// A map from the names of each game in this multiworld to metadata about
    /// those games.
    pub fn games(&self) -> &HashMap<String, GameData> {
        &self.data_package.games
    }

    /// The player that's currently connected to the multiworld.
    pub fn this_player(&self) -> &Player {
        self.players[self.player_index].as_ref()
    }

    /// All players in the multiworld.
    pub fn players(&self) -> Players<'_> {
        Players(self.players.as_slice().into_iter())
    }

    /// The player on the given [team] playing the given [slot], if one exists.
    ///
    /// See also [teammate] to only check the current player's team.
    pub fn player(&self, team: u64, slot: u64) -> Option<&Player> {
        self.players
            .iter()
            .find(|p| p.team() == team && p.slot() == slot)
            .map(|p| p.as_ref())
    }

    /// The player on the given [team] playing the given [slot]. Panics if this
    /// player doesn't exist.
    ///
    /// See also [assert_teammate] to only check the current player's team.
    pub fn assert_player(&self, team: u64, slot: u64) -> &Player {
        self.player(team, slot).unwrap_or_else(|| {
            if self.players.iter().any(|p| p.team() == team) {
                panic!("no player with slot {}", slot);
            } else {
                panic!("no team with ID {}", team);
            }
        })
    }

    /// The player playing the given [slot] on the current player's team, if one
    /// exists.
    pub fn teammate(&self, slot: u64) -> Option<&Player> {
        self.player(self.this_player().team(), slot)
    }

    /// The player playing the given [slot] on the current player's team. Panics
    /// if this player doesn't exist.
    ///
    /// See also [assert_teammate] to only check the current player's team.
    pub fn assert_teammate(&self, team: u64, slot: u64) -> &Player {
        self.assert_player(self.this_player().team(), slot)
    }

    // TODO: Take a Location object?
    /// Returns whether the local location with the given ID has been checked
    /// (either by us or by other players doing co-op).
    ///
    /// This may only be called for locations in the connected game's world.
    /// Panics if it's called with a location ID that doesn't exist for this
    /// world.
    pub fn is_local_location_checked(&self, id: i64) -> bool {
        *self.local_locations_checked.get(&id).unwrap_or_else(|| {
            panic!(
                "Archipelago location ID {} doesn't exist for {}",
                id, self.game
            )
        })
    }

    /// Returns the slot data provided by the apworld.
    pub fn slot_data(&self) -> &S {
        &self.slot_data
    }

    // TODO: Make this return custom structs.
    /// Information about clients that are observing the multiworld without
    /// participating.
    ///
    /// Note: at time of writing, spectator support is not actually implemented
    /// by the Archipelago server.
    pub fn spectators(&self) -> &[NetworkSlot] {
        self.spectators.as_slice()
    }

    // TODO: Make this return custom structs that contain player references.
    /// Information about groups of players.
    pub fn groups(&self) -> &[NetworkSlot] {
        self.groups.as_slice()
    }

    // == Event handling

    /// Returns any pending [Event]s from the Archipelago server and updates the
    /// rest of the client's state to accommodate new information.
    ///
    /// This call never blocks, and is expected to be called repeatedly in order
    /// to check for new messages from the underlying connection to Archipelago.
    /// Typically a caller that's integrated Archipelago into a game loop will
    /// call this once each frame.
    ///
    /// Unless this is called (or [Connection.update], which calls this), the
    /// client will never change state.
    pub fn update(&mut self) -> Vec<Event> {
        let mut events = Vec::<Event>::new();
        loop {
            match self.socket.try_recv() {
                Ok(Some(_event)) => {
                    // TODO: dispatch events
                }
                Ok(None) => return events,
                Err(err) => {
                    events.push(Event::Error(err));
                    return events;
                }
            }
        }
    }
}

// The only reason Client doesn't automatically implement [Unpin] is that S
// might not implement it (although being decoded from JSON it probably does).
// Since we treat slot data as immutable anyway, we can guarantee that nothing
// will change and so it's safe to declare the entire Client as Unpin.
impl<S> Unpin for Client<S> where S: DeserializeOwned {}

/// The iterator for [Client.players].
#[derive(Clone, Debug, Default)]
pub struct Players<'a>(slice::Iter<'a, Arc<Player>>);

impl<'a> Iterator for Players<'a> {
    type Item = &'a Player;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|a| a.as_ref())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Players<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|a| a.as_ref())
    }
}

impl ExactSizeIterator for Players<'_> {}
impl FusedIterator for Players<'_> {}

/// Events from the Archipelago server that clients may want to handle.
///
/// This only encompasses events that can be spontaneously sent by the server.
/// Events that are only ever sent as replies to client requests are represented
/// as [Future]s instead.
pub enum Event {
    /// The client has established a successful connection. This is only emitted
    /// from [Connection.update] and only once, always as the first event.
    Connected,

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
