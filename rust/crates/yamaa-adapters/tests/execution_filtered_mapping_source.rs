#![cfg(feature = "yaml")]
//! A mapping source carries the same filter, selection, and absent value as
//! a plain source binding, so a filtered raw column no longer needs a
//! helper column that exists only to feed the dictionary lookup. The
//! family's specifications are data in
//! `tests/fixtures/diagnostics/key_selection.json` (#3798); the benchmark
//! that pins a filter alone stays here.

/// Already data: `sdtm-dm-basic` maps SEX through a mapping source whose
/// filter selects the subject's sex item among its other items, and
/// `support::benchmarks::reproduce` compares its authored `expected/` bytes.
#[test]
fn mapping_source_with_a_filter_alone_reads_the_selected_record() {
    crate::support::benchmarks::reproduce("sdtm-dm-basic");
}
