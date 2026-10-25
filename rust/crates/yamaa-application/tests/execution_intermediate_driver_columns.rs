#![cfg(feature = "yaml")]
//! A column-level derivation completed only for rows a named intermediate
//! drives (ROW-2). The column binds against that driver the way its row
//! template does, and a field it derives, or a driver read beside an
//! aggregate, is refused before any data is read. The family's cases are
//! `fixtures/diagnostics/intermediate_driver_columns.json`; this file keeps
//! the benchmark that states it.

/// Already data: `schema-intermediate-driver-columns`'s own expected output.
/// A literal, the driver's qualifier, which reads the record that built the
/// row, another input's join on the keys it carries and a lookup selected
/// through its key each complete every row the selection drives.
#[test]
fn columns_stated_once_complete_every_intermediate_driven_row() {
    crate::support::benchmarks::reproduce("schema-intermediate-driver-columns");
}
