use core::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

use crate::{Field, ObjectMap, Sort};

impl<Key, Value> Serialize for ObjectMap<Key, Value>
where
    Key: Serialize + Sort<Key>,
    Value: Serialize,
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for Field { key, value } in self {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de, Key, Value> Deserialize<'de> for ObjectMap<Key, Value>
where
    Key: Deserialize<'de> + Sort<Key>,
    Value: Deserialize<'de>,
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(MapVisitor(PhantomData))
    }
}

struct MapVisitor<Key, Value>(PhantomData<(Key, Value)>);

impl<'de, Key, Value> Visitor<'de> for MapVisitor<Key, Value>
where
    Key: Deserialize<'de> + Sort<Key>,
    Value: Deserialize<'de>,
{
    type Value = ObjectMap<Key, Value>;

    #[inline]
    fn expecting(&self, formatter: &mut alloc::fmt::Formatter) -> alloc::fmt::Result {
        formatter.write_str("an ObjectMap")
    }

    #[inline]
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut obj = ObjectMap::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((key, value)) = map.next_entry()? {
            obj.insert(key, value);
        }
        Ok(obj)
    }
}
