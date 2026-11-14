//! UTF-8 JSON encoding with the contract's failures reported as [`HostError`].

use crate::message::HostError;
use crate::number::{Int, Nat};
use core::cell::Cell;
use core::fmt;
use serde::Serialize;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor,
};
use serde_json::{Map, Value};

/// Encode a message as JSON text.
///
/// This only serialises: numbers are safe because contract types hold them as
/// [`Nat`] and [`Int`], not because this function checks an arbitrary value.
///
/// # Errors
/// The value cannot be represented as JSON.
pub fn encode<T>(message: &T) -> Result<String, HostError>
where
    T: Serialize,
{
    serde_json::to_string(message).map_err(|_unrepresentable| HostError::PROJECTION)
}

/// Decode JSON text that states a message exactly.
///
/// Every defined field must be present, `null` included; no other field may appear and
/// no key may repeat.
///
/// # Errors
/// `range` when the text is JSON without a repeated key but some number is not a safe
/// integer; otherwise `format`.
pub fn decode<T>(json: &str) -> Result<T, HostError>
where
    T: DeserializeOwned + Serialize,
{
    let unsafe_number = Cell::new(false);
    let mut text = serde_json::Deserializer::from_str(json);
    let written = Faithful {
        unsafe_number: &unsafe_number,
    }
    .deserialize(&mut text)
    .and_then(|written| text.end().map(|()| written))
    .map_err(|_malformed| HostError::FORMAT)?;
    if unsafe_number.get() {
        return Err(HostError::RANGE);
    }
    let message = T::deserialize(&written).map_err(|_mismatch| HostError::FORMAT)?;
    // The typed read fills an absent nullable field and skips fields beside a bare tag;
    // the wire allows neither, so the text must say exactly what the message says.
    if serde_json::to_value(&message).is_ok_and(|stated| stated == written) {
        Ok(message)
    } else {
        Err(HostError::FORMAT)
    }
}

/// Reads JSON into a tree without losing what a plain tree hides.
///
/// A plain tree keeps only the last value of a repeated key and rounds a number it cannot
/// hold, so a refused token could vanish before any check. Here a repeated key is an
/// error and every number token is checked as it is read.
#[derive(Clone, Copy)]
struct Faithful<'report> {
    unsafe_number: &'report Cell<bool>,
}

impl<'de> DeserializeSeed<'de> for Faithful<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Faithful<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Bool(v))
    }

    fn visit_f64<E>(self, _v: f64) -> Result<Value, E>
    where
        E: de::Error,
    {
        // The parser yields a float for a fraction, an exponent or a magnitude it has
        // already rounded; none is a safe integer, whatever value survived.
        self.unsafe_number.set(true);
        Ok(Value::Null)
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E>
    where
        E: de::Error,
    {
        if Int::try_from(v).is_err() {
            self.unsafe_number.set(true);
        }
        Ok(Value::from(v))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut fields = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(self)?;
            if fields.insert(key, value).is_some() {
                return Err(de::Error::custom("repeated key"));
            }
        }
        Ok(Value::Object(fields))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E>
    where
        E: de::Error,
    {
        if Nat::try_from(v).is_err() {
            self.unsafe_number.set(true);
        }
        Ok(Value::from(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }
}
