#![cfg(feature = "yaml")]
//! A matched aggregate reads the current row through its match values, its
//! range value and its derive bindings (LOOKUP-5). The tests that wrote
//! files, ran one specification and compared what it reported are the cases
//! of `tests/fixtures/diagnostics/aggregate_key_reads.json` (#3798), with
//! their build routes and save results in `migrated_api_contracts.json`; the
//! test left reads a benchmark's own files.

/// Already data: `adam-adpp-carryover-exclusion`'s own files, run by
/// `support::benchmarks::reproduce`. Without row templates an aggregate key
/// that reads the base record builds; one that reads another input is the
/// `without_row_templates_a_key_that_reads_another_input_is_refused` case.
#[test]
fn without_row_templates_an_aggregate_key_reads_the_base_record() {
    crate::support::benchmarks::reproduce("adam-adpp-carryover-exclusion");
}
