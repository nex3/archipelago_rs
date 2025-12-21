//! A Rust library that for the [Archipelago game randomizer](archipelago.gg), that implements the [Archipelago network protocol](https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md)
//! Check out ArchipelagoClient for the meat of the logic

use std::{fmt::Debug, iter::FusedIterator};

mod client;
mod connection;
mod connection_options;
mod data;
mod error;
mod event;
pub mod protocol;

pub use client::*;
pub use connection::*;
pub use connection_options::*;
pub use data::*;
pub use error::*;
pub use event::*;

/// The trait of iterators returned by this package. This allows us to keep
/// iterator implementations opaque while still guaranteeing that they implement
/// various useful traits.
pub trait Iter<T>: Iterator<Item = T> + ExactSizeIterator + FusedIterator + Clone + Debug {}

impl<I, T> Iter<T> for I where
    I: Iterator<Item = T> + ExactSizeIterator + FusedIterator + Clone + Debug
{
}
