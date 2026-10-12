//! Independent SELF witnesses preserve phase context, donor order and complete outputs.
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
            "yamaa-self-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(
        &self,
        text: &str,
    ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml");
        fs::write(&specification, text).unwrap();
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
                example: "independent-self",
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
const BODY: &str = r#"schema_version: "1.0"
domain: TEST
input:
  SRC: {path: src.csv}
  OTHER: {path: other.csv}
keys: [ID, SECTION]
output: {path: result.csv, columns: [ID, SECTION, K, SEQ, V, FINAL, FIRST]}
intermediates:
  - id: P
    dataset: SELF
    key: {K: K}
    order_by: [SELF.SEQ]
    keep: last
    no_match: 0
  - id: F
    dataset: SELF
    key: [K]
    filter: "SECTION = 'first'"
    order_by: [SEQ]
    keep: last
    no_match: null
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: SECTION, type: str}
  - {name: K, type: str, derivation: SRC.K}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: V, type: int, derivation: SRC.V}
  - {name: FINAL, type: int, derivation: P.V}
  - {name: FIRST, type: int, derivation: F.V}
rows:
  - id: collected
    dataset: SRC
    derivations:
      SECTION: {literal: first}
  - id: grouped
    dataset: SRC
    group_by: [SRC.K]
    derivations:
      ID: {literal: g}
      SECTION: {literal: second}
      SEQ: {literal: 4}
      V: {compute: {expr: "P.V + 10"}}
  - id: added
    dataset: OTHER
    derivations:
      ID: OTHER.ID
      SECTION: {literal: third}
      K: OTHER.K
      SEQ: {literal: 9}
      V: {compute: {expr: "P.V + 100"}}
"#;
#[test]
fn completed_templates_are_borrowed_and_republished_in_construction_order() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n").unwrap();
    fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\nd,x\n").unwrap();
    let mut result = fixture.run(BODY).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read_to_string(fixture.0.join("result.csv")).unwrap(), "ID,SECTION,K,SEQ,V,FINAL,FIRST\na,first,x,2,5,115,5\nb,first,x,1,9,115,5\ng,second,x,4,15,115,5\nc,third,x,9,115,115,5\nd,third,x,9,115,115,5\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"rows[1].derivations.V.compute.multiple_matches","handler":"multiple_matches","count":1},
            {"spec_path":"rows[2].derivations.V.compute.multiple_matches","handler":"multiple_matches","count":2},
            {"spec_path":"columns.FINAL.derivation.source.multiple_matches","handler":"multiple_matches","count":5},
            {"spec_path":"columns.FIRST.derivation.source.multiple_matches","handler":"multiple_matches","count":5},
        ])
    );
    // The two third-template records both see completed value 15, never 115
    // from the first unfinished third-template record. The column sees all five.
}
#[test]
fn first_template_read_preserves_full_phase_context_before_study_access() {
    let fixture = Fixture::new();
    let body = BODY.replace(
        "SECTION: {literal: first}",
        "SECTION: {source: {variable: P.K}}",
    );
    assert_eq!(
        issues(fixture.run(&body)),
        vec![serde_json::json!({
            "phase":"validation", "condition":"phase_boundary", "requirement":"REQ-0120",
            "spec_paths":["rows[0].derivations.SECTION.source"],
            "context":{"identifier":"P.K","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
        })]
    );
    assert!(!fixture.0.join("src.csv").exists());
}
#[test]
fn column_only_donor_is_unavailable_to_later_templates() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\n").unwrap();
    fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\n").unwrap();
    let body = BODY.replace("expr: \"P.V + 10\"", "expr: \"P.FINAL + 10\"");
    let found = issues(fixture.run(&body));
    assert_eq!(
        found,
        vec![serde_json::json!({
            "phase":"validation", "condition":"unknown_field", "requirement":"REQ-0125",
            "spec_paths":["rows[1].derivations.V.compute.expr"], "context":{"identifier":"P.FINAL"}
        })]
    );
}
#[test]
fn empty_completed_template_uses_literal_absence_in_later_templates() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID,K,SEQ,V\na,x,2,5\nb,x,1,9\n").unwrap();
    fs::write(fixture.0.join("other.csv"), "ID,K\nc,x\nd,x\n").unwrap();
    let body = BODY.replace(
        "id: collected\n    dataset: SRC",
        "id: collected\n    dataset: SRC\n    filter: \"SRC.ID = 'absent'\"",
    );
    let mut result = fixture.run(&body).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read_to_string(fixture.0.join("result.csv")).unwrap(), "ID,SECTION,K,SEQ,V,FINAL,FIRST\ng,second,x,4,10,110,\nc,third,x,9,110,110,\nd,third,x,9,110,110,\n");
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([
            {"spec_path":"rows[1].derivations.V.compute.no_match","handler":"no_match","count":1},
            {"spec_path":"columns.FINAL.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
            {"spec_path":"columns.FIRST.derivation.source.no_match","handler":"no_match","count":3},
        ])
    );
}

#[test]
fn reserved_input_collision_and_missing_row_phase_are_refused_before_study_access() {
    let fixture = Fixture::new();
    let collision = BODY.replace(
        "  OTHER: {path: other.csv}",
        "  OTHER: {path: other.csv}\n  SELF: {path: self.csv}",
    );
    assert_eq!(
        issues(fixture.run(&collision)),
        vec![
            serde_json::json!({"phase":"validation", "condition":"duplicate_identifier", "requirement":"REQ-0113", "spec_paths":["intermediates[0].dataset","input.SELF"], "context":{"identifier":"SELF"}}),
            serde_json::json!({"phase":"validation", "condition":"duplicate_identifier", "requirement":"REQ-0113", "spec_paths":["intermediates[1].dataset","input.SELF"], "context":{"identifier":"SELF"}}),
        ]
    );
    let missing_rows = BODY
        .split("rows:\n")
        .next()
        .unwrap()
        .replace("domain: TEST", "domain: TEST\nbase: SRC")
        .replace(
            "  - {name: SECTION, type: str}",
            "  - {name: SECTION, type: str, derivation: {literal: first}}",
        );
    assert_eq!(
        issues(fixture.run(&missing_rows)),
        vec![
            serde_json::json!({"phase":"validation", "condition":"unknown_field", "requirement":"REQ-0120", "spec_paths":["intermediates[0].dataset"], "context":{"identifier":"SELF","intermediate":"P"}}),
            serde_json::json!({"phase":"validation", "condition":"unknown_field", "requirement":"REQ-0120", "spec_paths":["intermediates[1].dataset"], "context":{"identifier":"SELF","intermediate":"F"}}),
        ]
    );
    assert!(!fixture.0.join("src.csv").exists());
}

#[test]
fn hidden_first_template_alternative_preserves_its_authored_phase_path() {
    let fixture = Fixture::new();
    let body = BODY.replace("SECTION: {literal: first}", "SECTION: {case: [{when: \"SRC.ID = 'absent'\", then: {source: {variable: P.K}}}, {otherwise: {literal: first}}]}");
    assert_eq!(
        issues(fixture.run(&body)),
        vec![serde_json::json!({
            "phase":"validation", "condition":"phase_boundary", "requirement":"REQ-0120",
            "spec_paths":["rows[0].derivations.SECTION.case[0].then.source"],
            "context":{"identifier":"P.K","available_phase":"completed_prior_row_template","required_phase":"row_construction"}
        })]
    );
}

#[test]
fn aggregate_first_template_proof_keeps_full_references_including_record_star() {
    for (replacement, identifier, path) in [
        (
            "SECTION: {aggregate: {expr: 'SUM(P.V)'}}",
            "P.V",
            "rows[0].derivations.SECTION.aggregate",
        ),
        (
            "SECTION: {aggregate: {expr: 'COUNT(P.*)'}}",
            "P.*",
            "rows[0].derivations.SECTION.aggregate",
        ),
        (
            "SECTION: {case: [{when: 'TRUE', then: {aggregate: {expr: 'COUNT(P.*)'}}}]}",
            "P.*",
            "rows[0].derivations.SECTION.case[0].then.aggregate",
        ),
    ] {
        let fixture = Fixture::new();
        let body = BODY.replace("SECTION: {literal: first}", replacement);
        assert_eq!(
            issues(fixture.run(&body)),
            [serde_json::json!({
                "phase":"validation", "condition":"phase_boundary", "requirement":"REQ-0120",
                "spec_paths":[path], "context":{"identifier":identifier,
                    "available_phase":"completed_prior_row_template", "required_phase":"row_construction"}
            })]
        );
        assert!(!fixture.0.join("src.csv").exists());
        assert!(!fixture.0.join("other.csv").exists());
    }
}

#[test]
fn donor_qualified_global_aggregate_does_not_require_the_primary_row_driver() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("src.csv"), "ID\n99\n").unwrap();
    fs::write(fixture.0.join("alt.csv"), "ID\n1\n2\n").unwrap();
    fs::write(fixture.0.join("donor.csv"), "ID,V\n1,4\n1,7\n2,3\n").unwrap();
    let body = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  ALT: {path: alt.csv, types: {ID: int}}
  D: {path: donor.csv, types: {ID: int, V: int}}
output: {path: result.csv, columns: [ID, TOTAL]}
rows:
  - id: alternate
    dataset: ALT
    derivations: {ID: ALT.ID}
columns:
  - {name: ID, type: int}
  - {name: TOTAL, type: int, derivation: {aggregate: {expr: 'SUM(D.V)'}}}
"#;
    let mut result = fixture.run(body).unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
        "ID,TOTAL\n1,11\n2,3\n"
    );
    assert_eq!(
        result.observations()["handler_counts"],
        serde_json::json!([])
    );
}
