use std::collections::{HashMap, HashSet};
use std::{mem, ptr, sync::Arc};

use serde::de::DeserializeOwned;
use ustr::{Ustr, UstrMap};

use crate::{
    AsLocationId, ConnectionOptions, Error, Event, Game, Group, Iter, LocatedItem, Location,
    Player, Print, ProtocolError, Socket, UnsizedIter, UpdatedField, Version, protocol::*,
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
    pub fn player_arc(&self, team: u32, slot: u32) -> Result<Arc<Player>, Error> {
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
            Some(self.groups.iter().map(move |g| Group::hydrate(g, team, self)))
        }
    }

    /// The groups on the current player's.
    pub fn teammate_groups(&self) -> impl Iter<Group> {
        let team = self.player_key.0;
        self.groups.iter().map(move |g| Group::hydrate(g, team, self))
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
    ///
    /// Unless this is called (or [Connection.update], which calls this), the
    /// client will never change state.
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
                    let player = &self.players[&self.player_key];
                    let game = self.this_game();

                    let items_or_err = items
                        .into_iter()
                        .map(|network| {
                            if network.player != self.player_key.1 {
                                return Err(ProtocolError::ReceivedForeignItem(LocatedItem::hydrate(
                                    network, self,
                                )?)
                                .into());
                            }

                            LocatedItem::hydrate_with_player_and_game(network, player.clone(), game)
                        })
                        .collect::<Result<Vec<LocatedItem>, Error>>();
                    events.push(match items_or_err {
                        Ok(items) => Event::ReceivedItems { index, items },
                        Err(err) => Event::Error(err),
                    })
                }
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
