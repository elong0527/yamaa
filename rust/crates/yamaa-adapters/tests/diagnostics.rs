#![cfg(all(feature = "yaml", feature = "parquet"))]
//! The replay of the adapters' expected outcomes written as data (#3691,
//! #3798): every case of `tests/fixtures/diagnostics/` reports its authored
//! issues, and the tables and saved files it states, through the operation
//! and input it names. The cases read YAML documents and Parquet artifacts,
//! so the replay runs where both readers are built.

#[test]
fn every_case_reports_what_it_states() {
    crate::support::diagnostic::replay::replay();
}
