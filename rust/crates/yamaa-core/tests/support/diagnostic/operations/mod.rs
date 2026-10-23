//! The readers of the operations the diagnostic cases name (#3691). Each
//! reads a case's `input` as the arguments of one core entry point, calls it
//! and returns every issue it reports, in order, or none when it accepts the
//! input. A reader asserts nothing about what it returns: the replay compares
//! that with the case's authored rows.
use super::json::Json;
use yamaa_core::{
    diagnostic::Diagnostic,
    temporal::{Date, Precision},
    value::{ColumnType, Value, ValueType},
};

mod sources;
mod temporal;
mod text;
mod values;

/// The issues `operation` reports for `input`.
pub fn run(operation: &str, input: &Json) -> Vec<Diagnostic> {
    match operation {
        "csv_profile" => sources::csv_profile(input),
        "csv_types" => sources::csv_types(input),
        "cut" => values::cut(input),
        "dataset_finding" => values::dataset_finding(input),
        "impute" => temporal::impute(input),
        "impute_policy" => temporal::impute_policy(input),
        "mapping" => values::mapping(input),
        "parquet_columns" => sources::parquet_columns(input),
        "parquet_value" => sources::parquet_value(input),
        "predicate" => values::predicate(input),
        "sas_source" => sources::sas_source(input),
        "str_case" => text::str_case(input),
        "str_extract" => text::str_extract(input),
        "str_extract_apply" => text::str_extract_apply(input),
        "str_extract_unknown" => text::str_extract_unknown(input),
        "str_extract_unmatched" => text::str_extract_unmatched(input),
        "str_pad" => text::str_pad(input),
        "str_template" => text::str_template(input),
        "str_template_missing" => text::str_template_missing(input),
        "yaml_source" => sources::yaml_source(input),
        other => panic!("no reader for the operation {other:?}"),
    }
}

/// The text member `key` of `input`.
#[track_caller]
fn text<'a>(input: &'a Json, key: &str) -> &'a str {
    input.field(key).text()
}

/// A value as a case writes it: `null` is missing, a string is `str`, an
/// integer token is `int` and a token with a fraction or exponent `float`, a
/// Boolean is `bool`, and `{"date": "YYYY-MM-DD"}` is that date, collected
/// to the day or to the `precision` written beside it.
#[track_caller]
fn value(json: &Json) -> Value {
    match json {
        Json::Null => Value::Missing,
        Json::String(text) => Value::Str(text.clone()),
        Json::Bool(value) => Value::Bool(*value),
        Json::Number(token) if token.contains(['.', 'e', 'E']) => {
            Value::float(token.parse().expect("a finite float"))
        }
        Json::Number(_) => Value::Int(json.integer()),
        Json::Object(_) => {
            let text = json.field("date").text();
            let Some(precision) = json.get("precision") else {
                return Value::Date(text.parse().expect("a date"));
            };
            let parts = text
                .split('-')
                .map(|part| part.parse::<u16>().expect("a date part"))
                .collect::<Vec<_>>();
            let [year, month, day] = parts[..] else {
                panic!("a date is written YYYY-MM-DD: {text}")
            };
            let precision = match precision.text() {
                "year" => Precision::Year,
                "month" => Precision::Month,
                "day" => Precision::Day,
                other => panic!("no date precision {other:?}"),
            };
            Value::Date(Date::new(year, month as u8, day as u8, precision).expect("a date"))
        }
        Json::Array(_) => panic!("a value is not an array: {json:?}"),
    }
}

/// A column type as the language spells it.
#[track_caller]
fn column_type(json: &Json) -> ColumnType {
    match json.text() {
        "str" => ColumnType::Str,
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        other => panic!("no column type {other:?}"),
    }
}

/// A value type as the language spells it.
#[track_caller]
fn value_type(json: &Json) -> ValueType {
    match json.text() {
        "str" => ValueType::Str,
        "int" => ValueType::Int,
        "float" => ValueType::Float,
        "bool" => ValueType::Bool,
        "date" => ValueType::Date,
        "datetime" => ValueType::DateTime,
        other => panic!("no value type {other:?}"),
    }
}
