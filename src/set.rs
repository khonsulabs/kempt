use core::fmt::{self, Debug};

use crate::map::{self, Field, OwnedOrRef};
use crate::{Map, Sort};

/// An iterator over the vakyes in a [`Set`].
pub type Iter<'a, T> = map::Keys<'a, T, ()>;
/// An iterator that converts a [`Set`] into its owned values.
pub type IntoIter<T> = map::IntoKeys<T, ()>;

/// An ordered collection of unique `T`s.
///
/// This data type only allows each unique value to be stored once.
///
/// ```rust
/// use kempt::Set;
///
/// let mut set = Set::new();
/// set.insert(1);
/// assert!(!set.insert(1));
/// assert_eq!(set.len(), 1);
/// ```
///
/// The values in the collection are automatically sorted using `T`'s [`Ord`]
/// implementation.
///
/// ```rust
/// use kempt::Set;
///
/// let mut set = Set::new();
/// set.insert(1);
/// set.insert(3);
/// set.insert(2);
/// assert_eq!(set.member(0), Some(&1));
/// assert_eq!(set.member(1), Some(&2));
/// assert_eq!(set.member(2), Some(&3));
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Set<T>(Map<T, ()>)
where
    T: Sort<T>;

impl<T> Default for Set<T>
where
    T: Sort<T>,
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Set<T>
where
    T: Sort<T>,
{
    /// Returns an empty set.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self(Map::new())
    }

    /// Returns an empty set with enough allocated memory to store `capacity`
    /// values without reallocating.
    #[must_use]
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Map::with_capacity(capacity))
    }

    /// Returns the current capacity this map can hold before it must
    /// reallocate.
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Inserts or replaces `value` in the set, returning `true` if the
    /// collection is modified. If a previously contained value returns
    /// [`Ordering::Equal`](core::cmp::Ordering::Equal) from [`Ord::cmp`], the
    /// collection will not be modified and `false` will be returned.
    #[inline]
    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert_with(value, || ()).is_none()
    }

    /// Inserts or replaces `value` in the set. If a previously contained value
    /// returns [`Ordering::Equal`](core::cmp::Ordering::Equal) from
    /// [`Ord::cmp`], the new value will overwrite the stored value and it will
    /// be returned.
    #[inline]
    pub fn replace(&mut self, value: T) -> Option<T> {
        self.0.insert(value, ()).map(|field| field.into_parts().0)
    }

    /// Returns true if the set contains a matching `value`.
    #[inline]
    pub fn contains<SearchFor>(&self, value: &SearchFor) -> bool
    where
        T: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.0.contains(value)
    }

    /// Returns the contained value that matches `value`.
    #[inline]
    pub fn get<SearchFor>(&self, value: &SearchFor) -> Option<&T>
    where
        T: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.0.get_field(value).map(Field::key)
    }

    /// Removes a value from the set, returning the value if it was removed.
    #[inline]
    pub fn remove<SearchFor>(&mut self, value: &SearchFor) -> Option<T>
    where
        T: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.0.remove(value).map(|field| field.into_parts().0)
    }

    /// Returns the member at `index` inside of this ordered set. Returns `None`
    /// if `index` is greater than or equal to the set's length.
    #[inline]
    pub fn member(&self, index: usize) -> Option<&T> {
        self.0.field(index).map(Field::key)
    }

    /// Removes the member at `index`.
    ///
    /// # Panics
    ///
    /// A panic will occur if `index` is greater than or equal to the set's
    /// length.
    #[inline]
    pub fn remove_member(&mut self, index: usize) -> T {
        self.0.remove_by_index(index).into_key()
    }

    /// Returns the number of members in this set.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no members in this set.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the members in this set.
    #[must_use]
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.into_iter()
    }

    /// Returns an iterator that yields a single reference to all members found
    /// in either `self` or `other`.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    #[inline]
    pub fn union<'a>(&'a self, other: &'a Set<T>) -> Union<'a, T> {
        Union(self.0.union(&other.0))
    }

    /// Returns an iterator that yields a single reference to all members found
    /// in both `self` and `other`.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    #[inline]
    pub fn intersection<'a>(&'a self, other: &'a Set<T>) -> Intersection<'a, T> {
        Intersection(self.0.intersection(&other.0))
    }

    /// Returns an iterator that yields a single reference to all members found
    /// in `self` but not `other`.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    #[inline]
    pub fn difference<'a>(&'a self, other: &'a Set<T>) -> Difference<'a, T> {
        Difference(self.0.difference(&other.0))
    }

    /// Returns an iterator over the contents of this set. After the iterator is
    /// dropped, this set will be empty.
    #[inline]
    pub fn drain(&mut self) -> Drain<'_, T> {
        Drain(self.0.drain())
    }

    /// Clears the contents of this collection.
    ///
    /// This does not return any allocated memory to the OS.
    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Resizes this collection to fit its contents exactly.
    ///
    /// This function will reallocate its internal storage to fit the contents
    /// of this collection's current size. If the allocation is already the
    /// correct size, this is a no-op.
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    /// Resizes this collection to be able to hold `min_capacity`.
    ///
    /// This function will reallocate its internal storage to fit the contents
    /// of this collection's current size. If the allocation is already the
    /// correct size, this is a no-op.
    ///
    /// If the length of this collection is larger than `min_capacity`, this
    /// function will behave identically to
    /// [`shrink_to_fit()`](Self::shrink_to_fit).
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.0.shrink_to(min_capacity);
    }
}

impl<T> Debug for Set<T>
where
    T: Sort<T> + Debug,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_set();
        for member in self {
            s.entry(member);
        }
        s.finish()
    }
}

impl<'a, T> IntoIterator for &'a Set<T>
where
    T: Sort<T>,
{
    type IntoIter = Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.keys()
    }
}

impl<T> FromIterator<T> for Set<T>
where
    T: Sort<T>,
{
    #[inline]
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().map(|t| (t, ())).collect())
    }
}

/// An iterator that yields a single reference to all members found in either
/// [`Set`] being unioned.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Union<'a, T>(map::Union<'a, T, ()>)
where
    T: Sort<T>;

impl<'a, T> Iterator for Union<'a, T>
where
    T: Sort<T>,
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|unioned| unioned.map_both(|_, (), ()| OwnedOrRef::Owned(())).key)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An iterator that yields a single reference to all members found in both
/// [`Set`]s being intersected.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Intersection<'a, T>(map::Intersection<'a, T, ()>)
where
    T: Sort<T>;

impl<'a, T> Iterator for Intersection<'a, T>
where
    T: Sort<T>,
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, (), ())| k)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An iterator that yields a single reference to all members found in one
/// [`Set`], but not another.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Difference<'a, T>(map::Difference<'a, T, ()>)
where
    T: Sort<T>;

impl<'a, T> Iterator for Difference<'a, T>
where
    T: Sort<T>,
{
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, ())| k)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An iterator that drains the contents of a [`Set`].
///
/// When this is dropped, the remaining contents are drained.
pub struct Drain<'a, T>(map::Drain<'a, T, ()>);

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(map::Field::into_key)
    }
}

#[test]
fn basics() {
    let mut set = Set::default();
    assert!(set.is_empty());
    assert!(set.insert(1));
    assert!(set.contains(&1));
    assert_eq!(set.replace(1), Some(1));
    assert!(set.insert(0));

    assert_eq!(set.member(0), Some(&0));
    assert_eq!(set.member(1), Some(&1));

    assert_eq!(set.len(), 2);
    assert_eq!(set.remove(&0), Some(0));
    assert_eq!(set.len(), 1);
    assert_eq!(set.remove(&1), Some(1));
    assert_eq!(set.len(), 0);
}

#[test]
fn union() {
    use alloc::vec::Vec;
    let a = [1, 3, 5].into_iter().collect::<Set<u8>>();
    let b = [2, 3, 4].into_iter().collect::<Set<u8>>();
    assert_eq!(a.union(&b).copied().collect::<Vec<_>>(), [1, 2, 3, 4, 5]);

    let b = [2, 3, 6].into_iter().collect::<Set<u8>>();
    assert_eq!(a.union(&b).copied().collect::<Vec<_>>(), [1, 2, 3, 5, 6]);
}

#[test]
fn intersection() {
    use alloc::vec::Vec;
    let a = [1, 3, 5].into_iter().collect::<Set<u8>>();
    let b = [2, 3, 4].into_iter().collect::<Set<u8>>();
    assert_eq!(a.intersection(&b).copied().collect::<Vec<_>>(), [3]);

    let b = [2, 3, 6].into_iter().collect::<Set<u8>>();
    assert_eq!(a.intersection(&b).copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn difference() {
    use alloc::vec::Vec;
    let a = [1, 3, 5].into_iter().collect::<Set<u8>>();
    let b = [2, 3, 4].into_iter().collect::<Set<u8>>();
    assert_eq!(a.difference(&b).copied().collect::<Vec<_>>(), [1, 5]);

    let b = [2, 5, 6].into_iter().collect::<Set<u8>>();
    assert_eq!(a.difference(&b).copied().collect::<Vec<_>>(), [1, 3]);
}

#[test]
fn lookup() {
    let mut set = Set::with_capacity(1);
    let key = alloc::string::String::from("hello");
    let key_ptr = key.as_ptr();
    set.insert(key);
    assert_eq!(set.get("hello").unwrap().as_ptr(), key_ptr);
}

#[test]
fn iteration() {
    use alloc::vec::Vec;
    let mut set = Set::with_capacity(3);
    set.insert(1);
    set.insert(3);
    set.insert(2);
    assert_eq!(set.iter().copied().collect::<Vec<_>>(), &[1, 2, 3]);
}
