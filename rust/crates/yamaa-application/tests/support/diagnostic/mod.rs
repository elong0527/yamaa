//! The application's expected outcomes written as data, and the one replay that
//! reads them (#3691, #3798): `replay` reads the cases of
//! `tests/fixtures/diagnostics/` and compares what each reports with its
//! authored rows, tables and saved files, `form` holds the case form it
//! shares with core's replay, and `operations` holds the reader of each
//! operation a case names.
pub mod form;
pub mod operations;
pub mod replay;

/// Decode a quoted JSON member name with the fixture reader, while the
/// shared form reader keeps selected values as their original text.
pub fn json_string(text: &str, label: &str) -> String {
    match yamaa_adapters::json_wire::decode(text).unwrap_or_else(|error| panic!("{label}: {error}"))
    {
        yamaa_core::json::Value::String(text) => text,
        _ => panic!("{label}: expected a JSON string"),
    }
}
