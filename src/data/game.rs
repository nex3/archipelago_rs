use std::sync::{Arc, LazyLock};
use std::{collections::HashMap, fmt};

use super::{AsItemId, AsLocationId, CHEAT_CONSOLE, Item, Location, SERVER};
use crate::{Error, Iter, OwnedKey, ProtocolError, protocol::GameData};

/// The name of the special Archipelago game that's used for well-known
/// locations.
pub(crate) static ARCHIPELAGO_NAME: LazyLock<Arc<String>> =
    LazyLock::new(|| Arc::new("Archipelago".to_string()));

/// The well-known Archipelago game.
static ARCHIPELAGO: LazyLock<Game> = LazyLock::new(|| {
    Game::new(
        ARCHIPELAGO_NAME.clone(),
        vec![],
        vec![CHEAT_CONSOLE.clone(), SERVER.clone()],
    )
});

/// Information about a single game in an Archipelago multiworld.
pub struct Game {
    name: Arc<String>,

    /// All the items in this game.
    items: Vec<Arc<Item>>,

    /// All the locations in this game.
    locations: Vec<Arc<Location>>,

    // These map values are indices into [items].
    items_by_id: HashMap<i64, usize>,
    items_by_name: HashMap<OwnedKey<str>, usize>,

    // These map values are indices into [locations].
    locations_by_id: HashMap<i64, usize>,
    locations_by_name: HashMap<OwnedKey<str>, usize>,
}

impl Game {
    /// Constructs a [Game] directly from known items and locations.
    pub(crate) fn new(
        name: Arc<String>,
        items: Vec<Arc<Item>>,
        locations: Vec<Arc<Location>>,
    ) -> Game {
        let mut items_by_id = HashMap::with_capacity(items.len());
        let mut items_by_name = HashMap::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            items_by_id.insert(item.id(), i);
            // Safety: We own the item and this is immutable after creation.
            items_by_name.insert(unsafe { OwnedKey::from(item.name()) }, i);
        }

        let mut locations_by_id = HashMap::with_capacity(locations.len());
        let mut locations_by_name = HashMap::with_capacity(locations.len());
        for (i, location) in locations.iter().enumerate() {
            locations_by_id.insert(location.id(), i);
            // Safety: We own the location and this is immutable after creation.
            locations_by_name.insert(unsafe { OwnedKey::from(location.name()) }, i);
        }

        Game {
            name,
            items,
            locations,
            items_by_id,
            items_by_name,
            locations_by_id,
            locations_by_name,
        }
    }

    /// Converts the raw network-level game struct into a [Game].
    pub(crate) fn hydrate(name: Arc<String>, network: GameData) -> Game {
        // TODO: Is it really worth the hassle of supporting indexing by name?
        // Consider dropping that and just storing these as `HashMap<i64,
        // Arc<Item>>`s.
        let mut items = Vec::with_capacity(network.item_name_to_id.len());
        let mut items_by_id = HashMap::with_capacity(network.item_name_to_id.len());
        let mut items_by_name = HashMap::with_capacity(network.item_name_to_id.len());
        for (item_name, id) in network.item_name_to_id {
            items_by_id.insert(id, items.len());
            // Safety: We own the item and this is immutable after creation.
            items_by_name.insert(unsafe { OwnedKey::from_arc(&item_name) }, items.len());
            items.push(Arc::new(Item::new(id, item_name, name.clone())));
        }

        let mut locations = Vec::with_capacity(network.location_name_to_id.len());
        let mut locations_by_id = HashMap::with_capacity(network.location_name_to_id.len());
        let mut locations_by_name = HashMap::with_capacity(network.location_name_to_id.len());
        for (location_name, id) in network.location_name_to_id {
            locations_by_id.insert(id, locations.len());
            // Safety: We own the item and this is immutable after creation.
            locations_by_name.insert(
                unsafe { OwnedKey::from_arc(&location_name) },
                locations.len(),
            );
            locations.push(Arc::new(Location::new(id, location_name, name.clone())));
        }

        Game {
            name,
            items,
            locations,
            items_by_id,
            items_by_name,
            locations_by_id,
            locations_by_name,
        }
    }

    /// The pseudo-game "Archipelago" that's used to represent the location of
    /// items such as those sent by `!getitem`.
    pub fn archipelago() -> &'static Game {
        &*ARCHIPELAGO
    }

    /// Returns the game's name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// All the items in this game.
    pub fn items(&self) -> impl Iter<&Item> {
        self.items.iter().map(|i| i.as_ref())
    }

    /// All the locations in this game.
    pub fn locations(&self) -> impl Iter<&Location> {
        self.locations.iter().map(|l| l.as_ref())
    }

    /// Returns the item for the given [id] if one is defined in this game.
    pub fn item(&self, id: impl AsItemId) -> Option<&Item> {
        self.items_by_id
            .get(&id.as_item_id())
            .map(|i| self.items[*i].as_ref())
    }

    /// Returns a clone of the [Arc] for the item with the given [id].
    pub(crate) fn item_arc(&self, id: impl AsItemId) -> Result<Arc<Item>, Error> {
        let id = id.as_item_id();
        self.items_by_id
            .get(&id)
            .map(|i| self.items[*i].clone())
            .ok_or_else(|| {
                ProtocolError::MissingItem {
                    id,
                    game: self.name.clone(),
                }
                .into()
            })
    }

    /// Returns the item for the given [id]. Panics if there's no item with this
    /// ID.
    pub fn assert_item(&self, id: impl AsItemId) -> &Item {
        let id = id.as_item_id();
        self.item(id)
            .unwrap_or_else(|| panic!("{} doesn't contain an item with ID {}", self.name, id))
    }

    /// Returns the item with the given [name] if one is defined in this game.
    pub fn item_by_name(&self, name: impl AsRef<str>) -> Option<&Item> {
        self.items_by_name
            // Safety: the key is only used for this call while we own name.
            .get(&unsafe { OwnedKey::from(name) })
            .map(|i| self.items[*i].as_ref())
    }

    /// Returns the item with the given [name]. Panics if there's no item with
    /// this name.
    pub fn assert_item_by_name(&self, name: impl AsRef<str>) -> &Item {
        let name = name.as_ref();
        self.item_by_name(name)
            .unwrap_or_else(|| panic!("{} doesn't contain an item named \"{}\"", self.name, name))
    }

    /// Returns the location for the given [id] if one is defined in this game.
    pub fn location(&self, id: impl AsLocationId) -> Option<&Location> {
        self.locations_by_id
            .get(&id.as_location_id())
            .map(|i| self.locations[*i].as_ref())
    }

    /// Returns a clone of the [Arc] for the location with the given [id].
    pub(crate) fn location_arc(&self, id: impl AsLocationId) -> Result<Arc<Location>, Error> {
        let id = id.as_location_id();
        self.locations_by_id
            .get(&id)
            .map(|i| self.locations[*i].clone())
            .ok_or_else(|| {
                ProtocolError::MissingLocation {
                    id,
                    game: self.name.clone(),
                }
                .into()
            })
    }

    /// Returns the location for the given [id]. Panics if there's no location
    /// with this ID.
    pub fn assert_location(&self, id: impl AsLocationId) -> &Location {
        let id = id.as_location_id();
        self.location(id)
            .unwrap_or_else(|| panic!("{} doesn't contain an location with ID {}", self.name, id))
    }

    /// Returns the location with the given [name] if one is defined in this
    /// game.
    pub fn location_by_name(&self, name: impl AsRef<str>) -> Option<&Location> {
        self.locations_by_name
            // Safety: the key is only used for this call while we own name.
            .get(&unsafe { OwnedKey::from(name) })
            .map(|i| self.locations[*i].as_ref())
    }

    /// Returns the location with the given [name]. Panics if there's no
    /// location with this name.
    pub fn assert_location_by_name(&self, name: impl AsRef<str>) -> &Location {
        let name = name.as_ref();
        self.location_by_name(name).unwrap_or_else(|| {
            panic!(
                "{} doesn't contain an location named \"{}\"",
                self.name, name
            )
        })
    }
}

impl fmt::Debug for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{} ({} items, {} locations)",
            self.name,
            self.items.len(),
            self.locations.len()
        )
    }
}
