use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

use crate::ARCHIPELAGO_NAME;

/// A location in a game where an item may be placed.
#[derive(Debug, Clone)]
pub struct Location {
    id: i64,
    name: Arc<String>,
    game: Arc<String>,
}

/// The well-known cheat console location. This is stored as an Arc so it can be
/// used in places that expect other locations.
pub(crate) static CHEAT_CONSOLE: LazyLock<Arc<Location>> = LazyLock::new(|| {
    Arc::new(Location {
        id: -1,
        name: "Cheat Console".to_string().into(),
        game: ARCHIPELAGO_NAME.clone(),
    })
});

/// The well-known server location. This is stored as an Arc so it can be used
/// in places that expect other locations.
pub(crate) static SERVER: LazyLock<Arc<Location>> = LazyLock::new(|| {
    Arc::new(Location {
        id: -2,
        name: "Server".to_string().into(),
        game: ARCHIPELAGO_NAME.clone(),
    })
});

impl Location {
    /// Create a new location.
    pub(crate) fn new(id: i64, name: Arc<String>, game: Arc<String>) -> Location {
        Location { id, name, game }
    }

    /// The special location indicating that an item came from the cheat
    /// console.
    pub fn cheat_console() -> &'static Location {
        CHEAT_CONSOLE.as_ref()
    }

    /// The special location indicating that an item came from the server
    /// (typically starting inventory items).
    pub fn server() -> &'static Location {
        SERVER.as_ref()
    }

    /// If [id] represents a well-known universal location like [cheat_console]
    /// or [server], returns that location.
    pub fn well_known(id: i64) -> Option<&'static Location> {
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
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// This game this location exists in.
    pub fn game(&self) -> &str {
        self.game.as_str()
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
