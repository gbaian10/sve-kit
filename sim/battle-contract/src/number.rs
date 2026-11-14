//! Integers that a JavaScript `number` holds exactly.
//!
//! The limit applies to the wire only. Out-of-range values are refused rather than
//! truncated, saturated or turned into strings, and JSON tokens with a fraction or an
//! exponent are refused before any rounding could hide them.

use crate::message::HostError;
use alloc::borrow::Cow;
use core::fmt;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// `Number.MAX_SAFE_INTEGER`, 9007199254740991: larger magnitudes round in JavaScript.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const MAX_SIGNED: i64 = (1 << 53) - 1;
const MIN_SIGNED: i64 = -MAX_SIGNED;

/// A count, index or sequence number from 0 to [`MAX_SAFE_INTEGER`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nat(u64);

impl Nat {
    /// Largest value.
    pub const MAX: Self = Self(MAX_SAFE_INTEGER);
    /// Smallest value.
    pub const ZERO: Self = Self(0);

    /// The checked value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for Nat {
    type Error = HostError;

    fn try_from(value: u64) -> Result<Self, HostError> {
        if value <= MAX_SAFE_INTEGER {
            Ok(Self(value))
        } else {
            Err(HostError::RANGE)
        }
    }
}

impl TryFrom<i64> for Nat {
    type Error = HostError;

    fn try_from(value: i64) -> Result<Self, HostError> {
        let unsigned = u64::try_from(value).map_err(|_negative| HostError::RANGE)?;
        Self::try_from(unsigned)
    }
}

impl TryFrom<usize> for Nat {
    type Error = HostError;

    fn try_from(value: usize) -> Result<Self, HostError> {
        let unsigned = u64::try_from(value).map_err(|_wider| HostError::RANGE)?;
        Self::try_from(unsigned)
    }
}

impl TryFrom<f64> for Nat {
    type Error = HostError;

    fn try_from(value: f64) -> Result<Self, HostError> {
        Self::try_from(integral(value)?)
    }
}

impl From<u32> for Nat {
    fn from(value: u32) -> Self {
        Self(u64::from(value))
    }
}

impl Serialize for Nat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Nat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_u64(NatVisitor)
    }
}

impl JsonSchema for Nat {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Nat")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "minimum": 0_u64, "maximum": MAX_SAFE_INTEGER })
    }
}

/// A signed amount from -[`MAX_SAFE_INTEGER`] to [`MAX_SAFE_INTEGER`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Int(i64);

impl Int {
    /// Largest value.
    pub const MAX: Self = Self(MAX_SIGNED);
    /// Smallest value.
    pub const MIN: Self = Self(MIN_SIGNED);
    /// Zero.
    pub const ZERO: Self = Self(0);

    /// The checked value.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for Int {
    type Error = HostError;

    fn try_from(value: i64) -> Result<Self, HostError> {
        if (MIN_SIGNED..=MAX_SIGNED).contains(&value) {
            Ok(Self(value))
        } else {
            Err(HostError::RANGE)
        }
    }
}

impl TryFrom<u64> for Int {
    type Error = HostError;

    fn try_from(value: u64) -> Result<Self, HostError> {
        let signed = i64::try_from(value).map_err(|_wider| HostError::RANGE)?;
        Self::try_from(signed)
    }
}

impl TryFrom<f64> for Int {
    type Error = HostError;

    fn try_from(value: f64) -> Result<Self, HostError> {
        Self::try_from(integral(value)?)
    }
}

impl From<i32> for Int {
    fn from(value: i32) -> Self {
        Self(i64::from(value))
    }
}

impl From<Nat> for Int {
    fn from(value: Nat) -> Self {
        // Every `Nat` is below 2^63, so the sign bit is never set.
        Self(value.0.cast_signed())
    }
}

impl Serialize for Int {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for Int {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_i64(IntVisitor)
    }
}

impl JsonSchema for Int {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Int")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "minimum": MIN_SIGNED, "maximum": MAX_SIGNED })
    }
}

/// A finite float without a fraction, as the integer it denotes.
fn integral(value: f64) -> Result<i64, HostError> {
    // The shortest decimal form of such a float is a plain integer, so parsing it back
    // refuses fractions, NaN and infinities without a lossy cast.
    value
        .to_string()
        .parse()
        .map_err(|_not_integer| HostError::RANGE)
}

// Float tokens keep the visitors' default refusal: a decoder only produces one for a
// fraction, an exponent or a magnitude it has already rounded.
struct NatVisitor;

impl Visitor<'_> for NatVisitor {
    type Value = Nat;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer from 0 to 9007199254740991")
    }

    fn visit_i64<E>(self, v: i64) -> Result<Nat, E>
    where
        E: de::Error,
    {
        Nat::try_from(v).map_err(E::custom)
    }

    fn visit_u64<E>(self, v: u64) -> Result<Nat, E>
    where
        E: de::Error,
    {
        Nat::try_from(v).map_err(E::custom)
    }
}

struct IntVisitor;

impl Visitor<'_> for IntVisitor {
    type Value = Int;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer from -9007199254740991 to 9007199254740991")
    }

    fn visit_i64<E>(self, v: i64) -> Result<Int, E>
    where
        E: de::Error,
    {
        Int::try_from(v).map_err(E::custom)
    }

    fn visit_u64<E>(self, v: u64) -> Result<Int, E>
    where
        E: de::Error,
    {
        Int::try_from(v).map_err(E::custom)
    }
}
