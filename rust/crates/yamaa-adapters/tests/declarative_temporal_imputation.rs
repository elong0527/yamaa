use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        let path = std::env::temp_dir().join(format!(
            "yamaa-temporal-imputation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fn copy(source: &Path, target: &Path) {
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let dst = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir(&dst).unwrap();
                    copy(&entry.path(), &dst);
                } else {
                    fs::copy(entry.path(), dst).unwrap();
                }
            }
        }
        copy(&source, &path);
        Self(path)
    }
    fn run(&self) -> file_application::Domain {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "test",
            specification: &specification,
            base_directory: ".",
        };
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        )
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn golden(fixture: &Fixture) -> serde_json::Value {
    let expected = yamaa_adapters::yaml_decode::decode_yaml(
        &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
        Default::default(),
    )
    .unwrap()
    .document;
    fn json(d: &yamaa_core::schema::Document, id: usize) -> serde_json::Value {
        use yamaa_core::schema::DocumentNode as N;
        match &d.nodes()[id] {
            N::Null => serde_json::Value::Null,
            N::Text(value) => serde_json::json!(value),
            N::Integer(value) => value.parse::<i64>().unwrap().into(),
            N::Float(value) => serde_json::json!(value),
            N::Boolean(value) => (*value).into(),
            N::Sequence(items) => items.iter().map(|id| json(d, *id)).collect(),
            N::Mapping(items) => items
                .iter()
                .map(|(key, value)| {
                    let N::Text(key) = &d.nodes()[*key] else {
                        panic!("key")
                    };
                    (key.clone(), json(d, *value))
                })
                .collect(),
        }
    }
    json(&expected, expected.root())
}
#[test]
fn unchanged_original_fixtures_qualify_imputation_precision_and_conversion_bytes() {
    for (name, output) in [("schema-birth-date", "adsl.csv"), ("sdtm-dm-age", "dm.csv")] {
        let fixture = Fixture::new(name);
        let mut result = fixture.run();
        assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
        assert!(result.save().unwrap());
        assert_eq!(
            fs::read(fixture.0.join(output)).unwrap(),
            fs::read(fixture.0.join("expected").join(output)).unwrap(),
            "{name}"
        );
    }
}
#[test]
fn original_negative_fixtures_retain_complete_normative_diagnostics() {
    for name in [
        "negative-impute-bad-month",
        "negative-impute-unreachable-month",
        "negative-impute-missing-month",
        "negative-impute-day-rule",
        "negative-impute-bad-day",
        "negative-date-conversion",
    ] {
        let fixture = Fixture::new(name);
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
            specification: &specification,
            base_directory: ".",
        };
        let issues = match file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        ) {
            Ok(result) => {
                assert!(result.output().is_none(), "{name}");
                result.issues().to_vec()
            }
            Err(issues) => issues,
        };
        let expected = golden(&fixture);
        let actual = &issues[0];
        assert_eq!(actual.phase, expected["phase"], "{name}");
        assert_eq!(actual.condition, expected["condition"], "{name}");
        assert_eq!(
            actual.requirement.as_deref(),
            expected["requirement"].as_str(),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(&actual.spec_paths).unwrap(),
            expected["spec_paths"],
            "{name}"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&actual.context).unwrap(),
            expected["context"],
            "{name}"
        );
    }
}

/// Original error context is a required subset, as in the conformance adapter.
/// Check every unchanged required fact and separately compare the complete
/// independently captured reference finding, including additional operation facts.
#[test]
fn unchanged_partial_oracles_and_complete_captured_reference_facts_both_match() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/temporal_imputation_reference.json")).unwrap();
    for name in [
        "negative-impute-bad-source",
        "negative-precision-bad-source",
        "negative-datetime-impute-bad-source",
        "negative-datetime-precision-bad-source",
    ] {
        let fixture = Fixture::new(name);
        let result = fixture.run();
        assert!(result.output().is_none());
        assert_eq!(result.issues().len(), 1);
        let issue = &result.issues()[0];
        let actual = serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()});
        assert_eq!(actual, reference[name][0], "{name}");
        let expected = golden(&fixture);
        for field in ["phase", "condition", "requirement", "spec_paths"] {
            assert_eq!(actual[field], expected[field], "{name}: {field}");
        }
        for (key, value) in expected["context"].as_object().unwrap() {
            assert_eq!(actual["context"][key], *value, "{name}: context.{key}");
        }
        assert!(!expected["context"]
            .as_object()
            .unwrap()
            .contains_key("operation"));
        assert!(actual["context"]["operation"].is_string());
    }
}

#[test]
fn row_imputation_and_column_precision_use_original_binding_and_completed_values() {
    let fixture = Fixture::new("schema-birth-date");
    fs::write(
        fixture.0.join("spec.yaml"),
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, DTM, PR, DATE]}
columns:
  - {name: ID, type: int}
  - {name: DTM, type: datetime}
  - name: PR
    type: str
    derivation: {datetime_precision: {source: DTM, missing: null}}
  - name: DATE
    type: date
    derivation: {to_date: {source: DTM}}
rows:
  - id: records
    dataset: SRC
    derivations:
      ID: SRC.ID
      DTM: {datetime_impute: {source: SRC.TEXT, time: last, missing: null, invalid: null}}
"#,
    )
    .unwrap();
    fs::write(
        fixture.0.join("input.csv"),
        b"ID,TEXT\n1,2024-02-29\n2,2024-02-29T12:13\n3,\n",
    )
    .unwrap();
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(fixture.0.join("output.csv")).unwrap(),b"ID,DTM,PR,DATE\n1,2024-02-29T23:59:59,D,2024-02-29\n2,2024-02-29T12:13:00,S,2024-02-29\n3,,,\n");
}
#[test]
fn nested_case_conversion_reads_only_its_selected_completed_datetime() {
    let fixture = Fixture::new("schema-birth-date");
    fs::write(
        fixture.0.join("spec.yaml"),
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, DTM, PR, DATE]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - name: DTM
    type: datetime
    derivation: {datetime_impute: {source: SRC.TEXT, time: last, missing: null, invalid: null}}
  - name: PR
    type: str
    derivation: {datetime_precision: {source: DTM, missing: null}}
  - name: DATE
    type: date
    derivation:
      case:
        - when: "PR = 'D'"
          then: {to_date: {source: DTM}}
        - otherwise: {literal: null}
"#,
    )
    .unwrap();
    fs::write(
        fixture.0.join("input.csv"),
        b"ID,TEXT\n1,2024-02-29\n2,2024-02-29T12:13\n3,\n",
    )
    .unwrap();
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,DTM,PR,DATE\n1,2024-02-29T23:59:59,D,2024-02-29\n2,2024-02-29T12:13:00,S,\n3,,,\n"
    );
}
