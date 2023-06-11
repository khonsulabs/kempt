#![doc = include_str!("../README.md")]
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

#[cfg(test)]
extern crate std;

extern crate alloc;

/// Types supporting the [`Map<Key, Value>`] collection type.
pub mod map;
/// Types supporting the [`Set<T>`] collection type.
pub mod set;

pub use map::Map;
pub use set::Set;

#[cfg(feature = "serde")]
mod serde;

#[cfg(test)]
mod tests;

/// Provides a comparison between `Self` and `Other`.
///
/// This function should only be implemented for types who guarantee that their
/// `PartialOrd<Other>` implementations are identical to their `PartialOrd`
/// implementations. For example, `Path` and `PathBuf` can be interchangeably
/// compared regardless of whether the left or right or both are a `Path` or
/// `PathBuf`.
///
/// Why not just use `PartialOrd<Other>`? Unfortunately, `PartialOrd<str>` is
/// [not implemented for
/// `String`](https://github.com/rust-lang/rust/issues/82990). This led to
/// issues implementing the [`Map::entry`] function when passing a `&str`
/// when the `Key` type was `String`.
///
/// This trait is automatically implemented for types that implement `Ord` and
/// `PartialOrd<Other>`, but it additionally provides implementations for
/// `String`/`str` and `Vec<T>`/`[T]`.
///
/// **In general, this trait should not need to be implemented.** Implement
/// `Ord` on your `Key` type, and if needed, implement `PartialOrd<Other>` for
/// your borrowed form.
pub trait Sort<Other = Self>
where
    Other: ?Sized,
{
    /// Compare `self` and `other`, returning the comparison result.
    ///
    /// This function should be implemented identically to
    /// `Ord::cmp`/`PartialOrd::partial_cmp`.
    fn compare(&self, other: &Other) -> Ordering;
}

impl Sort<str> for String {
    #[inline]
    fn compare(&self, b: &str) -> Ordering {
        self.as_str().cmp(b)
    }
}

impl<T> Sort<[T]> for Vec<T>
where
    T: Ord,
{
    #[inline]
    fn compare(&self, b: &[T]) -> Ordering {
        self.as_slice().cmp(b)
    }
}

impl<Key, SearchFor> Sort<SearchFor> for Key
where
    Key: Ord + PartialOrd<SearchFor>,
{
    #[inline]
    fn compare(&self, b: &SearchFor) -> Ordering {
        self.partial_cmp(b).expect("comparison failed")
    }
}
