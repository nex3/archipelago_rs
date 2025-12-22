use std::{fmt::Debug, iter::FusedIterator};

/// The trait of iterators returned by this package that don't have a size known
/// ahead of time. This allows us to keep iterator implementations opaque while
/// still guaranteeing that they implement various useful traits.
pub trait UnsizedIter<T>: Iterator<Item = T> + FusedIterator + Clone + Debug {}

impl<I, T> UnsizedIter<T> for I where I: Iterator<Item = T> + FusedIterator + Clone + Debug {}

/// The trait of most iterators returned by this package. This allows us to keep
/// iterator implementations opaque while still guaranteeing that they implement
/// various useful traits.
pub trait Iter<T>: UnsizedIter<T> + ExactSizeIterator {}

impl<I, T> Iter<T> for I where I: UnsizedIter<T> + ExactSizeIterator {}
