#![cfg(feature = "yaml")]
//! Retired 0.4 spellings are refused before study data, with actionable hints.
//! The retired temporal verbs are cases of
//! `fixtures/diagnostics/one_spelling.json`.
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

/// Seam: parts of a hint whose whole text no authored literal holds, the
/// same hint from the layer admission API (`validate_layer`), and findings
/// found among others by their field. No replay case states a part of a
/// context value or calls that API.
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

    // An ordinary incomplete specification still asks for its root fields.
    let asks_for_domain = |findings: &[Value]| {
        findings.iter().any(|f| {
            f["condition"] == "missing_required_field" && f["spec_paths"] == json!(["domain"])
        })
    };
    assert!(asks_for_domain(&diagnostics(HEADER)));
}
