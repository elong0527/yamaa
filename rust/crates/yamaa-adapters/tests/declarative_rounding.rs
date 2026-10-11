#![cfg(any(unix, windows))]
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, public_table::Values, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
const POSITIVE: &str =
    include_str!("../../../../benchmarks/schema-round-half-away-from-zero-ties/spec.yaml");
const INPUT: &[u8] =
    include_bytes!("../../../../benchmarks/schema-round-half-away-from-zero-ties/input/adsl.csv");
const EXPECTED: &str =
    include_str!("../../../../benchmarks/schema-round-half-away-from-zero-ties/expected/adsl.csv");
const NEGATIVE: &str = include_str!(
    "../../../../benchmarks/negative-round-half-away-from-zero-non-numeric-source/spec.yaml"
);
struct Study(PathBuf);
impl Study {
    fn new(source: &str, input: bool) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-rounding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("spec.yaml"), source).unwrap();
        if input {
            fs::create_dir(path.join("input")).unwrap();
            fs::write(path.join("input/adsl.csv"), INPUT).unwrap();
        }
        Self(fs::canonicalize(path).unwrap())
    }
    fn path(&self) -> String {
        self.0.join("spec.yaml").to_str().unwrap().into()
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
        example: "test",
        specification: path,
        base_directory: ".",
    }
}
#[test]
fn original_rounding_yaml_reaches_the_public_lifecycle_and_independent_csv_truth() {
    let study = Study::new(POSITIVE, false);
    let path = study.path();
    let request = Request {
        specification: &path,
        environment: None,
    };
    assert!(file_application::check(request, identity(&path)).is_empty());
    fs::create_dir(study.0.join("input")).unwrap();
    fs::write(study.0.join("input/adsl.csv"), INPUT).unwrap();
    let result = file_application::domain(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    )
    .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_expected(&result);
    assert!(!study.0.join("adsl.csv").exists());
}
#[test]
fn original_nonnumeric_source_rejects_before_missing_study_data_and_has_no_formula_context() {
    let study = Study::new(NEGATIVE, false);
    let path = study.path();
    let request = Request {
        specification: &path,
        environment: None,
    };
    let findings = file_application::check(request, identity(&path));
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.phase, "validation");
    assert_eq!(finding.condition, "incompatible_input_type");
    assert_eq!(finding.requirement.as_deref(), Some("REQ-0418"));
    assert_eq!(
        finding.spec_paths,
        ["columns.RNDVAL.derivation.round_half_away_from_zero.source"]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&finding.context).unwrap(),
        json!({"source":"SEX", "expected":"numeric", "actual":"str"})
    );
    assert_eq!(
        file_application::domain(
            Request {
                specification: &path,
                environment: None
            },
            identity(&path)
        )
        .err()
        .unwrap(),
        findings
    );
    assert!(!study.0.join("input").exists());
}
#[test]
fn malformed_digits_precede_unknown_source_and_missing_study_data() {
    let source = NEGATIVE
        .replace("source: SEX", "source: ABSENT")
        .replace("digits: 1", "digits: true");
    let study = Study::new(&source, false);
    let path = study.path();
    let findings = file_application::check(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].phase, "validation");
    assert_eq!(findings[0].condition, "invalid_field_type");
    assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0287"));
    assert_eq!(
        findings[0].spec_paths,
        ["columns.RNDVAL.derivation.round_half_away_from_zero.digits"]
    );
    assert!(!study.0.join("input").exists());
}

#[test]
fn unsupported_rounding_secondary_scope_rejects_before_study_data() {
    let study = Study::new(
        &POSITIVE.replace("source: VAL1", "source: OTHER.VAL1"),
        false,
    );
    let path = study.path();
    let findings = file_application::check(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].condition, "unsupported_operation");
    assert_eq!(
        findings[0].spec_paths,
        ["columns.RND1.derivation.round_half_away_from_zero.source"]
    );
    assert_eq!(
        findings[0].context,
        "{\"operation\":\"secondary_source_expression\"}"
    );
    assert!(!study.0.join("input").exists());
}

fn assert_expected(result: &file_application::Domain) {
    let table = result.output_table().unwrap();
    let mut lines = EXPECTED.lines();
    let names = lines.next().unwrap().split(',').collect::<Vec<_>>();
    let rows = lines
        .map(|line| line.split(',').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(table.rows(), rows.len());
    assert_eq!(table.columns().len(), names.len());
    for (column, (name, values)) in table.columns().iter().enumerate() {
        assert_eq!(name, names[column]);
        match values {
            Values::Str(actual) => assert_eq!(
                *actual,
                rows.iter()
                    .map(|row| (!row[column].is_empty()).then(|| row[column].to_owned()))
                    .collect::<Vec<_>>()
            ),
            Values::Float(actual) => assert_eq!(
                *actual,
                rows.iter()
                    .map(|row| (!row[column].is_empty())
                        .then(|| row[column].parse::<f64>().unwrap()))
                    .collect::<Vec<_>>()
            ),
            _ => panic!("unexpected public type for {name}"),
        }
    }
}
fn qualified(source: &str, lookup: bool) {
    let study = Study::new(source, true);
    if lookup {
        fs::write(study.0.join("input/lookup.csv"), INPUT).unwrap();
    }
    let path = study.path();
    let issues = file_application::check(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    );
    assert!(issues.is_empty(), "{issues:?}");
    let result = file_application::domain(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    )
    .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert_expected(&result);
}
#[test]
fn admitted_driver_and_named_intermediate_operands_keep_independent_rounding_truth() {
    qualified(
        &POSITIVE
            .replace(
                "SOURCE: input/adsl.csv",
                "SOURCE: {path: input/adsl.csv, types: {VAL1: float, VAL0: float}}",
            )
            .replace("source: VAL1", "source: SOURCE.VAL1"),
        false,
    );
    let source = POSITIVE.replace("SOURCE: input/adsl.csv", "SOURCE: input/adsl.csv\n  LOOKUP: {path: input/lookup.csv, types: {VAL1: float}}\nbase: SOURCE\nintermediates:\n  - id: SELECTED\n    dataset: LOOKUP\n    key: [STUDYID, USUBJID]\n    no_match: null").replace("source: VAL1", "source: SELECTED.VAL1");
    qualified(&source, true);
}
#[test]
fn nested_case_and_row_template_rounding_keep_the_same_independent_expected_rows() {
    let source = POSITIVE.replace("round_half_away_from_zero:\n        source: VAL1\n        digits: 1", "case:\n        - when: \"VAL1 IS NOT NULL\"\n          then: {round_half_away_from_zero: {source: VAL1, digits: 1}}\n        - otherwise: {literal: null}");
    qualified(&source, false);
    let source = format!("{POSITIVE}\nrows:\n  - id: RECORDS\n    dataset: SOURCE\n    derivations:\n      RND1: {{round_half_away_from_zero: {{source: VAL1, digits: 1}}}}\n");
    qualified(&source, false);
}
#[test]
fn unselected_case_uses_static_known_type_validation_before_study_capture() {
    let source = NEGATIVE.replace("round_half_away_from_zero:\n        source: SEX\n        digits: 1", "case:\n        - when: \"SEX = 'NEVER'\"\n          then: {round_half_away_from_zero: {source: SEX, digits: 1}}\n        - otherwise: {literal: null}");
    let study = Study::new(&source, false);
    let path = study.path();
    let findings = file_application::check(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0418"));
    assert_eq!(
        findings[0].spec_paths,
        ["columns.RNDVAL.derivation.case[0].then.round_half_away_from_zero.source"]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&findings[0].context).unwrap(),
        json!({"source":"SEX","expected":"numeric","actual":"str"})
    );
    assert!(!study.0.join("input").exists());
}
