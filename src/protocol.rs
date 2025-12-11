use serde_with::DisplayFromStr;
use std::collections::HashMap;
use std::fmt::Display;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::serde_as;
use std::fmt;

mod bounce;

pub use bounce::*;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd")]
pub enum ClientMessage {
    Connect(Connect),
    ConnectUpdate(ConnectUpdate),
    Sync,
    LocationChecks(LocationChecks),
    LocationScouts(LocationScouts),
    UpdateHint(UpdateHint),
    StatusUpdate(StatusUpdate),
    Say(Say),
    GetDataPackage(GetDataPackage),
    Bounce(Bounce),
    Get(Get),
    Set(Set),
    SetNotify(SetNotify),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd")]
pub enum ServerMessage<S> {
    RoomInfo(RoomInfo),
    ConnectionRefused(ConnectionRefused),
    Connected(Connected<S>),
    ReceivedItems(ReceivedItems),
    LocationInfo(LocationInfo),
    RoomUpdate(RoomUpdate),
    Print(Print),
    PrintJSON(PrintJSON),
    DataPackage(DataPackage),
    Bounced(Bounced),
    InvalidPacket(InvalidPacket),
    Retrieved(Retrieved),
    SetReply(SetReply),
}

impl<S> ServerMessage<S> {
    /// Returns the name of this message's type.
    pub fn type_name(&self) -> &'static str {
        use ServerMessage::*;
        match self {
            RoomInfo(_) => "RoomInfo",
            ConnectionRefused(_) => "ConnectionRefused",
            Connected(_) => "Connected",
            ReceivedItems(_) => "ReceivedItems",
            LocationInfo(_) => "LocationInfo",
            RoomUpdate(_) => "RoomUpdate",
            Print(_) => "Print",
            PrintJSON(_) => "PrintJSON",
            DataPackage(_) => "DataPackage",
            Bounced(_) => "Bounced",
            InvalidPacket(_) => "InvalidPacket",
            Retrieved(_) => "Retrieved",
            SetReply(_) => "SetReply",
        }
    }
}

#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u16)]
pub enum Permission {
    Disabled = 0,
    Enabled = 1,
    Goal = 2,
    Auto = 6,
    AutoEnabled = 7,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkVersion {
    pub major: i64,
    pub minor: i64,
    pub build: i64,
    pub class: String,
}

impl Display for NetworkVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            format!("{}.{}.{}", self.major, self.minor, self.build)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPlayer {
    pub team: i64,
    pub slot: i64,
    pub alias: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkItem {
    pub item: i64,
    pub location: i64,
    pub player: i64,
    pub flags: NetworkItemFlags,
}

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(from = "u8")]
    #[serde(into = "u8")]
    pub struct NetworkItemFlags: u8 {
        /// The item can unlock logical advancement.
        const PROGRESSION = 0b001;

        /// The item is especially useful.
        const USEFUL = 0b010;

        /// The item is a trap.
        const TRAP = 0b100;
    }
}

impl From<u8> for NetworkItemFlags {
    fn from(value: u8) -> NetworkItemFlags {
        NetworkItemFlags::from_bits_retain(value)
    }
}

impl Into<u8> for NetworkItemFlags {
    fn into(self) -> u8 {
        self.bits()
    }
}

#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u16)]
pub enum SlotType {
    Spectator = 0,
    Player = 1,
    Group = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSlot {
    pub name: String,
    pub game: String,
    pub r#type: SlotType,
    pub group_members: Vec<i64>,
}

pub fn network_version() -> NetworkVersion {
    NetworkVersion {
        major: 0,
        minor: 6,
        build: 0,
        class: "Version".to_string(),
    }
}

// REQUESTS

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connect {
    pub password: Option<String>,
    pub game: String,
    pub name: String,
    pub uuid: String,
    pub version: NetworkVersion,
    pub items_handling: u8,
    pub tags: Vec<String>,
    #[serde(rename = "slot_data")]
    pub slot_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectUpdate {
    pub items_handling: u8,
    pub tags: Vec<String>,
}

bitflags! {
    #[repr(transparent)]
    pub struct ItemsHandlingFlags: u8 {
        /// Items are sent from other worlds.
        const OTHER_WORLDS = 0b001;

        /// Items are sent from your own world. Setting this automatically sets
        /// [OTHER_WORLDS] as well.
        const OWN_WORLD = 0b011;

        /// Items are sent from your starting inventory. Setting this
        /// automatically sets [OTHER_WORLDS] as well.
        const STARTING_INVENTORY = 0b101;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationChecks {
    pub locations: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationScouts {
    pub locations: Vec<i64>,
    pub create_as_hint: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHint {
    pub player: i64,
    pub location: i64,
    pub status: HintStatus,
}

#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u16)]
pub enum HintStatus {
    HintFound = 0,
    HintUnspecified = 1,
    HintNoPriority = 10,
    HintAvoid = 20,
    HintPriority = 30,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusUpdate {
    pub status: ClientStatus,
}

#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u16)]
pub enum ClientStatus {
    ClientUnknown = 0,
    ClientReady = 10,
    ClientPlaying = 20,
    ClientGoal = 30,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Say {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDataPackage {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub games: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Get {
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Set {
    pub key: String,
    pub default: Value,
    pub want_reply: bool,
    pub operations: Vec<DataStorageOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", content = "value", rename_all = "snake_case")]
pub enum DataStorageOperation {
    Replace(Value),
    Default,
    Add(Value),
    Mul(Value),
    Pow(Value),
    Mod(Value),
    Floor,
    Ceil,
    Max(Value),
    Min(Value),
    And(Value),
    Or(Value),
    Xor(Value),
    LeftShift(Value),
    RightShift(Value),
    Remove(Value),
    Pop(Value),
    Update(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetNotify {
    pub keys: Vec<String>,
}

// RESPONSES

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub version: NetworkVersion,
    pub generator_version: NetworkVersion,
    pub tags: Vec<String>,
    #[serde(rename = "password")]
    pub password_required: bool,
    pub permissions: HashMap<String, Permission>,
    pub hint_cost: i64,
    pub location_check_points: i64,
    pub games: Vec<String>,
    #[serde(default)]
    pub datapackage_versions: HashMap<String, i64>,
    #[serde(default)]
    pub datapackage_checksums: HashMap<String, String>,
    pub seed_name: String,
    pub time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRefused {
    #[serde(default)]
    pub errors: Vec<String>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct Connected<S> {
    pub team: i64,
    pub slot: i64,
    pub players: Vec<NetworkPlayer>,
    pub missing_locations: Vec<i64>,
    pub checked_locations: Vec<i64>,
    pub slot_data: S,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    pub slot_info: HashMap<i64, NetworkSlot>,
    pub hint_points: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedItems {
    pub index: i64,
    pub items: Vec<NetworkItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationInfo {
    pub locations: Vec<NetworkItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomUpdate {
    // Copied from RoomInfo
    pub version: Option<NetworkVersion>,
    pub tags: Option<Vec<String>>,
    #[serde(rename = "password")]
    pub password_required: Option<bool>,
    pub permissions: Option<HashMap<String, Permission>>,
    pub hint_cost: Option<i64>,
    pub location_check_points: Option<i64>,
    pub games: Option<Vec<String>>,
    pub datapackage_versions: Option<HashMap<String, i64>>,
    pub datapackage_checksums: Option<HashMap<String, String>>,
    pub seed_name: Option<String>,
    pub time: Option<f64>,
    // Exclusive to RoomUpdate
    pub hint_points: Option<i64>,
    pub players: Option<Vec<NetworkPlayer>>,
    pub checked_locations: Option<Vec<i64>>,
    pub missing_locations: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Print {
    pub text: String,
}

// Not a very elegant way to handle this. See
// https://github.com/serde-rs/serde/issues/1799.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PrintJSON {
    ItemSend {
        data: Vec<JSONMessagePart>,
        receiving: i64,
        item: NetworkItem,
    },
    ItemCheat {
        data: Vec<JSONMessagePart>,
        receiving: i64,
        item: NetworkItem,
        team: i64,
    },
    Hint {
        data: Vec<JSONMessagePart>,
        receiving: i64,
        item: NetworkItem,
        found: bool,
    },
    Join {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
        tags: Vec<String>,
    },
    Part {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
    },
    Chat {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
        message: String,
    },
    ServerChat {
        data: Vec<JSONMessagePart>,
        message: String,
    },
    Tutorial {
        data: Vec<JSONMessagePart>,
    },
    TagsChanged {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
        tags: Vec<String>,
    },
    CommandResult {
        data: Vec<JSONMessagePart>,
    },
    AdminCommandResult {
        data: Vec<JSONMessagePart>,
    },
    Goal {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
    },
    Release {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
    },
    Collect {
        data: Vec<JSONMessagePart>,
        team: i64,
        slot: i64,
    },
    Countdown {
        data: Vec<JSONMessagePart>,
        countdown: i64,
    },
    #[serde(untagged)]
    Unknown {
        data: Vec<JSONMessagePart>,
    },
}

impl PrintJSON {
    /// A utility method that returns a message of an unknown type that just
    /// contains the given unformatted [text].
    pub fn message(text: String) -> PrintJSON {
        PrintJSON::Unknown {
            data: vec![JSONMessagePart::Text { text }],
        }
    }

    /// Returns the data field for any JSONMessagePart.
    pub fn data(&self) -> &Vec<JSONMessagePart> {
        use PrintJSON::*;
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

impl fmt::Display for PrintJSON {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for part in self.data() {
            f.write_str(&part.text().as_str())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JSONMessagePart {
    PlayerId {
        text: String,
        player: i64,
    },
    PlayerName {
        text: String,
    },
    ItemId {
        text: String,
        flags: NetworkItemFlags,
        player: i64,
    },
    ItemName {
        text: String,
        flags: NetworkItemFlags,
        player: i64,
    },
    LocationId {
        text: String,
        player: i64,
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
        color: JSONColor,
    },
    #[serde(untagged)]
    Text {
        text: String,
    },
}

impl JSONMessagePart {
    /// Returns the text field for any JSONMessagePart.
    pub fn text(&self) -> &String {
        use JSONMessagePart::*;
        match self {
            PlayerId { text, .. } => text,
            PlayerName { text, .. } => text,
            ItemId { text, .. } => text,
            ItemName { text, .. } => text,
            LocationId { text, .. } => text,
            LocationName { text, .. } => text,
            EntranceName { text, .. } => text,
            Color { text, .. } => text,
            Text { text, .. } => text,
        }
    }
}

impl fmt::Display for JSONMessagePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.write_str(&self.text().as_str())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JSONColor {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPackage {
    pub data: DataPackageObject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPackageObject {
    pub games: HashMap<String, GameData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameData {
    pub item_name_to_id: HashMap<String, i64>,
    pub location_name_to_id: HashMap<String, i64>,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidPacket {
    pub r#type: String,
    pub original_cmd: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Retrieved {
    pub keys: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetReply {
    pub key: String,
    pub value: Value,
    pub original_value: Option<Value>, // Won't be there if key is prefixed with _read
}
