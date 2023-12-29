<!-- markdownlint-disable MD024 -->
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v0.2.3

### Added

- `Map::clear()`/`Set::clear()` have been added to clear the contents of the
  collections.
- `Map::shrink_to[_fit]()`/`Set::shrink_to[_fit]()` have been added to allow
  shrinking an existing collection's allocation.

## v0.2.2

### Added

- `Set::drain` returns an iterator that drains the contents of the set.
- `Set` now implements `Serialize` and `Deserialize` when the `serde` feature is
  enabled.
- `Set` and `Map` now implement `Hash`.
- `Set::capacity` returns the currently allocated capacity.

## v0.2.1

### Added

- `Map::get_mut` and `Map::get_field_mut` provide exclusive access to the fields
  that are found.
- `Set::remove_member` removes a member by index.
- `Set` now implements `Debug`, `Ord`, `Eq`, and `Clone`.

## v0.2.0

### Added

- `Set<T>` is a new ordered collection type for this crate. It is similar to
  BTreeSet, but uses this crate's `Map` type under the hood.
- `Map::insert_with` takes a closure for the value and only invokes the closure
  if they key is not found in the collection. If the key is found in the
  collection, the map is left unchanged and the key provided to this function is
  returned.
- `Map::get_field` returns the `Field` contained within the map, allowing access
  to both the Key and value.
- `Map::keys`/`Map::into_keys` return iterators over the keys of a Map.
- `Map::union`/`Map::intersection`/`Map::difference` are new functions that
  return iterators that efficiently perform the "set" operations.
- `Field::into_key` takes ownership of the field and returns the owned Key.
- `Field::into_parts` takes ownership of the field and returns a `(Key, Value)`
  tuple.
- `Entry::or_default` inserts a default value for the entry if it is vacant. A
  mutable reference to the entry's value is returned.
- `VacantEntry::key` returns a reference to the key being inserted.

## v0.1.0

Initial release.
