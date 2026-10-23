#![cfg(feature = "yaml")]
//! A row derivation reads only values that its own row template completes
//! (COLUMN-4). Each operand form that reads a column the template leaves to
//! the column phase reports the same phase boundary at its own operand,
//! naming the column and the row template, before any data is read.
use std::fs;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::Request;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("row-phase-reads");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("source.csv"), "ID,VAL\n1,3\n1,1\n2,5\n").unwrap();
        Self(root)
    }
    fn issues(&self) -> Vec<Diagnostic> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let mut domain = crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("row-phase-reads", specification),
        )
        .unwrap();
        let issues = domain.issues().to_vec();
        assert!(domain.save().is_err());
        assert!(!self.0.join("out.csv").exists());
        issues
    }
    fn output(&self) -> String {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let mut domain = crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("row-phase-reads", specification),
        )
        .unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.join("out.csv")).unwrap()
    }
}

fn issue(issue: &Diagnostic) -> yamaa_core::json::Value {
    crate::support::study::issue_row(issue)
}

#[test]
fn row_operands_reading_a_column_phase_column_report_its_phase_boundary() {
    // SEQ is a window, so it is never row-local (SPEC-6): every row
    // template leaves it to the column phase.
    let specification = |operand: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID, SEQ]\n\
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
        ("'SEQ * 10'", "rows.record.derivations.VALUE"),
        (
            "{flag: {condition: 'SEQ = 1', true_value: 1, false_value: 0, missing: 0}}",
            "rows.record.derivations.VALUE.flag.condition",
        ),
        (
            "{round_half_away_from_zero: {source: SEQ, digits: 0}}",
            "rows.record.derivations.VALUE.round_half_away_from_zero.source",
        ),
        (
            "{row_number: {window: {group_by: [ID], order_by: [SEQ]}}}",
            "rows.record.derivations.VALUE.row_number.window.order_by[0]",
        ),
        (
            "{case: [{when: 'SEQ = 1', then: 1}, {otherwise: 0}]}",
            "rows.record.derivations.VALUE.case[0].when",
        ),
    ] {
        let fixture = Fixture::new(&specification(operand));
        assert_eq!(
            fixture.issues().iter().map(issue).collect::<Vec<_>>(),
            vec![yamaa_adapters::json!({
                "phase": "validation",
                "condition": "phase_boundary",
                "rule": "COLUMN-4",
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

#[test]
fn a_lookup_matching_on_a_column_phase_column_reports_its_phase_boundary() {
    // SEQ is a window, so only the column phase completes it. K matches on it
    // through its key, W through its range, and W2 through W's range value
    // (LOOKUP-2).
    let specification = |row: &str, filter: &str, column: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID, VAL]\n\
             input:\n  SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}\n  \
             WIN: {{path: win.csv, types: {{ONE: int, LO: int, HI: int}}}}\n\
             output: {{path: out.csv, columns: [ID, VAL, SEQ, VALUE]}}\n\
             intermediates:\n  - id: K\n    dataset: WIN\n    key: {{LO: SEQ}}\n  \
             - id: W\n    dataset: WIN\n    \
             between: {{value: SEQ, lower: LO, upper: HI}}\n    key: {{ONE: 1}}\n  \
             - id: W2\n    dataset: WIN\n    key: {{ONE: 1}}\n    \
             between: {{value: W.LO, lower: LO, upper: HI}}\n\
             columns:\n  - {{name: ID, type: int}}\n  - {{name: VAL, type: int}}\n\
             \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
             window: {{group_by: [ID], order_by: [VAL]}}\n\
             \x20 - {{name: VALUE, type: str{column}}}\n\
             rows:\n  - id: record\n    dataset: SRC\n{filter}    derivations:\n      \
             ID: SRC.ID\n      VAL: SRC.VAL\n{row}"
        )
    };
    let refused = |paths: &[&str], intermediate: &str| {
        vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "phase_boundary",
            "rule": "LOOKUP-2",
            "spec_paths": paths,
            "context": {
                "identifier": "SEQ",
                "intermediate": intermediate,
                "row": "record",
                "available_phase": "column_derivation",
                "required_phase": "row_construction"
            }
        })]
    };
    let window = "ONE,LO,HI,NAME\n1,1,1,FIRST\n1,2,9,LATER\n";
    for (row, filter, paths, intermediate) in [
        (
            "      VALUE: K.NAME\n",
            "",
            vec!["rows.record.derivations.VALUE.source"],
            "K",
        ),
        (
            "      VALUE: W.NAME\n",
            "",
            vec!["rows.record.derivations.VALUE.source"],
            "W",
        ),
        // Reached through W's range value, the finding also names each
        // value followed: W2's range value, then W's. K is unread, so the
        // resolved specification holds W and W2 alone.
        (
            "      VALUE: W2.NAME\n",
            "",
            vec![
                "rows.record.derivations.VALUE.source",
                "intermediates.W2.between.value",
                "intermediates.W.between.value",
            ],
            "W2",
        ),
        // One site that reads the lookup twice is reported once.
        (
            "      VALUE: \"'x'\"\n",
            "    filter: 'K.NAME IS NOT NULL AND K.LO > 0'\n",
            vec!["rows.record.filter"],
            "K",
        ),
        (
            "      VALUE: \"'x'\"\n",
            "    filter: 'W.NAME IS NOT NULL AND W.HI > 0'\n",
            vec!["rows.record.filter"],
            "W",
        ),
        (
            "      VALUE: 'W2.LO + W2.HI'\n",
            "",
            vec![
                "rows.record.derivations.VALUE",
                "intermediates.W2.between.value",
                "intermediates.W.between.value",
            ],
            "W2",
        ),
    ] {
        let fixture = Fixture::new(&specification(row, filter, ""));
        fs::write(fixture.0.join("win.csv"), window).unwrap();
        assert_eq!(
            fixture.issues().iter().map(issue).collect::<Vec<_>>(),
            refused(&paths, intermediate),
            "{row}{filter}"
        );
    }
    // Read by a column instead, each lookup matches after SEQ is numbered:
    // SEQ 1 falls in FIRST, whose LO 1 falls in FIRST again, and SEQ 2 in
    // LATER, whose LO 2 falls in LATER. K matches the same LO values.
    for read in ["W2.NAME", "K.NAME"] {
        let fixture = Fixture::new(&specification("", "", &format!(", derivation: {read}")));
        fs::write(fixture.0.join("win.csv"), window).unwrap();
        assert_eq!(
            fixture.output(),
            "ID,VAL,SEQ,VALUE\n1,3,2,LATER\n1,1,1,FIRST\n2,5,1,FIRST\n",
            "{read}"
        );
    }
}

#[test]
fn a_chained_lookup_promotes_the_default_its_range_values_read() {
    // POS is row-local, so a row template that reads a lookup matching on
    // it derives it as a default (SPEC-6), whether the lookup reads it
    // directly (W) or through another lookup's range value (W2).
    let specification = |value: &str| {
        format!(
            "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID, VAL]\n\
             input:\n  SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}\n  \
             WIN: {{path: win.csv, types: {{ONE: int, LO: int, HI: int}}}}\n\
             output: {{path: out.csv, columns: [ID, VAL, VALUE]}}\n\
             intermediates:\n  - id: W\n    dataset: WIN\n    \
             between: {{value: POS, lower: LO, upper: HI}}\n    key: {{ONE: 1}}\n  \
             - id: W2\n    dataset: WIN\n    key: {{ONE: 1}}\n    \
             between: {{value: W.LO, lower: LO, upper: HI}}\n\
             columns:\n  - {{name: ID, type: int}}\n  - {{name: VAL, type: int}}\n\
             \x20 - {{name: POS, type: int, derivation: SRC.VAL}}\n\
             \x20 - {{name: VALUE, type: str}}\n\
             rows:\n  - id: record\n    dataset: SRC\n    derivations:\n      \
             ID: SRC.ID\n      VAL: SRC.VAL\n      VALUE: {value}\n"
        )
    };
    // POS 3 and 5 fall in LATER (2 to 9), whose LO 2 falls in LATER again;
    // POS 1 falls in FIRST (1 to 1), whose LO 1 falls in FIRST again.
    for value in ["W.NAME", "W2.NAME"] {
        let fixture = Fixture::new(&specification(value));
        fs::write(
            fixture.0.join("win.csv"),
            "ONE,LO,HI,NAME\n1,1,1,FIRST\n1,2,9,LATER\n",
        )
        .unwrap();
        assert_eq!(
            fixture.output(),
            "ID,VAL,VALUE\n1,3,LATER\n1,1,FIRST\n2,5,LATER\n",
            "{value}"
        );
    }
}
