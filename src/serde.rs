use core::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};

use crate::{Field, Map, Sort};

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
        for Field { key, value } in self {
            map.serialize_entry(key, value)?;
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
        formatter.write_str("an Map")
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

#[test]
fn tests() {
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
        r#"Error { msg: "invalid type: integer `1`, expected an Map" }"#,
    );
}
