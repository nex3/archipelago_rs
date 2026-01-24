use std::path::{Path, PathBuf};
use std::{fmt::Debug, iter::FusedIterator};

use smol::{fs, io};

mod signed_duration;

pub(crate) use signed_duration::*;

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

/// Writes a file atomically by writing to an adjacent file with additional
/// random characters in its name and then moving that to the desired location.
pub(crate) async fn write_file_atomic(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), io::Error> {
    let path = path.as_ref();
    let mut tmp_path = PathBuf::from(path);
    tmp_path.pop();

    let mut tmp_basename = path.file_stem().unwrap_or_else(|| {
        panic!(
            "write_file_atomic path must have a basename, was {:?}",
            path
        )
    }).to_owned();
    tmp_basename.push(format!("-tmp-{:0}", rand::random::<u32>()));
    if let Some(ext) = path.extension() {
        tmp_basename.push(ext);
    }

    fs::write(&tmp_path, contents).await?;
    fs::rename(tmp_path, path).await?;

    Ok(())
}
