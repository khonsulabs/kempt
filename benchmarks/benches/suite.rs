use std::any::type_name;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::hash::Hash;
use std::ops::AddAssign;

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, Bencher, BenchmarkId, Criterion,
};
use fnv::FnvBuildHasher;
use kempt::Map;
use rand::distributions::Standard;
use rand::prelude::Distribution;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng, SeedableRng};

fn btree_lookup<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Clone + Ord,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<BTreeMap<_, _>>();
    let mut keys = keys.iter().cycle();

    bench.iter(|| {
        let key = black_box(keys.next().expect("cycled"));
        assert!(set.get(key).is_some());
    });
}

fn hash_lookup<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Eq + Hash + Clone,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<HashMap<_, _, FnvBuildHasher>>();
    let mut keys = keys.iter().cycle();

    bench.iter(|| {
        let key = black_box(keys.next().expect("cycled"));
        assert!(set.get(key).is_some());
    });
}

fn object_lookup<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Clone + Ord,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<Map<Key, ()>>();
    let mut keys = keys.iter().cycle();

    bench.iter(|| {
        let key = black_box(keys.next().expect("cycled"));
        assert!(set.get(key).is_some());
    });
}

fn lookup<Key>(c: &mut Criterion, keys: &[Key], sizes: &[usize])
where
    Key: Eq + Hash + Clone + Ord + Default + From<u8> + TryFrom<usize> + AddAssign,
{
    let mut group = c.benchmark_group(format!("lookup {}", type_name::<Key>()));
    for limit in sizes.iter().copied() {
        if Key::try_from(limit).is_err() {
            break;
        }
        group.bench_with_input(BenchmarkId::new("hash", limit), &keys[..limit], hash_lookup);
        group.bench_with_input(
            BenchmarkId::new("btree", limit),
            &keys[..limit],
            btree_lookup,
        );
        group.bench_with_input(
            BenchmarkId::new("object", limit),
            &keys[..limit],
            object_lookup,
        );
    }
}

fn btree_fill<Key>(bench: &mut Bencher, (keys, starting_size): &(&[Key], usize))
where
    Key: Clone + Ord,
{
    bench.iter_batched(
        || &keys[..*starting_size],
        |keys: &[Key]| {
            let mut map = BTreeMap::new();
            for key in keys {
                map.insert(key.clone(), ());
            }
        },
        BatchSize::LargeInput,
    );
}

fn hash_fill<Key>(bench: &mut Bencher, (keys, starting_size): &(&[Key], usize))
where
    Key: Eq + Hash + Clone,
{
    bench.iter_batched(
        || &keys[..*starting_size],
        |keys: &[Key]| {
            let mut map =
                HashMap::with_capacity_and_hasher(*starting_size, FnvBuildHasher::default());
            for key in keys {
                map.insert(key.clone(), ());
            }
        },
        BatchSize::LargeInput,
    );
}

fn object_fill<Key>(bench: &mut Bencher, (keys, starting_size): &(&[Key], usize))
where
    Key: Clone + Ord,
{
    bench.iter_batched(
        || &keys[..*starting_size],
        |keys: &[Key]| {
            let mut map = Map::with_capacity(*starting_size);
            for key in keys {
                map.insert(key.clone(), ());
            }
        },
        BatchSize::LargeInput,
    );
}

fn fill<Key>(c: &mut Criterion, keys: &[Key], sizes: &[usize], name: &str)
where
    Key: Eq + Hash + Clone + Ord + TryFrom<usize>,
    Standard: Distribution<Key>,
{
    let mut group = c.benchmark_group(format!("{name} {}", type_name::<Key>()));
    for limit in sizes.iter().copied() {
        if Key::try_from(limit * 2).is_err() {
            break;
        }

        group.bench_with_input(BenchmarkId::new("hash", limit), &(keys, limit), hash_fill);
        group.bench_with_input(BenchmarkId::new("btree", limit), &(keys, limit), btree_fill);
        group.bench_with_input(
            BenchmarkId::new("object", limit),
            &(keys, limit),
            object_fill,
        );
    }
}

fn btree_remove<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Clone + Ord,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<BTreeMap<_, _>>();
    let mut keys = keys.iter().cycle();

    bench.iter_batched(
        || set.clone(),
        |mut set| {
            let key = black_box(keys.next().expect("cycled"));
            assert!(set.remove(key).is_some());
        },
        BatchSize::LargeInput,
    );
}

fn hash_remove<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Eq + Hash + Clone,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<HashMap<_, _, FnvBuildHasher>>();
    let mut keys = keys.iter().cycle();

    bench.iter_batched(
        || set.clone(),
        |mut set| {
            let key = black_box(keys.next().expect("cycled"));
            assert!(set.remove(key).is_some());
        },
        BatchSize::LargeInput,
    );
}

fn object_remove<Key>(bench: &mut Bencher, keys: &[Key])
where
    Key: Clone + Ord,
{
    let set = keys
        .iter()
        .map(|key| (key.clone(), ()))
        .collect::<Map<Key, ()>>();
    let mut keys = keys.iter().cycle();

    bench.iter_batched(
        || set.clone(),
        |mut set| {
            let key = black_box(keys.next().expect("cycled"));
            assert!(set.remove(key).is_some());
        },
        BatchSize::LargeInput,
    );
}

fn remove<Key>(c: &mut Criterion, keys: &[Key], sizes: &[usize])
where
    Key: Eq + Hash + Clone + Ord + Default + From<u8> + TryFrom<usize> + AddAssign,
{
    let mut group = c.benchmark_group(format!("remove {}", type_name::<Key>()));
    for limit in sizes.iter().copied() {
        if Key::try_from(limit).is_err() {
            break;
        }
        group.bench_with_input(BenchmarkId::new("hash", limit), &keys[..limit], hash_remove);
        group.bench_with_input(
            BenchmarkId::new("btree", limit),
            &keys[..limit],
            btree_remove,
        );
        group.bench_with_input(
            BenchmarkId::new("object", limit),
            &keys[..limit],
            object_remove,
        );
    }
}

fn generate_keys<Key>(max: Key, shuffle: bool, random_seed: &[u8; 32]) -> Vec<Key>
where
    Key: Eq + Hash + Copy + Ord + Default + From<u8> + AddAssign,
{
    let mut keys = Vec::new();
    let mut key = Key::default();
    while keys.len() < 10000 && key < max {
        keys.push(key);
        key += Key::from(1);
    }
    if shuffle {
        let mut rng = StdRng::from_seed(*random_seed);
        keys.shuffle(&mut rng);
    }
    keys
}

fn suite_for_key<Key>(c: &mut Criterion, max: Key, sizes: &[usize], random_seed: &[u8; 32])
where
    Key: Eq + Hash + Copy + Ord + Default + From<u8> + TryFrom<usize> + AddAssign,
    Standard: Distribution<Key>,
{
    let keys = generate_keys::<Key>(max, true, random_seed);
    fill::<Key>(c, &keys, sizes, "fill-rdm");
    lookup::<Key>(c, &keys, sizes);
    remove::<Key>(c, &keys, sizes);
    let keys = generate_keys::<Key>(max, false, random_seed);
    fill::<Key>(c, &keys, sizes, "fill-seq");
}

fn criterion_benchmark(c: &mut Criterion) {
    let random_seed = env::args().find(|arg| arg.starts_with("-s")).map_or_else(
        || thread_rng().gen(),
        |seed| {
            let (_, seed) = seed.split_at(2);
            let (upper, lower) = if seed.len() > 32 {
                let (upper, lower) = seed.split_at(seed.len() - 32);
                (
                    u128::from_str_radix(upper, 16).expect("invalid hexadecimal seed"),
                    u128::from_str_radix(lower, 16).expect("invalid hexadecimal seed"),
                )
            } else {
                (
                    0,
                    u128::from_str_radix(seed, 16).expect("invalid hexadecimal seed"),
                )
            };
            let mut seed = [0; 32];
            seed[..16].copy_from_slice(&upper.to_be_bytes());
            seed[16..].copy_from_slice(&lower.to_be_bytes());
            seed
        },
    );
    print!("Using random seed -s");
    for b in random_seed {
        print!("{b:02x}");
    }
    println!();

    let sizes = [5, 10, 25, 50, 100, 250, 500, 1000];
    suite_for_key::<u8>(c, u8::MAX, &sizes, &random_seed);
    suite_for_key::<usize>(c, usize::MAX, &sizes, &random_seed);
    suite_for_key::<u128>(c, u128::MAX, &sizes, &random_seed);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
