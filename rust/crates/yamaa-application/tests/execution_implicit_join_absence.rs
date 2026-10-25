#![cfg(feature = "yaml")]
//! A read through an implicit join that finds no record (LOOKUP-24), whose
//! counts, answers and failures are the cases of
//! `fixtures/diagnostics/lookup_model.json` (#3798). What stays here is the
//! benchmark that states a required input's failure whole.

/// Already data: `negative-input-require-match`'s own `expected/error.yaml`,
/// which `support::benchmarks::reproduce` compares. It reads its required
/// subject-level input through a concise read for an event whose subject has
/// no record, and states the whole finding.
#[test]
fn an_input_requiring_a_match_fails_a_concise_read_as_unmatched_key() {
    crate::support::benchmarks::reproduce("negative-input-require-match");
}
