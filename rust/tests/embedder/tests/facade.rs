//! The five compact-flow controls of the vendored parser
//! (`rust/vendor/saphyr-parser/tests/compact_flow_pairs.rs`), run through the
//! facade's public YAML admission in an outside graph. Each compact flow input
//! must admit the value that the block YAML beside it writes out by hand. A
//! published saphyr-parser through 0.2.2 fails the first four (#2583).

use std::sync::Arc;

use yamaa::{CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, Request};

/// Admit both documents through the facade and compare their values. A
/// document that admission refuses never compares equal, not even to itself.
fn same_value(flow: &str, block: &str) -> bool {
    yamaa::style_same_value(flow.as_bytes(), block.as_bytes())
}

#[test]
fn compact_pair_keeps_every_entry_of_its_nested_mapping() {
    let block = "- k:\n    a: 1\n    b: 2\n";
    for input in [
        "[k: {a: 1, b: 2}]",
        "[{k: {a: 1, b: 2}}]",
        "[? k : {a: 1, b: 2}]",
    ] {
        assert!(same_value(input, block), "{input}");
    }
}

#[test]
fn preceding_mapping_does_not_change_compact_pair_events() {
    let block = "before:\n  x: 0\nvalues:\n  - k:\n      a: 1\n      b: 2\n";
    for input in [
        "before: {x: 0}\nvalues: [k: {a: 1, b: 2}]",
        "before:\n  x: 0\nvalues: [k: {a: 1, b: 2}]",
    ] {
        assert!(same_value(input, block), "{input}");
    }
}

#[test]
fn compact_pairs_survive_nested_sequences_and_mapping_values() {
    let block =
        "- - k:\n      a:\n        - 1\n        - 2\n      b:\n        c: 3\n        d: 4\n";
    assert!(same_value("[[k: {a: [1, 2], b: {c: 3, d: 4}}]]", block));
}

#[test]
fn each_sequence_entry_has_its_own_pair_state() {
    let block = "- k:\n    a: 1\n    b: 2\n- j:\n    c: 3\n    d: 4\n";
    for input in [
        "[k: {a: 1, b: 2}, j: {c: 3, d: 4}]",
        "[? k : {a: 1, b: 2}, j: {c: 3, d: 4}]",
        "[{k: {a: 1, b: 2}}, j: {c: 3, d: 4}]",
    ] {
        assert!(same_value(input, block), "{input}");
    }
}

#[test]
fn mismatched_or_missing_closers_are_refused_without_panicking() {
    for input in [
        "[k: {a: 1, b: 2]",
        "[k: {a: 1, b: 2]]",
        "[k: {a: 1, b: 2}}]",
        "[k: {a: 1, b: 2}, j: [1, 2}",
        "{x: [k: {a: 1, b: 2}], y: 3",
    ] {
        assert!(!same_value(input, input), "{input}");
    }
}

/// The #2196 specification: block style throughout, with one compact flow
/// verification. An embedder built without the fix reported it as invalid
/// YAML at the flow pair, while the Python and R packages admitted it.
#[test]
fn check_admits_a_compact_flow_verification_in_a_block_specification() {
    let specification = "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: input/dm.csv\noutput:\n  path: adsl.csv\n  columns: [USUBJID]\ncolumns:\n  - name: USUBJID\n    type: str\n    derivation: DM.USUBJID\nverifications: [row_count: {min: 3, max: 3}]\n";
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:1",
        Arc::from(specification.as_bytes()),
    );
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "embedder".into(), "0".into())
            .with_resolver(resolver),
    );
    let report = engine.check(&Request::new("spec.yaml"), &CancellationToken::new());
    assert!(report.is_empty(), "{:?}", report.diagnostics);
}
