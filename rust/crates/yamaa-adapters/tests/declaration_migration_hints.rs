#![cfg(feature = "yaml")]
//! Retired 0.4 spellings are refused before study data, with actionable hints.
use yamaa_adapters::{
    json,
    report_storage::diagnostic_value,
    shipped_schema,
    specification_diagnostics::{inheritance_failure_ref, Rejection},
    specification_source::Source,
};
use yamaa_core::json::Value;
use yamaa_core::language::{FileType, LANGUAGE_VERSION};

/// The 1.0 header a specification starts with.
const HEADER: &str = "yamaa: \"1.0\"\ntype: spec\n";

fn diagnostics(text: &str) -> Vec<Value> {
    let error = shipped_schema::prepare(
        Source {
            identity: "spec.yaml".into(),
            bytes: text.as_bytes().to_vec(),
        },
        "spec.yaml".into(),
        &mut crate::support::inheritance::Files,
    )
    .unwrap_err();
    match inheritance_failure_ref(&error).unwrap() {
        Rejection::Invalid(findings) => findings.iter().map(diagnostic_value).collect(),
        other => panic!("expected authored findings, got {other:?}"),
    }
}

#[test]
fn a_retired_parent_list_names_the_chain_without_missing_root_fields() {
    let raw = format!("{HEADER}parents: [organization.yaml, compound.yaml]\n");
    let findings = diagnostics(&raw);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["condition"], "unknown_field");
    assert_eq!(findings[0]["spec_paths"], json!(["parents"]));
    assert_eq!(findings[0]["context"]["field"], "parents");
    assert_eq!(findings[0]["context"]["class"], "root_class");
    let hint = findings[0]["context"]["hint"].as_str().unwrap();
    assert!(hint.contains("parent: compound.yaml in the entry"));
    assert!(hint.contains("parent: organization.yaml in compound.yaml"));
    assert!(hint.contains("organization.yaml has no parent"));

    // Inheritance admission reports the same hint before following a parent.
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(raw.as_bytes()).unwrap();
    let layer_findings = shipped_schema::capture_versioned(FileType::Spec)
        .unwrap()
        .structure()
        .at_version(LANGUAGE_VERSION)
        .validate_layer(&decoded.document)
        .unwrap();
    assert_eq!(layer_findings.len(), 1);
    assert_eq!(layer_findings[0].path, "parents");
    assert_eq!(layer_findings[0].definition().condition, "unknown_field");
    assert!(layer_findings[0].context.iter().any(|(key, value)| {
        *key == "hint" && *value == yamaa_core::schema::SchemaContext::Text(hint.into())
    }));

    // The wrong parent spelling does not suppress the layer's other faults.
    let findings = diagnostics(&format!("{raw}unrelated: 1\ncolumns: [{{type: str}}]\n"));
    assert!(findings
        .iter()
        .any(|f| f["spec_paths"] == json!(["unrelated"])));
    // A 0.5 file also reports a written column's missing name in the same
    // pass. 0.5-only until #3447: a 1.0 layer leaves requiredness to the
    // composed member (LAYER-7).
    let legacy = raw.replacen(HEADER, "schema_version: '1.0'\n", 1);
    let findings = diagnostics(&format!("{legacy}unrelated: 1\ncolumns: [{{type: str}}]\n"));
    assert!(findings
        .iter()
        .any(|f| f["spec_paths"] == json!(["unrelated"])));
    assert!(findings.iter().any(|f| {
        f["condition"] == "missing_required_field" && f["spec_paths"] == json!(["columns[0].name"])
    }));
    assert!(findings
        .iter()
        .filter(|f| f["condition"] == "missing_required_field")
        .all(|f| { f["spec_paths"][0].as_str().unwrap().starts_with("columns") }));

    // An ordinary incomplete specification still asks for its root fields.
    let asks_for_domain = |findings: &[Value]| {
        findings.iter().any(|f| {
            f["condition"] == "missing_required_field" && f["spec_paths"] == json!(["domain"])
        })
    };
    assert!(asks_for_domain(&diagnostics(HEADER)));
    // A 0.5 file asks for them beside an empty retired list too. 0.5-only
    // until #3447: a 1.0 layer defers its root fields' requiredness to the
    // resolved specification, which a layer that writes an unknown field
    // never reaches (LAYER-4).
    for parents in ["parents: []\n", "parents: null\n"] {
        assert!(asks_for_domain(&diagnostics(&format!(
            "schema_version: '1.0'\n{parents}"
        ))));
    }
}

#[test]
fn retired_temporal_verbs_name_the_operation_and_declared_type() {
    for (old, new, kind) in [
        ("date_diff", "diff", "date"),
        ("datetime_diff", "diff", "datetime"),
        ("date_shift", "shift", "date"),
        ("datetime_shift", "shift", "datetime"),
        ("date_impute", "impute", "date"),
        ("datetime_impute", "impute", "datetime"),
        ("date_precision", "precision", "date"),
        ("datetime_precision", "precision", "datetime"),
    ] {
        // The retired payload is not interpreted, nor is missing.csv opened.
        let raw = format!(
            "{HEADER}domain: TEST\nkeys: [ID]\ninput: {{SRC: missing.csv}}\noutput: {{path: out.csv, columns: [ID]}}\ncolumns:\n  - name: ID\n    type: str\n    derivation: {{{old}: {{source: SRC.VALUE}}}}\n"
        );
        let findings = diagnostics(&raw);
        assert_eq!(findings.len(), 1, "{old}: {findings:?}");
        assert_eq!(findings[0]["condition"], "unknown_operation");
        assert_eq!(
            findings[0]["spec_paths"],
            json!([format!("columns.ID.derivation.{old}")])
        );
        assert_eq!(findings[0]["context"]["operation"], old);
        assert_eq!(
            findings[0]["context"]["hint"],
            format!("write {new} with type: {kind}")
        );
    }
}
