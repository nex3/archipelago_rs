use std::time::SystemTime;

/// A builder for options that can be passed to [Client.death_link].
///
/// This has sensible defaults for everything. See individual methods for
/// details.
#[derive(Debug, Clone)]
pub struct DeathLinkOptions {
    pub(crate) games: Option<Vec<String>>,
    pub(crate) slots: Option<Vec<String>>,
    pub(crate) time: Option<SystemTime>,
    pub(crate) cause: Option<String>,
}

impl DeathLinkOptions {
    /// Returns a [DeathLinkOptions] with all default option values.
    pub fn new() -> Self {
        DeathLinkOptions {
            games: None,
            slots: None,
            time: None,
            cause: None,
        }
    }

    /// Sets the names of games to which this death link will be broadcast.
    ///
    /// By default, it's broadcast to all games.
    pub fn games(mut self, games: Vec<String>) -> Self {
        self.games = Some(games);
        self
    }

    /// Sets the names of slots to which this death link will be broadcast.
    ///
    /// By default, it's broadcast to all slots.
    pub fn slots(mut self, slots: Vec<String>) -> Self {
        self.slots = Some(slots);
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

    /// Sets the cause of death. This should include the player's name. For
    /// example, "Berserker was run over by a train."
    ///
    /// By default, no cause is provided.
    pub fn cause(mut self, cause: String) -> Self {
        self.cause = Some(cause);
        self
    }
}

impl Default for DeathLinkOptions {
    fn default() -> Self {
        Self::new()
    }
}
