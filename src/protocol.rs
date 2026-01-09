use std::collections::HashMap;
use std::{fmt::Display, time::SystemTime};

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{DisplayFromStr, TimestampSeconds, serde_as};
use ustr::{Ustr, UstrMap, UstrSet};

mod bounce;
mod print;

pub(crate) use bounce::*;
pub use print::TextColor;
pub(crate) use print::*;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "cmd")]
pub(crate) enum ClientMessage {
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
pub(crate) enum ServerMessage<S> {
    RoomInfo(RoomInfo),
    ConnectionRefused(ConnectionRefused),
    Connected(Connected<S>),
    ReceivedItems(ReceivedItems),
    LocationInfo(LocationInfo),
    RoomUpdate(RoomUpdate),
    #[serde(rename = "Print")]
    PlainPrint(PlainPrint),
    #[serde(rename = "PrintJSON")]
    RawPrint(NetworkPrint),
    DataPackage(DataPackage),
    Bounced(Bounced),
    InvalidPacket(InvalidPacket),
    Retrieved(Retrieved),
    SetReply(SetReply),
}

impl<S> ServerMessage<S> {
    /// Returns the name of this message's type.
    pub(crate) fn type_name(&self) -> &'static str {
        use ServerMessage::*;
        match self {
            RoomInfo(_) => "RoomInfo",
            ConnectionRefused(_) => "ConnectionRefused",
            Connected(_) => "Connected",
            ReceivedItems(_) => "ReceivedItems",
            LocationInfo(_) => "LocationInfo",
            RoomUpdate(_) => "RoomUpdate",
            PlainPrint(_) => "Print",
            RawPrint(_) => "PrintJSON",
            DataPackage(_) => "DataPackage",
            Bounced(_) => "Bounced",
            InvalidPacket(_) => "InvalidPacket",
            Retrieved(_) => "Retrieved",
            SetReply(_) => "SetReply",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize_repr)]
#[repr(u8)]
/// Permissions for when certain actions (such as releasing all checks) may be
/// performed.
pub enum Permission {
    /// This action may never be performed.
    Disabled = 0,

    /// This action may be manually performed at any time.
    Enabled = 1,

    /// This action may be manually performed by a player after they have
    /// reached their goal.
    Goal = 2,

    /// This action is automatically performed after the player has reached
    /// their goal. This is only possible for release and collect.
    Auto = 6,

    /// This action is automatically performed after the player has reached
    /// their goal *and* may be manually performed at any time.
    AutoEnabled = 7,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkVersion {
    pub(crate) major: u16,
    pub(crate) minor: u16,
    pub(crate) build: u16,
    pub(crate) class: String,
}

impl Display for NetworkVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.build)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NetworkPlayer {
    pub(crate) team: u32,
    pub(crate) slot: u32,
    pub(crate) alias: String,
    pub(crate) name: Ustr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkItem {
    pub(crate) item: i64,
    pub(crate) location: i64,
    pub(crate) player: u32,
    pub(crate) flags: NetworkItemFlags,
}

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(from = "u8")]
    #[serde(into = "u8")]
    pub(crate) struct NetworkItemFlags: u8 {
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

impl From<NetworkItemFlags> for u8 {
    fn from(value: NetworkItemFlags) -> Self {
        value.bits()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub(crate) enum SlotType {
    // We ignore these because they aren't currently sent by the server.
    Spectator = 0,
    Player = 1,
    Group = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NetworkSlot {
    pub(crate) name: Ustr,
    pub(crate) game: Ustr,
    pub(crate) r#type: SlotType,
    pub(crate) group_members: Vec<u32>,
}

// REQUESTS

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Connect {
    pub(crate) password: Option<String>,
    pub(crate) game: Ustr,
    pub(crate) name: Ustr,
    pub(crate) uuid: String,
    pub(crate) version: NetworkVersion,
    pub(crate) items_handling: ItemsHandlingFlags,
    pub(crate) tags: UstrSet,
    pub(crate) slot_data: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConnectUpdate {
    pub(crate) items_handling: Option<ItemsHandlingFlags>,
    pub(crate) tags: Option<Vec<Ustr>>,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Serialize)]
    #[serde(into = "u8")]
    pub(crate) struct ItemsHandlingFlags: u8 {
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

impl From<ItemsHandlingFlags> for u8 {
    fn from(value: ItemsHandlingFlags) -> Self {
        value.bits()
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocationChecks {
    pub(crate) locations: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocationScouts {
    pub(crate) locations: Vec<i64>,
    pub(crate) create_as_hint: CreateAsHint,
}

/// Options for whether and how locations scouted with [Client.scout_locations]
/// should be broadcast as player-visible hints.
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum CreateAsHint {
    /// Don't broadcast locations as hints. Scouted locations will only be
    /// visible to the client code, not to the player.
    No = 0,

    /// Broadcast all scouted locations as hints.
    All = 1,

    /// Broadcast only locations that haven't already been hinted as hints. The
    /// return value for [scout_locations](crate::Client::scout_locations) will
    /// still contain *all* locations.
    New = 2,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateHint {
    pub(crate) player: u32,
    pub(crate) location: i64,
    pub(crate) status: HintStatus,
}

#[derive(Debug, Clone, Serialize_repr)]
#[repr(u8)]
pub enum HintStatus {
    HintUnspecified = 0,
    HintNoPriority = 10,
    HintAvoid = 20,
    HintPriority = 30,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusUpdate {
    pub(crate) status: ClientStatus,
}

/// Possible states for the client.
#[derive(Debug, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum ClientStatus {
    Unknown = 0,

    /// One or more clients are connected to the server. This is automatically
    /// set when the connection is first established.
    Connected = 5,
    Ready = 10,
    Playing = 20,

    /// This player has achieved their goal.
    Goal = 30,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Say {
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GetDataPackage {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) games: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Get {
    pub(crate) keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Set {
    pub(crate) key: String,
    pub(crate) default: Value,
    pub(crate) want_reply: bool,
    pub(crate) operations: Vec<DataStorageOperation>,
}

/// Operations that can be applied to keys in the Archipelago server's data
/// store.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "operation", content = "value", rename_all = "snake_case")]
pub enum DataStorageOperation {
    /// Replace the value entirely with a new value.
    Replace(Value),

    /// Set the value to the default value specified in
    /// [Client::change](crate::Client::change).
    Default,

    /// Adds the given number to the current value.
    Add(f64),

    /// Appends the given values to the end of an array.
    #[serde(rename = "add")]
    Appends(Vec<Value>),

    /// Multiplies the current value by the given number.
    #[serde(rename = "mul")]
    Multiply(f64),

    /// Exponentiates the current value to the given power.
    #[serde(rename = "pow")]
    Exponentiate(f64),

    /// Sets the current value to itself modulo the given number.
    Mod(f64),

    /// Rounds the current value down to the next integer.
    Floor,

    /// Rounds the current value up to the next integer.
    Ceil,

    /// Sets the current value to the given number if it's greater than the
    /// existing value.
    Max(i64),

    /// Sets the current value to the given number if it's less than the
    /// existing value.
    Min(i64),

    /// Sets the current value to the bitwise AND of it and the given number.
    And(i64),

    /// Sets the current value to the bitwise OR of it and the given number.
    Or(i64),

    /// Sets the current value to the bitwise XOR of it and the given number.
    Xor(i64),

    /// Shifts the current value left by the given number of bits.
    LeftShift(u8),

    /// Shifts the current value right by the given number of bits.
    RightShift(u8),

    /// If the current value is an array, removes the first instance of the
    /// given value from it.
    Remove(Value),

    /// If the current value is an array, removes the element at the given
    /// index.
    #[serde(rename = "pop")]
    RemoveIndex(i64),

    /// If the current value is a map, removes the element with the given key.
    #[serde(rename = "pop")]
    RemoveKey(String),

    /// If the current value is an array, adds all elements in the given array
    /// that aren't already present.
    #[serde(rename = "update")]
    Union(Vec<Value>),

    /// If the current value is a map, sets the given keys to their associated
    /// values.
    Update(HashMap<String, Value>),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SetNotify {
    pub(crate) keys: Vec<String>,
}

// RESPONSES

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RoomInfo {
    pub(crate) version: NetworkVersion,
    pub(crate) generator_version: NetworkVersion,
    pub(crate) tags: UstrSet,
    #[serde(rename = "password")]
    pub(crate) password_required: bool,
    pub(crate) permissions: PermissionMap,
    pub(crate) hint_cost: u8,
    pub(crate) location_check_points: u64,
    pub(crate) games: UstrSet,
    // TODO: Cache data packages
    #[serde(default)]
    #[serde(rename = "datapackage_checksums")]
    pub(crate) _datapackage_checksums: UstrMap<String>,
    pub(crate) seed_name: String,
    #[serde_as(as = "TimestampSeconds<f64>")]
    pub(crate) time: SystemTime,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PermissionMap {
    pub(crate) release: Permission,
    pub(crate) collect: Permission,
    pub(crate) remaining: Permission,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ConnectionRefused {
    #[serde(default)]
    pub(crate) errors: Vec<String>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Connected<S> {
    pub(crate) team: u32,
    pub(crate) slot: u32,
    pub(crate) players: Vec<NetworkPlayer>,
    pub(crate) missing_locations: Vec<i64>,
    pub(crate) checked_locations: Vec<i64>,
    pub(crate) slot_data: S,
    #[serde_as(as = "HashMap<DisplayFromStr, _>")]
    pub(crate) slot_info: HashMap<u32, NetworkSlot>,
    pub(crate) hint_points: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReceivedItems {
    pub(crate) index: usize,
    pub(crate) items: Vec<NetworkItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LocationInfo {
    pub(crate) locations: Vec<NetworkItem>,
}

// We only include fields here that might plausibly be changed during the
// lifetime of a single connection.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RoomUpdate {
    // Copied from RoomInfo
    pub(crate) tags: Option<UstrSet>,
    pub(crate) permissions: Option<PermissionMap>,
    pub(crate) hint_cost: Option<u8>,
    pub(crate) location_check_points: Option<u64>,
    // Copied from Connected
    pub(crate) hint_points: Option<u64>,
    pub(crate) players: Option<Vec<NetworkPlayer>>,
    pub(crate) checked_locations: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DataPackage {
    pub(crate) data: DataPackageObject,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DataPackageObject {
    pub(crate) games: HashMap<Ustr, GameData>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GameData {
    pub(crate) item_name_to_id: HashMap<Ustr, i64>,
    pub(crate) location_name_to_id: HashMap<Ustr, i64>,
    // TODO: Cache data packages
    #[serde(rename = "checksum")]
    pub(crate) _checksum: String,
}

// We could represent this as an enum of types, but there's no point when all we
// want to do is extract the error message anyway.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InvalidPacket {
    pub(crate) text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Retrieved {
    pub(crate) keys: HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetReply {
    pub(crate) key: String,
    pub(crate) value: Value,
    pub(crate) original_value: Option<Value>, // Won't be there if key is prefixed with _read
    // See https://github.com/ArchipelagoMW/Archipelago/issues/5829
    pub(crate) slot: Option<u32>,
}
