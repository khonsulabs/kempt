# Kempt

A `#[forbid_unsafe]` ordered collection crate for Rust. This crate is `no_std`
compatible using the `alloc` crate.

[![crate version](https://img.shields.io/crates/v/kempt.svg)](https://crates.io/crates/kempt)
[![Live Build Status](https://img.shields.io/github/actions/workflow/status/khonsulabs/kempt/rust.yml?branch=v0.2.3)](https://github.com/khonsulabs/kempt/actions?query=workflow:Tests)
[![HTML Coverage Report for `main` branch](https://khonsulabs.github.io/kempt/coverage/badge.svg)](https://khonsulabs.github.io/kempt/coverage/)
[![Documentation for `v0.2.3` branch](https://img.shields.io/badge/docs-main-informational)](https://docs.rs/kempt)

## `Map<K, V>`

[`Map<K,V>`][map] provides an interface similar to a `BTreeMap<K,V>`, but
utilizes a simpler storage model. The entries are stored in a `Vec`, ordered by
the keys. Retrieving values uses a hybrid binary search and sequential scan
algorithm that is aimed at taking advantage of sequential scans for better cache
performance.

```rust
use kempt::Map;

let mut map = Map::new();
map.insert("a", 1);
map.insert("b", 2);
assert_eq!(map.get(&"a"), Some(&1));
let replaced = map.insert("a", 2).expect("value exists");
assert_eq!(map.get(&"a"), Some(&2));
assert_eq!(replaced.value, 1);
```

### Why use `Map` instead of `BTreeMap` or `HashMap`?

The [`Map`][map] type provides several operations that the standard library Map
types do not:

- Ability to merge maps ([`merge_with()`][merge-with])
- Entry API that supports owned or borrowed representations, and only uses
  `ToOwned` when inserting borrowed key into a vacant entry
- Ability to access fields by index in addition to the key type

Overall, the `Map` type is very similar to the `BTreeMap` type, except that it
utilizes a single storage buffer. Because of this simplified storage model, the
`Map` type supports preallocation `with_capacity()`, while `BTreeMap` does not.

The `Map` type will not perform as well as `BTreeMap` when there is a
significant number of items in the collection (> 1k, depending on Key and Value
sizes). Some operations, such as insertion and removal, will also be slower on
moderately large collections (> 100 entries).

The `Map` type can be beneficial over using a `HashMap` for several reasons:

- Ordered iteration
- Only requires `Ord`
- Growing does not require "rebinning"
- May be faster for collections with fewer than ~100 elements (depending on Key
  and Value sizes), due to:
  - No hashing of keys
  - When hash maps are more full, the likelihood of collisions increases.
    Collisions require some sort of scan to find the matching key, and each key
    comparison falls back to `Eq`.

The `HashMap` type is more beneficial for many other use cases:

- Large Key types that are expensive to compare
- Moderately large collections (> 100 entries, depending on Key and Value sizes)

## `Set<T>`

[`Set<T>`][set] provides an interface similar to a `BTreeSet<T>`, but utilizes a
simpler storage model by leveraging this crate's `Map` type. The members are
stored in a `Vec` in sort order. Retrieving values uses a hybrid binary search
and sequential scan algorithm that is aimed at taking advantage of sequential
scans for better cache performance.

```rust
use kempt::Set;

let mut set = Set::new();
set.insert(42);
assert!(set.contains(&42));
// Inserting again will return false, because the member already exists.
assert!(!set.insert(42));
// The collection will be automatically sorted as it is mutated.
set.insert(1);
assert_eq!(set.member(0), Some(&1));
assert_eq!(set.member(1), Some(&42));
```

## Why?

This crate started as a thought experiment: when using [interned
strings][interner], could an ordered `Vec` outperform a `HashMap` for "small"
collections. And, if it could, how would it compare to `BTreeMap`? The use case
being considered was [stylecs][stylecs], where most collections were expected to
be well under 100 elements.

In addition to performing similarly or better than `HashMap` and `BTreeMap` for
the intended use cases, a generic `merge_with` API was designed that optimally
merges the contents of two collections together. This operation is expected to
be a common operation for [stylecs][stylecs], and there is no optimal way to
write such an algorithm with `HashMap` or `BTreeMap`.

This collection **is not always better than `HashMap` and/or `BTreeMap`**, and
depending on your `Key` and `Value` types, your mileage may vary. Before using
this in your own project, you should benchmark your intended use case to ensure
it meets your performance needs.

## Benchmarks

The benchmark suite can be run via cargo:

```sh
cargo bench -p benchmarks
```

The suite uses randomization, which means that each run may produce slightly
different results. However, each data structure is tested with the same
randomized data, so each individual run of a benchmark is a true comparison. The
randomization can be provided a stable seed to allow comparing the same data
sets by providing the `-s` option:

```sh
# -s takes up to 64 hexadecimal digits to produce the random seed
cargo bench -p benchmarks -- -s0
```

Full results as run on Github Actions [can be viewed
here](https://khonsulabs.github.io/kempt/benchmarks/report/index.html), using a
randomized seed. Here are the results run from the developer's machine (an AMD
Ryzen 7 3800X) for data sets containing 25 elements using `-s0`:

| Benchmark   | Key Type | [Map][map] | HashMap ([fnv][fnv]) | BTreeMap |
|-------------|----------|-----------|----------------------|----------|
| fill random | u8       |    161.1ns |              408.9ns |  447.5ns |
| fill random | usize    |    157.7ns |              289.2ns |  451.5ns |
| fill random | u128     |    260.1ns |              401.0ns |  527.8ns |
| fill seq    | u8       |    170.4ns |              413.0ns |  444.9ns |
| fill seq    | usize    |    173.3ns |              284.2ns |  461.2ns |
| fill seq    | u128     |    220.5ns |              404.9ns |  533.1ns |
| get         | u8       |      5.0ns |                4.1ns |    5.5ns |
| get         | usize    |      5.1ns |                6.4ns |    5.7ns |
| get         | u128     |      7.3ns |               11.7ns |    6.9ns |
| remove      | u8       |     19.4ns |               25.6ns |  187.1ns |
| remove      | usize    |     28.2ns |               34.6ns |  199.8ns |
| remove      | u128     |     32.2ns |               41.2ns |  203.8ns |

No benchmark suite is ever perfect. Pull requests are welcome. Each potential
user who cares about maximal performance should benchmark their own use case on
their target hardware rather than rely on these benchmark results.

[interner]: https://github.com/khonsulabs/interner
[stylecs]: https://github.com/khonsulabs/stylecs
[fnv]: https://github.com/servo/rust-fnv
[map]: https://docs.rs/kempt/*/kempt/struct.Map.html
[set]: https://docs.rs/kempt/*/kempt/struct.Set.html
[merge-with]: https://docs.rs/kempt/*/kempt/struct.Map.html#method.merge_with

## Open-source Licenses

This project, like all projects from [Khonsu Labs](https://khonsulabs.com/), is open-source.
This repository is available under the [MIT License](./LICENSE-MIT) or the
[Apache License 2.0](./LICENSE-APACHE).

To learn more about contributing, please see [CONTRIBUTING.md](./CONTRIBUTING.md).
