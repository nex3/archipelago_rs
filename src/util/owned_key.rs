use std::cmp::{Eq, PartialEq};
use std::hash::{Hash, Hasher};
use std::{ptr, sync::Arc};

/// An unsafe type to use as a hash key for values that own the key's data.
///
/// This lets callers avoid cloning the underlying data when it's always going
/// to be retained by the value anyway.
pub(crate) struct OwnedKey<T>(*const T)
where
    T: ?Sized;

impl<T> OwnedKey<T>
where
    T: ?Sized,
{
    /// Creates a [OwnedKey] from a reference.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that [reference] will outlive this key.
    pub(crate) unsafe fn from(reference: impl AsRef<T>) -> Self {
        OwnedKey(ptr::from_ref(reference.as_ref()))
    }

    /// Creates a [OwnedKey] from an [arc].
    ///
    /// ## Safety
    ///
    /// The caller must ensure that [arc] will outlive this key.
    pub(crate) unsafe fn from_arc<V: AsRef<T>>(arc: &Arc<V>) -> Self {
        OwnedKey(ptr::from_ref(arc.as_ref().as_ref()))
    }
}

impl<T> PartialEq for OwnedKey<T>
where
    T: PartialEq + ?Sized,
{
    fn eq(&self, other: &Self) -> bool {
        unsafe { *self.0 == *other.0 }
    }
}

impl<T> Eq for OwnedKey<T> where T: Eq + ?Sized {}

impl<T> Hash for OwnedKey<T>
where
    T: Hash + ?Sized,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { self.0.as_ref() }.hash(state)
    }
}

unsafe impl<T> Send for OwnedKey<T> where T: Sync + ?Sized {}
unsafe impl<T> Sync for OwnedKey<T> where T: Sync + ?Sized {}
