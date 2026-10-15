//! Unchanged public YAML and independent plain-reference outputs/findings.
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
            "yamaa-expression-key-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, name: &str, case: &serde_json::Value) -> file_application::Domain {
        fs::write(
            self.0.join("spec.yaml"),
            case["specification"].as_str().unwrap(),
        )
        .unwrap();
        for (path, input) in case["inputs"].as_object().unwrap() {
            fs::write(self.0.join(path), input.as_str().unwrap()).unwrap();
        }
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
            specification: &specification,
            base_directory: ".",
        };
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        )
        .unwrap()
    }
    fn check(&self) -> Vec<yamaa_adapters::issue_rows::Issue> {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::check(
            Request {
                specification: &specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "source-key-filter-check",
                specification: &specification,
                base_directory: ".",
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}
#[test]
fn independent_original_language_captures_match_all_output_bytes_and_complete_findings() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/intermediate_key_reference.json")).unwrap();
    for (name, case) in cases.as_object().unwrap() {
        let fixture = Fixture::new();
        let mut result = fixture.run(name, case);
        if let Some(csv) = case.get("csv") {
            assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                csv.as_str().unwrap().as_bytes(),
                "{name}"
            );
        } else {
            assert!(result.output().is_none(), "{name}");
            assert_eq!(
                result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                case["findings"].as_array().unwrap().as_slice(),
                "{name}"
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!fixture.0.join("out.csv").exists());
        }
    }
}

#[test]
fn resolved_source_keys_still_refuse_the_original_bad_donor_filter() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/intermediate_key_reference.json")).unwrap();
    let mut case = cases["dependency_before_bad_filter"].clone();
    // Eliminate the independently captured key-cardinality failure, leaving the
    // same authored bad filter and a matched donor record for it to evaluate.
    case["inputs"]["src.csv"] = serde_json::json!("ID,N\na,7\n");
    let fixture = Fixture::new();
    let mut result = fixture.run("resolved-source-key-bad-filter", &case);
    assert!(fixture.check().is_empty());
    assert_eq!(
        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
        serde_json::json!([{
            "phase": "validation",
            "condition": "incompatible_input_type",
            "requirement": "REQ-0190",
            "spec_paths": ["intermediates[0].filter"],
            "context": {"left_type": "int", "right_type": "str"},
        }])
        .as_array()
        .unwrap()
        .as_slice()
    );
    assert!(result.output().is_none());
    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
    assert!(!fixture.0.join("out.csv").exists());
}

#[test]
fn ordinary_source_keys_keep_static_filter_types_before_key_cardinality() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/intermediate_key_reference.json")).unwrap();
    let mut case = cases["dependency_before_bad_filter"].clone();
    case["specification"] = serde_json::json!(case["specification"].as_str().unwrap().replace(
        "key: {K: {str_pad: {source: SRC.N, width: 8}}}",
        "key: {KINT: SRC.N}"
    ));
    let fixture = Fixture::new();
    let mut result = fixture.run("ordinary-source-key-bad-filter", &case);
    assert!(fixture.check().is_empty());
    assert_eq!(
        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
        serde_json::json!([{
            "phase": "validation", "condition": "incompatible_input_type", "requirement": "REQ-0190",
            "spec_paths": ["intermediates[0].filter"], "context": {"left_type": "int", "right_type": "str"},
        }]).as_array().unwrap().as_slice()
    );
    assert!(result.output().is_none());
    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
    assert!(!fixture.0.join("out.csv").exists());
}
