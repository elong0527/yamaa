#![cfg(feature = "yaml")]
//! Derivations that read the current record augment, for each current row,
//! only the donor records whose stored keys match the row's match values
//! (LOOKUP-15). A record no row matches is never augmented, so a derivation
//! cannot fail on it. A key or `between` bound on a derived field, or a
//! window among the derivations, reads every record's derived values, so
//! every record is augmented as before.
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, source: &str) -> Self {
        let root = crate::support::Scratch::new("lookup-matched-derivations");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("src.csv"), source).unwrap();
        // Subject 9's day is the largest integer: adding any positive
        // reference day to it overflows.
        fs::write(
            root.join("lab.csv"),
            "ID,SEQ,DAY\n1,1,2\n1,2,6\n9,1,9223372036854775807\n",
        )
        .unwrap();
        Self(root)
    }
    /// The saved artifact, or the issues that kept anything from being saved.
    fn run(&self) -> Result<String, Vec<Value>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let mut domain = file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "lookup-matched-derivations",
                specification,
                base_directory: ".",
            },
        )
        .map_err(|failure| failure.iter().map(issue).collect::<Vec<_>>())?;
        let issues = domain.issues().iter().map(issue).collect::<Vec<_>>();
        if !issues.is_empty() {
            assert!(domain.save().is_err());
            assert!(!self.0.join("out.csv").exists());
            return Err(issues);
        }
        assert!(domain.save().unwrap());
        Ok(fs::read_to_string(self.0.join("out.csv")).unwrap())
    }
}

fn issue(issue: &Diagnostic) -> Value {
    yamaa_adapters::json!({"phase":issue.phase(),"condition":issue.condition(),"spec_paths":issue.spec_paths})
}

/// The closest-day pattern: each subject's LAB record with the smallest
/// shifted day, where the shift reads the current record's `REF`.
fn specification(intermediate: &str) -> String {
    format!(
        "schema_version: '1.0'
domain: OUT
keys: [ID]
base: SRC
input:
  SRC: {{path: src.csv, types: {{ID: int, REF: int}}}}
  LAB: {{path: lab.csv, types: {{ID: int, SEQ: int, DAY: int}}}}
output: {{path: out.csv, columns: [ID, NEAR]}}
intermediates:
  - id: NEAREST
    dataset: LAB
{intermediate}    order_by: [LAB.SHIFTED]
    keep: first
    no_match: {{literal: null}}
columns:
  - {{name: ID, type: int, derivation: SRC.ID}}
  - {{name: NEAR, type: int, derivation: NEAREST.SEQ}}
"
    )
}

const MATCHED: &str = "    key: [ID]
    derivations:
      SHIFTED: {compute: {expr: \"DAY + SRC.REF\"}}
";

/// The overflow of subject 9's shifted day, at the derivation.
fn overflow() -> Vec<Value> {
    vec![yamaa_adapters::json!({
        "phase": "derivation",
        "condition": "integer_overflow",
        "spec_paths": ["intermediates[0].derivations.SHIFTED.compute"],
    })]
}

#[test]
fn a_record_no_row_matches_is_never_augmented() {
    // Subject 1 picks SEQ 1 (2 + 5 = 7 before 6 + 5 = 11); subject 2 has no
    // record. Subject 9's record would overflow, but no row matches it.
    let fixture = Fixture::new(&specification(MATCHED), "ID,REF\n1,5\n2,7\n");
    assert_eq!(fixture.run().unwrap(), "ID,NEAR\n1,1\n2,\n");
}

#[test]
fn a_matched_record_that_fails_still_stops_the_run() {
    let fixture = Fixture::new(&specification(MATCHED), "ID,REF\n1,5\n2,7\n9,1\n");
    assert_eq!(fixture.run().unwrap_err(), overflow());
}

#[test]
fn a_derived_key_or_bound_or_a_window_augments_every_record() {
    let derived_key = "    key: {KEYN: ID}
    derivations:
      KEYN: {compute: {expr: \"ID + 0\"}}
      SHIFTED: {compute: {expr: \"DAY + SRC.REF\"}}
";
    let derived_bound = "    key: [ID]
    between: {value: SRC.REF, lower: LOW}
    derivations:
      LOW: {compute: {expr: \"SEQ - 10\"}}
      SHIFTED: {compute: {expr: \"DAY + SRC.REF\"}}
";
    let window = "    key: [ID]
    derivations:
      SHIFTED: {compute: {expr: \"DAY + SRC.REF\"}}
      ORDINAL: {row_number: {window: {group_by: [LAB.ID], order_by: [LAB.DAY]}}}
";
    for intermediate in [derived_key, derived_bound, window] {
        let fixture = Fixture::new(&specification(intermediate), "ID,REF\n1,5\n2,7\n");
        assert_eq!(fixture.run().unwrap_err(), overflow(), "{intermediate}");
    }
}
