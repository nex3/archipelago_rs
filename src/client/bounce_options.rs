use std::collections::HashSet;

use ustr::{Ustr, UstrSet};

/// A builder for options for which clients will receive a [Client.bounce]. By
/// default, all teammates receive it.
#[derive(Debug, Clone, Default)]
pub struct BounceOptions {
    pub(crate) games: Option<UstrSet>,
    pub(crate) slots: Option<HashSet<u32>>,
    pub(crate) tags: Option<UstrSet>,
}

impl BounceOptions {
    /// Creates a [BounceOptions] with all default option values.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the names of games to which this bounce will be broadcast.
    ///
    /// By default, it's broadcast to all games.
    pub fn games(mut self, games: impl IntoIterator<Item = impl Into<Ustr>>) -> Self {
        self.games = Some(games.into_iter().map(|u| u.into()).collect());
        self
    }

    /// Sets the names of slots to which this bounce will be broadcast.
    ///
    /// By default, it's broadcast to all slots.
    pub fn slots(mut self, slots: impl IntoIterator<Item = u32>) -> Self {
        self.slots = Some(slots.into_iter().collect());
        self
    }

    /// Sets the names of client tags to which this bounce will be broadcast.
    ///
    /// By default, it's broadcast to all teammates.
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<Ustr>>) -> Self {
        self.tags = Some(tags.into_iter().map(|u| u.into()).collect());
        self
    }
}
