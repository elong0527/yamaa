#![cfg(any(unix, windows))]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new(name: &str, input: bool) -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-temporal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        fs::copy(original.join("spec.yaml"), path.join("spec.yaml")).unwrap();
        if input {
            copy_directory(&original.join("input"), &path.join("input"));
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
fn copy_directory(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
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
fn original_temporal_documents_keep_independent_complete_csv_and_explicit_save() {
    for name in ["adam-adsl-duration", "adam-adsl-analysis-age"] {
        let study = Study::new(name, true);
        let path = study.path();
        let check = file_application::check(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        );
        assert!(check.is_empty(), "{name}: {check:?}");
        let mut result = file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        )
        .unwrap();
        assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
        let expected = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks")
                .join(name)
                .join("expected/adsl.csv"),
        )
        .unwrap();
        assert_eq!(
            result.output_table().unwrap().rows(),
            expected.iter().filter(|&&byte| byte == b'\n').count() - 1,
            "{name}"
        );
        assert!(!study.0.join("adsl.csv").exists());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(study.0.join("adsl.csv")).unwrap(), expected);
    }
}
#[test]
fn original_temporal_static_failures_keep_complete_independent_diagnostics_before_study_read() {
    for name in [
        "negative-date-diff-endpoints",
        "negative-date-diff-units",
        "negative-study-day-datetime",
        "negative-epoch-day-non-date",
    ] {
        let study = Study::new(name, false);
        let path = study.path();
        let findings = file_application::check(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        );
        let expected = expected_yaml(
            &fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name)
                    .join("expected/error.yaml"),
            )
            .unwrap(),
        );
        assert_eq!(findings.len(), 1, "{name}: {findings:?}");
        let finding = &findings[0];
        let actual = serde_json::json!({"phase":finding.phase, "condition":finding.condition, "requirement":finding.requirement, "spec_paths":finding.spec_paths, "context":serde_json::from_str::<serde_json::Value>(&finding.context).unwrap()});
        assert_eq!(actual, expected, "{name}");
        let failed = file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        )
        .err()
        .unwrap();
        assert_eq!(failed, findings);
        assert!(!study.0.join("input").exists());
        assert!(!study.0.join("adsl.csv").exists());
    }
}

fn expected_yaml(bytes: &[u8]) -> serde_json::Value {
    let reply: serde_json::Value =
        serde_json::from_str(&yamaa_adapters::yaml_transport::decode_yaml_bytes(bytes).unwrap())
            .unwrap();
    let tree = &reply["outcome"]["document"];
    fn node(tree: &serde_json::Value, index: usize) -> serde_json::Value {
        let value = &tree["nodes"][index];
        match value["kind"].as_str().unwrap() {
            "text" => value["value"].clone(),
            "null" => serde_json::Value::Null,
            "sequence" => value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| node(tree, id.as_u64().unwrap() as usize))
                .collect(),
            "mapping" => value["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    let pair = entry.as_array().unwrap();
                    (
                        node(tree, pair[0].as_u64().unwrap() as usize)
                            .as_str()
                            .unwrap()
                            .to_owned(),
                        node(tree, pair[1].as_u64().unwrap() as usize),
                    )
                })
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into(),
            other => panic!("unexpected independent diagnostic scalar: {other}"),
        }
    }
    node(tree, tree["root"].as_u64().unwrap() as usize)
}

#[test]
fn study_day_epoch_and_nested_row_cases_execute_typed_driver_operands_and_missing_values() {
    let source = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  SRC: {path: input.csv, types: {ID: int, D: date, R: date}}\noutput: {path: result.csv, columns: [ID, SD, EPOCH]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: SD\n    type: int\n    derivation: {study_day: {date: SRC.D, reference: SRC.R}}\n  - name: EPOCH\n    type: int\n    derivation: {to_epoch_day: {source: SRC.D}}\n";
    let input = b"ID,D,R\n1,2023-12-31,2024-01-01\n2,2024-01-01,2024-01-01\n3,2024-01-02,2024-01-01\n4,,2024-01-01\n5,2024-01-01,\n";
    let expected = b"ID,SD,EPOCH\n1,-1,19722\n2,1,19723\n3,2,19724\n4,,\n5,,19723\n";
    for source in [source.into(), source.replace("derivation: {study_day: {date: SRC.D, reference: SRC.R}}", "derivation: {case: [{when: 'ID > 0', then: {study_day: {date: SRC.D, reference: SRC.R}}}, {otherwise: {literal: null}}]}"), format!("{source}rows:\n  - id: records\n    dataset: SRC\n    derivations:\n      SD: {{study_day: {{date: SRC.D, reference: SRC.R}}}}\n")] {
        let study = Study::new("adam-adsl-duration", false);
        fs::write(study.0.join("spec.yaml"), source).unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let path = study.path();
        let issues = file_application::check(Request { specification: &path, environment: None }, identity(&path));
        assert!(issues.is_empty(), "{issues:?}");
        let mut result = file_application::domain(Request { specification: &path, environment: None }, identity(&path)).unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(!study.0.join("result.csv").exists());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(study.0.join("result.csv")).unwrap(), expected);
    }
}
