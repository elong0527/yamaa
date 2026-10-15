//! Named row drivers traverse filtered donor records without lookup keys.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issues(rows: &[yamaa_adapters::issue_rows::Issue]) -> serde_json::Value {
    serde_json::json!(rows.iter().map(|row| serde_json::json!({
        "phase":row.phase,"condition":row.condition,"requirement":row.requirement,
        "spec_paths":row.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&row.context).unwrap()
    })).collect::<Vec<_>>())
}
#[test]
fn source_only_named_row_drivers_preserve_lookup_key_admission() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["intermediate_row_drivers"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "yamaa-intermediate-row-drivers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&fixture.0).unwrap();
        for (path, bytes) in case["inputs"].as_object().unwrap() {
            let path = fixture.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes.as_str().unwrap()).unwrap();
        }
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
            example: "intermediate_row_drivers",
            specification: path,
            base_directory: ".",
        };
        assert_eq!(
            issues(&file_application::check(request(), identity())),
            case["check_issues"],
            "{}",
            case["name"]
        );
        let mut result = match file_application::domain(request(), identity()) {
            Ok(result) => result,
            Err(rows) => {
                assert_eq!(issues(&rows), case["issues"], "{}", case["name"]);
                assert!(case["csv"].is_null());
                assert!(!fixture.0.join("out.csv").exists());
                continue;
            }
        };
        assert_eq!(issues(&result.issues()), case["issues"], "{}", case["name"]);
        if let Some(csv) = case["csv"].as_str() {
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
        } else {
            assert!(result.output().is_none());
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!fixture.0.join("out.csv").exists());
        }
    }
}
