#![doc = include_str!("../README.md")]
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

#[cfg(test)]
extern crate std;

extern crate alloc;

#[cfg(feature = "serde")]
mod serde;

use alloc::vec::{self, Vec};
use core::alloc::Layout;
use core::cmp::Ordering;
use core::fmt::{self, Debug};
use core::iter::FusedIterator;
use core::ops::{Deref, DerefMut, Index, IndexMut};
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
pub struct ObjectMap<Key, Value>(Vec<Field<Key, Value>>);

impl<Key, Value> Default for ObjectMap<Key, Value> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<Key, Value> ObjectMap<Key, Value> {
    /// Returns an empty map.
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Returns a map with enough memory allocated to store `capacity` elements
    /// without reallocation.
    #[must_use]
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
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
    let scan_limit = 128 / aligned;
    if scan_limit > 16 {
        16
    } else if scan_limit < 4 {
        4
    } else {
        scan_limit
    }
}

impl<Key, Value> ObjectMap<Key, Value>
where
    Key: Ord,
{
    const SCAN_LIMIT: usize = scan_limit::<Key, Value>();

    /// Inserts `key` and `value`. If an entry already existed for `key`, the
    /// value being overwritten is returned.
    #[inline]
    pub fn insert(&mut self, key: Key, value: Value) -> Option<Value> {
        match self.find_key_mut(&key) {
            Ok(existing) => Some(mem::replace(&mut existing.value, value)),
            Err(insert_at) => {
                self.0.insert(insert_at, Field { key, value });
                None
            }
        }
    }

    /// Returns true if this object contains `key`.
    #[inline]
    pub fn contains<Needle>(&self, key: &Needle) -> bool
    where
        Needle: PartialOrd<Key>,
    {
        self.find_key_index(key).is_ok()
    }

    /// Returns the value associated with `key`, if found.
    #[inline]
    pub fn get<Needle>(&self, key: &Needle) -> Option<&Value>
    where
        Needle: PartialOrd<Key>,
    {
        self.find_key(key).ok().map(|field| &field.value)
    }

    /// Removes the value associated with `key`, if found.
    #[inline]
    pub fn remove<Needle>(&mut self, key: &Needle) -> Option<Value>
    where
        Needle: PartialOrd<Key>,
    {
        let index = self.find_key_index(key).ok()?;
        let field = self.0.remove(index);
        Some(field.value)
    }

    /// Returns the number of fields in this object.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if this object has no fields.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an [`Entry`] for the associated key.
    #[inline]
    pub fn entry<Needle>(&mut self, key: &Needle) -> Entry<'_, Key, Value>
    where
        Needle: PartialOrd<Key>,
    {
        match self.find_key_index(key) {
            Ok(index) => Entry::Occupied(OccupiedEntry::new(self, index)),
            Err(insert_at) => Entry::Vacant(VacantEntry::new(self, insert_at)),
        }
    }

    #[inline]
    fn find_key<Needle>(&self, needle: &Needle) -> Result<&Field<Key, Value>, usize>
    where
        Needle: PartialOrd<Key>,
    {
        self.find_key_index(needle).map(|index| &self.0[index])
    }

    #[inline]
    fn find_key_mut<Needle>(&mut self, needle: &Needle) -> Result<&mut Field<Key, Value>, usize>
    where
        Needle: PartialOrd<Key>,
    {
        self.find_key_index(needle).map(|index| &mut self.0[index])
    }

    #[inline]
    fn find_key_index<Needle>(&self, needle: &Needle) -> Result<usize, usize>
    where
        Needle: PartialOrd<Key>,
    {
        // When the collection contains `Self::SCAN_LIMIT` or fewer elements,
        // there should be no jumps before we reach a sequential scan for the
        // key. When the collection is larger, we use a binary search to narrow
        // the search window until the window is 16 elements or less.
        let mut min = 0;
        let mut max = self.0.len();
        loop {
            let delta = max - min;
            if delta <= Self::SCAN_LIMIT {
                for (relative_index, field) in self.0[min..max].iter().enumerate() {
                    let comparison = needle.partial_cmp(&field.key).expect("invalid comparison");
                    return match comparison {
                        Ordering::Less => Err(min + relative_index),
                        Ordering::Equal => Ok(min + relative_index),
                        Ordering::Greater => continue,
                    };
                }

                return Err(max);
            }

            let midpoint = min + delta / 2;
            let comparison = needle
                .partial_cmp(&self.0[midpoint].key)
                .expect("invalid comparison");

            match comparison {
                Ordering::Less => max = midpoint,
                Ordering::Equal => return Ok(midpoint),
                Ordering::Greater => min = midpoint + 1,
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
    pub fn iter_mut(&self) -> Iter<'_, Key, Value> {
        Iter(self.0.iter())
    }

    /// Returns an iterator over the values in this object.
    #[must_use]
    #[inline]
    pub fn values(&self) -> Values<'_, Key, Value> {
        Values(self.0.iter())
    }

    /// Returns an iterator returning all of the values contained in this
    /// object.
    #[must_use]
    #[inline]
    pub fn into_values(self) -> IntoValues<Key, Value> {
        IntoValues(self.0.into_iter())
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
        mut filter: impl FnMut(&Value) -> Option<Value>,
        mut merge: impl FnMut(&mut Value, &Value),
    ) where
        Key: Clone,
    {
        let mut self_index = 0;
        let mut other_index = 0;

        while self_index < self.len() && other_index < other.len() {
            let self_field = &mut self.0[self_index];
            let other_field = &other.0[other_index];
            match self_field.key.cmp(&other_field.key) {
                Ordering::Less => {
                    // Self has a key that other didn't.
                    self_index += 1;
                }
                Ordering::Equal => {
                    // Both have the value, we might need to merge.
                    self_index += 1;
                    other_index += 1;
                    merge(&mut self_field.value, &other_field.value);
                }
                Ordering::Greater => {
                    // Other has a value that self doesn't.
                    other_index += 1;
                    let Some(value) = filter(&other_field.value) else { continue };

                    self.0.insert(
                        self_index,
                        Field {
                            key: other_field.key.clone(),
                            value,
                        },
                    );
                    self_index += 1;
                }
            }
        }

        if other_index < other.0.len() {
            // Other has more entries that we don't have
            for field in &other.0[other_index..] {
                let Some(value) = filter(&field.value) else { continue };

                self.0.push(Field {
                    key: field.key.clone(),
                    value,
                });
            }
        }
    }

    /// Returns an iterator that returns all of the elements in this collection.
    /// After the iterator is dropped, this object will be empty.
    pub fn drain(&mut self) -> Drain<'_, Key, Value> {
        Drain(self.0.drain(..))
    }
}

impl<Key, Value> Clone for ObjectMap<Key, Value>
where
    Key: Clone + Ord,
    Value: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        let mut new_obj = Self::with_capacity(self.len());

        for field in &self.0 {
            new_obj.0.push(Field {
                key: field.key.clone(),
                value: field.value.clone(),
            });
            new_obj.insert(field.key.clone(), field.value.clone());
        }

        new_obj
    }
}

impl<Key, Value> Debug for ObjectMap<Key, Value>
where
    Key: Debug,
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

impl<Key, Value> Index<usize> for ObjectMap<Key, Value> {
    type Output = Value;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index].value
    }
}

impl<Key, Value> IndexMut<usize> for ObjectMap<Key, Value> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index].value
    }
}

impl<'a, Key, Value> IntoIterator for &'a ObjectMap<Key, Value> {
    type IntoIter = Iter<'a, Key, Value>;
    type Item = &'a Field<Key, Value>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0.iter())
    }
}

impl<Key, Value> IntoIterator for ObjectMap<Key, Value> {
    type IntoIter = IntoIter<Key, Value>;
    type Item = Field<Key, Value>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

impl<Key, Value> FromIterator<(Key, Value)> for ObjectMap<Key, Value>
where
    Key: Ord,
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

/// A field in an [`ObjectMap`].
#[derive(Debug, Clone)]
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

impl<Key, Value> PartialEq<Key> for Field<Key, Value>
where
    Key: PartialEq,
{
    #[inline]
    fn eq(&self, other: &Key) -> bool {
        &self.key == other
    }
}

impl<Key, Value> PartialOrd<Key> for Field<Key, Value>
where
    Key: PartialOrd,
{
    #[inline]
    fn partial_cmp(&self, other: &Key) -> Option<Ordering> {
        self.key.partial_cmp(other)
    }
}

/// The result of looking up an entry by its key.
#[derive(Debug)]
pub enum Entry<'a, Key, Value> {
    /// A field was found for the given key.
    Occupied(OccupiedEntry<'a, Key, Value>),
    /// A field was not found for the given key.
    Vacant(VacantEntry<'a, Key, Value>),
}

impl<'a, Key, Value> Entry<'a, Key, Value> {
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
    pub fn or_insert_with(self, contents: impl FnOnce() -> Field<Key, Value>) -> &'a mut Value {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert_field(contents()),
        }
    }
}

/// An entry that exists in an [`ObjectMap`].
#[derive(Debug)]
pub struct OccupiedEntry<'a, Key, Value> {
    object: &'a mut ObjectMap<Key, Value>,
    index: usize,
}

impl<'a, Key, Value> OccupiedEntry<'a, Key, Value> {
    #[inline]
    fn new(object: &'a mut ObjectMap<Key, Value>, index: usize) -> Self {
        Self { object, index }
    }

    #[inline]
    fn field(&self) -> &Field<Key, Value> {
        &self.object.0[self.index]
    }

    #[inline]
    fn field_mut(&mut self) -> &mut Field<Key, Value> {
        &mut self.object.0[self.index]
    }

    /// Converts this entry into a mutable reference to the value.
    ///
    /// This is different from `DerefMut` because the `DerefMut` extends the
    /// lifetime to include `self`. This function extracts the reference with
    /// the original lifetime of the map.
    #[must_use]
    #[inline]
    pub fn into_mut(self) -> &'a mut Value {
        &mut self.object.0[self.index].value
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
        self.object.0.remove(self.index)
    }
}

impl<'a, Key, Value> Deref for OccupiedEntry<'a, Key, Value> {
    type Target = Value;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.field().value
    }
}

impl<'a, Key, Value> DerefMut for OccupiedEntry<'a, Key, Value> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.field_mut().value
    }
}

/// A vacant entry in an [`ObjectMap`].
#[derive(Debug)]
pub struct VacantEntry<'a, Key, Value> {
    object: &'a mut ObjectMap<Key, Value>,
    insert_at: usize,
}

impl<'a, Key, Value> VacantEntry<'a, Key, Value> {
    #[inline]
    fn new(object: &'a mut ObjectMap<Key, Value>, insert_at: usize) -> Self {
        Self { object, insert_at }
    }

    /// Inserts `key` and `value` at this location in the object.
    ///
    /// # Panics
    ///
    /// This function panics if `key` does not match the original order of the
    /// key that was passed to [`ObjectMap::entry()`].
    #[inline]
    pub fn insert(self, key: Key, value: Value) -> &'a mut Value {
        self.insert_field(Field::new(key, value))
    }

    /// Inserts a field at this vacant location in the object.
    ///
    /// # Panics
    ///
    /// This function panics if `key` does not match the original order of the
    /// key that was passed to [`ObjectMap::entry()`].
    #[inline]
    pub fn insert_field(self, field: Field<Key, Value>) -> &'a mut Value {
        // TODO verify that this is the correct insert position! We trusted them
        // to give us the same key, but we can't verify it without causing
        // lifetime issues. The two extra comparisons should be less penalizing
        // than a forced clone for any type that owns an allocation, and should
        // be dwarfed by the memcpy of the underyling data when the vec
        // accomodates the insertion.
        self.object.0.insert(self.insert_at, field);
        &mut self.object.0[self.insert_at].value
    }
}

/// An iterator over the [`Field`]s in an [`ObjectMap`].
pub struct Iter<'a, Key, Value>(slice::Iter<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Iter<'a, Key, Value> {
    type Item = &'a Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Iter<'a, Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Iter<'a, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<'a, Key, Value> FusedIterator for Iter<'a, Key, Value> {}

/// An iterator over mutable [`Field`]s contained in an [`ObjectMap`].
pub struct IterMut<'a, Key, Value>(slice::IterMut<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for IterMut<'a, Key, Value> {
    type Item = (&'a Key, &'a mut Value);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some((&field.key, &mut field.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| (&field.key, &mut field.value))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| (&field.key, &mut field.value))
    }
}

impl<'a, Key, Value> ExactSizeIterator for IterMut<'a, Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for IterMut<'a, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0
            .next_back()
            .map(|field| (&field.key, &mut field.value))
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0
            .nth_back(n)
            .map(|field| (&field.key, &mut field.value))
    }
}

impl<'a, Key, Value> FusedIterator for IterMut<'a, Key, Value> {}

/// An iterator that returns all of the elements of an [`ObjectMap`] while
/// freeing its underlying memory.
pub struct IntoIter<Key, Value>(vec::IntoIter<Field<Key, Value>>);

impl<Key, Value> Iterator for IntoIter<Key, Value> {
    type Item = Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<Key, Value> ExactSizeIterator for IntoIter<Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<Key, Value> DoubleEndedIterator for IntoIter<Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<Key, Value> FusedIterator for IntoIter<Key, Value> {}

/// An iterator over the values contained in an [`ObjectMap`].
pub struct Values<'a, Key, Value>(slice::Iter<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Values<'a, Key, Value> {
    type Item = &'a Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some(&field.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| &field.value)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| &field.value)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Values<'a, Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Values<'a, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|field| &field.value)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|field| &field.value)
    }
}

impl<'a, Key, Value> FusedIterator for Values<'a, Key, Value> {}

/// An iterator returning all of the values contained in an [`ObjectMap`] as its
/// underlying storage is freed.
pub struct IntoValues<Key, Value>(vec::IntoIter<Field<Key, Value>>);

impl<Key, Value> Iterator for IntoValues<Key, Value> {
    type Item = Value;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let field = self.0.next()?;
        Some(field.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last().map(|field| field.value)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(|field| field.value)
    }
}

impl<Key, Value> ExactSizeIterator for IntoValues<Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<Key, Value> DoubleEndedIterator for IntoValues<Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|field| field.value)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(|field| field.value)
    }
}

impl<Key, Value> FusedIterator for IntoValues<Key, Value> {}

/// An iterator that removes all of the [`Field`]s of an [`ObjectMap`].
///
/// When this iterator is dropped, the underlying [`ObjectMap`] will be empty
/// regardless of whether the iterator has been fully exhausted.
pub struct Drain<'a, Key, Value>(vec::Drain<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Drain<'a, Key, Value> {
    type Item = Field<Key, Value>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.0.count()
    }

    fn last(self) -> Option<Self::Item>
    where
        Self: Sized,
    {
        self.0.last()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Drain<'a, Key, Value> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Drain<'a, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n)
    }
}

impl<'a, Key, Value> FusedIterator for Drain<'a, Key, Value> {}
