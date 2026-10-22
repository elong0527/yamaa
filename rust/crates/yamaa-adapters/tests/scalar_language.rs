#![cfg(feature = "yaml")]
//! Scalar contracts: scalar language, secondary scalar operands and run report.
mod scalar {
    mod scalar_language {
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(name: &str) -> Self {
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                let path = crate::support::Scratch::new("scalar");
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
                file_application::domain(
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
        fn attempt(
            fixture: &Fixture,
        ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>> {
            let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &specification,
                    environment: None,
                },
                crate::support::study::identity("test", &specification),
            )
        }
        #[test]
        fn unchanged_scalar_benchmarks_and_negative_truth() {
            for name in [
                "negative-greatest-mixed",
                "negative-least-mixed",
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
        fn row_scalar_derivations_share_the_same_raw_contract() {
            let source="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {I: int, F: float}}}\noutput: {path: output.csv, columns: [ID, RESULT, MAXIMUM]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: RESULT, type: str}\n  - {name: MAXIMUM, type: int}\nrows:\n  - id: values\n    derivations:\n      ID: SRC.ID\n      RESULT: {str_template: '[{SRC.TEXT}]'}\n      MAXIMUM: {greatest: {sources: [SRC.I, SRC.F]}}\n";
            let fixture = bespoke(source, b"ID,I,F,TEXT\na,3,2,abc\n");
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("output.csv")).unwrap(),
                b"ID,RESULT,MAXIMUM\na,[abc],3\n"
            );
        }

        /// A Boolean converts to no column type, so a row derivation that can
        /// only return one fails before any record is read (VALUE-24).
        #[test]
        fn row_boolean_is_refused_at_validation_and_refuses_every_save() {
            for literal in ["false", "true"] {
                let source = format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, RESULT]}}\ncolumns:\n  - {{name: ID, type: str}}\n  - {{name: RESULT, type: str}}\nrows:\n  - id: values\n    derivations:\n      ID: SRC.ID\n      RESULT: {{literal: {literal}}}\n"
        );
                let fixture = bespoke(&source, b"ID,TEXT\na,abc\n");
                let mut result = fixture.run();
                let issues = result.issues();
                assert_eq!(issues.len(), 1);
                let issue = issue_json(&issues[0]);
                assert_eq!(issue["phase"], "validation");
                assert_eq!(issue["condition"], "conversion_failed");
                assert_eq!(issue["rule"], "VALUE-24");
                assert_eq!(
                    issue["spec_paths"],
                    yamaa_adapters::json!(["columns.RESULT"])
                );
                assert_eq!(
                    issue["context"],
                    yamaa_adapters::json!({"from": "bool", "to": "str"})
                );
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
            for (expression, suffix, condition, rule) in [
        (
            "{str_extract: {source: SRC.TEXT, pattern: 'a)|(b'}}",
            "str_extract.pattern",
            "invalid_regex",
            "TEXT-6",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '[z-a]'}}",
            "str_extract.pattern",
            "invalid_regex",
            "TEXT-6",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '[]a(]'}}",
            "str_extract.pattern",
            "invalid_regex",
            "TEXT-6",
        ),
        (
            "{str_extract: {source: SRC.TEXT, pattern: '(a)', group: 2}}",
            "str_extract.group",
            "regex_group_out_of_range",
            "TEXT-10",
        ),
        (
            "{case: [{when: 'SRC.I > 0', then: {str_extract: {source: SRC.TEXT, pattern: 'a)|(b'}}}, {otherwise: {literal: x}}]}",
            "case[0].then.str_extract.pattern",
            "invalid_regex",
            "TEXT-6",
        ),
    ] {
        for typed in [false, true] {
            let source = spec(expression, "str");
            let source = if typed {
                source.edit("I: int, F: float, D: date", "I: int, F: float, D: date, TEXT: str")
            } else {
                source
            };
            let fixture = bespoke(&source, b"ID,I,F,D,TEXT\na,1,1,2025-01-01,ab\n");
            fs::remove_file(fixture.0.join("input.csv")).unwrap();
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let identity = crate::support::study::identity("regex-preflight", &path);
            let checked = file_application::check(yamaa_core::project_function::Language::Python,
                Request { specification: &path, environment: None },
                identity,
            );
            assert_eq!(checked.len(), 1, "{expression}: {checked:?}");
            assert_eq!(checked[0].condition(), condition);
            assert_eq!(checked[0].rule(), Some(rule));
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
            assert_eq!(issues[0].condition(), "incompatible_input_type");
            assert_eq!(issues[0].rule(), Some("TEXT-1"));
            assert_eq!(
                issues[0].spec_paths,
                ["columns.RESULT.derivation.str_extract.source"]
            );
        }
    }
    mod secondary_scalar_operands {
        //! Implicit joins bind completed keys, preserve laziness and keep source scopes.
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("secondary-scalars");
                Self(path)
            }
            fn run(
                &self,
            ) -> Result<file_application::Domain, Vec<yamaa_core::diagnostic::Diagnostic>>
            {
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    crate::support::study::identity("test", &specification),
                )
            }
            fn saved(&self) -> Vec<u8> {
                let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                assert!(domain.save().unwrap());
                fs::read(self.0.join("result.csv")).unwrap()
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
                fs::write(fixture.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {{path: a.csv, types: {{ID: int, N: int}}}}\n  B: {{path: b.csv, types: {{ID: int, N: int}}}}\ncolumns:\n  - {{name: ID, type: int, derivation: {driver}.ID}}\n  - {{name: VALUE, type: int}}\nrows:\n  - id: selected\n    dataset: {driver}\n    filter: '{filter}'\n    derivations:\n      VALUE: {{literal: 0}}\noutput: {{path: result.csv, columns: [ID, VALUE]}}\n")).unwrap();
                assert_eq!(fixture.saved(), expected.as_bytes());
            }
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), "ID\n3\n").unwrap();
            fs::write(fixture.0.join("b.csv"), "ID,N\n3,60\n3,70\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"), "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {path: a.csv, types: {ID: int}}\n  B: {path: b.csv, types: {ID: int, N: int}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\nrows:\n  - id: selected\n    dataset: SRC\n    filter: 'ID = 3 OR B.N = 40'\n    derivations: {}\noutput: {path: result.csv, columns: [ID]}\n").unwrap();
            let mut result = fixture.run().unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition(), "multiple_matches");
            assert_eq!(result.issues()[0].rule(), Some("LOOKUP-10"));
            assert_eq!(result.issues()[0].spec_paths, ["rows[0].filter"]);
            let context = yamaa_core::json::Value::Object(result.issues()[0].context.clone());
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
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: {path: a.csv, types: {D: date}}, B: {path: b.csv, types: {D: date}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, DAYS]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: TEXT, type: str}\n  - {name: DAYS, type: int}\nrows:\n  - id: records\n    dataset: A\n    derivations:\n      TEXT: {case: [{when: \"A.USE = 'Y'\", then: {str_case: {source: B.TEXT, to: upper, missing: {literal: null}}}}, {otherwise: {literal: SKIP}}]}\n      DAYS: {case: [{when: \"A.USE = 'Y'\", then: {diff: {type: date, start: B.D, end: A.D, unit: day}}}, {otherwise: {literal: 0}}]}\n").unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,DAYS\na,LOWER,2\nb,SKIP,0\nc,,\n");
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(
                fixture.0.join("spec.yaml"),
                original.edit(
                    "{str_case: {source: B.TEXT, to: upper, missing: {literal: null}}}",
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
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: a.csv, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, TEXT, FLAG]}\ncolumns:\n  - {name: ID, type: str}\n  - {name: TEXT, type: str}\n  - {name: FLAG, type: str, derivation: {flag: \"A.USE = 'Y'\"}}\nrows:\n  - id: beta\n    dataset: B\n    derivations:\n      ID: B.ID\n      TEXT: {str_case: {source: A.TEXT, to: upper, missing: {literal: null}}}\n").unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,MIXED,\na,LOWER,Y\nc,,\n");
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            fs::write(fixture.0.join("spec.yaml"),original.edit("TEXT: {str_case: {source: A.TEXT, to: upper, missing: {literal: null}}}",
        "TEXT: {source: {variable: A.TEXT, filter: \"A.USE = 'Y'\", order_by: [A.ID], keep: first}}" )).unwrap();
            assert_eq!(fixture.saved(), b"ID,TEXT,FLAG\nb,,\na,lower,Y\nc,,\n");
        }
        #[test]
        fn forbidden_foreign_numeric_identifier_is_classified_before_donor_evaluation() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,valid\n").unwrap();
            fs::write(fixture.0.join("b.csv"), b"ID,N\na,1\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"),"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {A: a.csv, B: {path: b.csv, types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: VALUE, type: int}\nrows:\n  - id: alpha\n    dataset: A\n    derivations:\n      VALUE: {compute: {expr: 'B.N + 1'}}\n").unwrap();
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
                issues.iter().all(|issue| issue.phase() == "validation"),
                "{issues:?}"
            );
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.condition() == "unknown_field"
                        && issue.rule() == Some("FORMULA-9")
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
                    original.edit("compute: {expr: 'B.N + 1'}", operation),
                )
                .unwrap();
                let issues = match fixture.run() {
                    Ok(domain) => domain.issues().to_vec(),
                    Err(issues) => issues,
                };
                assert!(
                    issues
                        .iter()
                        .any(|issue| issue.condition() == "unknown_field"
                            && issue.rule() == Some("FORMULA-9")
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
input: {A: a.csv, B: {path: b.csv, types: {SEQ: int}}}
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
            fs::write(fixture.0.join("spec.yaml"),original.edit("then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}",
        "then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: first}}" )).unwrap();
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
input: {A: a.csv, B: b.csv}
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
            fs::write(fixture.0.join("spec.yaml"),body.edit("then: A.TEXT","then: {source: {variable: A.TEXT, filter: \"A.ID = 'a'\", order_by: [A.ID], keep: first}}" )).unwrap();
            assert_eq!(
                fixture.saved(),
                b"ID,RN,PRIMARY,SECONDARY\na,1,primary,\nb,2,,beta\nc,3,,gamma\n"
            );
        }
    }
    mod run_report {
        use crate::support::Edit as _;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::new("report-witness");
                fs::create_dir_all(path.join("input")).unwrap();
                Self(path.resolved(std::fs::canonicalize))
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
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("report", &path),
                )
            }
            fn build(&self, raw: &str) -> file_application::Domain {
                self.try_build(raw).unwrap()
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
            // One run, input and base row, two column rows and three check rows.
            assert_eq!(result.report_table().rows(), 8);
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
            let mut result = study.build(&raw.edit("severity: warning", "severity: error"));
            assert!(result.output().is_none());
            // The run, its input and base rows, then the two column checks it reached.
            assert_eq!(result.report_table().rows(), 5);
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition(), "range_failed");
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
                     adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,CHECK-5,error,held,,0,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,CHECK-7,error,violated,range_failed,1,3,\"[{{\"\"USUBJID\"\":\"\"S-2\"\"}}]\",\"{{\"\"column\"\":\"\"AGE\"\"}}\"\n"
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
            let mut result = study.build(&raw.edit("path: adsl.csv", "path: absent/adsl.csv"));
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
                study.build(&raw.edit("columns: [ID]}", "columns: [ID], report: report.csv}"));
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
            let (study, _) = Study::benchmark();
            // The benchmark as 1.0 writes it, since the test edits its text
            // (#3422). FAIL must be projected to stay live under pruning
            // (LAYER-11); a dead column's division failure would be
            // discarded with it.
            let raw = crate::support::benchmarks::converted("schema-verification-log")
                .edit("\nverifications:\n", "\n  - name: FAIL\n    type: float\n    derivation: '1 / 0'\n\nverifications:\n")
                .edit("columns: [USUBJID, AGE]", "columns: [USUBJID, AGE, FAIL]");
            let mut result = study.build(&raw);
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition(), "division_by_zero");
            assert!(result.save().is_err());
            assert!(!study.0.join("adsl.csv").exists());
            assert_eq!(
                saved_report(&study, "adsl-checks.csv"),
                format!(
                    "{HEADER}adsl.csv,run,output,,,,,,failed,division_by_zero,,,,{{}}\n\
                     adsl.csv,input,input.DM,DM,,,,,,,3,,,\"{{\"\"bytes\"\":66,\"\"path\"\":\"\"input/dm.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     adsl.csv,rows,base,DM,,,,,,,3,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[0].not_missing,not_missing,,AGE,CHECK-5,error,held,,0,3,,{{}}\n\
                     adsl.csv,check,columns.AGE.verifications[1].range,range,,AGE,CHECK-7,warning,violated,range_failed,1,3,\"[{{\"\"USUBJID\"\":\"\"S-2\"\"}}]\",\"{{\"\"column\"\":\"\"AGE\"\"}}\"\n"
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
            assert_eq!(result.issues()[0].condition(), "resource_path_missing");
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
                     out.csv,handler,rows[0].derivations.N.unconvertible,unconvertible,,N,VALUE-3,,,,1,,,{{}}\n\
                     out.csv,handler,rows[0].derivations.LABEL.mapping.missing,missing,,LABEL,,,,,1,,,{{}}\n\
                     out.csv,handler,rows[0].derivations.LABEL.mapping.unmapped,unmapped,,LABEL,COLUMN-10,,,,1,,,{{}}\n\
                     out.csv,column,columns.ID,ID,,,,,,,0,5,,\"{{\"\"max_length\"\":2}}\"\n\
                     out.csv,column,columns.KIND,KIND,,,,,,,0,5,,\"{{\"\"max_length\"\":5}}\"\n\
                     out.csv,column,columns.LABEL,LABEL,,,,,,,1,5,,\"{{\"\"max_length\"\":5}}\"\n\
                     out.csv,column,columns.N,N,,,,,,,0,5,,{{}}\n"
                )
            );
        }

        /// One specification written in 0.5 and in 1.0. S-2 has no
        /// subject-level record and S-3 is outside the safety population, so
        /// the row template's read of SAFETY answers both and its read of
        /// ADSL through the implicit join answers S-2. Each handler row names
        /// the column it derives, whichever way the file writes the row
        /// template: `rows[0]` in 0.5, `rows.event` in 1.0 (P-CLR-2). A 1.0
        /// file counts both answers under `absent` (D202), and each cites
        /// LOOKUP-24 (D204); a 0.5 file counts SAFETY's under `no_match`,
        /// citing LOOKUP-24, and cites NAME-3 for the join's `absent`, as at
        /// the `yamaa-v0.5.0` tag (D75).
        #[test]
        fn a_row_template_s_handler_rows_name_their_column_and_cite_their_surface_s_rule() {
            let body = r#"output: {path: out.csv, columns: [USUBJID, ARM, TRTSDT], report: report.csv}
columns:
  - {name: USUBJID, type: str}
  - {name: ARM, type: str}
  - {name: TRTSDT, type: str}
rows:
  - id: event
    dataset: AE
    derivations:
      USUBJID: AE.USUBJID
      ARM: SAFETY.ARM
      TRTSDT: ADSL.TRTSDT
"#;
            for (surface, header, constructed) in [
                (
                    "0.5",
                    r#"schema_version: '1.0'
domain: ADAE
keys: [USUBJID]
input:
  AE: input/ae.csv
  ADSL: input/adsl.csv
intermediates:
  - {id: SAFETY, dataset: ADSL, filter: "ADSL.SAFFL = 'Y'"}
"#,
                    "out.csv,rows,rows[0],AE,,,,,,,3,3,,{}\n\
                     out.csv,handler,rows[0].derivations.ARM.source.no_match,no_match,,ARM,LOOKUP-24,,,,2,,,{}\n\
                     out.csv,handler,rows[0].derivations.TRTSDT.source.absent,absent,,TRTSDT,NAME-3,,,,1,,,{}\n",
                ),
                (
                    "1.0",
                    r#"yamaa: "1.0"
type: spec
domain: ADAE
keys: [USUBJID]
input:
  AE: input/ae.csv
  ADSL: {path: input/adsl.csv, key: [USUBJID]}
intermediates:
  - {id: SAFETY, dataset: ADSL, key: [USUBJID], filter: "ADSL.SAFFL = 'Y'"}
"#,
                    "out.csv,rows,rows.event,AE,,,,,,,3,3,,{}\n\
                     out.csv,handler,rows.event.derivations.ARM.source.absent,absent,,ARM,LOOKUP-24,,,,2,,,{}\n\
                     out.csv,handler,rows.event.derivations.TRTSDT.source.absent,absent,,TRTSDT,LOOKUP-24,,,,1,,,{}\n",
                ),
            ] {
                let study = Study::new();
                fs::write(study.0.join("input/ae.csv"), "USUBJID\nS-1\nS-2\nS-3\n").unwrap();
                fs::write(
                    study.0.join("input/adsl.csv"),
                    "USUBJID,ARM,TRTSDT,SAFFL\nS-1,drug a,2025-01-10,Y\nS-3,drug b,2025-02-01,N\n",
                )
                .unwrap();
                let mut result = study.build(&format!("{header}{body}"));
                assert!(result.issues().is_empty(), "{surface}: {:?}", result.issues());
                assert!(result.save().unwrap(), "{surface}");
                assert_eq!(
                    fs::read_to_string(study.0.join("out.csv")).unwrap(),
                    "USUBJID,ARM,TRTSDT\nS-1,drug a,2025-01-10\nS-2,,\nS-3,,2025-02-01\n",
                    "{surface}"
                );
                assert_eq!(
                    saved_report(&study, "report.csv"),
                    format!(
                        "{HEADER}out.csv,run,output,,,,,,succeeded,,3,,,{{}}\n\
                         out.csv,input,input.AE,AE,,,,,,,3,,,\"{{\"\"bytes\"\":20,\"\"path\"\":\"\"input/ae.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                         out.csv,input,input.ADSL,ADSL,,,,,,,2,,,\"{{\"\"bytes\"\":73,\"\"path\"\":\"\"input/adsl.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                         {constructed}\
                         out.csv,column,columns.USUBJID,USUBJID,,,,,,,0,3,,\"{{\"\"max_length\"\":3}}\"\n\
                         out.csv,column,columns.ARM,ARM,,,,,,,2,3,,\"{{\"\"max_length\"\":6}}\"\n\
                         out.csv,column,columns.TRTSDT,TRTSDT,,,,,,,1,3,,\"{{\"\"max_length\"\":10}}\"\n"
                    ),
                    "{surface}"
                );
            }
        }

        /// The report's handler rows and the run's handler counts, as
        /// `(path, count)` pairs in report order.
        fn handler_rows(study: &Study, result: &file_application::Domain) -> Vec<String> {
            let rows: Vec<String> = saved_report(study, "report.csv")
                .lines()
                .filter(|line| line.starts_with("out.csv,handler,"))
                .map(String::from)
                .collect();
            let counts: Vec<String> = result.observations()["handler_counts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    format!(
                        "{},{}",
                        entry["spec_path"].as_str().unwrap(),
                        entry["count"].as_i64().unwrap()
                    )
                })
                .collect();
            let reported: Vec<String> = rows
                .iter()
                .map(|row| {
                    let cells: Vec<&str> = row.split(',').collect();
                    format!("{},{}", cells[2], cells[10])
                })
                .collect();
            assert_eq!(reported, counts, "report rows and engine counts agree");
            rows
        }

        #[test]
        fn handler_rows_register_every_path_at_zero_in_one_order() {
            let study = Study::new();
            fs::write(
                study.0.join("input/src.csv"),
                "ID,NUM,CODE,ALT\n1,10,A,x\n2,y,,z\n3,30,B,\n",
            )
            .unwrap();
            fs::write(
                study.0.join("input/lkp.csv"),
                "ID,SEQ,VAL,KEEP\n1,1,one,Y\n1,2,two,N\n2,1,uno,N\n",
            )
            .unwrap();
            let mut result = study.build(
                r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: input/src.csv, types: {ID: int}}
  LKP: {path: input/lkp.csv, types: {ID: int, SEQ: int}}
output: {path: out.csv, columns: [ID, N, LAST, UP, ANY, SELFV, MUSTV, BYC], report: report.csv}
intermediates:
  - id: PICK
    dataset: LKP
    key: [ID]
    filter: "LKP.KEEP = 'Y'"
    order_by: [LKP.SEQ]
    keep: last
  - id: SELFREF
    dataset: SRC
    key: [ID]
    filter: "SRC.ID IS NOT NULL"
  - id: MUST
    dataset: SRC
    key: [ID]
    filter: "SRC.ID IS NOT NULL"
    require_match: true
  - id: BYCODE
    dataset: LKP
    key:
      VAL: {mapping: {source: SRC.CODE, dict: {A: one, B: dos}}}
    no_match: {literal: null}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: N, type: int, derivation: {value: {source: SRC.NUM}, unconvertible: -1}}
  - {name: LAST, type: str, derivation: PICK.VAL}
  - {name: UP, type: str, derivation: {str_case: {source: PICK.VAL, to: upper}}}
  - {name: ANY, type: str, derivation: {first_available: {sources: [PICK.VAL, SRC.ALT]}}}
  - {name: SELFV, type: str, derivation: SELFREF.CODE}
  - {name: MUSTV, type: str, derivation: MUST.ALT}
  - {name: BYC, type: int, derivation: BYCODE.SEQ}
"#,
            );
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("out.csv")).unwrap(),
                "ID,N,LAST,UP,ANY,SELFV,MUSTV,BYC\n\
                 1,10,one,ONE,one,A,x,1\n\
                 2,-1,,,z,,z,\n\
                 3,30,,,,B,,\n"
            );
            // Every path is registered before any row: the conversion
            // handler first, then each column derivation's paths in
            // expression order, then the intermediate key expression. A
            // read of PICK counts `multiple_matches` (its `keep` never chose
            // among several) and `no_match`, declared or not; SELFREF's read
            // never misses and MUST's read requires a match, so it has none.
            // A read inside a `first_available` operand registers up front too.
            assert_eq!(
                handler_rows(&study, &result),
                [
                    "out.csv,handler,columns.N.derivation.unconvertible,unconvertible,,N,VALUE-3,,,,1,,,{}",
                    "out.csv,handler,columns.LAST.derivation.source.multiple_matches,multiple_matches,,LAST,LOOKUP-10,,,,0,,,{}",
                    "out.csv,handler,columns.LAST.derivation.source.no_match,no_match,,LAST,LOOKUP-24,,,,2,,,{}",
                    "out.csv,handler,columns.UP.derivation.str_case.missing,missing,,UP,,,,,2,,,{}",
                    "out.csv,handler,columns.UP.derivation.str_case.source.multiple_matches,multiple_matches,,UP,LOOKUP-10,,,,0,,,{}",
                    "out.csv,handler,columns.UP.derivation.str_case.source.no_match,no_match,,UP,LOOKUP-24,,,,2,,,{}",
                    "out.csv,handler,columns.ANY.derivation.first_available.missing,missing,,ANY,,,,,1,,,{}",
                    "out.csv,handler,columns.ANY.derivation.first_available.sources[0].multiple_matches,multiple_matches,,ANY,LOOKUP-10,,,,0,,,{}",
                    "out.csv,handler,columns.ANY.derivation.first_available.sources[0].no_match,no_match,,ANY,LOOKUP-24,,,,2,,,{}",
                    "out.csv,handler,columns.SELFV.derivation.source.no_match,no_match,,SELFV,LOOKUP-24,,,,0,,,{}",
                    "out.csv,handler,columns.BYC.derivation.source.no_match,no_match,,BYC,LOOKUP-24,,,,2,,,{}",
                    "out.csv,handler,intermediates[3].key.VAL.mapping.missing,missing,,,,,,,1,,,{}",
                ]
            );
        }

        #[test]
        fn intermediate_derivation_handlers_count_each_build_of_the_donor_records() {
            let study = Study::new();
            fs::write(
                study.0.join("input/src.csv"),
                "ID,REF,TAG\n1,5,7\n2,,x\n3,1,\n",
            )
            .unwrap();
            fs::write(
                study.0.join("input/lab.csv"),
                "ID,SEQ,DAY,FLAG\n1,1,2,H\n1,2,6,\n2,1,4,L\n",
            )
            .unwrap();
            let mut result = study.build(
                r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: input/src.csv, types: {ID: int, REF: int}}
  LAB: {path: input/lab.csv, types: {ID: int, SEQ: int, DAY: int}}
output: {path: out.csv, columns: [ID, CAT, NEAR, TAGN], report: report.csv}
intermediates:
  - id: CODED
    dataset: LAB
    key: [ID]
    derivations:
      GRADE: {mapping: {source: FLAG, dict: {H: high, L: low}}}
      FLAGN: {type: int, derivation: FLAG, unconvertible: {literal: null}}
    order_by: [LAB.SEQ]
    keep: first
  - id: NEAREST
    dataset: LAB
    key: [ID]
    derivations:
      DIST: {compute: {expr: "ABS(DAY - SRC.REF)"}}
      G2: CODED.GRADE
    filter: "LAB.DIST IS NOT NULL"
    order_by: [LAB.DIST]
    keep: first
    no_match: {literal: null}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: CAT, type: str, derivation: CODED.GRADE}
  - {name: NEAR, type: int, derivation: NEAREST.SEQ}
  - {name: TAGN, type: int, derivation: {value: {source: SRC.TAG}, unconvertible: -1}}
"#,
            );
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("out.csv")).unwrap(),
                "ID,CAT,NEAR,TAGN\n1,high,2,7\n2,low,,-1\n3,,,\n"
            );
            // The dataset's own paths come first, its `unconvertible` before
            // its derivations; then each intermediate in turn, its
            // `unconvertible` first. Every build of the donor records counts.
            // CODED is built once for CAT. NEAREST's DIST reads the current
            // record, so NEAREST is built for each of the three rows and
            // augments only the LAB records whose ID matches the row's
            // (LOOKUP-15): subject 1's two records, subject 2's one, and none
            // for subject 3. The first two builds read CODED for their
            // records, building CODED again: three builds of CODED. FLAGN
            // converts neither `H` nor `L` (LAB records 1 and 3), so it
            // answers twice per build; GRADE answers record 2, which has no
            // FLAG, once per build; and G2 chooses between subject 1's two
            // CODED records once for each of subject 1's two LAB records.
            assert_eq!(
                handler_rows(&study, &result),
                [
                    "out.csv,handler,columns.TAGN.derivation.unconvertible,unconvertible,,TAGN,VALUE-3,,,,1,,,{}",
                    "out.csv,handler,columns.CAT.derivation.source.multiple_matches,multiple_matches,,CAT,LOOKUP-10,,,,1,,,{}",
                    "out.csv,handler,columns.CAT.derivation.source.no_match,no_match,,CAT,LOOKUP-24,,,,1,,,{}",
                    "out.csv,handler,columns.NEAR.derivation.source.multiple_matches,multiple_matches,,NEAR,LOOKUP-10,,,,1,,,{}",
                    "out.csv,handler,columns.NEAR.derivation.source.no_match,no_match,,NEAR,LOOKUP-24,,,,2,,,{}",
                    "out.csv,handler,intermediates[0].derivations.FLAGN.unconvertible,unconvertible,,,VALUE-3,,,,6,,,{}",
                    "out.csv,handler,intermediates[0].derivations.GRADE.mapping.missing,missing,,,,,,,3,,,{}",
                    "out.csv,handler,intermediates[1].derivations.G2.source.multiple_matches,multiple_matches,,,LOOKUP-10,,,,2,,,{}",
                    "out.csv,handler,intermediates[1].derivations.G2.source.no_match,no_match,,,LOOKUP-24,,,,0,,,{}",
                ]
            );
        }

        #[test]
        fn a_read_that_feeds_another_lookup_or_a_predicate_counts_nothing() {
            let value = "  - {name: V, type: str, derivation: P.VAL}\n";
            let flag = "  - {name: F, type: str, derivation: {flag: {condition: \"P.VAL IS NOT NULL\", false_value: N, missing: N}}}\n";
            let v_row = "out.csv,handler,columns.V.derivation.source.no_match,no_match,,V,LOOKUP-24,,,,2,,,{}";
            let f_row = "out.csv,handler,columns.F.derivation.flag.missing,missing,,F,,,,,0,,,{}";
            // Either column may read P first: the flag before the value, or
            // the value before the flag.
            for (first, second, rows) in
                [(value, flag, [v_row, f_row]), (flag, value, [f_row, v_row])]
            {
                let study = Study::new();
                fs::write(study.0.join("input/src.csv"), "ID\n1\n2\n3\n").unwrap();
                fs::write(study.0.join("input/base.csv"), "ID,REF\n1,5\n3,20\n").unwrap();
                fs::write(
                    study.0.join("input/rng.csv"),
                    "ID,LO,HI,VAL\n1,1,10,a\n3,1,10,c\n",
                )
                .unwrap();
                let mut result = study.build(&format!(
                    "schema_version: '1.0'\n\
                     domain: TEST\n\
                     base: SRC\n\
                     keys: [ID]\n\
                     input:\n\
                     \x20 SRC: {{path: input/src.csv, types: {{ID: int}}}}\n\
                     \x20 BASE: {{path: input/base.csv, types: {{ID: int, REF: int}}}}\n\
                     \x20 RNG: {{path: input/rng.csv, types: {{ID: int, LO: int, HI: int}}}}\n\
                     output: {{path: out.csv, columns: [ID, V, F], report: report.csv}}\n\
                     intermediates:\n\
                     \x20 - id: Q\n\
                     \x20   dataset: BASE\n\
                     \x20   key: [ID]\n\
                     \x20   filter: \"BASE.REF IS NOT NULL\"\n\
                     \x20 - id: P\n\
                     \x20   dataset: RNG\n\
                     \x20   key: [ID]\n\
                     \x20   between: {{value: Q.REF, lower: LO, upper: HI}}\n\
                     columns:\n\
                     \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
                     {first}{second}"
                ));
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("out.csv")).unwrap(),
                    "ID,V,F\n1,a,Y\n2,,N\n3,,N\n"
                );
                // Q supplies P's `between` value, so its miss for subject 2
                // is answered uncounted, as in a predicate. P counts its own
                // answer for subject 2, whose range value is missing, and for
                // subject 3, whose 20 is out of range. The flag reads P in its
                // condition and counts nothing, at any path.
                assert_eq!(handler_rows(&study, &result), rows);
            }
        }

        #[test]
        fn a_grouped_row_count_names_every_offending_count_even_for_one_group() {
            let study = Study::new();
            fs::write(study.0.join("input/input.csv"), "ID,ARM\n1,A\n2,A\n3,B\n").unwrap();
            let mut result = study.build(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input/input.csv, types: {ID: int}}}
output: {path: out.csv, columns: [ID, ARM], report: report.csv}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: ARM, type: str, derivation: SRC.ARM}
verifications:
  - row_count: {group_by: [ARM], max: 1, severity: warning}
  - row_count: {max: 2, severity: warning}
  - row_count: {filter: "ARM = 'A'", max_fraction: 0.5, severity: warning}
"#,
            );
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            let report = saved_report(&study, "report.csv");
            // Arm A's two rows exceed the per-arm maximum while arm B holds:
            // one offending group, still reported as a sequence of counts.
            assert!(
                report.contains(
                    "out.csv,check,verifications[0].row_count,row_count,,,CHECK-13,warning,violated,row_count_failed,1,2,\"[{\"\"ARM\"\":\"\"A\"\"}]\",\"{\"\"counts\"\":[2]}\"\n"
                ),
                "{report}"
            );
            // An ungrouped check names its one count; only a fraction bound
            // adds the denominator.
            assert!(
                report.contains(
                    "out.csv,check,verifications[1].row_count,row_count,,,CHECK-13,warning,violated,row_count_failed,1,1,[{}],\"{\"\"count\"\":3}\"\n"
                ),
                "{report}"
            );
            assert!(
                report.contains(
                    "out.csv,check,verifications[2].row_count,row_count,,,CHECK-13,warning,violated,row_count_failed,1,1,[{}],\"{\"\"count\"\":2,\"\"denominator\"\":3}\"\n"
                ),
                "{report}"
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
    derivation: {impute: {type: date, source: SRC.DTC, day: first, minimum_source_precision: month, invalid: {literal: null}}}
  - name: STAMP
    type: datetime
    derivation: {impute: {type: datetime, source: SRC.DTM, minimum_source_precision: day, time: first, invalid: {literal: null}}}
  - name: SPAN
    type: float
    derivation: {duration_value: {source: SRC.DUR, unit: day, invalid: {literal: null}}}
  - name: PR
    type: str
    derivation: {precision: {type: datetime, source: SRC.DTM, invalid: {literal: null}}}
"#,
            );
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            // A missing source raises no condition, so its handler cites no
            // rule; text no reader accepts cites the rule its operation
            // raises without the handler: record 2's text for STAMP, SPAN
            // and PR, and record 3's text for START.
            assert_eq!(
                saved_report(&study, "report.csv"),
                format!(
                    "{HEADER}out.csv,run,output,,,,,,succeeded,,3,,,{{}}\n\
                     out.csv,input,input.SRC,SRC,,,,,,,3,,,\"{{\"\"bytes\"\":68,\"\"path\"\":\"\"input/input.csv\"\",\"\"profile\"\":\"\"csv\"\"}}\"\n\
                     out.csv,rows,base,SRC,,,,,,,3,3,,{{}}\n\
                     out.csv,handler,columns.START.derivation.impute.missing,missing,,START,,,,,1,,,{{}}\n\
                     out.csv,handler,columns.START.derivation.impute.invalid,invalid,,START,TIME-13,,,,1,,,{{}}\n\
                     out.csv,handler,columns.STAMP.derivation.impute.missing,missing,,STAMP,,,,,1,,,{{}}\n\
                     out.csv,handler,columns.STAMP.derivation.impute.invalid,invalid,,STAMP,TIME-13,,,,1,,,{{}}\n\
                     out.csv,handler,columns.SPAN.derivation.duration_value.invalid,invalid,,SPAN,TIME-17,,,,1,,,{{}}\n\
                     out.csv,handler,columns.PR.derivation.precision.missing,missing,,PR,,,,,1,,,{{}}\n\
                     out.csv,handler,columns.PR.derivation.precision.invalid,invalid,,PR,TIME-13,,,,1,,,{{}}\n\
                     out.csv,column,columns.ID,ID,,,,,,,0,3,,{{}}\n\
                     out.csv,column,columns.START,START,,,,,,,2,3,,{{}}\n\
                     out.csv,column,columns.STAMP,STAMP,,,,,,,2,3,,{{}}\n\
                     out.csv,column,columns.SPAN,SPAN,,,,,,,1,3,,{{}}\n\
                     out.csv,column,columns.PR,PR,,,,,,,2,3,,\"{{\"\"max_length\"\":6}}\"\n"
                )
            );
        }

        #[test]
        #[cfg(feature = "parquet")]
        fn parquet_reports_preserve_governed_logical_rows_and_integer_columns() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw.edit("adsl-checks.csv", "checks.parquet"));
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
            assert_eq!(artifact["row_count"], 8);
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
