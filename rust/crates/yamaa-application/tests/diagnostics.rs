#![cfg(all(feature = "yaml", feature = "parquet", feature = "odm"))]
//! The replay of the application's expected outcomes written as data (#3691,
//! #3798): every case of `tests/fixtures/diagnostics/` reports its authored
//! issues, and the tables and saved files it states, through the operation
//! and input it names. The cases read YAML documents, Parquet artifacts and
//! ODM XML files, so the replay runs where the three readers are built.

#[test]
fn every_case_reports_what_it_states() {
    crate::support::diagnostic::replay::replay();
}
