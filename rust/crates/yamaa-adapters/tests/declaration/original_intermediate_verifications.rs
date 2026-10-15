//! Shipped-schema successors for specification/test_intermediate_verification.py
//! retired in #2110. Original valid and invalid verification payloads are kept;
//! private Python model instances are replaced by the native check boundary.
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_engine::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-original-intermediate-verification-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn check(&self, intermediate: Value) -> Vec<Issue> {
        let document = json!({
            "schema_version":"1.0", "domain":"OUT", "base":"DS",
            "keys":["STUDYID","USUBJID"],
            "input":{"DS":{"path":"never-read.csv","types":{"STUDYID":"str","USUBJID":"str"}}},
            "output":{"path":"out.csv","columns":["STUDYID","USUBJID"]},
            "columns":[
                {"name":"STUDYID","type":"str","derivation":"DS.STUDYID"},
                {"name":"USUBJID","type":"str","derivation":"DS.USUBJID"}
            ],
            "intermediates":[intermediate]
        });
        let entry = self.0.join("spec.yaml");
        fs::write(&entry, serde_json::to_vec(&document).unwrap()).unwrap();
        let entry = entry.to_str().unwrap();
        let issues = file_application::check(
            Request {
                specification: entry,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "original-intermediate-verification",
                specification: entry,
                base_directory: ".",
            },
        );
        assert!(!self.0.join("never-read.csv").exists());
        assert!(!self.0.join("out.csv").exists());
        issues
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn original_short_and_named_intermediate_unique_checks_are_admitted() {
    let study = Study::new();
    let intermediate = json!({
        "id":"DS_EOS", "dataset":"DS",
        "verifications":[
            {"unique":["STUDYID","USUBJID"]},
            {"unique":{"id":"one-row-per-subject","columns":["USUBJID"]}}
        ]
    });
    assert!(study.check(intermediate).is_empty());
}

#[test]
fn all_original_invalid_intermediate_unique_payloads_are_refused() {
    let study = Study::new();
    for verification in [
        json!({"unique":[]}),
        json!({"unique":{"columns":[]}}),
        json!({"unique":{"columns":["STUDYID"],"severity":"warning"}}),
        json!({"unique":{"id":null,"columns":["STUDYID"]}}),
        json!({"assert":{"require":"TRUE"}}),
    ] {
        let issues = study.check(json!({
            "id":"DS_EOS", "dataset":"DS", "verifications":[verification]
        }));
        assert!(!issues.is_empty(), "{verification}");
        assert!(
            issues.iter().all(|issue| issue.phase == "validation"
                && issue.condition != "resource_path_missing"
                && issue
                    .spec_paths
                    .iter()
                    .all(|path| path.starts_with("intermediates"))),
            "{verification}: {issues:?}"
        );
    }
}

#[test]
fn original_singular_intermediate_verification_field_stays_unknown() {
    let study = Study::new();
    let issues = study.check(json!({
        "id":"DS_EOS", "dataset":"DS", "verification":{"unique":["USUBJID"]}
    }));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "unknown_field");
    assert_eq!(issues[0].spec_paths, ["intermediates.DS_EOS.verification"]);
}

#[test]
fn original_absent_intermediate_verifications_remain_optional() {
    assert!(Study::new()
        .check(json!({"id":"DS_EOS","dataset":"DS"}))
        .is_empty());
}
