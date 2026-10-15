//! Independent current-row key operands through complete file check/build/save routes.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let fixture = Self(std::env::temp_dir().join(format!(
            "yamaa-key-expressions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&fixture.0).unwrap();
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn rows(issues: &[Issue]) -> serde_json::Value {
    issues.iter().map(|row| serde_json::json!({
        "phase": row.phase, "condition": row.condition, "requirement": row.requirement,
        "spec_paths": row.spec_paths, "context": serde_json::from_str::<serde_json::Value>(&row.context).unwrap()
    })).collect()
}
#[test]
fn scalar_and_window_keys_reuse_current_operands_types_handlers_and_independent_saved_truth() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["key_expressions"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture::new();
        let entry = fixture.0.join("spec.yaml");
        fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
        let entry = entry.to_str().unwrap();
        let request = || Request {
            specification: entry,
            environment: None,
        };
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "key-expressions",
            specification: entry,
            base_directory: ".",
        };
        if !case["check_issues"].as_array().unwrap().is_empty() {
            assert_eq!(
                rows(&file_application::check(request(), identity())),
                case["check_issues"],
                "{} before input capture",
                case["name"]
            );
            let issues = match file_application::domain(request(), identity()) {
                Ok(mut result) => {
                    assert!(result.output().is_none(), "{}", case["name"]);
                    assert!(!result.save().unwrap_or(false), "{}", case["name"]);
                    result.issues()
                }
                Err(issues) => issues,
            };
            assert_eq!(
                rows(&issues),
                case["check_issues"],
                "{} build before input capture",
                case["name"]
            );
            assert!(!fixture.0.join("out.csv").exists(), "{}", case["name"]);
        }
        for (input, bytes) in case["inputs"].as_object().unwrap() {
            fs::write(fixture.0.join(input), bytes.as_str().unwrap()).unwrap();
        }
        for _ in 0..2 {
            assert_eq!(
                rows(&file_application::check(request(), identity())),
                case["check_issues"],
                "{} check",
                case["name"]
            );
            let result = file_application::domain(request(), identity());
            if case["issues"].as_array().unwrap().is_empty() {
                let mut result =
                    result.unwrap_or_else(|issues| panic!("{}: {:?}", case["name"], rows(&issues)));
                assert!(result.issues().is_empty(), "{}", case["name"]);
                assert_eq!(
                    result.observations()["handler_counts"],
                    case["handlers"],
                    "{} handlers",
                    case["name"]
                );
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join("out.csv")).unwrap(),
                    case["csv"].as_str().unwrap().as_bytes(),
                    "{}",
                    case["name"]
                );
                fs::remove_file(fixture.0.join("out.csv")).unwrap();
            } else {
                let issues = match result {
                    Ok(mut result) => {
                        let issues = result.issues();
                        assert!(!result.save().unwrap_or(false));
                        issues
                    }
                    Err(issues) => issues,
                };
                assert_eq!(rows(&issues), case["issues"], "{} build", case["name"]);
                assert!(!fixture.0.join("out.csv").exists());
            }
        }
    }
}

#[test]
fn missing_sources_keep_filter_types_but_other_resource_and_output_checkpoints_stay_separate() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    let case = truth["families"]["original-specifications"]["key_expressions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["name"] == "generic-key-valid-cardinality-retains-donor-filter-type-refusal"
        })
        .unwrap();
    let fixture = Fixture::new();
    let entry = fixture.0.join("spec.yaml");
    let specification = case["specification"].as_str().unwrap();
    fs::write(&entry, specification).unwrap();
    let refused = || {
        let entry = entry.to_str().unwrap();
        let mut result = file_application::domain(
            Request {
                specification: entry,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "missing-key-expression-input",
                specification: entry,
                base_directory: ".",
            },
        )
        .unwrap_or_else(|issues| panic!("{:?}", rows(&issues)));
        assert!(result.output().is_none());
        assert_eq!(result.observations()["source_reads"], serde_json::json!([]));
        assert!(!result.save().unwrap_or(false));
        assert!(!fixture.0.join("out.csv").exists());
        rows(&result.issues())
    };
    for present in ["src.csv", "tab.csv"] {
        fs::write(
            fixture.0.join(present),
            case["inputs"][present].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(refused(), case["check_issues"], "only {present} exists");
        fs::remove_file(fixture.0.join(present)).unwrap();
    }
    let missing_sources = serde_json::json!([
        {"phase": "validation", "condition": "resource_path_missing", "requirement": "REQ-0785",
         "spec_paths": ["input.SRC.path"], "context": {"dataset": "SRC", "path": "src.csv"}},
        {"phase": "validation", "condition": "resource_path_missing", "requirement": "REQ-0785",
         "spec_paths": ["input.TAB.path"], "context": {"dataset": "TAB", "path": "tab.csv"}},
    ]);
    fs::create_dir(fixture.0.join("tab.csv")).unwrap();
    let mut mixed_resources = missing_sources.clone();
    mixed_resources[1]["condition"] = serde_json::json!("resource_path_not_regular_file");
    assert_eq!(refused(), mixed_resources);
    fs::remove_dir(fixture.0.join("tab.csv")).unwrap();

    // A valid filter has no static refusal. Even an independently known output
    // declaration error retains its later checkpoint when sources are missing.
    fs::write(
        &entry,
        specification
            .replace("TAB.KINT > 'bad'", "TAB.KINT > 0")
            .replace("path: out.csv", "path: out.csv\n  decimals: 19"),
    )
    .unwrap();
    assert_eq!(refused(), missing_sources);
}
