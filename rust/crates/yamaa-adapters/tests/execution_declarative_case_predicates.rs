#![cfg(feature = "yaml")]
//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod declarative_case_predicates {
        //! Independently authored full case reports and unchanged original CSV truth.
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
                    "yamaa-case-predicates-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>
            {
                let path = self.0.join("spec.yaml");
                fs::write(&path, body).unwrap();
                let specification = path.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-case-predicates",
                        specification,
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
        fn findings(result: &file_application::Domain) -> Vec<yamaa_core::json::Value> {
            result
                .issues()
                .iter()
                .map(|issue| {
                    let mut value = issue.as_value();
                    assert!(value["context"].is_string());
                    value["context"] = yamaa_adapters::json_wire::decode(&issue.context).unwrap();
                    value
                })
                .collect()
        }
        const SOURCE: &str = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {V: int}}}
output: {path: result.csv, columns: [ID, VALUE]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - name: VALUE
    type: int
    derivation:
      case:
        - when: "SRC.V > 0"
          then: {literal: 7}
        - when: 'FALSE'
          then: SRC.BAD
        - otherwise: {literal: 8}
"#;
        #[test]
        fn implicit_case_predicates_reduce_feeding_values_and_only_true_selects() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("input.csv"),
                "ID,V,BAD\na,1,hidden\na,,hidden\nb,,hidden\nc,-1,hidden\n",
            )
            .unwrap();
            let mut result = fixture.run(SOURCE).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert_eq!(
                result.observations()["verifications"],
                yamaa_adapters::json!([])
            );
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,VALUE\na,7\nb,8\nc,8\n"
            );
        }
        #[test]
        fn case_predicate_multiplicity_retains_its_complete_report_and_blocks_publication() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("input.csv"),
                "ID,V,BAD\na,1,hidden\na,2,hidden\nb,,hidden\n",
            )
            .unwrap();
            fs::write(fixture.0.join("result.csv"), "sentinel").unwrap();
            let mut result = fixture.run(SOURCE).unwrap();
            let issues = findings(&result);
            assert_eq!(
                issues,
                vec![
                    yamaa_adapters::json!({"phase":"derivation","condition":"multiple_values_per_key","requirement":"REQ-0075","spec_paths":["columns.VALUE.derivation.case[0].when"],"context":{"identifier":"SRC.V","value_count":2,"keys":[{"ID":"a"}]}})
                ]
            );
            assert!(result.save().is_err());
            assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"sentinel");
        }
        #[test]
        fn filtered_case_source_is_shared_by_columns_and_record_rows() {
            let body = SOURCE
                .replace("        - when: 'FALSE'\n          then: SRC.BAD\n", "")
                .replace("when: \"SRC.V > 0\"", "when: 'TRUE'")
                .replace(
                    "then: {literal: 7}",
                    "then: {source: {variable: SRC.ID, filter: 'SRC.V > 0'}}",
                )
                .replace("    type: int", "    type: str");
            for (rows, expected) in [
                ("", b"ID,VALUE\na,a\nb,\n".as_slice()),
                (
                    "rows:\n  - id: records\n    dataset: SRC\n    derivations: {}\n",
                    b"ID,VALUE\na,a\nb,\n".as_slice(),
                ),
            ] {
                let fixture = Fixture::new();
                fs::write(
                    fixture.0.join("input.csv"),
                    "ID,V,BAD\na,1,hidden\nb,,hidden\n",
                )
                .unwrap();
                let mut result = fixture.run(&format!("{body}{rows}")).unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }

        #[test]
        fn qualified_case_predicate_and_full_pattern_capture_execute_in_each_expression_position() {
            let expression = "{case: [{when: 'SRC.V > 0', then: {str_extract: {source: SRC.TEXT, pattern: '(?<=prefix:)([A-Z]+)(?=$)', group: 1}}}, {otherwise: {literal: none}}]}";
            let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n";
            for (position, tail) in [
        ("column-case", format!("  - {{name: VALUE, type: str, derivation: {expression}}}\n")),
        ("record-default-case", format!("  - {{name: VALUE, type: str, derivation: {expression}}}\nrows: [{{id: records, dataset: SRC, derivations: {{}}}}]\n")),
        ("record-local-case", format!("  - {{name: VALUE, type: str}}\nrows: [{{id: records, dataset: SRC, derivations: {{VALUE: {expression}}}}}]\n")),
        ("grouped-local-case", format!("  - {{name: VALUE, type: str}}\nrows: [{{id: groups, dataset: SRC, group_by: [SRC.ID, SRC.V, SRC.TEXT], derivations: {{VALUE: {expression}}}}}]\n")),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("input.csv"), "ID,V,TEXT\na,1,prefix:ONE\nb,0,prefix:TWO\n").unwrap();
        let mut result = fixture.run(&format!("{prefix}{tail}")).unwrap();
        assert!(result.issues().is_empty(), "{position}: {:?}", result.issues());
        assert!(result.save().unwrap(), "{position}");
        assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID,VALUE\na,ONE\nb,none\n", "{position}");
    }
        }

        #[test]
        fn verification_predicate_and_literals_check_completed_column_case_and_row_outputs() {
            let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {V: int}}}\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - name: VALUE\n    type: int\n    verifications:\n      - allowed_values: {values: [0, 1.0]}\n      - range: {min: 0.0, max: 1.5}\n";
            let expression =
                "{case: [{when: 'SRC.V > 0', then: {literal: 1}}, {otherwise: {literal: 0}}]}";
            for tail in [
        format!("    derivation: {expression}\n"),
        format!("rows: [{{id: records, dataset: SRC, derivations: {{VALUE: {expression}}}}}]\n"),
        format!("rows: [{{id: groups, dataset: SRC, group_by: [SRC.ID, SRC.V], derivations: {{VALUE: {expression}}}}}]\n"),
    ] {
        for predicate in ["VALUE >= 0", "VALUE =", "7"] {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("input.csv"), "ID,V\na,1\nb,0\n").unwrap();
            let declaration = if predicate == "7" { "7".into() } else { format!("'{predicate}'") };
            let built = fixture.run(&format!("{prefix}{tail}verifications: [{{assert: {{require: {declaration}}}}}]\n"));
            if predicate == "7" {
                let Err(issues) = built else { panic!("non-text public predicate was admitted") };
                assert_eq!(issues[0].condition, "invalid_field_type");
                assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0287"));
                assert_eq!(issues[0].spec_paths, ["verifications[0].assert.require"]);
                assert!(!fixture.0.join("result.csv").exists());
                continue;
            }
            let mut result = built.unwrap();
            if predicate == "VALUE >= 0" {
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID,VALUE\na,1\nb,0\n");
            } else {
                let issue = &result.issues()[0];
                assert_eq!(issue.condition, "invalid_predicate");
                assert_eq!(issue.spec_paths, ["verifications[0].assert.require"]);
                assert!(result.output().is_none());
                assert!(result.save().is_err());
                assert!(!fixture.0.join("result.csv").exists());
            }
        }
    }
        }
        #[test]
        fn unchanged_original_case_and_filtered_source_truth_passes_in_full() {
            let benchmarks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            fn copy(source: &Path, target: &Path) {
                for entry in fs::read_dir(source).unwrap() {
                    let entry = entry.unwrap();
                    let path = target.join(entry.file_name());
                    if entry.file_type().unwrap().is_dir() {
                        fs::create_dir(&path).unwrap();
                        copy(&entry.path(), &path);
                    } else {
                        fs::copy(entry.path(), path).unwrap();
                    }
                }
            }
            let mut remaining = Vec::new();
            for name in [
                "adam-adae-protocol-review",
                "schema-expressions",
                "schema-predicates",
                "sdtm-ae-seriousness",
                "sdtm-mh-ongoing-conditions",
                "sdtm-tr-tumor-measurements",
                "sdtm-tu-tumor-identification",
                "sdtm-ds-reasons",
            ] {
                let fixture = Fixture::new();
                let source = benchmarks.join(name);
                copy(&source, &fixture.0);
                let authored = fs::read(source.join("spec.yaml")).unwrap();
                let mut result = match fixture.run(std::str::from_utf8(&authored).unwrap()) {
                    Ok(result) => result,
                    Err(issues) => {
                        remaining.push(format!("{name}: {issues:?}"));
                        continue;
                    }
                };
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap(), "{name}");
                for expected in fs::read_dir(source.join("expected")).unwrap() {
                    let expected = expected.unwrap();
                    if expected
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "csv")
                    {
                        assert_eq!(
                            fs::read(fixture.0.join(expected.file_name())).unwrap(),
                            fs::read(expected.path()).unwrap(),
                            "{name}/{}",
                            expected.file_name().to_string_lossy()
                        );
                    }
                }
                assert_eq!(fs::read(source.join("spec.yaml")).unwrap(), authored);
            }
            assert!(
                remaining.is_empty(),
                "remaining original blockers: {}",
                remaining.join("\n")
            );
        }

        #[test]
        fn independently_authored_installed_host_truth_has_the_same_complete_native_observations() {
            let truth: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(include_str!("fixtures/case_predicates.json"))
                    .unwrap();
            let dispositions: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/case_predicate_dispositions.json"),
            )
            .unwrap();
            for name in dispositions["cases"].as_object().unwrap().keys() {
                assert!(
                    truth["cases"].get(name).is_some(),
                    "unknown disposition {name}"
                );
            }
            for (name, case) in truth["cases"].as_object().unwrap() {
                if let Some(disposition) = dispositions["cases"].get(name) {
                    assert_eq!(
                        disposition["classification"],
                        "historical_private_reference_defect"
                    );
                    assert_eq!(case["csv"], disposition["historical_csv"]);
                    assert_eq!(disposition["requirement"], "REQ-0148");
                    continue;
                }
                let fixture = Fixture::new();
                for (name, contents) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(name), contents.as_str().unwrap()).unwrap();
                }
                let mut result = fixture
                    .run(case["specification"].as_str().unwrap())
                    .unwrap();
                let findings = findings(&result);
                assert_eq!(
                    yamaa_adapters::json!(findings),
                    case.get("findings")
                        .cloned()
                        .unwrap_or_else(|| yamaa_adapters::json!([])),
                    "{name}"
                );
                if let Some(csv) = case.get("csv") {
                    assert!(result.save().unwrap(), "{name}");
                    assert_eq!(
                        fs::read(fixture.0.join("result.csv")).unwrap(),
                        csv.as_str().unwrap().as_bytes(),
                        "{name}"
                    );
                } else {
                    assert!(result.output().is_none(), "{name}");
                    assert!(result.save().is_err(), "{name}");
                    assert!(!fixture.0.join("result.csv").exists(), "{name}");
                }
            }
        }

        fn lookup_fixture(name: &str) -> (Fixture, String) {
            let truth: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(include_str!("fixtures/case_predicates.json"))
                    .unwrap();
            let case = &truth["cases"][name];
            let fixture = Fixture::new();
            for (filename, contents) in case["inputs"].as_object().unwrap() {
                fs::write(fixture.0.join(filename), contents.as_str().unwrap()).unwrap();
            }
            (fixture, case["specification"].as_str().unwrap().into())
        }

        #[test]
        fn assertion_lookup_reuses_current_selection_without_adding_derivation_handlers() {
            let (fixture, body) = lookup_fixture("assert_lookup_selected");
            let result = fixture.run(&body).unwrap();
            assert!(result.issues().is_empty());
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.VALUE.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.VALUE.derivation.source.no_match","handler":"no_match","count":1}
                ])
            );
        }

        #[test]
        fn false_when_still_resolves_require_lookup_and_refuses_an_unhandled_absence() {
            let (fixture, body) = lookup_fixture("assert_lookup_only");
            for pattern in ["    no_match: null\n", "when: 'P.X IS NOT NULL'"] {
                assert!(body.contains(pattern), "fixture drifted: {pattern}");
            }
            let body = body
                .replace("    no_match: null\n", "")
                .replace("when: 'P.X IS NOT NULL'", "when: 'FALSE'");
            let mut result = fixture.run(&body).unwrap();
            assert!(
                !result.issues().is_empty(),
                "require must be evaluated on exempt rows"
            );
            let definition =
                yamaa_core::diagnostic::ConditionCode::DatasetUnmatchedKey.definition();
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].phase, definition.phase);
            assert_eq!(result.issues()[0].condition, definition.condition);
            assert_eq!(
                result.issues()[0].requirement.as_deref(),
                definition.requirement
            );
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            assert!(!fixture.0.join("result.csv").exists());
        }

        #[test]
        fn empty_output_validates_lookup_operand_types_without_selecting_a_donor() {
            let (fixture, body) = lookup_fixture("assert_lookup_empty");
            for pattern in [
                "when: 'P.X IS NOT NULL'",
                "require: 'VALUE = P.X AND P.X >= 10'",
            ] {
                assert!(body.contains(pattern), "fixture drifted: {pattern}");
            }
            let body = body
                .replace("when: 'P.X IS NOT NULL'", "when: 'FALSE'")
                .replace(
                    "require: 'VALUE = P.X AND P.X >= 10'",
                    "require: \"P.X LIKE 'text%'\"",
                );
            let mut result = fixture.run(&body).unwrap();
            assert!(
                !result.issues().is_empty(),
                "empty output still admits declaration types"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(result.output().is_none());
            assert!(result.save().is_err());
        }

        #[test]
        fn warning_assertion_missing_lookup_fields_retain_field_specific_diagnostics() {
            for (field, pattern) in [
                ("when", "when: 'P.X IS NOT NULL'"),
                ("require", "require: 'P.X >= 10'"),
            ] {
                let (fixture, body) = lookup_fixture("assert_lookup_only");
                for pattern in ["  - assert:\n", "output: {path: result.csv,", pattern] {
                    assert!(body.contains(pattern), "fixture drifted: {pattern}");
                }
                let body = body
                    .replace(
                        "output: {path: result.csv,",
                        "output: {path: result.csv, warning_log: warnings.csv,",
                    )
                    .replace("  - assert:\n", "  - assert:\n      severity: warning\n")
                    .replace(pattern, &pattern.replace("P.X", "P.MISSING"));
                let mut result = fixture.run(&body).unwrap();
                let definition =
                    yamaa_core::diagnostic::ConditionCode::VerificationUnknownReference
                        .definition();
                assert_eq!(result.issues().len(), 1);
                let issue = &result.issues()[0];
                assert_eq!(issue.phase, definition.phase);
                assert_eq!(issue.condition, definition.condition);
                assert_eq!(issue.requirement.as_deref(), definition.requirement);
                assert_eq!(
                    issue.spec_paths,
                    vec![format!("verifications[0].assert.{field}")]
                );
                assert_eq!(
                    yamaa_adapters::json_wire::decode(&issue.context).unwrap(),
                    yamaa_adapters::json!({"identifier": "P.MISSING"})
                );
                assert!(result.output().is_none());
                assert!(result.save().is_err());
                assert!(!fixture.0.join("result.csv").exists());
            }
        }
    }
    mod declarative_correlated_predicates {
        //! Same-input donor selection retains one captured relation across driver scopes.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-correlated-predicate-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, body).unwrap();
                let specification = specification.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-correlated-predicate",
                        specification,
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

        const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int, CUT: int}}
  D: {path: donor.csv, types: {ID: int, TIME: int}}
output: {path: result.csv, columns: [ID, SEQ, A, B]}
intermediates:
  - id: P
    dataset: D
    key: [ID]
    filter: "D.TIME < SRC.CUT"
    order_by: [D.TIME]
    keep: last
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
        fn populate(fixture: &Fixture) {
            fs::write(
                fixture.0.join("src.csv"),
                "ID,SEQ,CUT\n1,1,2\n1,2,4\n3,1,2\n1,3,\n",
            )
            .unwrap();
            fs::write(
                fixture.0.join("donor.csv"),
                "ID,TIME,A,B\n1,1,one,x\n1,3,three,\n2,9,hidden,hidden\n",
            )
            .unwrap();
            fs::write(fixture.0.join("alt.csv"), "ID,SEQ,CUT\n1,1,4\n3,1,2\n").unwrap();
        }
        fn finding(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
        }
        #[test]
        fn complete_root_outputs_and_handler_geometry_are_current_row_specific() {
            let fixture = Fixture::new();
            populate(&fixture);
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SEQ,A,B\n1,1,one,x\n1,2,three,\n3,1,,\n1,3,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":2},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":2}
                ])
            );
        }
        fn secondary_body(filter: &str) -> String {
            BODY.replace("  D: {path:","  ALT: {path: alt.csv, types: {ID: int, SEQ: int, CUT: int}}\n  D: {path:")
        .replace("D.TIME < SRC.CUT",filter)
        .replace("columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: A, type: str, derivation: P.A}\n  - {name: B, type: str, derivation: P.B}","rows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, SEQ: ALT.SEQ, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: SEQ, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}")
        }
        #[test]
        fn repeated_donor_and_current_identifiers_share_bindings_without_changing_selection() {
            let fixture = Fixture::new();
            populate(&fixture);
            let body = BODY.replace(
                "D.TIME < SRC.CUT",
                "D.TIME < SRC.CUT AND (D.TIME IS NULL OR D.TIME < SRC.CUT)",
            );
            let mut result = fixture.run(&body).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,SEQ,A,B\n1,1,one,x\n1,2,three,\n3,1,,\n1,3,,\n"
            );
        }
        #[test]
        fn secondary_driver_fields_bind_to_the_actual_template_and_original_native_port() {
            let fixture = Fixture::new();
            populate(&fixture);
            let mut result = fixture.run(&secondary_body("D.TIME < ALT.CUT")).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SEQ,A,B\n1,1,three,\n3,1,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":1}
                ])
            );
        }
        fn refusal(fixture: &Fixture, body: &str) -> Vec<yamaa_core::json::Value> {
            match fixture.run(body) {
                Err(issues) => issues.iter().map(finding).collect(),
                Ok(mut result) => {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    result.issues().iter().map(finding).collect()
                }
            }
        }
        #[test]
        fn known_unrelated_input_fields_are_refused_with_complete_selection_geometry() {
            let fixture = Fixture::new();
            populate(&fixture);
            let issues = refusal(&fixture, &secondary_body("D.TIME < SRC.CUT"));
            assert_eq!(
                issues,
                [
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0120","spec_paths":["intermediates[0].filter"],"context":{"identifier":"SRC.CUT","intermediate":"P"}})
                ]
            );
            assert!(!fixture.0.join("result.csv").exists());
        }
        #[test]
        fn donor_qualifier_takes_precedence_and_bare_stored_fields_suggest_it() {
            let fixture = Fixture::new();
            populate(&fixture);
            let mut result = fixture
                .run(&BODY.replace("D.TIME < SRC.CUT", "D.TIME < D.ID"))
                .unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SEQ,A,B\n1,1,,\n1,2,,\n3,1,,\n1,3,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":4},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":4}
                ])
            );
            // Only B is neutralised: A still derives P.A, so pruning
            // (REQ-0638) keeps P live and its filter is still refused. A
            // refused intermediate's fields no longer bind, so A's read
            // reports unknown_field as well — the same pair a composed
            // specification (parents: []) reports.
            let issues = refusal(
                &fixture,
                &BODY
                    .replace("D.TIME < SRC.CUT", "TIME < SRC.CUT")
                    .replace("derivation: P.B", "derivation: {literal: unused}"),
            );
            assert_eq!(
                issues,
                [
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0120","spec_paths":["intermediates[0].filter"],"context":{"identifier":"TIME","intermediate":"P","suggestion":"D.TIME"}}),
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.A.derivation.source"],"context":{"identifier":"P.A"}})
                ]
            );
        }
    }
    mod declarative_flags {
        //! Unchanged public originals and independently captured authored witnesses.
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
                    "yamaa-flag-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn original(name: &str) -> Self {
                let fixture = Self::new();
                let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                fn copy(src: &Path, dst: &Path) {
                    for e in fs::read_dir(src).unwrap() {
                        let e = e.unwrap();
                        let target = dst.join(e.file_name());
                        if e.file_type().unwrap().is_dir() {
                            fs::create_dir(&target).unwrap();
                            copy(&e.path(), &target);
                        } else {
                            fs::copy(e.path(), target).unwrap();
                        }
                    }
                }
                copy(&source, &fixture.0);
                fixture
            }
            fn run(
                &self,
                name: &str,
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
                        example: name,
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
        fn issues(issues: &[yamaa_adapters::issue_rows::Issue]) -> Vec<yamaa_core::json::Value> {
            issues.iter().map(|i|yamaa_adapters::json!({"phase":i.phase,"condition":i.condition,"requirement":i.requirement,"spec_paths":i.spec_paths,"context":yamaa_adapters::json_wire::decode(&i.context).unwrap()})).collect()
        }
        fn reference() -> yamaa_core::json::Value {
            yamaa_adapters::json_wire::decode(include_str!("fixtures/flag_reference.json")).unwrap()
        }
        #[test]
        fn six_unchanged_original_positive_documents_match_csv_and_full_handlers() {
            let truth = reference();
            for name in [
                "schema-str-contains",
                "adam-adae-occurrence-flags",
                "adam-adex-dose-reduction",
                "adam-adlb-closest-visit",
                "adam-adsl-completion-flag",
                "sdtm-dm-death",
            ] {
                let fixture = Fixture::original(name);
                assert_eq!(
                    fs::read_to_string(fixture.0.join("spec.yaml")).unwrap(),
                    truth["originals"][name]["specification"].as_str().unwrap()
                );
                let mut result = fixture
                    .run(name)
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap());
                let original = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name)
                    .join("expected");
                let expected = fs::read_dir(original)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| p.extension().is_some_and(|s| s == "csv"))
                    .unwrap();
                let output_path = if name.starts_with("schema-") {
                    "adae.csv"
                } else {
                    match name {
                        "adam-adae-occurrence-flags" => "adae.csv",
                        "adam-adex-dose-reduction" => "adex.csv",
                        "adam-adlb-closest-visit" => "adlb.csv",
                        "adam-adsl-completion-flag" => "adsl.csv",
                        _ => "dm.csv",
                    }
                };
                assert_eq!(
                    fs::read(fixture.0.join(output_path)).unwrap(),
                    fs::read(expected).unwrap(),
                    "{name}"
                );
                assert_eq!(
                    result.observations()["handler_counts"],
                    truth["originals"][name]["handlers"],
                    "{name}"
                );
            }
        }
        #[test]
        fn unchanged_negative_originals_retain_complete_semantic_findings() {
            let truth = reference();
            for name in [
                "negative-flag-condition",
                "negative-flag-missing-value",
                "negative-flag-payload",
                "negative-review-arithmetic",
                "negative-review-date-text",
                "negative-review-unknown-date",
            ] {
                let fixture = Fixture::original(name);
                let mut expected = truth["originals"][name]["findings"].clone();
                if name == "negative-flag-payload" {
                    for d in expected.as_array_mut().unwrap() {
                        let c = d["context"].as_object_mut().unwrap();
                        assert!(c.remove("source").is_some());
                        assert!(c.remove("entry").is_some());
                    }
                }
                let actual = match fixture.run(name) {
                    Err(issues_) => issues(&issues_),
                    Ok(mut result) => {
                        assert!(result.output().is_none());
                        assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                        issues(&result.issues())
                    }
                };
                assert_eq!(actual, expected.as_array().unwrap().as_slice(), "{name}");
            }
        }
        #[test]
        fn captured_scope_presence_precedence_and_lazy_named_witnesses_match_full_result() {
            let truth = reference();
            for (name, case) in truth["witnesses"].as_object().unwrap() {
                // Generic case-reference requirements and recursive map report projection
                // are existing separate debt, retained in the complete reference fixture.
                if ["nested_case_mixed_declarations", "implicit_raw_ambiguous"]
                    .contains(&name.as_str())
                {
                    continue;
                }
                let fixture = Fixture::new();
                fs::write(
                    fixture.0.join("spec.yaml"),
                    case["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (path, bytes) in case["inputs"].as_object().into_iter().flatten() {
                    fs::write(fixture.0.join(path), bytes.as_str().unwrap()).unwrap();
                }
                let mut expected = case["findings"].clone();
                if case["result_type"] == "SpecificationError" {
                    for d in expected.as_array_mut().unwrap() {
                        let c = d["context"].as_object_mut().unwrap();
                        assert!(c.remove("source").is_some());
                        assert!(c.remove("entry").is_some());
                    }
                }
                match fixture.run(name) {
                    Err(actual) => assert_eq!(
                        issues(&actual),
                        expected.as_array().unwrap().as_slice(),
                        "{name}"
                    ),
                    Ok(mut result) => {
                        assert_eq!(
                            issues(&result.issues()),
                            expected.as_array().unwrap().as_slice(),
                            "{name}"
                        );
                        assert_eq!(
                            result.observations()["handler_counts"],
                            case["handlers"],
                            "{name}"
                        );
                        if case["result_type"] == "ExecutionSuccess" {
                            assert!(result.save().unwrap());
                            assert_eq!(
                                fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                                case["csv"].as_str().unwrap(),
                                "{name}"
                            );
                        } else {
                            assert!(result.output().is_none());
                            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                            assert!(!fixture.0.join("out.csv").exists());
                        }
                    }
                }
            }
        }
    }
    mod declarative_intermediate_admission {
        //! Complete independent truth for applicable keys and declaration error precedence.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-intermediate-admission-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, body).unwrap();
                let specification = specification.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-intermediate-admission",
                        specification,
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
        fn issues(
            result: Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>,
        ) -> Vec<yamaa_core::json::Value> {
            let records = match result {
                Ok(result) => result.issues().to_vec(),
                Err(records) => records,
            };
            records.iter().map(|issue| yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})).collect()
        }
        const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, PARAM]
input:
  SRC: {path: src.csv, types: {ID: int}}
  DONOR: {path: donor.csv, types: {ID: float, SEQ: int}}
output: {path: result.csv, columns: [ID, PARAM, A, B]}
intermediates:
  - id: P
    dataset: DONOR
    order_by: [DONOR.SEQ]
    keep: first
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: PARAM, type: str, derivation: {literal: only}}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
        #[test]
        fn omitted_key_uses_only_applicable_fields_and_keeps_one_record_for_every_read() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID\n2\n1\n3\n").unwrap();
            fs::write(
                fixture.0.join("donor.csv"),
                "SEQ,ID,A,B\n1,1,first,\n1,1,tied,second\n0,2,two,both\n",
            )
            .unwrap();
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,PARAM,A,B\n2,only,two,both\n1,only,first,\n3,only,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1},
                ])
            );
        }
        #[test]
        fn rename_only_precedes_key_types_and_every_study_read() {
            let fixture = Fixture::new();
            let body = BODY.replace("    order_by: [DONOR.SEQ]\n    keep: first\n", "");
            assert_eq!(
                issues(fixture.run(&body)),
                vec![
                    yamaa_adapters::json!({"phase":"validation", "condition":"rename_only_intermediate", "requirement":"REQ-1248", "spec_paths":["intermediates[0]"], "context":{"intermediate":"P","dataset":"DONOR"}})
                ]
            );
            assert!(!fixture.0.join("src.csv").exists());
        }
        #[test]
        fn unused_rename_only_handler_survives_explicit_composition_and_empty_child() {
            let fixture = Fixture::new();
            let body = BODY
                .replace("    order_by: [DONOR.SEQ]\n    keep: first\n", "")
                .replace("derivation: P.A", "derivation: {literal: a}")
                .replace("derivation: P.B", "derivation: {literal: b}");
            fs::write(fixture.0.join("parent.yaml"), &body).unwrap();
            let explicit = body.replace("domain: TEST", "parents: []\ndomain: TEST");
            for authored in [
                body,
                explicit,
                "schema_version: '1.0'\nparents: parent.yaml\n".into(),
            ] {
                assert_eq!(
                    issues(fixture.run(&authored)),
                    vec![
                        yamaa_adapters::json!({"phase":"validation", "condition":"rename_only_intermediate", "requirement":"REQ-1248", "spec_paths":["intermediates[0]"], "context":{"intermediate":"P","dataset":"DONOR"}})
                    ]
                );
                assert!(!fixture.0.join("src.csv").exists());
                assert!(!fixture.0.join("donor.csv").exists());
            }
        }
        #[test]
        fn either_unpaired_selection_is_a_complete_declaration_finding() {
            let fixture = Fixture::new();
            for (line, declared, missing) in [
                ("    keep: first\n", "order_by", "keep"),
                ("    order_by: [DONOR.SEQ]\n", "keep", "order_by"),
            ] {
                assert_eq!(
                    issues(fixture.run(&BODY.replace(line, ""))),
                    vec![
                        yamaa_adapters::json!({"phase":"validation", "condition":"unpaired_fields", "requirement":"REQ-0119", "spec_paths":["intermediates[0]"], "context":{"intermediate":"P", "declared":[declared], "missing":[missing]}})
                    ]
                );
            }
        }
        #[test]
        fn absence_of_shared_fields_and_first_inferred_type_mismatch_are_distinct() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "ID\n1\n").unwrap();
            fs::write(fixture.0.join("donor.csv"), "OTHER,SEQ,A,B\nx,0,a,b\n").unwrap();
            // Only B is neutralised: A still derives P.A, so pruning
            // (REQ-0638) keeps P live and key admission still runs for it.
            // A refused intermediate's fields no longer bind, so A's read
            // reports unknown_field as well — the same pair a composed
            // specification (parents: []) reports.
            let unused = BODY
                .replace("derivation: P.B", "derivation: {literal: b}")
                .replace("types: {ID: float, SEQ: int}", "types: {SEQ: int}");
            assert_eq!(
                issues(fixture.run(&unused)),
                vec![
                    yamaa_adapters::json!({"phase":"validation", "condition":"no_applicable_keys", "requirement":"REQ-0153", "spec_paths":["intermediates[0]"], "context":{"dataset":"DONOR", "keys":["ID","PARAM"], "hint":"declare the `key` explicitly"}}),
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.A.derivation.source"],"context":{"identifier":"P.A"}})
                ]
            );
            fs::write(fixture.0.join("donor.csv"), "ID,PARAM,SEQ,A,B\nx,0,0,a,b\n").unwrap();
            let mismatched = unused.replace("types: {SEQ: int}", "types: {SEQ: int, PARAM: int}");
            assert_eq!(
                issues(fixture.run(&mismatched)),
                vec![
                    yamaa_adapters::json!({"phase":"validation", "condition":"incompatible_input_type", "requirement":"REQ-0151", "spec_paths":["intermediates[0]"], "context":{"source":"ID", "expected":"int", "actual":"str"}}),
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0103","spec_paths":["columns.A.derivation.source"],"context":{"identifier":"P.A"}})
                ]
            );
        }
    }
    mod declarative_intermediate_key_shorthand {
        //! REQ-0115 scalar key shorthand retains exact integers and shared donor choice.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-scalar-intermediate-key-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(&self) -> file_application::Domain {
                let path = self.0.join("spec.yaml");
                fs::write(
                    &path,
                    r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  D: {path: donor.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, A, B]}
intermediates:
  - id: P
    dataset: D
    key: ID
    order_by: [D.SEQ]
    keep: first
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: A, type: str, derivation: P.V}
  - {name: B, type: str, derivation: P.V}
"#,
                )
                .unwrap();
                let path = path.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-key-shorthand",
                        specification: path,
                        base_directory: ".",
                    },
                )
                .unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn scalar_shorthand_preserves_distinct_large_integer_keys_and_per_read_handlers() {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("src.csv"),
                "ID\n9007199254740992\n9007199254740993\n-9223372036854775808\n",
            )
            .unwrap();
            fs::write(
        fixture.0.join("donor.csv"),
        "ID,SEQ,V\n9007199254740992,1,lower\n9007199254740993,2,later\n9007199254740993,1,exact\n",
    )
    .unwrap();
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(),b"ID,A,B\n9007199254740992,lower,lower\n9007199254740993,exact,exact\n-9223372036854775808,,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
                ])
            );
        }
    }
    mod declarative_intermediate_keys {
        //! Unchanged public YAML and independent plain-reference outputs/findings.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
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
            fn run(&self, name: &str, case: &yamaa_core::json::Value) -> file_application::Domain {
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
        fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
        }
        #[test]
        fn independent_original_language_captures_match_all_output_bytes_and_complete_findings() {
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
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
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
            let mut case = cases["dependency_before_bad_filter"].clone();
            // Eliminate the independently captured key-cardinality failure, leaving the
            // same authored bad filter and a matched donor record for it to evaluate.
            case["inputs"]["src.csv"] = yamaa_adapters::json!("ID,N\na,7\n");
            let fixture = Fixture::new();
            let mut result = fixture.run("resolved-source-key-bad-filter", &case);
            assert_eq!(
                fixture.check().iter().map(issue_json).collect::<Vec<_>>(),
                result.issues().iter().map(issue_json).collect::<Vec<_>>()
            );
            assert_eq!(
                result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                yamaa_adapters::json!([{
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
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
            let mut case = cases["dependency_before_bad_filter"].clone();
            case["specification"] =
                yamaa_adapters::json!(case["specification"].as_str().unwrap().replace(
                    "key: {K: {str_pad: {source: SRC.N, width: 8}}}",
                    "key: {KINT: SRC.N}"
                ));
            let fixture = Fixture::new();
            let mut result = fixture.run("ordinary-source-key-bad-filter", &case);
            assert_eq!(
                fixture.check().iter().map(issue_json).collect::<Vec<_>>(),
                result.issues().iter().map(issue_json).collect::<Vec<_>>()
            );
            assert_eq!(
        result.issues().iter().map(issue_json).collect::<Vec<_>>(),
        yamaa_adapters::json!([{
            "phase": "validation", "condition": "incompatible_input_type", "requirement": "REQ-0190",
            "spec_paths": ["intermediates[0].filter"], "context": {"left_type": "int", "right_type": "str"},
        }]).as_array().unwrap().as_slice()
    );
            assert!(result.output().is_none());
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn deferred_filter_types_refuse_empty_missing_and_unread_selectors() {
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
            for variant in [
                "empty_donor",
                "empty_driver",
                "missing_filter_operand",
                "unused",
                "unread_branch",
                "boolean_missing",
            ] {
                let mut case = cases["dependency_before_bad_filter"].clone();
                case["inputs"]["src.csv"] = yamaa_adapters::json!("ID,N\na,7\n");
                match variant {
            "empty_donor" => case["inputs"]["tab.csv"] = yamaa_adapters::json!("K,KINT,KNUM,VAL\n"),
            "empty_driver" => case["inputs"]["src.csv"] = yamaa_adapters::json!("ID,N\n"),
            "missing_filter_operand" => {
                case["inputs"]["tab.csv"] =
                    yamaa_adapters::json!("K,KINT,KNUM,VAL\n\"       7\",,7,A\n")
            }
            "unused" => {
                case["specification"] = yamaa_adapters::json!(case["specification"]
                    .as_str()
                    .unwrap()
                    .replace("derivation: LOOK.VAL", "derivation: {literal: ok}"))
            }
            "unread_branch" => {
                case["specification"] = yamaa_adapters::json!(case["specification"]
                    .as_str()
                    .unwrap()
                    .replace("derivation: LOOK.VAL", "derivation: {case: [{when: 'FALSE', then: {source: LOOK.VAL}}, {otherwise: {literal: ok}}]}"))
            }
            "boolean_missing" => {
                case["inputs"]["tab.csv"] = yamaa_adapters::json!("K,KINT,KNUM,VAL\n\"       7\",,7,A\n");
                case["specification"] = yamaa_adapters::json!(case["specification"].as_str().unwrap().replace("TAB.KINT > 'bad'", "FALSE AND TAB.KINT > 'bad'"));
            }
            _ => unreachable!(),
        }
                let fixture = Fixture::new();
                let mut result = fixture.run(variant, &case);
                if variant == "unused" {
                    // Option B (REQ-0638, REQ-0641): no column reads LOOK, so
                    // pruning discards the intermediate together with its
                    // ill-typed filter; the specification builds instead of
                    // refusing the dead declaration.
                    assert!(result.issues().is_empty(), "{variant}");
                    assert!(result.output().is_some(), "{variant}");
                    assert!(result.save().unwrap(), "{variant}");
                    assert!(fixture.0.join("out.csv").exists(), "{variant}");
                    for path in case["inputs"].as_object().unwrap().keys() {
                        fs::remove_file(fixture.0.join(path)).unwrap();
                    }
                    assert!(fixture.check().is_empty(), "check without data: {variant}");
                    continue;
                }
                let expected = yamaa_adapters::json!([{"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0190","spec_paths":["intermediates[0].filter"],"context":{"left_type":"int","right_type":"str"}}]);
                assert_eq!(
                    result.issues().iter().map(issue_json).collect::<Vec<_>>(),
                    expected.as_array().unwrap().as_slice(),
                    "{variant}"
                );
                assert_eq!(result.observations()["diagnostics"], expected, "{variant}");
                assert_eq!(
                    result.observations()["nodes"][0]["diagnostics"],
                    expected,
                    "{variant}"
                );
                assert!(result.output().is_none(), "{variant}");
                for _ in 0..2 {
                    assert!(
                        matches!(result.save(), Err(file_application::FailedBuild)),
                        "{variant}"
                    );
                }
                assert!(!fixture.0.join("out.csv").exists(), "{variant}");
                for path in case["inputs"].as_object().unwrap().keys() {
                    fs::remove_file(fixture.0.join(path)).unwrap();
                }
                assert_eq!(
                    fixture.check().iter().map(issue_json).collect::<Vec<_>>(),
                    expected.as_array().unwrap().as_slice(),
                    "check without data: {variant}"
                );
            }
        }

        #[test]
        fn expression_keys_without_type_refusals_keep_successful_selection_and_saves() {
            let cases: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/intermediate_key_reference.json"
            ))
            .unwrap();
            for filter in ["filter: \"TAB.KINT > 0\"", ""] {
                let mut case = cases["dependency_before_bad_filter"].clone();
                case["inputs"]["src.csv"] = yamaa_adapters::json!("ID,N\na,7\n");
                case["specification"] = yamaa_adapters::json!(case["specification"]
                    .as_str()
                    .unwrap()
                    .replace("filter: \"TAB.KINT > 'bad'\"", filter));
                let fixture = Fixture::new();
                let mut result = fixture.run("valid-expression-key-filter", &case);
                assert!(fixture.check().is_empty());
                assert!(result.issues().is_empty());
                for _ in 0..2 {
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(fixture.0.join("out.csv")).unwrap(),
                        b"ID,V,V2\na,A,A\n"
                    );
                }
            }
        }
    }
    mod declarative_intermediate_ranges {
        //! Complete independent reports for inclusive ranges and visible donor columns.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-intermediate-ranges-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(&self, body: &str) -> file_application::Domain {
                let path = self.0.join("spec.yaml");
                fs::write(&path, body).unwrap();
                let path = path.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-intermediate-range",
                        specification: path,
                        base_directory: ".",
                    },
                )
                .unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int, X: int}}
  D: {path: donor.csv, types: {ID: int, LOW: int, HIGH: int, ORD: int}}
output: {path: result.csv, columns: [ID, SEQ, X, A, B]}
intermediates:
  - id: P
    dataset: D
    key: ID
    filter: "D.ELIG = 'Y'"
    between: {value: X, lower: LOW, upper: HIGH}
    columns: [V, W, V]
    order_by: [D.ORD]
    keep: first
    no_match: absent
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: X, type: int, derivation: SRC.X}
  - {name: A, type: str, derivation: P.V}
  - {name: B, type: str, derivation: P.W}
"#;
        fn populate(f: &Fixture) {
            fs::write(
                f.0.join("src.csv"),
                "ID,SEQ,X\n1,1,3\n1,2,4\n1,3,\n1,4,6\n2,1,3\n1,5,2\n",
            )
            .unwrap();
            fs::write(f.0.join("donor.csv"), "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,3,2,Y,edge,\n1,3,5,1,Y,later,paired\n1,,10,3,Y,missing-bound,hidden\n1,1,10,0,N,filtered,hidden\n").unwrap();
        }

        #[test]
        fn derived_date_bound_keeps_the_last_record_on_the_current_date() {
            let f = Fixture::new();
            fs::write(
                f.0.join("src.csv"),
                "ID,TRTSDT\nS01,2030-01-10\nS02,2030-02-01\nS03,\n",
            )
            .unwrap();
            fs::write(
        f.0.join("donor.csv"),
        "ID,VSDTC,WEIGHT\nS01,2030-01-05T08:00,70\nS01,2030-01-10T07:30,71\nS01,2030-01-15T09:00,72\nS02,2030-02-02T08:00,80\nS03,2030-01-03,60\n",
    )
    .unwrap();
            let body = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID]
input:
  SRC: {path: src.csv, types: {TRTSDT: date}}
  VS: {path: donor.csv, types: {WEIGHT: float}}
output: {path: result.csv, columns: [ID, TRTSDT, WEIGHTBL]}
intermediates:
  - id: BL
    dataset: VS
    derivations:
      ADT: {to_date: {source: VSDTC}}
    between: {value: TRTSDT, lower: ADT}
    order_by: [VS.ADT]
    keep: last
    no_match: null
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: TRTSDT, type: date, derivation: SRC.TRTSDT}
  - {name: WEIGHTBL, type: float, derivation: BL.WEIGHT}
"#;
            let mut result = f.run(body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(!f.0.join("result.csv").exists());
            assert!(result.save().unwrap());
            // S01's same-day timestamp qualifies after the bound is derived as a date.
            // S02 has only a later donor; S03's missing current value selects no donor.
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,TRTSDT,WEIGHTBL\nS01,2030-01-10,71\nS02,2030-02-01,\nS03,,\n"
            );
        }

        #[test]
        fn inclusive_ranges_keep_one_record_choice_and_project_only_named_reads() {
            let f = Fixture::new();
            populate(&f);
            let mut result = f.run(BODY);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":3},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":3}
                ])
            );
        }
        #[test]
        fn unstated_lower_bound_never_turns_a_missing_unused_donor_field_into_absence() {
            let f = Fixture::new();
            populate(&f);
            let mut result =
                f.run(&BODY.replace("value: X, lower: LOW, upper: HIGH", "value: X, upper: HIGH"));
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,missing-bound,hidden\n2,1,3,absent,absent\n1,5,2,later,paired\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":2},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":3},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":2}
                ])
            );
        }
        #[test]
        fn a_hidden_read_has_the_complete_original_reference_finding_and_no_output() {
            let f = Fixture::new();
            populate(&f);
            let mut result = f.run(&BODY.replace("derivation: P.W", "derivation: P.LOW"));
            assert_eq!(
                result
                    .issues()
                    .iter()
                    .map(|issue| {
                        let mut value = issue.as_value();
                        value["context"] =
                            yamaa_adapters::json_wire::decode(&issue.context).unwrap();
                        value
                    })
                    .collect::<Vec<_>>(),
                [
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-0125","spec_paths":["columns.B.derivation.source"],"context":{"identifier":"P.LOW"}})
                ]
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!f.0.join("result.csv").exists());
        }
        #[test]
        fn integer_absence_replacement_retains_full_signed_integer_value_and_ledger() {
            let f = Fixture::new();
            fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
            )
            .unwrap();
            let body = BODY
                .replace("no_match: absent", "no_match: -9223372036854775808")
                .replace("name: A, type: str", "name: A, type: int")
                .replace("name: B, type: str", "name: B, type: int");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,SEQ,X,A,B\n2,1,3,-9223372036854775808,-9223372036854775808\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
                ])
            );
        }

        #[test]
        fn range_current_outputs_and_scalar_keys_promote_their_defaults_before_row_reads() {
            let f = Fixture::new();
            populate(&f);
            let body = BODY
        .replace("\ncolumns:\n", "\nrows:\n  - id: selected\n    dataset: SRC\n    derivations: {A: P.V, B: P.W}\ncolumns:\n")
        .replace("name: A, type: str, derivation: P.V", "name: A, type: str")
        .replace("name: B, type: str, derivation: P.W", "name: B, type: str");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(f.0.join("result.csv")).unwrap(), b"ID,SEQ,X,A,B\n1,1,3,later,paired\n1,2,4,later,paired\n1,3,,absent,absent\n1,4,6,absent,absent\n2,1,3,absent,absent\n1,5,2,edge,\n");
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":3},
                    {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":3}
                ])
            );
        }

        #[test]
        fn float_absence_replacement_is_a_value_with_complete_per_read_observations() {
            let f = Fixture::new();
            fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
            )
            .unwrap();
            let body = BODY
                .replace("no_match: absent", "no_match: 0.5")
                .replace("name: A, type: str", "name: A, type: float")
                .replace("name: B, type: str", "name: B, type: float");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,SEQ,X,A,B\n2,1,3,0.5,0.5\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
                ])
            );
        }

        #[test]
        fn boolean_absence_replacement_reaches_a_null_test_without_conversion() {
            let f = Fixture::new();
            fs::write(f.0.join("src.csv"), "ID,SEQ,X\n2,1,3\n").unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,1,4,1,Y,3,4\n",
            )
            .unwrap();
            let body = BODY.replace("no_match: absent", "no_match: true")
        .replace("derivation: P.V", "derivation: {flag: {condition: 'P.V IS NOT NULL', true_value: Y, false_value: N, missing: null}}")
        .replace("derivation: P.W", "derivation: {flag: {condition: 'P.W IS NOT NULL', true_value: Y, false_value: N, missing: null}}");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(f.0.join("result.csv")).unwrap(),
                b"ID,SEQ,X,A,B\n2,1,3,Y,Y\n"
            );
            // Predicate normalization retains the literal value and discards recovery sites.
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
        }

        #[test]
        fn integer_current_values_do_not_round_into_a_float_donor_endpoint() {
            let f = Fixture::new();
            fs::write(
                f.0.join("src.csv"),
                "ID,SEQ,X\n1,1,9007199254740993\n1,2,9007199254740992\n",
            )
            .unwrap();
            fs::write(
                f.0.join("donor.csv"),
                "ID,LOW,HIGH,ORD,ELIG,V,W\n1,9007199254740992,9007199254740992,1,Y,edge,paired\n",
            )
            .unwrap();
            let body = BODY.replace("LOW: int, HIGH: int", "LOW: float, HIGH: float");
            let mut result = f.run(&body);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
        fs::read(f.0.join("result.csv")).unwrap(),
        b"ID,SEQ,X,A,B\n1,1,9007199254740993,absent,absent\n1,2,9007199254740992,edge,paired\n"
    );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1}
                ])
            );
        }

        #[test]
        fn unchanged_bad_range_document_preserves_its_single_complete_historical_finding() {
            let f = Fixture::new();
            let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/negative-lookup-bad-range");
            fs::create_dir(f.0.join("input")).unwrap();
            for name in ["vs.csv", "epoch.csv"] {
                fs::copy(
                    original.join("input").join(name),
                    f.0.join("input").join(name),
                )
                .unwrap();
            }
            let mut result = f.run(&fs::read_to_string(original.join("spec.yaml")).unwrap());
            let findings = result
                .issues()
                .iter()
                .map(|issue| {
                    let mut value = issue.as_value();
                    value["context"] = yamaa_adapters::json_wire::decode(&issue.context).unwrap();
                    value
                })
                .collect::<Vec<_>>();
            assert_eq!(
                findings,
                [
                    yamaa_adapters::json!({"phase":"validation","condition":"incomparable_range_types","requirement":"REQ-0121","spec_paths":["intermediates[0].between"],"context":{"intermediate":"EPOCHDEF","value_type":"date","lower_type":"int","upper_type":"int"}})
                ]
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([])
            );
            assert!(matches!(result.save(), Err(file_application::FailedBuild)));
            assert!(!f.0.join("vs.csv").exists());
        }
    }
    mod declarative_output_ordering {
        //! Authored public-file observations and immutable original benchmark artifacts.
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new(name: &str) -> Self {
                let root = std::env::temp_dir().join(format!(
                    "yamaa-output-order-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                fn copy(from: &Path, to: &Path) {
                    for entry in fs::read_dir(from).unwrap() {
                        let entry = entry.unwrap();
                        let target = to.join(entry.file_name());
                        if entry.file_type().unwrap().is_dir() {
                            fs::create_dir(&target).unwrap();
                            copy(&entry.path(), &target)
                        } else {
                            fs::copy(entry.path(), target).unwrap();
                        }
                    }
                }
                copy(
                    &Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../../benchmarks")
                        .join(name),
                    &root,
                );
                Self(root)
            }
            fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
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
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn authored(spec: &str, input: &[u8]) -> Fixture {
            let f = Fixture::new("schema-ordering");
            fs::write(f.0.join("spec.yaml"), spec).unwrap();
            fs::write(f.0.join("source.csv"), input).unwrap();
            f
        }
        #[test]
        fn original_ordering_artifact_matches_immutable_csv_truth() {
            let fixture = Fixture::new("schema-ordering");
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adae.csv")).unwrap(),
                fs::read(fixture.0.join("expected/adae.csv")).unwrap()
            );
        }
        #[test]
        fn internal_working_columns_order_the_projection_and_omission_keeps_construction_order() {
            let spec = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, types: {SORT: int}}}
output: {path: result.csv, columns: [ID], order_by: [SORT]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: SORT, type: int, derivation: SRC.SORT}
"#;
            for (source, expected) in [
                (spec.to_owned(), b"ID\nb\nc\na\n".as_slice()),
                (
                    spec.replace(", order_by: [SORT]", ""),
                    b"ID\na\nb\nc\n".as_slice(),
                ),
            ] {
                let fixture = authored(&source, b"ID,SORT\na,2\nb,1\nc,1\n");
                let mut result = fixture.run().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }
        #[test]
        fn ordering_and_unknown_profile_negatives_keep_registered_first_findings() {
            for (name, condition, path, requirement, context) in [
                (
                    "negative-order-repeated",
                    "duplicate_order_term",
                    "output.order_by[2]",
                    "REQ-0224",
                    yamaa_adapters::json!({"column":"USUBJID"}),
                ),
                (
                    "negative-order-unknown-column",
                    "undeclared_column",
                    "output.order_by[1]",
                    "REQ-0224",
                    yamaa_adapters::json!({"column":"VISITNUM"}),
                ),
                (
                    "negative-source-unknown-format",
                    "source_profile_unknown",
                    "input.DM.path",
                    "REQ-0852",
                    yamaa_adapters::json!({"dataset":"DM", "path":"input/dm.txt"}),
                ),
            ] {
                let fixture = Fixture::new(name);
                let issues = match fixture.run() {
                    Ok(result) => {
                        assert!(result.output().is_none());
                        result.issues().to_vec()
                    }
                    Err(issues) => issues,
                };
                assert_eq!(issues.len(), 1, "{name}: {issues:?}");
                let issue = &issues[0];
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    ("validation", condition, Some(requirement)),
                    "{name}"
                );
                assert_eq!(issue.spec_paths, [path], "{name}");
                assert_eq!(
                    yamaa_adapters::json_wire::decode(&issue.context).unwrap(),
                    context,
                    "{name}"
                );
            }
        }
        #[test]
        fn csv_present_empty_is_rejected_before_study_bytes_are_requested() {
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: absent.csv, empty_string: present}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"unused\n",
            );
            let issues = fixture.run().err().expect("static declaration");
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (
                    issues[0].phase.as_str(),
                    issues[0].condition.as_str(),
                    issues[0].requirement.as_deref()
                ),
                (
                    "validation",
                    "empty_string_present_unsupported",
                    Some("REQ-1161")
                )
            );
            assert_eq!(issues[0].spec_paths, ["input.SRC.empty_string"]);
            assert_eq!(
                yamaa_adapters::json_wire::decode(&issues[0].context).unwrap(),
                yamaa_adapters::json!({"dataset":"SRC"})
            );
            assert!(!fixture.0.join("result.csv").exists());
        }

        /// Root predicates gate raw base records before key conversion and keep first key appearance.
        #[test]
        fn root_filter_reads_raw_driver_values_before_key_derivation() {
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, V]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: V, type: str, derivation: SRC.V}
"#,
                b"ID,V,KEEP\nbad,excluded,N\n02,seven,Y\n2,conflict,\n1,eight,Y\n2,seven,Y\n",
            );
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,V\n2,seven\n1,eight\n"
            );
            // A raw string comparison would fail if the filter read the completed integer key.
            let raw = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "SRC.ID = '02'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: int, derivation: SRC.ID}]
"#,
                b"ID\n02\n3\n",
            );
            let mut result = raw.run().unwrap();
            assert!(result.issues().is_empty());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(raw.0.join("result.csv")).unwrap(), b"ID\n2\n");
        }
        #[test]
        fn root_filter_uses_explicit_base_independently_of_input_declaration_order() {
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {OTHER: other.csv, SRC: source.csv}
base: SRC
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"ID,KEEP\na,N\nb,Y\n",
            );
            fs::write(fixture.0.join("other.csv"), b"ID,KEEP\nwrong,Y\n").unwrap();
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), b"ID\nb\n");
        }
        #[test]
        fn root_filter_forbids_constructed_columns_and_conflicting_row_templates() {
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: source.csv}
filter: "ID = 'a'"
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: SRC.ID}]
"#,
                b"ID\na\n",
            );
            let result = fixture.run().unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            let issue = &result.issues()[0];
            assert_eq!(
                (
                    issue.phase.as_str(),
                    issue.condition.as_str(),
                    issue.requirement.as_deref()
                ),
                ("validation", "phase_boundary", Some("REQ-1170"))
            );
            assert_eq!(issue.spec_paths, ["filter"]);
            assert_eq!(
                yamaa_adapters::json_wire::decode(&issue.context).unwrap(),
                yamaa_adapters::json!({"identifier":"ID", "available_phase":"column_derivation", "required_phase":"row_filter"})
            );
            let mut spec = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            spec.push_str("rows: [{id: records, dataset: SRC, derivations: {}}]\n");
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::remove_file(fixture.0.join("source.csv")).unwrap();
            let issues = fixture
                .run()
                .err()
                .expect("static conflicting row construction");
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (
                    issues[0].condition.as_str(),
                    issues[0].requirement.as_deref()
                ),
                ("conflicting_row_construction", Some("REQ-1171"))
            );
        }

        #[test]
        fn source_ordinals_precede_filtering_and_count_multiline_records_once() {
            let spec = r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
filter: "SRC.KEEP = 'Y'"
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#;
            for (input, expected) in [
                (
                    b"ID,KEEP,NOTE\na,N,first\nb,Y,\"two\nlines\"\nc,Y,last\n".as_slice(),
                    b"ID,POSITION\nb,2\nc,3\n".as_slice(),
                ),
                (b"ID,KEEP,NOTE\n", b"ID,POSITION\n"),
            ] {
                let fixture = authored(spec, input);
                let mut result = fixture.run().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }

        #[test]
        fn ordinal_collision_and_fixed_odm_schema_have_complete_public_diagnostics() {
            let fixture = Fixture::new("negative-source-ordinal-collision");
            let result = fixture.run().unwrap();
            assert!(result.output().is_none());
            let issues = result.issues();
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (
                    issues[0].phase.as_str(),
                    issues[0].condition.as_str(),
                    issues[0].requirement.as_deref()
                ),
                ("validation", "ordinal_field_collision", Some("REQ-1294"))
            );
            assert_eq!(issues[0].spec_paths, ["input.SOURCE.ordinal"]);
            assert_eq!(
                yamaa_adapters::json_wire::decode(&issues[0].context).unwrap(),
                yamaa_adapters::json!({"dataset":"SOURCE", "field":"SourceOrdinal"})
            );

            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: absent.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID]}
columns: [{name: ID, type: str, derivation: {odm: {item: SRC.ITEM}}}]
"#,
                b"unused\n",
            );
            let issues = fixture
                .run()
                .err()
                .expect("fixed schema rejects ordinal before capture");
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (
                    issues[0].phase.as_str(),
                    issues[0].condition.as_str(),
                    issues[0].requirement.as_deref()
                ),
                ("validation", "odm_schema_field_type", Some("REQ-1295"))
            );
            assert_eq!(issues[0].spec_paths, ["input.SRC.ordinal"]);
            assert_eq!(
                yamaa_adapters::json_wire::decode(&issues[0].context).unwrap(),
                yamaa_adapters::json!({"dataset":"SRC", "declared":"ordinal"})
            );
        }

        #[test]
        fn metadata_odm_spelling_does_not_turn_an_ordinal_source_into_odm() {
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.csv, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: int, metadata: {odm: SRC.ITEM}, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
                b"ID\n3\n1\n",
            );
            let mut result = fixture.run().unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,POSITION\n3,1\n1,2\n"
            );
        }

        #[test]
        fn nested_column_and_row_odm_derivations_still_reject_ordinals_before_capture() {
            for declarations in [
        "columns:\n  - name: ID\n    type: str\n    derivation:\n      case:\n        - when: \"1 = 1\"\n          then: {odm: {item: SRC.ITEM}}\n        - otherwise: {literal: x}\n",
        "columns: [{name: ID, type: str, derivation: SRC.ID}]\nrows:\n  - id: records\n    derivations:\n      ID: {value: {odm: {item: SRC.ITEM}}}\n",
    ] {
        let fixture = authored(
            &format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: absent.parquet, ordinal: REC}}}}\noutput: {{path: result.csv, columns: [ID]}}\n{declarations}"),
            b"unused\n",
        );
        let issues = fixture.run().err().expect("ODM ordinal admission before capture");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "odm_schema_field_type");
        assert_eq!(issues[0].spec_paths, ["input.SRC.ordinal"]);
    }
        }

        #[test]
        #[cfg(feature = "parquet")]
        fn parquet_records_expose_identical_ordinal_policy_without_changing_stored_fields() {
            use yamaa_adapters::{parquet_artifact, typed_csv};
            let fixture = authored(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: source.parquet, ordinal: REC}}
output: {path: result.csv, columns: [ID, POSITION]}
columns:
  - {name: ID, type: str, derivation: SRC.ID}
  - {name: POSITION, type: int, derivation: SRC.REC}
"#,
                b"unused\n",
            );
            for (input, expected) in [
                (
                    b"ID\nb\na\n".as_slice(),
                    b"ID,POSITION\nb,1\na,2\n".as_slice(),
                ),
                (b"ID\n", b"ID,POSITION\n"),
            ] {
                // Known physical input; the expected generated positions above are authored independently.
                let table = typed_csv::parse(input, &[]).unwrap();
                let bytes = parquet_artifact::render(&table, &[0]).unwrap();
                fs::write(fixture.0.join("source.parquet"), bytes).unwrap();
                let mut result = fixture.run().unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
            }
        }
    }
    mod declarative_primary_intermediates {
        //! Same-input donor selection retains one captured relation across driver scopes.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-primary-intermediate-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn run(
                &self,
                body: &str,
            ) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>>
            {
                let specification = self.0.join("spec.yaml");
                fs::write(&specification, body).unwrap();
                let specification = specification.to_str().unwrap();
                file_application::domain(
                    Request {
                        specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-primary-intermediate",
                        specification,
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
        const BODY: &str = r#"schema_version: '1.0'
domain: TEST
base: SRC
keys: [ID, SEQ]
input:
  SRC: {path: src.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, SEQ, A, B]}
intermediates:
  - id: P
    dataset: SRC
    key: [ID]
    filter: "SRC.ELIG = 'Y'"
    order_by: [SRC.SEQ]
    keep: last
    no_match: null
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SEQ, type: int, derivation: SRC.SEQ}
  - {name: A, type: str, derivation: P.A}
  - {name: B, type: str, derivation: P.B}
"#;
        fn populate(fixture: &Fixture) {
            fs::write(
                fixture.0.join("src.csv"),
                "ID,SEQ,ELIG,A,B\n1,1,Y,early,x\n1,2,Y,last,\n2,1,N,hidden,hidden\n",
            )
            .unwrap();
        }
        #[test]
        fn driver_and_donor_share_the_input_but_selection_stays_at_the_donor_grain() {
            let fixture = Fixture::new();
            populate(&fixture);
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,SEQ,A,B\n1,1,last,\n1,2,last,\n2,1,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.A.derivation.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.B.derivation.source.no_match","handler":"no_match","count":1},
                ])
            );
        }
        #[test]
        fn a_different_template_driver_still_reads_the_original_primary_donors() {
            let fixture = Fixture::new();
            populate(&fixture);
            fs::write(fixture.0.join("alt.csv"), "ID\n1\n3\n2\n").unwrap();
            let body = BODY.replace("keys: [ID, SEQ]", "keys: [ID]")
        .replace("  SRC: {path: src.csv, types: {ID: int, SEQ: int}}", "  SRC: {path: src.csv, types: {ID: int, SEQ: int}}\n  ALT: {path: alt.csv, types: {ID: int}}")
        .replace("output: {path: result.csv, columns: [ID, SEQ, A, B]}", "output: {path: result.csv, columns: [ID, A, B]}")
        .replace("columns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: A, type: str, derivation: P.A}\n  - {name: B, type: str, derivation: P.B}", "rows:\n  - id: alternate\n    dataset: ALT\n    derivations: {ID: ALT.ID, A: P.A, B: P.B}\ncolumns:\n  - {name: ID, type: int}\n  - {name: A, type: str}\n  - {name: B, type: str}");
            let mut result = fixture.run(&body).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                "ID,A,B\n1,last,\n3,,\n2,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"rows[0].derivations.A.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.B.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"rows[0].derivations.A.source.no_match","handler":"no_match","count":2},
                    {"spec_path":"rows[0].derivations.B.source.no_match","handler":"no_match","count":2},
                ])
            );
        }
    }
    mod declarative_row_intermediates {
        //! Unchanged public YAML and independent plain-reference outputs/findings.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
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
        fn issue_json(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
        }
        #[test]
        fn complete_independent_record_contexts_and_unchanged_original() {
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/row_intermediate_reference.json"),
            )
            .unwrap();
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
                let expected = case["findings"].as_array().unwrap().clone();
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
                        .unwrap_or_else(|| yamaa_adapters::json!([])),
                    "{name} complete handler ledger"
                );
                if let Some(csv) = case.get("csv") {
                    // The committed artifact remains the oracle; an older rendered
                    // reference capture uses redundant .0 on whole binary64 values.
                    let csv = case.get("expected_csv").unwrap_or(csv);
                    assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                    assert!(result.save().unwrap());
                    let output = if matches!(
                        name.as_str(),
                        "adam-adlb-supplb-padded-key" | "adam-adlb-end-of-treatment"
                    ) {
                        "adlb.csv"
                    } else {
                        "result.csv"
                    };
                    assert_eq!(
                        fs::read(fixture.0.join(output)).unwrap(),
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
                }
            }
        }
    }
}
