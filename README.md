# ObjectMap

A `#[forbid_unsafe]` ordered map for Rust.

[![crate version](https://img.shields.io/crates/v/objectmap.svg)](https://crates.io/crates/objectmap)
[![Live Build Status](https://img.shields.io/github/actions/workflow/status/khonsulabs/objectmap/rust.yml?branch=main)](https://github.com/khonsulabs/objectmap/actions?query=workflow:Tests)
[![HTML Coverage Report for `main` branch](https://khonsulabs.github.io/objectmap/coverage/badge.svg)](https://khonsulabs.github.io/objectmap/coverage/)
[![Documentation for `main` branch](https://img.shields.io/badge/docs-main-informational)](https://khonsulabs.github.io/objectmap/main/objectmap/)

```rust
use objectmap::ObjectMap;

let mut map = ObjectMap::new();
map.insert("a", 1);
map.insert("b", 2);
assert_eq!(map.get(&"a"), Some(&1));
let replaced = map.insert("a", 2).expect("value exists");
assert_eq!(map.get(&"a"), Some(&2));
assert_eq!(replaced.value, 1);
```

## Why?

This crate started as a thought experiment: when using [interned
strings][interner], could an ordered `Vec` outperform a `HashMap` for "small"
collections. And, if it could, how would it compare to `BTreeMap`? The use case
being considered was [objectmap][objectmap], where most collections were expected to
be well under 100 elements.

In addition to performing similarly or better than `HashMap` and `BTreeMap` for
the intended use cases, a generic `merge_with` API was designed that optimally
merges the contents of two collections together. This operation is expected to
be a common operation for [objectmap][objectmap], and there is no optimal way to
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
randomized data, so each individual run of a benchmark is a true comparison.

Full results as run on Github Actions [can be viewed
here](https://khonsulabs.github.io/objectmap/benchmarks/report/index.html). Here
are the results run from the developer's machine (an AMD Ryzen 7 3800X) for data
sets containing 25 elements:

| Benchmark   | Key Type | ObjectMap | HashMap ([fnv][fnv]) | BTreeMap |
|-------------|----------|-----------|----------------------|----------|
| fill random | u8       |   211.0ns |              408.1ns |  612.2ns |
| fill random | usize    |   201.6ns |              298.5ns |  640.0ns |
| fill random | u128     |   282.0ns |              415.6ns |  680.2ns |
| fill seq    | u8       |   221.0ns |              418.3ns |  606.6ns |
| fill seq    | usize    |   189.1ns |              291.1ns |  603.3ns |
| fill seq    | u128     |   262.6ns |              401.6ns |  659.7ns |
| get         | u8       |     5.0ns |                4.5ns |    5.6ns |
| get         | usize    |     5.2ns |                6.4ns |    6.1ns |
| get         | u128     |     6.0ns |               11.6ns |    7.2ns |

No benchmark suite is ever perfect. Pull requests are welcome. Each potential
user who cares about maximal performance should benchmark their own use case on
their target hardware rather than rely on these benchmark results.

[interner]: https://github.com/khonsulabs/interner
[objectmap]: https://github.com/khonsulabs/objectmap
[fnv]: https://github.com/servo/rust-fnv
