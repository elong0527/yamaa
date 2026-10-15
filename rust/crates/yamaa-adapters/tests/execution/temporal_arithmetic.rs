//! The same authored conformance vectors run through Rust, installed Python and R.
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
        let path = std::env::temp_dir().join(format!(
            "yamaa-temporal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn assert_issues(issues: &[Issue], expected: &serde_json::Value, name: &str) {
    if expected.is_null() {
        assert!(issues.is_empty(), "{name}: {issues:?}");
    } else {
        assert!(
            issues.iter().any(|issue| {
                issue.phase == expected["phase"].as_str().unwrap()
                    && issue.condition == expected["condition"].as_str().unwrap()
                    && issue.requirement.as_deref() == expected["requirement"].as_str()
                    && issue.spec_paths == [expected["spec_path"].as_str().unwrap()]
            }),
            "{name}: {issues:?}"
        );
    }
}

#[test]
fn temporal_operations_match_shared_host_truth_before_and_after_source_capture() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["temporal_arithmetic"]
        .as_array()
        .unwrap()
    {
        let name = case["name"].as_str().unwrap();
        let fixture = Fixture::new();
        let entry = fixture.0.join("spec.yaml");
        fs::write(&entry, case["spec"].as_str().unwrap()).unwrap();
        let specification = entry.to_str().unwrap();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
            specification,
            base_directory: ".",
        };
        let request = || Request {
            specification,
            environment: None,
        };
        // Invalid types are rejected while every input file is still absent.
        assert_issues(
            &file_application::check(request(), identity()),
            &case["check_issue"],
            name,
        );
        for (path, text) in case["inputs"].as_object().unwrap() {
            fs::write(fixture.0.join(path), text.as_str().unwrap()).unwrap();
        }
        match file_application::domain(request(), identity()) {
            Ok(mut domain) => {
                assert_issues(&domain.issues(), &case["run_issue"], name);
                let reached: u64 = domain.observations()["handler_counts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["handler"] == "invalid")
                    .map(|item| item["count"].as_u64().unwrap())
                    .sum();
                assert_eq!(reached, case["invalid_count"].as_u64().unwrap(), "{name}");
                if let Some(csv) = case["csv"].as_str() {
                    assert!(domain.save().unwrap(), "{name}");
                    assert_eq!(
                        fs::read(fixture.0.join("result.csv")).unwrap(),
                        csv.as_bytes(),
                        "{name}"
                    );
                } else {
                    assert!(domain.output().is_none(), "{name}");
                    assert!(!fixture.0.join("result.csv").exists());
                }
            }
            Err(issues) => assert_issues(&issues, &case["run_issue"], name),
        }
    }
}
