//! The readers of the operations the diagnostic cases name (#3691). Each
//! reads a case's `input` as the arguments of one core entry point, refusing
//! a member it does not take, calls it and returns every issue it reports, in
//! order, or none when it accepts the input. A reader asserts nothing about
//! what it returns: the replay compares that with the case's authored rows.
//!
//! Each part of the move owns one module: its readers and the fixture files
//! its cases are in, so two parts never edit the same lines here.
use super::json::Json;
use yamaa_core::{
    diagnostic::Diagnostic,
    temporal::{Date, Precision},
    value::{ColumnType, Value, ValueType},
};

/// A fixture file of `tests/fixtures/diagnostics/`, read at compile time, so
/// renaming or deleting it fails the build rather than dropping its cases.
pub struct File {
    pub name: &'static str,
    pub text: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        crate::support::diagnostic::operations::File {
            name: $name,
            text: include_str!(concat!("../../../fixtures/diagnostics/", $name)),
        }
    };
}

mod document;
mod sources;
mod temporal;
mod text;
mod values;

// The parts that follow (#3691): A1 submission and the front end, A2
// preflight and binding, B1 the other declaration tables, B2 the other
// findings, C the registry walk's own records and the project environment's.
mod binding;
mod declarations;
mod findings;
mod registry;
mod submission;

type Reader = fn(&str, &Json) -> Option<Vec<Diagnostic>>;

/// Each module's readers, by the operations it names, and its files.
const MODULES: [(Reader, &[File]); 10] = [
    (document::run, document::FILES),
    (sources::run, sources::FILES),
    (temporal::run, temporal::FILES),
    (text::run, text::FILES),
    (values::run, values::FILES),
    (submission::run, submission::FILES),
    (binding::run, binding::FILES),
    (declarations::run, declarations::FILES),
    (findings::run, findings::FILES),
    (registry::run, registry::FILES),
];

/// The issues `operation` reports for `input`.
pub fn run(operation: &str, input: &Json) -> Vec<Diagnostic> {
    MODULES
        .iter()
        .find_map(|(run, _)| run(operation, input))
        .unwrap_or_else(|| panic!("no reader for the operation {operation:?}"))
}

/// Every fixture file the modules read.
pub fn files() -> impl Iterator<Item = &'static File> {
    MODULES.iter().flat_map(|(_, files)| files.iter())
}

/// The text member `key` of `input`.
#[track_caller]
fn text<'a>(input: &'a Json, key: &str) -> &'a str {
    input.field(key).text()
}

/// A value as a case writes it: `null` is missing, a string is `str`, an
/// integer token is `int` and a token with a fraction or exponent `float`, a
/// Boolean is `bool`, `{"date": "YYYY-MM-DD"}` is that date, collected to the
/// day or to the `precision` written beside it, and `{"datetime": ...}` is
/// the datetime its text spells.
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
        Json::Object(_) if json.get("datetime").is_some() => Value::DateTime(
            text(json.closed(&["datetime"]), "datetime")
                .parse()
                .expect("a datetime"),
        ),
        Json::Object(_) => {
            let text = text(json.closed(&["date", "precision"]), "date");
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
