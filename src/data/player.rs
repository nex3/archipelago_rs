use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::{collections::HashMap, fmt, sync::Arc};

use ustr::Ustr;

use crate::{ARCHIPELAGO_NAME, Error, ProtocolError, protocol::*};

/// A single player (that is, slot) in the multiworld.
#[derive(Debug, Clone)]
pub struct Player {
    team: u32,
    slot: u32,
    alias: String,
    name: Ustr,
    game: Ustr,
    group_members: Vec<Arc<Player>>,
}

impl Player {
    /// Converts the raw network-level player struct into a [Player].
    pub(crate) fn hydrate(
        network: NetworkPlayer,
        slot_info: &NetworkSlot,
        players: &HashMap<(u32, u32), Arc<Player>>,
    ) -> Result<Self, Error> {
        let team = network.team;
        Ok(Player {
            team,
            slot: network.slot,
            alias: network.alias,
            name: network.name,
            game: slot_info.game,
            group_members: slot_info
                .group_members
                .iter()
                .copied()
                .map(|slot| {
                    players
                        .get(&(team, slot))
                        .cloned()
                        .ok_or(ProtocolError::MissingPlayer { team, slot })
                })
                .collect::<Result<_, _>>()?,
        })
    }

    /// Returns the special reserved player used for Archipelago itself.
    pub(crate) fn archipelago(team: u32) -> Self {
        Player {
            team,
            slot: 0,
            alias: "Archipelago".into(),
            name: *ARCHIPELAGO_NAME,
            game: *ARCHIPELAGO_NAME,
            group_members: Default::default(),
        }
    }

    /// If [alias] is different than this player's current alias, returns a
    /// clone of this player with the new alias.
    pub(crate) fn with_alias(&self, alias: String) -> Option<Self> {
        if self.alias == alias {
            None
        } else {
            Some(Player {
                alias,
                ..self.clone()
            })
        }
    }

    /// The player's team number. For multiworlds without teams, this will
    /// always be 0.
    pub fn team(&self) -> u32 {
        self.team
    }

    /// The player's slot number. Slot 0 refers to the Archipelago server, and
    /// normal players begin counting from 1.
    pub fn slot(&self) -> u32 {
        self.slot
    }

    /// The player's current name.
    pub fn alias(&self) -> &str {
        self.alias.as_str()
    }

    /// The player's original name at the time the session was generated.
    pub fn name(&self) -> Ustr {
        self.name
    }

    /// The name of the game this player is playing.
    pub fn game(&self) -> Ustr {
        self.game
    }

    /// The members of this player, if it's a group.
    ///
    /// A player is a group if and only if it has any members.
    pub fn group_members(&self) -> &[Arc<Player>] {
        &self.group_members
    }
}

/// Displays the player's alias.
impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        self.alias.fmt(f)
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.team == other.team && self.slot == other.slot
    }
}

impl Eq for Player {}

impl Hash for Player {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.team.hash(state);
        self.slot.hash(state);
    }
}
