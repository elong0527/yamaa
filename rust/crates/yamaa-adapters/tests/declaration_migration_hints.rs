#![cfg(feature = "yaml")]
//! Retired 0.4 spellings are refused before study data, with actionable hints.
use yamaa_adapters::{
    json,
    report_storage::diagnostic_value,
    shipped_schema,
    specification_diagnostics::{capture_failure, Rejection},
    specification_source::Source,
};
use yamaa_core::json::Value;

fn diagnostics(text: &str) -> Vec<Value> {
    let error = shipped_schema::capture()
        .unwrap()
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap_err();
    match capture_failure(&error) {
        Rejection::Invalid(findings) => findings.iter().map(diagnostic_value).collect(),
        other => panic!("expected authored findings, got {other:?}"),
    }
}

#[test]
fn a_retired_parent_list_names_the_chain_without_missing_root_fields() {
    let raw = "schema_version: '1.0'\nparents: [organization.yaml, compound.yaml]\n";
    let findings = diagnostics(raw);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["condition"], "unknown_field");
    assert_eq!(findings[0]["spec_paths"], json!(["parents"]));
    assert_eq!(findings[0]["context"]["field"], "parents");
    assert_eq!(findings[0]["context"]["class"], "root_class");
    let hint = findings[0]["context"]["hint"].as_str().unwrap();
    assert!(hint.contains("parent: compound.yaml in the entry"));
    assert!(hint.contains("parent: organization.yaml in compound.yaml"));

    // Inheritance admission reports the same hint before following a parent.
    let decoded = yamaa_adapters::yaml_decode::decode_yaml(raw.as_bytes()).unwrap();
    let error = shipped_schema::capture()
        .unwrap()
        .structure()
        .normalize_layer(&decoded.document)
        .unwrap_err();
    let yamaa_core::schema::NormalizationError::Invalid(layer_findings) = error else {
        panic!("expected a layer finding: {error:?}")
    };
    assert_eq!(layer_findings.len(), 1);
    assert_eq!(layer_findings[0].path, "parents");
    assert_eq!(layer_findings[0].definition().condition, "unknown_field");
    assert!(layer_findings[0].context.iter().any(|(key, value)| {
        *key == "hint" && *value == yamaa_core::schema::SchemaContext::Text(hint.into())
    }));

    // The wrong parent spelling does not suppress faults in written values.
    let findings = diagnostics(&format!("{raw}unrelated: 1\ncolumns: [{{type: str}}]\n"));
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
    let findings = diagnostics("schema_version: '1.0'\n");
    assert!(findings.iter().any(|f| {
        f["condition"] == "missing_required_field" && f["spec_paths"] == json!(["domain"])
    }));
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
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: missing.csv}}\noutput: {{path: out.csv, columns: [ID]}}\ncolumns:\n  - name: ID\n    type: str\n    derivation: {{{old}: {{source: SRC.VALUE}}}}\n"
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
