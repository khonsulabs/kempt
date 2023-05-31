#![doc = include_str!("../README.md")]
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

#[cfg(test)]
extern crate std;

extern crate alloc;

#[cfg(feature = "serde")]
mod serde;

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::{self, Vec};
use core::alloc::Layout;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Debug};
use core::iter::FusedIterator;
use core::ops::{Deref, DerefMut};
use core::{mem, slice};

/// An ordered Key/Value map.
///
/// This type is similar to [`BTreeMap`](alloc::collections::BTreeMap), but
/// utilizes a simpler storage model. Additionally, it provides a more thorough
/// interface and has a [`merge_with()`](Self::merge_with) function.
///
/// This type is designed for collections with a limited number of keys. In
/// general, this collection excels when there are fewer entries, while
/// `HashMap` or `BTreeMap` will be better choices with larger numbers of
/// entries.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Map<Key, Value>
where
    Key: Sort<Key>,
{
    fields: Vec<Field<Key, Value>>,
}

impl<Key, Value> Default for Map<Key, Value>
where
    Key: Sort<Key>,
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Returns a heuristic guessing the size that should be allowed to be scanned
/// sequentially.
///
/// This uses the key and value types's layout to calculate based on multiple
/// cache line widths. Magic numbers are a code smell, but I'm not sure how else
/// to tune this heuristic based on the information available at compile time.
const fn scan_limit<Key, Value>() -> usize {
    let field_layout = Layout::new::<Field<Key, Value>>();
    let align = field_layout.align();
    let aligned = ((field_layout.size() + (align - 1)) / align) * align;
    if aligned == 0 {
        return 1;
    }

    let scan_limit = 128 / aligned;
    if scan_limit > 16 {
        16
    } else if scan_limit < 4 {
        4
    } else {
        scan_limit
    }
}

impl<Key, Value> Map<Key, Value>
where
    Key: Sort<Key>,
{
    const SCAN_LIMIT: usize = scan_limit::<Key, Value>();

    /// Returns an empty map.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Returns a map with enough memory allocated to store `capacity` elements
    /// without reallocation.
    #[must_use]
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fields: Vec::with_capacity(capacity),
        }
    }

    /// Inserts `key` and `value`. If an entry already existed for `key`, the
    /// value being overwritten is returned.
    #[inline]
    pub fn insert(&mut self, key: Key, value: Value) -> Option<Field<Key, Value>> {
        let field = Field::new(key, value);
        match self.find_key_mut(&field.key) {
            Ok(existing) => Some(mem::replace(existing, field)),
            Err(insert_at) => {
                self.fields.insert(insert_at, field);
                None
            }
        }
    }

    /// Returns true if this object contains `key`.
    #[inline]
    pub fn contains<Needle>(&self, key: &Needle) -> bool
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        self.find_key_index(key).is_ok()
    }

    /// Returns the value associated with `key`, if found.
    #[inline]
    pub fn get<Needle>(&self, key: &Needle) -> Option<&Value>
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        self.find_key(key).ok().map(|field| &field.value)
    }

    /// Returns the [`Field`] at the specified `index`, or None if the index is
    /// outside of the bounds of this collection.
    #[inline]
    #[must_use]
    pub fn field(&self, index: usize) -> Option<&Field<Key, Value>> {
        self.fields.get(index)
    }

    /// Returns a mutable reference to the [`Field`] at the specified `index`,
    /// or None if the index is outside of the bounds of this collection.
    #[inline]
    #[must_use]
    pub fn field_mut(&mut self, index: usize) -> Option<&mut Field<Key, Value>> {
        self.fields.get_mut(index)
    }

    /// Removes the value associated with `key`, if found.
    #[inline]
    pub fn remove<Needle>(&mut self, key: &Needle) -> Option<Field<Key, Value>>
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        let index = self.find_key_index(key).ok()?;
        Some(self.remove_by_index(index))
    }

    /// Removes the field at `index`.
    ///
    /// # Panics
    ///
    /// This function will panic if `index` is outside of the bounds of this
    /// collection.
    #[inline]
    pub fn remove_by_index(&mut self, index: usize) -> Field<Key, Value> {
        self.fields.remove(index)
    }

    /// Returns the number of fields in this object.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Returns true if this object has no fields.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Returns an [`Entry`] for the associated key.
    #[inline]
    pub fn entry<'key, Needle>(
        &mut self,
        key: impl Into<SearchKey<'key, Key, Needle>>,
    ) -> Entry<'_, 'key, Key, Value, Needle>
    where
        Key: Sort<Needle> + Borrow<Needle>,
        Needle: ToOwned<Owned = Key> + ?Sized + 'key,
    {
        let key = key.into();
        match self.find_key_index(key.as_ref()) {
            Ok(index) => Entry::Occupied(OccupiedEntry::new(self, index)),
            Err(insert_at) => Entry::Vacant(VacantEntry::new(self, key, insert_at)),
        }
    }

    #[inline]
    fn find_key<Needle>(&self, needle: &Needle) -> Result<&Field<Key, Value>, usize>
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        self.find_key_index(needle).map(|index| &self.fields[index])
    }

    #[inline]
    fn find_key_mut<Needle>(&mut self, needle: &Needle) -> Result<&mut Field<Key, Value>, usize>
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        self.find_key_index(needle)
            .map(|index| &mut self.fields[index])
    }

    #[inline]
    fn find_key_index<Needle>(&self, needle: &Needle) -> Result<usize, usize>
    where
        Key: Sort<Needle>,
        Needle: ?Sized,
    {
        // When the collection contains `Self::SCAN_LIMIT` or fewer elements,
        // there should be no jumps before we reach a sequential scan for the
        // key. When the collection is larger, we use a binary search to narrow
        // the search window until the window is 16 elements or less.
        let mut min = 0;
        let field_count = self.fields.len();
        let mut max = field_count;
        loop {
            let delta = max - min;
            if delta <= Self::SCAN_LIMIT {
                for (relative_index, field) in self.fields[min..max].iter().enumerate() {
                    let comparison = <Key as crate::Sort<Needle>>::compare(&field.key, needle);
                    return match comparison {
                        Ordering::Less => continue,
                        Ordering::Equal => Ok(min + relative_index),
                        Ordering::Greater => Err(min + relative_index),
                    };
                }

                return Err(max);
            }

            let midpoint = min + delta / 2;
            let comparison =
                <Key as crate::Sort<Needle>>::compare(&self.fields[midpoint].key, needle);

            match comparison {
                Ordering::Less => min = midpoint + 1,
                Ordering::Equal => return Ok(midpoint),
                Ordering::Greater => max = midpoint,
            }
        }
    }

    /// Returns an iterator over the fields in this object.
    #[must_use]
    #[inline]
    pub fn iter(&self) -> Iter<'_, Key, Value> {
        self.into_iter()
    }

    /// Returns an iterator over the fields in this object, with mutable access.
    #[must_use]
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, Key, Value> {
        IterMut(self.fields.iter_mut())
    }

    /// Returns an iterator over the values in this object.
    #[must_use]
    #[inline]
    pub fn values(&self) -> Values<'_, Key, Value> {
        Values(self.fields.iter())
    }

    /// Returns an iterator over the fields in this object, with mutable access.
    #[must_use]
    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<'_, Key, Value> {
        ValuesMut(self.fields.iter_mut())
    }

    /// Returns an iterator returning all of the values contained in this
    /// object.
    #[must_use]
    #[inline]
    pub fn into_values(self) -> IntoValues<Key, Value> {
        IntoValues(self.fields.into_iter())
    }

    /// Merges the fields from `self` and `other` into a new object, returning
    /// the updated object.
    ///
    /// * If a field is contained in `other` but not contained in `self`,
    ///   `filter()` is called. If `filter()` returns a value, the returned
    ///   value is inserted into the new object using the original key.
    /// * If a field is contained in both `other` and `self`, `merge()` is
    ///   called with mutable access to a clone of the value from `self` and a
    ///   reference to the value from `other`. The `merge()` function is
    ///   responsible for updating the value if needed to complete the merge.
    ///   The merged value is inserted into the returned object.
    /// * If a field is contained in `self` but not in `other`, it is always
    ///   cloned.
    #[inline]
    #[must_use]
    pub fn merged_with(
        mut self,
        other: &Self,
        filter: impl FnMut(&Key, &Value) -> Option<Value>,
        merge: impl FnMut(&Key, &mut Value, &Value),
    ) -> Self
    where
        Key: Clone,
        Value: Clone,
    {
        self.merge_with(other, filter, merge);
        self
    }

    /// Merges the fields from `other` into `self`.
    ///
    /// * If a field is contained in `other` but not contained in `self`,
    ///   `filter()` is called. If `filter()` returns a value, the returned
    ///   value is inserted into `self` using the original key.
    /// * If a field is contained in both `other` and `self`, `merge()` is
    ///   called with mutable access to the value from `self` and a reference to
    ///   the value from `other`. The `merge()` function is responsible for
    ///   updating the value if needed to complete the merge.
    /// * If a field is contained in `self` but not in `other`, it is ignored.
    #[inline]
    pub fn merge_with(
        &mut self,
        other: &Self,
        mut filter: impl FnMut(&Key, &Value) -> Option<Value>,
        mut merge: impl FnMut(&Key, &mut Value, &Value),
    ) where
        Key: Clone,
    {
        let mut self_index = 0;
        let mut other_index = 0;

        while self_index < self.len() && other_index < other.len() {
            let self_field = &mut self.fields[self_index];
            let other_field = &other.fields[other_index];
            match Key::compare(&self_field.key, &other_field.key) {
                Ordering::Less => {
                    // Self has a key that other didn't.
                    self_index += 1;
                }
                Ordering::Equal => {
                    // Both have the value, we might need to merge.
                    self_index += 1;
                    other_index += 1;
                    merge(&self_field.key, &mut self_field.value, &other_field.value);
                }
                Ordering::Greater => {
                    // Other has a value that self doesn't.
                    other_index += 1;
                    let Some(value) = filter(&other_field.key, &other_field.value) else { continue };

                    self.fields
                        .insert(self_index, Field::new(other_field.key.clone(), value));
                    self_index += 1;
                }
            }
        }

        if other_index < other.fields.len() {
            // Other has more entries that we don't have
            for field in &other.fields[other_index..] {
                let Some(value) = filter(&field.key, &field.value) else { continue };

                self.fields.push(Field::new(field.key.clone(), value));
            }
        }
    }

    /// Returns an iterator that returns all of the elements in this collection.
    /// After the iterator is dropped, this object will be empty.
    pub fn drain(&mut self) -> Drain<'_, Key, Value> {
        Drain(self.fields.drain(..))
    }
}

trait EntryKey<Key, Needle = Key>
where
    Needle: ?Sized,
{
    fn as_ref(&self) -> &Needle;
    fn into_owned(self) -> Key;
}

/// A key provided to the [`Map::entry`] function.
///
/// This is a [`Cow`](alloc::borrow::Cow)-like type that is slightly more
/// flexible with `From` implementations. The `Owned` and `Borrowed` types are
/// kept separate, allowing for more general `From` implementations.
#[derive(Debug)]
pub enum SearchKey<'key, Owned, Borrowed>
where
    Borrowed: ?Sized,
{
    /// A borrowed key.
    Borrowed(&'key Borrowed),
    /// An owned key.
    Owned(Owned),
}

impl<'key, K> From<K> for SearchKey<'key, K, K> {
    fn from(value: K) -> Self {
        SearchKey::Owned(value)
    }
}

impl<'key, Key, Needle> From<&'key Needle> for SearchKey<'key, Key, Needle>
where
    Needle: ?Sized,
{
    fn from(value: &'key Needle) -> Self {
        SearchKey::Borrowed(value)
    }
}

impl<'key, Key, Needle> SearchKey<'key, Key, Needle>
where
    Key: Borrow<Needle>,
    Needle: ToOwned<Owned = Key> + ?Sized,
{
    fn as_ref(&self) -> &Needle {
        match self {
            SearchKey::Borrowed(key) => key,
            SearchKey::Owned(owned) => owned.borrow(),
        }
    }

    fn into_owned(self) -> Key {
        match self {
            SearchKey::Borrowed(key) => key.to_owned(),
            SearchKey::Owned(owned) => owned,
        }
    }
}

impl<Key, Value> Debug for Map<Key, Value>
where
    Key: Debug + Sort<Key>,
    Value: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_map();
        for Field { key, value } in self {
            s.entry(key, value);
        }
        s.finish()
    }
}

impl<'a, Key, Value> IntoIterator for &'a Map<Key, Value>
where
    Key: Sort<Key>,
{
    type IntoIter = Iter<'a, Key, Value>;
    type Item = &'a Field<Key, Value>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        Iter(self.fields.iter())
    }
}

impl<Key, Value> IntoIterator for Map<Key, Value>
where
    Key: Sort<Key>,
{
    type IntoIter = IntoIter<Key, Value>;
    type Item = Field<Key, Value>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.fields.into_iter())
    }
}

impl<Key, Value> FromIterator<(Key, Value)> for Map<Key, Value>
where
    Key: Sort<Key>,
{
    #[inline]
    fn from_iter<T: IntoIterator<Item = (Key, Value)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut obj = Self::with_capacity(iter.size_hint().0);
        for (key, value) in iter {
            obj.insert(key, value);
        }
        obj
    }
}

/// A field in an [`Map`].
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Field<Key, Value> {
    key: Key,
    /// The value contained in this field.
    pub value: Value,
}

impl<Key, Value> Field<Key, Value> {
    /// Returns a new field with `key` and `value`.
    #[must_use]
    #[inline]
    pub fn new(key: Key, value: Value) -> Self {
        Self { key, value }
    }

    /// Returns the key of this field.
    #[inline]
    pub fn key(&self) -> &Key {
        &self.key
    }
}

/// The result of looking up an entry by its key.
#[derive(Debug)]
pub enum Entry<'a, 'key, Key, Value, BorrowedKey>
where
    Key: Sort<Key>,
    BorrowedKey: ?Sized,
{
    /// A field was found for the given key.
    Occupied(OccupiedEntry<'a, Key, Value>),
    /// A field was not found for the given key.
    Vacant(VacantEntry<'a, 'key, Key, Value, BorrowedKey>),
}

impl<'a, 'key, Key, Value, BorrowedKey> Entry<'a, 'key, Key, Value, BorrowedKey>
where
    Key: Sort<Key>,
    BorrowedKey: ?Sized,
{
    /// Invokes `update()` with the stored entry, if one was found.
    #[must_use]
    #[inline]
    pub fn and_modify(mut self, update: impl FnOnce(&mut Value)) -> Self {
        if let Self::Occupied(entry) = &mut self {
            update(&mut *entry);
        }

        self
    }

    /// If an entry was not found for the given key, `contents` is invoked to
    #[inline]
    pub fn or_insert_with(self, contents: impl FnOnce() -> Value) -> &'a mut Value
    where
        Key: Borrow<BorrowedKey>,
        BorrowedKey: ToOwned<Owned = Key>,
    {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(contents()),
        }
    }

    /// If an entry was not found for the given key, `contents` is invoked to
    #[inline]
    pub fn or_insert(self, value: Value) -> &'a mut Value
    where
        Key: Borrow<BorrowedKey>,
        BorrowedKey: ToOwned<Owned = Key>,
    {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(value),
        }
    }
}

/// An entry that exists in an [`Map`].
#[derive(Debug)]
pub struct OccupiedEntry<'a, Key, Value>
where
    Key: Sort<Key>,
{
    object: &'a mut Map<Key, Value>,
    index: usize,
}

impl<'a, Key, Value> OccupiedEntry<'a, Key, Value>
where
    Key: Sort<Key>,
{
    #[inline]
    fn new(object: &'a mut Map<Key, Value>, index: usize) -> Self {
        Self { object, index }
    }

    #[inline]
    fn field(&self) -> &Field<Key, Value> {
        &self.object.fields[self.index]
    }

    #[inline]
    fn field_mut(&mut self) -> &mut Field<Key, Value> {
        &mut self.object.fields[self.index]
    }

    /// Converts this entry into a mutable reference to the value.
    ///
    /// This is different from `DerefMut` because the `DerefMut` extends the
    /// lifetime to include `self`. This function extracts the reference with
    /// the original lifetime of the map.
    #[must_use]
    #[inline]
    pub fn into_mut(self) -> &'a mut Value {
        &mut self.object.fields[self.index].value
    }

    /// Returns the key of this field.
    #[must_use]
    #[inline]
    pub fn key(&self) -> &Key {
        &self.field().key
    }

    /// Replaces the contents of this field with `value`, and returns the
    /// existing value.
    #[inline]
    pub fn replace(self, value: Value) -> Value {
        core::mem::replace(self.into_mut(), value)
    }

    /// Removes the entry from the map, and returns the value.
    #[must_use]
    #[inline]
    pub fn remove(self) -> Field<Key, Value> {
        self.object.fields.remove(self.index)
    }
}

impl<'a, Key, Value> Deref for OccupiedEntry<'a, Key, Value>
where
    Key: Sort<Key>,
{
    type Target = Value;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.field().value
    }
}

impl<'a, Key, Value> DerefMut for OccupiedEntry<'a, Key, Value>
where
    Key: Sort<Key>,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.field_mut().value
    }
}

/// A vacant entry in an [`Map`].
#[derive(Debug)]
pub struct VacantEntry<'a, 'key, Key, Value, BorrowedKey>
where
    Key: Sort<Key>,
    BorrowedKey: ?Sized,
{
    object: &'a mut Map<Key, Value>,
    key: SearchKey<'key, Key, BorrowedKey>,
    insert_at: usize,
}

impl<'a, 'key, Key, Value, BorrowedKey> VacantEntry<'a, 'key, Key, Value, BorrowedKey>
where
    Key: Sort<Key>,
    BorrowedKey: ?Sized,
{
    #[inline]
    fn new(
        object: &'a mut Map<Key, Value>,
        key: SearchKey<'key, Key, BorrowedKey>,
        insert_at: usize,
    ) -> Self {
        Self {
            object,
            key,
            insert_at,
        }
    }

    /// Inserts `key` and `value` at this location in the object.
    ///
    /// # Panics
    ///
    /// This function panics if `key` does not match the original order of the
    /// key that was passed to [`Map::entry()`].
    #[inline]
    pub fn insert(self, value: Value) -> &'a mut Value
    where
        Key: Borrow<BorrowedKey>,
        BorrowedKey: ToOwned<Owned = Key>,
    {
        self.object
            .fields
            .insert(self.insert_at, Field::new(self.key.into_owned(), value));
        &mut self.object.fields[self.insert_at].value
    }
}

/// An iterator over the [`Field`]s in an [`Map`].
pub struct Iter<'a, Key, Value>(slice::Iter<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Iter<'a, Key, Value> {
    type Item = &'a Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Iter<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Iter<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<'a, Key, Value> FusedIterator for Iter<'a, Key, Value> {}

/// An iterator over mutable [`Field`]s contained in an [`Map`].
pub struct IterMut<'a, Key, Value>(slice::IterMut<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for IterMut<'a, Key, Value> {
    type Item = (&'a Key, &'a mut Value);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some((&field.key, &mut field.value))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| (&field.key, &mut field.value))
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| (&field.key, &mut field.value))
    }
}

impl<'a, Key, Value> ExactSizeIterator for IterMut<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for IterMut<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0
            .next_back()
            .map(|field| (&field.key, &mut field.value))
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0
            .nth_back(n)
            .map(|field| (&field.key, &mut field.value))
    }
}

impl<'a, Key, Value> FusedIterator for IterMut<'a, Key, Value> {}

/// An iterator that returns all of the elements of an [`Map`] while
/// freeing its underlying memory.
pub struct IntoIter<Key, Value>(vec::IntoIter<Field<Key, Value>>);

impl<Key, Value> Iterator for IntoIter<Key, Value> {
    type Item = Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<Key, Value> ExactSizeIterator for IntoIter<Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<Key, Value> DoubleEndedIterator for IntoIter<Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<Key, Value> FusedIterator for IntoIter<Key, Value> {}

/// An iterator over the values contained in an [`Map`].
pub struct Values<'a, Key, Value>(slice::Iter<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Values<'a, Key, Value> {
    type Item = &'a Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some(&field.value)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| &field.value)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| &field.value)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Values<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Values<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|field| &field.value)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|field| &field.value)
    }
}

impl<'a, Key, Value> FusedIterator for Values<'a, Key, Value> {}

/// An iterator over mutable values contained in an [`Map`].
pub struct ValuesMut<'a, Key, Value>(slice::IterMut<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for ValuesMut<'a, Key, Value> {
    type Item = &'a mut Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some(&mut field.value)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| &mut field.value)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| &mut field.value)
    }
}

impl<'a, Key, Value> ExactSizeIterator for ValuesMut<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for ValuesMut<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|field| &mut field.value)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|field| &mut field.value)
    }
}

impl<'a, Key, Value> FusedIterator for ValuesMut<'a, Key, Value> {}

/// An iterator returning all of the values contained in an [`Map`] as its
/// underlying storage is freed.
pub struct IntoValues<Key, Value>(vec::IntoIter<Field<Key, Value>>);

impl<Key, Value> Iterator for IntoValues<Key, Value> {
    type Item = Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some(field.value)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| field.value)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| field.value)
    }
}

impl<Key, Value> ExactSizeIterator for IntoValues<Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<Key, Value> DoubleEndedIterator for IntoValues<Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|field| field.value)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|field| field.value)
    }
}

impl<Key, Value> FusedIterator for IntoValues<Key, Value> {}

/// An iterator that removes all of the [`Field`]s of an [`Map`].
///
/// When this iterator is dropped, the underlying [`Map`] will be empty
/// regardless of whether the iterator has been fully exhausted.
pub struct Drain<'a, Key, Value>(vec::Drain<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Drain<'a, Key, Value> {
    type Item = Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    #[inline]
    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    #[inline]
    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Drain<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Drain<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<'a, Key, Value> FusedIterator for Drain<'a, Key, Value> {}

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

impl<Key, Needle> Sort<Needle> for Key
where
    Key: Ord + PartialOrd<Needle>,
{
    #[inline]
    fn compare(&self, b: &Needle) -> Ordering {
        self.partial_cmp(b).expect("comparison failed")
    }
}

impl<'a, Key> EntryKey<<Key as ToOwned>::Owned, Key> for &'a Key
where
    Key: ToOwned,
{
    #[inline]
    fn as_ref(&self) -> &Key {
        self
    }

    #[inline]
    fn into_owned(self) -> <Key as ToOwned>::Owned {
        self.to_owned()
    }
}
