//! Original cyclic donor reads and valid dependency order through public routes.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;
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
fn cyclic_donor_reads_preserve_original_findings_and_forward_dependencies_execute() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["intermediate_dependencies"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "yamaa-intermediate-dependencies-{}-{}",
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
            example: "intermediate-dependencies",
            specification: path,
            base_directory: ".",
        };
        let failing = !case["issues"].as_array().unwrap().is_empty();
        if failing {
            assert_eq!(
                rows(&file_application::check(request(), identity())),
                case["issues"]
            );
            assert_eq!(
                rows(
                    &file_application::domain(request(), identity())
                        .err()
                        .unwrap()
                ),
                case["issues"]
            );
        }
        for (input, bytes) in case["inputs"].as_object().unwrap() {
            let input = fixture.0.join(input);
            fs::create_dir_all(input.parent().unwrap()).unwrap();
            fs::write(input, bytes.as_str().unwrap()).unwrap();
        }
        for _ in 0..2 {
            let checked = file_application::check(request(), identity());
            assert_eq!(rows(&checked), case["issues"], "{}", case["name"]);
            if failing {
                assert_eq!(
                    rows(
                        &file_application::domain(request(), identity())
                            .err()
                            .unwrap()
                    ),
                    case["issues"]
                );
                assert!(!fixture.0.join("out.csv").exists());
            } else {
                let mut result = file_application::domain(request(), identity()).unwrap();
                assert!(
                    result.issues().is_empty(),
                    "{}: {:?}",
                    case["name"],
                    result.issues()
                );
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join("out.csv")).unwrap(),
                    case["csv"].as_str().unwrap().as_bytes()
                );
                fs::remove_file(fixture.0.join("out.csv")).unwrap();
            }
        }
    }
}
