use std::cmp::{Eq, PartialEq};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use ustr::Ustr;

use crate::ARCHIPELAGO_NAME;

/// A location in a game where an item may be placed.
#[derive(Debug, Clone, Copy)]
pub struct Location {
    id: i64,
    name: Ustr,
    game: Ustr,
}

/// The well-known cheat console location. This is stored as an Arc so it can be
/// used in places that expect other locations.
static CHEAT_CONSOLE: LazyLock<Location> = LazyLock::new(|| Location {
    id: -1,
    name: "Cheat Console".to_string().into(),
    game: *ARCHIPELAGO_NAME,
});

/// The well-known server location. This is stored as an Arc so it can be used
/// in places that expect other locations.
static SERVER: LazyLock<Location> = LazyLock::new(|| Location {
    id: -2,
    name: "Server".to_string().into(),
    game: *ARCHIPELAGO_NAME,
});

impl Location {
    /// Create a new location.
    pub(crate) fn new(id: i64, name: Ustr, game: Ustr) -> Location {
        Location { id, name, game }
    }

    /// The special location indicating that an item came from the cheat
    /// console.
    pub fn cheat_console() -> Location {
        *CHEAT_CONSOLE
    }

    /// The special location indicating that an item came from the server
    /// (typically starting inventory items).
    pub fn server() -> Location {
        *SERVER
    }

    /// If [id] represents a well-known universal location like [cheat_console]
    /// or [server], returns that location.
    pub fn well_known(id: i64) -> Option<Location> {
        match id {
            -1 => Some(Self::cheat_console()),
            -2 => Some(Self::server()),
            _ => None,
        }
    }

    /// This location's numeric ID.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// This location's name.
    pub fn name(&self) -> Ustr {
        self.name
    }

    /// This game this location exists in.
    pub fn game(&self) -> Ustr {
        self.game
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

impl PartialEq for Location {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Location {}

impl Hash for Location {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A trait for values that can be interpreted as Archipelago locations.
pub trait AsLocationId {
    /// Returns this value as an Archipelago location ID.
    fn as_location_id(&self) -> i64;

    /// Returns whether this represents the same Archipelago location as
    /// [other].
    fn same_location(&self, other: impl AsLocationId) -> bool {
        self.as_location_id() == other.as_location_id()
    }
}

impl AsLocationId for Location {
    fn as_location_id(&self) -> i64 {
        self.id
    }
}

impl AsLocationId for i64 {
    fn as_location_id(&self) -> i64 {
        *self
    }
}
