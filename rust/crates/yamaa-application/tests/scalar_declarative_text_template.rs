#![cfg(feature = "yaml")]
//! Text template: what no case of `tests/fixtures/diagnostics/` can state
//! for `str_template` (TEXT-3, #3798). The first test proves the template
//! parser (`yamaa_core::text_template`), whose parts and identifiers no run
//! reports, against the parse cases of `yaml/grammar/string-template.yaml`;
//! it is their only reader. The others are capacity tests: they build long
//! templates and many columns by code and read `PreparedRun::prepare`, a
//! seam no case operation reaches, before the file route.
use std::{fs, path::Path};

fn reference() -> yamaa_core::json::Value {
    yamaa_adapters::json_wire::decode(include_str!("fixtures/text_template_reference.json"))
        .unwrap()
}

#[test]
fn original_grammar_and_complete_reference_parser_parts_qualify() {
    use yamaa_core::text_template::{Part, StringTemplate};
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml/grammar/string-template.yaml");
    let grammar = yamaa_adapters::yaml_decode::decode_yaml(&fs::read(source).unwrap())
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
            let context: yamaa_core::json::Value = error
                .context
                .iter()
                .map(|(key, value)| {
                    let yamaa_core::json::Value::String(text) = value else {
                        panic!("grammar scalar")
                    };
                    (key.clone(), yamaa_adapters::json!(text))
                })
                .collect();
            assert_eq!(context, value["context"]);
        } else {
            assert_eq!(
                op.identifiers(),
                &value["identifiers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
            let mut parts: Vec<yamaa_core::json::Value> = vec![];
            for part in op.parts() {
                if let Part::Placeholder(index) = part {
                    parts.push(
                yamaa_adapters::json!({"kind":"placeholder","name":op.identifiers()[*index]}),
            );
                } else {
                    let text = op.literal(part).unwrap();
                    if let Some(previous) = parts.last_mut().filter(|p| p["kind"] == "text") {
                        let merged = format!("{}{}", previous["value"].as_str().unwrap(), text);
                        previous["value"] = merged.into();
                    } else {
                        parts.push(yamaa_adapters::json!({"kind":"text","value":text}));
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
fn complete_template_findings_precede_zero_rows_and_inherited_row_defaults() {
    fn admission(spec: yamaa_core::json::Value, paths: Vec<String>) {
        let document = crate::support::specification_run::prepare(
            yamaa_adapters::json_wire::encode(&spec).as_bytes(),
        );
        let Err(yamaa_adapters::specification_run::Error::Prepare(
            yamaa_core::specification::PrepareError::Invalid(findings),
        )) = yamaa_adapters::specification_run::PreparedRun::prepare(document)
        else {
            panic!("all malformed templates must retain their language findings")
        };
        assert_eq!(findings.len(), paths.len());
        let expected = paths
            .iter()
            .map(|path| {
                yamaa_adapters::json!({
                    "phase":"validation", "condition":"invalid_string_template", "rule":"TEXT-3",
                    "spec_paths":[path], "context":{"reason":"unmatched_brace"}
                })
            })
            .collect::<Vec<_>>();
        let study = crate::support::study::Study::new("str-template");
        study.write("spec.yaml", yamaa_adapters::json_wire::encode(&spec));
        study.write("input.csv", b"ID,A,B,N\n");
        for missing in [false, true] {
            if missing {
                fs::remove_file(study.path("input.csv")).unwrap();
            }
            let Err(issues) = study.run("spec.yaml") else {
                panic!("invalid declarations must precede study access")
            };
            assert_eq!(
                issues
                    .iter()
                    .map(crate::support::study::issue_row)
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(!Path::new(&study.path("output.csv")).exists());
        }
    }
    let mut spec = reference()["literal-only"]["specification"].clone();
    let columns = spec["columns"].as_array_mut().unwrap();
    columns[4]["derivation"] =
        yamaa_adapters::json!({"str_template":format!("{{{}","a".repeat(1600))});
    let mut paths = vec!["columns.RESULT.derivation.str_template".to_owned()];
    for index in 0..55 {
        columns.push(yamaa_adapters::json!({"name":format!("V{index}"),"type":"str","derivation":{"str_template":format!("{{{}","a".repeat(1600))}}));
        paths.push(format!("columns.V{index}.derivation.str_template"));
    }
    // The added columns must be live under pruning (LAYER-11) for
    // their template findings to be reported at all.
    let mut projected = spec["output"]["columns"].as_array().unwrap().clone();
    projected.extend((0..55).map(|index| yamaa_adapters::json!(format!("V{index}"))));
    spec["output"]["columns"] = yamaa_adapters::json!(projected);
    admission(spec, paths);
    let mut spec = reference()["row-template"]["specification"].clone();
    spec["columns"][4]["derivation"] =
        yamaa_adapters::json!({"str_template":format!("{{{}","a".repeat(4000))});
    let mut row = spec["rows"][0].clone();
    row["derivations"].as_object_mut().unwrap().remove("RESULT");
    spec["rows"] = (0..16)
        .map(|index| {
            let mut row = row.clone();
            row["id"] = format!("row{index}").into();
            row
        })
        .collect();
    admission(spec, vec!["columns.RESULT.derivation.str_template".into()]);
}
#[test]
fn cumulative_template_queries_admit_and_compute_every_projected_value() {
    let mut spec = reference()["literal-only"]["specification"].clone();
    let columns = spec["columns"].as_array_mut().unwrap();
    columns.truncate(1);
    for index in 0..20 {
        columns.push(yamaa_adapters::json!({"name":format!("X{index}"),"type":"str","derivation":{"str_template":"{ID}".repeat(400)}}));
    }
    spec["output"]["columns"] = yamaa_adapters::json!(["ID"]);
    let document = crate::support::specification_run::prepare(
        yamaa_adapters::json_wire::encode(&spec).as_bytes(),
    );
    assert!(yamaa_adapters::specification_run::PreparedRun::prepare(document).is_ok());
    let study = crate::support::study::Study::new("str-template");
    study.write("spec.yaml", yamaa_adapters::json_wire::encode(&spec));
    study.write("input.csv", b"ID,A,B,N\n");
    let mut result = study.run("spec.yaml").unwrap();
    assert!(result.issues().is_empty());
    assert!(result.output().is_some());
    assert!(!Path::new(&study.path("output.csv")).exists());
    assert!(result.save().unwrap());
    assert_eq!(study.saved("output.csv"), b"ID\n");

    let names = std::iter::once("ID".to_owned())
        .chain((0..20).map(|index| format!("X{index}")))
        .collect::<Vec<_>>();
    spec["output"]["columns"] = yamaa_adapters::json!(names);
    let study = crate::support::study::Study::new("str-template");
    study.write("spec.yaml", yamaa_adapters::json_wire::encode(&spec));
    study.write("input.csv", b"ID,A,B,N\nrow1,alpha,beta,7\n");
    let mut result = study.run("spec.yaml").unwrap();
    assert!(result.issues().is_empty());
    let mut expected = names.join(",") + "\nrow1";
    for _ in 0..20 {
        expected.push(',');
        expected.push_str(&"row1".repeat(400));
    }
    expected.push('\n');
    assert!(result.output().is_some());
    assert!(!Path::new(&study.path("output.csv")).exists());
    assert!(result.save().unwrap());
    assert_eq!(study.saved("output.csv"), expected.as_bytes());
}
