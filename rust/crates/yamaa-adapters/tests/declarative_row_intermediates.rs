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
            "yamaa-row-context-{}-{}",
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
fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}
#[test]
fn complete_independent_record_contexts_and_unchanged_original() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/row_intermediate_reference.json")).unwrap();
    for (name, case) in reference["cases"]
        .as_object()
        .unwrap()
        .iter()
        .chain(reference["originals"].as_object().unwrap())
    {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("spec.yaml"),
            case["specification"].as_str().unwrap(),
        )
        .unwrap();
        for (path, input) in case["inputs"].as_object().unwrap() {
            let path = if name.starts_with("adam-") {
                fixture.0.join("input").join(path)
            } else {
                fixture.0.join(path)
            };
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, input.as_str().unwrap()).unwrap();
        }
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
            specification: &specification,
            base_directory: ".",
        };
        let built = file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        );
        let expected = if name == "adam-adlb-end-of-treatment" {
            // Keep the historical capture intact. The remaining native gates are
            // intermediate verifications and derivations; same-input scope is admitted.
            let mut gates = reference["eot_native_gates"].as_array().unwrap().clone();
            assert_eq!(gates.len(), 5);
            assert_eq!(
                gates.pop().unwrap(),
                serde_json::json!({"phase":"planning","condition":"unsupported_operation","requirement":null,"spec_paths":["columns.EOTFL.derivation.flag"],"context":{"operation":"flag"}})
            );
            assert_eq!(
                gates.pop().unwrap(),
                serde_json::json!({"phase":"planning","condition":"unsupported_operation","requirement":null,"spec_paths":["columns.ENDPOINT.derivation.source"],"context":{"operation":"secondary_source_expression"}})
            );
            assert_eq!(
                gates.pop().unwrap(),
                serde_json::json!({"phase":"planning","condition":"unsupported_operation","requirement":null,"spec_paths":["intermediates[1]"],"context":{"operation":"intermediate_scope"}})
            );
            gates
        } else {
            case["findings"].as_array().unwrap().clone()
        };
        let mut result = match built {
            Ok(result) => result,
            Err(issues) => {
                assert_eq!(
                    issues.iter().map(issue_json).collect::<Vec<_>>(),
                    expected,
                    "{name}"
                );
                continue;
            }
        };
        assert_eq!(
            result.observations()["handler_counts"],
            case.get("handler_counts")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "{name} complete handler ledger"
        );
        if name == "adam-adlb-end-of-treatment" {
            assert_eq!(
                result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                expected.as_slice()
            );
        } else if let Some(csv) = case.get("csv") {
            assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
            assert!(result.save().unwrap());
            let output = if name == "adam-adlb-supplb-padded-key" {
                "adlb.csv"
            } else {
                "result.csv"
            };
            assert_eq!(
                fs::read(fixture.0.join(output)).unwrap(),
                csv.as_str().unwrap().as_bytes(),
                "{name}"
            );
            if let Some(expected) = case.get("expected_csv") {
                assert_eq!(csv, expected, "unchanged golden");
            }
        } else {
            assert!(result.output().is_none(), "{name}");
            assert_eq!(
                result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                case["findings"].as_array().unwrap().as_slice(),
                "{name}"
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
        }
    }
}
