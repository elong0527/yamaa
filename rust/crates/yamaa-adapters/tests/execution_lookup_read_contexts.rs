#![cfg(feature = "yaml")]
//! A named intermediate reads the current row only through values that every
//! context reading it holds: the driver record of a row template, the groups
//! of a grouped row template, a row filter's own template, or the completed
//! rows a dataset verification checks (LOOKUP-2, LOOKUP-5, LOOKUP-8,
//! LOOKUP-9, LOOKUP-15). A read that names no such value fails once, before
//! any data is read.
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("lookup-read-contexts");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("source.csv"), "ID,GRP,VAL\n1,A,3\n2,A,4\n3,B,5\n").unwrap();
        fs::write(root.join("donor.csv"), "KEY,X\n1,10\n2,20\n3,30\n").unwrap();
        fs::write(
            root.join("win.csv"),
            "ID,LO,HI,NAME\n1,1,4,LOW\n1,5,9,HIGH\n",
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
            crate::support::study::identity("lookup-read-contexts", specification),
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
    crate::support::study::issue_row(issue)
}

fn run(specification: &str) -> Result<String, Vec<Value>> {
    Fixture::new(specification).run()
}

/// One `unknown_field` finding at `path` for `identifier`, read by `intermediate`.
fn refused(rule: &str, path: &str, identifier: &str, intermediate: &str) -> Vec<Value> {
    vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "unknown_field",
        "rule": rule,
        "spec_paths": [path],
        "context": {"identifier": identifier, "intermediate": intermediate}
    })]
}

const INPUT: &str = "input:\n  SRC: {path: source.csv, types: {ID: int, VAL: int}}\n  DONOR: {path: donor.csv, types: {KEY: int, X: int}}\n  WIN: {path: win.csv, types: {ID: int, LO: int, HI: int}}\n";

#[test]
fn a_grouped_row_template_reads_a_lookup_only_through_its_groups() {
    // One row per group of source records, reading `intermediate` through
    // `column`; `extra` declares the further columns the template derives.
    let grouped = |intermediate: &str, group: &str, column: &str, extra: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [{group}]\n{INPUT}\
             output: {{path: out.csv, columns: [{group}, VALUE]}}\n\
             intermediates:\n{intermediate}\
             columns:\n  - {{name: {group}, type: {kind}}}\n{extra}  - {{name: VALUE, type: str}}\n\
             rows:\n  - id: grouped\n    dataset: SRC\n    group_by: [SRC.{group}]\n    \
             derivations:\n      {group}: SRC.{group}\n      VALUE: {column}\n",
            kind = if group == "ID" { "int" } else { "str" },
        )
    };
    // A match value, a range value and a lookup derivation that read a
    // source field outside the groups name no single value of the group's
    // row (LOOKUP-2, ROW-5).
    let ungrouped = |path: &str, identifier: &str, intermediate: &str| {
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "ungrouped_driver_field",
            "rule": "ROW-5",
            "spec_paths": [path],
            "context": {
                "identifier": identifier,
                "intermediate": intermediate,
                "row": "grouped",
                "dataset": "SRC"
            }
        })])
    };
    let keyed = "  - id: P\n    dataset: DONOR\n    key: {KEY: SRC.ID}\n    no_match: null\n";
    assert_eq!(
        run(&grouped(keyed, "GRP", "P.X", "")),
        ungrouped("intermediates[0].key", "SRC.ID", "P")
    );
    let ranged = |value: &str| {
        format!(
            "  - id: W\n    dataset: WIN\n    key: {{ID: {{literal: 1}}}}\n    \
             between: {{value: {value}, lower: LO, upper: HI}}\n    no_match: null\n"
        )
    };
    assert_eq!(
        run(&grouped(&ranged("SRC.VAL"), "GRP", "W.NAME", "")),
        ungrouped("intermediates[0].between.value", "SRC.VAL", "W")
    );
    let derived = "  - id: W\n    dataset: WIN\n    key: {ID: {literal: 1}}\n    \
                   filter: 'WIN.LO <= WIN.DAY AND WIN.DAY <= WIN.HI'\n    no_match: null\n    \
                   derivations: {DAY: SRC.VAL}\n";
    assert_eq!(
        run(&grouped(derived, "GRP", "W.NAME", "")),
        ungrouped("intermediates[0].derivations.DAY.source", "SRC.VAL", "W")
    );
    // A correlated filter field outside the groups keeps its 0.4.0 finding:
    // the field names no value of the filter's scope (LOOKUP-8).
    let filtered = "  - id: W\n    dataset: WIN\n    key: {ID: {literal: 1}}\n    \
                    filter: 'WIN.HI < SRC.VAL'\n    order_by: [WIN.LO]\n    keep: first\n    \
                    no_match: null\n";
    assert_eq!(
        run(&grouped(filtered, "GRP", "W.NAME", "")),
        Err(refused(
            "LOOKUP-7",
            "intermediates[0].filter",
            "SRC.VAL",
            "W"
        ))
    );
    // A group key, or a value the group's row derives, is one value of it.
    assert_eq!(
        run(&grouped(keyed, "ID", "P.X", "")),
        Ok("ID,VALUE\n1,10\n2,20\n3,30\n".into())
    );
    let top = grouped(
        &ranged("TOP"),
        "GRP",
        "W.NAME",
        "  - {name: TOP, type: int}\n",
    )
    .edit(
        "      VALUE: W.NAME\n",
        "      TOP: {aggregate: 'MAX(SRC.VAL)'}\n      VALUE: W.NAME\n",
    );
    assert_eq!(run(&top), Ok("GRP,VALUE\nA,LOW\nB,HIGH\n".into()));
}

#[test]
fn a_correlated_lookup_reads_only_the_driver_of_the_reading_template() {
    // Rows from the source records, then from the donor records; `source`
    // and `donor` are what each template derives VALUE from, and `filter` the
    // source template's row filter.
    let templates = |intermediate: &str, filter: &str, source: &str, donor: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, ORIGIN]\n{INPUT}\
             output: {{path: out.csv, columns: [ID, ORIGIN, VALUE]}}\n\
             intermediates:\n{intermediate}\
             columns:\n  - {{name: ID, type: int}}\n  - {{name: ORIGIN, type: str}}\n  \
             - {{name: VALUE, type: str}}\n\
             rows:\n  - id: source\n    dataset: SRC\n{filter}    derivations:\n      \
             ID: SRC.ID\n      ORIGIN: {{literal: SOURCE}}\n      VALUE: {source}\n\n  \
             - id: donor\n    dataset: DONOR\n    derivations:\n      ID: DONOR.KEY\n      \
             ORIGIN: {{literal: DONOR}}\n      VALUE: {donor}\n"
        )
    };
    // A lookup derivation reads the source record, which the donor rows do
    // not have (LOOKUP-15).
    let derived = "  - id: W\n    dataset: WIN\n    key: {ID: {literal: 1}}\n    \
                   filter: 'WIN.LO <= WIN.DAY AND WIN.DAY <= WIN.HI'\n    no_match: null\n    \
                   derivations: {DAY: SRC.VAL}\n";
    assert_eq!(
        run(&templates(derived, "", "W.NAME", "W.NAME")),
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "rule": "LOOKUP-15",
            "spec_paths": ["intermediates[0].derivations.DAY.source"],
            "context": {"identifier": "SRC.VAL", "intermediate": "W"}
        })])
    );
    assert_eq!(
        run(&templates(derived, "", "W.NAME", "{literal: NONE}")),
        Ok(
            "ID,ORIGIN,VALUE\n1,SOURCE,LOW\n2,SOURCE,LOW\n3,SOURCE,HIGH\n\
            1,DONOR,NONE\n2,DONOR,NONE\n3,DONOR,NONE\n"
                .into()
        )
    );
    // A row filter reads its lookup for the template it filters, here the
    // source template, which has no donor record (LOOKUP-8).
    let filtered = |filter: &str| {
        format!(
            "  - id: R\n    dataset: WIN\n    key: {{ID: {{literal: 1}}}}\n    \
             filter: '{filter}'\n    order_by: [WIN.LO]\n    keep: first\n    no_match: null\n"
        )
    };
    let row_filter = "    filter: 'R.NAME IS NOT NULL'\n";
    let literal = "{literal: NONE}";
    assert_eq!(
        run(&templates(
            &filtered("WIN.HI >= DONOR.X"),
            row_filter,
            literal,
            literal
        )),
        Err(refused(
            "LOOKUP-7",
            "intermediates[0].filter",
            "DONOR.X",
            "R"
        ))
    );
    // Compared with the source record instead, only the source record with a
    // window ending before its value is kept.
    assert_eq!(
        run(&templates(
            &filtered("WIN.HI < SRC.VAL"),
            row_filter,
            "R.NAME",
            literal
        )),
        Ok("ID,ORIGIN,VALUE\n3,SOURCE,LOW\n1,DONOR,NONE\n2,DONOR,NONE\n3,DONOR,NONE\n".into())
    );
    // A row filter's lookup takes its range value from that template's row
    // too, here a donor field the source rows do not have (LOOKUP-9).
    let windowed = "  - id: R\n    dataset: WIN\n    key: {ID: {literal: 1}}\n    \
                    between: {value: DONOR.X, lower: LO, upper: HI}\n    no_match: null\n";
    assert_eq!(
        run(&templates(windowed, row_filter, literal, literal)),
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "rule": "LOOKUP-9",
            "spec_paths": ["intermediates[0].between.value"],
            "context": {"identifier": "DONOR.X", "intermediate": "R"}
        })])
    );
    // A match value reads the source record, which the donor rows do not
    // have, whichever input the specification reads first (LOOKUP-5).
    let keyed = |value: &str| {
        format!(
            "  - id: K\n    dataset: WIN\n    key: {{ID: {value}}}\n    order_by: [WIN.LO]\n    \
             keep: first\n    no_match: null\n"
        )
    };
    assert_eq!(
        run(&templates(&keyed("SRC.ID"), "", "K.NAME", "K.NAME")),
        Err(refused("LOOKUP-5", "intermediates[0].key", "SRC.ID", "K"))
    );
    assert_eq!(
        run(&templates(&keyed("SRC.ID"), "", "K.NAME", literal)),
        Ok("ID,ORIGIN,VALUE\n1,SOURCE,LOW\n2,SOURCE,\n3,SOURCE,\n\
            1,DONOR,NONE\n2,DONOR,NONE\n3,DONOR,NONE\n"
            .into())
    );
    // Grouped by donor key, the donor rows hold no single donor value: the
    // finding names the donor template and its input.
    let grouped = templates(&keyed("DONOR.X"), "", literal, "K.NAME").edit(
        "    dataset: DONOR\n",
        "    dataset: DONOR\n    group_by: [DONOR.KEY]\n",
    );
    assert_eq!(
        run(&grouped),
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "ungrouped_driver_field",
            "rule": "ROW-5",
            "spec_paths": ["intermediates[0].key"],
            "context": {
                "identifier": "DONOR.X",
                "intermediate": "K",
                "row": "donor",
                "dataset": "DONOR"
            }
        })])
    );
}

#[test]
fn a_dataset_verification_reads_a_lookup_as_a_column_does() {
    let verified = |intermediate: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\n{INPUT}base: SRC\n\
             output: {{path: out.csv, columns: [ID]}}\n\
             intermediates:\n  - id: P\n    dataset: DONOR\n{intermediate}    \
             no_match: null\n\
             columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             verifications:\n  - assert:\n      id: positive\n      require: 'P.X > 0'\n"
        )
    };
    // Each completed row is built from a source record, never a window
    // record: neither a match value nor a range value reads one (LOOKUP-5,
    // LOOKUP-9).
    assert_eq!(
        run(&verified("    key: {KEY: WIN.ID}\n")),
        Err(refused("LOOKUP-5", "intermediates[0].key", "WIN.ID", "P"))
    );
    assert_eq!(
        run(&verified(
            "    key: {KEY: ID}\n    between: {value: WIN.LO, lower: X, upper: X}\n"
        )),
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "unknown_field",
            "rule": "LOOKUP-9",
            "spec_paths": ["intermediates[0].between.value"],
            "context": {"identifier": "WIN.LO", "intermediate": "P"}
        })])
    );
    assert_eq!(
        run(&verified("    key: {KEY: ID}\n")),
        Ok("ID\n1\n2\n3\n".into())
    );
}

#[test]
fn a_lookup_read_by_several_row_templates_is_reported_once() {
    // Two row templates over `driver` read the window through one column
    // derivation, so each template reads it the same way.
    let specification = |driver: &str, id: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, ORIGIN]\n{INPUT}\
             output: {{path: out.csv, columns: [ID, ORIGIN, VALUE]}}\n\
             intermediates:\n  - id: P\n    dataset: WIN\n    key: {{ID: {{literal: 1}}}}\n    \
             filter: 'WIN.HI < SRC.VAL'\n    order_by: [WIN.LO]\n    keep: first\n    \
             no_match: null\n\
             columns:\n  - {{name: ID, type: int}}\n  - {{name: ORIGIN, type: str}}\n  \
             - {{name: VALUE, type: str, derivation: P.NAME}}\n\
             rows:\n  - id: first\n    dataset: {driver}\n    \
             derivations: {{ID: {driver}.{id}, ORIGIN: {{literal: A}}}}\n\n  \
             - id: second\n    dataset: {driver}\n    \
             derivations: {{ID: {driver}.{id}, ORIGIN: {{literal: B}}}}\n"
        )
    };
    // Donor records build every row, so no row has a source record to
    // compare with; the one cause is reported once, not once per template.
    assert_eq!(
        run(&specification("DONOR", "KEY")),
        Err(refused(
            "LOOKUP-7",
            "intermediates[0].filter",
            "SRC.VAL",
            "P"
        ))
    );
    assert_eq!(
        run(&specification("SRC", "ID")),
        Ok("ID,ORIGIN,VALUE\n1,A,\n2,A,\n3,A,LOW\n1,B,\n2,B,\n3,B,LOW\n".into())
    );
}
