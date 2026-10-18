#![cfg(feature = "yaml")]
//! A row derivation reads only values that its own row template completes
//! (REQ-0069). Each operand form that reads a column the template leaves to
//! the column phase reports the same phase boundary at its own operand,
//! naming the column and the row template, before any data is read.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-row-phase-reads-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("source.csv"), "ID,VAL\n1,3\n1,1\n2,5\n").unwrap();
        Self(root)
    }
    fn issues(&self) -> Vec<Issue> {
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
                example: "row-phase-reads",
                specification,
                base_directory: ".",
            },
        )
        .unwrap();
        let issues = domain.issues().to_vec();
        assert!(domain.save().is_err());
        assert!(!self.0.join("out.csv").exists());
        issues
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn issue(issue: &Issue) -> yamaa_core::json::Value {
    yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
}

#[test]
fn row_operands_reading_a_column_phase_column_report_its_phase_boundary() {
    // SEQ is a window, so it is never row-local (REQ-1260): every row
    // template leaves it to the column phase.
    let specification = |operand: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, SEQ]\n\
             input: {{SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}}}\n\
             output: {{path: out.csv, columns: [ID, SEQ, VALUE]}}\n\
             columns:\n  - {{name: ID, type: int}}\n\
             \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
             window: {{group_by: [ID], order_by: [SRC.VAL]}}\n\
             \x20 - {{name: VALUE, type: int}}\n\
             rows:\n  - id: record\n    derivations:\n      ID: SRC.ID\n      VALUE: {operand}\n"
        )
    };
    for (operand, path) in [
        (
            "{compute: {expr: 'SEQ * 10'}}",
            "rows[0].derivations.VALUE.compute.expr",
        ),
        (
            "{flag: {condition: 'SEQ = 1', true_value: 1, false_value: 0, missing: 0}}",
            "rows[0].derivations.VALUE.flag.condition",
        ),
        (
            "{round_half_away_from_zero: {source: SEQ, digits: 0}}",
            "rows[0].derivations.VALUE.round_half_away_from_zero.source",
        ),
        (
            "{row_number: {window: {group_by: [ID], order_by: [SEQ]}}}",
            "rows[0].derivations.VALUE.row_number.window.order_by[0]",
        ),
        (
            "{case: [{when: 'SEQ = 1', then: {literal: 1}}, {otherwise: {literal: 0}}]}",
            "rows[0].derivations.VALUE.case[0].when",
        ),
    ] {
        let fixture = Fixture::new(&specification(operand));
        assert_eq!(
            fixture.issues().iter().map(issue).collect::<Vec<_>>(),
            vec![yamaa_adapters::json!({
                "phase": "validation",
                "condition": "phase_boundary",
                "requirement": "REQ-0069",
                "spec_paths": [path],
                "context": {
                    "identifier": "SEQ",
                    "row": "record",
                    "available_phase": "column_derivation",
                    "required_phase": "row_construction"
                }
            })],
            "{operand}"
        );
    }
}
