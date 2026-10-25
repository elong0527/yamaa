//! Owned diagnostic values with exact JSON number tokens.
//!
//! Number admission preserves spelling and never passes integers through binary64.
//! This module owns only the pure owned values: wire admission, codec traits,
//! JSON text encoding/decoding and artifact writers live in yamaa-adapters
//! (see `json_wire` there), which projects these values through explicit
//! wire wrappers because the serde orphan rule bars foreign trait impls.

mod map;

use std::{fmt, ops, str::FromStr};

pub use map::Map;

/// A validated JSON number, retaining integer, fractional and exponent spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expected exact JSON number token")]
pub struct InvalidNumber;

impl Number {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_integer(&self) -> bool {
        !self.0.bytes().any(|b| matches!(b, b'.' | b'e' | b'E'))
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.is_integer().then(|| self.0.parse().ok()).flatten()
    }
    pub fn as_u64(&self) -> Option<u64> {
        self.is_integer().then(|| self.0.parse().ok()).flatten()
    }
    pub fn as_f64(&self) -> Option<f64> {
        self.0.parse::<f64>().ok().filter(|n| n.is_finite())
    }
    pub fn from_f64(value: f64) -> Option<Self> {
        value
            .is_finite()
            .then(|| Self(ryu::Buffer::new().format_finite(value).into()))
    }
}

impl FromStr for Number {
    type Err = InvalidNumber;
    fn from_str(text: &str) -> Result<Self, InvalidNumber> {
        let bytes = text.as_bytes();
        let mut at = usize::from(bytes.first() == Some(&b'-'));
        match bytes.get(at) {
            Some(b'0') => at += 1,
            Some(b'1'..=b'9') => {
                at += 1;
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
            }
            _ => return Err(InvalidNumber),
        }
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if start == at {
                return Err(InvalidNumber);
            }
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if start == at {
                return Err(InvalidNumber);
            }
        }
        if at != bytes.len() {
            return Err(InvalidNumber);
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Structured context shared by diagnostics and the retained run report.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn as_number(&self) -> Option<&Number> {
        if let Self::Number(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        self.as_number()?.as_i64()
    }
    pub fn as_u64(&self) -> Option<u64> {
        self.as_number()?.as_u64()
    }
    pub fn as_f64(&self) -> Option<f64> {
        self.as_number()?.as_f64()
    }
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_object(&self) -> Option<&Map<String, Value>> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_object_mut(&mut self) -> Option<&mut Map<String, Value>> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }
    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }
    pub fn get<I: Index>(&self, index: I) -> Option<&Value> {
        index.get(self)
    }
    pub fn get_mut<I: Index>(&mut self, index: I) -> Option<&mut Value> {
        index.get_mut(self)
    }
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

/// Index only structured values; a missing read has the conventional null view.
pub trait Index {
    fn get(self, value: &Value) -> Option<&Value>;
    fn get_mut(self, value: &mut Value) -> Option<&mut Value>;
    fn insert(self, value: &mut Value) -> &mut Value;
}
impl Index for &str {
    fn get(self, value: &Value) -> Option<&Value> {
        value.as_object()?.get(self)
    }
    fn get_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_object_mut()?.get_mut(self)
    }
    fn insert(self, value: &mut Value) -> &mut Value {
        if value.is_null() {
            *value = Value::Object(Map::new());
        }
        value
            .as_object_mut()
            .expect("JSON object index")
            .entry(self.to_owned())
            .or_default()
    }
}
impl Index for &String {
    fn get(self, value: &Value) -> Option<&Value> {
        Index::get(self.as_str(), value)
    }
    fn get_mut(self, value: &mut Value) -> Option<&mut Value> {
        Index::get_mut(self.as_str(), value)
    }
    fn insert(self, value: &mut Value) -> &mut Value {
        Index::insert(self.as_str(), value)
    }
}
impl Index for usize {
    fn get(self, value: &Value) -> Option<&Value> {
        value.as_array()?.get(self)
    }
    fn get_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_array_mut()?.get_mut(self)
    }
    fn insert(self, value: &mut Value) -> &mut Value {
        &mut value.as_array_mut().expect("JSON array index")[self]
    }
}
impl<I: Index> ops::Index<I> for Value {
    type Output = Value;
    fn index(&self, index: I) -> &Value {
        index.get(self).unwrap_or(&Value::Null)
    }
}
impl<I: Index> ops::IndexMut<I> for Value {
    fn index_mut(&mut self, index: I) -> &mut Value {
        index.insert(self)
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}
impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::Array(value)
    }
}
impl<const N: usize> From<[Value; N]> for Value {
    fn from(value: [Value; N]) -> Self {
        Self::Array(Vec::from(value))
    }
}
impl FromIterator<Value> for Value {
    fn from_iter<T: IntoIterator<Item = Value>>(values: T) -> Self {
        Self::Array(values.into_iter().collect())
    }
}
impl<K: Into<String>> FromIterator<(K, Value)> for Value {
    fn from_iter<T: IntoIterator<Item = (K, Value)>>(values: T) -> Self {
        Self::Object(
            values
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
}
impl From<Map<String, Value>> for Value {
    fn from(value: Map<String, Value>) -> Self {
        Self::Object(value)
    }
}
impl From<Number> for Value {
    fn from(value: Number) -> Self {
        Self::Number(value)
    }
}

macro_rules! integer {
    ($($ty:ty),*) => { $(
        impl From<$ty> for Number { fn from(value: $ty) -> Self { Self(value.to_string()) } }
        impl From<$ty> for Value { fn from(value: $ty) -> Self { Self::Number(value.into()) } }
        impl PartialEq<$ty> for Value {
            fn eq(&self, value: &$ty) -> bool { self == &Self::from(*value) }
        }
        impl PartialEq<Value> for $ty {
            fn eq(&self, value: &Value) -> bool { value == self }
        }
    )* };
}
integer!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
impl PartialEq<str> for Value {
    fn eq(&self, value: &str) -> bool {
        self.as_str() == Some(value)
    }
}
impl PartialEq<&str> for Value {
    fn eq(&self, value: &&str) -> bool {
        self.as_str() == Some(*value)
    }
}
impl PartialEq<String> for Value {
    fn eq(&self, value: &String) -> bool {
        self.as_str() == Some(value)
    }
}
impl PartialEq<bool> for Value {
    fn eq(&self, value: &bool) -> bool {
        self.as_bool() == Some(*value)
    }
}
impl PartialEq<Value> for str {
    fn eq(&self, value: &Value) -> bool {
        value == self
    }
}
impl PartialEq<Value> for &str {
    fn eq(&self, value: &Value) -> bool {
        value == self
    }
}
impl PartialEq<Value> for String {
    fn eq(&self, value: &Value) -> bool {
        value == self
    }
}
impl PartialEq<Value> for bool {
    fn eq(&self, value: &Value) -> bool {
        value == self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_number_tokens_are_admitted_with_spelling_retained() {
        for text in [
            "0",
            "-0",
            "1",
            "1.0",
            "-0.0",
            "1e0",
            "1E+0009",
            "0.00000000000000000001",
            "92233720368547758081234567890",
            "-184467440737095516161234567890",
            "1e9999",
        ] {
            let number: Number = text.parse().unwrap();
            assert_eq!(number.as_str(), text);
            assert_eq!(Value::Number(number).as_number().unwrap().as_str(), text);
        }
        assert_ne!(
            Value::Number("1".parse().unwrap()),
            Value::Number("1.0".parse().unwrap())
        );
        assert_eq!("1e9999".parse::<Number>().unwrap().as_f64(), None);
    }

    #[test]
    fn number_admission_rejects_values_that_could_escape_the_numeric_token() {
        for text in [
            "", "-", "+1", "01", "-01", "1.", ".1", "1e", "1e+", " 1", "1\n", "1,2", "null", "NaN",
            "[]", "{}", "\"1\"", "1\"x\":2",
        ] {
            assert!(text.parse::<Number>().is_err(), "{text:?}");
        }
    }

    #[test]
    fn owned_tree_accessors_and_indexing_keep_typed_semantics() {
        let mut value = Value::Object(Map::from([
            ("name".into(), Value::String("é".into())),
            (
                "counts".into(),
                Value::Array(vec![Value::from(1), Value::from(2)]),
            ),
            ("flag".into(), Value::Bool(true)),
            ("absent".into(), Value::Null),
        ]));
        assert_eq!(value["name"], "é");
        assert_eq!(value["counts"][1], 2);
        assert_eq!(value["flag"], true);
        assert!(value["absent"].is_null());
        assert!(value["missing"].is_null());
        value["counts"] = Value::from(7);
        assert_eq!(value["counts"], 7);
        assert_eq!(value.take()["name"], "é");
        assert!(value.is_null());
    }
}
