#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-row-intermediate-operands-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("spec.yaml"), source).unwrap();
        fs::write(
            path.join("pp.csv"),
            "USUBJID,PPTESTCD,DATE\nS1,AUCLST,2030-01-03\nS2,CMAX,2030-02-04\n",
        )
        .unwrap();
        fs::write(path.join("ex.csv"), "USUBJID,EXSPID,EXROUTE,DATE,DTM,N\nS1,PER1-D1,ORAL,2030-01-01,2030-01-01T08:00:00,1.25\nS2,PER2-D1,INTRAVENOUS,2030-02-01,2030-02-01T09:00:00,-2.25\n").unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn path(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().into()
    }
    fn run(&self) -> file_application::Domain {
        let path = self.path();
        file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        )
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn identity(path: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "row-intermediate-operands",
        specification: path,
        base_directory: ".",
    }
}
fn specification(expression: &str, kind: &str, row_derivation: bool) -> String {
    let declaration = if row_derivation {
        format!("  - {{name: X, type: {kind}}}\nrows:\n  - id: pp_records\n    dataset: PP\n    derivations: {{X: {expression}}}\n")
    } else {
        format!("  - {{name: X, type: {kind}, derivation: {expression}}}\nrows:\n  - id: pp_records\n    dataset: PP\n    derivations: {{}}\n")
    };
    format!("schema_version: '1.0'\ndomain: ADPP\nkeys: [USUBJID]\ninput:\n  PP: {{path: pp.csv, types: {{DATE: date}}}}\n  EX: {{path: ex.csv, types: {{DATE: date, N: float}}}}\nintermediates:\n  - id: DOSE\n    dataset: EX\n    key: [USUBJID]\ncolumns:\n  - {{name: USUBJID, type: str, derivation: PP.USUBJID}}\n{declaration}output: {{path: output.csv, columns: [USUBJID, X]}}\n")
}

#[test]
fn row_and_column_operations_read_named_intermediate_fields() {
    for (expression, kind, values) in [
        (
            "{str_extract: {source: DOSE.EXSPID, pattern: '^PER([0-9]+)', group: 1}}",
            "str",
            "S1,1\nS2,2\n",
        ),
        (
            "{str_template: '{PP.PPTESTCD}/{DOSE.EXROUTE}'}",
            "str",
            "S1,AUCLST/ORAL\nS2,CMAX/INTRAVENOUS\n",
        ),
        (
            "{str_case: {source: DOSE.EXROUTE, to: lower}}",
            "str",
            "S1,oral\nS2,intravenous\n",
        ),
        (
            "{str_pad: {source: DOSE.EXSPID, width: 9}}",
            "str",
            "S1,  PER1-D1\nS2,  PER2-D1\n",
        ),
        (
            "{to_date: {source: DOSE.DTM}}",
            "date",
            "S1,2030-01-01\nS2,2030-02-01\n",
        ),
        (
            "{datetime_impute: {source: DOSE.DTM, time: first}}",
            "datetime",
            "S1,2030-01-01T08:00:00\nS2,2030-02-01T09:00:00\n",
        ),
        (
            "{round_half_away_from_zero: {source: DOSE.N, digits: 1}}",
            "float",
            "S1,1.3\nS2,-2.3\n",
        ),
        (
            "{date_diff: {start: DOSE.DATE, end: PP.DATE, unit: day}}",
            "int",
            "S1,2\nS2,3\n",
        ),
    ] {
        for row_derivation in [false, true] {
            let source = specification(expression, kind, row_derivation);
            let study = Study::new(&source);
            let path = study.path();
            assert!(file_application::check(
                Request {
                    specification: &path,
                    environment: None
                },
                identity(&path)
            )
            .is_empty());
            let mut result = study.run();
            assert!(
                result.issues().is_empty(),
                "{expression}, row={row_derivation}: {:?}",
                result.issues()
            );
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("output.csv")).unwrap(),
                format!("USUBJID,X\n{values}"),
                "{expression}, row={row_derivation}"
            );
        }
    }
}

#[test]
fn contains_reads_lookup_and_preserves_boolean_conversion_diagnostic() {
    for row_derivation in [false, true] {
        let study = Study::new(&specification(
            "{str_contains: {source: DOSE.EXSPID, pattern: '^PER'}}",
            "str",
            row_derivation,
        ));
        let result = study.run();
        let issues = result.issues();
        assert_eq!(issues.len(), 1, "{issues:?}");
        for issue in issues {
            assert_eq!(issue.condition, "conversion_failed");
            assert_eq!(issue.requirement.as_deref(), Some("REQ-0013"));
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()["value"],
                true
            );
        }
        assert!(result.output().is_none());
    }
}

#[test]
fn lookup_scalar_operands_preserve_completed_keys_and_missing_field_findings() {
    let source = specification("{str_case: {source: DOSE.EXROUTE, to: lower}}", "str", true)
        .replace("derivations: {X:", "derivations: {USUBJID: PP.USUBJID, X:")
        .replace(
            "  - {name: USUBJID, type: str, derivation: PP.USUBJID}",
            "  - {name: USUBJID, type: str}",
        );
    let study = Study::new(&source);
    assert!(study.run().issues().is_empty());

    let source = specification(
        "{str_extract: {source: DOSE.MISSING, pattern: '(.)'}}",
        "str",
        false,
    );
    let study = Study::new(&source);
    let issues = study.run().issues();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].condition, "unknown_field");
    assert_eq!(
        issues[0].spec_paths,
        ["columns.X.derivation.str_extract.source"]
    );
    assert!(study.run().output().is_none());
}
