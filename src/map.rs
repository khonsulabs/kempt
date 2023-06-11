use alloc::borrow::ToOwned;
use alloc::vec::{self, Vec};
use core::alloc::Layout;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{self, Debug};
use core::iter::{FusedIterator, Peekable};
use core::ops::{Deref, DerefMut};
use core::{mem, slice};

use crate::Sort;

/// An ordered Key/Value map.
///
/// This type is similar to [`BTreeMap`](alloc::collections::BTreeMap), but
/// utilizes a simpler storage model. Additionally, it provides a more thorough
/// interface and has a [`merge_with()`](Self::merge_with) function.
///
/// This type is designed for collections with a limited number of keys. In
/// general, this collection excels when there are fewer entries, while
/// `HashMap` or `BTreeMap` will be better choices with larger numbers of
/// entries. Additionally, `HashMap` will perform better if comparing the keys
/// is expensive.
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

#[test]
fn scan_limit_tests() {
    // Small sizes seem better to narrow down via binary search up until ~16
    // elements.
    assert_eq!(scan_limit::<u8, ()>(), 16);
    // Test a mid-point of the heuristic.
    assert_eq!(scan_limit::<u64, u64>(), 8);
    // Large field sizes only scan chunks of 4.
    assert_eq!(scan_limit::<(u128, u128), (u128, u128)>(), 4);
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

    /// Returns the current capacity this map can hold before it must
    /// reallocate.
    #[must_use]
    #[inline]
    pub fn capacity(&self) -> usize {
        self.fields.capacity()
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

    /// Inserts an entry with `key` only if the map does not already contain
    /// that key.
    ///
    /// If an existing key is found, `Some(key)` is returned. If an existing key
    /// isn't found, `value()` will be called, a new entry will be inserted, and
    /// `None` will be returned.
    ///
    /// This is similar to using [`Map::entry`], except this function does not
    /// require that `Key` implement [`ToOwned`].
    pub fn insert_with(&mut self, key: Key, value: impl FnOnce() -> Value) -> Option<Key> {
        match self.find_key_index(&key) {
            Err(insert_at) => {
                self.fields.insert(insert_at, Field::new(key, value()));
                None
            }
            Ok(_) => Some(key),
        }
    }

    /// Returns true if this object contains `key`.
    #[inline]
    pub fn contains<SearchFor>(&self, key: &SearchFor) -> bool
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.find_key_index(key).is_ok()
    }

    /// Returns the value associated with `key`, if found.
    #[inline]
    pub fn get<SearchFor>(&self, key: &SearchFor) -> Option<&Value>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.get_field(key).map(|field| &field.value)
    }

    /// Returns the value associated with `key`, if found.
    #[inline]
    pub fn get_field<SearchFor>(&self, key: &SearchFor) -> Option<&Field<Key, Value>>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.find_key(key).ok()
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
    pub fn remove<SearchFor>(&mut self, key: &SearchFor) -> Option<Field<Key, Value>>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
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
    pub fn entry<'key, SearchFor>(
        &mut self,
        key: impl Into<SearchKey<'key, Key, SearchFor>>,
    ) -> Entry<'_, 'key, Key, Value, SearchFor>
    where
        Key: Sort<SearchFor> + Borrow<SearchFor>,
        SearchFor: ToOwned<Owned = Key> + ?Sized + 'key,
    {
        let key = key.into();
        match self.find_key_index(key.as_ref()) {
            Ok(index) => Entry::Occupied(OccupiedEntry::new(self, index)),
            Err(insert_at) => Entry::Vacant(VacantEntry::new(self, key, insert_at)),
        }
    }

    #[inline]
    fn find_key<SearchFor>(&self, search_for: &SearchFor) -> Result<&Field<Key, Value>, usize>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.find_key_index(search_for)
            .map(|index| &self.fields[index])
    }

    #[inline]
    fn find_key_mut<SearchFor>(
        &mut self,
        search_for: &SearchFor,
    ) -> Result<&mut Field<Key, Value>, usize>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
    {
        self.find_key_index(search_for)
            .map(|index| &mut self.fields[index])
    }

    #[inline]
    fn find_key_index<SearchFor>(&self, search_for: &SearchFor) -> Result<usize, usize>
    where
        Key: Sort<SearchFor>,
        SearchFor: ?Sized,
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
                    let comparison =
                        <Key as crate::Sort<SearchFor>>::compare(&field.key, search_for);
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
                <Key as crate::Sort<SearchFor>>::compare(&self.fields[midpoint].key, search_for);

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

    /// Returns an iterator over the keys in this object.
    #[must_use]
    #[inline]
    pub fn keys(&self) -> Keys<'_, Key, Value> {
        Keys(self.fields.iter())
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
    ///
    /// ```rust
    /// use kempt::Map;
    ///
    /// let a: Map<&'static str, usize> = [("a", 1), ("b", 2)].into_iter().collect();
    /// let b: Map<&'static str, usize> = [("a", 1), ("c", 3)].into_iter().collect();
    /// let merged = a.merged_with(&b, |_key, b| Some(*b), |_key, a, b| *a += *b);
    ///
    /// assert_eq!(merged.get(&"a"), Some(&2));
    /// assert_eq!(merged.get(&"b"), Some(&2));
    /// assert_eq!(merged.get(&"c"), Some(&3));
    /// ```
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
    ///
    /// ```rust
    /// use kempt::Map;
    ///
    /// let mut a: Map<&'static str, usize> = [("a", 1), ("b", 2)].into_iter().collect();
    /// let b: Map<&'static str, usize> = [("a", 1), ("c", 3)].into_iter().collect();
    /// a.merge_with(&b, |_key, b| Some(*b), |_key, a, b| *a += *b);
    /// assert_eq!(a.get(&"a"), Some(&2));
    /// assert_eq!(a.get(&"b"), Some(&2));
    /// assert_eq!(a.get(&"c"), Some(&3));
    /// ```
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

    /// Returns an iterator that yields [`Unioned`] entries.
    ///
    /// The iterator will return a single result for each unique `Key` contained
    /// in either `self` or `other`. If both collections contain a key, the
    /// iterator will contain [`Unioned::Both`] for that key.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    pub fn union<'a>(&'a self, other: &'a Self) -> Union<'a, Key, Value> {
        Union {
            left: self.iter().peekable(),
            right: other.iter().peekable(),
        }
    }

    /// Returns an iterator that yields entries that appear in both `self` and
    /// `other`.
    ///
    /// The iterator will return a result for each `Key` contained in both
    /// `self` and `other`. If a particular key is only found in one collection,
    /// it will not be included.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    pub fn intersection<'a>(&'a self, other: &'a Self) -> Intersection<'a, Key, Value> {
        Intersection {
            left: self.iter().peekable(),
            right: other.iter().peekable(),
        }
    }

    /// Returns an iterator that yields entries that appear in `self`, but not
    /// in `other`.
    ///
    /// The iterator will return a result for each `Key` contained in `self` but
    /// not contained in `other`. If a `Key` is only in `other` or is in both
    /// collections, it will not be returned.
    ///
    /// This iterator is guaranteed to return results in the sort order of the
    /// `Key` type.
    #[must_use]
    pub fn difference<'a>(&'a self, other: &'a Self) -> Difference<'a, Key, Value> {
        Difference {
            left: self.iter().peekable(),
            right: other.iter().peekable(),
        }
    }
}

trait EntryKey<Key, SearchFor = Key>
where
    SearchFor: ?Sized,
{
    fn as_ref(&self) -> &SearchFor;
    fn into_owned(self) -> Key;
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

impl<'key, Key, Borrowed> From<&'key Borrowed> for SearchKey<'key, Key, Borrowed>
where
    Borrowed: ?Sized,
{
    fn from(value: &'key Borrowed) -> Self {
        SearchKey::Borrowed(value)
    }
}

impl<'key, Key, Borrowed> SearchKey<'key, Key, Borrowed>
where
    Key: Borrow<Borrowed>,
    Borrowed: ToOwned<Owned = Key> + ?Sized,
{
    fn as_ref(&self) -> &Borrowed {
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
        // Insert out of order, then sort before returning.
        for (key, value) in iter {
            obj.fields.push(Field::new(key, value));
        }
        obj.fields.sort_unstable_by(|a, b| a.key().compare(b.key()));
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

    /// Converts this field into its key.
    #[inline]
    pub fn into_key(self) -> Key {
        self.key
    }

    /// Returns this field as the contained key and value.
    pub fn into_parts(self) -> (Key, Value) {
        (self.key, self.value)
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
    Key: Borrow<BorrowedKey> + Sort<Key>,
    BorrowedKey: ToOwned<Owned = Key> + ?Sized,
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

    /// Returns a reference to the key being inserted.
    #[inline]
    pub fn key(&self) -> &BorrowedKey {
        self.key.as_ref()
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

/// An iterator over the keys in an [`Map`].
pub struct Keys<'a, Key, Value>(slice::Iter<'a, Field<Key, Value>>);

impl<'a, Key, Value> Iterator for Keys<'a, Key, Value> {
    type Item = &'a Key;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Field::key)
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
        self.0.last().map(Field::key)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(Field::key)
    }
}

impl<'a, Key, Value> ExactSizeIterator for Keys<'a, Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a, Key, Value> DoubleEndedIterator for Keys<'a, Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(Field::key)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(Field::key)
    }
}

impl<'a, Key, Value> FusedIterator for Keys<'a, Key, Value> {}

/// An iterator converting a [`Map`] into a series of owned keys.
pub struct IntoKeys<Key, Value>(vec::IntoIter<Field<Key, Value>>);

impl<Key, Value> Iterator for IntoKeys<Key, Value> {
    type Item = Key;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Field::into_key)
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
        self.0.last().map(Field::into_key)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth(n).map(Field::into_key)
    }
}

impl<Key, Value> ExactSizeIterator for IntoKeys<Key, Value> {
    #[inline]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<Key, Value> DoubleEndedIterator for IntoKeys<Key, Value> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(Field::into_key)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.0.nth_back(n).map(Field::into_key)
    }
}

impl<Key, Value> FusedIterator for IntoKeys<Key, Value> {}

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

/// An iterator that yields [`Unioned`] entries for two [`Map`]s.
///
/// The iterator will return a single result for each unique `Key` contained in
/// either map. If both collections contain a key, the iterator will contain
/// [`Unioned::Both`] for that key.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Union<'a, K, V>
where
    K: Sort,
{
    left: Peekable<Iter<'a, K, V>>,
    right: Peekable<Iter<'a, K, V>>,
}

impl<'a, K, V> Iterator for Union<'a, K, V>
where
    K: Sort,
{
    type Item = Unioned<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(left) = self.left.peek() {
            if let Some(right) = self.right.peek() {
                match left.key().compare(right.key()) {
                    Ordering::Less => Some(Unioned::left(self.left.next().expect("just peeked"))),
                    Ordering::Equal => Some(Unioned::both(
                        self.left.next().expect("just peeked"),
                        self.right.next().expect("just peeked"),
                    )),
                    Ordering::Greater => {
                        Some(Unioned::right(self.right.next().expect("just peeked")))
                    }
                }
            } else {
                Some(Unioned::left(self.left.next().expect("just peeked")))
            }
        } else {
            self.right.next().map(Unioned::right)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left.len(), Some(self.left.len() + self.right.len()))
    }
}

/// A unioned entry from a [`Union`] iterator. An entry can be from the left,
/// right, or both maps.
pub enum Unioned<'a, K, V> {
    /// The `self`/left map contained this key/value pair.
    Left {
        /// The key of the entry.
        key: &'a K,
        /// The value of the entry.
        value: &'a V,
    },
    /// The `other`/right map contained this key/value pair.
    Right {
        /// The key of the entry.
        key: &'a K,
        /// The value of the entry.
        value: &'a V,
    },
    /// Both maps contained this `key`.
    Both {
        /// The key of the entry.
        key: &'a K,
        /// The value of the `self`/left entry.
        left: &'a V,
        /// The value of the `other`/right entry.
        right: &'a V,
    },
}

impl<'a, K, V> Unioned<'a, K, V> {
    /// If `self` is [`Unioned::Both`] `merge` will be called to produce a
    /// single value. If `self` is either [`Unioned::Left`] or
    /// [`Unioned::Right`], the key/value will be returned without calling
    /// `merge`.
    ///
    /// ```rust
    /// use kempt::Map;
    ///
    /// fn merge(a: &Map<String, u32>, b: &Map<String, u32>) -> Map<String, u32> {
    ///     a.union(b)
    ///         .map(|unioned| {
    ///             dbg!(unioned
    ///                 .map_both(|_key, left, right| *left + *right)
    ///                 .into_owned())
    ///         })
    ///         .collect()
    /// }
    ///
    /// let mut a = Map::new();
    /// a.insert(String::from("a"), 1);
    /// a.insert(String::from("b"), 1);
    /// a.insert(String::from("c"), 1);
    /// let mut b = Map::new();
    /// b.insert(String::from("b"), 1);
    ///
    /// let merged = merge(&a, &b);
    /// assert_eq!(merged.get("a"), Some(&1));
    /// assert_eq!(merged.get("b"), Some(&2));
    /// ```
    pub fn map_both<R>(self, merge: impl FnOnce(&'a K, &'a V, &'a V) -> R) -> EntryRef<'a, K, V>
    where
        R: Into<OwnedOrRef<'a, V>>,
    {
        match self {
            Unioned::Left { key, value } | Unioned::Right { key, value } => EntryRef {
                key,
                value: OwnedOrRef::Ref(value),
            },
            Unioned::Both { key, left, right } => EntryRef {
                key,
                value: merge(key, left, right).into(),
            },
        }
    }
}

impl<'a, K, V> Unioned<'a, K, V> {
    fn both(left: &'a Field<K, V>, right: &'a Field<K, V>) -> Self {
        Self::Both {
            key: left.key(),
            left: &left.value,
            right: &right.value,
        }
    }

    fn left(field: &'a Field<K, V>) -> Self {
        Self::Left {
            key: field.key(),
            value: &field.value,
        }
    }

    fn right(field: &'a Field<K, V>) -> Self {
        Self::Right {
            key: field.key(),
            value: &field.value,
        }
    }
}

/// A reference to a key from a [`Map`] and an associated value.
pub struct EntryRef<'a, K, V> {
    /// The key of the entry.
    pub key: &'a K,
    /// The associated value of this key.
    pub value: OwnedOrRef<'a, V>,
}

impl<'a, K, V> EntryRef<'a, K, V> {
    /// Returns the owned versions of the contained key and value, cloning as
    /// needed.
    pub fn into_owned(self) -> (K, V)
    where
        K: Clone,
        V: Clone,
    {
        (self.key.clone(), self.value.into_owned())
    }
}

/// An owned value or a reference to a value of that type.
///
/// This type is similar to [`alloc::borrow::Cow`] except that it does not
/// require that the contained type implement `ToOwned`.
pub enum OwnedOrRef<'a, K> {
    /// An owned value.
    Owned(K),
    /// A reference to a value.
    Ref(&'a K),
}

impl<'a, K> OwnedOrRef<'a, K> {
    /// Converts the contained value into an owned representation, cloning only
    /// if needed.
    pub fn into_owned(self) -> K
    where
        K: Clone,
    {
        match self {
            OwnedOrRef::Owned(owned) => owned,
            OwnedOrRef::Ref(r) => r.clone(),
        }
    }
}

impl<K> From<K> for OwnedOrRef<'_, K> {
    fn from(value: K) -> Self {
        Self::Owned(value)
    }
}

impl<'a, K> From<&'a K> for OwnedOrRef<'a, K> {
    fn from(value: &'a K) -> Self {
        Self::Ref(value)
    }
}

/// An iterator that yields entries that appear in two maps.
///
/// The iterator will return a result for each `Key` contained in both maps. If
/// a particular key is only found in one collection, it will not be included.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Intersection<'a, K, V>
where
    K: Sort,
{
    left: Peekable<Iter<'a, K, V>>,
    right: Peekable<Iter<'a, K, V>>,
}

impl<'a, K, V> Iterator for Intersection<'a, K, V>
where
    K: Sort,
{
    type Item = (&'a K, &'a V, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let left = self.left.peek()?;
            let right = self.right.peek()?;
            match left.key().compare(right.key()) {
                Ordering::Less => {
                    let _skipped = self.left.next();
                }
                Ordering::Equal => {
                    let left = self.left.next().expect("just peeked");
                    let right = self.right.next().expect("just peeked");
                    return Some((left.key(), &left.value, &right.value));
                }
                Ordering::Greater => {
                    let _skipped = self.right.next();
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.left.len().min(self.right.len())))
    }
}

/// An iterator over the difference between two [`Map`]s.
///
/// This iterator will return a result for each `Key` contained in `self` but
/// not contained in `other`. If a `Key` is only in `other` or is in both
/// collections, it will not be returned.
///
/// This iterator is guaranteed to return results in the sort order of the `Key`
/// type.
pub struct Difference<'a, K, V>
where
    K: Sort,
{
    left: Peekable<Iter<'a, K, V>>,
    right: Peekable<Iter<'a, K, V>>,
}

impl<'a, K, V> Iterator for Difference<'a, K, V>
where
    K: Sort,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let left = self.left.peek()?;
            if let Some(right) = self.right.peek() {
                match left.key().compare(right.key()) {
                    Ordering::Less => {
                        let left = self.left.next().expect("just peeked");
                        return Some((left.key(), &left.value));
                    }
                    Ordering::Equal => {
                        let _left = self.left.next();
                        let _right = self.right.next();
                    }
                    Ordering::Greater => {
                        let _skipped = self.right.next();
                    }
                }
            } else {
                let left = self.left.next().expect("just peeked");
                return Some((left.key(), &left.value));
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.left.len()))
    }
}
