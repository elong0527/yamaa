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

#[test]
fn transitive_nested_conversion_handlers_survive_success_and_failure() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    let case = truth["families"]["original-specifications"]["typed_intermediates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "nested-conversion-ledger")
        .unwrap();
    // QSV -> FIRST -> MIDDLE -> DEEP crosses several donor counters.
    let specification = case["spec"]
        .as_str()
        .unwrap()
        .replace(
            "intermediates:\n",
            "intermediates:\n  - id: DEEP\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: {literal: bad}, unconvertible: 2}\n    order_by: [VS.VSSEQ]\n    keep: first\n  - id: MIDDLE\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: DEEP.PICK\n    order_by: [VS.VSSEQ]\n    keep: first\n",
        )
        .replace(
            "  - id: FIRST\n    dataset: VS\n    key: {USUBJID: QS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: {literal: bad}, unconvertible: 2}",
            "  - id: FIRST\n    dataset: VS\n    key: {USUBJID: QS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: MIDDLE.PICK, unconvertible: 2}",
        );
    for (replacement, expected_count) in [("2", 3), ("bad-replacement", 1)] {
        let fixture = Fixture::new();
        for (path, content) in case["inputs"].as_object().unwrap() {
            let mut content = content.as_str().unwrap().to_owned();
            if path == "qs.csv" {
                // The outer donor's own typed handler must still count once.
                content.push_str("S01,WEEK 9223372036854775808,0\n");
            }
            fs::write(fixture.0.join(path), content).unwrap();
        }
        fs::write(
            fixture.0.join("spec.yaml"),
            specification.replacen(
                "unconvertible: 2",
                &format!("unconvertible: {replacement}"),
                1,
            ),
        )
        .unwrap();
        let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        if replacement == "2" {
            assert!(domain.issues().is_empty(), "{:?}", domain.issues());
            assert!(domain.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                case["csv"].as_str().unwrap().as_bytes(),
            );
        } else {
            assert!(
                domain
                    .issues()
                    .iter()
                    .any(|issue| issue.condition == "conversion_failed"
                        && issue.spec_paths == ["intermediates[0].derivations.PICK.unconvertible"]),
                "{:?}",
                domain.issues()
            );
        }
        let observations = domain.observations();
        let counts = observations["handler_counts"].as_array().unwrap();
        assert_eq!(
            counts
                .iter()
                .find(
                    |entry| entry["spec_path"] == "intermediates[0].derivations.PICK.unconvertible"
                )
                .unwrap()["count"],
            expected_count,
        );
        assert_eq!(
            counts
                .iter()
                .find(|entry| entry["spec_path"]
                    == "intermediates[3].derivations.QSVISITN.unconvertible")
                .unwrap()["count"],
            1,
        );
        assert_eq!(
            counts
                .iter()
                .filter(|entry| entry["handler"] == "unconvertible")
                .map(|entry| entry["count"].as_u64().unwrap())
                .sum::<u64>(),
            expected_count + 1,
        );
    }
}
