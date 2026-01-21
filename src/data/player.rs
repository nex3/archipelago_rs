use std::cmp::{Eq, PartialEq};
use std::fmt;
use std::hash::{Hash, Hasher};

use ustr::Ustr;

use crate::{ARCHIPELAGO_NAME, protocol::NetworkPlayer};

/// A single player (that is, slot) in the multiworld.
#[derive(Debug, Clone)]
pub struct Player {
    team: u32,
    slot: u32,
    alias: String,
    name: Ustr,
    game: Ustr,
}

impl Player {
    /// Converts the raw network-level player struct into a [Player].
    pub(crate) fn hydrate(network: NetworkPlayer, game: Ustr) -> Self {
        Player {
            team: network.team,
            slot: network.slot,
            alias: network.alias,
            name: network.name,
            game,
        }
    }

    /// Returns the special reserved player used for Archipelago itself.
    pub(crate) fn archipelago(team: u32) -> Self {
        Player {
            team,
            slot: 0,
            alias: "Archipelago".into(),
            name: *ARCHIPELAGO_NAME,
            game: *ARCHIPELAGO_NAME,
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
