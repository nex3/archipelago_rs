use std::fmt;

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::protocol::*;

// Not a very elegant way to handle this. See
// https://github.com/serde-rs/serde/issues/1799.

/// A rich-text message sent by the server, with annotations indicating how the
/// text should be formatted and what individual components refer to.
///
/// When this is received from the server, any [RichMessageId]s it contains
/// won't have their names filled in. Pass it to [DataPackage.add_names] to add
/// those in order to make it human-readable before displaying it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RichPrint {
    ItemSend {
        data: Vec<RichMessagePart>,
        receiving: i64,
        item: NetworkItem,
    },
    ItemCheat {
        data: Vec<RichMessagePart>,
        receiving: i64,
        item: NetworkItem,
        team: i64,
    },
    Hint {
        data: Vec<RichMessagePart>,
        receiving: i64,
        item: NetworkItem,
        found: bool,
    },
    Join {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
        tags: Vec<String>,
    },
    Part {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
    },
    Chat {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
        message: String,
    },
    ServerChat {
        data: Vec<RichMessagePart>,
        message: String,
    },
    Tutorial {
        data: Vec<RichMessagePart>,
    },
    TagsChanged {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
        tags: Vec<String>,
    },
    CommandResult {
        data: Vec<RichMessagePart>,
    },
    AdminCommandResult {
        data: Vec<RichMessagePart>,
    },
    Goal {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
    },
    Release {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
    },
    Collect {
        data: Vec<RichMessagePart>,
        team: i64,
        slot: i64,
    },
    Countdown {
        data: Vec<RichMessagePart>,
        countdown: i64,
    },
    #[serde(untagged)]
    Unknown {
        data: Vec<RichMessagePart>,
    },
}

impl RichPrint {
    /// A utility method that returns a message of an unknown type that just
    /// contains the given unformatted [text].
    pub fn message(text: String) -> RichPrint {
        RichPrint::Unknown {
            data: vec![RichMessagePart::Text { text }],
        }
    }

    /// Returns the data field for any RichPrint.
    pub fn data(&self) -> &[RichMessagePart] {
        use RichPrint::*;
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

    /// Returns the mutable data field for any RichPrint.
    pub fn data_mut(&mut self) -> &mut [RichMessagePart] {
        use RichPrint::*;
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

    /// Fills in [RichMessagePart::PlayerId.name],
    /// [RichMessagePart::ItemId.name], and [RichMessagePart::LocationId.name]
    /// for all parts of this print if possible.
    ///
    /// In order to fill in `ItemId` and `LocationId` names for other games,
    /// those games' data packages must have been requested as part of the data
    /// package.
    pub fn add_names<S>(&mut self, connected: &Connected<S>, data_package: &DataPackageObject) {
        for part in self.data_mut() {
            part.add_name(connected, data_package);
        }
    }
}

impl fmt::Display for RichPrint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for part in self.data() {
            part.fmt(f)?;
        }
        Ok(())
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RichMessagePart {
    PlayerId {
        /// The slot ID of the player this part refers to.
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: i64,

        /// This field is neither set nor read by the server. It's filled in
        /// based on [id] when [add_name] is called.
        #[serde(skip)]
        name: Option<Arc<String>>,
    },
    PlayerName {
        text: String,
    },
    ItemId {
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: i64,
        flags: NetworkItemFlags,
        player: i64,

        /// This field is neither set nor read by the server. It's filled in
        /// based on [id] and [player] when [add_name] is called.
        #[serde(skip)]
        name: Option<Arc<String>>,
    },
    ItemName {
        text: String,
        flags: NetworkItemFlags,
        player: i64,
    },
    LocationId {
        #[serde(rename = "text")]
        #[serde_as(as = "DisplayFromStr")]
        id: i64,
        player: i64,

        /// This field is neither set nor read by the server. It's filled in
        /// based on [id] and [player] when [add_name] is called.
        #[serde(skip)]
        name: Option<Arc<String>>,
    },
    LocationName {
        text: String,
        player: i64,
    },
    EntranceName {
        text: String,
    },
    Color {
        text: String,
        color: RichMessageColor,
    },
    #[serde(untagged)]
    Text {
        text: String,
    },
}

impl RichMessagePart {
    /// Fills in [RichMessagePart::PlayerId.name],
    /// [RichMessagePart::ItemId.name], and [RichMessagePart::LocationId.name]
    /// if possible.
    ///
    /// In order to fill in `ItemId` and `LocationId` names for other games,
    /// those games' data packages must have been requested as part of the data
    /// package.
    ///
    /// See also [RichPrint.add_names].
    pub fn add_name<S>(&mut self, connected: &Connected<S>, data_package: &DataPackageObject) {
        use RichMessagePart::*;
        match self {
            PlayerId { id, name } => {
                if let Some(player) = connected.players.iter().find(|p| p.slot == *id) {
                    name.replace(Arc::new(player.alias.clone()));
                }
            }
            ItemId {
                id, player, name, ..
            } => {
                if let Some(item) = connected
                    .slot_info
                    .get(player)
                    .and_then(|s| data_package.games.get(&s.game))
                    .and_then(|g| g.item_id_to_name().get(id))
                {
                    name.replace(item.clone());
                }
            }
            LocationId {
                id, player, name, ..
            } => {
                if let Some(item) = connected
                    .slot_info
                    .get(player)
                    .and_then(|s| data_package.games.get(&s.game))
                    .and_then(|g| g.location_id_to_name().get(id))
                {
                    name.replace(item.clone());
                }
            }
            _ => {}
        }
    }
}

impl fmt::Display for RichMessagePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use RichMessagePart::*;
        match self {
            PlayerId {
                name: Some(text), ..
            }
            | ItemId {
                name: Some(text), ..
            }
            | LocationId {
                name: Some(text), ..
            } => text.fmt(f),
            PlayerName { text, .. }
            | ItemName { text, .. }
            | LocationName { text, .. }
            | EntranceName { text, .. }
            | Color { text, .. }
            | Text { text, .. } => text.fmt(f),
            PlayerId { id, .. } => write!(f, "<player {}>", id),
            ItemId { id, player, .. } => write!(f, "<item {}:{}>", player, id),
            LocationId { id, player, .. } => write!(f, "<loc {}:{}>", player, id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RichMessageColor {
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
