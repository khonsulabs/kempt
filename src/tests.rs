use alloc::borrow::ToOwned;
use alloc::rc::Rc;
use alloc::string::String;
use core::borrow::Borrow;
use std::println;

use crate::{scan_limit, Entry, Field, Map};

#[test]
fn basics() {
    let mut map = Map::default();
    assert!(map.is_empty());
    assert!(map.insert("b", 1).is_none());
    assert_eq!(map.len(), 1);
    assert_eq!(map.insert("b", 2), Some(Field::new("b", 1)));
    assert_eq!(map.len(), 1);

    assert!(map.insert("a", 1).is_none());
    assert_eq!(map.len(), 2);

    assert!(map.contains(&"a"));
    assert_eq!(map.get(&"a"), Some(&1));
    assert!(map.contains(&"b"));
    assert_eq!(map.get(&"b"), Some(&2));
    assert!(!map.contains(&"c"));
    assert_eq!(map.get(&"c"), None);

    // Various iteration.
    let mut iter = map.iter();
    assert_eq!(iter.next().unwrap(), &Field::new("a", 1));
    assert_eq!(iter.next().unwrap(), &Field::new("b", 2));
    let mut iter = map.values();
    assert_eq!(iter.next().unwrap(), &1);
    assert_eq!(iter.next().unwrap(), &2);
    let mut iter = map.clone().into_iter();
    assert_eq!(iter.next().unwrap(), Field::new("a", 1));
    assert_eq!(iter.next().unwrap(), Field::new("b", 2));
    let mut iter = map.clone().into_values();
    assert_eq!(iter.next().unwrap(), 1);
    assert_eq!(iter.next().unwrap(), 2);

    // Increment via iter_mut
    for (_, value) in map.iter_mut() {
        *value += 1;
    }

    // Increment via values_mut
    for value in map.values_mut() {
        *value += 1;
    }

    // Removal
    assert_eq!(map.remove(&"a"), Some(Field::new("a", 3)));
    assert_eq!(map.remove(&"a"), None);

    // Drain
    let drained = map.drain();
    drop(drained);
    assert!(map.is_empty());
}

#[test]
fn scan_limits() {
    // Small sizes seem better to narrow down via binary search up until ~16
    // elements.
    assert_eq!(scan_limit::<u8, ()>(), 16);
    // Test a mid-point of the heuristic.
    assert_eq!(scan_limit::<u64, u64>(), 8);
    // Large field sizes only scan chunks of 4.
    assert_eq!(scan_limit::<(u128, u128), (u128, u128)>(), 4);
}

#[test]
fn entry() {
    let mut map = Map::<String, usize>::new();
    let entry = map.entry("a").or_insert(1);
    assert_eq!(*entry, 1);
    let entry = map
        .entry(String::from("a"))
        .and_modify(|value| *value += 1)
        .or_insert_with(|| unreachable!());
    assert_eq!(*entry, 2);
    let entry = map
        .entry(&String::from("b"))
        .and_modify(|_| unreachable!())
        .or_insert_with(|| 1);
    assert_eq!(*entry, 1);

    let entry = map.entry("a").or_insert(0);
    assert_eq!(*entry, 2);

    let Entry::Occupied(entry) = map.entry("a") else { unreachable!() };
    assert_eq!(entry.key(), "a");
    assert_eq!(*entry, 2);
    let removed = entry.remove();
    assert_eq!(removed.key(), "a");
    assert_eq!(removed.value, 2);

    let Entry::Occupied(entry) = map.entry("b") else { unreachable!() };
    assert_eq!(entry.replace(2), 1);
    assert_eq!(map.get("b"), Some(&2));

    let mut map = Map::<CustomType, usize>::new();
    let entry = map.entry(&CustomTypeBorrowed(1)).or_insert(42);
    assert_eq!(*entry, 42);
    let entry = map
        .entry(CustomType::new(1))
        .or_insert_with(|| unreachable!("key should be found"));
    assert_eq!(*entry, 42);
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct CustomType(CustomTypeBorrowed);

impl CustomType {
    pub fn new(value: usize) -> Self {
        Self(CustomTypeBorrowed(value))
    }
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct CustomTypeBorrowed(usize);

impl Borrow<CustomTypeBorrowed> for CustomType {
    fn borrow(&self) -> &CustomTypeBorrowed {
        &self.0
    }
}

impl ToOwned for CustomTypeBorrowed {
    type Owned = CustomType;

    fn to_owned(&self) -> Self::Owned {
        CustomType(CustomTypeBorrowed(self.0))
    }
}

impl ToOwned for CustomType {
    type Owned = Self;

    fn to_owned(&self) -> Self::Owned {
        CustomType(CustomTypeBorrowed(self.0 .0))
    }
}

impl PartialOrd<CustomTypeBorrowed> for CustomType {
    fn partial_cmp(&self, other: &CustomTypeBorrowed) -> Option<core::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialEq<CustomTypeBorrowed> for CustomType {
    fn eq(&self, other: &CustomTypeBorrowed) -> bool {
        self.0.eq(other)
    }
}

#[test]
fn binary_search_extremes() {
    // fill in 0..100 in two passes: first with evens, second with odds. This
    // should hit every possible combination of the binary search algorithm.
    let mut map = Map::new();
    for i in (0..100).step_by(2) {
        map.insert(i, i);
    }
    for i in (1..100).step_by(2) {
        map.insert(i, i);
    }
    for i in 0..100 {
        assert_eq!(map.get(&i), Some(&i));
    }
}

#[test]
fn merge() {
    let multiples_of_two = (2..100).step_by(2).map(|i| (i, 1)).collect::<Map<_, _>>();
    let multiples_of_three = (3..100).step_by(3).map(|i| (i, 1)).collect::<Map<_, _>>();
    let copy_if_not_five = |key: &usize, value: &usize| (*key % 5 != 0).then_some(*value);
    let multiples_of_2_and_3_but_not_5 = Map::new()
        .merged_with(
            &multiples_of_two,
            copy_if_not_five,
            |_key, _existing, _incoming| unreachable!(),
        )
        .merged_with(
            &multiples_of_three,
            copy_if_not_five,
            |_key, existing, incoming| *existing += *incoming,
        );
    println!(
        "All: {multiples_of_2_and_3_but_not_5:?}, {}",
        multiples_of_2_and_3_but_not_5.len()
    );
    assert_eq!(multiples_of_2_and_3_but_not_5.get(&2), Some(&1));
    assert_eq!(multiples_of_2_and_3_but_not_5.get(&3), Some(&1));
    assert_eq!(multiples_of_2_and_3_but_not_5.get(&6), Some(&2));
    assert_eq!(multiples_of_2_and_3_but_not_5.get(&30), None);
    assert_eq!(multiples_of_2_and_3_but_not_5.len(), 54);
}

#[test]
fn entry_to_owned_on_insert() {
    #[derive(Ord, PartialOrd, Eq, PartialEq)]
    struct NotCloneable;

    impl Clone for NotCloneable {
        fn clone(&self) -> Self {
            unreachable!()
        }
    }

    let rc = Rc::new(0);
    let mut map = Map::<Rc<usize>, ()>::new();
    map.entry(&rc);
    assert_eq!(Rc::strong_count(&rc), 1);
    map.entry(&rc).or_insert(());
    assert_eq!(Rc::strong_count(&rc), 2);

    // This final test proves that when passing in the owned copy, it is used
    // without being cloned.
    let mut map = Map::<NotCloneable, ()>::new();
    map.entry(NotCloneable).or_insert(());
    assert!(map.contains(&NotCloneable));
}
