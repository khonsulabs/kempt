# ObjectMap

A `#[forbid_unsafe]` ordered map for Rust.

```rust
use orderedmap::OrderedMap;

let mut map = OrderedMap::new();
map.insert("a", 1);
map.insert("b", 2);
assert_eq!(map["a"], 1);
let replaced = map.insert("a", 2);
assert_eq!(map["a"], 2);
assert_eq!(replaced, Some(1));
```

## Why?

This crate started as a thought experiment: when using [interned
strings](interner), could an ordered `Vec` outperform a `HashMap` for "small"
collections. And, if it could, how would it compare to `BTreeMap`? The use case
being considered was [stylecs](stylecs), where
most collections were expected to be well under 100 elements.

In addition to performing similarly or better than `HashMap` and `BTreeMap` for
the intended use cases, a generic `merge_with` API was designed that optimally
merges the contents of two collections together. This operation is expected to
be a common operation for [stylecs](stylecs), and there is no optimal way to
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

interner: <https://github.com/khonsulabs/interner>
stylecs: <https://github.com/khonsulabs/stylecs>
