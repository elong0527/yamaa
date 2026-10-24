#![cfg(all(feature = "yaml", feature = "parquet"))]
//! The replay of the adapters' expected diagnostics written as data (#3691):
//! every case of `tests/fixtures/diagnostics/` reports its authored issues,
//! through the operation and input it names. The cases read YAML documents
//! and Parquet artifacts, so the replay runs where both readers are built.

#[test]
fn every_case_reports_its_authored_issues() {
    crate::support::diagnostic::replay::replay();
}
