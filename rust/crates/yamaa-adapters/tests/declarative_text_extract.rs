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
            "yamaa-str-extract-{}-{}",
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
    fn run(&self) -> file_application::Domain {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "test",
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
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
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
fn bespoke(source: &str, input: &[u8]) -> Fixture {
    let fixture = Fixture::new("adam-adae-text-cleanup");
    fs::write(fixture.0.join("spec.yaml"), source).unwrap();
    fs::write(fixture.0.join("input.csv"), input).unwrap();
    fixture
}
fn reference() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/text_extract_reference.json")).unwrap()
}
fn attempt(
    fixture: &Fixture,
) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
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
#[test]
fn unchanged_original_adae_bytes_and_complete_handler_ledger_qualify() {
    let fixture = Fixture::new("adam-adae-text-cleanup");
    let original = fs::read(fixture.0.join("spec.yaml")).unwrap();
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(!fixture.0.join("adae.csv").exists());
    let observed = result.observations();
    assert_eq!(
        observed["handler_counts"],
        reference()["adam-adae-text-cleanup"]["handler_counts"]
    );
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("adae.csv")).unwrap(),
        fs::read(fixture.0.join("expected/adae.csv")).unwrap()
    );
    assert_eq!(
        fs::read(fixture.0.join("adae.csv")).unwrap(),
        reference()["adam-adae-text-cleanup"]["csv"]
            .as_str()
            .unwrap()
            .as_bytes()
    );
    assert_eq!(fs::read(fixture.0.join("spec.yaml")).unwrap(), original);
}
#[test]
fn unchanged_original_group_negative_is_complete_and_precedes_input_read() {
    let fixture = Fixture::new("negative-str-uncaptured-group");
    fs::remove_dir_all(fixture.0.join("input")).unwrap();
    let issues = match attempt(&fixture) {
        Ok(result) => result.issues().to_vec(),
        Err(issues) => issues,
    };
    assert_eq!(
        issues.iter().map(issue_json).collect::<Vec<_>>(),
        vec![golden(&fixture)]
    );
    assert_eq!(
        issue_json(&issues[0]),
        reference()["negative-str-uncaptured-group"]["findings"][0]
    );
}
#[test]
fn complete_independent_scalar_captures_and_failures_survive_file_application() {
    let truth = reference();
    for name in [
        "missing",
        "no-match",
        "non-string",
        "invalid-pattern",
        "negative-group",
        "optional-unentered",
        "entered-empty",
        "whole-empty",
        "leftmost-alternative",
        "last-repeated-capture",
        "literal-null",
        "one-final-conversion",
        "combined-nonstring-group",
        "combined-unknown-group",
        "combined-nonstring-invalid-pattern",
        "combined-unknown-invalid-pattern",
        "nested-case",
        "row-extract",
        "scope-before-pattern",
    ] {
        let value = &truth[name];
        let fixture = bespoke(
            &serde_json::to_string(&value["specification"]).unwrap(),
            value["input"].as_str().unwrap().as_bytes(),
        );
        match attempt(&fixture) {
            Ok(mut result) => {
                let findings = result.issues().iter().map(issue_json).collect::<Vec<_>>();
                assert_eq!(
                    findings,
                    value["findings"].as_array().cloned().unwrap_or_default(),
                    "{name}"
                );
                if let Some(csv) = value["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(fixture.0.join("output.csv")).unwrap(),
                        csv.as_bytes(),
                        "{name}"
                    );
                }
                if let Some(handlers) = value.get("handler_counts") {
                    let observed = result.observations();
                    if !observed.is_null() {
                        assert_eq!(observed["handler_counts"], *handlers, "{name}");
                    }
                }
            }
            Err(issues) => assert_eq!(
                issues.iter().map(issue_json).collect::<Vec<_>>(),
                value["findings"].as_array().unwrap().clone(),
                "{name}"
            ),
        }
    }
}
#[test]
fn native_integer_group_preserves_the_independent_pure_expression_truth() {
    let wide = "100000000000000000000000000000000000000000000000000000000000000000000000000000000";
    let source = format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, TEXT, RESULT]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: TEXT, type: str, derivation: SRC.TEXT}}\n  - name: RESULT\n    type: str\n    derivation:\n      str_extract: {{source: TEXT, pattern: '(a)', group: {wide}}}\n"
    );
    let fixture = bespoke(&source, b"ID,TEXT\nrow1,a\n");
    let issues = match attempt(&fixture) {
        Ok(result) => result.issues().to_vec(),
        Err(issues) => issues,
    };
    let truth = reference();
    let cause = &truth["wide-group-pure-expression"]["condition"];
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issue_json(&issues[0]),
        serde_json::json!({
            "phase":cause["phase"],"condition":cause["condition"],"requirement":cause["requirement"],
            "spec_paths":["columns.RESULT.derivation.str_extract.group"],"context":cause["context"]
        })
    );
    // This tests native integer admission against the independently captured pure
    // expression, not parity with the legacy YAML loader's floating classification.
    assert!(!fixture.0.join("output.csv").exists());
}
#[test]
fn whole_declaration_admission_precedes_parser_rejections_even_with_zero_rows() {
    let mut columns = vec![
        serde_json::json!({"name":"ID","type":"str","derivation":"SRC.ID"}),
        serde_json::json!({"name":"TEXT","type":"str","derivation":"SRC.TEXT"}),
    ];
    let mut names = vec!["ID".to_owned(), "TEXT".to_owned()];
    // Stay below the independent 64-column ceiling, so this must reach the
    // extraction scope's aggregate retained-parser admission.
    for index in 0..60 {
        let name = format!("V{index}");
        names.push(name.clone());
        columns.push(serde_json::json!({"name":name,"type":"str","derivation":{"str_extract":{"source":"TEXT","pattern":format!("[{}","a".repeat(600)),"group":0}}}));
    }
    let spec = serde_json::json!({"schema_version":"1.0","domain":"TEST","keys":["ID"],"input":{"SRC":"input.csv"},"output":{"path":"output.csv","columns":names},"columns":columns});
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let document = schema
        .prepare_standalone(yamaa_adapters::specification_source::Source {
            identity: "spec.yaml".into(),
            bytes: spec.to_string().into_bytes(),
        })
        .unwrap();
    assert!(matches!(
        yamaa_adapters::specification_run::PreparedRun::prepare(document),
        Err(yamaa_adapters::specification_run::Error::Prepare(
            yamaa_core::specification::PrepareError::Limit("str_extract_metadata")
        ))
    ));
    let fixture = bespoke(&spec.to_string(), b"ID,TEXT\n");
    let Err(issues) = attempt(&fixture) else {
        panic!("whole request quota")
    };
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "engine_rejected");
    assert_eq!(issues[0].phase, "engine");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
        serde_json::json!({"code":"compiler_limit","stage":"prepare"})
    );
    assert!(!fixture.0.join("output.csv").exists());
}
