//! Independent YAML, donor bytes and exact saved truth for per-record aggregates.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-typed-intermediates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification: &specification,
                base_directory: ".",
            },
        )
    }
    fn save(&self) -> Vec<u8> {
        let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read(self.0.join("result.csv")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn typed_intermediate_conversion_and_static_checks_match_independent_host_truth() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["typed_intermediates"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture::new();
        let name = case["name"].as_str().unwrap();
        fs::write(fixture.0.join("spec.yaml"), case["spec"].as_str().unwrap()).unwrap();
        // Checking uses declaration types and never reads the absent CSV files.
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let issues = file_application::check(
            Request {
                specification: &specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: name,
                specification: &specification,
                base_directory: ".",
            },
        );
        if let Some(condition) = case["check_condition"].as_str() {
            assert!(
                issues.iter().any(|issue| issue.condition == condition),
                "{name}: {issues:?}"
            );
            if condition == "incompatible_input_type" {
                let issue = issues
                    .iter()
                    .find(|issue| issue.condition == condition)
                    .unwrap();
                assert_eq!(issue.requirement.as_deref(), Some("REQ-0323"));
                assert_eq!(issue.spec_paths, ["intermediates[0].key"]);
            }
        } else {
            assert!(issues.is_empty(), "{name}: {issues:?}");
        }
        for (path, content) in case["inputs"].as_object().unwrap() {
            fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
        }
        if let Some(expected) = case["csv"].as_str() {
            assert_eq!(fixture.save(), expected.as_bytes(), "{name}");
        }
        match fixture.run() {
            Ok(domain) => {
                let issues = domain.issues();
                if let Some(condition) = case["run_condition"].as_str() {
                    assert!(
                        issues.iter().any(|issue| issue.condition == condition),
                        "{name}: {issues:?}"
                    );
                } else {
                    assert!(issues.is_empty(), "{name}: {issues:?}");
                }
                let observations = domain.observations();
                let count: u64 = observations["handler_counts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|handler| handler["handler"] == "unconvertible")
                    .map(|handler| handler["count"].as_u64().unwrap())
                    .sum();
                assert_eq!(
                    count,
                    case["unconvertible_count"].as_u64().unwrap(),
                    "{name}: {observations}"
                );
            }
            Err(issues) => {
                let condition = case["run_condition"].as_str().unwrap();
                assert!(
                    issues.iter().any(|issue| issue.condition == condition),
                    "{name}: {issues:?}"
                );
            }
        }
    }
}
