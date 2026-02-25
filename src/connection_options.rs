use ustr::{Ustr, UstrSet};

use crate::{Cache, protocol::ItemsHandlingFlags};

/// A builder that defines options for
/// [Connection::new](crate::Connection::new).
pub struct ConnectionOptions {
    pub(crate) password: Option<String>,
    pub(crate) item_handling: ItemHandling,
    pub(crate) tags: UstrSet,
    pub(crate) cache: Option<Cache>,
}

impl ConnectionOptions {
    /// Creates a [ConnectionOptions] with default options.
    pub fn new() -> Self {
        Self {
            password: None,
            item_handling: Default::default(),
            tags: Default::default(),
            cache: None,
        }
    }

    /// Sets this player's password. By default, no password is passed.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Sets which items to receive. By default, you'll receive items from other
    /// worlds and your starting inventory, but not items from your own world.
    pub fn receive_items(mut self, items: ItemHandling) -> Self {
        self.item_handling = items;
        self
    }

    /// Sets the tags to send to the server to identify details of this client.
    pub fn tags(mut self, tags: impl IntoIterator<Item: Into<Ustr>>) -> Self {
        self.tags = tags.into_iter().map(|t| t.into()).collect();
        self
    }

    /// Specify where cached data should be stored.
    ///
    /// By default, this will write to Archipelago's shared cache directory.
    pub fn cache(mut self, cache: Cache) -> Self {
        self.cache = Some(cache);
        self
    }
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Possible options for handling items.
pub enum ItemHandling {
    /// No items are sent to this client.
    None,

    /// Items are sent from other worlds.
    OtherWorlds {
        /// Whether to also send items that are found in the local world.
        own_world: bool,

        /// Whether to also send items in the player's starting inventory.
        starting_inventory: bool,
    },
}

/// The default item handling receives items from other worlds and the player's
/// starting inventory, but not from the local world.
impl Default for ItemHandling {
    fn default() -> Self {
        ItemHandling::OtherWorlds {
            own_world: false,
            starting_inventory: true,
        }
    }
}

impl From<ItemHandling> for ItemsHandlingFlags {
    fn from(value: ItemHandling) -> ItemsHandlingFlags {
        let mut flags = ItemsHandlingFlags::empty();
        if let ItemHandling::OtherWorlds {
            own_world,
            starting_inventory,
        } = value
        {
            if own_world {
                flags.insert(ItemsHandlingFlags::OWN_WORLD);
            }
            if starting_inventory {
                flags.insert(ItemsHandlingFlags::STARTING_INVENTORY);
            }
        }
        flags
    }
}
