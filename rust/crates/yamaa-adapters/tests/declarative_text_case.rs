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
            "yamaa-str-case-{}-{}",
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
#[test]
fn original_sentence_fixture_preserves_its_explicit_unimplemented_ordering_blocker() {
    let fixture = Fixture::new("schema-sentence-case");
    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
    let identity = Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "test",
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
        Ok(_) => panic!("original output ordering is still unimplemented"),
        Err(issues) => issues,
    };
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issue_json(&issues[0]),
        serde_json::json!({"phase":"planning","condition":"unsupported_operation","requirement":null,"spec_paths":["output.order_by"],"context":{"operation":"order_by"}})
    );
    assert!(!fixture.0.join("adlb.csv").exists());
}
#[test]
fn original_static_negatives_preserve_complete_findings_before_input_capture() {
    for name in ["negative-str-case-number", "negative-str-case-unknown"] {
        let fixture = Fixture::new(name);
        fs::remove_dir_all(fixture.0.join("input")).unwrap();
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: name,
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
            Ok(result) => {
                assert!(result.output().is_none());
                result.issues().to_vec()
            }
            Err(issues) => issues,
        };
        assert_eq!(issues.len(), 1);
        assert_eq!(issue_json(&issues[0]), golden(&fixture), "{name}");
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/text_case_reference.json")).unwrap();
        let mut shared = reference[name][0].clone();
        if name == "negative-str-case-unknown" {
            // The complete independent legacy capture retains loader provenance.
            // Native schema diagnostics currently publish only normative facts.
            let context = shared["context"].as_object_mut().unwrap();
            let source = context.remove("source").unwrap();
            let entry = context.remove("entry").unwrap();
            assert_eq!(source, entry);
            assert_eq!(source, "/private/tmp/yamaa-root-text-operators/benchmarks/negative-str-case-unknown/spec.yaml");
        }
        assert_eq!(issue_json(&issues[0]), shared, "{name}");
    }
}
fn bespoke(source: &str, input: &[u8]) -> Fixture {
    let fixture = Fixture::new("schema-sentence-case");
    fs::write(fixture.0.join("spec.yaml"), source).unwrap();
    fs::write(fixture.0.join("input.csv"), input).unwrap();
    fixture
}
#[test]
fn four_modes_and_literal_handlers_preserve_unicode_empty_nul_and_original_case() {
    let fixture = bespoke(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, UP, LOW, SENT, TITLE]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: UP, type: str, derivation: {str_case: {source: SRC.TEXT, to: upper, missing: MiXeD}}}
  - {name: LOW, type: str, derivation: {str_case: {source: SRC.TEXT, to: lower, missing: null}}}
  - {name: SENT, type: str, derivation: {str_case: {source: SRC.TEXT, to: sentence, missing: null}}}
  - {name: TITLE, type: str, derivation: {str_case: {source: SRC.TEXT, to: title, missing: null}}}
"#,
        "ID,TEXT\n1,FOLLOW-UP\n2,\u{00df}\u{0130}\u{00e9}\u{1f642}AbC\n3,a\0BC\n4,\n".as_bytes(),
    );
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(fs::read(fixture.0.join("output.csv")).unwrap(),"ID,UP,LOW,SENT,TITLE\n1,FOLLOW-UP,follow-up,Follow-up,Follow-Up\n2,\u{00df}\u{0130}\u{00e9}\u{1f642}ABC,\u{00df}\u{0130}\u{00e9}\u{1f642}abc,\u{00df}\u{0130}\u{00e9}\u{1f642}abc,\u{00df}\u{0130}\u{00e9}\u{1f642}Abc\n3,A\0BC,a\0bc,A\0bc,A\0Bc\n4,MiXeD,,,\n".as_bytes());
}
const MISSING_SPEC: &str = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, TEXT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: TEXT, type: str, derivation: {str_case: {source: SRC.TEXT, to: upper}}}
"#;
#[test]
fn runtime_missing_retains_complete_independent_reference_context_and_keys() {
    let fixture = bespoke(MISSING_SPEC, b"ID,TEXT\nrow1,\n");
    let result = fixture.run();
    assert!(result.output().is_none());
    assert_eq!(result.issues().len(), 1);
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/text_case_reference.json")).unwrap();
    assert_eq!(issue_json(&result.issues()[0]), reference["missing"][0]);
}
#[test]
fn row_substitution_and_selected_nested_case_preserve_scope_and_branch_effects() {
    let fixture = bespoke(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, TEXT, RESULT]}
columns:
  - {name: ID, type: str}
  - {name: TEXT, type: str}
  - {name: RESULT, type: str, derivation: {str_case: {source: TEXT, to: sentence, missing: null}}}
rows:
  - id: records
    dataset: SRC
    derivations:
      ID: SRC.ID
      TEXT: {str_case: {source: SRC.TEXT, to: title, missing: null}}
"#,
        b"ID,TEXT\n1,FOLLOW-UP\n2,\n",
    );
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,TEXT,RESULT\n1,Follow-Up,Follow-up\n2,,\n"
    );
    let fixture = bespoke(
        r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, TEXT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - name: TEXT
    type: str
    derivation:
      case:
        - when: "ID = '1'"
          then:
            case:
              - when: "ID = '1'"
                then: {str_case: {source: SRC.TEXT, to: title, missing: MiXeD}}
              - otherwise: {str_case: {source: SRC.TEXT, to: upper}}
        - otherwise: {literal: safe}
"#,
        b"ID,TEXT\n1,FOLLOW-UP\n2,\n",
    );
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,TEXT\n1,Follow-Up\n2,safe\n"
    );
}

#[test]
fn declared_csv_source_types_are_rejected_in_row_scope_before_data_capture() {
    for kind in ["int", "float", "date", "datetime"] {
        let source=format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{TEXT: {kind}}}}}}}\noutput: {{path: output.csv, columns: [ID, TEXT]}}\ncolumns:\n  - {{name: ID, type: str}}\n  - {{name: TEXT, type: str}}\nrows:\n  - id: records\n    dataset: SRC\n    derivations:\n      ID: SRC.ID\n      TEXT: {{str_case: {{source: SRC.TEXT, to: upper, missing: fallback}}}}\n");
        let fixture = bespoke(&source, b"ID,TEXT\nrow1,1\n");
        fs::remove_file(fixture.0.join("input.csv")).unwrap();
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let identity = Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "test",
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
            Ok(result) => {
                assert!(result.output().is_none());
                result.issues().to_vec()
            }
        };
        assert_eq!(
            issues.iter().map(issue_json).collect::<Vec<_>>(),
            [
                serde_json::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0308","spec_paths":["rows[0].derivations.TEXT.str_case.source"],"context":{"source":"SRC.TEXT","expected":"str","actual":kind}})
            ]
        );
        assert!(!fixture.0.join("output.csv").exists());
    }
}
