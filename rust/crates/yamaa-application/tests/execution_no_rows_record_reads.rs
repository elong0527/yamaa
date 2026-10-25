#![cfg(feature = "yaml")]
//! Without row templates, each key is computed from one base record
//! (COLUMN-7). A key derived by a window gives each record its own row, so
//! every column then reads that one record (ROW-8); otherwise a key that
//! reads other records, an aggregate or an `odm` read, fails before any data
//! is read, as does a key that reads a non-key column. The tests that wrote
//! files, ran one specification and compared what it reported are cases of
//! `tests/fixtures/diagnostics/row_scopes.json` (#3798), with their build
//! routes and save results in `migrated_api_contracts.json`; the test left
//! reads a benchmark's own files.

/// Already data: `schema-window-key-odm-records`' own files, run by
/// `support::benchmarks::reproduce`. Every ODM record is its own row, and an
/// item read takes the scope of the record's own item group; the reads of
/// another item group occurrence at the same repeat key are the
/// `a_window_key_odm_record_...` cases of `row_scopes.json`.
#[test]
fn a_window_key_gives_every_odm_record_its_own_item_group() {
    crate::support::benchmarks::reproduce("schema-window-key-odm-records");
}
