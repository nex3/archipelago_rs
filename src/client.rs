use std::collections::{HashMap, HashSet, VecDeque};
use std::{mem, ptr, sync::Arc};

use serde::de::DeserializeOwned;
use ustr::{Ustr, UstrMap};

use crate::{
    protocol::*, ArgumentError, AsLocationId, ConnectionOptions, Error, Event, Game, Group,
    ItemHandling, Iter, LocatedItem, Location, Player, Print, ProtocolError, Socket, UnsizedIter,
    UpdatedField, Version,
};

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
    game: *const Game,
    server_version: Version,
    generator_version: Version,
    server_tags: HashSet<String>,
    password_required: bool,
    permissions: PermissionMap,
    hint_cost_percentage: u8,
    hint_points_per_check: u64,
    hint_points: u64,
    seed_name: String,
    games: UstrMap<Game>,
    slot_data: S,
    groups: Vec<NetworkSlot>,

    /// A map from `(team, slot)` to the player with that team and slot.
    players: HashMap<(u32, u32), Arc<Player>>,

    /// The key for the current player in [players].
    player_key: (u32, u32),

    /// The number of teams in this multiworld. Also the maximum team ID.
    teams: u32,

    /// A map from location IDs for this game to booleans indicating whether or
    /// not they've been checked.
    local_locations_checked: HashMap<i64, bool>,

    /// Senders for [Client.scout_locations].
    location_scout_senders: VecDeque<oneshot::Sender<Result<Vec<LocatedItem>, Error>>>,

    /// Senders for [Client.get].
    get_senders: VecDeque<oneshot::Sender<Result<HashMap<String, serde_json::Value>, Error>>>,
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
        //
        // TODO: also, apparently the data package may just be missing data for
        // some or all games. Handle this by falling back to a stupider Game
        // implementation. See
        // https://discord.com/channels/731205301247803413/731214280439103580/1453152623766012128
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
            items_handling: options.item_handling.into(),
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

        let teams = connected
            .players
            .iter()
            .map(|p| p.team)
            .max()
            .ok_or_else(|| ProtocolError::EmptyPlayers)?;

        let groups = connected
            .slot_info
            .extract_if(|_, slot| slot.r#type == SlotType::Group)
            .map(|(_, slot)| slot)
            .collect::<Vec<_>>();
        for group in &groups {
            for member in &group.group_members {
                if !connected.slot_info.contains_key(&member) {
                    return Err(ProtocolError::MissingSlotInfo(*member).into());
                }
            }
        }

        let players = connected
            .players
            .into_iter()
            .map(|p| {
                let game = connected
                    .slot_info
                    .get(&p.slot)
                    .or_else(|| groups.iter().find(|s| s.group_members.contains(&p.slot)))
                    .ok_or_else(|| ProtocolError::MissingSlotInfo(p.slot))?
                    .game;
                let player = Player::hydrate(p, game);
                Ok(((player.team(), player.slot()), player.into()))
            })
            .collect::<Result<HashMap<(u32, u32), Arc<Player>>, Error>>()?;
        let player_key = (connected.team, connected.slot);
        if !players.contains_key(&player_key) {
            return Err(ProtocolError::MissingPlayer {
                team: connected.team,
                slot: connected.slot,
            }
            .into());
        }

        let games = data_package
            .games
            .into_iter()
            .map(|(name, data)| (name, Game::hydrate(name, data)))
            .collect::<UstrMap<_>>();
        let game = games
            // Safety: This is only used for the duration of the get.
            .get(&Ustr::from(&game))
            .ok_or_else(|| ProtocolError::MissingGameData(game.into()))?;
        let game_ptr = ptr::from_ref(game);

        let mut local_locations_checked = HashMap::with_capacity(total_locations);
        for id in connected.missing_locations {
            game.verify_location(id)?;
            local_locations_checked.insert(id, false);
        }
        for id in connected.checked_locations {
            game.verify_location(id)?;
            local_locations_checked.insert(id, true);
        }

        Ok(Client {
            socket,
            game: game_ptr,
            server_version: room_info.version.into(),
            generator_version: room_info.generator_version.into(),
            server_tags: room_info.tags,
            password_required: room_info.password_required,
            permissions: room_info.permissions,
            hint_cost_percentage: room_info.hint_cost,
            hint_points_per_check: room_info.location_check_points,
            hint_points: connected.hint_points,
            seed_name: room_info.seed_name,
            games,
            slot_data: connected.slot_data,
            groups,
            players,
            player_key,
            teams,
            local_locations_checked,
            location_scout_senders: Default::default(),
            get_senders: Default::default(),
        })
    }

    // == Session information

    /// The game that's currently being played.
    pub fn this_game(&self) -> &Game {
        // Safety: This game is stored in [games], which we own and which is
        // never mutated.
        unsafe { &*self.game }
    }

    /// The version of Archipelago which the server is running.
    pub fn server_version(&self) -> &Version {
        &self.server_version
    }

    /// The version of Archipelago that generated the multiworld.
    pub fn generator_version(&self) -> &Version {
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
        (self.local_locations_checked.len() as u64) * u64::from(self.hint_cost_percentage) / 100
    }

    /// The number of hint points granted for each location a player checks.
    pub fn hint_points_per_check(&self) -> u64 {
        self.hint_points_per_check
    }

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

    /// A map from the names of each game in this multiworld to metadata about
    /// those games.
    pub fn games(&self) -> impl Iter<&Game> {
        self.games.values()
    }

    /// Returns the game with the given [name], if one is in this multiworld.
    ///
    /// Unlike [games], this will return the special [Game::archipelago] game.
    pub fn game(&self, name: impl Into<Ustr>) -> Option<&Game> {
        let name = name.into();
        // Safety: We own the name for the duration of the call.
        self.games.get(&name).or_else(|| {
            let archipelago = Game::archipelago();
            if name == archipelago.name() {
                Some(archipelago)
            } else {
                None
            }
        })
    }

    /// Returns the game in this multiworld with the given [name]. Panics if
    /// there's no game with that name.
    pub fn assert_game(&self, name: impl Into<Ustr>) -> &Game {
        let name = name.into();
        self.game(name)
            .unwrap_or_else(|| panic!("multiworld doesn't contain a game named \"{}\"", name))
    }

    /// Returns the game in this multiworld with the given [name]. Returns an
    /// error if there's no game with that name.
    pub(crate) fn game_or_err(&self, name: impl Into<Ustr>) -> Result<&Game, Error> {
        let name = name.into();
        self.game(name)
            .ok_or_else(|| ProtocolError::MissingGameData(name).into())
    }

    /// The player that's currently connected to the multiworld.
    pub fn this_player(&self) -> &Player {
        self.players[&self.player_key].as_ref()
    }

    /// All players in the multiworld.
    pub fn players(&self) -> impl Iter<&Player> {
        self.players.values().map(|p| p.as_ref())
    }

    /// The player on the given [team] playing the given [slot], if one exists.
    ///
    /// See also [teammate] to only check the current player's team.
    pub fn player(&self, team: u32, slot: u32) -> Option<&Player> {
        self.players.get(&(team, slot)).map(|p| p.as_ref())
    }

    /// CLones the Arc for the player on the given [team] playing the given
    /// [slot].
    pub(crate) fn player_arc(&self, team: u32, slot: u32) -> Result<Arc<Player>, Error> {
        self.players
            .get(&(team, slot))
            .map(|p| p.clone())
            .ok_or_else(|| ProtocolError::MissingPlayer { team, slot }.into())
    }

    /// The player on the given [team] playing the given [slot]. Panics if this
    /// player doesn't exist.
    ///
    /// See also [assert_teammate] to only check the current player's team.
    pub fn assert_player(&self, team: u32, slot: u32) -> &Player {
        self.player(team, slot).unwrap_or_else(|| {
            if self.players.keys().any(|k| k.0 == team) {
                panic!("no player with slot {}", slot);
            } else {
                panic!("no team with ID {}", team);
            }
        })
    }

    /// The player playing the given [slot] on the current player's team, if one
    /// exists.
    pub fn teammate(&self, slot: u32) -> Option<&Player> {
        self.player(self.player_key.0, slot)
    }

    /// A clone of the [Arc] for the player playing the given [slot] on the
    /// current player's team.
    pub(crate) fn teammate_arc(&self, slot: u32) -> Result<Arc<Player>, Error> {
        self.player_arc(self.player_key.0, slot)
    }

    /// The player playing the given [slot] on the current player's team. Panics
    /// if this player doesn't exist.
    ///
    /// See also [assert_teammate] to only check the current player's team.
    pub fn assert_teammate(&self, slot: u32) -> &Player {
        self.assert_player(self.player_key.0, slot)
    }

    /// The groups on the given [team], if such a team exists.
    pub fn groups(&self, team: u32) -> Option<impl Iter<Group>> {
        if team > self.teams {
            return None;
        } else {
            Some(
                self.groups
                    .iter()
                    .map(move |g| Group::hydrate(g, team, self)),
            )
        }
    }

    /// The groups on the current player's.
    pub fn teammate_groups(&self) -> impl Iter<Group> {
        let team = self.player_key.0;
        self.groups
            .iter()
            .map(move |g| Group::hydrate(g, team, self))
    }

    /// Returns whether the local location with the given ID has been checked
    /// (either by us or by other players doing co-op).
    ///
    /// This may only be called for locations in the connected game's world.
    /// Panics if it's called with a location ID that doesn't exist for this
    /// world.
    pub fn is_local_location_checked(&self, id: impl AsLocationId) -> bool {
        let id = id.as_location_id();
        *self.local_locations_checked.get(&id).unwrap_or_else(|| {
            panic!(
                "Archipelago location ID {} doesn't exist for {}",
                id,
                self.this_game().name()
            )
        })
    }

    /// Returns all the locations that the player has already checked.
    pub fn checked_locations(&self) -> impl UnsizedIter<Location> {
        let game = self.this_game();
        self.local_locations_checked
            .iter()
            .filter(|(_, checked)| **checked)
            .map(|(id, _)| game.assert_location(*id))
    }

    /// Returns all the locations that the player has not yet checked.
    pub fn unchecked_locations(&self) -> impl UnsizedIter<Location> {
        let game = self.this_game();
        self.local_locations_checked
            .iter()
            .filter(|(_, checked)| !**checked)
            .map(|(id, _)| game.assert_location(*id))
    }

    /// Returns the slot data provided by the apworld.
    pub fn slot_data(&self) -> &S {
        &self.slot_data
    }

    // == Requests

    /// Updates the current connection settings with new [item_handling] and/or [tags].
    pub fn update_connection(
        &mut self,
        item_handling: Option<ItemHandling>,
        tags: Option<impl IntoIterator<Item: Into<Ustr>>>,
    ) -> Result<(), Error> {
        self.socket
            .send(ClientMessage::ConnectUpdate(ConnectUpdate {
                items_handling: item_handling.map(|i| i.into()),
                tags: tags.map(|ts| ts.into_iter().map(|t| t.into()).collect()),
            }))
    }

    /// Requests that an [Event::ReceivedItems] be emitted with all the items
    /// the player has ever received.
    pub fn sync(&mut self) -> Result<(), Error> {
        self.socket.send(ClientMessage::Sync)
    }

    /// Notifies the server that the given [locations] have been checked.
    pub fn mark_checked(
        &mut self,
        locations: impl IntoIterator<Item = impl AsLocationId>,
    ) -> Result<(), Error> {
        let locations = self.verify_local_locations(locations)?;
        self.socket
            .send(ClientMessage::LocationChecks(LocationChecks {
                locations: locations.clone(),
            }))?;

        for id in locations {
            if matches!(self.local_locations_checked.insert(id, true), Some(false)) {
                self.hint_points += self.hint_points_per_check;
            }
        }
        Ok(())
    }

    /// Sends a request to the server that can serve one or both of two
    /// purposes:
    ///
    /// * Informing the client which items exist at which locations. At some
    ///   point shortly after this is called, the server will emit an
    ///   [Event::ScoutedLocations] which includes [LocatedItem]s for each
    ///   [location].
    ///
    /// * Informing the server of locations that the client has seen but not
    ///   checked. If [CreateAsHint.All] or [CreateAsHint.New] is passed,
    ///   scouted locations will be broadcast as hints without deducting hint
    ///   points from the player.
    pub fn scout_locations(
        &mut self,
        locations: impl IntoIterator<Item = impl AsLocationId>,
        create_as_hint: CreateAsHint,
    ) -> oneshot::Receiver<Result<Vec<LocatedItem>, Error>> {
        let (sender, receiver) = oneshot::channel();
        match self
            .verify_local_locations(locations)
            .and_then(|locations| {
                self.socket
                    .send(ClientMessage::LocationScouts(LocationScouts {
                        locations,
                        create_as_hint,
                    }))
            }) {
            Ok(()) => self.location_scout_senders.push_back(sender),
            // If `send()` returns an error, that means that the receiver was
            // dropped, which is fine to silently ignore.
            Err(err) => mem::drop(sender.send(Err(err))),
        }
        receiver
    }

    /// Updates the status of the given hint on the server.
    ///
    /// This allows the player to indicate which hinted items from other worlds
    /// they care about.
    pub fn update_hint(
        &mut self,
        slot: u32,
        location: impl AsLocationId,
        status: HintStatus,
    ) -> Result<(), Error> {
        let location = location.as_location_id();
        let player = self.verify_teammate(slot)?;
        let game = self.assert_game(player.game());
        if !game.has_location(location) {
            return Err(ArgumentError::InvalidLocation {
                location,
                game: game.name(),
            }
            .into());
        }

        self.socket.send(ClientMessage::UpdateHint(UpdateHint {
            player: player.slot(),
            location,
            status,
        }))
    }

    /// Notifies the server that the client has the given [status].
    pub fn set_status(&mut self, status: ClientStatus) -> Result<(), Error> {
        self.socket
            .send(ClientMessage::StatusUpdate(StatusUpdate { status }))
    }

    /// Retrieves custom data from the server's data store. The specific
    /// structure of the data is up to the clients that set it.
    ///
    /// THis can also retrieve special fields generated by the Archipelago
    /// server. See [the protocol documentation] for details.
    ///
    /// [the protocol documentation]: https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md#get
    pub fn get(
        &mut self,
        keys: impl IntoIterator<Item = impl Into<String>>,
    ) -> oneshot::Receiver<Result<HashMap<String, serde_json::Value>, Error>> {
        let (sender, receiver) = oneshot::channel();
        match self.socket.send(ClientMessage::Get(Get {
            keys: keys.into_iter().map(|k| k.into()).collect(),
        })) {
            Ok(()) => self.get_senders.push_back(sender),
            Err(err) => mem::drop(sender.send(Err(err))),
        }
        receiver
    }

    /// Sets custom data in the server's data store. The specific structure of
    /// the data is up to the clients that set it.
    ///
    /// If [emit_event] is `true`, [update] will eventually emit
    /// [Event::KeyChanged] for this key, even if it's not otherwise being
    /// watched.
    pub fn set(
        &mut self,
        key: impl Into<String>,
        value: serde_json::Value,
        emit_event: bool,
    ) -> Result<(), Error> {
        self.socket.send(ClientMessage::Set(Set {
            key: key.into(),
            default: serde_json::Value::Null,
            operations: vec![DataStorageOperation::Replace(value)],
            want_reply: emit_event,
        }))
    }

    /// Changes custom data in the server's data store. The specific structure
    /// of the data is up to the clients that set it.
    ///
    /// This applies each change in [operations] in order. If the key doesn't
    /// already have a value, [default] is used.
    ///
    /// If [emit_event] is `true`, [update] will eventually emit
    /// [Event::KeyChanged] for this key, even if it's not otherwise being
    /// watched.
    pub fn change(
        &mut self,
        key: impl Into<String>,
        default: serde_json::Value,
        operations: impl IntoIterator<Item = DataStorageOperation>,
        emit_event: bool,
    ) -> Result<(), Error> {
        self.socket.send(ClientMessage::Set(Set {
            key: key.into(),
            default,
            operations: operations.into_iter().collect(),
            want_reply: emit_event,
        }))
    }

    /// Watches the given [keys] in the server's data store. Any time the key is
    /// set (even if it doesn't change), [Event::KeySet] will be emitted.
    pub fn watch(
        &mut self,
        keys: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<(), Error> {
        self.socket.send(ClientMessage::SetNotify(SetNotify {
            keys: keys.into_iter().map(|k| k.into()).collect(),
        }))
    }

    /// Converts [locations] to a vector and verifies that they're valid for the
    /// current game.
    fn verify_local_locations(
        &self,
        locations: impl IntoIterator<Item = impl AsLocationId>,
    ) -> Result<Vec<i64>, Error> {
        let game = self.this_game();
        locations
            .into_iter()
            .map(|l| {
                let id = l.as_location_id();
                if game.has_location(id) {
                    Ok(id)
                } else {
                    Err(ArgumentError::InvalidLocation {
                        location: id,
                        game: game.name(),
                    }
                    .into())
                }
            })
            .collect()
    }

    /// Returns the [Player] for [slot] on the current team and verifies that
    /// it's a valid slot number.
    fn verify_teammate(&self, slot: u32) -> Result<&Player, Error> {
        self.teammate(slot)
            .ok_or(ArgumentError::InvalidSlot(slot).into())
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
    /// Most errors are fatal, and if emitted mean that the client will not emit
    /// any more events and should be discarded and reconnected. Some
    /// (specifically [Error::ProtocolError]s) are recoverable, though, and the
    /// client will continue to emit additional events after they're emitted if
    /// it's not dropped. You can detect which errors are fatal using
    /// [Error.is_fatal].
    pub fn update(&mut self) -> Vec<Event> {
        let mut events = Vec::<Event>::new();
        loop {
            match self.socket.try_recv() {
                Ok(Some(ServerMessage::RawPrint(print))) => match Print::hydrate(print, self) {
                    Ok(print) => events.push(Event::Print(print)),
                    Err(err) => events.push(Event::Error(err)),
                },

                Ok(Some(ServerMessage::PlainPrint(PlainPrint { text }))) => {
                    events.push(Event::Print(Print::message(text)))
                }

                Ok(Some(ServerMessage::RoomUpdate(update))) => match self.update_room(update) {
                    Ok(event) => events.push(event),
                    Err(err) => events.push(Event::Error(err)),
                },

                Ok(Some(ServerMessage::ReceivedItems(ReceivedItems { index, items }))) => {
                    let receiver = &self.players[&self.player_key];
                    let receiver_game = self.this_game();

                    let items_or_err = items
                        .into_iter()
                        .map(|network| {
                            let sender = self.teammate_arc(network.player)?;
                            let sender_game = self.game_or_err(sender.game())?;
                            LocatedItem::hydrate_with_games(
                                network,
                                sender,
                                receiver.clone(),
                                sender_game,
                                receiver_game,
                            )
                        })
                        .collect::<Result<Vec<LocatedItem>, Error>>();
                    events.push(match items_or_err {
                        Ok(items) => Event::ReceivedItems { index, items },
                        Err(err) => Event::Error(err),
                    })
                }

                Ok(Some(ServerMessage::LocationInfo(LocationInfo { locations }))) => {
                    let sender = &self.players[&self.player_key];
                    let sender_game = self.this_game();

                    let locations_or_err = locations
                        .into_iter()
                        .map(|network| {
                            let receiver = self.teammate_arc(network.player)?;
                            let receiver_game = self.game_or_err(sender.game())?;
                            LocatedItem::hydrate_with_games(
                                network,
                                sender.clone(),
                                receiver,
                                sender_game,
                                receiver_game,
                            )
                        })
                        .collect::<Result<Vec<LocatedItem>, Error>>();
                    if let Some(sender) = self.location_scout_senders.pop_front() {
                        mem::drop(sender.send(locations_or_err));
                    } else {
                        events.push(Event::Error(
                            ProtocolError::ResponseWithoutRequest("LocationInfo").into(),
                        ));
                    }
                }

                Ok(Some(ServerMessage::Bounced(Bounced {
                    games,
                    slots,
                    tags,
                    data: BounceData::Generic(data),
                }))) => events.push(Event::Bounce {
                    games,
                    slots,
                    tags,
                    data,
                }),

                Ok(Some(ServerMessage::Bounced(Bounced {
                    games,
                    slots,
                    tags,
                    data: BounceData::DeathLink(data),
                }))) => events.push(Event::DeathLink {
                    games,
                    slots,
                    tags: tags.unwrap(),
                    time: data.time,
                    cause: data.cause,
                    source: data.source,
                }),

                Ok(Some(ServerMessage::InvalidPacket(InvalidPacket { text }))) => {
                    events.push(Event::Error(Error::InvalidPacket(text)))
                }

                Ok(Some(ServerMessage::Retrieved(Retrieved { keys }))) => {
                    if let Some(sender) = self.get_senders.pop_front() {
                        mem::drop(sender.send(Ok(keys)));
                    } else {
                        events.push(Event::Error(
                            ProtocolError::ResponseWithoutRequest("Get").into(),
                        ));
                    }
                }

                Ok(Some(ServerMessage::SetReply(SetReply {
                    key,
                    value,
                    original_value,
                    slot,
                }))) => events.push(match self.teammate_arc(slot) {
                    Ok(player) => Event::KeyChanged {
                        key,
                        old_value: original_value,
                        new_value: value,
                        player,
                    },
                    Err(err) => Event::Error(err),
                }),

                // TODO: dispatch all events
                Ok(Some(_)) => todo!(),
                Ok(None) => return events,
                Err(err) => events.push(Event::Error(err)),
            }
        }
    }

    /// Updates the room with the information in [update].
    fn update_room(&mut self, update: RoomUpdate) -> Result<Event, Error> {
        // Check for errors before making any changes so we don't end up in an
        // intermediate state.
        let checked_locations = update
            .checked_locations
            .map(|ids| {
                let game = self.this_game();
                ids.into_iter()
                    .map(|id| game.location_or_err(id))
                    .collect::<Result<Vec<_>, Error>>()
            })
            .transpose()?;

        let updated_players = update
            .players
            .map(|players| {
                players
                    .into_iter()
                    .filter_map(|new| {
                        self.players
                            .get(&(new.team, new.slot))
                            .ok_or_else(|| ProtocolError::MissingPlayer {
                                team: new.team,
                                slot: new.slot,
                            })
                            .map(|old| {
                                if old.alias() == new.alias {
                                    None
                                } else {
                                    Some(Player::hydrate(new, old.game()))
                                }
                            })
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, ProtocolError>>()
            })
            .transpose()?;

        let mut updated = Vec::new();
        if let Some(tags) = update.tags {
            updated.push(UpdatedField::ServerTags(mem::replace(
                &mut self.server_tags,
                HashSet::from_iter(tags),
            )))
        }

        if let Some(permissions) = update.permissions {
            updated.push(UpdatedField::Permissions {
                release: self.permissions.release,
                collect: self.permissions.collect,
                remaining: self.permissions.remaining,
            });
            self.permissions = permissions;
        }

        if update.hint_cost.is_some() || update.location_check_points.is_some() {
            updated.push(UpdatedField::HintEconomy {
                points_per_hint: self.points_per_hint(),
                hint_points_per_check: self.hint_points_per_check(),
            });
            if let Some(hint_cost_percentage) = update.hint_cost {
                self.hint_cost_percentage = hint_cost_percentage;
            }
            if let Some(hint_points_per_check) = update.location_check_points {
                self.hint_points_per_check = hint_points_per_check;
            }
        }

        if let Some(hint_points) = update.hint_points {
            updated.push(UpdatedField::HintPoints(mem::replace(
                &mut self.hint_points,
                hint_points,
            )))
        }

        if let Some(players) = updated_players {
            updated.push(UpdatedField::Players(
                players
                    .into_iter()
                    .filter_map(|p| self.players.insert((p.team(), p.slot()), p.into()))
                    .collect(),
            ))
        }

        if let Some(locations) = checked_locations {
            updated.push(UpdatedField::CheckedLocations(
                locations
                    .into_iter()
                    // Omit locations that we already know are checked from
                    // local information.
                    .filter(|loc| !self.local_locations_checked.insert(loc.id(), true).unwrap())
                    .collect(),
            ))
        }

        Ok(Event::Updated(updated))
    }
}

// The only reason Client doesn't automatically implement [Unpin] is that S
// might not implement it (although being decoded from JSON it probably does).
// Since we treat slot data as immutable anyway, we can guarantee that nothing
// will change and so it's safe to declare the entire Client as Unpin.
impl<S> Unpin for Client<S> where S: DeserializeOwned {}
