//! A Rust library that for the [Archipelago game randomizer](archipelago.gg), that implements the [Archipelago network protocol](https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md)
//! Check out ArchipelagoClient for the meat of the logic

mod client;
mod connection;
mod connection_options;
mod data;
mod error;
pub mod protocol;

pub use client::*;
pub use connection::*;
pub use connection_options::*;
pub use data::*;
pub use error::*;
