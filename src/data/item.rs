use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// An Archipelago item for some player's game.
#[derive(Debug, Clone)]
pub struct Item {
    id: i64,
    name: Arc<String>,
    game: Arc<String>,
}

impl Item {
    /// Create a new item.
    pub(crate) fn new(id: i64, name: Arc<String>, game: Arc<String>) -> Item {
        Item { id, name, game }
    }

    /// The item's numeric ID.
    pub fn id(&self) -> i64 {
        self.id
    }

    /// The item's name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// This game this item exists in.
    pub fn game(&self) -> &str {
        self.game.as_str()
    }
}

impl PartialEq for Item {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Item {}

impl Hash for Item {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A trait for values that can be interpreted as Archipelago items.
pub trait AsItemId {
    /// Returns this value as an Archipelago item ID.
    fn as_item_id(&self) -> i64;

    /// Returns whether this represents the same Archipelago item as
    /// [other].
    fn same_item(&self, other: impl AsItemId) -> bool {
        self.as_item_id() == other.as_item_id()
    }
}

impl AsItemId for Item {
    fn as_item_id(&self) -> i64 {
        self.id
    }
}

impl AsItemId for i64 {
    fn as_item_id(&self) -> i64 {
        *self
    }
}
