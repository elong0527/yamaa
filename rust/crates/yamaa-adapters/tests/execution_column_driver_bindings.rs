#![cfg(feature = "yaml")]
//! A column-level derivation completes the rows of every row template that
//! leaves the column to the column phase, each row from the record that built
//! it (#2829), binding once for each driver those templates read (LOOKUP-1).
//! The family's cases are `fixtures/diagnostics/column_driver_bindings.json`;
//! this file keeps the benchmark that states it.

/// Already data: `schema-column-default-own-record`'s own expected output.
/// The subject template states the start date itself; the event and
/// medication rows complete it from the column, inside an operation, each
/// from its own record, and join the other input on the key it declares.
#[test]
fn each_driver_reads_its_own_record_as_its_benchmark_states() {
    crate::support::benchmarks::reproduce("schema-column-default-own-record");
}
