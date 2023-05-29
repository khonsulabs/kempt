use alloc::borrow::ToOwned;
use alloc::string::String;
use core::borrow::Borrow;

use crate::{scan_limit, Field, ObjectMap};

#[test]
fn basics() {
    let mut map = ObjectMap::default();
    assert!(map.insert("b", 1).is_none());
    assert_eq!(map.insert("b", 2), Some(Field::new("b", 1)));
    assert!(map.insert("a", 1).is_none());

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
    let mut map = ObjectMap::<String, usize>::new();
    let entry = map.entry("a").or_insert(1);
    assert_eq!(*entry, 1);
    let entry = map
        .entry(String::from("a"))
        .or_insert_with(|| unreachable!());
    assert_eq!(*entry, 1);
    let entry = map
        .entry(&String::from("a"))
        .or_insert_with(|| unreachable!());
    assert_eq!(*entry, 1);

    let mut map = ObjectMap::<CustomType, usize>::new();
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
