//! Independent completed-row assertion scope, order and whole-record truth.
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
fn issues(rows: &[Issue]) -> serde_json::Value {
    serde_json::Value::Array(
        rows.iter()
            .map(|issue| {
                let mut value = issue.as_value();
                value["context"] = serde_json::from_str(&issue.context).unwrap();
                value
            })
            .collect(),
    )
}
#[test]
fn complete_independent_lookup_assertions_keep_declaring_sites_and_prior_records() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/completed_lookup_assertions.json")).unwrap();
    for (name, case) in truth["cases"].as_object().unwrap() {
        for _ in 0..2 {
            let fixture = Fixture(std::env::temp_dir().join(format!(
                "yamaa-completed-assert-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )));
            fs::create_dir(&fixture.0).unwrap();
            let entry = fixture.0.join("spec.yaml");
            fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
            for (path, content) in case["inputs"].as_object().unwrap() {
                fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
            }
            let written = entry.to_str().unwrap();
            let identity = || Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "completed-assert",
                specification: written,
                base_directory: ".",
            };
            let request = || Request {
                specification: written,
                environment: None,
            };
            assert_eq!(
                issues(&file_application::check(request(), identity())),
                case["check_issues"],
                "{name}: check"
            );
            let mut result = file_application::domain(request(), identity())
                .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
            assert_eq!(
                issues(&result.issues()),
                case["issues"],
                "{name}: complete findings"
            );
            let report = result.observations();
            assert_eq!(
                report["handler_counts"], case["handlers"],
                "{name}: declared handler ledger"
            );
            let mut expected = case["verifications"].clone();
            for record in expected.as_array_mut().unwrap() {
                record["specification"] = written.into();
            }
            assert_eq!(
                report["verifications"], expected,
                "{name}: complete ordered verification records"
            );
            assert_eq!(report["artifacts"], serde_json::json!([]));
            if let Some(csv) = case.get("csv") {
                assert!(result.output().is_some(), "{name}");
                assert!(result.save().unwrap(), "{name}");
                assert_eq!(
                    fs::read(fixture.0.join("result.csv")).unwrap(),
                    csv.as_str().unwrap().as_bytes(),
                    "{name}: exact CSV"
                );
                assert!(result.save().unwrap());
            } else {
                assert!(result.output().is_none(), "{name}");
                assert!(result.save().is_err(), "{name}");
                assert!(!fixture.0.join("result.csv").exists());
            }
        }
    }
}
