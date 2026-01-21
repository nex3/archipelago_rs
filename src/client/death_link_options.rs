use std::{collections::HashSet, time::SystemTime};

use ustr::{Ustr, UstrSet};

/// A builder for options that can be passed to [Client.death_link].
///
/// This has sensible defaults for everything. See individual methods for
/// details.
#[derive(Debug, Clone, Default)]
pub struct DeathLinkOptions {
    pub(crate) games: Option<UstrSet>,
    pub(crate) slots: Option<HashSet<u32>>,
    pub(crate) tags: Option<UstrSet>,
    pub(crate) time: Option<SystemTime>,
    pub(crate) source: Option<String>,
    pub(crate) cause: Option<String>,
}

impl DeathLinkOptions {
    /// Returns a [DeathLinkOptions] with all default option values.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sets the names of games to which this death link will be broadcast.
    ///
    /// By default, it's broadcast to all games.
    pub fn games(mut self, games: impl IntoIterator<Item = impl Into<Ustr>>) -> Self {
        self.games = Some(games.into_iter().map(|u| u.into()).collect());
        self
    }

    /// Sets the names of slots to which this death link will be broadcast.
    ///
    /// By default, it's broadcast to all slots.
    pub fn slots(mut self, slots: impl IntoIterator<Item = u32>) -> Self {
        self.slots = Some(slots.into_iter().collect());
        self
    }

    /// Sets the names of client tags to which this death link will be
    /// broadcast. The `"DeathLink"` tag is always implicitly added.
    ///
    /// By default, it's broadcast to all teammates with the `"DeathLink"` tags.
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<Ustr>>) -> Self {
        self.tags = Some(tags.into_iter().map(|u| u.into()).collect());
        self
    }

    /// Sets the time at which the death occurred.
    ///
    /// By default, this uses the time that the [Client.death_link] method is
    /// called.
    pub fn time(mut self, time: SystemTime) -> Self {
        self.time = Some(time);
        self
    }

    /// Sets the name of the player who died. Defaults to the current slot's
    /// alias.
    ///
    /// By default, no cause is provided.
    pub fn source(mut self, source: String) -> Self {
        self.cause = Some(source);
        self
    }

    /// Sets the cause of death. This should include the player's name. For
    /// example, "Berserker was run over by a train."
    ///
    /// By default, no cause is provided.
    pub fn cause(mut self, cause: String) -> Self {
        self.cause = Some(cause);
        self
    }
}
