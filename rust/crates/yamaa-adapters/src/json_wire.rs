//! Wire projection for the core's owned JSON values.
//!
//! yamaa-core owns only the pure owned `Number`/`Value` values. Every
//! serialization responsibility lives here: serde trait impls through
//! explicit wrappers (the orphan rule bars implementing serde's foreign
//! traits for the core's foreign types), RawValue admission, JSON
//! decode/format, and typed projection between serde trees and the owned
//! tree. There are no compatibility aliases for the removed core helpers:
//! callers use `WireValue`, `decode`, `encode` and `project` directly.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use serde_json::Error;
use yamaa_core::json::{Map, Number, Value};

// Preserve serde_json's default wire recursion budget. It decrements before
// entering a container and rejects zero, admitting 127 nested containers.
const DEFAULT_JSON_RECURSION_LIMIT: usize = 128;

/// Serialize an owned exact number through its admitted raw token.
pub struct WireNumber<'a>(pub &'a Number);

impl Serialize for WireNumber<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw: &RawValue = serde_json::from_str(self.0.as_str()).expect("admitted number token");
        raw.serialize(serializer)
    }
}

/// Serialize an owned JSON tree with exact number tokens retained.
pub struct WireValue<'a>(pub &'a Value);

impl Serialize for WireValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Number(value) => WireNumber(value).serialize(serializer),
            Value::String(value) => serializer.serialize_str(value),
            Value::Array(values) => serializer.collect_seq(values.iter().map(WireValue)),
            Value::Object(values) => {
                serializer.collect_map(values.iter().map(|(key, value)| (key, WireValue(value))))
            }
        }
    }
}

/// Deserialize an owned JSON tree from one admitted raw wire value.
pub struct WireValueOwned(pub Value);

impl<'de> Deserialize<'de> for WireValueOwned {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        decode_admitted(raw.get(), true)
            .map(WireValueOwned)
            .map_err(serde::de::Error::custom)
    }
}

/// Encode an owned JSON tree to its JSON text.
pub fn encode(value: &Value) -> String {
    serde_json::to_string(&WireValue(value)).expect("owned JSON tree serializes")
}

/// Encode an owned JSON object to the text `encode` writes for the object
/// value holding it, without building that value.
pub fn encode_object(values: &Map<String, Value>) -> String {
    struct WireObject<'a>(&'a Map<String, Value>);
    impl Serialize for WireObject<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.collect_map(self.0.iter().map(|(key, value)| (key, WireValue(value))))
        }
    }
    serde_json::to_string(&WireObject(values)).expect("owned JSON tree serializes")
}

/// Decode JSON text into the owned tree, admitting exact number tokens and
/// the wire recursion budget.
pub fn decode(text: &str) -> Result<Value, Error> {
    serde_json::from_str::<WireValueOwned>(text).map(|wire| wire.0)
}

/// Decode JSON bytes into the owned tree; see `decode`.
pub fn decode_slice(bytes: &[u8]) -> Result<Value, Error> {
    serde_json::from_slice::<WireValueOwned>(bytes).map(|wire| wire.0)
}

/// Serde field adapter: serialize an owned tree through the wire wrapper,
/// for use with `#[serde(serialize_with = "...")]` on a `Value` field.
pub fn serialize_value<S: Serializer>(value: &Value, serializer: S) -> Result<S::Ok, S::Error> {
    WireValue(value).serialize(serializer)
}

/// Serde field adapter: decode an owned tree through the wire wrapper,
/// for use with `#[serde(deserialize_with = "...")]` on a `Value` field.
pub fn deserialize_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
    WireValueOwned::deserialize(deserializer).map(|wire| wire.0)
}

/// Decode JSON text whose top level is an array, into owned rows.
pub fn decode_array(text: &str) -> Result<Vec<Value>, Error> {
    match decode(text)? {
        Value::Array(values) => Ok(values),
        _ => Err(<Error as serde::de::Error>::custom("expected JSON array")),
    }
}

/// Project any serde value into the owned tree by walking the typed serde
/// tree directly (never through text). Exactness: integers retain their full
/// width and floats keep serde's canonical token, matching the previous
/// text round trip for every value serde itself can represent.
pub fn project<T: Serialize>(value: &T) -> Result<Value, Error> {
    serde_json::to_value(value).map(from_wire)
}

/// Leaf conversion used by the `json!` macro. Owned tree values project by
/// identity (they are already the owned representation); every other leaf
/// projects through `project`. The two traits share one method name and the
/// identity impls sit one autoref closer to the receiver, so method
/// resolution picks identity for owned values and the serde blanket for
/// everything else (autoref specialization).
pub trait ProjectIdentity {
    fn project_leaf(&self) -> Value;
}
impl ProjectIdentity for Value {
    fn project_leaf(&self) -> Value {
        self.clone()
    }
}
impl ProjectIdentity for Vec<Value> {
    fn project_leaf(&self) -> Value {
        Value::Array(self.clone())
    }
}
impl ProjectIdentity for [Value] {
    fn project_leaf(&self) -> Value {
        Value::Array(self.to_vec())
    }
}
impl ProjectIdentity for Vec<Vec<Value>> {
    fn project_leaf(&self) -> Value {
        Value::Array(self.iter().map(|row| Value::Array(row.clone())).collect())
    }
}
impl ProjectIdentity for Number {
    fn project_leaf(&self) -> Value {
        Value::Number(self.clone())
    }
}
impl ProjectIdentity for Map<String, Value> {
    fn project_leaf(&self) -> Value {
        Value::Object(self.clone())
    }
}
impl ProjectIdentity for Option<Value> {
    fn project_leaf(&self) -> Value {
        self.clone().unwrap_or(Value::Null)
    }
}
impl ProjectIdentity for Option<Vec<Value>> {
    fn project_leaf(&self) -> Value {
        self.clone().map_or(Value::Null, Value::Array)
    }
}
/// Blanket serde leaf projection; see `ProjectIdentity`.
pub trait ProjectSerialize {
    fn project_leaf(&self) -> Value;
}
impl<T: Serialize> ProjectSerialize for &T {
    fn project_leaf(&self) -> Value {
        project(self).expect("serializable owned JSON context")
    }
}

fn from_wire(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => Value::Number(
            value
                .to_string()
                .parse()
                .expect("serde_json number token is an admitted JSON number"),
        ),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => {
            Value::Array(values.into_iter().map(from_wire).collect())
        }
        serde_json::Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, from_wire(value)))
                .collect(),
        ),
    }
}

fn decode_admitted(text: &str, check_wire_depth: bool) -> Result<Value, Error> {
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

/// Construct an owned context. Interpolated values retain exact numeric
/// spelling through direct typed projection (never text reparse).
#[macro_export]
macro_rules! json {
    (null) => { ::yamaa_core::json::Value::Null };
    (true) => { ::yamaa_core::json::Value::Bool(true) };
    (false) => { ::yamaa_core::json::Value::Bool(false) };
    ([]) => { ::yamaa_core::json::Value::Array(::std::vec::Vec::new()) };
    ({}) => { ::yamaa_core::json::Value::Object(::yamaa_core::json::Map::new()) };
    ([$($tokens:tt)*]) => {{
        let mut values = ::std::vec::Vec::new();
        $crate::json!(@array values $($tokens)*);
        ::yamaa_core::json::Value::Array(values)
    }};
    ({$($tokens:tt)*}) => {{
        let mut values = ::yamaa_core::json::Map::new();
        $crate::json!(@object values $($tokens)*);
        ::yamaa_core::json::Value::Object(values)
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
    ($value:expr) => {{
        #[allow(unused_imports)]
        use $crate::json_wire::ProjectIdentity as _;
        #[allow(unused_imports)]
        use $crate::json_wire::ProjectSerialize as _;
        (&$value).project_leaf()
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let value: Value = decode(text).unwrap();
            assert_eq!(value, Value::Number(number));
            assert_eq!(encode(&value), text);
            assert_eq!(decode(&encode(&value)).unwrap(), value);
        }
        assert_ne!(decode("1").unwrap(), decode("1.0").unwrap());
        assert_eq!("1e9999".parse::<Number>().unwrap().as_f64(), None);
    }

    #[test]
    fn an_object_encodes_as_the_value_that_holds_it() {
        for text in [
            "{}",
            r#"{"b":[1.0,-18446744073709551617,null],"a":{"é🙂":"\n"},"c":true}"#,
        ] {
            let Value::Object(values) = decode(text).unwrap() else {
                panic!("object fixture")
            };
            assert_eq!(
                encode_object(&values),
                encode(&Value::Object(values.clone()))
            );
        }
    }

    #[test]
    fn nested_report_interpolation_preserves_wide_numbers_and_float_kinds() {
        let exact: Value = decode("92233720368547758081234567890").unwrap();
        let mut report = crate::json!({"keys": [{"ID":1.0}], "label":"é\n\""});
        report["context"] = Value::Object(Map::from([("actual".into(), exact)]));
        let encoded = encode(&report);
        assert_eq!(encoded, "{\"context\":{\"actual\":92233720368547758081234567890},\"keys\":[{\"ID\":1.0}],\"label\":\"é\\n\\\"\"}");
        assert_eq!(decode(&encoded).unwrap(), report);
    }

    #[test]
    fn project_walks_the_typed_serde_tree_without_text() {
        #[derive(Serialize)]
        struct Sample {
            count: u64,
            label: String,
            nested: Vec<Option<bool>>,
        }
        let value = project(&Sample {
            count: u64::MAX,
            label: "é".into(),
            nested: vec![Some(true), None],
        })
        .unwrap();
        assert_eq!(
            value["count"].as_number().unwrap().as_str(),
            "18446744073709551615"
        );
        assert_eq!(value["label"], "é");
        assert_eq!(value["nested"][0], true);
        assert_eq!(value["nested"][1], Value::Null);
        assert_eq!(
            project(&1.0f64).unwrap(),
            Value::Number("1.0".parse().unwrap())
        );
        assert_eq!(
            project(&-0.0f64).unwrap(),
            Value::Number("-0.0".parse().unwrap())
        );
    }

    #[test]
    fn serde_private_token_shaped_objects_remain_ordinary_objects() {
        for text in [
            r#"{"$serde_json::private::Number":"99999999999999999999"}"#,
            r#"{"$serde_json::private::RawValue":"[1,2]"}"#,
        ] {
            let value: Value = decode(text).unwrap();
            assert!(value.is_object());
            assert_eq!(encode(&value), text);
        }
    }

    #[test]
    fn recursive_values_keep_the_existing_json_deserializer_boundary() {
        for depth in [0, 1, 126, 127, 128, 129, 130] {
            for leaf in ["0", "[]", "{}"] {
                for (opening, closing) in [("[", "]"), ("{\"nested\":", "}")] {
                    let text = format!("{}{leaf}{}", opening.repeat(depth), closing.repeat(depth));
                    assert_eq!(
                        decode(&text).is_ok(),
                        serde_json::from_str::<serde_json::Value>(&text).is_ok(),
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
        let mut context = Value::Object(Map::new());
        context["context"] = value.clone();
        assert_eq!(context["context"], value);
        let text = format!("{}{DIGITS}{}", "[".repeat(132), "]".repeat(132));
        assert_eq!(encode(&value), text);
        assert!(decode(&text).is_err());
    }

    #[test]
    fn validated_walk_retains_escaped_keys_nested_values_and_duplicate_behavior() {
        let value: Value = decode(r#" { "\u96ea\ud83d\ude00": [null, true, false, {}, [], {"q\"\\":"雪\ud83d\ude00"}], "same": 1, "same": 2, "wide": -184467440737095516160000, "exp": 1E+9999 } "#).unwrap();
        assert_eq!(value["雪😀"][0], Value::Null);
        assert_eq!(value["雪😀"][1], true);
        assert_eq!(value["雪😀"][2], false);
        assert_eq!(value["雪😀"][3], crate::json!({}));
        assert_eq!(value["雪😀"][4], crate::json!([]));
        assert_eq!(value["雪😀"][5]["q\"\\"], "雪😀");
        assert_eq!(value["same"], 2);
        assert_eq!(
            value["wide"].as_number().unwrap().as_str(),
            "-184467440737095516160000"
        );
        assert_eq!(value["exp"].as_number().unwrap().as_str(), "1E+9999");
        assert_eq!(decode(&encode(&value)).unwrap(), value);
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
            assert!(decode(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn scalar_context_wire_spelling_matches_the_pre_move_bytes() {
        use yamaa_core::value::Value as Scalar;
        let context = yamaa_core::json::Map::from([
            ("bool".to_string(), Value::from(Scalar::Bool(true))),
            (
                "date".to_string(),
                Value::from(Scalar::Date("2025-01-02".parse().unwrap())),
            ),
            (
                "datetime".to_string(),
                Value::from(Scalar::DateTime("2025-01-02T03:04".parse().unwrap())),
            ),
            ("float".to_string(), Value::from(Scalar::float(1.0))),
            ("max".to_string(), Value::from(Scalar::Int(i64::MAX))),
            ("min".to_string(), Value::from(Scalar::Int(i64::MIN))),
            ("missing".to_string(), Value::from(Scalar::Missing)),
            ("negzero".to_string(), Value::from(Scalar::float(-0.0))),
            ("text".to_string(), Value::from(Scalar::Str("001".into()))),
        ]);
        let tree = Value::Object(context);
        assert_eq!(
            encode(&tree),
            r#"{"bool":true,"date":"2025-01-02","datetime":"2025-01-02T03:04:00","float":1.0,"max":9223372036854775807,"min":-9223372036854775808,"missing":null,"negzero":-0.0,"text":"001"}"#,
        );
    }
}
