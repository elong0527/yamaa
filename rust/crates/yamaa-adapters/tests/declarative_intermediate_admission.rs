//! Complete independent truth for applicable keys and declaration error precedence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-intermediate-admission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(
        &self,
        body: &str,
    ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml");
        fs::write(&specification, body).unwrap();
        let specification = specification.to_str().unwrap();
        file_application::domain(
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-intermediate-admission",
                specification,
                base_directory: ".",
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issues(
    result: Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>,
) -> Vec<serde_json::Value> {
    let records = match result {
        Ok(result) => result.issues().to_vec(),
        Err(records) => records,
    };
    records.iter().map(|issue| serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})).collect()
}
const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, PARAM]
input:
  SRC: {path: src.csv, types: {ID: int}}
  DONOR: {path: donor.csv, types: {ID: float, SEQ: int}}
output: {path: result.csv, columns: [ID, PARAM, A, B]}
intermediates:
  - id: P
    dataset: DONOR
    order_by: [DONOR.SEQ]
    keep: first
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: PARAM, type: str, derivation: {literal: only}}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
#[test]
fn omitted_key_uses_only_applicable_fields_and_keeps_one_record_for_every_read() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID\n2\n1\n3\n").unwrap();
    fs::write(
        fixture.0.join("donor.csv"),
        "SEQ,ID,A,B\n1,1,first,\n1,1,tied,second\n0,2,two,both\n",
    )
    .unwrap();
    let mut result = fixture.run(BODY).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,PARAM,A,B\n2,only,two,both\n1,only,first,\n3,only,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1},
        ])
    );
}
#[test]
fn rename_only_precedes_key_types_and_every_study_read() {
    let fixture = Fixture::new();
    let body = BODY.replace("    order_by: [DONOR.SEQ]\n    keep: first\n", "");
    assert_eq!(
        issues(fixture.run(&body)),
        vec![
            serde_json::json!({"phase":"validation", "condition":"rename_only_intermediate", "requirement":"REQ-1248", "spec_paths":["intermediates[0]"], "context":{"intermediate":"P","dataset":"DONOR"}})
        ]
    );
    assert!(!fixture.0.join("src.csv").exists());
}
#[test]
fn either_unpaired_selection_is_a_complete_declaration_finding() {
    let fixture = Fixture::new();
    for (line, declared, missing) in [
        ("    keep: first\n", "order_by", "keep"),
        ("    order_by: [DONOR.SEQ]\n", "keep", "order_by"),
    ] {
        assert_eq!(
            issues(fixture.run(&BODY.replace(line, ""))),
            vec![
                serde_json::json!({"phase":"validation", "condition":"unpaired_fields", "requirement":"REQ-0119", "spec_paths":["intermediates[0]"], "context":{"intermediate":"P", "declared":[declared], "missing":[missing]}})
            ]
        );
    }
}
#[test]
fn absence_of_shared_fields_and_first_inferred_type_mismatch_are_distinct() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID\n1\n").unwrap();
    fs::write(fixture.0.join("donor.csv"), "OTHER,SEQ,A,B\nx,0,a,b\n").unwrap();
    let unused = BODY
        .replace("derivation: P.A", "derivation: {literal: a}")
        .replace("derivation: P.B", "derivation: {literal: b}")
        .replace("types: {ID: float, SEQ: int}", "types: {SEQ: int}");
    assert_eq!(
        issues(fixture.run(&unused)),
        vec![
            serde_json::json!({"phase":"validation", "condition":"no_applicable_keys", "requirement":"REQ-0153", "spec_paths":["intermediates[0]"], "context":{"dataset":"DONOR", "keys":["ID","PARAM"], "hint":"declare the `key` explicitly"}})
        ]
    );
    fs::write(fixture.0.join("donor.csv"), "ID,PARAM,SEQ,A,B\nx,0,0,a,b\n").unwrap();
    let mismatched = unused.replace("types: {SEQ: int}", "types: {SEQ: int, PARAM: int}");
    assert_eq!(
        issues(fixture.run(&mismatched)),
        vec![
            serde_json::json!({"phase":"validation", "condition":"incompatible_input_type", "requirement":"REQ-0151", "spec_paths":["intermediates[0]"], "context":{"source":"ID", "expected":"int", "actual":"str"}})
        ]
    );
}
