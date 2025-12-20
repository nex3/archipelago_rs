use crate::protocol::ItemsHandlingFlags;

/// A builder that defines options for [Connection::new].
pub struct ConnectionOptions {
    pub(crate) password: Option<String>,
    pub(crate) items_handling: ItemsHandlingFlags,
    pub(crate) tags: Vec<String>,
    pub(crate) slot_data: bool,
}

impl ConnectionOptions {
    /// Creates a [ConnectionOptions] with default options.
    pub fn new() -> Self {
        Self {
            password: None,
            items_handling: ItemsHandlingFlags::OTHER_WORLDS
                | ItemsHandlingFlags::STARTING_INVENTORY,
            tags: Vec::new(),
            slot_data: true,
        }
    }

    /// Sets this player's password. By default, no password is passed.
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    // TODO: Make a cleaner enum for ItemsHandlingFlags
    /// Sets which items to receive. By default, you'll receive items from other
    /// worlds and your starting inventory, but not items from your own world.
    pub fn receive_items(mut self, items: ItemsHandlingFlags) -> Self {
        self.items_handling = items;
        self
    }

    /// Sets the tags to send to the server to identify details of this client.
    pub fn tags(mut self, tags: impl IntoIterator<Item: Into<String>>) -> Self {
        self.tags = tags.into_iter().map(|t| t.into()).collect();
        self
    }

    /// Don't receive slot data.
    pub fn no_slot_data(mut self) -> Self {
        self.slot_data = false;
        self
    }
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self::new()
    }
}
