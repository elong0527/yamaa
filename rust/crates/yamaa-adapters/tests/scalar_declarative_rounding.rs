#![cfg(feature = "yaml")]
//! Scalar contracts: declarative text case, declarative text extract,
//! declarative text pad, declarative text template and scalar bytes. The
//! declarative rounding, temporal and temporal imputation cases are data in
//! `tests/fixtures/diagnostics/` (#3798).
mod scalar {
    mod declarative_text_case {
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = crate::support::Scratch::new("str-case");
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
                let identity = crate::support::study::identity("test", &specification);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                )
                .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
            }
        }
        fn golden(fixture: &Fixture) -> yamaa_core::json::Value {
            let expected = yamaa_adapters::yaml_decode::decode_yaml(
                &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
            )
            .unwrap()
            .document;
            fn json(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Text(value) => yamaa_adapters::json!(value),
                    N::Integer(value) => value.parse::<i64>().unwrap().into(),
                    N::Float(value) => yamaa_adapters::json!(value),
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

        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        #[test]
        fn original_sentence_fixture_matches_its_ordered_artifact() {
            let fixture = Fixture::new("schema-sentence-case");
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adlb.csv")).unwrap(),
                fs::read(fixture.0.join("expected/adlb.csv")).unwrap()
            );
        }
        #[test]
        fn original_static_negatives_preserve_complete_findings_before_input_capture() {
            for name in ["negative-str-case-number", "negative-str-case-unknown"] {
                let fixture = Fixture::new(name);
                fs::remove_dir_all(fixture.0.join("input")).unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let identity = crate::support::study::identity(name, &specification);
                let issues = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
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
                // A 1.x specification's schema finding also names the files
                // the run loaded, which neither the golden nor the capture
                // compares (#3422).
                let mut found = issue_json(&issues[0]);
                found["context"] =
                    crate::support::benchmarks::without_layer(issues[0].context.clone());
                assert_eq!(found, golden(&fixture), "{name}");
                let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                    include_str!("fixtures/text_case_reference.json"),
                )
                .unwrap();
                let mut shared = reference[name][0].clone();
                if name == "negative-str-case-unknown" {
                    // The complete independent legacy capture retains loader provenance,
                    // the files that capture loaded.
                    let context = shared["context"].as_object_mut().unwrap();
                    let source = context.remove("source").unwrap();
                    let entry = context.remove("entry").unwrap();
                    assert_eq!(source, entry);
                    assert_eq!(source, "/private/tmp/yamaa-root-text-operators/benchmarks/negative-str-case-unknown/spec.yaml");
                }
                assert_eq!(found, shared, "{name}");
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
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, UP, LOW, SENT, TITLE]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: UP, type: str, derivation: {str_case: {source: SRC.TEXT, to: upper, missing: MiXeD}}}
  - {name: LOW, type: str, derivation: {str_case: {source: SRC.TEXT, to: lower}}}
  - {name: SENT, type: str, derivation: {str_case: {source: SRC.TEXT, to: sentence}}}
  - {name: TITLE, type: str, derivation: {str_case: {source: SRC.TEXT, to: title}}}
"#,
                "ID,TEXT\n1,FOLLOW-UP\n2,\u{00df}\u{0130}\u{00e9}\u{1f642}AbC\n3,a\0BC\n4,\n"
                    .as_bytes(),
            );
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("output.csv")).unwrap(),"ID,UP,LOW,SENT,TITLE\n1,FOLLOW-UP,follow-up,Follow-up,Follow-Up\n2,\u{00df}\u{0130}\u{00e9}\u{1f642}ABC,\u{00df}\u{0130}\u{00e9}\u{1f642}abc,\u{00df}\u{0130}\u{00e9}\u{1f642}abc,\u{00df}\u{0130}\u{00e9}\u{1f642}Abc\n3,A\0BC,a\0bc,A\0bc,A\0Bc\n4,MiXeD,,,\n".as_bytes());
        }
        /// The proper-case benchmarks give their authored output and refusal:
        /// SAS's six delimiters and a declared list, and a list beside
        /// `title` (TEXT-5).
        #[test]
        fn proper_case_benchmarks_reproduce_their_expected_files() {
            for name in [
                "adam-adae-proper-case-terms",
                "negative-str-case-delimiters-mode",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }
        /// `list` as a YAML double-quoted scalar. A list a row declares is
        /// printable ASCII or the tab, which the scalar writes as its escape.
        fn yaml_quoted(list: &str) -> String {
            let mut quoted = String::from('"');
            for scalar in list.chars() {
                match scalar {
                    '"' | '\\' => {
                        quoted.push('\\');
                        quoted.push(scalar);
                    }
                    '\t' => quoted.push_str("\\t"),
                    ' '..='~' => quoted.push(scalar),
                    other => panic!("a declared list holds {other:?}"),
                }
            }
            quoted.push('"');
            quoted
        }
        #[test]
        fn proper_raises_the_first_scalar_and_each_scalar_after_a_delimiter() {
            // The shared proper-case rows (TEXT-5), each read in the column
            // of its list: P0 takes SAS PROPCASE's six delimiters, and each
            // later column one list that a row declares.
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/text_case_reference.json"),
            )
            .unwrap();
            let rows = reference["proper"].as_array().unwrap();
            assert_eq!(rows.len(), 23, "the authored proper-case rows");
            let list = |row: &yamaa_core::json::Value| {
                row.get("delimiters")
                    .map(|list| list.as_str().unwrap().to_owned())
            };
            let mut lists = vec![None];
            for row in rows {
                if !lists.contains(&list(row)) {
                    lists.push(list(row));
                }
            }
            let names = (0..lists.len())
                .map(|index| format!("P{index}"))
                .collect::<Vec<_>>();
            let mut source = format!(
                "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, {}]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n",
                names.join(", ")
            );
            for (name, list) in names.iter().zip(&lists) {
                let delimiters = list
                    .as_deref()
                    .map(|list| format!(", delimiters: {}", yaml_quoted(list)))
                    .unwrap_or_default();
                source.push_str(&format!(
                    "  - {{name: {name}, type: str, derivation: {{str_case: {{source: SRC.TEXT, to: proper{delimiters}}}}}}}\n"
                ));
            }
            let mut input = String::from("ID,TEXT\n");
            for (index, row) in rows.iter().enumerate() {
                let text = row["source"].as_str().unwrap().replace('"', "\"\"");
                input.push_str(&format!("{index:02},\"{text}\"\n"));
            }
            let fixture = bespoke(&source, input.as_bytes());
            let result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let table = result.output().unwrap();
            let text = |name: &str| {
                let column = table.names().position(|written| written == name).unwrap();
                table
                    .batches()
                    .iter()
                    .flat_map(|batch| {
                        batch
                            .column(column)
                            .as_any()
                            .downcast_ref::<arrow_array::StringArray>()
                            .unwrap()
                            .iter()
                            .map(|value| value.map(str::to_owned))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            let ids = text("ID");
            let columns = names.iter().map(|name| text(name)).collect::<Vec<_>>();
            for (index, row) in rows.iter().enumerate() {
                let label = format!("{:?} with {:?}", row["source"], list(row));
                let column = lists
                    .iter()
                    .position(|written| *written == list(row))
                    .unwrap();
                assert_eq!(
                    (ids[index].as_deref(), columns[column][index].as_deref()),
                    (
                        Some(format!("{index:02}").as_str()),
                        row["expected"].as_str()
                    ),
                    "{label}"
                );
            }
        }
        const MISSING_SPEC: &str = r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, TEXT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: TEXT, type: str, derivation: {str_case: {source: SRC.TEXT, to: upper}}}
"#;
        #[test]
        fn runtime_missing_gives_missing_and_counts_the_default_substitution() {
            let fixture = bespoke(MISSING_SPEC, b"ID,TEXT\nrow1,\n");
            let mut result = fixture.run();
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/text_case_reference.json"),
            )
            .unwrap();
            assert_eq!(
                yamaa_adapters::json!(result.issues().iter().map(issue_json).collect::<Vec<_>>()),
                reference["missing"]
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.TEXT.derivation.str_case.missing","handler":"missing","count":1}
                ])
            );
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,TEXT\nrow1,\n"
            );
        }
        #[test]
        fn row_substitution_and_selected_nested_case_preserve_scope_and_branch_effects() {
            let fixture = bespoke(
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, TEXT, RESULT]}
columns:
  - {name: ID, type: str}
  - {name: TEXT, type: str}
  - {name: RESULT, type: str, derivation: {str_case: {source: TEXT, to: sentence}}}
rows:
  - id: records
    dataset: SRC
    derivations:
      ID: SRC.ID
      TEXT: {str_case: {source: SRC.TEXT, to: title}}
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
                r#"yamaa: "1.0"
type: spec
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
        - otherwise: "'safe'"
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
                let source=format!("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{TEXT: {kind}}}}}}}\noutput: {{path: output.csv, columns: [ID, TEXT]}}\ncolumns:\n  - {{name: ID, type: str}}\n  - {{name: TEXT, type: str}}\nrows:\n  - id: records\n    dataset: SRC\n    derivations:\n      ID: SRC.ID\n      TEXT: {{str_case: {{source: SRC.TEXT, to: upper, missing: fallback}}}}\n");
                let fixture = bespoke(&source, b"ID,TEXT\nrow1,1\n");
                fs::remove_file(fixture.0.join("input.csv")).unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let identity = crate::support::study::identity("test", &specification);
                let issues = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
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
                        yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","rule":"TEXT-1","spec_paths":["rows.records.derivations.TEXT.str_case.source"],"context":{"source":"SRC.TEXT","expected":"str","actual":kind}})
                    ]
                );
                assert!(!fixture.0.join("output.csv").exists());
            }
        }
    }
    mod declarative_text_extract {
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = crate::support::Scratch::new("str-extract");
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
                let identity = crate::support::study::identity("test", &specification);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                )
                .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
            }
        }
        fn golden(fixture: &Fixture) -> yamaa_core::json::Value {
            let expected = yamaa_adapters::yaml_decode::decode_yaml(
                &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
            )
            .unwrap()
            .document;
            fn json(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Text(value) => yamaa_adapters::json!(value),
                    N::Integer(value) => value.parse::<i64>().unwrap().into(),
                    N::Float(value) => yamaa_adapters::json!(value),
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

        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        fn bespoke(source: &str, input: &[u8]) -> Fixture {
            let fixture = Fixture::new("adam-adae-text-cleanup");
            fs::write(fixture.0.join("spec.yaml"), source).unwrap();
            fs::write(fixture.0.join("input.csv"), input).unwrap();
            fixture
        }
        fn reference() -> yamaa_core::json::Value {
            yamaa_adapters::json_wire::decode(include_str!("fixtures/text_extract_reference.json"))
                .unwrap()
        }
        fn attempt(
            fixture: &Fixture,
        ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>> {
            let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &specification,
                    environment: None,
                },
                crate::support::study::identity("test", &specification),
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
                    &yamaa_adapters::json_wire::encode(&value["specification"]),
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
            let wide =
                "100000000000000000000000000000000000000000000000000000000000000000000000000000000";
            let source = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, TEXT, RESULT]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: TEXT, type: str, derivation: SRC.TEXT}}\n  - name: RESULT\n    type: str\n    derivation:\n      str_extract: {{source: TEXT, pattern: '(a)', group: {wide}}}\n"
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
                yamaa_adapters::json!({
                    "phase":cause["phase"],"condition":cause["condition"],"rule":cause["rule"],
                    "spec_paths":["columns.RESULT.derivation.str_extract.group"],"context":cause["context"]
                })
            );
            // This tests native integer admission against the independently captured pure
            // expression, not parity with the legacy YAML loader's floating classification.
            assert!(!fixture.0.join("output.csv").exists());
        }
        #[test]
        fn all_invalid_patterns_keep_complete_findings_before_zero_row_execution() {
            let mut columns = vec![
                yamaa_adapters::json!({"name":"ID","type":"str","derivation":"SRC.ID"}),
                yamaa_adapters::json!({"name":"TEXT","type":"str","derivation":"SRC.TEXT"}),
            ];
            let mut names = vec!["ID".to_owned(), "TEXT".to_owned()];
            // Each declared pattern remains a separate finding within fixed width limits.
            for index in 0..60 {
                let name = format!("V{index}");
                names.push(name.clone());
                columns.push(yamaa_adapters::json!({"name":name,"type":"str","derivation":{"str_extract":{"source":"TEXT","pattern":format!("[{}","a".repeat(600)),"group":0}}}));
            }
            let spec = yamaa_adapters::json!({"yamaa":"1.0","type":"spec","domain":"TEST","keys":["ID"],"input":{"SRC":"input.csv"},"output":{"path":"output.csv","columns":names},"columns":columns});
            // Prepared in the language it is written in.
            let document = crate::support::specification_run::prepare(
                yamaa_adapters::json_wire::encode(&spec).as_bytes(),
            );
            let Err(yamaa_adapters::specification_run::Error::Prepare(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )) = yamaa_adapters::specification_run::PreparedRun::prepare(document)
            else {
                panic!("all malformed patterns must retain their language findings")
            };
            assert_eq!(findings.len(), 60);
            let expected = (0..60).map(|index| yamaa_adapters::json!({
        "phase":"validation", "condition":"invalid_regex", "rule":"TEXT-6",
        "spec_paths":[format!("columns.V{index}.derivation.str_extract.pattern")],
        "context":{"pattern":format!("[{}", "a".repeat(600)), "reason":"a character class is never closed"}
    })).collect::<Vec<_>>();
            let fixture = bespoke(&yamaa_adapters::json_wire::encode(&spec), b"ID,TEXT\n");
            for missing in [false, true] {
                if missing {
                    fs::remove_file(fixture.0.join("input.csv")).unwrap();
                }
                let Err(issues) = attempt(&fixture) else {
                    panic!("invalid declarations must precede study access")
                };
                assert_eq!(issues.iter().map(issue_json).collect::<Vec<_>>(), expected);
                assert!(!fixture.0.join("output.csv").exists());
            }
        }
    }
    mod declarative_text_pad {
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = crate::support::Scratch::new("str-pad");
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
        fn golden(fixture: &Fixture) -> yamaa_core::json::Value {
            let expected = yamaa_adapters::yaml_decode::decode_yaml(
                &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
            )
            .unwrap()
            .document;
            fn json(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Text(value) => yamaa_adapters::json!(value),
                    N::Integer(value) => value.parse::<i64>().unwrap().into(),
                    N::Float(value) => yamaa_adapters::json!(value),
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

        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        #[test]
        fn independent_reference_captures_match_full_output_and_reached_findings() {
            let reference: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(include_str!("fixtures/text_pad_reference.json"))
                    .unwrap();
            for (name, case) in reference.as_object().unwrap() {
                let fixture = Fixture::new("negative-str-pad-width");
                fs::write(
                    fixture.0.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                fs::write(fixture.0.join("input.csv"), case["input"].as_str().unwrap()).unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let identity = crate::support::study::identity(name, &specification);
                let mut result = match crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
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
                        // The capture names the file its loader read; a 1.x
                        // schema finding names the file this run loaded, by
                        // its identity.
                        let loaded = yamaa_adapters::json!(fixture.0.identity("spec.yaml"));
                        let context = expected["context"].as_object_mut().unwrap();
                        for field in ["source", "entry"] {
                            assert_eq!(
                                context.insert(field.into(), loaded.clone()).unwrap(),
                                "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                            );
                        }
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
            let identity =
                crate::support::study::identity("negative-str-pad-width", &specification);
            let issues = match crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
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
            // A 1.x specification's schema finding also names the files the
            // run loaded, which the golden does not compare (#3422).
            let mut found = issue_json(&issues[0]);
            found["context"] = crate::support::benchmarks::without_layer(issues[0].context.clone());
            assert_eq!(found, golden(&fixture));
        }
        /// A positive width beyond any representable length is admitted
        /// (TEXT-2). A missing source yields missing without needing it, while
        /// padding a present value, even the empty string, cannot be allocated
        /// and refuses the whole run. `str_pad` declares no handler, so the
        /// successful run counts none.
        #[test]
        fn missing_skips_unrepresentable_positive_width_but_present_refuses_whole_output() {
            let width = "9".repeat(81);
            let attempt = |text: &str| {
                let fixture = Fixture::new("negative-str-pad-width");
                fs::write(
                    fixture.0.join("spec.yaml"),
                    format!(
                        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, TEXT, RESULT]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: TEXT, type: str, derivation: {text}}}\n  - {{name: RESULT, type: str, derivation: {{str_pad: {{source: TEXT, width: {width}}}}}}}\n"
                    ),
                )
                .unwrap();
                fs::write(fixture.0.join("input.csv"), "ID\nrow1\n").unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    crate::support::study::identity("test", &specification),
                );
                (fixture, result)
            };
            let (fixture, result) = attempt("null");
            let mut result = result.unwrap_or_else(|issues| panic!("preparation {issues:?}"));
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,TEXT,RESULT\nrow1,,\n"
            );
            let (fixture, result) = attempt("\"''\"");
            let Err(issues) = result else {
                panic!("a present value cannot be padded to the width")
            };
            assert_eq!(
                issues.iter().map(issue_json).collect::<Vec<_>>(),
                [yamaa_adapters::json!({
                    "phase": "engine",
                    "condition": "engine_rejected",
                    "rule": null,
                    "spec_paths": [],
                    "context": {"step": "build", "code": "result_projection"},
                })]
            );
            assert!(!fixture.0.join("output.csv").exists());
        }
    }
    mod declarative_text_template {
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = crate::support::Scratch::new("str-template");
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
                let identity = crate::support::study::identity("test", &specification);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                )
                .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
            }
        }
        fn golden(fixture: &Fixture) -> yamaa_core::json::Value {
            let expected = yamaa_adapters::yaml_decode::decode_yaml(
                &fs::read(fixture.0.join("expected/error.yaml")).unwrap(),
            )
            .unwrap()
            .document;
            fn json(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Text(value) => yamaa_adapters::json!(value),
                    N::Integer(value) => value.parse::<i64>().unwrap().into(),
                    N::Float(value) => yamaa_adapters::json!(value),
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

        fn issue_json(issue: &yamaa_core::diagnostic::Diagnostic) -> yamaa_core::json::Value {
            crate::support::study::issue_row(issue)
        }
        fn bespoke(source: &str, input: &[u8]) -> Fixture {
            let fixture = Fixture::new("adam-adsl-site-parse");
            fs::write(fixture.0.join("spec.yaml"), source).unwrap();
            fs::write(fixture.0.join("input.csv"), input).unwrap();
            fixture
        }
        fn ascii_json(value: &yamaa_core::json::Value) -> String {
            let raw = yamaa_adapters::json_wire::encode(value);
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
        fn reference() -> yamaa_core::json::Value {
            yamaa_adapters::json_wire::decode(include_str!("fixtures/text_template_reference.json"))
                .unwrap()
        }
        fn attempt(
            fixture: &Fixture,
        ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>> {
            let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &specification,
                    environment: None,
                },
                crate::support::study::identity("test", &specification),
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
                    Err(issues) => {
                        // A finding about the file's text names the file the
                        // case wrote, which the reference writes as `{entry}`.
                        let written = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                        let mut findings =
                            value["findings"].as_array().cloned().unwrap_or_default();
                        for finding in &mut findings {
                            if finding["context"]["path"] == "{entry}" {
                                finding["context"]["path"] =
                                    yamaa_core::json::Value::String(written.clone());
                            }
                        }
                        assert_eq!(
                            issues.iter().map(issue_json).collect::<Vec<_>>(),
                            findings,
                            "{name}"
                        )
                    }
                }
            }
        }
        #[test]
        fn original_grammar_and_complete_reference_parser_parts_qualify() {
            use yamaa_core::text_template::{Part, StringTemplate};
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../yaml/grammar/string-template.yaml");
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
                    let yamaa_core::schema::DocumentNode::Text(written) = &grammar.nodes()[id]
                    else {
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
                            if let Some(previous) = parts.last_mut().filter(|p| p["kind"] == "text")
                            {
                                let merged =
                                    format!("{}{}", previous["value"].as_str().unwrap(), text);
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
                    let yamaa_core::schema::DocumentNode::Text(written) = &grammar.nodes()[id]
                    else {
                        panic!("shape")
                    };
                    assert_eq!(&shape, written);
                    let id = grammar.field(*case, "identifiers").unwrap();
                    let yamaa_core::schema::DocumentNode::Sequence(items) = &grammar.nodes()[id]
                    else {
                        panic!("identifiers")
                    };
                    let mut expected = items
                        .iter()
                        .map(|id| {
                            let yamaa_core::schema::DocumentNode::Text(text) =
                                &grammar.nodes()[*id]
                            else {
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
                let expected = paths.iter().map(|path| yamaa_adapters::json!({
            "phase":"validation", "condition":"invalid_string_template", "rule":"TEXT-3",
            "spec_paths":[path], "context":{"reason":"unmatched_brace"}
        })).collect::<Vec<_>>();
                let fixture = bespoke(&yamaa_adapters::json_wire::encode(&spec), b"ID,A,B,N\n");
                for missing in [false, true] {
                    if missing {
                        fs::remove_file(fixture.0.join("input.csv")).unwrap();
                    }
                    let Err(issues) = attempt(&fixture) else {
                        panic!("invalid declarations must precede study access")
                    };
                    assert_eq!(issues.iter().map(issue_json).collect::<Vec<_>>(), expected);
                    assert!(!fixture.0.join("output.csv").exists());
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
            let fixture = bespoke(&yamaa_adapters::json_wire::encode(&spec), b"ID,A,B,N\n");
            let mut result = attempt(&fixture).unwrap();
            assert!(result.issues().is_empty());
            assert!(result.output().is_some());
            assert!(!fixture.0.join("output.csv").exists());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("output.csv")).unwrap(), b"ID\n");

            let names = std::iter::once("ID".to_owned())
                .chain((0..20).map(|index| format!("X{index}")))
                .collect::<Vec<_>>();
            spec["output"]["columns"] = yamaa_adapters::json!(names);
            let fixture = bespoke(
                &yamaa_adapters::json_wire::encode(&spec),
                b"ID,A,B,N\nrow1,alpha,beta,7\n",
            );
            let mut result = attempt(&fixture).unwrap();
            assert!(result.issues().is_empty());
            let mut expected = names.join(",") + "\nrow1";
            for _ in 0..20 {
                expected.push(',');
                expected.push_str(&"row1".repeat(400));
            }
            expected.push('\n');
            assert!(result.output().is_some());
            assert!(!fixture.0.join("output.csv").exists());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                expected.as_bytes()
            );
        }
    }
    mod scalar_bytes {
        use yamaa_adapters::scalar_bytes::{decode, encode, exact_float, operate};
        use yamaa_core::value::Value;

        /// Every i64 endpoint remains distinct from missing and every raw float bit survives.
        #[test]
        fn lossless_primitive_carriers() {
            for n in [i64::MIN, i64::MAX, 9007199254740993, -2147483648, 0] {
                assert_eq!(decode(1, n.to_string().as_bytes()), Ok(Value::Int(n)));
                let bytes = encode(Value::Int(n));
                assert_eq!(bytes.tag, 1);
                assert_eq!(bytes.payload, n.to_string().as_bytes());
            }
            for bits in [
                0_u64,
                1,
                0x8000000000000000,
                0x7fefffffffffffff,
                0x3ff0000000000000,
            ] {
                let scalar = decode(2, &bits.to_le_bytes()).unwrap();
                assert_eq!(encode(scalar).payload, bits.to_le_bytes());
            }
            for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                assert_eq!(decode(2, &f.to_le_bytes()), Ok(Value::Missing));
            }
            for text in ["", "a\0b", "\u{96ea}\u{1f980}", "e\u{301}", "\u{e9}"] {
                let bytes = encode(Value::Str(text.into()));
                assert_eq!(bytes.tag, 3);
                assert_eq!(decode(3, &bytes.payload), Ok(Value::Str(text.into())));
            }
            assert_eq!(decode(0, &[]), Ok(Value::Missing));
            assert_eq!(decode(4, &[0]), Ok(Value::Bool(false)));
            assert_eq!(decode(4, &[1]), Ok(Value::Bool(true)));
        }

        /// No malformed type, alternate integer spelling or invalid Unicode reaches core.
        #[test]
        fn reject_invalid_payloads() {
            for text in [
                "",
                "01",
                "+1",
                "-0",
                " 1",
                "1.0",
                "9223372036854775808",
                "-9223372036854775809",
            ] {
                assert!(decode(1, text.as_bytes()).is_err());
            }
            for (tag, bytes) in [
                (0, vec![0]),
                (4, vec![2]),
                (4, vec![]),
                (2, vec![0; 7]),
                (2, vec![0; 9]),
                (5, vec![]),
                (3, vec![0xff]),
                (3, vec![0xc0, 0x80]),
                (3, vec![0xed, 0xa0, 0x80]),
                (3, vec![0xf4, 0x90, 0x80, 0x80]),
            ] {
                assert!(decode(tag, &bytes).is_err());
            }
        }

        #[test]
        fn large_scalar_text_keeps_unicode_nul_and_strict_utf8_validation() {
            let text = "\u{e9}\0".repeat(400_000);
            let bytes = encode(Value::Str(text.clone()));
            assert_eq!(bytes.tag, 3);
            assert_eq!(bytes.payload, text.as_bytes());
            assert_eq!(decode(3, &bytes.payload), Ok(Value::Str(text)));
            let mut invalid = bytes.payload;
            invalid.push(0xff);
            assert_eq!(decode(3, &invalid), Err("invalid UTF-8 scalar"));
        }

        /// Scalar operations compose the same exact arithmetic/comparison primitives.
        #[test]
        fn checked_operations_and_comparisons() {
            assert_eq!(
                operate(1, &Value::Int(9007199254740993), Some(&Value::Int(1))),
                Ok(Value::Int(9007199254740994))
            );
            assert!(operate(1, &Value::Int(i64::MAX), Some(&Value::Int(1)))
                .unwrap_err()
                .contains("9223372036854775808"));
            assert!(operate(7, &Value::Int(i64::MIN), None)
                .unwrap_err()
                .contains("9223372036854775808"));
            assert_eq!(
                operate(
                    12,
                    &Value::Int(9007199254740993),
                    Some(&Value::float(9007199254740992.0))
                ),
                Ok(Value::Bool(true))
            );
            assert_eq!(
                operate(1, &Value::Int(1), Some(&Value::float(0.5))),
                Ok(Value::float(1.5))
            );
            assert_eq!(
                operate(4, &Value::Int(3), Some(&Value::Int(2))),
                Ok(Value::float(1.5))
            );
            assert!(operate(4, &Value::Int(3), Some(&Value::Int(0)))
                .unwrap_err()
                .contains("division_by_zero"));
            assert_eq!(
                operate(
                    8,
                    &Value::Str("a\0b".into()),
                    Some(&Value::Str("a\0b".into()))
                ),
                Ok(Value::Bool(true))
            );
            assert_eq!(
                operate(8, &Value::Missing, Some(&Value::Int(1))),
                Ok(Value::Missing)
            );
            assert!(operate(1, &Value::Int(1), Some(&Value::Bool(true))).is_err());
            assert!(operate(8, &Value::Int(1), Some(&Value::Str("1".into()))).is_err());
            assert!(operate(99, &Value::Int(1), Some(&Value::Int(1))).is_err());
            assert!(operate(6, &Value::Int(1), Some(&Value::Int(1))).is_err());
            assert!(operate(1, &Value::Int(1), None).is_err());
        }

        /// Exact conversion at both i64 ends avoids host decimal-parser rounding.
        #[test]
        fn exact_float_conversion_uses_bits() {
            assert_eq!(
                exact_float(&Value::Int(i64::MIN)),
                Ok(Value::float(-9223372036854775808.0))
            );
            assert!(exact_float(&Value::Int(i64::MAX)).is_err());
            assert!(exact_float(&Value::Int(9007199254740993)).is_err());
            assert_eq!(
                exact_float(&Value::Int(9007199254740992)),
                Ok(Value::float(9007199254740992.0))
            );
            assert!(exact_float(&Value::Missing).is_err());
        }

        /// Host epochs cover the entire civil range and drop precision at this boundary.
        #[test]
        fn temporal_host_epochs_have_exact_values() {
            use yamaa_core::temporal::{Date, DateTime, Precision};
            for (year, month, day, days) in [
                (1, 1, 1, -719162.0_f64),
                (1970, 1, 1, 0.0),
                (2000, 2, 29, 11016.0),
                (9999, 12, 31, 2932896.0),
            ] {
                let date = Date::new(year, month, day, Precision::Year).unwrap();
                let bytes = encode(Value::Date(date));
                assert_eq!(bytes.tag, 5);
                assert_eq!(bytes.payload, days.to_le_bytes());
                let Value::Date(decoded) = decode(5, &bytes.payload).unwrap() else {
                    panic!("date")
                };
                assert_eq!(decoded.collected_precision(), Precision::Day);
                assert_eq!(
                    decode(5, &bytes.payload),
                    Ok(Value::Date(
                        Date::new(year, month, day, Precision::Day).unwrap()
                    ))
                );
            }
            for (year, month, day, hour, minute, second, epoch) in [
                (1, 1, 1, 0, 0, 0, -62135596800.0_f64),
                (1969, 12, 31, 23, 59, 59, -1.0),
                (1970, 1, 1, 0, 0, 0, 0.0),
                (9999, 12, 31, 23, 59, 59, 253402300799.0),
            ] {
                let date = Date::new(year, month, day, Precision::Day).unwrap();
                let value = DateTime::new(date, hour, minute, second, Precision::Day).unwrap();
                let bytes = encode(Value::DateTime(value));
                assert_eq!(bytes.tag, 6);
                assert_eq!(bytes.payload, epoch.to_le_bytes());
                let Value::DateTime(decoded) = decode(6, &bytes.payload).unwrap() else {
                    panic!("datetime")
                };
                assert_eq!(decoded.collected_precision(), Precision::Second);
                assert_eq!(
                    decode(6, &bytes.payload),
                    Ok(Value::DateTime(
                        DateTime::new(date, hour, minute, second, Precision::Second).unwrap()
                    ))
                );
            }
        }

        /// Fractional, nonfinite and out-of-calendar epochs never saturate into values.
        #[test]
        fn temporal_host_epochs_reject_invalid_values() {
            for tag in [5, 6] {
                for epoch in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.5, -0.5, 1e13] {
                    assert!(decode(tag, &epoch.to_le_bytes()).is_err());
                }
                assert!(decode(tag, &[0; 7]).is_err());
                assert!(decode(tag, &[0; 9]).is_err());
            }
            for epoch in [-719163.0_f64, 2932897.0] {
                assert!(decode(5, &epoch.to_le_bytes()).is_err());
            }
            for epoch in [-62135596801.0_f64, 253402300800.0] {
                assert!(decode(6, &epoch.to_le_bytes()).is_err());
            }
        }
    }
}
