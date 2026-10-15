//! Owned diagnostic values with exact JSON number tokens.
//!
//! Number admission preserves spelling and never passes integers through binary64.
//! Wire helpers keep that representation across reports and verification records.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeMap, fmt, ops, str::FromStr};

pub use crate::json;
use serde_json::value;
use serde_json::Error;
pub type Map<K, V> = BTreeMap<K, V>;

// Preserve serde_json's default wire recursion budget. It decrements before
// entering a container and rejects zero, admitting 127 nested containers.
const DEFAULT_JSON_RECURSION_LIMIT: usize = 128;

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

impl Serialize for Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw: &value::RawValue = serde_json::from_str(&self.0).expect("admitted number token");
        raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<value::RawValue>::deserialize(deserializer)?;
        raw.get().parse().map_err(serde::de::Error::custom)
    }
}

/// Structured context shared by diagnostics and retained verification logs.
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

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(value) => value.serialize(serializer),
            Self::Object(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<value::RawValue>::deserialize(deserializer)?;
        decode(raw.get(), true).map_err(serde::de::Error::custom)
    }
}

fn decode(text: &str, check_wire_depth: bool) -> Result<Value, Error> {
    let mut decoder = Decoder {
        text,
        offset: 0,
        check_wire_depth,
    };
    let value = decoder.value(0)?;
    decoder.space();
    if decoder.offset != text.len() {
        return Err(Decoder::invalid());
    }
    Ok(value)
}

/// Walk an already validated RawValue once. Revalidating each child container
/// would multiply the work of materializing large retained table reports.
struct Decoder<'a> {
    text: &'a str,
    offset: usize,
    check_wire_depth: bool,
}
impl Decoder<'_> {
    fn invalid() -> Error {
        <Error as serde::de::Error>::custom("invalid exact JSON tree")
    }
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.offset).copied()
    }
    fn space(&mut self) {
        while matches!(self.byte(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }
    fn consume(&mut self, byte: u8) -> Result<(), Error> {
        if self.byte() != Some(byte) {
            return Err(Self::invalid());
        }
        self.offset += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String, Error> {
        self.consume(b'"')?;
        let start = self.offset;
        let mut escaped = false;
        while let Some(byte) = self.byte() {
            match byte {
                b'"' => {
                    let end = self.offset;
                    self.offset += 1;
                    return if escaped {
                        serde_json::from_str(&self.text[start - 1..self.offset])
                    } else {
                        Ok(self.text[start..end].to_owned())
                    };
                }
                b'\\' => {
                    escaped = true;
                    self.offset += 2;
                }
                _ => self.offset += 1,
            }
        }
        Err(Self::invalid())
    }
    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        self.space();
        if self.check_wire_depth
            && depth >= DEFAULT_JSON_RECURSION_LIMIT - 1
            && matches!(self.byte(), Some(b'[' | b'{'))
        {
            return Err(<Error as serde::de::Error>::custom(
                "JSON recursion limit exceeded",
            ));
        }
        Ok(match self.byte() {
            Some(b'n' | b't' | b'f') => {
                let (word, value) = match self.byte() {
                    Some(b'n') => ("null", Value::Null),
                    Some(b't') => ("true", Value::Bool(true)),
                    _ => ("false", Value::Bool(false)),
                };
                if !self.text[self.offset..].starts_with(word) {
                    return Err(Self::invalid());
                }
                self.offset += word.len();
                value
            }
            Some(b'"') => Value::String(self.string()?),
            Some(b'[') => {
                self.offset += 1;
                self.space();
                let mut values = Vec::new();
                if self.byte() != Some(b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        self.space();
                        if self.byte() != Some(b',') {
                            break;
                        }
                        self.offset += 1;
                    }
                }
                self.consume(b']')?;
                Value::Array(values)
            }
            Some(b'{') => {
                self.offset += 1;
                self.space();
                let mut values = Map::new();
                if self.byte() != Some(b'}') {
                    loop {
                        self.space();
                        let key = self.string()?;
                        self.space();
                        self.consume(b':')?;
                        values.insert(key, self.value(depth + 1)?);
                        self.space();
                        if self.byte() != Some(b',') {
                            break;
                        }
                        self.offset += 1;
                    }
                }
                self.consume(b'}')?;
                Value::Object(values)
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.offset;
                while matches!(
                    self.byte(),
                    Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                ) {
                    self.offset += 1;
                }
                Value::Number(
                    self.text[start..self.offset]
                        .parse()
                        .map_err(<Error as serde::de::Error>::custom)?,
                )
            }
            _ => return Err(Self::invalid()),
        })
    }
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
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&serde_json::to_string(self).map_err(|_| fmt::Error)?)
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

/// Serialize into the exact tree, without an intermediate machine-width JSON value.
pub fn to_value<T: Serialize>(value: T) -> Result<Value, Error> {
    let text = serde_json::to_string(&value)?;
    let raw: &value::RawValue = serde_json::from_str(&text)?;
    // Programmatic construction did not use serde_json's wire recursion guard.
    decode(raw.get(), false)
}

/// Construct an owned context. Interpolated values retain exact numeric spelling.
#[macro_export]
macro_rules! json {
    (null) => { $crate::json::Value::Null };
    (true) => { $crate::json::Value::Bool(true) };
    (false) => { $crate::json::Value::Bool(false) };
    ([]) => { $crate::json::Value::Array(::std::vec::Vec::new()) };
    ({}) => { $crate::json::Value::Object($crate::json::Map::new()) };
    ([$($tokens:tt)*]) => {{
        let mut values = ::std::vec::Vec::new();
        $crate::json!(@array values $($tokens)*);
        $crate::json::Value::Array(values)
    }};
    ({$($tokens:tt)*}) => {{
        let mut values = $crate::json::Map::new();
        $crate::json!(@object values $($tokens)*);
        $crate::json::Value::Object(values)
    }};
    (@array $values:ident) => {};
    (@array $values:ident , $($rest:tt)*) => { $crate::json!(@array $values $($rest)*); };
    (@array $values:ident null $($rest:tt)*) => { $values.push($crate::json!(null)); $crate::json!(@array $values $($rest)*); };
    (@array $values:ident [$($nested:tt)*] $($rest:tt)*) => { $values.push($crate::json!([$($nested)*])); $crate::json!(@array $values $($rest)*); };
    (@array $values:ident {$($nested:tt)*} $($rest:tt)*) => { $values.push($crate::json!({$($nested)*})); $crate::json!(@array $values $($rest)*); };
    (@array $values:ident $value:expr, $($rest:tt)*) => { $values.push($crate::json!($value)); $crate::json!(@array $values $($rest)*); };
    (@array $values:ident $value:expr) => { $values.push($crate::json!($value)); };
    (@object $values:ident) => {};
    (@object $values:ident , $($rest:tt)*) => { $crate::json!(@object $values $($rest)*); };
    (@object $values:ident $key:tt : null $($rest:tt)*) => { $values.insert(($key).into(), $crate::json!(null)); $crate::json!(@object $values $($rest)*); };
    (@object $values:ident $key:tt : [$($nested:tt)*] $($rest:tt)*) => { $values.insert(($key).into(), $crate::json!([$($nested)*])); $crate::json!(@object $values $($rest)*); };
    (@object $values:ident $key:tt : {$($nested:tt)*} $($rest:tt)*) => { $values.insert(($key).into(), $crate::json!({$($nested)*})); $crate::json!(@object $values $($rest)*); };
    (@object $values:ident $key:tt : $value:expr, $($rest:tt)*) => { $values.insert(($key).into(), $crate::json!($value)); $crate::json!(@object $values $($rest)*); };
    (@object $values:ident $key:tt : $value:expr) => { $values.insert(($key).into(), $crate::json!($value)); };
    ($value:expr) => { $crate::json::to_value(&$value).expect("serializable owned JSON context") };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn exact_number_tokens_survive_owned_tree_and_wire_roundtrips() {
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
            let value: Value = from_str(text).unwrap();
            assert_eq!(value, Value::Number(number));
            assert_eq!(to_string(&value).unwrap(), text);
            assert_eq!(to_value(&value).unwrap(), value);
        }
        assert_ne!(
            from_str::<Value>("1").unwrap(),
            from_str::<Value>("1.0").unwrap()
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
    fn nested_report_interpolation_preserves_wide_numbers_and_float_kinds() {
        let exact: Value = from_str("92233720368547758081234567890").unwrap();
        let report = json!({"context": {"actual": exact}, "keys": [{"ID":1.0}], "label":"é\n\""});
        let encoded = to_string(&report).unwrap();
        assert_eq!(encoded, "{\"context\":{\"actual\":92233720368547758081234567890},\"keys\":[{\"ID\":1.0}],\"label\":\"é\\n\\\"\"}");
        assert_eq!(from_str::<Value>(&encoded).unwrap(), report);
    }

    #[test]
    fn serde_private_token_shaped_objects_remain_ordinary_objects() {
        for text in [
            r#"{"$serde_json::private::Number":"99999999999999999999"}"#,
            r#"{"$serde_json::private::RawValue":"[1,2]"}"#,
        ] {
            let value: Value = from_str(text).unwrap();
            assert!(value.is_object());
            assert_eq!(to_string(&value).unwrap(), text);
        }
    }

    #[test]
    fn recursive_values_keep_the_existing_json_deserializer_boundary() {
        for depth in [0, 1, 126, 127, 128, 129, 130] {
            for leaf in ["0", "[]", "{}"] {
                for (opening, closing) in [("[", "]"), ("{\"nested\":", "}")] {
                    let text = format!("{}{leaf}{}", opening.repeat(depth), closing.repeat(depth));
                    assert_eq!(
                        from_str::<Value>(&text).is_ok(),
                        from_str::<serde_json::Value>(&text).is_ok(),
                        "depth={depth}, leaf={leaf}, opening={opening}",
                    );
                }
            }
        }
    }

    #[test]
    fn programmatic_construction_keeps_trees_beyond_the_wire_recursion_boundary() {
        const DIGITS: &str = "92233720368547758081234567890";
        let mut value = Value::Number(DIGITS.parse().unwrap());
        let mut previous = serde_json::Value::from(1);
        for _ in 0..132 {
            value = Value::Array(vec![value]);
            previous = serde_json::Value::Array(vec![previous]);
        }
        assert!(serde_json::to_value(&previous).is_ok());
        assert_eq!(to_value(&value).unwrap(), value);
        let context = json!({"context": value.clone()});
        assert_eq!(context["context"], value);
        let text = format!("{}{DIGITS}{}", "[".repeat(132), "]".repeat(132));
        assert_eq!(to_string(&value).unwrap(), text);
        assert!(from_str::<Value>(&text).is_err());
    }

    #[test]
    fn validated_walk_retains_escaped_keys_nested_values_and_duplicate_behavior() {
        let value: Value = from_str(r#" { "\u96ea\ud83d\ude00": [null, true, false, {}, [], {"q\"\\":"雪\ud83d\ude00"}], "same": 1, "same": 2, "wide": -184467440737095516160000, "exp": 1E+9999 } "#).unwrap();
        assert_eq!(value["雪😀"][0], Value::Null);
        assert_eq!(value["雪😀"][1], true);
        assert_eq!(value["雪😀"][2], false);
        assert_eq!(value["雪😀"][3], json!({}));
        assert_eq!(value["雪😀"][4], json!([]));
        assert_eq!(value["雪😀"][5]["q\"\\"], "雪😀");
        assert_eq!(value["same"], 2);
        assert_eq!(
            value["wide"].as_number().unwrap().as_str(),
            "-184467440737095516160000"
        );
        assert_eq!(value["exp"].as_number().unwrap().as_str(), "1E+9999");
        assert_eq!(
            from_str::<Value>(&to_string(&value).unwrap()).unwrap(),
            value
        );
    }

    #[test]
    fn wire_admission_keeps_json_grammar_and_escaped_unicode_validation() {
        for text in [
            r#"{"x":1,}"#,
            "[1,]",
            r#"{"x":"\ud800"}"#,
            r#"{"x":"\q"}"#,
            r#"{"x":01}"#,
            "[true false]",
            "[1e,0]",
        ] {
            assert!(from_str::<Value>(text).is_err(), "{text:?}");
        }
    }
}
