//! Same-input donor selection retains one captured relation across driver scopes.
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
            "yamaa-correlated-predicate-{}-{}",
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
                example: "independent-correlated-predicate",
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

const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int, CUT: int}}
  D: {path: donor.csv, types: {ID: int, TIME: int}}
output: {path: result.csv, columns: [ID, SEQ, A, B]}
intermediates:
  - id: P
    dataset: D
    key: [ID]
    filter: "D.TIME < SRC.CUT"
    order_by: [D.TIME]
    keep: last
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
fn populate(fixture: &Fixture) {
    fs::write(
        fixture.0.join("src.csv"),
        "ID,SEQ,CUT\n1,1,2\n1,2,4\n3,1,2\n1,3,\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("donor.csv"),
        "ID,TIME,A,B\n1,1,one,x\n1,3,three,\n2,9,hidden,hidden\n",
    )
    .unwrap();
    fs::write(fixture.0.join("alt.csv"), "ID,SEQ,CUT\n1,1,4\n3,1,2\n").unwrap();
}
fn finding(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}
#[test]
fn complete_root_outputs_and_handler_geometry_are_current_row_specific() {
    let fixture = Fixture::new();
    populate(&fixture);
    let mut result = fixture.run(BODY).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,SEQ,A,B\n1,1,one,x\n1,2,three,\n3,1,,\n1,3,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":2},
            {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":2}
        ])
    );
}
fn secondary_body(filter: &str) -> String {
    BODY.replace("  D: {path:","  ALT: {path: alt.csv, types: {ID: int, SEQ: int, CUT: int}}\n  D: {path:")
        .replace("D.TIME < SRC.CUT",filter)
        .replace("columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: A, type: str, derivation: P.A}\n  - {name: B, type: str, derivation: P.B}","rows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, SEQ: ALT.SEQ, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: SEQ, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}")
}
#[test]
fn secondary_driver_fields_bind_to_the_actual_template_and_original_native_port() {
    let fixture = Fixture::new();
    populate(&fixture);
    let mut result = fixture.run(&secondary_body("D.TIME < ALT.CUT")).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,SEQ,A,B\n1,1,three,\n3,1,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":1},
            {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":1}
        ])
    );
}
fn refusal(fixture: &Fixture, body: &str) -> Vec<serde_json::Value> {
    match fixture.run(body) {
        Err(issues) => issues.iter().map(finding).collect(),
        Ok(mut result) => {
            assert!(result.output().is_none());
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            result.issues().iter().map(finding).collect()
        }
    }
}
#[test]
fn known_unrelated_input_fields_are_refused_with_complete_selection_geometry() {
    let fixture = Fixture::new();
    populate(&fixture);
    let issues = refusal(&fixture, &secondary_body("D.TIME < SRC.CUT"));
    assert_eq!(
        issues,
        [
            serde_json::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0120","spec_paths":["intermediates[0].filter"],"context":{"identifier":"SRC.CUT","intermediate":"P"}})
        ]
    );
    assert!(!fixture.0.join("result.csv").exists());
}
#[test]
fn donor_qualifier_takes_precedence_and_bare_stored_fields_suggest_it() {
    let fixture = Fixture::new();
    populate(&fixture);
    let mut result = fixture
        .run(&BODY.replace("D.TIME < SRC.CUT", "D.TIME < D.ID"))
        .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,SEQ,A,B\n1,1,,\n1,2,,\n3,1,,\n1,3,,\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":4},
            {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":4}
        ])
    );
    let issues = refusal(
        &fixture,
        &BODY
            .replace("D.TIME < SRC.CUT", "TIME < SRC.CUT")
            .replace("derivation: P.A", "derivation: {literal: unused}")
            .replace("derivation: P.B", "derivation: {literal: unused}"),
    );
    assert_eq!(
        issues,
        [
            serde_json::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0120","spec_paths":["intermediates[0].filter"],"context":{"identifier":"TIME","intermediate":"P","suggestion":"D.TIME"}})
        ]
    );
}
