//! Original public unknown-reference findings retain the authored diagnostic site.
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
fn unknown_references_preserve_original_identifiers_and_dataset_context() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["diagnostic_sites"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "yamaa-diagnostic-sites-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&fixture.0).unwrap();
        let path = fixture.0.join("spec.yaml");
        fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
        let path = path.to_str().unwrap();
        let request = || Request {
            specification: path,
            environment: None,
        };
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "diagnostic-sites",
            specification: path,
            base_directory: ".",
        };
        assert_eq!(
            rows(&file_application::check(request(), identity())),
            case["check_issues"]
        );
        for (input, bytes) in case["inputs"].as_object().unwrap() {
            let input = fixture.0.join(input);
            fs::create_dir_all(input.parent().unwrap()).unwrap();
            fs::write(input, bytes.as_str().unwrap()).unwrap();
        }
        for _ in 0..2 {
            assert_eq!(
                rows(&file_application::check(request(), identity())),
                case["check_issues"],
                "{}",
                case["name"]
            );
            let issues = match file_application::domain(request(), identity()) {
                Ok(mut result) => {
                    assert!(result.save().is_err());
                    result.issues().to_vec()
                }
                Err(issues) => issues,
            };
            assert_eq!(rows(&issues), case["issues"], "{}", case["name"]);
            assert!(!fixture.0.join(case["output"].as_str().unwrap()).exists());
        }
    }
}
