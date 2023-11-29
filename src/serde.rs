use core::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize};

use crate::{Map, Set, Sort};

impl<Key, Value> Serialize for Map<Key, Value>
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
        for field in self {
            map.serialize_entry(field.key(), &field.value)?;
        }
        map.end()
    }
}

impl<'de, Key, Value> Deserialize<'de> for Map<Key, Value>
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
    type Value = Map<Key, Value>;

    #[inline]
    fn expecting(&self, formatter: &mut alloc::fmt::Formatter) -> alloc::fmt::Result {
        formatter.write_str("a Map")
    }

    #[inline]
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut obj = Map::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((key, value)) = map.next_entry()? {
            obj.insert(key, value);
        }
        Ok(obj)
    }
}

impl<Key> Serialize for Set<Key>
where
    Key: Ord + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for field in self {
            seq.serialize_element(field)?;
        }
        seq.end()
    }
}

impl<'de, Key> Deserialize<'de> for Set<Key>
where
    Key: Ord + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SetVisitor(PhantomData))
    }
}

struct SetVisitor<Key>(PhantomData<(Key,)>);

impl<'de, Key> Visitor<'de> for SetVisitor<Key>
where
    Key: Deserialize<'de> + Sort<Key>,
{
    type Value = Set<Key>;

    #[inline]
    fn expecting(&self, formatter: &mut alloc::fmt::Formatter) -> alloc::fmt::Result {
        formatter.write_str("a Set")
    }

    #[inline]
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut obj = Set::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(key) = seq.next_element()? {
            obj.insert(key);
        }
        Ok(obj)
    }
}

#[test]
fn map_tests() {
    use serde_test::{assert_de_tokens_error, assert_tokens, Token};

    let map = [(1, 1), (2, 2)].into_iter().collect::<Map<u8, u16>>();
    assert_tokens(
        &map,
        &[
            Token::Map { len: Some(2) },
            Token::U8(1),
            Token::U16(1),
            Token::U8(2),
            Token::U16(2),
            Token::MapEnd,
        ],
    );

    assert_de_tokens_error::<Map<u8, u16>>(
        &[Token::U8(1)],
        "invalid type: integer `1`, expected a Map",
    );
}

#[test]
fn set_tests() {
    use serde_test::{assert_de_tokens_error, assert_tokens, Token};

    let map = [1, 2].into_iter().collect::<Set<u8>>();
    assert_tokens(
        &map,
        &[
            Token::Seq { len: Some(2) },
            Token::U8(1),
            Token::U8(2),
            Token::SeqEnd,
        ],
    );

    assert_de_tokens_error::<Set<u8>>(&[Token::U8(1)], "invalid type: integer `1`, expected a Set");
}
