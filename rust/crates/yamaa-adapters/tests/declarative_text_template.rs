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
            "yamaa-str-template-{}-{}",
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
    let fixture = Fixture::new("adam-adsl-site-parse");
    fs::write(fixture.0.join("spec.yaml"), source).unwrap();
    fs::write(fixture.0.join("input.csv"), input).unwrap();
    fixture
}
fn ascii_json(value: &serde_json::Value) -> String {
    let raw = serde_json::to_string(value).unwrap();
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}
fn reference() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/text_template_reference.json")).unwrap()
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
fn unchanged_original_site_bytes_and_complete_handler_ledger_qualify() {
    let fixture = Fixture::new("adam-adsl-site-parse");
    let original = fs::read(fixture.0.join("spec.yaml")).unwrap();
    let mut result = fixture.run();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(!fixture.0.join("adsl.csv").exists());
    assert_eq!(
        result.observations()["handler_counts"],
        reference()["adam-adsl-site-parse"]["handler_counts"]
    );
    assert!(result.save().unwrap());
    let bytes = fs::read(fixture.0.join("adsl.csv")).unwrap();
    assert_eq!(
        bytes,
        fs::read(fixture.0.join("expected/adsl.csv")).unwrap()
    );
    assert_eq!(
        bytes,
        reference()["adam-adsl-site-parse"]["csv"]
            .as_str()
            .unwrap()
            .as_bytes()
    );
    assert_eq!(fs::read(fixture.0.join("spec.yaml")).unwrap(), original);
}
#[test]
fn unchanged_original_subject_negative_is_complete_before_source_read() {
    let fixture = Fixture::new("negative-subject-reference");
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
        reference()["negative-subject-reference"]["findings"][0]
    );
}
#[test]
fn all_complete_independent_domain_captures_preserve_ordered_findings_bytes_and_ledgers() {
    let truth = reference();
    for (name, value) in truth.as_object().unwrap() {
        if [
            "adam-adsl-site-parse",
            "negative-subject-reference",
            "grammar_and_effects",
        ]
        .contains(&name.as_str())
        {
            continue;
        }
        let fixture = bespoke(
            &ascii_json(&value["specification"]),
            value["input"].as_str().unwrap().as_bytes(),
        );
        match attempt(&fixture) {
            Ok(mut result) => {
                assert_eq!(
                    result.issues().iter().map(issue_json).collect::<Vec<_>>(),
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
                value["findings"].as_array().cloned().unwrap_or_default(),
                "{name}"
            ),
        }
    }
}
#[test]
fn original_grammar_and_complete_reference_parser_parts_qualify() {
    use yamaa_core::text_template::{Part, StringTemplate};
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml/grammar/string-template.yaml");
    let grammar =
        yamaa_adapters::yaml_decode::decode_yaml(&fs::read(source).unwrap(), Default::default())
            .unwrap()
            .document;
    // Pin the actual authority's full case count independently of the retained capture.
    let id = grammar.field(grammar.root(), "cases").unwrap();
    let yamaa_core::schema::DocumentNode::Sequence(cases) = &grammar.nodes()[id] else {
        panic!("cases")
    };
    let truth = reference();
    let vectors = truth["grammar_and_effects"]["grammar"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    assert_eq!(vectors.len(), cases.len());
    for (case, value) in cases.iter().zip(vectors) {
        let id = grammar.field(*case, "text").unwrap();
        let yamaa_core::schema::DocumentNode::Text(text) = &grammar.nodes()[id] else {
            panic!("text")
        };
        assert_eq!(text, value["text"].as_str().unwrap());
        for name in ["id", "parse"] {
            let id = grammar.field(*case, name).unwrap();
            let yamaa_core::schema::DocumentNode::Text(written) = &grammar.nodes()[id] else {
                panic!("case metadata")
            };
            assert_eq!(written, value["expected"][name].as_str().unwrap());
        }
        let op = StringTemplate::new(
            text.clone(),
            "columns.TEXT.derivation.str_template".into(),
            None,
        )
        .unwrap();
        if let Some(error) = op.declaration_diagnostic() {
            assert_eq!(
                error.definition().condition,
                value["condition"].as_str().unwrap()
            );
            let context: serde_json::Value = error
                .context
                .iter()
                .map(|(key, value)| {
                    let yamaa_core::diagnostic::ContextValue::Scalar(
                        yamaa_core::value::Value::Str(text),
                    ) = value
                    else {
                        panic!("grammar scalar")
                    };
                    (key.clone(), serde_json::json!(text))
                })
                .collect();
            assert_eq!(context, value["context"]);
        } else {
            assert_eq!(
                op.identifiers(),
                &serde_json::from_value::<Vec<String>>(value["identifiers"].clone()).unwrap()
            );
            let mut parts: Vec<serde_json::Value> = vec![];
            for part in op.parts() {
                if let Part::Placeholder(index) = part {
                    parts.push(
                        serde_json::json!({"kind":"placeholder","name":op.identifiers()[*index]}),
                    );
                } else {
                    let text = op.literal(part).unwrap();
                    if let Some(previous) = parts.last_mut().filter(|p| p["kind"] == "text") {
                        let merged = format!("{}{}", previous["value"].as_str().unwrap(), text);
                        previous["value"] = merged.into();
                    } else {
                        parts.push(serde_json::json!({"kind":"text","value":text}));
                    }
                }
            }
            assert_eq!(parts, value["parts"].as_array().unwrap().clone());
            let shape = format!(
                "(template{})",
                parts
                    .iter()
                    .map(|part| if part["kind"] == "placeholder" {
                        format!(" (placeholder {})", part["name"].as_str().unwrap())
                    } else {
                        format!(
                            " (text '{}')",
                            part["value"].as_str().unwrap().replace('\'', "''")
                        )
                    })
                    .collect::<String>()
            );
            let id = grammar.field(*case, "shape").unwrap();
            let yamaa_core::schema::DocumentNode::Text(written) = &grammar.nodes()[id] else {
                panic!("shape")
            };
            assert_eq!(&shape, written);
            let id = grammar.field(*case, "identifiers").unwrap();
            let yamaa_core::schema::DocumentNode::Sequence(items) = &grammar.nodes()[id] else {
                panic!("identifiers")
            };
            let mut expected = items
                .iter()
                .map(|id| {
                    let yamaa_core::schema::DocumentNode::Text(text) = &grammar.nodes()[*id] else {
                        panic!("identifier")
                    };
                    text.clone()
                })
                .collect::<Vec<_>>();
            let mut actual = op.identifiers().to_vec();
            expected.sort();
            actual.sort();
            assert_eq!(actual, expected);
        }
    }
}
#[test]
fn zero_row_whole_metadata_and_effective_row_default_copies_precede_parser_findings() {
    fn admission(spec: serde_json::Value, expected: &'static str) {
        let schema = yamaa_adapters::shipped_schema::capture().unwrap();
        let document = schema
            .prepare_standalone(yamaa_adapters::specification_source::Source {
                identity: "spec.yaml".into(),
                bytes: spec.to_string().into_bytes(),
            })
            .unwrap();
        assert!(
            matches!(yamaa_adapters::specification_run::PreparedRun::prepare(document),Err(yamaa_adapters::specification_run::Error::Prepare(yamaa_core::specification::PrepareError::Limit(resource))) if resource==expected)
        );
        let fixture = bespoke(&spec.to_string(), b"ID,A,B,N\n");
        let Err(issues) = attempt(&fixture) else {
            panic!("whole quota before study")
        };
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].phase, "engine");
        assert_eq!(issues[0].condition, "engine_rejected");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
            serde_json::json!({"code":"compiler_limit","stage":"prepare"})
        );
        assert!(!fixture.0.join("output.csv").exists());
    }
    let mut spec = reference()["literal-only"]["specification"].clone();
    let columns = spec["columns"].as_array_mut().unwrap();
    columns[4]["derivation"] = serde_json::json!({"str_template":format!("{{{}","a".repeat(1600))});
    for index in 0..55 {
        columns.push(serde_json::json!({"name":format!("V{index}"),"type":"str","derivation":{"str_template":format!("{{{}","a".repeat(1600))}}));
    }
    admission(spec, "str_template_metadata");
    let mut spec = reference()["row-template"]["specification"].clone();
    spec["columns"][4]["derivation"] =
        serde_json::json!({"str_template":format!("{{{}","a".repeat(4000))});
    let mut row = spec["rows"][0].clone();
    row["derivations"].as_object_mut().unwrap().remove("RESULT");
    spec["rows"] = (0..16)
        .map(|index| {
            let mut row = row.clone();
            row["id"] = format!("row{index}").into();
            row
        })
        .collect();
    admission(spec, "str_template_metadata");
}
#[test]
fn cumulative_template_query_work_is_admitted_before_all_parser_arenas() {
    let mut spec = reference()["literal-only"]["specification"].clone();
    // A short operation path keeps the independent metadata bound below its
    // limit while total conservative scope/query work exceeds its own bound.
    let columns = spec["columns"].as_array_mut().unwrap();
    columns.truncate(1);
    for index in 0..20 {
        columns.push(serde_json::json!({"name":format!("X{index}"),"type":"str","derivation":{"str_template":"{ID}".repeat(400)}}));
    }
    spec["output"]["columns"] = serde_json::json!(["ID"]);
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
            yamaa_core::specification::PrepareError::Limit("str_template_work")
        ))
    ));
}
