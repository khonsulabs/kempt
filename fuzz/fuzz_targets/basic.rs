#![no_main]
use std::collections::BTreeSet;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use objectmap::ObjectMap;

#[derive(Debug, Arbitrary)]
enum Operation {
    Insert(u8),
    // Remove(u8),
}

fuzz_target!(|operations: Vec<Operation>| {
    let mut master = BTreeSet::new();
    let mut obj = ObjectMap::new();

    for operation in operations {
        match operation {
            Operation::Insert(key) => {
                let contained = master.insert(key);
                assert_eq!(contained, obj.insert(key, ()).is_none());
            } /* Operation::Remove(key) => {
               *     let contained = master.remove(&key);
               *     assert_eq!(contained, obj.remove(&key).is_some());
               * } */
        }

        assert_eq!(master.len(), obj.len());
        for (master, (obj, _)) in master.iter().zip(&obj) {
            assert_eq!(master, obj);
        }
        for key in &master {
            assert!(obj.contains(key));
        }
    }
});
