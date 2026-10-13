//! Independently authored typed public results and ordered source-selection findings.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_application,
    issue_rows::Issue,
    public_table::{PublicTable, Values},
    specification_report::Identity,
};
use yamaa_engine::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(case: &serde_json::Value) -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-source-selection-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("spec.yaml"),
            case["specification"].as_str().unwrap(),
        )
        .unwrap();
        for (path, bytes) in case["inputs"].as_object().unwrap() {
            fs::write(root.join(path), bytes.as_str().unwrap()).unwrap();
        }
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issues(rows: &[Issue]) -> serde_json::Value {
    serde_json::Value::Array(
        rows.iter()
            .map(|issue| {
                let mut row = issue.as_value();
                assert!(row["context"].is_string());
                row["context"] = serde_json::from_str(&issue.context).unwrap();
                row
            })
            .collect(),
    )
}
fn table(view: &PublicTable) -> serde_json::Value {
    let columns: Vec<_> = view
        .columns()
        .iter()
        .map(|(name, values)| {
            let kind = match values {
                Values::Str(_) => "str",
                Values::Int(_) => "int",
                _ => panic!("unexpected control type"),
            };
            serde_json::json!([name, kind])
        })
        .collect();
    let rows: Vec<Vec<Option<String>>> = (0..view.rows())
        .map(|row| {
            view.columns()
                .iter()
                .map(|(_, values)| match values {
                    Values::Str(values) => values[row].clone(),
                    Values::Int(values) => values[row].map(|value| value.to_string()),
                    _ => panic!("unexpected control type"),
                })
                .collect()
        })
        .collect();
    serde_json::json!({"columns": columns, "rows": rows})
}
fn identity(specification: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "independent-source-selection",
        specification,
        base_directory: ".",
    }
}

fn verify_truth(fixture: &str) {
    let truth: serde_json::Value = serde_json::from_str(fixture).unwrap();
    for (name, case) in truth["cases"].as_object().unwrap() {
        for _ in 0..2 {
            let fixture = Fixture::new(case);
            let entry = fixture.0.join("spec.yaml");
            let written = entry.to_str().unwrap();
            let request = || Request {
                specification: written,
                environment: None,
            };
            assert_eq!(
                issues(&file_application::check(request(), identity(written))),
                case["check_issues"],
                "{name}: static check"
            );
            let mut result = file_application::domain(request(), identity(written)).unwrap();
            assert_eq!(
                issues(&result.issues()),
                case["issues"],
                "{name}: complete issues"
            );
            let report = result.observations();
            assert_eq!(
                report["handler_counts"], case["handlers"],
                "{name}: handler ledger"
            );
            let reads: Vec<_> = report["source_reads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|read| {
                    assert_eq!(read["outcome"], "captured", "{name}");
                    assert_eq!(read["snapshots_created"], 1, "{name}");
                    read["path"].clone()
                })
                .collect();
            assert_eq!(
                serde_json::json!(reads),
                case["source_reads"],
                "{name}: capture order"
            );
            assert_eq!(report["artifacts"], serde_json::json!([]));
            if let Some(expected) = case.get("table") {
                assert_eq!(
                    table(result.output_table().unwrap()),
                    *expected,
                    "{name}: typed output"
                );
                assert!(result.save().unwrap(), "{name}");
                assert_eq!(
                    fs::read(fixture.0.join("result.csv")).unwrap(),
                    case["csv"].as_str().unwrap().as_bytes(),
                    "{name}: exact CSV"
                );
                fs::write(fixture.0.join("input.csv"), b"changed after build").unwrap();
                assert!(result.save().unwrap(), "{name}: retained save");
                assert_eq!(
                    fs::read(fixture.0.join("result.csv")).unwrap(),
                    case["csv"].as_str().unwrap().as_bytes(),
                    "{name}: retained bytes"
                );
            } else {
                assert!(result.output().is_none(), "{name}");
                assert!(
                    result.save().is_err(),
                    "{name}: failed build cannot publish"
                );
                assert!(!fixture.0.join("result.csv").exists(), "{name}");
            }
        }
    }
}

#[test]
fn every_source_selection_case_keeps_exact_types_diagnostics_counts_and_saved_bytes() {
    verify_truth(include_str!("fixtures/source_selection.json"));
}
#[test]
fn group_key_filters_have_complete_normative_findings_and_record_controls() {
    verify_truth(include_str!("fixtures/source_filter_scope.json"));
}
