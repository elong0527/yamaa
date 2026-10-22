//! The compact-flow controls of the vendored parser
//! (`rust/vendor/saphyr-parser/tests/compact_flow_pairs.rs`), run through the
//! facade's use cases in an outside graph. Each control writes a
//! specification's `verifications` in compact flow style and again in block
//! style; the two must resolve to the same verifications, and a flow list
//! with a mismatched or missing closer must be refused as YAML, without a
//! panic. A published saphyr-parser through 0.2.2 fails the compact pairs
//! (#2583).

use std::sync::Arc;

use yamaa::{CancellationToken, ContextValue, Engine, EngineOptions, HostLanguage, MemoryResolver};

/// A specification with one column, its `output` written as `output`, and
/// its `verifications` as `verifications`.
fn specification(output: &str, verifications: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, key: [ID]}}}}\n{output}\ncolumns:\n  - {{name: ID, type: str, label: Identifier, derivation: SRC.ID}}\n  - {{name: NAME, type: str, label: Name, derivation: SRC.NAME}}\nverifications:{verifications}\n"
    )
}

const OUTPUT: &str = "output:\n  path: out.csv\n  columns: [ID, NAME]";

/// An engine that serves `specification` from memory as `spec.yaml`.
fn engine(specification: &str) -> Engine {
    let resolver = Arc::new(MemoryResolver::new());
    resolver.insert(
        "/",
        "spec.yaml",
        "spec:1",
        Arc::from(specification.as_bytes()),
    );
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "embedder".into(), "0".into())
            .with_resolver(resolver),
    )
}

/// The verifications the specification resolves to.
fn verifications(specification: &str) -> ContextValue {
    let resolved = engine(specification)
        .spec_resolve("spec.yaml", None)
        .unwrap_or_else(|failure| panic!("{failure}"));
    let ContextValue::Map(artifact) = resolved else {
        panic!("a resolved specification is a map");
    };
    let ContextValue::List(entries) = &artifact["entries"] else {
        panic!("a resolved specification lists its entries");
    };
    let ContextValue::Map(entry) = &entries[0] else {
        panic!("an entry is a map");
    };
    let ContextValue::Map(composed) = &entry["specification"] else {
        panic!("an entry holds its composed specification");
    };
    composed["verifications"].clone()
}

/// Every flow spelling resolves to the verifications the block one does.
fn assert_same(flows: &[&str], block: &str) {
    let expected = verifications(&specification(OUTPUT, block));
    for flow in flows {
        let resolved = verifications(&specification(OUTPUT, &format!(" {flow}")));
        assert_eq!(resolved, expected, "{flow}");
    }
}

#[test]
fn compact_pair_keeps_every_entry_of_its_nested_mapping() {
    assert_same(
        &[
            "[row_count: {min: 1, max: 9}]",
            "[{row_count: {min: 1, max: 9}}]",
            "[? row_count : {min: 1, max: 9}]",
        ],
        "\n  - row_count:\n      min: 1\n      max: 9",
    );
}

#[test]
fn preceding_mapping_does_not_change_compact_pair_events() {
    let block = verifications(&specification(
        OUTPUT,
        "\n  - row_count:\n      min: 1\n      max: 9",
    ));
    let flow = verifications(&specification(
        "output: {path: out.csv, columns: [ID, NAME]}",
        " [row_count: {min: 1, max: 9}]",
    ));
    assert_eq!(flow, block);
}

#[test]
fn compact_pairs_keep_sequence_values() {
    assert_same(
        &["[row_count: {group_by: [ID, NAME], min: 1, max: 9}]"],
        "\n  - row_count:\n      group_by:\n        - ID\n        - NAME\n      min: 1\n      max: 9",
    );
}

#[test]
fn each_sequence_entry_has_its_own_pair_state() {
    assert_same(
        &[
            "[row_count: {min: 1, max: 9}, all_or_none: {columns: [ID, NAME]}]",
            "[? row_count : {min: 1, max: 9}, all_or_none: {columns: [ID, NAME]}]",
            "[{row_count: {min: 1, max: 9}}, all_or_none: {columns: [ID, NAME]}]",
        ],
        "\n  - row_count:\n      min: 1\n      max: 9\n  - all_or_none:\n      columns: [ID, NAME]",
    );
}

#[test]
fn mismatched_or_missing_closers_are_refused_without_panicking() {
    for flow in [
        "[row_count: {min: 1, max: 9]",
        "[row_count: {min: 1, max: 9]]",
        "[row_count: {min: 1, max: 9}}]",
        "[row_count: {min: 1, max: 9}, all_or_none: [ID}",
    ] {
        let check = engine(&specification(OUTPUT, &format!(" {flow}")))
            .spec_check("spec.yaml", None, &CancellationToken::new())
            .unwrap();
        let conditions = check
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.clone())
            .collect::<Vec<_>>();
        assert_eq!(conditions, ["invalid_yaml"], "{flow}");
    }
}

/// The #2196 specification: block style throughout, with one compact flow
/// verification. An embedder built without the fix reported it as invalid
/// YAML at the flow pair, while the Python and R packages admitted it.
#[test]
fn check_admits_a_compact_flow_verification_in_a_block_specification() {
    let specification = "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: input/dm.csv\noutput:\n  path: adsl.csv\n  columns: [USUBJID]\ncolumns:\n  - name: USUBJID\n    type: str\n    derivation: DM.USUBJID\nverifications: [row_count: {min: 3, max: 3}]\n";
    let check = engine(specification)
        .spec_check("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    assert!(check.diagnostics().is_empty(), "{:?}", check.diagnostics());
}
