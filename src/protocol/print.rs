use std::fmt;

use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};

use crate::protocol::*;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PlainPrint {
    pub text: String,
}

// Not a very elegant way to handle this. See
// https://github.com/serde-rs/serde/issues/1799.

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum NetworkPrint {
    ItemSend {
        data: Vec<NetworkText>,
        receiving: u32,
        item: NetworkItem,
    },
    ItemCheat {
        data: Vec<NetworkText>,
        receiving: u32,
        item: NetworkItem,
        team: u32,
    },
    Hint {
        data: Vec<NetworkText>,
        receiving: u32,
        item: NetworkItem,
        found: bool,
    },
    Join {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
        tags: Vec<String>,
    },
    Part {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
    },
    Chat {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
        message: String,
    },
    ServerChat {
        data: Vec<NetworkText>,
        message: String,
    },
    Tutorial {
        data: Vec<NetworkText>,
    },
    TagsChanged {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
        tags: Vec<String>,
    },
    CommandResult {
        data: Vec<NetworkText>,
    },
    AdminCommandResult {
        data: Vec<NetworkText>,
    },
    Goal {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
    },
    Release {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
    },
    Collect {
        data: Vec<NetworkText>,
        team: u32,
        slot: u32,
    },
    Countdown {
        data: Vec<NetworkText>,
        countdown: u64,
    },
    #[serde(untagged)]
    Unknown {
        data: Vec<NetworkText>,
    },
}

impl NetworkPrint {
    /// Returns the data field for any NetworkPrint.
    pub(crate) fn data(&self) -> &[NetworkText] {
        use NetworkPrint::*;
        match self {
            ItemSend { data, .. } => data,
            ItemCheat { data, .. } => data,
            Hint { data, .. } => data,
            Join { data, .. } => data,
            Part { data, .. } => data,
            Chat { data, .. } => data,
            ServerChat { data, .. } => data,
            Tutorial { data, .. } => data,
            TagsChanged { data, .. } => data,
            CommandResult { data, .. } => data,
            AdminCommandResult { data, .. } => data,
            Goal { data, .. } => data,
            Release { data, .. } => data,
            Collect { data, .. } => data,
            Countdown { data, .. } => data,
            Unknown { data, .. } => data,
        }
    }
}

impl fmt::Display for NetworkPrint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for part in self.data() {
            part.fmt(f)?;
        }
        Ok(())
    }
}

/// A single text component of a [NetworkPrint], with additional metadata indicating
/// its formatting and semantics.
///
/// Unlike [RichText], this has not yet been hydrated with additional metadata
/// known by the client.
#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NetworkText {
    PlayerId {
        /// The slot ID of the player this part refers to.
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: u32,
    },
    PlayerName {
        text: String,
    },
    ItemId {
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: i64,
        player: u32,
        flags: NetworkItemFlags,
    },
    LocationId {
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: i64,
        player: u32,
    },
    EntranceName {
        text: String,
    },
    Color {
        text: String,
        color: TextColor,
    },
    // We don't explicitly provide variants for `ItemName` or `LocationName`
    // because they aren't ever actually sent by the server. If that changes,
    // this will fall back to using [Text] for them until we add explicit
    // support.
    #[serde(untagged)]
    Text {
        text: String,
    },
}

impl fmt::Display for NetworkText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use NetworkText::*;
        match self {
            PlayerName { text, .. }
            | EntranceName { text, .. }
            | Color { text, .. }
            | Text { text, .. } => text.fmt(f),
            PlayerId { id, .. } => write!(f, "<player {}>", id),
            ItemId { id, player, .. } => write!(f, "<item {}:{}>", player, id),
            LocationId { id, player, .. } => write!(f, "<loc {}:{}>", player, id),
        }
    }
}

/// Possible colors for Archipelago text.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextColor {
    Bold,
    Underline,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BlackBg,
    RedBg,
    GreenBg,
    YellowBg,
    BlueBg,
    MagentaBg,
    CyanBg,
    WhiteBg,
}
