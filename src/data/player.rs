use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::{fmt, sync::Arc};

use crate::protocol::NetworkPlayer;

/// A single player (that is, slot) in the multiworld.
#[derive(Debug, Clone)]
pub struct Player {
    team: u64,
    slot: u64,
    alias: Arc<String>,
    name: Arc<String>,
    game: Arc<String>,
}

impl Player {
    /// Converts the raw network-level player struct into a [Player].
    pub(crate) fn hydrate(network: NetworkPlayer, game: Arc<String>) -> Player {
        Player {
            team: network.team,
            slot: network.slot,
            alias: network.alias,
            name: network.name,
            game,
        }
    }

    /// The player's team number. For multiworlds without teams, this will
    /// always be 0.
    pub fn team(&self) -> u64 {
        self.team
    }

    /// The player's slot number. Slot 0 refers to the Archipelago server, and
    /// normal players begin counting from 1.
    pub fn slot(&self) -> u64 {
        self.slot
    }

    /// The player's current name.
    pub fn alias(&self) -> &str {
        self.alias.as_str()
    }

    /// The player's original name at the time the session was generated.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// The name of the game this player is playing.
    pub fn game(&self) -> &str {
        self.game.as_str()
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
        self.team == other.team && self.slot == self.slot
    }
}

impl Eq for Player {}

impl Hash for Player {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.team.hash(state);
        self.slot.hash(state);
    }
}
