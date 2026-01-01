use std::{fmt, sync::Arc};

use serde::de::DeserializeOwned;

use crate::protocol::{NetworkItem, NetworkItemFlags};
use crate::{Client, Error, Game, Item, Location, Player};

/// An item associated with a particular location in particular player's world.
#[derive(Clone)]
pub struct LocatedItem {
    item: Item,
    location: Location,
    sender: Arc<Player>,
    receiver: Arc<Player>,
    flags: NetworkItemFlags,
}

impl LocatedItem {
    /// Creates a fully-hydrated [LocatedItem] from a [NetworkItem].
    ///
    /// Because a [NetworkItem] alone doesn't provide full context on who the
    /// sender or receiver is, this requires them to be passed in explicitly.
    pub(crate) fn hydrate<S: DeserializeOwned>(
        network: NetworkItem,
        sender: Arc<Player>,
        receiver: Arc<Player>,
        client: &Client<S>,
    ) -> Result<LocatedItem, Error> {
        let sender_game = client.game_or_err(sender.game())?;
        let receiver_game = client.game_or_err(receiver.game())?;
        LocatedItem::hydrate_with_games(network, sender, receiver, sender_game, receiver_game)
    }

    /// Creates a fully-hydrated [LocatedItem] from an already-loaded
    /// [sender_game] and [receiver_game].
    pub(crate) fn hydrate_with_games(
        network: NetworkItem,
        sender: Arc<Player>,
        receiver: Arc<Player>,
        sender_game: &Game,
        receiver_game: &Game,
    ) -> Result<LocatedItem, Error> {
        debug_assert!(network.player == sender.slot() || network.player == receiver.slot());
        debug_assert!(sender.game() == sender_game.name());
        debug_assert!(receiver.game() == receiver_game.name());
        Ok(LocatedItem {
            item: receiver_game.item_or_err(network.item)?,
            location: match Location::well_known(network.location) {
                Some(location) => location,
                None => sender_game.location_or_err(network.location)?,
            },
            sender,
            receiver,
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

    /// The player whose world contains `location`.
    pub fn sender(&self) -> &Player {
        self.sender.as_ref()
    }

    /// The player to whom `item` has been or would be sent.
    pub fn receiver(&self) -> &Player {
        self.receiver.as_ref()
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
            "item {} ({}) at location {} ({}) from {} for {}",
            self.item.id(),
            self.item.name(),
            self.location.id(),
            self.location.name(),
            self.sender.alias(),
            self.receiver.alias(),
        )
    }
}
