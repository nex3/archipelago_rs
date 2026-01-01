use std::fmt;

use crate::{Item, LocatedItem, Location, Player};

/// An item that was received from the server.
///
/// This has all the information in a [LocatedItem], as well as
/// [ReceivedItem.index] which indicates its position in the global list of all
/// items this slot has ever received.
#[derive(Clone)]
pub struct ReceivedItem {
    item: LocatedItem,
    index: usize,
}

impl ReceivedItem {
    /// Creates a [ReceivedItem] from a [LocatedItem] and its index in the
    /// list of all items the player has ever received.
    pub(crate) fn new(item: LocatedItem, index: usize) -> ReceivedItem {
        ReceivedItem { item, index }
    }

    /// The index of this item in the list of all items the connected player has
    /// ever been sent (which is available as
    /// [Client::received_items](crate::Client::received_items)). See
    /// [Synchronizing Items] for details on how to use this to keep the
    /// player's items in sync with the server.
    ///
    /// [Synchronizing Items]: https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md#synchronizing-items
    ///
    /// This will be 0 for the first item in
    /// [Event::ReceivedItems](crate::Event::ReceivedItems) if the server is
    /// sending the player's entire inventory again.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The item at this location.
    pub fn item(&self) -> Item {
        self.item.item()
    }

    /// The location that contains this item.
    pub fn location(&self) -> Location {
        self.item.location()
    }

    /// The player whose world contains `location`.
    pub fn sender(&self) -> &Player {
        self.item.sender()
    }

    /// The player to whom `item` has been or would be sent.
    pub fn receiver(&self) -> &Player {
        self.item.receiver()
    }

    /// Whether this item can unblock logical advancement.
    pub fn is_progression(&self) -> bool {
        self.item.is_progression()
    }

    /// Whether this item is especially useful.
    pub fn is_useful(&self) -> bool {
        self.item.is_useful()
    }

    /// Whether this item is a trap.
    pub fn is_trap(&self) -> bool {
        self.item.is_trap()
    }
}

impl fmt::Debug for ReceivedItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        self.item.fmt(f)
    }
}

impl From<ReceivedItem> for LocatedItem {
    fn from(value: ReceivedItem) -> LocatedItem {
        value.item
    }
}

impl AsRef<LocatedItem> for &ReceivedItem {
    fn as_ref(&self) -> &LocatedItem {
        &self.item
    }
}
