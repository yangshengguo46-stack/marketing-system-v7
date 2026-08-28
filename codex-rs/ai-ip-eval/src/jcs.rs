use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use serde::de::DeserializeSeed;
use serde::de::Error as _;
use serde::de::MapAccess;
use serde::de::SeqAccess;
use serde::de::Visitor;
use serde_json::Map;
use serde_json::Number;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum JcsError {
    Invalid(String),
}

impl fmt::Display for JcsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for JcsError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct JcsCommitment {
    pub(crate) canonical_bytes: Vec<u8>,
    pub(crate) sha256: String,
}

pub(crate) fn parse_json(raw_json: &[u8]) -> Result<Value, JcsError> {
    let mut deserializer = serde_json::Deserializer::from_slice(raw_json);
    let value = ValueSeed
        .deserialize(&mut deserializer)
        .map_err(|error| JcsError::Invalid(format!("parse unique-key JSON: {error}")))?;
    deserializer
        .end()
        .map_err(|error| JcsError::Invalid(format!("reject trailing JSON: {error}")))?;
    Ok(value)
}

pub(crate) fn canonicalize_value(value: &Value) -> Result<Vec<u8>, JcsError> {
    serde_json_canonicalizer::to_vec(value)
        .map_err(|error| JcsError::Invalid(format!("canonicalize JSON with RFC 8785: {error}")))
}

pub(crate) fn commitment_value(domain: &[u8], value: &Value) -> Result<JcsCommitment, JcsError> {
    let canonical_bytes = canonicalize_value(value)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(&canonical_bytes);
    Ok(JcsCommitment {
        canonical_bytes,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

pub(crate) fn commitment(domain: &[u8], raw_json: &[u8]) -> Result<JcsCommitment, JcsError> {
    commitment_value(domain, &parse_json(raw_json)?)
}

struct ValueSeed;

impl<'de> DeserializeSeed<'de> for ValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        ValueSeed.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
        while let Some(value) = sequence.next_element_seed(ValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        const ARBITRARY_PRECISION_NUMBER_TOKEN: &str = "$serde_json::private::Number";
        let mut keys = HashSet::with_capacity(object.size_hint().unwrap_or(0));
        let mut values = Map::new();
        let Some(first_key) = object.next_key::<String>()? else {
            return Ok(Value::Object(values));
        };
        if first_key == ARBITRARY_PRECISION_NUMBER_TOKEN {
            let raw = object.next_value::<String>()?;
            if object.next_key::<String>()?.is_some() {
                return Err(A::Error::custom(
                    "invalid arbitrary-precision JSON number token",
                ));
            }
            let number = Number::from_str(&raw)
                .map_err(|error| A::Error::custom(format!("invalid JSON number: {error}")))?;
            return Ok(Value::Number(number));
        }
        keys.insert(first_key.clone());
        values.insert(first_key, object.next_value_seed(ValueSeed)?);
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(A::Error::custom(format!("duplicate object key {key:?}")));
            }
            values.insert(key, object.next_value_seed(ValueSeed)?);
        }
        Ok(Value::Object(values))
    }
}
