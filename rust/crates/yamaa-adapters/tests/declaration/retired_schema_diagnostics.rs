use serde_json::{json, Value};
use yamaa_adapters::{
    shipped_schema,
    specification_diagnostics::capture_failure,
    specification_source::{CapturedSchema, Error, Source},
};

const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
const ENVIRONMENT_HINT: &str = "replace the retired environment format with language, lock and functions definitions containing function, description, params, returns and tests (REQ-1080, REQ-1083); see benchmarks/adam-adsl-bmi";

fn diagnostics(schema: &std::sync::Arc<CapturedSchema>, text: &str) -> Vec<Value> {
    let error = schema
        .prepare_structural(Source {
            identity: "held/spec.yaml".into(),
            bytes: text.as_bytes().to_vec(),
        })
        .unwrap_err();
    assert!(matches!(error, Error::Findings(_)));
    let encoded: Value = serde_json::from_str(&capture_failure(&error)).unwrap();
    assert_eq!(encoded["outcome"]["status"], "invalid");
    encoded["outcome"]["diagnostics"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn retired_verification_names_keep_findings_and_project_boolean_replacement_hints() {
    let schema = shipped_schema::capture().unwrap();
    let found = diagnostics(
        &schema,
        &(VALID.to_owned() + "verifications: [{implies: {when: 'ID = 1', then: 'ID > 0'}}]\n"),
    );
    assert_eq!(
        found,
        vec![json!({
            "phase": "validation", "condition": "unknown_operation", "requirement": "REQ-0286",
            "spec_paths": ["verifications[0].implies"],
            "context": {"operation": "implies", "registry": "dataset_verifications", "retired": true,
                        "suggestion": "replace implies with assert using when and require (REQ-0383)"}
        })]
    );
    let found = diagnostics(
        &schema,
        &(VALID.to_owned() + "verifications: [{assert: {expr: 'ID > 0'}}]\n"),
    );
    assert_eq!(
        found,
        vec![
            json!({"phase": "validation", "condition": "missing_required_field", "requirement": "REQ-0285",
            "spec_paths": ["verifications[0].assert.require"],
            "context": {"class": "assert", "field": "require", "retired": true, "suggestion": "replace expr with require (REQ-0384)"}}),
            json!({"phase": "validation", "condition": "unknown_field", "requirement": "REQ-0285",
            "spec_paths": ["verifications[0].assert.expr"],
            "context": {"class": "assert", "field": "expr", "retired": true, "suggestion": "replace expr with require (REQ-0384)"}})
        ]
    );
    // Current spellings still validate. A typo still receives the generic finding.
    schema
        .prepare_structural(Source {
            identity: "current.yaml".into(),
            bytes: (VALID.to_owned()
                + "verifications: [{assert: {when: 'ID = 1', require: 'ID > 0'}}]\n")
                .into_bytes(),
        })
        .unwrap();
    let typo = diagnostics(
        &schema,
        &(VALID.to_owned() + "verifications: [{imples: {}}]\n"),
    );
    assert_eq!(
        typo[0]["context"],
        json!({"operation": "imples", "registry": "dataset_verifications"})
    );
}

#[test]
fn retired_call_field_points_to_the_current_call_without_admitting_an_alias() {
    let schema = shipped_schema::capture().unwrap();
    let found = diagnostics(
        &schema,
        &VALID.replace(
            "derivation: SRC.ID",
            "derivation: {function: {name: count, contract_version: '1.0.0', args: {}}}",
        ),
    );
    assert_eq!(
        found,
        vec![json!({
            "phase": "validation", "condition": "unknown_field", "requirement": "REQ-0285",
            "spec_paths": ["columns.ID.derivation.function.contract_version"],
            "context": {"class": "function", "field": "contract_version", "retired": true,
                        "suggestion": "delete contract_version; a call takes name and args only (REQ-1085)"}
        })]
    );
}

#[test]
fn legacy_environment_findings_share_one_migration_explanation_and_keep_order() {
    let schema = shipped_schema::capture_environment().unwrap();
    let found = diagnostics(&schema, "schema_version: '1.0'\nversion: '1.0.0'\nruntime: {language: r, artifact: {reference: org.example/bmi-r:1.0.0}}\nfunctions:\n  bmi: {contract: contracts.yaml, binding: {call: mypkg::bmi}, conformance: conformance/bmi.yaml}\n");
    let paths = found
        .iter()
        .map(|d| d["spec_paths"][0].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "functions.bmi.function",
            "functions.bmi.description",
            "functions.bmi.params",
            "functions.bmi.returns",
            "functions.bmi.tests",
            "functions.bmi.contract",
            "functions.bmi.binding",
            "functions.bmi.conformance",
            "version",
            "runtime"
        ]
    );
    for (index, finding) in found.iter().enumerate() {
        assert_eq!(
            finding["condition"],
            if index < 5 {
                "missing_required_field"
            } else {
                "unknown_field"
            }
        );
        assert_eq!(finding["requirement"], "REQ-0285");
        assert_eq!(finding["context"]["retired"], true);
        assert_eq!(finding["context"]["suggestion"], ENVIRONMENT_HINT);
    }
    let typo = diagnostics(&schema, "schema_version: '1.0'\nruntme: {}\n");
    assert_eq!(
        typo[0]["context"],
        json!({"class": "environment_class", "field": "runtme"})
    );
    // A normal incomplete definition does not get a retirement label.
    let incomplete = diagnostics(&schema, "schema_version: '1.0'\nfunctions: {bmi: {}}\n");
    assert_eq!(incomplete.len(), 5);
    assert!(incomplete
        .iter()
        .all(|d| d["context"].get("retired").is_none()));
}
