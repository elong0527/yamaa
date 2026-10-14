use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        let path = std::env::temp_dir().join(format!(
            "yamaa-str-pad-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fn copy(source: &Path, target: &Path) {
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let dst = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir(&dst).unwrap();
                    copy(&entry.path(), &dst);
                } else {
                    fs::copy(entry.path(), dst).unwrap();
                }
            }
        }
        copy(&source, &path);
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn golden(fixture: &Fixture) -> serde_json::Value {
    let expected = yamaa_adapters::yaml_decode::decode_yaml(
        &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
        Default::default(),
    )
    .unwrap()
    .document;
    fn json(d: &yamaa_core::schema::Document, id: usize) -> serde_json::Value {
        use yamaa_core::schema::DocumentNode as N;
        match &d.nodes()[id] {
            N::Null => serde_json::Value::Null,
            N::Text(value) => serde_json::json!(value),
            N::Integer(value) => value.parse::<i64>().unwrap().into(),
            N::Float(value) => serde_json::json!(value),
            N::Boolean(value) => (*value).into(),
            N::Sequence(items) => items.iter().map(|id| json(d, *id)).collect(),
            N::Mapping(items) => items
                .iter()
                .map(|(key, value)| {
                    let N::Text(key) = &d.nodes()[*key] else {
                        panic!("key")
                    };
                    (key.clone(), json(d, *value))
                })
                .collect(),
        }
    }
    json(&expected, expected.root())
}

fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> serde_json::Value {
    serde_json::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()})
}
#[test]
fn independent_reference_captures_match_full_output_and_reached_findings() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/text_pad_reference.json")).unwrap();
    for (name, case) in reference.as_object().unwrap() {
        let fixture = Fixture::new("negative-str-pad-width");
        fs::write(
            fixture.0.join("spec.yaml"),
            case["specification"].as_str().unwrap(),
        )
        .unwrap();
        fs::write(fixture.0.join("input.csv"), case["input"].as_str().unwrap()).unwrap();
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
            specification: &specification,
            base_directory: ".",
        };
        let mut result = match file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            identity,
        ) {
            Ok(result) => Some(result),
            Err(issues) => {
                assert_eq!(name, "width-noninteger");
                assert_eq!(issues.len(), 1);
                let mut expected = case["findings"][0].clone();
                // Keep the complete independent loader capture in the fixture.
                // Native schema diagnostics still omit these loader provenance fields.
                let context = expected["context"].as_object_mut().unwrap();
                assert_eq!(
                    context.remove("source").unwrap(),
                    "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                );
                assert_eq!(
                    context.remove("entry").unwrap(),
                    "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                );
                assert_eq!(issue_json(&issues[0]), expected);
                None
            }
        };
        if let Some(result) = result.as_mut() {
            if let Some(csv) = case.get("csv") {
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join("output.csv")).unwrap(),
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
                assert!(!fixture.0.join("output.csv").exists());
            }
        }
    }
}
#[test]
fn original_noninteger_width_matches_complete_unchanged_normative_golden_before_input() {
    let fixture = Fixture::new("negative-str-pad-width");
    fs::remove_dir_all(fixture.0.join("input")).unwrap();
    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
    let identity = Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "negative-str-pad-width",
        specification: &specification,
        base_directory: ".",
    };
    let issues = match file_application::domain(
        Request {
            specification: &specification,
            environment: None,
        },
        identity,
    ) {
        Err(issues) => issues,
        Ok(_) => panic!("invalid width must fail before input capture"),
    };
    assert_eq!(issues.len(), 1);
    assert_eq!(issue_json(&issues[0]), golden(&fixture));
}
