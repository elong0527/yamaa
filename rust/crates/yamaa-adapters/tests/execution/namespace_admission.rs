//! The original namespace obligations through complete public check and build.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::domain_entry::Request;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn authored_namespace_refusals_precede_build_and_source_access() {
    let truth: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/host-fixtures/cases.json")).unwrap();
    for case in truth["families"]["original-specifications"]["namespace_admission"]
        .as_array()
        .unwrap()
    {
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "yamaa-namespace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&fixture.0).unwrap();
        let path = fixture.0.join("spec.yaml");
        fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
        // Deliberately absent: an invalid namespace grants no source authority.
        let path = path.to_str().unwrap();
        let request = || Request {
            specification: path,
            environment: None,
        };
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "namespace_admission",
            specification: path,
            base_directory: ".",
        };
        let checked = file_application::check(request(), identity());
        let built = file_application::domain(request(), identity())
            .err()
            .unwrap();
        assert_eq!(checked, built, "{}", case["name"]);
        let rows = checked.iter().map(|row| serde_json::json!({
            "phase":row.phase, "condition":row.condition, "requirement":row.requirement,
            "spec_paths":row.spec_paths, "context":serde_json::from_str::<serde_json::Value>(&row.context).unwrap()
        })).collect::<Vec<_>>();
        assert_eq!(serde_json::json!(rows), case["issues"], "{}", case["name"]);
        assert!(!fixture.0.join("out.csv").exists());
    }
}
