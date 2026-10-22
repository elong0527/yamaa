#![cfg(feature = "yaml")]
//! Without row templates, each key is computed from one base record
//! (COLUMN-7). A key derived by a window gives each record its own row, so
//! every column then reads that one record (ROW-8); otherwise a key that
//! reads other records, an aggregate or an `odm` read, fails before any data
//! is read, as does a key that reads a non-key column.
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        let root = crate::support::Scratch::new("no-rows-record-reads");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, text) in files {
            fs::write(root.join(name), text).unwrap();
        }
        Self(root)
    }
    /// The saved artifact, or the issues that kept anything from being saved.
    fn run(&self) -> Result<String, Vec<Value>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let identity = crate::support::study::identity("no-rows-record-reads", specification);
        let request = Request {
            specification,
            environment: None,
        };
        let mut domain = file_application::domain(
            yamaa_core::project_function::Language::Python,
            request,
            identity,
        )
        .map_err(|failure| failure.iter().map(issue).collect::<Vec<_>>())?;
        let issues = domain.issues().iter().map(issue).collect::<Vec<_>>();
        if !issues.is_empty() {
            assert!(domain.save().is_err());
            return Err(issues);
        }
        assert!(domain.save().unwrap());
        Ok(fs::read_to_string(self.0.join("out.csv")).unwrap())
    }
    /// The issues `check` reports without reading any input.
    fn check(&self) -> Vec<Value> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::check(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("no-rows-record-reads", specification),
        )
        .iter()
        .map(issue)
        .collect()
    }
}

fn issue(issue: &Diagnostic) -> Value {
    crate::support::study::issue_row(issue)
}

const SOURCE: &str = "ID,VAL,X\n1,3,a\n1,5,\n2,4,c\n";
const DONOR: &str = "ID,ARM,N\n1,A,10\n2,B,20\n";
const ODM: &str = "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,\
FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value\n\
S1,MDV.1,001,V1,1,FO.VS,1,IG.VS,1,IT.VS.SYSBP,120\n\
S1,MDV.1,001,V1,1,FO.VS,1,IG.VS,2,IT.VS.SYSBP,130\n\
S1,MDV.1,002,V1,1,FO.VS,1,IG.VS,1,IT.VS.SYSBP,125\n\
S1,MDV.1,001,V1,1,FO.DM,1,IG.DM,1,IT.DM.SEX,F\n\
S1,MDV.1,002,V1,1,FO.DM,1,IG.DM,1,IT.DM.SEX,M\n";

/// SEQ numbers each ID's records by VAL, so each record is its own row and
/// VALUE reads that one record.
fn windowed(value: &str) -> String {
    format!(
        "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, SEQ]\n\
         input: {{SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}}}\n\
         output: {{path: out.csv, columns: [ID, SEQ, VALUE]}}\n\
         columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
         window: {{group_by: [ID], order_by: [SRC.VAL]}}\n\
         \x20 - name: VALUE\n    type: str\n    derivation: {value}\n"
    )
}

#[test]
fn a_window_key_gives_every_column_its_row_record() {
    for (value, output) in [
        (
            "{flag: 'SRC.VAL > 3'}",
            "ID,SEQ,VALUE\n1,1,\n1,2,Y\n2,1,Y\n",
        ),
        (
            "{flag: {condition: 'SRC.X IS NULL', true_value: none, false_value: some, missing: none}}",
            "ID,SEQ,VALUE\n1,1,some\n1,2,none\n2,1,some\n",
        ),
        (
            "{case: [{when: 'ID = 1', then: {flag: 'SRC.VAL > 3'}}, {otherwise: {literal: other}}]}",
            "ID,SEQ,VALUE\n1,1,\n1,2,Y\n2,1,other\n",
        ),
        (
            "{first_available: {sources: [SRC.X, {variable: SRC.X, filter: 'SRC.VAL > 3'}]}}",
            "ID,SEQ,VALUE\n1,1,a\n1,2,\n2,1,c\n",
        ),
        (
            "{case: [{when: 'ID = 2', then: {first_available: {sources: [SRC.X]}}}, {otherwise: {literal: other}}]}",
            "ID,SEQ,VALUE\n1,1,other\n1,2,other\n2,1,c\n",
        ),
    ] {
        let fixture = Fixture::new(&windowed(value), &[("source.csv", SOURCE)]);
        assert_eq!(fixture.run(), Ok(output.into()), "{value}");
    }
    // Every ODM record is its own row: an item read takes the scope of the
    // record's own item group, and the key may read one too.
    let odm = |keys: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [{keys}]\ninput: {{ODM: odm.csv}}\n\
             output: {{path: out.csv, columns: [USUBJID, SEQ, SEX]}}\n\
             columns:\n  - {{name: USUBJID, type: str, derivation: ODM.SubjectKey}}\n\
             \x20 - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
             window: {{group_by: [USUBJID], order_by: [ODM.ItemGroupRepeatKey]}}\n\
             \x20 - {{name: SEX, type: str, derivation: {{odm: ODM.IT.DM.SEX}}}}\n"
        )
    };
    let fixture = Fixture::new(&odm("USUBJID, SEQ"), &[("odm.csv", ODM)]);
    assert_eq!(
        fixture.run(),
        Ok("USUBJID,SEQ,SEX\n001,1,\n001,3,\n002,1,\n001,2,F\n002,2,M\n".into())
    );
    let fixture = Fixture::new(&odm("USUBJID, SEQ, SEX"), &[("odm.csv", ODM)]);
    let refused = fixture.run().unwrap_err();
    assert_eq!(refused[0]["condition"], Value::String("missing_key".into()));
}

#[test]
fn a_key_reads_a_named_intermediate_for_its_record() {
    let keyed = |value: &str, kind: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, GROUP]\nbase: SRC\n\
             input:\n  SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}\n  \
             DONOR: {{path: donor.csv, types: {{ID: int, N: int}}}}\n\
             intermediates:\n  - id: SUBJ\n    dataset: DONOR\n    key: [ID]\n    no_match: null\n\
             output: {{path: out.csv, columns: [ID, GROUP]}}\n\
             columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n\
             \x20 - name: GROUP\n    type: {kind}\n    derivation: {value}\n"
        )
    };
    for (value, kind, output) in [
        (
            "{flag: {condition: \"SUBJ.ARM = 'A'\", true_value: Y, false_value: N, missing: N}}",
            "str",
            "ID,GROUP\n1,Y\n2,N\n",
        ),
        (
            "{compute: {expr: 'SUBJ.N + 1'}}",
            "int",
            "ID,GROUP\n1,11\n2,21\n",
        ),
        (
            "{case: [{when: \"SUBJ.ARM = 'A'\", then: {literal: Y}}, {otherwise: {literal: N}}]}",
            "str",
            "ID,GROUP\n1,Y\n2,N\n",
        ),
    ] {
        let fixture = Fixture::new(
            &keyed(value, kind),
            &[("source.csv", SOURCE), ("donor.csv", DONOR)],
        );
        assert_eq!(fixture.run(), Ok(output.into()), "{value}");
    }
}

#[test]
fn a_key_that_reads_other_records_fails_before_data() {
    let refused = |column: &str, operation: &str| {
        vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "key_dependency",
            "rule": "COLUMN-7",
            "spec_paths": [format!("columns.{column}.derivation.{operation}")],
            "context": {"column": column, "operation": operation}
        })]
    };
    // An aggregate reduces the records of a key combination that does not
    // exist yet.
    let aggregate = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, TOP]\n\
                     input: {SRC: {path: source.csv, types: {ID: int, VAL: int}}}\n\
                     output: {path: out.csv, columns: [ID, TOP]}\n\
                     columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  \
                     - {name: TOP, type: int, derivation: {aggregate: 'MAX(SRC.VAL)'}}\n";
    let fixture = Fixture::new(aggregate, &[("source.csv", SOURCE)]);
    assert_eq!(fixture.run(), Err(refused("TOP", "aggregate")));
    assert_eq!(fixture.check(), refused("TOP", "aggregate"));
    // An odm read reads the items of the key combination, also inside a case.
    let odm = |derivation: &str, path: &str| {
        format!(
            "schema_version: '1.0'\ndomain: OUT\nkeys: [USUBJID, SEX]\ninput: {{ODM: odm.csv}}\n\
             output: {{path: out.csv, columns: [USUBJID, SEX]}}\n\
             columns:\n  - {{name: USUBJID, type: str, derivation: ODM.SubjectKey}}\n\
             \x20 - {{name: SEX, type: str, derivation: {derivation}}}\n{path}"
        )
    };
    let fixture = Fixture::new(&odm("{odm: ODM.IT.DM.SEX}", ""), &[("odm.csv", ODM)]);
    assert_eq!(fixture.run(), Err(refused("SEX", "odm")));
    assert_eq!(fixture.check(), refused("SEX", "odm"));
    let fixture = Fixture::new(
        &odm(
            "{case: [{when: \"USUBJID = '001'\", then: {odm: ODM.IT.DM.SEX}}, {otherwise: {literal: U}}]}",
            "",
        ),
        &[("odm.csv", ODM)],
    );
    assert_eq!(
        fixture.run(),
        Err(vec![yamaa_adapters::json!({
            "phase": "validation",
            "condition": "key_dependency",
            "rule": "COLUMN-7",
            "spec_paths": ["columns.SEX.derivation.case[0].then.odm"],
            "context": {"column": "SEX", "operation": "odm"}
        })])
    );
    // As an ordinary column, the case reads the subject's item.
    let fixture = Fixture::new(
        &odm(
            "{case: [{when: \"USUBJID = '001'\", then: {odm: ODM.IT.DM.SEX}}, {otherwise: {literal: U}}]}",
            "",
        )
        .edit("keys: [USUBJID, SEX]", "keys: [USUBJID]"),
        &[("odm.csv", ODM)],
    );
    assert_eq!(fixture.run(), Ok("USUBJID,SEX\n001,F\n002,U\n".into()));
    // With a window key every record is its own row, and an aggregate key
    // is computed for that row as any column is.
    let numbered = aggregate
        .edit("keys: [ID, TOP]", "keys: [ID, SEQ, TOP]")
        .edit("columns: [ID, TOP]", "columns: [ID, SEQ, TOP]")
        .edit(
            "  - {name: TOP,",
            "  - name: SEQ\n    type: int\n    derivation:\n      row_number:\n        \
             window: {group_by: [ID], order_by: [SRC.VAL]}\n  - {name: TOP,",
        );
    let fixture = Fixture::new(&numbered, &[("source.csv", SOURCE)]);
    assert_eq!(
        fixture.run(),
        Ok("ID,SEQ,TOP\n1,1,5\n1,2,5\n2,1,4\n".into())
    );
}

/// A key that reads a non-key column fails before any data is read, at its
/// derivation whatever its operation: a 1.0 formula, which writes no
/// operation segment of its own, also under `value`, reports where a bare
/// name's source binding does, as its 0.5 `compute` did (COLUMN-7, D18).
#[test]
fn a_key_that_reads_a_non_key_column_fails_at_its_derivation() {
    let refused = vec![yamaa_adapters::json!({
        "phase": "validation",
        "condition": "key_dependency",
        "rule": "COLUMN-7",
        "spec_paths": ["columns.ID.derivation"],
        "context": {"column": "ID", "dependency": "VALUE"}
    })];
    for derivation in ["VALUE + 1", "{value: VALUE + 1, unconvertible: 0}", "VALUE"] {
        let fixture = Fixture::new(
            &format!(
                "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [ID]\n\
                 input: {{SRC: {{path: source.csv, types: {{ID: int, VAL: int}}}}}}\n\
                 output: {{path: out.csv, columns: [ID, VALUE]}}\n\
                 columns:\n  - {{name: VALUE, type: int, derivation: SRC.VAL}}\n  \
                 - {{name: ID, type: int, derivation: {derivation}}}\n"
            ),
            &[("source.csv", SOURCE)],
        );
        assert_eq!(fixture.run(), Err(refused.clone()), "{derivation}");
        assert_eq!(fixture.check(), refused, "{derivation}");
    }
}
