#![cfg(feature = "yaml")]
//! Scalar contracts.
#[allow(dead_code)]
mod support;
mod scalar {
    mod scalar_language {
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = std::env::temp_dir().join(format!(
                    "yamaa-scalar-{}-{}",
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

        fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
        }
        fn bespoke(source: &str, input: &[u8]) -> Fixture {
            let fixture = Fixture::new("adam-adae-text-cleanup");
            fs::write(fixture.0.join("spec.yaml"), source).unwrap();
            fs::write(fixture.0.join("input.csv"), input).unwrap();
            fixture
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
        fn unchanged_scalar_benchmarks_and_negative_truth() {
            for name in [
                "negative-greatest-mixed",
                "negative-least-mixed",
                "negative-str-contains-bool-result",
                "negative-str-contains-invalid-regex",
                "negative-usubjid-length",
            ] {
                let fixture = Fixture::new(name);
                let expected = golden(&fixture);
                if name.ends_with("mixed") {
                    fs::remove_dir_all(fixture.0.join("input")).unwrap();
                }
                let issues = match attempt(&fixture) {
                    Ok(result) => result.issues().to_vec(),
                    Err(issues) => issues,
                };
                assert_eq!(
                    issues.iter().map(issue_json).collect::<Vec<_>>(),
                    vec![expected],
                    "{name}"
                );
            }
            let fixture = Fixture::new("schema-str-contains-missing");
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let observed = result.observations();
            assert_eq!(
                observed["handler_counts"][0]["count"],
                yamaa_adapters::json!(2)
            );
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adae.csv")).unwrap(),
                fs::read(fixture.0.join("expected/adae.csv")).unwrap()
            );
        }
        fn spec(derivation: &str, result_type: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{I: int, F: float, D: date}}}}}}\noutput: {{path: output.csv, columns: [ID, RESULT]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: RESULT, type: {result_type}, derivation: {derivation}}}\n")
        }
        #[test]
        fn extremes_skip_missing_and_keep_wide_integers_and_temporal_values() {
            for (operation, result_type, input, expected) in [
                (
                    "{greatest: {sources: [SRC.I, SRC.F]}}",
                    "str",
                    "ID,I,F,D\na,9007199254740993,9007199254740992,2025-01-01\nb,,,2025-01-01\n",
                    "ID,RESULT\na,9007199254740993\nb,\n",
                ),
                (
                    "{least: {sources: [SRC.I, SRC.F]}}",
                    "str",
                    "ID,I,F,D\na,-9007199254740993,-9007199254740992,2025-01-01\nb,,,2025-01-01\n",
                    "ID,RESULT\na,-9007199254740993\nb,\n",
                ),
                (
                    "{greatest: {sources: [SRC.D, SRC.D]}}",
                    "date",
                    "ID,I,F,D\na,1,1,2025-01-01\nb,1,1,2025-01-02\n",
                    "ID,RESULT\na,2025-01-01\nb,2025-01-02\n",
                ),
            ] {
                let fixture = bespoke(&spec(operation, result_type), input.as_bytes());
                let mut result = fixture.run();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join("output.csv")).unwrap(),
                    expected.as_bytes()
                );
            }
        }
        #[test]
        fn nested_concat_keeps_order_and_handler_counts_and_stops_at_missing() {
            let derivation="{str_concat: {sources: [{literal: '['}, {str_case: {source: SRC.TEXT, to: upper, missing: absent}}, {str_concat: {sources: [{literal: ']'}, {source: SRC.PEER}], missing: fallback}}]}}";
            let fixture = bespoke(
        &spec(derivation, "str"),
        b"ID,I,F,D,TEXT,PEER\na,1,1,2025-01-01,abc,z\nb,1,1,2025-01-01,,q\nc,1,1,2025-01-01,xyz,\n",
    );
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,RESULT\na,[ABC]z\nb,[absent]q\nc,[XYZfallback\n"
            );
            let observed = result.observations();
            let counts = observed["handler_counts"].as_array().unwrap();
            assert!(counts.iter().any(|row| row["spec_path"]
                == "columns.RESULT.derivation.str_concat.sources[1].str_case.missing"
                && row["count"] == 1));
            assert!(counts.iter().any(|row| row["spec_path"]
                == "columns.RESULT.derivation.str_concat.sources[2].str_concat.missing"
                && row["count"] == 1));
            let fixture = bespoke(
        &spec(
            "{str_concat: {sources: [{source: SRC.TEXT}, {literal: 42}], missing: fallback}}",
            "str",
        ),
        b"ID,I,F,D,TEXT\na,1,1,2025-01-01,\n",
    );
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,RESULT\na,fallback\n"
            );
        }

        #[test]
        fn concat_missing_fallback_keeps_an_earlier_nested_absent_handler() {
            // Preserve the distinct REQ-0361 case from the retired Python expression
            // control: a later missing value must retain the earlier source's handler.
            let fixture = bespoke(
        &spec(
            "{str_concat: {sources: [{source: {variable: SRC.ABSENT, absent: NA}}, {source: SRC.B}], missing: '?' }}",
            "str",
        ),
        b"ID,I,F,D,B\na,1,1,2025-01-01,\n",
    );
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let counts = yamaa_adapters::json!([
                {"spec_path": "columns.RESULT.derivation.str_concat.missing", "handler": "missing", "count": 1},
                {"spec_path": "columns.RESULT.derivation.str_concat.sources[0].source.absent", "handler": "absent", "count": 1},
            ]);
            assert_eq!(result.observations()["handler_counts"], counts);
            assert_eq!(result.observations()["nodes"][0]["handler_counts"], counts);
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,RESULT\na,?\n"
            );
        }

        #[test]
        fn concat_input_diagnostic_preserves_operand_path_and_blocks_publication() {
            let fixture = bespoke(
                &spec(
                    "{str_concat: {sources: [{literal: x}, {literal: 42}]}}",
                    "str",
                ),
                b"ID,I,F,D\na,1,1,2025-01-01\n",
            );
            let result = fixture.run();
            assert_eq!(result.issues().len(), 1);
            assert_eq!(
                issue_json(&result.issues()[0]),
                yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0308","spec_paths":["columns.RESULT.derivation.str_concat.sources[1]"],"context":{"source":"sources[1]","expected":"str","actual":"int"}})
            );
            assert!(!fixture.0.join("output.csv").exists());
        }
        #[test]
        fn contains_returns_false_and_true_with_single_result_conversion() {
            for (pattern, subject, expected) in [
                ("z", "abc", false),
                ("()", "abc", true),
                ("a|z", "abc", true),
            ] {
                let fixture = bespoke(
                    &spec(
                        &format!("{{str_contains: {{source: SRC.TEXT, pattern: '{pattern}'}}}}"),
                        "str",
                    ),
                    format!("ID,I,F,D,TEXT\na,1,1,2025-01-01,{subject}\n").as_bytes(),
                );
                let result = fixture.run();
                let issue = &result.issues()[0];
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    ("convert", "conversion_failed", Some("REQ-0013"))
                );
                assert_eq!(
                    issue_json(issue)["context"]["value"],
                    yamaa_adapters::json!(expected)
                );
            }
        }
        #[test]
        fn row_scalar_derivations_share_the_same_raw_contract() {
            let source="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {I: int, F: float}}}\noutput: {path: output.csv, columns: [ID, RESULT, MAXIMUM]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: RESULT, type: str}\n  - {name: MAXIMUM, type: int}\nrows:\n  - id: values\n    derivations:\n      ID: SRC.ID\n      RESULT: {str_concat: {sources: [{literal: '['}, {source: SRC.TEXT}, {literal: ']'}]}}\n      MAXIMUM: {greatest: {sources: [SRC.I, SRC.F]}}\n";
            let fixture = bespoke(source, b"ID,I,F,TEXT\na,3,2,abc\n");
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,RESULT,MAXIMUM\na,[abc],3\n"
            );
        }

        #[test]
        fn row_contains_keeps_boolean_conversion_failure_and_refuses_every_save() {
            for (pattern, expected) in [("z", false), ("a", true)] {
                let source = format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, RESULT]}}\ncolumns:\n  - {{name: ID, type: str}}\n  - {{name: RESULT, type: str}}\nrows:\n  - id: values\n    derivations:\n      ID: SRC.ID\n      RESULT: {{str_contains: {{source: SRC.TEXT, pattern: '{pattern}'}}}}\n"
        );
                let fixture = bespoke(&source, b"ID,TEXT\na,abc\n");
                let mut result = fixture.run();
                let issues = result.issues();
                assert_eq!(issues.len(), 1);
                let issue = issue_json(&issues[0]);
                assert_eq!(issue["phase"], "convert");
                assert_eq!(issue["condition"], "conversion_failed");
                assert_eq!(issue["requirement"], "REQ-0013");
                assert_eq!(issue["context"]["value"], yamaa_adapters::json!(expected));
                assert!(result.output().is_none());
                for _ in 0..2 {
                    assert!(result.save().is_err());
                    assert_eq!(
                        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                        vec![issue.clone()]
                    );
                    assert!(!fixture.0.join("output.csv").exists());
                }
            }
        }

        #[test]
        fn csv_text_regex_declarations_are_reported_without_source_capture() {
            for (expression, suffix, condition, requirement) in [
        (
            "{str_extract: {source: SRC.TEXT, pattern: 'a)|(b'}}",
            "str_extract.pattern",
            "invalid_regex",
            "REQ-0827",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '[z-a]'}}",
            "str_extract.pattern",
            "invalid_regex",
            "REQ-0827",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '[]a(]'}}",
            "str_extract.pattern",
            "invalid_regex",
            "REQ-0827",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '(a)', group: 2}}",
            "str_extract.group",
            "regex_group_out_of_range",
            "REQ-0816",
        ),
        (
            "{str_contains: {source: SRC.TEXT, pattern: 'a)|(b'}}",
            "str_contains.pattern",
            "invalid_regex",
            "REQ-0827",
        ),
        (
            "{case: [{when: 'SRC.I > 0', then: {str_extract: {source: SRC.TEXT, pattern: 'a)|(b'}}}, {otherwise: {literal: x}}]}",
            "case[0].then.str_extract.pattern",
            "invalid_regex",
            "REQ-0827",
        ),
    ] {
        for typed in [false, true] {
            let source = spec(expression, "str");
            let source = if typed {
                source.replace("I: int, F: float, D: date", "I: int, F: float, D: date, TEXT: str")
            } else {
                source
            };
            let fixture = bespoke(&source, b"ID,I,F,D,TEXT\na,1,1,2025-01-01,ab\n");
            fs::remove_file(fixture.0.join("input.csv")).unwrap();
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let identity = Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "regex-preflight",
                specification: &path,
                base_directory: ".",
            };
            let checked = file_application::check(
                Request { specification: &path, environment: None },
                identity,
            );
            assert_eq!(checked.len(), 1, "{expression}: {checked:?}");
            assert_eq!(checked[0].condition, condition);
            assert_eq!(checked[0].requirement.as_deref(), Some(requirement));
            assert_eq!(checked[0].spec_paths, [format!("columns.RESULT.derivation.{suffix}")]);
            assert_eq!(attempt(&fixture).err().unwrap(), checked);
            assert!(!fixture.0.join("output.csv").exists());
        }
    }
        }

        #[test]
        fn nontext_csv_source_type_precedes_regex_declaration_findings() {
            let fixture = bespoke(
                &spec("{str_extract: {source: SRC.I, pattern: 'a)|(b'}}", "str"),
                b"ID,I,F,D\na,1,1,2025-01-01\n",
            );
            fs::remove_file(fixture.0.join("input.csv")).unwrap();
            let issues = attempt(&fixture).err().unwrap();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition, "incompatible_input_type");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0308"));
            assert_eq!(
                issues[0].spec_paths,
                ["columns.RESULT.derivation.str_extract.source"]
            );
        }
    }
    mod secondary_scalar_operands {
        //! Implicit joins bind completed keys, preserve laziness and keep source scopes.
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-secondary-scalars-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(
                &self,
            ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>
            {
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
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
            fn saved(&self) -> Vec<u8> {
                let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                assert!(domain.save().unwrap());
                fs::read(self.0.join("result.csv")).unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn row_filters_join_other_inputs_after_keys_and_preserve_eager_predicate_failures() {
            for (driver, filter, expected) in [
                ("SRC", "ID = 3 OR B.N = 40", "ID,VALUE\n2,0\n3,0\n"),
                ("B", "SRC.N >= 20", "ID,VALUE\n2,0\n3,0\n"),
            ] {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("a.csv"), "ID,N\n1,10\n2,20\n3,30\n").unwrap();
                fs::write(fixture.0.join("b.csv"), "ID,N\n2,40\n1,50\n3,60\n").unwrap();
                fs::write(fixture.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {{path: a.csv, types: {{ID: int, N: int}}}}\n  B: {{path: b.csv, types: {{ID: int, N: int}}}}\ncolumns:\n  - {{name: ID, type: int, derivation: {{source: {{variable: {driver}.ID}}}}}}\n  - {{name: VALUE, type: int}}\nrows:\n  - id: selected\n    dataset: {driver}\n    filter: '{filter}'\n    derivations:\n      VALUE: {{literal: 0}}\noutput: {{path: result.csv, columns: [ID, VALUE]}}\n")).unwrap();
                assert_eq!(fixture.saved(), expected.as_bytes());
            }
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), "ID\n3\n").unwrap();
            fs::write(fixture.0.join("b.csv"), "ID,N\n3,60\n3,70\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"), "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {path: a.csv, types: {ID: int}}\n  B: {path: b.csv, types: {ID: int, N: int}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\nrows:\n  - id: selected\n    dataset: SRC\n    filter: 'ID = 3 OR B.N = 40'\n    derivations: {}\noutput: {path: result.csv, columns: [ID]}\n").unwrap();
            let mut result = fixture.run().unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition, "multiple_matches");
            assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0127"));
            assert_eq!(result.issues()[0].spec_paths, ["rows[0].filter"]);
            let context: yamaa_core::json::Value = result.issues()[0].context.clone();
            assert_eq!(context["match_count"], 2);
            assert!(result.save().is_err());
            assert!(!fixture.0.join("result.csv").exists());
        }
        #[test]
        fn nested_row_text_and_date_operands_match_completed_keys_and_skip_unselected_donors() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("a.csv"),
                b"ID,D,USE\na,2020-01-03,Y\nb,2020-01-04,N\nc,2020-01-05,Y\n",
            )
            .unwrap();
            fs::write(
                fixture.0.join("b.csv"),
                b"ID,D,TEXT\nb,2020-01-01,poison1\nb,2020-01-01,poison2\na,2020-01-01,Lower\n",
            )
            .unwrap();
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv, types: {D: date}}, B: {path: b.csv, types: {D: date}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, DAYS]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: TEXT, type: str}\n  - {name: DAYS, type: int}\nrows:\n  - id: records\n    dataset: A\n    derivations:\n      TEXT: {case: [{when: \"A.USE = 'Y'\", then: {str_case: {source: B.TEXT, to: upper, missing: null}}}, {otherwise: {literal: SKIP}}]}\n      DAYS: {case: [{when: \"A.USE = 'Y'\", then: {date_diff: {start: B.D, end: A.D, unit: day}}}, {otherwise: {literal: 0}}]}\n").unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,DAYS\na,LOWER,2\nb,SKIP,0\nc,,\n");
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(
                fixture.0.join("spec.yaml"),
                original.replace(
                    "{str_case: {source: B.TEXT, to: upper, missing: null}}",
                    "{first_available: {sources: [B.TEXT], missing: NONE}}",
                ),
            )
            .unwrap();
            assert_eq!(
                fixture.saved(),
                b"ID,TEXT,DAYS\na,Lower,2\nb,SKIP,0\nc,NONE,\n"
            );
        }
        #[test]
        fn original_secondary_source_fixtures_preserve_frozen_csv_bytes() {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            fn copy(from: &Path, to: &Path) {
                for entry in fs::read_dir(from).unwrap() {
                    let entry = entry.unwrap();
                    if entry.file_name() == "expected" {
                        continue;
                    }
                    let target = to.join(entry.file_name());
                    if entry.file_type().unwrap().is_dir() {
                        fs::create_dir(&target).unwrap();
                        copy(&entry.path(), &target);
                    } else {
                        fs::copy(entry.path(), target).unwrap();
                    }
                }
            }
            for (name, result) in [
                ("adam-advs-bmi", "advs.csv"),
                ("schema-rows-override", "advs.csv"),
                ("sdtm-se-subject-elements", "se.csv"),
            ] {
                let fixture = Fixture::new();
                copy(&root.join(name), &fixture.0);
                let mut domain = fixture
                    .run()
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                assert!(domain.issues().is_empty(), "{name}: {:?}", domain.issues());
                assert!(domain.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join(result)).unwrap(),
                    fs::read(root.join(name).join("expected").join(result)).unwrap(),
                    "{name}"
                );
            }
        }
        #[test]
        fn foreign_driver_scalar_operands_join_primary_records_and_column_predicates_share_keys() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("a.csv"),
                b"ID,TEXT,USE\na,lower,Y\nb,mixed,N\n",
            )
            .unwrap();
            fs::write(fixture.0.join("b.csv"), b"ID,N\nb,2\na,1\nc,3\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv}, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, FLAG]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: TEXT, type: str}\n  - {name: FLAG, type: str, derivation: {flag: \"A.USE = 'Y'\"}}\nrows:\n  - id: beta\n    dataset: B\n    derivations:\n      ID: B.ID\n      TEXT: {str_case: {source: A.TEXT, to: upper, missing: null}}\n").unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,MIXED,\na,LOWER,Y\nc,,\n");
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(fixture.0.join("spec.yaml"),original.replace("TEXT: {str_case: {source: A.TEXT, to: upper, missing: null}}",
        "TEXT: {source: {variable: A.TEXT, filter: \"A.USE = 'Y'\", order_by: [A.ID], keep: first}}" )).unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,,\na,lower,Y\nc,,\n");
        }
        #[test]
        fn forbidden_foreign_numeric_identifier_is_classified_before_donor_evaluation() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,valid\n").unwrap();
            fs::write(fixture.0.join("b.csv"), b"ID,N\na,1\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv}, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: VALUE, type: int}\nrows:\n  - id: alpha\n    dataset: A\n    derivations:\n      VALUE: {compute: {expr: 'B.N + 1'}}\n").unwrap();
            let issues = match fixture.run() {
                Ok(mut domain) => {
                    assert!(domain.output().is_none());
                    assert!(domain.save().is_err());
                    domain.issues()
                }
                Err(issues) => issues,
            };
            assert!(!issues.is_empty());
            assert!(
                issues.iter().all(|issue| issue.phase == "validation"),
                "{issues:?}"
            );
            assert!(
                issues.iter().any(|issue| issue.condition == "unknown_field"
                    && issue.requirement.as_deref() == Some("REQ-0410")
                    && issue.spec_paths == ["rows[0].derivations.VALUE.compute.expr"]),
                "{issues:?}"
            );
            assert!(!fixture.0.join("result.csv").exists());
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            for (operation, path) in [
                (
                    "case: [{when: 'B.N = 1', then: {literal: 1}}, {otherwise: {literal: 0}}]",
                    "rows[0].derivations.VALUE.case[0].when",
                ),
                (
                    "flag: {condition: 'B.N = 1', true_value: 1, false_value: 0}",
                    "rows[0].derivations.VALUE.flag.condition",
                ),
            ] {
                fs::write(
                    fixture.0.join("spec.yaml"),
                    original.replace("compute: {expr: 'B.N + 1'}", operation),
                )
                .unwrap();
                let issues = match fixture.run() {
                    Ok(domain) => domain.issues().to_vec(),
                    Err(issues) => issues,
                };
                assert!(
                    issues.iter().any(|issue| issue.condition == "unknown_field"
                        && issue.requirement.as_deref() == Some("REQ-0410")
                        && issue.spec_paths == [path]),
                    "{issues:?}"
                );
            }
        }

        #[test]
        fn nested_structured_sources_keep_filters_stable_choices_absence_and_lazy_handlers() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("a.csv"),
                b"ID,USE,FALLBACK\na,Y,fallback\nb,N,fallback\nc,Y,fallback\n",
            )
            .unwrap();
            fs::write(fixture.0.join("b.csv"),b"ID,SEQ,USE,TEXT,NUM\na,10,Y,late,2\na,1,Y,early,1\na,1,Y,same-order,1\na,50,N,excluded,9\nb,1,Y,hidden,1\nb,2,Y,hidden2,2\n").unwrap();
            fs::write(
                fixture.0.join("spec.yaml"),
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {A: {path: a.csv}, B: {path: b.csv, types: {SEQ: int}}}
base: A
output: {path: result.csv, columns: [ID, TEXT, FIRST, MAP, ABSENT]}
columns:
  - {name: ID, type: str, derivation: A.ID}
  - {name: TEXT, type: str}
  - {name: FIRST, type: str}
  - {name: MAP, type: int}
  - {name: ABSENT, type: str}
rows:
  - id: records
    dataset: A
    derivations:
      TEXT:
        case:
          - when: "A.USE = 'Y'"
            then: {source: {variable: B.TEXT, filter: "B.USE = 'Y'", order_by: [B.SEQ], keep: last}}
          - otherwise: {literal: SKIP}
      FIRST:
        case:
          - when: "A.USE = 'Y'"
            then:
              first_available:
                sources:
                  - {variable: B.TEXT, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                  - {variable: A.FALLBACK, filter: "A.USE = 'Y'"}
                missing: NONE
          - otherwise: {literal: SKIP}
      MAP:
        case:
          - when: "A.USE = 'Y'"
            then:
              mapping:
                source: {variable: B.NUM, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                dict: {'1': 10, '2': 20}
                missing: 0
          - otherwise: {literal: 0}
      ABSENT:
        case:
          - when: "A.USE = 'Y'"
            then: {source: {variable: B.NOTTHERE, absent: EDGE}}
          - otherwise: {literal: SKIP}
"#,
            )
            .unwrap();
            let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
            assert!(domain.issues().is_empty(), "{:?}", domain.issues());
            assert!(domain.save().unwrap());
            assert_eq!(
        fs::read(fixture.0.join("result.csv")).unwrap(),
        b"ID,TEXT,FIRST,MAP,ABSENT\na,late,late,20,EDGE\nb,SKIP,SKIP,0,SKIP\nc,,fallback,0,EDGE\n"
    );
            let counts = domain.observations()["handler_counts"]
                .as_array()
                .unwrap()
                .clone();
            for (path, handler, count) in [
                (
                    "rows[0].derivations.TEXT.case[0].then.source.multiple_matches",
                    "multiple_matches",
                    1,
                ),
                (
                    "rows[0].derivations.MAP.case[0].then.mapping.missing",
                    "missing",
                    1,
                ),
                (
                    "rows[0].derivations.ABSENT.case[0].then.source.absent",
                    "absent",
                    2,
                ),
            ] {
                assert!(
                    counts.contains(
                        &yamaa_adapters::json!({"spec_path":path,"handler":handler,"count":count})
                    ),
                    "{counts:?}"
                );
            }
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(fixture.0.join("spec.yaml"),original.replace("then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}",
        "then: {str_concat: {sources: [{source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: first}}], missing: null}}" )).unwrap();
            assert_eq!(
        fixture.saved(),
        b"ID,TEXT,FIRST,MAP,ABSENT\na,early,late,20,EDGE\nb,SKIP,SKIP,0,SKIP\nc,,fallback,0,EDGE\n"
    );
            let case = "TEXT:\n        case:\n          - when: \"A.USE = 'Y'\"\n            then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}\n          - otherwise: {literal: SKIP}";
            fs::write(fixture.0.join("spec.yaml"), original.replace(case, "TEXT: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}" )).unwrap();
            assert_eq!(fixture.saved(),b"ID,TEXT,FIRST,MAP,ABSENT\na,late,late,20,EDGE\nb,hidden2,SKIP,0,SKIP\nc,,fallback,0,EDGE\n");
        }

        #[test]
        fn global_columns_join_captured_ports_after_mixed_driver_rows_and_windows() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,primary\n").unwrap();
            fs::write(
                fixture.0.join("b.csv"),
                b"ID,EXTRA,TEXT\nb,ignored,beta\nc,ignored,gamma\n",
            )
            .unwrap();
            fs::write(
                fixture.0.join("spec.yaml"),
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
base: A
input: {A: {path: a.csv}, B: {path: b.csv}}
output: {path: result.csv, columns: [ID, RN, PRIMARY, SECONDARY]}
columns:
  - {name: ID, type: str}
  - {name: ORD, type: int}
  - {name: RN, type: int, derivation: {row_number: {window: {order_by: [ORD, ID]}}}}
  - name: PRIMARY
    type: str
    derivation:
      case:
        - when: 'RN > 0'
          then: A.TEXT
        - otherwise: {literal: SKIP}
  - name: SECONDARY
    type: str
    derivation:
      case:
        - when: 'RN > 0'
          then: B.TEXT
        - otherwise: {literal: SKIP}
rows:
  - {id: alpha, dataset: A, derivations: {ID: A.ID, ORD: {literal: 1}}}
  - {id: beta, dataset: B, derivations: {ID: B.ID, ORD: {literal: 2}}}
"#,
            )
            .unwrap();
            assert_eq!(
                fixture.saved(),
                b"ID,RN,PRIMARY,SECONDARY\na,1,primary,\nb,2,,beta\nc,3,,gamma\n"
            );
            let body = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(fixture.0.join("spec.yaml"),body.replace("then: A.TEXT","then: {source: {variable: A.TEXT, filter: \"A.ID = 'a'\", order_by: [A.ID], keep: first}}" )).unwrap();
            assert_eq!(
                fixture.saved(),
                b"ID,RN,PRIMARY,SECONDARY\na,1,primary,\nb,2,,beta\nc,3,,gamma\n"
            );
        }
    }
    mod run_report {
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-report-witness-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(path.join("input")).unwrap();
                Self(fs::canonicalize(path).unwrap())
            }
            fn benchmark() -> (Self, String) {
                let study = Self::new();
                let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks/schema-verification-log");
                fs::copy(source.join("input/dm.csv"), study.0.join("input/dm.csv")).unwrap();
                let raw = fs::read_to_string(source.join("spec.yaml")).unwrap();
                (study, raw)
            }
            fn try_build(
                &self,
                raw: &str,
            ) -> Result<file_application::Domain, file_application::Failure> {
                fs::write(self.0.join("spec.yaml"), raw).unwrap();
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "report",
                        specification: &path,
                        base_directory: ".",
                    },
                )
            }
            fn build(&self, raw: &str) -> file_application::Domain {
                self.try_build(raw).unwrap()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        const HEADER: &str = "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,COUNT,TOTAL,KEYS,DETAILS\n";
        /// The report as CSV text, read back from a save into `report.csv`.
        fn saved_report(study: &Study, name: &str) -> String {
            fs::read_to_string(study.0.join(name)).unwrap()
        }
        #[test]
        fn original_report_benchmark_matches_the_unchanged_goldens_without_build_writes() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw);
            assert!(result.issues().is_empty());
            assert!(result.output().is_some());
            // One run, input and base row, two column rows and four check rows.
            assert_eq!(result.report_table().rows(), 9);
            assert_eq!(
                result.observations()["artifacts"],
                yamaa_adapters::json!([])
            );
            for name in ["adsl.csv", "adsl-checks.csv"] {
                assert!(!study.0.join(name).exists());
            }
            fs::write(study.0.join("input/dm.csv"), "changed").unwrap();
            fs::remove_file(study.0.join("spec.yaml")).unwrap();
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/schema-verification-log/expected");
            for _ in 0..2 {
                assert!(result.save().unwrap());
                for name in ["adsl.csv", "adsl-checks.csv"] {
                    assert_eq!(
                        fs::read(study.0.join(name)).unwrap(),
                        fs::read(root.join(name)).unwrap(),
                        "{name}"
                    );
                }
                assert_eq!(
                    result.observations()["artifacts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|a| a["name"].as_str().unwrap())
                        .collect::<Vec<_>>(),
                    ["adsl-checks", "adsl"]
                );
                let mut expected: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                    include_str!("fixtures/specs/schema-verification-log.json"),
                )
                .unwrap();
                expected["runtime"] = yamaa_adapters::json!("test");
                expected["runtime_version"] = yamaa_adapters::json!("test");
                expected["engine_version"] = yamaa_adapters::json!(yamaa_core::VERSION);
                expected["example"] = yamaa_adapters::json!("report");
                for group in ["nodes", "tables", "verifications", "callbacks"] {
                    for record in expected[group].as_array_mut().unwrap() {
                        record["specification"] =
                            yamaa_adapters::json!(study.0.join("spec.yaml").to_str().unwrap());
                    }
                }
                assert_eq!(result.observations(), expected);
            }
        }
        #[test]
        fn failed_save_publishes_only_the_report_and_preserves_the_primary() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw.replace("severity: warning", "severity: error"));
            assert!(result.output().is_none());
            // The run, its input and base rows, then the two column checks it reached.
            assert_eq!(result.report_table().rows(), 5);
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition, "range_failed");
            for name in ["adsl.csv", "adsl-checks.csv"] {
                fs::write(study.0.join(name), "sentinel").unwrap();
            }
            fs::remove_file(study.0.join("input/dm.csv")).unwrap();
            fs::remove_file(study.0.join("spec.yaml")).unwrap();
            for _ in 0..2 {
                assert!(result.save().is_err());
                assert_eq!(fs::read(study.0.join("adsl.csv")).unwrap(), b"sentinel");
                let expected = format!(
                    "{HEADER}adsl.csv,run,output,,,,,,failed,range_failed,,,,{{}}\n\
                     adsl.csv,input,input.DM,DM,,,,,,,3,,,\"{{\"\"bytes\"\":66,\"\"path\"\":\"\"input/dm.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     adsl.csv,rows,base,DM,,,,,,,3,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,REQ-0375,error,held,,0,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,REQ-0377,error,violated,range_failed,1,3,\"[{{\"\"USUBJID\"\":\"\"S-2\"\"}}]\",\"{{\"\"column\"\":\"\"AGE\"\"}}\"\n"
                );
                assert_eq!(saved_report(&study, "adsl-checks.csv"), expected);
                assert_eq!(
                    result.observations()["artifacts"].as_array().unwrap().len(),
                    1
                );
                assert_eq!(result.issues().len(), 1);
            }
        }

        #[test]
        fn whole_target_preflight_precedes_the_first_report_write_and_can_retry_held_bytes() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw.replace("path: adsl.csv", "path: absent/adsl.csv"));
            fs::write(study.0.join("adsl-checks.csv"), "old checks").unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("adsl-checks.csv")).unwrap(),
                b"old checks"
            );
            assert_eq!(
                result.observations()["artifacts"],
                yamaa_adapters::json!([])
            );
            fs::create_dir(study.0.join("absent")).unwrap();
            assert!(result.save().unwrap());
            assert!(result.issues().is_empty());
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                2
            );
            assert!(saved_report(&study, "adsl-checks.csv")
                .starts_with(&format!("{HEADER}absent/adsl.csv,run,output,")));
        }
        #[test]
        fn every_run_reports_and_only_a_declared_report_file_is_written() {
            let study = Study::new();
            fs::write(study.0.join("input/input.csv"), "ID\n1\n").unwrap();
            let raw = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let mut undeclared = study.build(raw);
            assert!(undeclared.issues().is_empty());
            // A run, an input, a base and a column row; no handler or check.
            assert_eq!(undeclared.report_table().rows(), 4);
            assert!(undeclared.save().unwrap());
            assert_eq!(
                undeclared.observations()["artifacts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert!(!study.0.join("report.csv").exists());
            let mut declared =
                study.build(&raw.replace("columns: [ID]}", "columns: [ID], report: report.csv}"));
            assert_eq!(declared.run_report(), undeclared.run_report());
            fs::write(study.0.join("report.csv"), "stale").unwrap();
            assert!(declared.save().unwrap());
            assert_eq!(
                saved_report(&study, "report.csv"),
                format!(
                    "{HEADER}out.csv,run,output,,,,,,succeeded,,1,,,{{}}\n\
                     out.csv,input,input.SRC,SRC,,,,,,,1,,,\"{{\"\"bytes\"\":5,\"\"path\"\":\"\"input/input.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     out.csv,rows,base,SRC,,,,,,,1,1,,{{}}\n\
                     out.csv,column,columns.ID,ID,,,,,,,0,1,,{{}}\n"
                )
            );
        }

        #[test]
        fn failed_runs_report_the_prefix_they_reached_and_no_columns() {
            let (study, raw) = Study::benchmark();
            // FAIL must be projected to stay live under pruning (REQ-0638);
            // a dead column's division failure would be discarded with it.
            let raw = raw
                .replace("\nverifications:\n", "\n  - name: FAIL\n    type: float\n    derivation: {compute: {expr: '1 / 0'}}\n\nverifications:\n")
                .replace("columns: [USUBJID, AGE]", "columns: [USUBJID, AGE, FAIL]");
            let mut result = study.build(&raw);
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition, "division_by_zero");
            assert!(result.save().is_err());
            assert!(!study.0.join("adsl.csv").exists());
            assert_eq!(
                saved_report(&study, "adsl-checks.csv"),
                format!(
                    "{HEADER}adsl.csv,run,output,,,,,,failed,division_by_zero,,,,{{}}\n\
                     adsl.csv,input,input.DM,DM,,,,,,,3,,,\"{{\"\"bytes\"\":66,\"\"path\"\":\"\"input/dm.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     adsl.csv,rows,base,DM,,,,,,,3,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,REQ-0375,error,held,,0,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,REQ-0377,warning,violated,range_failed,1,3,\"[{{\"\"USUBJID\"\":\"\"S-2\"\"}}]\",\"{{\"\"column\"\":\"\"AGE\"\"}}\"\n"
                )
            );
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                1
            );
        }

        #[test]
        fn unread_inputs_report_missing_counts_and_null_bytes() {
            let (study, raw) = Study::benchmark();
            fs::remove_file(study.0.join("input/dm.csv")).unwrap();
            let mut result = study.build(&raw);
            assert!(result.output().is_none());
            assert_eq!(result.issues()[0].condition, "resource_path_missing");
            assert!(result.save().is_err());
            assert_eq!(
                saved_report(&study, "adsl-checks.csv"),
                format!(
                    "{HEADER}adsl.csv,run,output,,,,,,failed,resource_path_missing,,,,{{}}\n\
                     adsl.csv,input,input.DM,DM,,,,,,,,,,\"{{\"\"bytes\"\":null,\"\"path\"\":\"\"input/dm.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     adsl.csv,rows,base,DM,,,,,,,,,,{{}}\n"
                )
            );
        }

        #[test]
        fn template_and_handler_rows_count_each_construction_and_answer() {
            let study = Study::new();
            fs::write(
                study.0.join("input/input.csv"),
                "ID,GRP,KEEP,CODE,NUM\n1,g1,Y,A,10\n2,g1,N,B,x\n3,g2,Y,,7\n4,g2,Y,A,8\n",
            )
            .unwrap();
            let mut result = study.build(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID, KIND]
input: {SRC: input/input.csv}
output: {path: out.csv, columns: [ID, KIND, LABEL, N], report: report.csv}
columns:
  - {name: ID, type: str}
  - {name: KIND, type: str}
  - {name: LABEL, type: str}
  - {name: N, type: int}
rows:
  - id: each
    filter: "SRC.KEEP = 'Y'"
    derivations:
      ID: SRC.ID
      KIND: {literal: each}
      LABEL:
        mapping:
          source: SRC.CODE
          dict: {A: Alpha}
          missing: null
          unmapped: Other
      N: {value: {source: SRC.NUM}, unconvertible: -1}
  - id: grouped
    group_by: [SRC.GRP]
    derivations: {ID: SRC.GRP, KIND: {literal: group}, LABEL: {literal: Group}, N: {literal: 0}}
"#,
            );
            assert!(result.issues().is_empty());
            assert!(result.save().unwrap());
            // Record 2 is derived before the filter discards it, so its
            // unmapped code and unconvertible number are both answered.
            assert_eq!(
                saved_report(&study, "report.csv"),
                format!(
                    "{HEADER}out.csv,run,output,,,,,,succeeded,,5,,,{{}}\n\
                     out.csv,input,input.SRC,SRC,,,,,,,4,,,\"{{\"\"bytes\"\":65,\"\"path\"\":\"\"input/input.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     out.csv,rows,rows[0],SRC,,,,,,,3,4,,{{}}\n\
                     out.csv,rows,rows[1],SRC,,,,,,,2,4,,{{}}\n\
                     out.csv,handler,rows[0].derivations.N.unconvertible,unconvertible,,N,REQ-0013,,,,1,,,{{}}\n\
                     out.csv,handler,rows[0].derivations.LABEL.mapping.missing,missing,,LABEL,REQ-0334,,,,1,,,{{}}\n\
                     out.csv,handler,rows[0].derivations.LABEL.mapping.unmapped,unmapped,,LABEL,REQ-0334,,,,1,,,{{}}\n\
                     out.csv,column,columns.ID,ID,,,,,,,0,5,,\"{{\"\"max_length\"\":2}}\"\n\
                     out.csv,column,columns.KIND,KIND,,,,,,,0,5,,\"{{\"\"max_length\"\":5}}\"\n\
                     out.csv,column,columns.LABEL,LABEL,,,,,,,1,5,,\"{{\"\"max_length\"\":5}}\"\n\
                     out.csv,column,columns.N,N,,,,,,,0,5,,{{}}\n"
                )
            );
        }

        #[test]
        fn temporal_handlers_cite_the_condition_they_answer() {
            let study = Study::new();
            fs::write(
                study.0.join("input/input.csv"),
                "ID,DTC,DTM,DUR\n1,2025-03,2025-03-05T10:20,P1D\n2,,bad,bad\n3,bad,,P2D\n",
            )
            .unwrap();
            let mut result = study.build(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input/input.csv, types: {ID: int}}}
output: {path: out.csv, columns: [ID, START, STAMP, SPAN, PR], report: report.csv}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - name: START
    type: date
    derivation: {date_impute: {source: SRC.DTC, month: 1, day: first, missing: null, invalid: null}}
  - name: STAMP
    type: datetime
    derivation: {datetime_impute: {source: SRC.DTM, time: first, missing: null, invalid: null}}
  - name: SPAN
    type: float
    derivation: {duration_value: {source: SRC.DUR, unit: day, invalid: null}}
  - name: PR
    type: str
    derivation: {datetime_precision: {source: STAMP, missing: null}}
"#,
            );
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            // Each handler answers the condition its operation raises without
            // it: record 2's missing date and record 3's missing datetime, the
            // text no reader accepts, and both records whose datetime was
            // answered to missing before its precision was read.
            assert_eq!(
                saved_report(&study, "report.csv"),
                format!(
                    "{HEADER}out.csv,run,output,,,,,,succeeded,,3,,,{{}}\n\
                     out.csv,input,input.SRC,SRC,,,,,,,3,,,\"{{\"\"bytes\"\":68,\"\"path\"\":\"\"input/input.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     out.csv,rows,base,SRC,,,,,,,3,3,,{{}}\n\
                     out.csv,handler,columns.START.derivation.date_impute.missing,missing,,START,REQ-0588,,,,1,,,{{}}\n\
                     out.csv,handler,columns.START.derivation.date_impute.invalid,invalid,,START,REQ-0588,,,,1,,,{{}}\n\
                     out.csv,handler,columns.STAMP.derivation.datetime_impute.missing,missing,,STAMP,REQ-1182,,,,1,,,{{}}\n\
                     out.csv,handler,columns.STAMP.derivation.datetime_impute.invalid,invalid,,STAMP,REQ-1182,,,,1,,,{{}}\n\
                     out.csv,handler,columns.SPAN.derivation.duration_value.invalid,invalid,,SPAN,REQ-1301,,,,1,,,{{}}\n\
                     out.csv,handler,columns.PR.derivation.datetime_precision.missing,missing,,PR,REQ-1183,,,,2,,,{{}}\n\
                     out.csv,column,columns.ID,ID,,,,,,,0,3,,{{}}\n\
                     out.csv,column,columns.START,START,,,,,,,2,3,,{{}}\n\
                     out.csv,column,columns.STAMP,STAMP,,,,,,,2,3,,{{}}\n\
                     out.csv,column,columns.SPAN,SPAN,,,,,,,1,3,,{{}}\n\
                     out.csv,column,columns.PR,PR,,,,,,,2,3,,\"{{\"\"max_length\"\":1}}\"\n"
                )
            );
        }

        #[test]
        #[cfg(feature = "parquet")]
        fn parquet_reports_preserve_governed_logical_rows_and_integer_columns() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw.replace("adsl-checks.csv", "checks.parquet"));
            assert!(result.issues().is_empty());
            assert!(result.save().unwrap());
            let report = result.observations();
            let artifact = report["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["name"] == "checks")
                .unwrap();
            assert_eq!(artifact["profile"], "parquet");
            assert_eq!(artifact["row_count"], 9);
            assert_eq!(artifact["content"], "");
            let table = result.report_table();
            for column in [10, 11] {
                assert_eq!(
                    table.columns()[column].data_type(),
                    &arrow_schema::DataType::Int64
                );
            }
            assert_eq!(
                fs::read(study.0.join("checks.parquet")).unwrap().get(..4),
                Some(b"PAR1".as_slice())
            );
            let range: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(artifact["records"][7].as_str().unwrap())
                    .unwrap();
            assert_eq!(range[1], "check");
            assert_eq!(range[10], 1);
            assert_eq!(range[11], 3);
            assert_eq!(range[12], "[{\"USUBJID\":\"S-2\"}]");
        }
    }
}
