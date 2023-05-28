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
