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
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "yamaa-key-expressions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&fixture.0).unwrap();
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
