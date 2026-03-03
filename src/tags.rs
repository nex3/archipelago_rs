//! Well-known tags for Archipelago clients with specific semantics attached.

/// Indicates that this is a reference client. It's mostly useful for debugging,
/// to compare client behaviors more easily.
pub const AP: &str = "AP";

/// Indicates that this client participates in the DeathLink mechanic. Clients
/// with this tag send and receive [DeathLink](crate::Event::DeathLink) packets.
pub const DEATH_LINK: &str = "DeathLink";

/// Indicates that this client is a hint game, made to send hints instead of
/// locations.
pub const HINT_GAME: &str = "HintGame";

/// Indicates that this client is a tracker, made to track progress instead of
/// sending locations.
pub const TRACKER: &str = "Tracker";

/// Indicates that this is a basic client, made to chat instead of sending
/// locations.
pub const TEXT_ONLY: &str = "TextOnly";

/// Indicates the client does not want to receive text messages, which can
/// improve performance.
pub const NO_TEXT: &str = "NoText";
