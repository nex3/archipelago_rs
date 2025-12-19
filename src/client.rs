use std::collections::{HashMap, HashSet};

use serde::de::DeserializeOwned;

use crate::{ConnectionOptions, Error, ProtocolError, Socket, protocol::*};

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
    players: Vec<NetworkPlayer>,
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
        let Some(player_index) = connected
            .players
            .iter()
            .position(|p| p.team == connected.team && p.slot == connected.slot)
        else {
            return Err(ProtocolError::MissingNetworkPlayer {
                team: connected.team,
                slot: connected.slot,
            }
            .into());
        };
        let spectators = connected
            .slot_info
            .extract_if(|_, slot| slot.r#type == SlotType::Spectator)
            .map(|(_, slot)| slot)
            .collect();
        let groups = connected
            .slot_info
            .extract_if(|_, slot| slot.r#type == SlotType::Group)
            .map(|(_, slot)| slot)
            .collect();

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
            players: connected.players,
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
    pub fn connected_player(&self) -> &NetworkPlayer {
        &self.players[self.player_index]
    }

    // TODO: Give these players game names from slot_info
    /// All players in the multiworld.
    pub fn players(&self) -> &[NetworkPlayer] {
        self.players.as_slice()
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
