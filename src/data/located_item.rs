use std::{fmt, sync::Arc};

use serde::de::DeserializeOwned;

use crate::protocol::{NetworkItem, NetworkItemFlags};
use crate::{Client, Error, Game, Item, Location, Player, ProtocolError};

/// An item associated with a particular location in particular player's world.
#[derive(Clone)]
pub struct LocatedItem {
    item: Item,
    location: Location,
    player: Arc<Player>,
    flags: NetworkItemFlags,
}

impl LocatedItem {
    /// Creates a fully-hydrated [LocatedItem] from a [NetworkItem].
    pub(crate) fn hydrate<S: DeserializeOwned>(
        network: NetworkItem,
        client: &Client<S>,
    ) -> Result<LocatedItem, Error> {
        let player = client.teammate_arc(network.player)?;
        let game = client
            .game(player.game())
            .ok_or_else(|| ProtocolError::MissingGameData(player.game()))?;
        LocatedItem::hydrate_with_player_and_game(network, player, game)
    }

    /// Creates a fully-hydrated [LocatedItem] from an existing [player] and [game].
    pub(crate) fn hydrate_with_player_and_game(
        network: NetworkItem,
        player: Arc<Player>,
        game: &Game,
    ) -> Result<LocatedItem, Error> {
        debug_assert!(network.player == player.slot());
        debug_assert!(player.game() == game.name());
        Ok(LocatedItem {
            item: game.item_or_err(network.item)?,
            location: match Location::well_known(network.location) {
                Some(location) => location,
                None => game.location_or_err(network.location)?,
            },
            player,
            flags: network.flags,
        })
    }

    /// The item at this location.
    pub fn item(&self) -> Item {
        self.item
    }

    /// The location that contains this item.
    pub fn location(&self) -> Location {
        self.location
    }

    /// The player to which this item belongs.
    pub fn player(&self) -> &Player {
        self.player.as_ref()
    }

    /// Whether this item can unblock logical advancement.
    pub fn is_progression(&self) -> bool {
        self.flags.contains(NetworkItemFlags::PROGRESSION)
    }

    /// Whether this item is especially useful.
    pub fn is_useful(&self) -> bool {
        self.flags.contains(NetworkItemFlags::USEFUL)
    }

    /// Whether this item is a trap.
    pub fn is_trap(&self) -> bool {
        self.flags.contains(NetworkItemFlags::TRAP)
    }
}

impl fmt::Debug for LocatedItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "item {} ({}) at location {} ({}) for {}",
            self.item.id(),
            self.item.name(),
            self.location.id(),
            self.location.name(),
            self.player.alias(),
        )
    }
}
