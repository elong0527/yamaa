#![cfg(feature = "yaml")]
//! Execution contracts.

/// The host that each runtime name in these tests stands for.
fn host(runtime: &str) -> yamaa_core::project_function::Language {
    match runtime {
        "python" => yamaa_core::project_function::Language::Python,
        "r" => yamaa_core::project_function::Language::R,
        other => panic!("no host for test runtime {other}"),
    }
}

#[allow(dead_code)]
mod support;
mod execution {
    mod aggregate_record_operations {
        //! Independent YAML, donor bytes and exact saved truth for per-record aggregates.
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
                    "yamaa-aggregate-records-{}-{}",
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
                    yamaa_core::project_function::Language::Python,
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
            fn save(&self) -> Vec<u8> {
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
        fn specification(operation: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  A: {{path: a.csv, types: {{CUT: int}}}}\n  B: {{path: b.csv, types: {{N: int}}}}\nbase: A\noutput: {{path: result.csv, columns: [ID, CUT, TOTAL]}}\ncolumns:\n  - {{name: ID, type: str, derivation: A.ID}}\n  - {{name: CUT, type: int, derivation: A.CUT}}\n  - name: TOTAL\n    type: float\n    derivation:\n      aggregate:\n{operation}")
        }
        #[test]
        fn ordered_converted_bindings_reduce_only_matched_filtered_inclusive_records() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,3\nb,2\nc,\nd,1\n").unwrap();
            fs::write(fixture.0.join("b.csv"),b"ID,N,V,USE\na,2,1.5,Y\na,3,2,Y\na,4,invalid,Y\na,1,invalid,N\nb,2,bad,Y\nc,1,99,Y\nd,2,99,Y\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"),specification("        filter: B.USE = 'Y'\n        between: {value: CUT, lower: N}\n        derive:\n          - name: VALUE\n            type: float\n            derivation: {value: {source: B.V}, unconvertible: 0}\n          - name: TWICE\n            type: float\n            derivation: {compute: {expr: 'VALUE * 2'}}\n        expr: SUM(TWICE)\n")).unwrap();
            assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,3,7\nb,2,0\nc,,\nd,1,\n");
        }
        #[test]
        fn aggregate_bindings_admit_extractions_and_templates_before_binding() {
            for (derivation, kind, expression) in [
        (
            "{str_extract: {source: B.V, pattern: '([0-9]+)$', group: 1}}",
            "int",
            "MAX(V)",
        ),
        (
            "{case: [{when: \"B.USE = 'Y'\", then: {str_extract: {source: B.V, pattern: '([0-9]+)$', group: 1}}}, {otherwise: {literal: 0}}]}",
            "int",
            "MAX(V)",
        ),
        ("{str_template: '{B.V}!'}", "str", "COUNT(V)"),
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,3\nb,2\n").unwrap();
        fs::write(
            fixture.0.join("b.csv"),
            b"ID,N,V,USE\na,1,PERIOD 1,Y\na,2,PERIOD 2,Y\nb,1,PERIOD 1,Y\n",
        )
        .unwrap();
        fs::write(
            fixture.0.join("spec.yaml"),
            specification(&format!(
                "        derive:\n          - {{name: V, type: {kind}, derivation: {derivation}}}\n        expr: {expression}\n"
            )),
        )
        .unwrap();
        let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
        let issues = file_application::check(yamaa_core::project_function::Language::Python,
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
        );
        assert!(issues.is_empty(), "{derivation}: {issues:?}");
        assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,3,2\nb,2,1\n", "{derivation}");
    }
        }
        #[test]
        fn original_public_aggregate_fixtures_preserve_frozen_csv_bytes() {
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
                ("schema-aggregation", "adsl.csv"),
                ("schema-derive-date-epoch", "adex.csv"),
                ("schema-derive-text-score", "adqs.csv"),
                ("schema-aggregate-derive-intermediate", "adex.csv"),
                ("adam-adsl-new-therapy", "adsl.csv"),
                ("adam-adtr-nadir", "adtr.csv"),
                ("adam-adtte-pfs", "adtte.csv"),
                ("sdtm-rs-timepoint-response", "rs.csv"),
            ] {
                let fixture = Fixture::new();
                copy(&root.join(name), &fixture.0);
                let mut domain = fixture
                    .run()
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                assert!(domain.issues().is_empty(), "{name}: {:?}", domain.issues());
                assert!(domain.save().unwrap(), "{name}");
                assert_eq!(
                    fs::read(fixture.0.join(result)).unwrap(),
                    fs::read(root.join(name).join("expected").join(result)).unwrap(),
                    "{name}"
                );
            }
        }

        #[test]
        fn derive_and_range_faults_fail_before_any_output_with_registered_causes() {
            for (operation,condition,requirement) in [
        ("        derive: []\n        expr: SUM(V)\n","invalid_derive_step","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: W}\n        expr: SUM(V)\n","unknown_derive_variable","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: {literal: 1}}\n        expr: SUM(V)\n","invalid_derive_step","REQ-1191"),
        ("        between: {value: CUT, lower: USE}\n        expr: SUM(B.N)\n","incomparable_range_types","REQ-0121"),
        ("        between: {value: CUT, lower: ABSENT}\n        expr: SUM(B.N)\n","unknown_field","REQ-0121"),
    ] {
        let fixture=Fixture::new();
        fs::write(fixture.0.join("a.csv"),b"ID,CUT\na,2\n").unwrap();
        fs::write(fixture.0.join("b.csv"),b"ID,N,USE\na,2,Y\n").unwrap();
        fs::write(fixture.0.join("spec.yaml"),specification(operation)).unwrap();
        let issues=match fixture.run(){Ok(mut domain)=>{assert!(domain.output().is_none());assert!(domain.save().is_err());domain.issues()},Err(issues)=>issues};
        assert!(issues.iter().any(|issue|issue.condition==condition && issue.requirement.as_deref()==Some(requirement)),"{issues:?}");
        assert!(!fixture.0.join("result.csv").exists());
    }
        }
        /// A qualified `between.value` reads the record that built the row, so
        /// it names the driver of every row the aggregate is derived for.
        /// Another input has no record there and is refused (#2904).
        #[test]
        fn a_qualified_between_value_reads_only_the_rows_own_driver_record() {
            let run = |spec: String| {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,1\nb,2\n").unwrap();
                fs::write(
                    fixture.0.join("b.csv"),
                    b"ID,N\na,1\na,2\na,3\nb,1\nb,2\nb,3\n",
                )
                .unwrap();
                // C orders the subjects the other way and keeps each limit in
                // the column where A keeps its cutoff.
                fs::write(fixture.0.join("c.csv"), b"ID,LIMIT\nb,3\na,2\n").unwrap();
                fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
                let issues = match fixture.run() {
                    Ok(mut domain) if domain.issues().is_empty() => {
                        assert!(domain.save().unwrap());
                        return Ok(fs::read_to_string(fixture.0.join("result.csv")).unwrap());
                    }
                    Ok(mut domain) => {
                        assert!(domain.save().is_err());
                        domain.issues()
                    }
                    Err(issues) => issues,
                };
                Err(issues
                    .iter()
                    .map(|issue| yamaa_adapters::json!({"phase": issue.phase, "condition": issue.condition, "requirement": issue.requirement, "spec_paths": issue.spec_paths, "context": issue.context.clone()}))
                    .collect::<Vec<_>>())
            };
            let refused = |identifier: &str| {
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-0121",
                    "spec_paths": ["columns.TOTAL.derivation.aggregate.between.value"],
                    "context": {"identifier": identifier}
                })])
            };
            let operation = |value: &str| {
                format!("        between: {{value: {value}, lower: N}}\n        expr: SUM(B.N)\n")
            };
            let inputs = |spec: String| {
                spec.replace(
                    "  B: {path: b.csv, types: {N: int}}\n",
                    "  B: {path: b.csv, types: {N: int}}\n  C: {path: c.csv, types: {LIMIT: int}}\n",
                )
            };
            // Without row templates the base input builds every row.
            for value in ["A.CUT", "CUT"] {
                assert_eq!(
                    run(inputs(specification(&operation(value)))),
                    Ok("ID,CUT,TOTAL\na,1,1\nb,2,3\n".into())
                );
            }
            assert_eq!(
                run(inputs(specification(&operation("C.LIMIT")))),
                refused("C.LIMIT")
            );
            // Rows built from A and from C share the aggregate's column, so
            // neither input supplies a record for every row.
            let rows = |value: &str, templates: &str| {
                format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID, ORIGIN]\ninput:\n  A: {{path: a.csv, types: {{CUT: int}}}}\n  B: {{path: b.csv, types: {{N: int}}}}\n  C: {{path: c.csv, types: {{LIMIT: int}}}}\noutput: {{path: result.csv, columns: [ID, ORIGIN, TOTAL]}}\ncolumns:\n  - {{name: ID, type: str}}\n  - {{name: ORIGIN, type: str}}\n  - name: TOTAL\n    type: float\n    derivation:\n      aggregate:\n{}rows:\n{templates}", operation(value))
            };
            let from_a =
                "  - id: a\n    dataset: A\n    derivations: {ID: A.ID, ORIGIN: {literal: A}}\n";
            let from_c =
                "  - id: c\n    dataset: C\n    derivations: {ID: C.ID, ORIGIN: {literal: C}}\n";
            assert_eq!(
                run(rows("A.CUT", from_a)),
                Ok("ID,ORIGIN,TOTAL\na,A,1\nb,A,3\n".into())
            );
            for value in ["A.CUT", "C.LIMIT"] {
                assert_eq!(
                    run(rows(value, &format!("{from_a}{from_c}"))),
                    refused(value)
                );
            }
        }
        #[test]
        fn named_derive_read_requires_keep_and_uses_original_current_record() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,2\nb,3\n").unwrap();
            fs::write(fixture.0.join("b.csv"), b"ID,N\nb,10\na,1\na,4\n").unwrap();
            let operation="        derive:\n          - {name: N, type: int, derivation: B.N}\n          - {name: C, type: int, derivation: CAP.CUT}\n          - {name: VALUE, type: int, derivation: {least: {sources: [N, C]}}}\n        expr: SUM(VALUE)\n";
            let spec=specification(operation).replace("base: A\n","base: A\nintermediates:\n  - {id: CAP, dataset: A, key: {ID: A.ID}, order_by: [A.CUT], keep: first}\n");
            fs::write(fixture.0.join("spec.yaml"), &spec).unwrap();
            assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,2,3\nb,3,3\n");
            fs::write(
                fixture.0.join("spec.yaml"),
                spec.replace(", order_by: [A.CUT], keep: first", ""),
            )
            .unwrap();
            let issues = match fixture.run() {
                Ok(domain) => domain.issues(),
                Err(issues) => issues,
            };
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.condition == "invalid_derive_step"
                        && issue.requirement.as_deref() == Some("REQ-1242")),
                "{issues:?}"
            );
        }

        fn original_fieldless_count(expression: &str, key: &str, filter: &str) -> String {
            format!(
        "schema_version: '1.0'\ndomain: OUT\ninput:\n  SRC: {{path: src.csv, types: {{N: int}}}}\n  OTHER: {{path: other.csv, types: {{N: int}}}}\nbase: SRC\nkeys: [K]\noutput: {{path: out.csv, columns: [K, V]}}\ncolumns:\n  - name: V\n    type: int\n    derivation:\n      aggregate:\n        expr: '{expression}'\n        key: {key}\n{filter}  - {{name: K, type: int, derivation: SRC.N}}\n"
    )
        }

        fn fieldless_count_fixture(source: &str) -> Fixture {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("src.csv"), "N\n1\n").unwrap();
            fs::write(fixture.0.join("other.csv"), "N\n1\n1\n2\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"), source).unwrap();
            fixture
        }

        fn saved_count(fixture: &Fixture, mut result: file_application::Domain) -> Vec<u8> {
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(!fixture.0.join("out.csv").exists());
            assert!(result.save().unwrap());
            fs::read(fixture.0.join("out.csv")).unwrap()
        }

        // Original planning/test_aggregate_relation_reads.py inputs: COUNT still owns
        // a relation even though it reads no payload field. Its key producer is written
        // after the consuming column, and only the two matching donors are eligible.
        #[test]
        fn original_fieldless_reductions_schedule_real_key_dependencies_once() {
            for (expression, count) in [
                ("COUNT(OTHER.*)", 2),
                ("SUM(OTHER.N)", 2),
                ("COUNT(OTHER.*) + COUNT(OTHER.*)", 4),
                ("COUNT(OTHER.*) + SUM(OTHER.N)", 4),
            ] {
                for key in [
                    "{N: K}",
                    "{N: {source: K}}",
                    "{N: {literal: 1}}",
                    "{N: {greatest: {sources: [K, K]}}}",
                ] {
                    let source = original_fieldless_count(expression, key, "");
                    let fixture = fieldless_count_fixture(&source);
                    let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                    assert!(
                        result.issues().is_empty(),
                        "{expression}, {key}: {:?}",
                        result.issues()
                    );
                    assert_eq!(
                        saved_count(&fixture, result),
                        format!("K,V\n1,{count}\n").as_bytes()
                    );
                }
            }
            let source = original_fieldless_count("COUNT(OTHER.*)", "{N: K}", "")
                .replace("keys: [K]", "keys: [N]")
                .replace("columns: [K, V]", "columns: [N, V]")
                .replace(
                    "      aggregate:\n        expr: 'COUNT(OTHER.*)'\n        key: {N: K}\n",
                    "      aggregate: 'COUNT(OTHER.*)'\n",
                )
                .replace("name: K", "name: N");
            let fixture = fieldless_count_fixture(&source);
            let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(saved_count(&fixture, result), b"N,V\n1,2\n");
        }

        fn complete_aggregate_issues(fixture: &Fixture) -> Vec<yamaa_core::json::Value> {
            let issues = match fixture.run() {
                Ok(mut result) => {
                    assert!(result.output().is_none());
                    assert!(result.save().is_err());
                    result.issues()
                }
                Err(issues) => issues,
            };
            assert!(!fixture.0.join("out.csv").exists());
            issues
                .iter()
                .map(|issue| {
                    yamaa_adapters::json!({
                        "phase": issue.phase, "condition": issue.condition,
                        "requirement": issue.requirement, "spec_paths": issue.spec_paths,
                        "context": issue.context.clone()
                    })
                })
                .collect()
        }

        #[test]
        fn original_fieldless_reduction_type_findings_keep_one_authored_owner() {
            for expression in [
                "COUNT(OTHER.*)",
                "SUM(OTHER.N)",
                "COUNT(OTHER.*) + COUNT(OTHER.*)",
                "COUNT(OTHER.*) + SUM(OTHER.N)",
            ] {
                for (literal, kind) in [("true", "bool"), ("x", "str")] {
                    let fixture = fieldless_count_fixture(&original_fieldless_count(
                        expression,
                        &format!("{{N: {{literal: {literal}}}}}"),
                        "",
                    ));
                    assert_eq!(
                        complete_aggregate_issues(&fixture),
                        vec![yamaa_adapters::json!({
                            "phase": "validation", "condition": "incompatible_input_type",
                            "requirement": "LOOKUP-9", "spec_paths": ["columns.V.derivation.aggregate.expr"],
                            "context": {"source": "key[N]", "expected": kind, "actual": "int"}
                        })],
                        "{expression}, {literal}"
                    );
                }
            }
            for (expression, path) in [
                ("COUNT(OTHER.*)", "columns.V.derivation.aggregate.filter"),
                ("SUM(OTHER.N)", "columns.V.derivation.aggregate.expr"),
                (
                    "COUNT(OTHER.*) + SUM(OTHER.N)",
                    "columns.V.derivation.aggregate.expr",
                ),
            ] {
                let fixture = fieldless_count_fixture(&original_fieldless_count(
                    expression,
                    "{N: {literal: true}}",
                    "        filter: OTHER.N > 0\n",
                ));
                assert_eq!(
                    complete_aggregate_issues(&fixture),
                    vec![yamaa_adapters::json!({
                        "phase": "validation", "condition": "incompatible_input_type",
                        "requirement": "LOOKUP-9", "spec_paths": [path],
                        "context": {"source": "key[N]", "expected": "bool", "actual": "int"}
                    })],
                    "{expression}"
                );
            }
        }

        #[test]
        fn original_fieldless_reduction_unknown_names_do_not_invent_join_findings() {
            for (expression, key, identifier, requirement) in [
                ("COUNT(OTHER.*)", "{ABSENT: K}", "OTHER.ABSENT", "LOOKUP-5"),
                ("COUNT(OTHER.*)", "{N: ABSENT}", "ABSENT", "LOOKUP-5"),
                ("COUNT(ABSENT.*)", "{N: K}", "ABSENT.*", "REQ-0103"),
            ] {
                let fixture =
                    fieldless_count_fixture(&original_fieldless_count(expression, key, ""));
                assert_eq!(
                    complete_aggregate_issues(&fixture),
                    vec![yamaa_adapters::json!({
                        "phase": "validation", "condition": "unknown_field",
                        "requirement": requirement, "spec_paths": ["columns.V.derivation.aggregate.expr"],
                        "context": {"identifier": identifier}
                    })],
                    "{expression}, {key}"
                );
            }
            for (expression, identifier) in [
                ("SUM(ABSENT.N)", "ABSENT.N"),
                ("COUNT(ABSENT.*)", "ABSENT.*"),
            ] {
                let fixture = fieldless_count_fixture(&original_fieldless_count(
                    expression,
                    "{N: K}",
                    "        filter: ABSENT.OTHER > 0\n",
                ));
                assert_eq!(
                    complete_aggregate_issues(&fixture),
                    vec![yamaa_adapters::json!({
                        "phase": "validation", "condition": "unknown_field",
                        "requirement": "REQ-0103", "spec_paths": ["columns.V.derivation.aggregate.expr"],
                        "context": {"identifier": identifier}
                    })],
                    "{expression}"
                );
            }
        }

        #[test]
        fn fieldless_literal_bindings_keep_the_filter_as_the_only_donor_reference() {
            for expression in ["COUNT(OTHER.*)", "SUM(H)"] {
                for key in ["{N: {literal: true}}", "{N: K}"] {
                    let source = original_fieldless_count(expression, key, "        filter: OTHER.N > 0\n")
                .replace(
                    "        filter: OTHER.N > 0\n",
                    "        derive:\n          - {name: H, type: int, derivation: {literal: 1}}\n        filter: OTHER.N > 0\n",
                );
                    let fixture = fieldless_count_fixture(&source);
                    if key == "{N: K}" {
                        let result = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                        assert_eq!(saved_count(&fixture, result), b"K,V\n1,2\n");
                    } else {
                        assert_eq!(
                            complete_aggregate_issues(&fixture),
                            vec![yamaa_adapters::json!({
                                "phase": "validation", "condition": "incompatible_input_type",
                                "requirement": "LOOKUP-9", "spec_paths": ["columns.V.derivation.aggregate.filter"],
                                "context": {"source": "key[N]", "expected": "bool", "actual": "int"}
                            })],
                            "{expression}"
                        );
                    }
                }
            }
        }

        const ORIGINAL_DERIVE_SPEC: &str = r#"schema_version: "1.0"
domain: ADQS
keys: [STUDYID, USUBJID]
input:
  QS: input/qs.csv
output:
  path: adqs.csv
  columns: [STUDYID, USUBJID, AVAL]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: QS.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: QS.USUBJID
  - name: AVAL
    type: float
    label: Analysis Value
    derivation:
      aggregate:
        filter: "QS.QSCAT = 'SCALE'"
        key: [STUDYID, USUBJID]
        derive:
DERIVE_BINDINGS
        expr: "REDUCTION_EXPRESSION"
rows:
  - id: subscale
    filter: "QS.QSCAT = 'SCALE' AND QS.QSTESTCD = 'PF01'"
    derivations: {}
"#;
        const ORIGINAL_QS: &str = r#"STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,2
S1,001,PF03,SCALE,3
"#;
        const ORIGINAL_QS_WITH_TEXT: &str = r#"STUDYID,USUBJID,QSTESTCD,QSCAT,QSORRES
S1,001,PF01,SCALE,1
S1,001,PF02,SCALE,N/A
S1,001,PF03,SCALE,3
"#;
        const ORIGINAL_ENUM_DERIVE_SPEC: &str = r#"schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  EX: input/ex.csv
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTEDT]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: EX.STUDYID
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: EX.USUBJID
  - name: TRTEDT
    type: date
    label: Date of Last Exposure
    derivation:
      aggregate:
        key: [STUDYID, USUBJID]
        derive:
          - name: H
            type: date
            derivation:
              impute:
                type: date
                source: EX.EXENDTC
                day: last
                minimum_source_precision: month
        expr: "MAX(H)"
rows:
  - id: one
    derivations: {}
"#;
        const ORIGINAL_EX: &str = r#"STUDYID,USUBJID,EXENDTC
S1,001,2024-05
S1,002,2024-06
"#;

        fn original_derive_fixture(bindings: &str, expression: &str, input: &str) -> Fixture {
            let fixture = Fixture::new();
            fs::create_dir(fixture.0.join("input")).unwrap();
            fs::write(fixture.0.join("input/qs.csv"), input).unwrap();
            let bindings = bindings
                .lines()
                .map(|line| format!("          {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(
                fixture.0.join("spec.yaml"),
                ORIGINAL_DERIVE_SPEC
                    .replace("DERIVE_BINDINGS", &bindings)
                    .replace("REDUCTION_EXPRESSION", expression),
            )
            .unwrap();
            fixture
        }

        fn original_derive_issues(fixture: &Fixture) -> Vec<yamaa_adapters::issue_rows::Issue> {
            match fixture.run() {
                Ok(mut domain) => {
                    assert!(domain.output().is_none());
                    assert!(domain.save().is_err());
                    domain.issues()
                }
                Err(issues) => issues,
            }
        }

        #[test]
        fn original_aggregate_bindings_reject_duplicates_and_forward_references() {
            let binding = "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES";
            let fixture = original_derive_fixture(
                &format!("{binding}\n{binding}"),
                "SUM(QSNUM)",
                ORIGINAL_QS,
            );
            let issues = original_derive_issues(&fixture);
            assert_eq!(issues[0].condition, "invalid_derive_step");
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert!(context["reason"].as_str().unwrap().contains("unique"));

            let fixture = original_derive_fixture(
                &format!("- name: DOUBLED\n  type: float\n  derivation: QSNUM\n{binding}"),
                "SUM(DOUBLED)",
                ORIGINAL_QS,
            );
            let issues = original_derive_issues(&fixture);
            assert_eq!(issues[0].condition, "unknown_derive_variable");
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["variable"], "QSNUM");
        }

        #[test]
        fn original_aggregate_bindings_read_converted_earlier_values_and_recover_once() {
            for (bindings, expression, input, expected) in [
        (
            "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES\n- name: DOUBLED\n  type: float\n  derivation:\n    compute:\n      expr: 'QSNUM * 2'",
            "SUM(DOUBLED)",
            ORIGINAL_QS,
            12.0,
        ),
        (
            "- name: QSNUM\n  type: float\n  derivation:\n    value:\n      source: QS.QSORRES\n    unconvertible: 0",
            "SUM(QSNUM)",
            ORIGINAL_QS_WITH_TEXT,
            4.0,
        ),
    ] {
        let fixture = original_derive_fixture(bindings, expression, input);
        let domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        let table = domain.output_table().unwrap();
        assert_eq!(table.rows(), 1);
        let batch = &table.batches()[0];
        assert_eq!(batch.column(0).as_any().downcast_ref::<arrow_array::StringArray>().unwrap().iter().collect::<Vec<_>>(), [Some("S1")]);
        assert_eq!(batch.column(1).as_any().downcast_ref::<arrow_array::StringArray>().unwrap().iter().collect::<Vec<_>>(), [Some("001")]);
        assert_eq!(batch.column(2).as_any().downcast_ref::<arrow_array::Float64Array>().unwrap().iter().collect::<Vec<_>>(), [Some(expected)]);
    }
            let fixture = original_derive_fixture(
                "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES",
                "SUM(QSNUM)",
                ORIGINAL_QS_WITH_TEXT,
            );
            assert_eq!(
                original_derive_issues(&fixture)[0].condition,
                "conversion_failed"
            );
        }

        #[test]
        fn original_aggregate_derive_enum_arguments_are_not_variable_references() {
            let fixture = Fixture::new();
            fs::create_dir(fixture.0.join("input")).unwrap();
            fs::write(fixture.0.join("input/ex.csv"), ORIGINAL_EX).unwrap();
            fs::write(fixture.0.join("spec.yaml"), ORIGINAL_ENUM_DERIVE_SPEC).unwrap();
            let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
            assert!(domain.issues().is_empty(), "{:?}", domain.issues());
            assert!(domain.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adsl.csv")).unwrap(),
                b"STUDYID,USUBJID,TRTEDT\nS1,001,2024-05-31\nS1,002,2024-06-30\n"
            );
        }

        #[test]
        fn original_aggregate_binding_relation_and_field_findings_keep_their_context() {
            let fixture = original_derive_fixture(
        "- name: A\n  type: float\n  derivation: QS.QSORRES\n- name: B\n  type: float\n  derivation: EX.EXSEQ",
        "SUM(A)",
        ORIGINAL_QS,
    );
            let issues = original_derive_issues(&fixture);
            assert!(issues
                .iter()
                .any(|issue| issue.condition == "invalid_derive_step"));

            let fixture = original_derive_fixture(
        "- name: A\n  type: float\n  derivation: QS.QSORRES\n- name: B\n  type: float\n  derivation: XX.YY\n- name: CAPNUM\n  type: float\n  derivation: CAP.DSNUM",
        "SUM(A)",
        ORIGINAL_QS,
    );
            add_original_cap_intermediate(&fixture, true);
            let issues = original_derive_issues(&fixture);
            let issue = issues
                .iter()
                .find(|issue| issue.condition == "invalid_derive_step")
                .unwrap();
            assert_eq!(issue.requirement.as_deref(), Some("REQ-1191"));
            let context: yamaa_core::json::Value = issue.context.clone();
            assert_eq!(context["relations"], yamaa_adapters::json!(["QS", "XX"]));

            for (source, requirement) in
                [("QS.NOSUCHFIELD", "REQ-0103"), ("CAP.NOSUCH", "REQ-0125")]
            {
                let fixture = original_derive_fixture(
                    &format!("- name: A\n  type: float\n  derivation: {source}"),
                    "SUM(A)",
                    ORIGINAL_QS,
                );
                add_original_cap_intermediate(&fixture, true);
                let issues = original_derive_issues(&fixture);
                let unknown = issues
                    .iter()
                    .filter(|issue| issue.condition == "unknown_field")
                    .collect::<Vec<_>>();
                assert_eq!(unknown.len(), 1, "{issues:?}");
                assert_eq!(unknown[0].requirement.as_deref(), Some(requirement));
                let context: yamaa_core::json::Value = unknown[0].context.clone();
                assert_eq!(context["identifier"], source);
            }

            let fixture = original_derive_fixture(
        "- name: QSNUM\n  type: float\n  derivation: QS.QSORRES\n- name: CAPNUM\n  type: float\n  derivation: CAP.DSNUM\n- name: CAPPED\n  type: float\n  derivation: {least: {sources: [QSNUM, CAPNUM]}}",
        "SUM(CAPPED)",
        ORIGINAL_QS,
    );
            add_original_cap_intermediate(&fixture, false);
            let issues = original_derive_issues(&fixture);
            let invalid = issues
                .iter()
                .filter(|issue| issue.condition == "invalid_derive_step")
                .collect::<Vec<_>>();
            assert_eq!(invalid.len(), 1, "{issues:?}");
            assert_eq!(invalid[0].requirement.as_deref(), Some("REQ-1242"));
            let context: yamaa_core::json::Value = invalid[0].context.clone();
            assert_eq!(context["intermediate"], "CAP");
        }

        fn add_original_cap_intermediate(fixture: &Fixture, keep: bool) {
            fs::write(
                fixture.0.join("input/ds.csv"),
                b"STUDYID,USUBJID,DSNUM\nS1,001,9\nS1,001,2\nS2,002,10\n",
            )
            .unwrap();
            let path = fixture.0.join("spec.yaml");
            let spec = fs::read_to_string(&path).unwrap()
        .replace("  QS: input/qs.csv", "  QS: input/qs.csv\n  DS: input/ds.csv")
        .replace("output:\n", &format!("intermediates:\n  - id: CAP\n    dataset: DS\n    key: [STUDYID, USUBJID]{}\noutput:\n", if keep { "\n    order_by: [DS.DSNUM]\n    keep: first" } else { "" }))
        .replace("  - id: subscale\n", "  - id: subscale\n    dataset: QS\n");
            fs::write(path, spec).unwrap();
        }
    }
    mod baseline_window_types {
        //! Baseline temporal operands retain their typed condition and authored input site.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn issues(rows: &[yamaa_adapters::issue_rows::Issue]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(|row| yamaa_adapters::json!({
                    "phase":row.phase,"condition":row.condition,"requirement":row.requirement,
                    "spec_paths":row.spec_paths,"context":row.context.clone()
                }))
                .collect::<Vec<_>>())
        }
        #[test]
        fn baseline_types_preserve_conditions_and_reference_priority() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["baseline_window_types"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(std::env::temp_dir().join(format!(
                    "yamaa-baseline-window-types-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                )));
                fs::create_dir(&fixture.0).unwrap();
                for (path, bytes) in case["inputs"].as_object().unwrap() {
                    let path = fixture.0.join(path);
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, bytes.as_str().unwrap()).unwrap();
                }
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "baseline_window_types",
                    specification: path,
                    base_directory: ".",
                };
                assert_eq!(
                    issues(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let mut result = match file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(result) => result,
                    Err(rows) => {
                        assert_eq!(
                            issues(&rows),
                            case["issues"],
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        assert!(case["csv"].is_null());
                        assert!(!fixture.0.join("out.csv").exists());
                        continue;
                    }
                };
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                if let Some(csv) = case["csv"].as_str() {
                    assert!(result.save().unwrap());
                    assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), csv.as_bytes());
                } else {
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("out.csv").exists());
                }
            }
        }
    }
    mod case_item_order {
        //! REQ-0339 case item order is one validation finding at every expression site,
        //! with the same issue from check and domain in both shared host routes. Paths
        //! follow the schema validator: keyed list members are named by `name` or `id`.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::{domain_entry::Request, json::Value};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new(body: &str) -> Self {
                let root = std::env::temp_dir().join(format!(
                    "yamaa-case-item-order-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                fs::write(
                    root.join("spec.yaml"),
                    format!(
                        "schema_version: '1.0'\ndomain: ADSL\nbase: DM\nkeys: [USUBJID]\ninput:\n  DM: {{path: dm.csv, types: {{AGE: int}}}}\n  EX: {{path: ex.csv, types: {{DOSE: float}}}}\noutput: {{path: out.csv, columns: [USUBJID, FL]}}\n{body}"
                    ),
                )
                .unwrap();
                fs::write(root.join("dm.csv"), "USUBJID,AGE\nA,34\nB,70\n").unwrap();
                fs::write(root.join("ex.csv"), "USUBJID,DOSE\nA,5\nB,10\n").unwrap();
                Self(root)
            }
            fn specification(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().to_owned()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        fn identity<'a>(runtime: &'a str, specification: &'a str) -> Identity<'a> {
            Identity {
                runtime,
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "case-item-order",
                specification,
                base_directory: ".",
            }
        }

        fn rows(issues: &[Issue]) -> Value {
            Value::Array(
                issues
                    .iter()
                    .map(|row| {
                        yamaa_adapters::json!({
                            "phase": row.phase, "condition": row.condition, "requirement": row.requirement,
                            "spec_paths": row.spec_paths, "context": row.context.clone()
                        })
                    })
                    .collect(),
            )
        }

        fn issues(fixture: &Fixture) -> Vec<(String, Value)> {
            let specification = fixture.specification();
            let mut result = Vec::new();
            for runtime in ["python", "r"] {
                let request = || Request {
                    specification: &specification,
                    environment: None,
                };
                let checked = file_application::check(
                    crate::host(runtime),
                    request(),
                    identity(runtime, &specification),
                );
                let built = match file_application::domain(
                    crate::host(runtime),
                    request(),
                    identity(runtime, &specification),
                ) {
                    Ok(domain) => domain.issues(),
                    Err(issues) => issues,
                };
                result.push((format!("{runtime}/check"), rows(&checked)));
                result.push((format!("{runtime}/domain"), rows(&built)));
            }
            result
        }

        fn finding(path: &str, expected: &str) -> Value {
            yamaa_adapters::json!({
                "phase": "validation", "condition": "invalid_field_type", "requirement": "REQ-0339",
                "spec_paths": [path], "context": {"expected": expected}
            })
        }

        const NO_BRANCH: &str = "at least one when/then item";
        const NOT_LAST: &str = "otherwise only as the last item";
        const ONLY: &str = "{case: [{otherwise: {literal: N}}]}";

        fn column(derivation: &str) -> String {
            format!("columns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: FL, type: str, derivation: {derivation}}}\n")
        }

        #[test]
        fn case_item_order_reports_req_0339_at_each_expression_site() {
            let sites = [
                (column(ONLY), "columns.FL.derivation.case", NO_BRANCH),
                (column("{case: []}"), "columns.FL.derivation.case", NO_BRANCH),
                (
                    column("{case: [{otherwise: {literal: N}}, {when: 'DM.AGE >= 65', then: {literal: Y}}]}"),
                    "columns.FL.derivation.case[0]",
                    NOT_LAST,
                ),
                (
                    column("{case: [{when: 'DM.AGE >= 65', then: {literal: Y}}, {otherwise: {literal: N}}, {otherwise: {literal: U}}]}"),
                    "columns.FL.derivation.case[1]",
                    NOT_LAST,
                ),
                (
                    column(&format!("{{case: [{{when: 'DM.AGE >= 65', then: {ONLY}}}, {{otherwise: {{literal: N}}}}]}}")),
                    "columns.FL.derivation.case[0].then.case",
                    NO_BRANCH,
                ),
                (
                    format!("columns:\n  - {{name: USUBJID, type: str}}\n  - {{name: FL, type: str}}\nrows:\n  - {{id: subject, dataset: DM, derivations: {{USUBJID: DM.USUBJID, FL: {ONLY}}}}}\n"),
                    "rows.subject.derivations.FL.case",
                    NO_BRANCH,
                ),
                (
                    format!("intermediates:\n  - {{id: DOSED, dataset: EX, key: [USUBJID], derivations: {{GROUP: {ONLY}}}}}\n{}", column("DOSED.GROUP")),
                    "intermediates.DOSED.derivations.GROUP.case",
                    NO_BRANCH,
                ),
                (
                    column(&format!("{{aggregate: {{key: [USUBJID], filter: 'EX.DOSE > 0', derive: [{{name: LEVEL, type: str, derivation: {ONLY}}}], expr: 'MAX(LEVEL)'}}}}")),
                    "columns.FL.derivation.aggregate.derive.LEVEL.derivation.case",
                    NO_BRANCH,
                ),
            ];
            for (body, path, expected) in sites {
                let fixture = Fixture::new(&body);
                for (route, actual) in issues(&fixture) {
                    assert_eq!(
                        actual,
                        yamaa_adapters::json!([finding(path, expected)]),
                        "{route}: {body}"
                    );
                }
                assert!(!fixture.0.join("out.csv").exists());
            }
        }

        #[test]
        fn case_with_only_otherwise_items_reports_the_case_and_each_misplaced_item() {
            let fixture = Fixture::new(&column(
                "{case: [{otherwise: {literal: N}}, {otherwise: {literal: U}}]}",
            ));
            for (route, actual) in issues(&fixture) {
                assert_eq!(
                    actual,
                    yamaa_adapters::json!([
                        finding("columns.FL.derivation.case", NO_BRANCH),
                        finding("columns.FL.derivation.case[0]", NOT_LAST),
                    ]),
                    "{route}"
                );
            }
        }

        #[test]
        fn ordered_case_items_still_build() {
            let fixture = Fixture::new(&column(
                "{case: [{when: 'DM.AGE >= 65', then: {literal: Y}}, {otherwise: {literal: N}}]}",
            ));
            for (route, actual) in issues(&fixture) {
                assert_eq!(actual, yamaa_adapters::json!([]), "{route}");
            }
        }
    }
    mod check_output_validation {
        //! `check` reports the validation findings that `domain` reaches without
        //! study data: output declarations (#2295), formula qualification and column
        //! order. The same first issue, in both host routes. Committed negative
        //! benchmarks and `expected/error.yaml` remain the oracle.
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
                let p = std::env::temp_dir().join(format!(
                    "yamaa-check-output-{}-{}-{}",
                    name,
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&p).unwrap();
                Self(p)
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for item in fs::read_dir(source).unwrap() {
                let item = item.unwrap();
                if item.file_type().unwrap().is_dir() {
                    copy(&item.path(), &destination.join(item.file_name()));
                } else {
                    fs::copy(item.path(), destination.join(item.file_name())).unwrap();
                }
            }
        }

        fn issue(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
        }

        fn expected(path: &Path) -> yamaa_core::json::Value {
            let text = fs::read_to_string(path).unwrap();
            let d = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes())
                .unwrap()
                .document;
            fn value(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Boolean(v) => (*v).into(),
                    N::Integer(v) => yamaa_adapters::json_wire::decode(v).unwrap(),
                    N::Float(v) => yamaa_adapters::json!(v),
                    N::Text(v) => v.clone().into(),
                    N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
                    N::Mapping(pairs) => pairs
                        .iter()
                        .map(|&(key, item)| {
                            (value(d, key).as_str().unwrap().to_owned(), value(d, item))
                        })
                        .collect(),
                }
            }
            value(&d, d.root())
        }

        fn identity<'a>(
            runtime: &'a str,
            example: &'a str,
            specification: &'a str,
        ) -> Identity<'a> {
            Identity {
                runtime,
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example,
                specification,
                base_directory: ".",
            }
        }

        /// Run `check` and `domain` over one staged specification and return the
        /// first issue each route reports.
        fn first_issues(
            specification: &str,
            example: &str,
            runtime: &str,
        ) -> (yamaa_core::json::Value, yamaa_core::json::Value) {
            let request = || Request {
                specification,
                environment: None,
            };
            let checked = file_application::check(
                crate::host(runtime),
                request(),
                identity(runtime, example, specification),
            );
            assert!(!checked.is_empty(), "{example}: check reported no issues");
            let built = file_application::domain(
                crate::host(runtime),
                request(),
                identity(runtime, example, specification),
            );
            let built = match built {
                Ok(result) => {
                    let issues = result.issues().to_vec();
                    assert!(!issues.is_empty(), "{example}: domain reported no issues");
                    issues
                }
                Err(failure) => {
                    assert!(!failure.is_empty(), "{example}: domain reported no issues");
                    failure
                }
            };
            (issue(&checked[0]), issue(&built[0]))
        }

        fn benchmarks_root() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
        }

        /// Every committed negative benchmark whose first issue comes from `output.*`
        /// is an output-declaration validation case. The denominator is pinned so a
        /// benchmark that silently stops being selected fails this test.
        fn output_validation_benchmarks() -> Vec<String> {
            let root = benchmarks_root();
            let mut selected = Vec::new();
            for entry in fs::read_dir(&root).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("negative-") {
                    continue;
                }
                let oracle = entry.path().join("expected/error.yaml");
                if !oracle.exists() {
                    continue;
                }
                let expected = expected(&oracle);
                let from_output = expected["spec_paths"]
                    .as_array()
                    .and_then(|paths| paths.first())
                    .and_then(|path| path.as_str())
                    .is_some_and(|path| path.starts_with("output."));
                if from_output {
                    selected.push(name);
                }
            }
            selected.sort();
            selected
        }

        #[test]
        fn check_matches_domain_on_output_validation_benchmarks() {
            let selected = output_validation_benchmarks();
            assert_eq!(
                selected,
                [
                    "negative-order-repeated",
                    "negative-order-unknown-column",
                    "negative-output-transport",
                ],
                "output-validation benchmark denominator changed"
            );
            let root = benchmarks_root();
            for name in &selected {
                let original = root.join(name);
                let fixture = Fixture::new(name);
                copy(&original.join("input"), &fixture.0.join("input"));
                fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let oracle = expected(&original.join("expected/error.yaml"));
                for runtime in ["python", "r"] {
                    let (checked, built) = first_issues(&spec, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert_eq!(
                        checked, oracle,
                        "{name} ({runtime}): first issue vs expected/error.yaml"
                    );
                }
            }
        }

        /// The three rows of #2295's table, with the issue's own minimal study: an
        /// undeclared output column, `decimals` on a parquet output, and a negative
        /// `decimals`. None is covered by a committed negative benchmark.
        #[test]
        fn check_matches_domain_on_the_issue_rows() {
            let rows: [(&str, &str, yamaa_core::json::Value); 3] = [
                (
                    "undeclared",
                    "{path: out.csv, columns: [USUBJID, N, ZZZ]}",
                    yamaa_adapters::json!({"phase":"validation","condition":"undeclared_column","requirement":"REQ-0234","spec_paths":["output.columns[2]"],"context":{"column":"ZZZ"}}),
                ),
                (
                    "decimals-parquet",
                    "{path: out.parquet, columns: [USUBJID, N], decimals: 2}",
                    yamaa_adapters::json!({"phase":"validation","condition":"decimals_not_applicable","requirement":"REQ-0762","spec_paths":["output.decimals"],"context":{"path":"out.parquet","profile":"parquet"}}),
                ),
                (
                    "decimals-negative",
                    "{path: out.csv, columns: [USUBJID, N], decimals: -1}",
                    yamaa_adapters::json!({"phase":"validation","condition":"invalid_field_type","requirement":"REQ-0744","spec_paths":["output.decimals"],"context":{"expected":"an integer from 0 through 18","actual":-1}}),
                ),
            ];
            for (name, output, oracle) in rows {
                let fixture = Fixture::new(name);
                fs::create_dir(fixture.0.join("input")).unwrap();
                fs::write(fixture.0.join("input/dm.csv"), "USUBJID,AGE\nS-001,34\n").unwrap();
                let spec = format!(
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input/dm.csv}}\noutput: {output}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: N, type: float, derivation: DM.AGE}}\n"
        );
                fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
                let spec_path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                for runtime in ["python", "r"] {
                    let (checked, built) = first_issues(&spec_path, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert_eq!(checked, oracle, "{name} ({runtime}): first issue");
                }
            }
        }

        /// The partial oracle of `expected/error.yaml`: phase, condition and spec
        /// paths exactly, the requirement when one is written, and every written
        /// context field.
        fn matches_oracle(
            oracle: &yamaa_core::json::Value,
            issue: &yamaa_core::json::Value,
        ) -> bool {
            fn subset(want: &yamaa_core::json::Value, got: &yamaa_core::json::Value) -> bool {
                match (want, got) {
                    (
                        yamaa_core::json::Value::Object(want),
                        yamaa_core::json::Value::Object(got),
                    ) => want
                        .iter()
                        .all(|(key, value)| got.get(key).is_some_and(|got| subset(value, got))),
                    _ => want == got,
                }
            }
            ["phase", "condition", "spec_paths"]
                .into_iter()
                .all(|field| oracle[field] == issue[field])
                && oracle
                    .get("requirement")
                    .is_none_or(|requirement| *requirement == issue["requirement"])
                && oracle
                    .get("context")
                    .is_none_or(|context| subset(context, &issue["context"]))
        }

        /// Validation negatives whose finding `check` does not report, with what the
        /// finding is decided from. `domain` reaches each of them once it reads or
        /// binds the inputs. A case that `check` starts to report fails until it
        /// leaves this list.
        const NOT_REPORTED_BY_CHECK: [(&str, &str); 31] = [
            (
                "negative-adlb-computed-param",
                "a cycle through an aggregate and a case",
            ),
            ("negative-cut-coded", "the stored type of an input field"),
            (
                "negative-dose-intensity",
                "an aggregate field outside a reduction",
            ),
            (
                "negative-first-available-self",
                "a cycle through first_available",
            ),
            (
                "negative-forward-key-reference",
                "column order in a specification with row templates",
            ),
            (
                "negative-grouped-lookup-ungrouped-key",
                "the group keys of the row template that reads a lookup",
            ),
            (
                "negative-grouped-range-ungrouped-value",
                "the group keys of the row template that reads a lookup range",
            ),
            (
                "negative-keys-missing",
                "a key column that reads a cut of a non-key column",
            ),
            (
                "negative-lookup-bad-range",
                "the declared types of a lookup range",
            ),
            (
                "negative-lookup-derivation-scope",
                "the driver of each row template that reads a lookup derivation",
            ),
            (
                "negative-lookup-key-other-template",
                "the driver of each row template that reads a lookup key",
            ),
            (
                "negative-lookup-mismatched",
                "the fields stored in the current input",
            ),
            (
                "negative-lookup-unknown-key",
                "the fields stored in the lookup input",
            ),
            (
                "negative-odm-schema-ambiguous",
                "the fields stored in the ODM input",
            ),
            (
                "negative-odm-schema-missing",
                "the fields stored in the ODM input",
            ),
            ("negative-path-missing", "whether the input file exists"),
            (
                "negative-path-symlink",
                "whether the input file is a symbolic link",
            ),
            (
                "negative-review-unknown-date",
                "an unknown name in a flag condition",
            ),
            (
                "negative-row-aggregate-keys",
                "the scope of a grouped row aggregate",
            ),
            (
                "negative-row-filter-lookup-scope",
                "the driver of the row template whose filter reads a lookup",
            ),
            (
                "negative-row-phase-flag",
                "the phase of a column a row template reads",
            ),
            (
                "negative-row-ungrouped-second-template",
                "the group keys of a later grouped row template",
            ),
            (
                "negative-row-value-self",
                "a cycle through first_available and row_value",
            ),
            (
                "negative-row-window-ungrouped",
                "the group keys of a grouped row template's window",
            ),
            (
                "negative-source-missing-field",
                "a source filter name that is not an input field",
            ),
            (
                "negative-source-ordinal-collision",
                "the fields stored in the input",
            ),
            (
                "negative-source-trivial-filter",
                "a source filter on an output column",
            ),
            (
                "negative-source-undeclared-field",
                "the fields stored in the input",
            ),
            (
                "negative-sum-non-numeric",
                "the declared type of an aggregate operand",
            ),
            (
                "negative-types-unknown-field",
                "the fields stored in the input",
            ),
            (
                "negative-verification-lookup-key",
                "the driver of the rows a verification's lookup reads",
            ),
        ];

        /// Validation negatives this harness does not run, with the reason.
        const NOT_BUILT_HERE: [(&str, &str); 2] = [
            (
                "negative-allowed-values-beside-codelist",
                "the bound codelist's items come from its host environment, which this harness does not pass",
            ),
            (
                "negative-function-contract",
                "domain activates its project functions only in a host",
            ),
        ];

        /// Every committed validation-phase negative benchmark that conformance runs.
        fn validation_benchmarks() -> Vec<String> {
            let root = benchmarks_root();
            let mut selected = Vec::new();
            for entry in fs::read_dir(&root).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                let oracle = entry.path().join("expected/error.yaml");
                if !name.starts_with("negative-") || !oracle.exists() {
                    continue;
                }
                let metadata = expected(&entry.path().join("benchmark.yaml"));
                // A Define benchmark fails in its study's composition, which
                // `submission_transport` runs; its domain build succeeds.
                if expected(&oracle)["phase"] == "validation"
                    && metadata["conformance"]["excluded"].is_null()
                    && metadata["define"].is_null()
                {
                    selected.push(name);
                }
            }
            selected.sort();
            selected
        }

        /// `check` reports every validation finding of a committed negative that
        /// `domain` reaches without study data: the same first issue in both host
        /// routes, matching `expected/error.yaml`. Benchmarks run in place, so
        /// parents, links and relative paths keep their authored meaning.
        #[test]
        fn check_matches_domain_on_validation_benchmarks() {
            let selected = validation_benchmarks();
            for (name, _) in NOT_REPORTED_BY_CHECK.iter().chain(&NOT_BUILT_HERE) {
                assert!(selected.iter().any(|s| s == name), "{name}: not selected");
            }
            let root = benchmarks_root();
            let mut compared = Vec::new();
            for name in &selected {
                if NOT_BUILT_HERE.iter().any(|(n, _)| n == name) {
                    continue;
                }
                let case = root.join(name);
                let spec = case.join("spec.yaml").to_str().unwrap().to_owned();
                let oracle = expected(&case.join("expected/error.yaml"));
                let missed = NOT_REPORTED_BY_CHECK.iter().find(|(n, _)| n == name);
                for runtime in ["python", "r"] {
                    if let Some((_, reason)) = missed {
                        let checked = file_application::check(
                            crate::host(runtime),
                            Request {
                                specification: &spec,
                                environment: None,
                            },
                            identity(runtime, name, &spec),
                        );
                        assert!(
                            !checked
                                .first()
                                .is_some_and(|i| matches_oracle(&oracle, &issue(i))),
                            "{name} ({runtime}): check reports {reason}; remove the exclusion"
                        );
                        continue;
                    }
                    let (checked, built) = first_issues(&spec, name, runtime);
                    assert_eq!(
                        checked, built,
                        "{name} ({runtime}): check/domain first issue"
                    );
                    assert!(
                        matches_oracle(&oracle, &checked),
                        "{name} ({runtime}): first issue {checked:?} vs expected/error.yaml {oracle:?}"
                    );
                }
                if missed.is_none() {
                    compared.push(name.as_str());
                }
            }
            for name in ["negative-direct-dose-read", "negative-forward-reference"] {
                assert!(compared.contains(&name), "{name}: not compared");
            }
        }

        /// Formula reads bind without a source schema, so when nothing else is wrong
        /// `check` reports exactly what `domain` reports: each qualified name that is
        /// not a record lookup, then column order over the formulas' reads.
        #[test]
        fn check_reports_formula_qualification_and_column_order() {
            let qualified = |expr: &str, identifier: &str| yamaa_adapters::json!({"phase":"validation","condition":"qualified_identifier","requirement":"REQ-0442","spec_paths":["columns.A.derivation.compute.expr"],"context":{"expr":expr,"identifier":identifier}});
            let forward = |dependency: &str| yamaa_adapters::json!({"phase":"validation","condition":"forward_reference","requirement":"REQ-0071","spec_paths":["columns.A.derivation.compute"],"context":{"column":"A","dependency":dependency}});
            let cycle = yamaa_adapters::json!({"phase":"validation","condition":"dependency_cycle","requirement":"REQ-0072","spec_paths":["columns.A.derivation.compute","columns.B.derivation.compute"],"context":{"cycle":["A","B","A"]}});
            let rows: [(&str, &str, &str, Vec<yamaa_core::json::Value>); 4] = [
                (
                    "qualified-then-forward",
                    "A, B",
                    "  - {name: A, type: float, derivation: {compute: {expr: 'B + DM.AGE + DM.WT'}}}\n  - {name: B, type: float, derivation: DM.AGE}\n",
                    vec![
                        qualified("B + DM.AGE + DM.WT", "DM.AGE"),
                        qualified("B + DM.AGE + DM.WT", "DM.WT"),
                        forward("B"),
                    ],
                ),
                (
                    "forward-in-written-order",
                    "A, B, C",
                    "  - {name: A, type: float, derivation: {compute: {expr: 'C - B'}}}\n  - {name: B, type: float, derivation: DM.AGE}\n  - {name: C, type: float, derivation: DM.WT}\n",
                    vec![forward("C"), forward("B")],
                ),
                (
                    "cycle",
                    "A, B",
                    "  - {name: A, type: float, derivation: {compute: {expr: 'B + 1'}}}\n  - {name: B, type: float, derivation: {compute: {expr: 'A + 1'}}}\n",
                    vec![cycle],
                ),
                (
                    "earlier-column",
                    "B, A",
                    "  - {name: B, type: float, derivation: DM.AGE}\n  - {name: A, type: float, derivation: {compute: {expr: 'B * 2'}}}\n",
                    vec![],
                ),
            ];
            for (name, outputs, columns, oracle) in rows {
                let fixture = Fixture::new(name);
                fs::create_dir(fixture.0.join("input")).unwrap();
                fs::write(
                    fixture.0.join("input/dm.csv"),
                    "USUBJID,AGE,WT\nU1,34,70.5\n",
                )
                .unwrap();
                fs::write(
                    fixture.0.join("spec.yaml"),
                    format!(
                        "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: {{path: input/dm.csv, types: {{AGE: float, WT: float}}}}}}\noutput: {{path: out.csv, columns: [USUBJID, {outputs}]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n{columns}"
                    ),
                )
                .unwrap();
                let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let request = || Request {
                    specification: &spec,
                    environment: None,
                };
                for runtime in ["python", "r"] {
                    let checked = file_application::check(
                        crate::host(runtime),
                        request(),
                        identity(runtime, name, &spec),
                    );
                    let built = match file_application::domain(
                        crate::host(runtime),
                        request(),
                        identity(runtime, name, &spec),
                    ) {
                        Ok(result) => result.issues().to_vec(),
                        Err(failure) => failure,
                    };
                    let checked = checked.iter().map(issue).collect::<Vec<_>>();
                    let built = built.iter().map(issue).collect::<Vec<_>>();
                    assert_eq!(checked, oracle, "{name} ({runtime}): check");
                    assert_eq!(built, oracle, "{name} ({runtime}): domain");
                }
            }
        }

        fn precision_specification(decimals: Option<&str>, kind: &str) -> String {
            let precision = decimals.map_or(String::new(), |n| format!(", decimals: {n}"));
            format!(
        "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\ninput: {{DM: input.csv}}\noutput: {{path: out.csv, columns: [USUBJID, N]{precision}}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: DM.USUBJID}}\n  - {{name: N, type: {kind}, derivation: DM.VALUE}}\n"
    )
        }

        #[test]
        fn csv_precision_range_is_validated_for_empty_and_non_float_outputs_in_both_hosts() {
            let oracle: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/csv_precision_limit.json"
            ))
            .unwrap();
            let huge = format!("1{}", "0".repeat(400));
            for decimals in [
                "-1",
                "19",
                "400",
                "5000",
                "10000000",
                "9223372036854775807",
                "9223372036854775808",
                "18446744073709551616",
                huge.as_str(),
            ] {
                let mut expected = oracle.clone();
                expected["context"]["actual"] =
                    yamaa_adapters::json_wire::decode(decimals).unwrap();
                for (kind, input) in [
                    ("float", "USUBJID,VALUE\nS-001,0.125\n"),
                    ("float", "USUBJID,VALUE\n"),
                    ("int", "USUBJID,VALUE\nS-001,34\n"),
                ] {
                    let fixture = Fixture::new("precision-limit");
                    fs::write(
                        fixture.0.join("spec.yaml"),
                        precision_specification(Some(decimals), kind),
                    )
                    .unwrap();
                    fs::write(fixture.0.join("input.csv"), input).unwrap();
                    let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                    for runtime in ["python", "r"] {
                        let (checked, built) =
                            first_issues(&specification, "precision-limit", runtime);
                        assert_eq!(checked, expected, "check {runtime}: {decimals}/{kind}");
                        assert_eq!(built, expected, "domain {runtime}: {decimals}/{kind}");
                    }
                    assert!(!fixture.0.join("out.csv").exists());
                    // The static finding does not require study data to exist.
                    fs::remove_file(fixture.0.join("input.csv")).unwrap();
                    for runtime in ["python", "r"] {
                        let issues = file_application::check(
                            crate::host(runtime),
                            Request {
                                specification: &specification,
                                environment: None,
                            },
                            identity(runtime, "precision-limit", &specification),
                        );
                        assert_eq!(issues.len(), 1);
                        assert_eq!(issue(&issues[0]), expected);
                    }
                }
            }
        }

        #[test]
        fn csv_precision_endpoints_and_omission_keep_exact_saved_bytes() {
            for (decimals, expected) in [
                (Some("0"), "USUBJID,N\nS-001,0\nS-002,0\n"),
                (
                    Some("18"),
                    "USUBJID,N\nS-001,0.125000000000000000\nS-002,-0.125000000000000000\n",
                ),
                (None, "USUBJID,N\nS-001,0.125\nS-002,-0.125\n"),
            ] {
                let fixture = Fixture::new("precision-endpoints");
                fs::write(
                    fixture.0.join("spec.yaml"),
                    precision_specification(decimals, "float"),
                )
                .unwrap();
                fs::write(
                    fixture.0.join("input.csv"),
                    "USUBJID,VALUE\nS-001,0.125\nS-002,-0.125\n",
                )
                .unwrap();
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                for runtime in ["python", "r"] {
                    let request = || Request {
                        specification: &specification,
                        environment: None,
                    };
                    assert!(file_application::check(
                        crate::host(runtime),
                        request(),
                        identity(runtime, "precision-endpoints", &specification)
                    )
                    .is_empty());
                    let mut domain = file_application::domain(
                        crate::host(runtime),
                        request(),
                        identity(runtime, "precision-endpoints", &specification),
                    )
                    .unwrap();
                    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                    assert!(domain.save().unwrap());
                    assert_eq!(
                        fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                        expected
                    );
                }
            }
        }
    }
    mod compiled_model {
        //! Original YAML reaches core compilation; expectations are authored from the
        //! six unchanged documents, not emitted by a compiler or the legacy plan codec.
        use crate::support::table_schema as schema;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::{
            shipped_schema,
            specification_source::{InheritancePort, PreparedDocument, Source},
        };
        use yamaa_core::inheritance::{Source as Identity, SourceError};
        use yamaa_core::{
            bound_expression::{Binding, Read},
            dataset::{DatasetPlan, Expression, RowMode, WindowKind},
            schema::DocumentNode as N,
            specification::{PrepareError, PreparedSpecification},
            table::TableSchema,
            value::ColumnType as T,
        };

        struct Parents {
            root: PathBuf,
            reads: Vec<String>,
            unsupported: bool,
        }
        impl InheritancePort for Parents {
            type Error = ();
            fn canonicalize(
                &mut self,
                _: &str,
                written: &str,
            ) -> Result<Identity, SourceError<()>> {
                assert!(
                    written.ends_with(".yaml"),
                    "preparation acquired study authority"
                );
                Ok(Identity {
                    identity: written.into(),
                    display_path: written.into(),
                })
            }
            fn capture(&mut self, source: &Identity) -> Result<Vec<u8>, SourceError<()>> {
                assert!(
                    source.identity.ends_with(".yaml"),
                    "compiler read study data"
                );
                self.reads.push(source.identity.clone());
                let mut bytes = fs::read(self.root.join(&source.identity)).unwrap();
                if self.unsupported && source.identity == "spec_organization.yaml" {
                    bytes = String::from_utf8(bytes)
                        .unwrap()
                        .replace("input/lb.csv", "input/lb.xlsx")
                        .into_bytes();
                }
                Ok(bytes)
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, written: &str) -> Result<String, ()> {
                Ok(written.into())
            }
        }
        fn prepare(name: &str, unsupported: bool) -> (PreparedDocument, Parents) {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks")
                .join(name);
            let entry = if name == "schema-inheritance" {
                "spec_study.yaml"
            } else {
                "spec.yaml"
            };
            let mut parents = Parents {
                root,
                reads: vec![],
                unsupported,
            };
            let document = shipped_schema::prepare(
                Source {
                    identity: entry.into(),
                    bytes: fs::read(parents.root.join(entry)).unwrap(),
                },
                entry.into(),
                &mut parents,
            )
            .unwrap();
            (document, parents)
        }
        fn names(plan: &DatasetPlan) -> Vec<&str> {
            plan.output()
                .columns()
                .iter()
                .map(|column| column.name.as_str())
                .collect()
        }
        fn provenance(document: &PreparedDocument, plan: &DatasetPlan, inherited: bool) {
            for assignment in plan
                .templates()
                .iter()
                .flat_map(|template| &template.assignments)
                .chain(plan.columns())
            {
                let declaring = document.declaring_source(&assignment.path).unwrap();
                assert_eq!(
                    declaring,
                    if inherited {
                        "spec_organization.yaml"
                    } else {
                        "spec.yaml"
                    }
                );
            }
            assert_eq!(
                document.raw().locations.len(),
                document.raw().document.nodes().len()
            );
            for parent in document.parents() {
                assert_eq!(
                    parent.raw().locations.len(),
                    parent.raw().document.nodes().len()
                );
                assert_eq!(
                    parent.source().bytes,
                    fs::read(
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../../benchmarks/schema-inheritance")
                            .join(&parent.source().identity)
                    )
                    .unwrap()
                );
            }
        }

        #[test]
        fn all_six_original_yaml_documents_compile_against_independent_model_expectations() {
            type Case<'a> = (
                &'a str,
                Vec<TableSchema>,
                &'a [&'a str],
                &'a [usize],
                usize,
                usize,
            );
            let cases: [Case<'_>; 6] = [
                (
                    "negative-zero-division",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBSTRESN", T::Str),
                        ("LBBLRESN", T::Str),
                    ])],
                    &["STUDYID", "USUBJID", "PARAMCD", "AVAL", "BASE", "PCHG"],
                    &[0, 1, 2],
                    1,
                    0,
                ),
                (
                    "negative-integer-overflow",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("CELLCNT", T::Str),
                        ("VOLUML", T::Str),
                    ])],
                    &["STUDYID", "USUBJID", "CELLCNT", "VOLUML", "CELLTOT"],
                    &[0, 1],
                    1,
                    0,
                ),
                (
                    "adam-adlb-ordered-sum",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("LBSEQ", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBTEST", T::Str),
                        ("VISIT", T::Str),
                        ("LBSTRESN", T::Float),
                    ])],
                    &[
                        "STUDYID", "USUBJID", "AVISIT", "PARAMCD", "PARAM", "AVAL", "DTYPE",
                    ],
                    &[0, 1, 3, 2],
                    2,
                    1,
                ),
                (
                    "schema-window-functions",
                    vec![schema(&[
                        ("STUDYID", T::Str),
                        ("USUBJID", T::Str),
                        ("VISITN", T::Int),
                        ("VISIT", T::Str),
                        ("VSDTC", T::Date),
                        ("TRTSDT", T::Date),
                        ("VSSTRESN", T::Float),
                        ("VSEVAL", T::Str),
                    ])],
                    &[
                        "STUDYID", "USUBJID", "VISITN", "VISIT", "VSDTC", "TRTSDT", "VSSTRESN",
                        "VSEVAL", "VSSEQ", "SEVRANKC", "SEVRANKD", "PREVRES", "NEXTRES",
                        "EVALPREV", "LASTRES", "CARRYRES", "BLFL",
                    ],
                    &[1, 2],
                    1,
                    0,
                ),
                (
                    "schema-lookup",
                    vec![
                        schema(&[("USUBJID", T::Str)]),
                        schema(&[
                            ("USUBJID", T::Str),
                            ("AEDECOD", T::Str),
                            ("AEOUT", T::Str),
                            ("AEDY", T::Str),
                        ]),
                        schema(&[("LLTNAME", T::Str), ("PTNAME", T::Str)]),
                    ],
                    &["USUBJID", "DTHDY", "DTHCAUS", "DTHPTERM"],
                    &[0],
                    1,
                    0,
                ),
                (
                    "schema-inheritance",
                    vec![schema(&[
                        ("USUBJID", T::Str),
                        ("LBTESTCD", T::Str),
                        ("LBSTRESN", T::Float),
                        ("LBSTRESU", T::Str),
                    ])],
                    &["USUBJID", "PARAMCD", "AVAL", "AVALU"],
                    &[0, 1],
                    1,
                    0,
                ),
            ];
            for (name, schemas, output, keys, templates, checks) in cases {
                let (document, parents) = prepare(name, false);
                // Only YAML parent captures are possible. No study bytes are supplied;
                // binding receives independently enumerated immutable metadata. The
                // chain is captured from the entry upward: the compound level names
                // the organization level.
                assert_eq!(
                    parents.reads,
                    if name == "schema-inheritance" {
                        vec!["spec_compound.yaml", "spec_organization.yaml"]
                    } else {
                        vec![]
                    }
                );
                let compiled = PreparedSpecification::prepare(document.model()).unwrap();
                let plan = compiled
                    .bind_sources(&schemas.iter().collect::<Vec<_>>())
                    .unwrap();
                assert_eq!(names(&plan), output, "{name}");
                let output_types = match name {
                    "negative-zero-division" => {
                        vec![T::Str, T::Str, T::Str, T::Float, T::Float, T::Float]
                    }
                    "negative-integer-overflow" => vec![T::Str, T::Str, T::Int, T::Int, T::Int],
                    "adam-adlb-ordered-sum" => {
                        vec![T::Str, T::Str, T::Str, T::Str, T::Str, T::Float, T::Str]
                    }
                    "schema-window-functions" => vec![
                        T::Str,
                        T::Str,
                        T::Int,
                        T::Str,
                        T::Date,
                        T::Date,
                        T::Float,
                        T::Str,
                        T::Int,
                        T::Int,
                        T::Int,
                        T::Float,
                        T::Float,
                        T::Float,
                        T::Float,
                        T::Float,
                        T::Str,
                    ],
                    "schema-lookup" => vec![T::Str, T::Int, T::Str, T::Str],
                    "schema-inheritance" => vec![T::Str, T::Str, T::Float, T::Str],
                    _ => unreachable!(),
                };
                assert_eq!(
                    plan.output()
                        .columns()
                        .iter()
                        .map(|column| column.kind)
                        .collect::<Vec<_>>(),
                    output_types,
                    "{name}"
                );
                let projection = match name {
                    "negative-integer-overflow" => vec!["STUDYID", "USUBJID", "CELLTOT"],
                    "adam-adlb-ordered-sum" => vec![
                        "STUDYID", "USUBJID", "PARAMCD", "PARAM", "AVISIT", "AVAL", "DTYPE",
                    ],
                    _ => output.to_vec(),
                };
                assert_eq!(compiled.projection(), projection, "{name}");
                assert_eq!(plan.keys(), keys, "{name}");
                assert_eq!(plan.templates().len(), templates, "{name}");
                assert_eq!(plan.verifications().len(), checks, "{name}");
                provenance(&document, &plan, name == "schema-inheritance");
                match name {
                    "negative-zero-division" | "negative-integer-overflow" => {
                        let Expression::Compute(numeric) =
                            &plan.columns().last().unwrap().expression
                        else {
                            panic!("numeric derivation")
                        };
                        assert_eq!(
                            numeric.expression().expression(),
                            if name == "negative-zero-division" {
                                "100 * (AVAL - BASE) / BASE"
                            } else {
                                "CELLCNT * VOLUML"
                            }
                        );
                        let bindings = if name == "negative-zero-division" {
                            vec![
                                Binding {
                                    name: "AVAL".into(),
                                    read: Read::Column(3),
                                },
                                Binding {
                                    name: "BASE".into(),
                                    read: Read::Column(4),
                                },
                            ]
                        } else {
                            vec![
                                Binding {
                                    name: "CELLCNT".into(),
                                    read: Read::Column(2),
                                },
                                Binding {
                                    name: "VOLUML".into(),
                                    read: Read::Column(3),
                                },
                            ]
                        };
                        assert_eq!(numeric.bindings(), bindings);
                    }
                    "adam-adlb-ordered-sum" => {
                        assert_eq!(plan.templates()[0].mode, RowMode::Records);
                        assert_eq!(plan.templates()[1].mode, RowMode::Groups(vec![0, 1, 5]));
                        let total = plan.templates()[1]
                            .assignments
                            .iter()
                            .find(|assignment| assignment.column == 5)
                            .unwrap();
                        let Expression::Aggregate(aggregate) = &total.expression else {
                            panic!("the authored grouped SUM is a bound aggregate");
                        };
                        assert_eq!(
                            aggregate.relation(),
                            &yamaa_core::dataset::AggregateRelation::CurrentGroup
                        );
                        assert_eq!(aggregate.expression().expression(), "SUM(LB.LBSTRESN)");
                        assert_eq!(
                            aggregate.bindings(),
                            [yamaa_core::dataset::AggregateBinding {
                                name: "LB.LBSTRESN".into(),
                                column: 6,
                            }]
                        );
                        assert_eq!(aggregate.expression().reductions().len(), 1);
                        let reduction = &aggregate.expression().reductions()[0];
                        assert_eq!(reduction.reducer, yamaa_core::expression::Reducer::Sum);
                        assert_eq!(
                            reduction.operand,
                            yamaa_core::expression::ReductionOperand::Value("LB.LBSTRESN".into())
                        );
                    }
                    "schema-window-functions" => {
                        let kinds = plan
                            .columns()
                            .iter()
                            .filter_map(|assignment| match &assignment.expression {
                                Expression::Window(window) => Some(window.kind),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(
                            kinds,
                            vec![
                                WindowKind::RowNumber,
                                WindowKind::Competition,
                                WindowKind::Dense,
                                WindowKind::RowValue {
                                    column: 6,
                                    offset: -1
                                },
                                WindowKind::RowValue {
                                    column: 6,
                                    offset: 1
                                },
                                WindowKind::RowValue {
                                    column: 6,
                                    offset: -1
                                },
                                WindowKind::Locf {
                                    column: 6,
                                    include_current: false
                                },
                                WindowKind::Locf {
                                    column: 6,
                                    include_current: true
                                },
                                WindowKind::BaselineFlag {
                                    date: 4,
                                    reference_date: 5,
                                    value: 6,
                                    inclusive: true
                                }
                            ]
                        );
                    }
                    "schema-lookup" => {
                        assert_eq!(
                            plan.intermediates()
                                .iter()
                                .map(|item| item.identifier.as_str())
                                .collect::<Vec<_>>(),
                            ["DEATHEV", "MEDDRA_PT"]
                        );
                        assert_eq!(
                            plan.intermediates()[0].keys,
                            [yamaa_core::dataset::IntermediateKey {
                                source_column: 0,
                                path: "intermediates[0].key".into(),
                                value: yamaa_core::dataset::MatchValue::Variable(
                                    yamaa_core::dataset::MatchRead::Output {
                                        column: 0,
                                        identifier: "USUBJID".into()
                                    }
                                ),
                            }]
                        );
                        assert_eq!(
                            plan.intermediates()[1].keys,
                            [yamaa_core::dataset::IntermediateKey {
                                source_column: 0,
                                path: "intermediates[1].key".into(),
                                value: yamaa_core::dataset::MatchValue::Variable(
                                    yamaa_core::dataset::MatchRead::Output {
                                        column: 2,
                                        identifier: "DTHCAUS".into()
                                    }
                                ),
                            }]
                        );
                        assert_eq!(
                            plan.secondary()
                                .iter()
                                .map(|source| source.name.as_str())
                                .collect::<Vec<_>>(),
                            ["AE", "MEDDRA"]
                        );
                    }
                    "schema-inheritance" => {
                        assert_eq!(
                            compiled
                                .sources()
                                .iter()
                                .map(|source| source.name.as_str())
                                .collect::<Vec<_>>(),
                            ["LB"]
                        );
                        assert_eq!(
                            document.declaring_source("columns.AVAL.label"),
                            Some("spec_study.yaml")
                        );
                        assert_eq!(
                            document.declaring_source("input.LB.types"),
                            Some("spec_compound.yaml")
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }

        #[test]
        fn inherited_unknown_profile_retains_written_layer_and_location_before_study_effects() {
            let (document, parents) = prepare("schema-inheritance", true);
            let Err(PrepareError::Invalid(findings)) =
                PreparedSpecification::prepare(document.model())
            else {
                panic!("unknown source profile")
            };
            assert_eq!(findings.len(), 1);
            let diagnostic = findings[0].diagnostic();
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().requirement
                ),
                ("validation", "source_profile_unknown", Some("REQ-0852"))
            );
            assert_eq!(diagnostic.spec_paths, ["input.LB.path"]);
            assert_eq!(
                diagnostic.context,
                [
                    (
                        "dataset".into(),
                        yamaa_core::json::Value::String("LB".into())
                    ),
                    (
                        "path".into(),
                        yamaa_core::json::Value::String("input/lb.xlsx".into())
                    )
                ]
                .into()
            );
            assert_eq!(
                document.declaring_source(&diagnostic.spec_paths[0]),
                Some("spec_organization.yaml")
            );
            let parent = document
                .parents()
                .iter()
                .find(|parent| parent.source().identity == "spec_organization.yaml")
                .unwrap();
            let raw = &parent.raw().document;
            let input = raw.field(raw.root(), "input").unwrap();
            let node = raw.field(input, "LB").unwrap();
            assert_eq!(raw.nodes()[node], N::Text("input/lb.xlsx".into()));
            let position = parent.raw().locations[node];
            assert_eq!(position.line, 3);
            assert!(position.offset < parent.source().bytes.len());
            assert_eq!(
                parents.reads,
                ["spec_compound.yaml", "spec_organization.yaml"]
            );
        }
    }
    mod completed_intermediate_scopes {
        //! Original committed YAML and independent expected artifacts remain the oracle.
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
                let p = std::env::temp_dir().join(format!(
                    "yamaa-intermediate-scopes-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&p).unwrap();
                Self(p)
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for item in fs::read_dir(source).unwrap() {
                let item = item.unwrap();
                if item.file_type().unwrap().is_dir() {
                    copy(&item.path(), &destination.join(item.file_name()));
                } else {
                    fs::copy(item.path(), destination.join(item.file_name())).unwrap();
                }
            }
        }
        fn issue(issue: &yamaa_adapters::issue_rows::Issue) -> yamaa_core::json::Value {
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":issue.context.clone()})
        }
        fn expected(path: &Path) -> yamaa_core::json::Value {
            let text = fs::read_to_string(path).unwrap();
            let d = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes())
                .unwrap()
                .document;
            fn value(d: &yamaa_core::schema::Document, id: usize) -> yamaa_core::json::Value {
                use yamaa_core::schema::DocumentNode as N;
                match &d.nodes()[id] {
                    N::Null => yamaa_core::json::Value::Null,
                    N::Boolean(v) => (*v).into(),
                    N::Integer(v) => yamaa_adapters::json_wire::decode(v).unwrap(),
                    N::Float(v) => yamaa_adapters::json!(v),
                    N::Text(v) => v.clone().into(),
                    N::Sequence(ids) => ids.iter().map(|&id| value(d, id)).collect(),
                    N::Mapping(pairs) => pairs
                        .iter()
                        .map(|&(key, item)| {
                            (value(d, key).as_str().unwrap().to_owned(), value(d, item))
                        })
                        .collect(),
                }
            }
            value(&d, d.root())
        }
        fn unchanged_original(name: &str) {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");

            let original = root.join("benchmarks").join(name);
            let fixture = Fixture::new();
            copy(&original.join("input"), &fixture.0.join("input"));
            fs::copy(original.join("spec.yaml"), fixture.0.join("spec.yaml")).unwrap();
            let spec = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let id = Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: name,
                specification: &spec,
                base_directory: ".",
            };
            let outcome = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &spec,
                    environment: None,
                },
                id,
            );
            if let Ok(mut result) = outcome {
                if name.starts_with("negative-") {
                    assert_eq!(result.issues().len(), 1, "{name}");
                    let expected: yamaa_core::json::Value =
                        expected(&original.join("expected/error.yaml"));
                    assert_eq!(issue(&result.issues()[0]), expected, "{name}");
                    assert!(result.save().is_err());
                } else {
                    assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                    assert!(result.save().unwrap());
                    for file in fs::read_dir(original.join("expected")).unwrap() {
                        let file = file.unwrap();
                        assert_eq!(
                            fs::read(fixture.0.join(file.file_name())).unwrap(),
                            fs::read(file.path()).unwrap(),
                            "{name}"
                        );
                    }
                }
            } else {
                let actual = outcome.err().unwrap().iter().map(issue).collect::<Vec<_>>();
                if name == "negative-lookup-id-collision" {
                    let expected: yamaa_core::json::Value =
                        expected(&original.join("expected/error.yaml"));
                    assert_eq!(actual, vec![expected], "{name}");
                } else {
                    panic!("{name}: {actual:?}");
                }
            }
        }

        macro_rules! original {
            ($test:ident,$case:literal) => {
                #[test]
                fn $test() {
                    unchanged_original($case)
                }
            };
        }
        original!(unique, "schema-lookup-intermediate-unique");
        original!(duplicate, "negative-lookup-duplicate-intermediate");
        original!(collision, "negative-lookup-id-collision");
        original!(ae, "sdtm-ae-meddra-hierarchy");
        original!(cm, "sdtm-cm-whodrug-coding");
        original!(vs, "sdtm-vs-collected-form");
        original!(lb, "sdtm-lb-last-observation-before-exposure");
        original!(timepoints, "sdtm-vs-planned-time-points");
        original!(qs, "sdtm-qs-questionnaire-items");
        original!(fa, "sdtm-fa-event-findings");
        original!(ex, "sdtm-ex-from-ec");
        original!(derived_filter, "schema-lookup-intermediate-derived-filter");
        original!(
            correlated_derive,
            "schema-lookup-intermediate-correlated-derive"
        );
        original!(derived_window, "schema-window-intermediate-rank");
        original!(eot, "adam-adlb-end-of-treatment");
        original!(serious, "adam-adae-serious-events");
        original!(self_unique, "adam-adae-prior-serious-event");

        original!(advs, "adam-advs-windows");

        fn independent(
            spec: &str,
            source: &str,
            donor: Option<&str>,
        ) -> (
            Fixture,
            Result<file_application::Domain, file_application::Failure>,
        ) {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::write(fixture.0.join("source.csv"), source).unwrap();
            if let Some(donor) = donor {
                fs::write(fixture.0.join("donor.csv"), donor).unwrap();
            }
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            let id = Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "independent-intermediate",
                specification: &path,
                base_directory: ".",
            };
            let result = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                id,
            );
            (fixture, result)
        }
        #[test]
        fn unused_unique_checks_are_eager_and_retain_all_failed_declarations() {
            let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    verifications:\n      - unique: {columns: [ID], id: donor-id}\n      - unique: [X]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n1,2\n"));
            let mut result = result.unwrap();
            let actual = result.issues().iter().map(issue).collect::<Vec<_>>();
            assert_eq!(
                actual,
                vec![
                    yamaa_adapters::json!({"phase":"verification","condition":"duplicate_intermediate_records","requirement":"REQ-1245","spec_paths":["intermediates[0].verifications[0].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["ID"],"duplicate_count":1,"verification_id":"donor-id"}}),
                    yamaa_adapters::json!({"phase":"verification","condition":"duplicate_intermediate_records","requirement":"REQ-1245","spec_paths":["intermediates[0].verifications[1].unique"],"context":{"intermediate":"P","dataset":"DONOR","columns":["X"],"duplicate_count":1}})
                ]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn unused_donor_derivation_failure_is_not_hidden_by_an_unread_lookup() {
            let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    derivations: {NEXT: {compute: {expr: 'X + 1'}}}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) =
                independent(spec, "ID\n1\n", Some("ID,X\n1,9223372036854775807\n"));
            let mut result = result.unwrap();
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition, "integer_overflow");
            assert!(result.issues()[0].spec_paths[0]
                .starts_with("intermediates[0].derivations.NEXT.compute"));
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn current_row_correlation_cannot_turn_a_unique_check_into_a_lazy_check() {
            let spec="schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {ID: int, X: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: DONOR\n    filter: 'DONOR.X > SRC.ID'\n    derivations: {NEXT: {compute: {expr: 'X + SRC.ID'}}}\n    verifications: [{unique: [ID]}]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (_fixture, result) = independent(spec, "ID\n1\n", Some("ID,X\n1,2\n"));
            let result = match result {
                Err(issues) => issues,
                Ok(domain) => domain.issues().to_vec(),
            };
            assert_eq!(
                result
                    .iter()
                    .map(|issue| (issue.condition.as_str(), issue.spec_paths[0].as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        "correlated_filter_with_unique_verification",
                        "intermediates[0].verifications[0].unique"
                    ),
                    (
                        "correlated_derivation_with_unique_verification",
                        "intermediates[0].verifications[0].unique"
                    )
                ]
            );
        }

        #[test]
        fn an_earlier_selector_key_uses_the_later_donor_current_record_owner() {
            let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            for key in ["DONOR.KEY", "{source: DONOR.KEY}"] {
                let spec = spec.replace("key: {ID: DONOR.KEY}", &format!("key: {{ID: {key}}}"));
                // Different field names and reversed donor order expose owner/coordinate aliasing.
                let (fixture, result) =
                    independent(&spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
                let mut result = result.unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join("out.csv")).unwrap(),
                    b"ID,VALUE\n1,100\n2,200\n"
                );
            }
        }

        #[test]
        fn retained_foreign_key_is_valid_until_it_is_read_in_the_wrong_current_scope() {
            let declaration = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let (fixture, result) = independent(declaration, "ID\n1\n", Some("KEY\n2\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("out.csv")).unwrap(), b"ID\n1\n");
            let used = declaration.replace("columns: [ID]", "columns: [ID, VALUE]")
                + "  - {name: VALUE, type: int, derivation: P.ID}\n";
            let (fixture, result) = independent(&used, "ID\n1\n", Some("KEY\n2\n"));
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![
                    yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"LOOKUP-5","spec_paths":["intermediates[0].key"],"context":{"identifier":"DONOR.KEY","intermediate":"P"}})
                ]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn a_foreign_row_driver_owns_its_qualified_selector_keys() {
            let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: P\n    dataset: SRC\n    key: {ID: DONOR.KEY}\ncolumns:\n  - {name: ID, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: donor\n    dataset: DONOR\n    derivations: {ID: DONOR.KEY, VALUE: P.VAL}\n";
            let (fixture, result) =
                independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n2,200\n1,100\n"
            );
        }

        #[test]
        fn an_earlier_selectors_filter_uses_the_reading_donor_current_fields() {
            let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int, MIN: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.KEY}\n    filter: 'SRC.VAL > DONOR.MIN'\n    no_match: -1\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations: {LOOKED_UP: EARLIER.VAL}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let (fixture, result) = independent(
                spec,
                "ID,VAL\n1,100\n2,200\n",
                Some("KEY,MIN\n2,50\n1,150\n"),
            );
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n1,-1\n2,200\n"
            );
        }

        #[test]
        fn named_match_keys_bind_an_earlier_derived_field_of_the_reading_donor() {
            let spec = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let (fixture, result) =
                independent(spec, "ID,VAL\n1,100\n2,200\n", Some("KEY\n2\n1\n"));
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,VALUE\n1,100\n2,200\n"
            );
        }

        #[test]
        fn contextual_keys_reject_later_unknown_wrong_owner_and_known_type_at_binding() {
            let base = "schema_version: '1.0'\ndomain: OUT\nkeys: [ID]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}, DONOR: {path: donor.csv, types: {KEY: int}}}\noutput: {path: out.csv, columns: [ID, VALUE]}\nintermediates:\n  - id: EARLIER\n    dataset: SRC\n    key: {ID: DONOR.NEWKEY}\n  - id: LATER\n    dataset: DONOR\n    key: {KEY: ID}\n    derivations:\n      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: LATER.LOOKED_UP}\n";
            let unknown = |name: &str| yamaa_adapters::json!({"phase":"validation","condition":"unknown_field","requirement":"REQ-1263","spec_paths":["intermediates[1].derivations.LOOKED_UP.source"],"context":{"identifier":name,"intermediate":"LATER","read":"EARLIER"}});
            for (spec, first) in [
                (
                    base.replace(
                        "      NEWKEY: KEY\n      LOOKED_UP: EARLIER.VAL",
                        "      LOOKED_UP: EARLIER.VAL\n      NEWKEY: KEY",
                    ),
                    unknown("DONOR.NEWKEY"),
                ),
                (
                    base.replace("ID: DONOR.NEWKEY", "ID: DONOR.ABSENT"),
                    unknown("DONOR.ABSENT"),
                ),
                (
                    base.replace("ID: DONOR.NEWKEY", "ID: WRONG.NEWKEY"),
                    unknown("WRONG.NEWKEY"),
                ),
                (
                    base.replace("NEWKEY: KEY", "NEWKEY: {literal: wrong-type}"),
                    yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"LOOKUP-9","spec_paths":["intermediates[0].key"],"context":{"intermediate":"EARLIER","source":"DONOR.NEWKEY","expected":"str","actual":"int"}}),
                ),
            ] {
                let (fixture, result) = independent(&spec, "ID,VAL\n1,100\n", Some("KEY\n2\n"));
                let mut result = result.unwrap();
                assert_eq!(
                    result.issues().iter().map(issue).collect::<Vec<_>>(),
                    vec![first],
                    "{spec}"
                );
                assert!(result.save().is_err());
                assert!(!fixture.0.join("out.csv").exists());
            }
        }

        #[test]
        fn self_derivations_read_completed_rows_and_publish_each_template_snapshot() {
            let spec = self_derivation_spec();
            let (fixture, result) = independent(spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,STAGE,VALUE\n1,1,10\n1,2,21\n1,3,43\n"
            );
        }

        fn self_derivation_spec() -> &'static str {
            "schema_version: '1.0'\ndomain: OUT\nkeys: [ID, STAGE]\nbase: SRC\ninput: {SRC: {path: source.csv, types: {ID: int, VAL: int}}}\noutput: {path: out.csv, columns: [ID, STAGE, VALUE]}\nintermediates:\n  - id: P\n    dataset: SELF\n    key: [ID]\n    derivations:\n      TWICE: {compute: {expr: 'SELF.VALUE * 2'}}\n      PLUS: {compute: {expr: 'TWICE + 1'}}\n    order_by: [SELF.STAGE]\n    keep: last\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: STAGE, type: int}\n  - {name: VALUE, type: int}\nrows:\n  - id: first\n    derivations: {STAGE: {literal: 1}, VALUE: SRC.VAL}\n  - id: second\n    derivations: {STAGE: {literal: 2}, VALUE: P.PLUS}\n  - id: third\n    derivations: {STAGE: {literal: 3}, VALUE: P.PLUS}\n"
        }

        #[test]
        fn self_derivations_reject_later_and_unknown_fields() {
            for reference in ["SELF.PLUS", "PLUS", "SELF.ABSENT", "ABSENT"] {
                let spec = self_derivation_spec().replace("SELF.VALUE * 2", reference);
                let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
                let mut result = result.unwrap();
                assert_eq!(
                    result.issues().iter().map(issue).collect::<Vec<_>>(),
                    vec![yamaa_adapters::json!({
                        "phase":"validation", "condition":"unknown_field", "requirement":"REQ-1185",
                        "spec_paths":["intermediates[0].derivations.TWICE.compute.expr"],
                        "context":{"intermediate":"P", "identifier":reference}
                    })],
                    "{reference}"
                );
                assert!(result.save().is_err());
                assert!(!fixture.0.join("out.csv").exists());
            }
        }
        #[test]
        fn self_derivations_cannot_shadow_completed_stored_fields() {
            let spec = self_derivation_spec()
                .replace("TWICE:", "VALUE:")
                .replace("TWICE + 1", "VALUE + 1");
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase":"validation", "condition":"duplicate_derivation", "requirement":"REQ-1185",
                    "spec_paths":["intermediates[0].derivations.VALUE"],
                    "context":{"intermediate":"P", "derivation":"VALUE", "identifier":"SELF.VALUE"}
                })]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn self_derived_unique_checks_are_eager_after_a_completed_template() {
            let spec = self_derivation_spec()
                .replace(
                    "    key: [ID]",
                    "    key: [ID]\n    verifications: [{unique: [TWICE]}]",
                )
                .replace(
                    "VALUE: P.PLUS",
                    "VALUE: {compute: {expr: '9223372036854775807 + 1'}}",
                );
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n2,10\n", None);
            let mut result = result.unwrap();
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase":"verification", "condition":"duplicate_intermediate_records", "requirement":"REQ-1245",
                    "spec_paths":["intermediates[0].verifications[0].unique"],
                    "context":{"intermediate":"P", "dataset":"SELF", "columns":["TWICE"], "duplicate_count":1}
                })]
            );
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());
        }
        #[test]
        fn a_first_template_cannot_read_a_self_derived_field_from_future_rows() {
            let spec = self_derivation_spec().replace("VALUE: SRC.VAL", "VALUE: P.PLUS");
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let issues = result
                .err()
                .expect("the first template is refused at preparation");
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "phase_boundary");
            assert_eq!(issues[0].spec_paths, ["rows[0].derivations.VALUE.source"]);
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn a_self_donor_derived_key_can_read_an_input_backed_selector() {
            let spec = self_derivation_spec()
                .replace(
                    "  - id: P",
                    "  - id: INPUT\n    dataset: SRC\n    key: {ID: SELF.NEWKEY}\n  - id: P",
                )
                .replace(
                    "      TWICE: {compute: {expr: 'SELF.VALUE * 2'}}",
                    "      NEWKEY: ID\n      TWICE: {compute: {expr: 'INPUT.VAL * 2'}}",
                );
            let (fixture, result) = independent(&spec, "ID,VAL\n1,10\n", None);
            let mut result = result.unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("out.csv")).unwrap(),
                b"ID,STAGE,VALUE\n1,1,10\n1,2,21\n1,3,21\n"
            );
        }

        fn between_spec(intermediates: &str, x: &str, kind: &str, donor_time: bool) -> String {
            let dm = if donor_time {
                "{path: dm.csv, types: {ADTM: datetime}}"
            } else {
                "dm.csv"
            };
            format!("schema_version: '1.0'\ndomain: ADPC\nkeys: [USUBJID, PCSEQ]\nbase: PC\ninput:\n  PC: {{path: pc.csv, types: {{PCSEQ: int, PCDTC: datetime}}}}\n  EX: {{path: ex.csv, types: {{EXSTDTC: datetime}}}}\n  DM: {dm}\nintermediates:\n{intermediates}columns:\n  - {{name: USUBJID, type: str, derivation: PC.USUBJID}}\n  - {{name: PCSEQ, type: int, derivation: PC.PCSEQ}}\n  - {{name: ADTM, type: datetime, derivation: PC.PCDTC}}\n  - {{name: LDOSEDTM, type: datetime, derivation: LAST_DOSE.EXSTDTC}}\n  - {{name: X, type: {kind}, derivation: {x}}}\noutput: {{path: out.csv, columns: [USUBJID, PCSEQ, ADTM, LDOSEDTM, X]}}\n")
        }
        fn between_fixture(spec: &str, donor_time: bool) -> Fixture {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("spec.yaml"), spec).unwrap();
            fs::write(fixture.0.join("pc.csv"), "USUBJID,PCSEQ,PCDTC\nS1,1,2030-01-01T09:00:00\nS1,2,2030-01-02T09:00:00\nS2,1,2030-01-01T10:00:00\n").unwrap();
            fs::write(fixture.0.join("ex.csv"), "USUBJID,EXTRT,EXSTDTC\nS1,DRUG A,2030-01-01T07:00:00\nS1,DRUG B,2030-01-02T07:00:00\nS2,DRUG A,2030-01-01T07:30:00\n").unwrap();
            let dm = if donor_time {
                "USUBJID,SEX,ADTM\nS1,F,2030-01-02T09:00:00\nS2,M,2030-01-01T10:00:00\n"
            } else {
                "USUBJID,SEX\nS1,F\nS2,M\n"
            };
            fs::write(fixture.0.join("dm.csv"), dm).unwrap();
            fixture
        }
        fn between_run(fixture: &Fixture) -> file_application::Domain {
            let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
            file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "between-donor-scope",
                    specification: &path,
                    base_directory: ".",
                },
            )
            .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
        }
        const LAST_DOSE: &str = "  - id: LAST_DOSE\n    dataset: EX\n    key: [USUBJID]\n    between: {value: ADTM, lower: EXSTDTC}\n    order_by: [EX.EXSTDTC]\n    keep: last\n";
        const SUBJ: &str = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      SEXL: {str_case: {source: SEX, to: lower}}\n";
        const LAST_TRT: &str = "  - id: LAST_TRT\n    dataset: EX\n    key: [USUBJID]\n    between: {value: ADTM, lower: EXSTDTC}\n    derivations:\n      TRT: EX.EXTRT\n    order_by: [EX.EXSTDTC]\n    keep: last\n";

        #[test]
        fn unrelated_donor_derivations_do_not_rebase_earlier_between_values() {
            for (intermediates, x, expected) in [
                (format!("{LAST_DOSE}{SUBJ}"), "SUBJ.SEXL", ["f", "f", "m"]),
                (format!("{SUBJ}{LAST_DOSE}"), "SUBJ.SEXL", ["f", "f", "m"]),
                (
                    format!(
                        "{}{SUBJ}",
                        LAST_DOSE.replace("value: ADTM", "value: PC.PCDTC")
                    ),
                    "SUBJ.SEXL",
                    ["f", "f", "m"],
                ),
                (
                    format!("{LAST_DOSE}  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n"),
                    "SUBJ.SEX",
                    ["F", "F", "M"],
                ),
                (
                    format!("{LAST_DOSE}{LAST_TRT}"),
                    "LAST_TRT.TRT",
                    ["DRUG A", "DRUG B", "DRUG A"],
                ),
                (
                    format!("{LAST_TRT}{LAST_DOSE}"),
                    "LAST_TRT.TRT",
                    ["DRUG A", "DRUG B", "DRUG A"],
                ),
            ] {
                let spec = between_spec(&intermediates, x, "str", false);
                let fixture = between_fixture(&spec, false);
                let mut result = between_run(&fixture);
                assert!(result.issues().is_empty(), "{spec}: {:?}", result.issues());
                assert!(result.save().unwrap());
                let expected = format!("USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,{}\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,{}\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,{}\n", expected[0], expected[1], expected[2]);
                assert_eq!(
                    fs::read_to_string(fixture.0.join("out.csv")).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn a_read_of_between_requires_its_value_in_the_reading_donor_scope() {
            let reader = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      LD: LAST_DOSE.EXSTDTC\n";
            let spec = between_spec(
                &format!("{LAST_DOSE}{reader}"),
                "SUBJ.LD",
                "datetime",
                false,
            );
            let fixture = between_fixture(&spec, false);
            let mut result = between_run(&fixture);
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-1263",
                    "spec_paths": ["intermediates[1].derivations.LD.source"],
                    "context": {"intermediate": "SUBJ", "identifier": "ADTM", "read": "LAST_DOSE"}
                })]
            );
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            assert!(!fixture.0.join("out.csv").exists());

            let spec = between_spec(&format!("{LAST_DOSE}{reader}"), "SUBJ.LD", "datetime", true);
            let fixture = between_fixture(&spec, true);
            let mut result = between_run(&fixture);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("out.csv")).unwrap(), "USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,2030-01-02T07:00:00\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,2030-01-02T07:00:00\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,2030-01-01T07:30:00\n");
        }

        #[test]
        fn rebasing_retains_transitive_selector_dependencies_and_missing_names() {
            let second = "  - id: SECOND\n    dataset: EX\n    key: [USUBJID]\n    between: {value: LAST_DOSE.EXSTDTC, lower: EXSTDTC}\n    order_by: [EX.EXSTDTC]\n    keep: last\n";
            let reader = "  - id: SUBJ\n    dataset: DM\n    key: [USUBJID]\n    derivations:\n      LD: SECOND.EXSTDTC\n";
            let declarations = format!("{LAST_DOSE}{second}{reader}");
            let spec = between_spec(&declarations, "SUBJ.LD", "datetime", false);
            let fixture = between_fixture(&spec, false);
            let result = between_run(&fixture);
            assert_eq!(
                result.issues().iter().map(issue).collect::<Vec<_>>(),
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-1263",
                    "spec_paths": ["intermediates[2].derivations.LD.source"],
                    "context": {"intermediate": "SUBJ", "identifier": "ADTM", "read": "SECOND"}
                })]
            );
            let spec = between_spec(&declarations, "SUBJ.LD", "datetime", true);
            let fixture = between_fixture(&spec, true);
            let mut result = between_run(&fixture);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read_to_string(fixture.0.join("out.csv")).unwrap(), "USUBJID,PCSEQ,ADTM,LDOSEDTM,X\nS1,1,2030-01-01T09:00:00,2030-01-01T07:00:00,2030-01-02T07:00:00\nS1,2,2030-01-02T09:00:00,2030-01-02T07:00:00,2030-01-02T07:00:00\nS2,1,2030-01-01T10:00:00,2030-01-01T07:30:00,2030-01-01T07:30:00\n");
        }

        /// A `between.value` qualified by an input reads that input's record
        /// only where it drives the reading row or donor record. Elsewhere it
        /// names no current-row value, however the intermediate is reached.
        #[test]
        fn a_qualified_between_value_reads_only_its_own_driver_record() {
            const WINDOW: &str = "  - id: W\n    dataset: WIN\n    key: [USUBJID]\n    between: {value: REF.REFDAY, lower: LO, upper: HI}\n    order_by: [WIN.LO]\n    keep: first\n    no_match: null\n";
            let run = |body: &str| {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: OUT\ninput:\n  SUBJ: {{path: subj.csv, types: {{SUBJDAY: int}}}}\n  REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n  WIN: {{path: win.csv, types: {{LO: int, HI: int}}}}\n  EVT: {{path: evt.csv, types: {{EVDAY: int}}}}\n{body}")).unwrap();
                // REF orders subjects the other way and keeps its day in the
                // column where SUBJ keeps a different day.
                fs::write(fixture.0.join("subj.csv"), "USUBJID,SUBJDAY\nS1,5\nS2,50\n").unwrap();
                fs::write(fixture.0.join("ref.csv"), "USUBJID,REFDAY\nS2,5\nS1,50\n").unwrap();
                fs::write(fixture.0.join("win.csv"), "USUBJID,NAME,LO,HI\nS1,EARLY,1,10\nS1,LATE,11,100\nS2,EARLY,1,10\nS2,LATE,11,100\n").unwrap();
                fs::write(fixture.0.join("evt.csv"), "USUBJID,EVDAY\nS1,3\nS2,3\n").unwrap();
                let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let mut result = match file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "between-current-driver",
                        specification: &path,
                        base_directory: ".",
                    },
                ) {
                    Ok(result) => result,
                    Err(failure) => return Err(failure.iter().map(issue).collect::<Vec<_>>()),
                };
                let issues = result.issues().iter().map(issue).collect::<Vec<_>>();
                if !issues.is_empty() {
                    assert!(result.save().is_err());
                    return Err(issues);
                }
                assert!(result.save().unwrap());
                Ok(fs::read_to_string(fixture.0.join("out.csv")).unwrap())
            };
            let base = "keys: [USUBJID]\nbase: SUBJ\noutput: {path: out.csv, columns: [USUBJID, WINDOW]}\nintermediates:\n";
            let column = |derivation: &str| {
                format!("columns:\n  - {{name: USUBJID, type: str, derivation: SUBJ.USUBJID}}\n  - {{name: WINDOW, type: str, derivation: {derivation}}}\n")
            };
            // The base driver's rows name no REF record: read directly, through
            // another intermediate's value, from an aggregate's derive step, or
            // through an aggregate's range value.
            let direct = format!("{base}{WINDOW}{}", column("W.NAME"));
            let chained = WINDOW.replace("id: W\n", "id: W2\n")
                + &WINDOW.replace("value: REF.REFDAY", "value: W2.LO");
            let chained = format!("{base}{chained}{}", column("W.NAME"));
            let aggregated = |derivation: &str| {
                format!("{base}{WINDOW}{}", column(derivation)).replace(
                    "type: str, derivation: {aggregate",
                    "type: int, derivation: {aggregate",
                )
            };
            let derived = aggregated("{aggregate: {key: [USUBJID], derive: [{name: DAY, type: int, derivation: EVT.EVDAY}, {name: LOW, type: int, derivation: W.LO}], expr: 'MAX(LOW)'}}");
            let ranged = aggregated(
                "{aggregate: {between: {value: W.LO, lower: EVDAY}, expr: 'COUNT(EVT.EVDAY)'}}",
            );
            let refused = |intermediate: &str| {
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-0121",
                    "spec_paths": ["intermediates[0].between.value"],
                    "context": {"intermediate": intermediate, "identifier": "REF.REFDAY"}
                })])
            };
            assert_eq!(run(&direct), refused("W"));
            assert_eq!(run(&chained), refused("W2"));
            assert_eq!(run(&derived), refused("W"));
            assert_eq!(run(&ranged), refused("W"));
            // A donor record of REF supplies the value under REQ-1263.
            let reader = "  - id: R\n    dataset: REF\n    key: [USUBJID]\n    derivations: {WINNAME: W.NAME}\n";
            assert_eq!(
                run(&format!("{base}{WINDOW}{reader}{}", column("R.WINNAME"))),
                Ok("USUBJID,WINDOW\nS1,LATE\nS2,EARLY\n".into())
            );
            // So does REF's record in the row template it drives, read by the
            // range or by a correlated filter.
            let rows = "keys: [USUBJID, ORIGIN]\noutput: {path: out.csv, columns: [USUBJID, ORIGIN, WINDOW]}\ncolumns:\n  - {name: USUBJID, type: str}\n  - {name: ORIGIN, type: str}\n  - {name: WINDOW, type: str}\nrows:\n  - id: subj\n    dataset: SUBJ\n    derivations: {USUBJID: SUBJ.USUBJID, ORIGIN: {literal: SUBJ}, WINDOW: {literal: NONE}}\n  - id: ref\n    dataset: REF\n    derivations: {USUBJID: REF.USUBJID, ORIGIN: {literal: REF}, WINDOW: W.NAME}\nintermediates:\n";
            let filtered = WINDOW.replace(
                "between: {value: REF.REFDAY, lower: LO, upper: HI}",
                "filter: 'WIN.LO <= REF.REFDAY AND REF.REFDAY <= WIN.HI'",
            );
            for window in [WINDOW.to_owned(), filtered] {
                assert_eq!(
                    run(&format!("{rows}{window}")),
                    Ok("USUBJID,ORIGIN,WINDOW\nS1,SUBJ,NONE\nS2,SUBJ,NONE\nS2,REF,EARLY\nS1,REF,LATE\n".into())
                );
            }
        }

        /// An aggregate's `between.value` qualified by an input reads that
        /// input's record only where it drives every row the column derives.
        /// `check` refuses any other input from the declarations alone.
        #[test]
        fn an_aggregate_between_value_reads_only_its_own_driver_record() {
            const WINDOW: &str = "intermediates:\n  - id: W\n    dataset: WIN\n    key: [USUBJID]\n    between: {value: REF.REFDAY, lower: LO, upper: HI}\n    order_by: [WIN.LO]\n    keep: first\n    no_match: null\n";
            // The check issues, and the domain issues or the saved artifact.
            let run = |body: &str| {
                let fixture = Fixture::new();
                fs::write(fixture.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: OUT\ninput:\n  SUBJ: {{path: subj.csv, types: {{SUBJDAY: int}}}}\n  REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n  WIN: {{path: win.csv, types: {{LO: int, HI: int}}}}\n  EVT: {{path: evt.csv, types: {{EVDAY: int}}}}\n{body}")).unwrap();
                // REF orders subjects the other way and keeps its day in the
                // column where SUBJ keeps a different day.
                fs::write(fixture.0.join("subj.csv"), "USUBJID,SUBJDAY\nS1,5\nS2,50\n").unwrap();
                fs::write(fixture.0.join("ref.csv"), "USUBJID,REFDAY\nS2,5\nS1,50\n").unwrap();
                fs::write(fixture.0.join("win.csv"), "USUBJID,NAME,LO,HI\nS1,EARLY,1,10\nS1,LATE,11,100\nS2,EARLY,1,10\nS2,LATE,11,100\n").unwrap();
                fs::write(
                    fixture.0.join("evt.csv"),
                    "USUBJID,EVDAY\nS1,3\nS1,10\nS1,60\nS2,3\nS2,10\nS2,60\n",
                )
                .unwrap();
                let path = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let request = || Request {
                    specification: &path,
                    environment: None,
                };
                let identity = || Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "aggregate-between-current-driver",
                    specification: &path,
                    base_directory: ".",
                };
                let checked = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .iter()
                .map(issue)
                .collect::<Vec<_>>();
                let built = match file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(mut result) => {
                        let issues = result.issues().iter().map(issue).collect::<Vec<_>>();
                        if issues.is_empty() {
                            assert!(result.save().unwrap());
                            Ok(fs::read_to_string(fixture.0.join("out.csv")).unwrap())
                        } else {
                            assert!(result.save().is_err());
                            Err(issues)
                        }
                    }
                    Err(failure) => Err(failure.iter().map(issue).collect::<Vec<_>>()),
                };
                (checked, built)
            };
            // Events on or after the day the value names.
            let count = |value: &str| {
                format!("{{aggregate: {{between: {{value: {value}, upper: EVDAY}}, expr: 'COUNT(EVT.*)'}}}}")
            };
            let base = |derivation: &str| {
                format!("keys: [USUBJID]\nbase: SUBJ\noutput: {{path: out.csv, columns: [USUBJID, N]}}\ncolumns:\n  - {{name: USUBJID, type: str, derivation: SUBJ.USUBJID}}\n  - {{name: RDAY, type: int, derivation: REF.REFDAY}}\n  - {{name: N, type: int, derivation: {derivation}}}\n")
            };
            let refused_at = |path: &str, identifier: &str| {
                vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-0121",
                    "spec_paths": [path], "context": {"identifier": identifier}
                })]
            };
            let refused = |identifier: &str| {
                refused_at("columns.N.derivation.aggregate.between.value", identifier)
            };
            // The base driver's rows name no REF record, read directly or from
            // an alternative of a case.
            let alternative = format!(
                "{{case: [{{when: 'USUBJID IS NOT NULL', then: {}}}, {{otherwise: {{literal: null}}}}]}}",
                count("REF.REFDAY")
            );
            for (derivation, expected) in [
                (count("REF.REFDAY"), refused("REF.REFDAY")),
                (
                    alternative,
                    refused_at(
                        "columns.N.derivation.case[0].then.aggregate.between.value",
                        "REF.REFDAY",
                    ),
                ),
            ] {
                assert_eq!(
                    run(&base(&derivation)),
                    (expected.clone(), Err(expected)),
                    "{derivation}"
                );
            }
            // An output-row reduction narrows by no range at all: binding refuses
            // the range itself, and `check` does not misname its value.
            let (checked, built) = run(&base(
                "{aggregate: {group_by: [USUBJID], between: {value: REF.REFDAY, upper: RDAY}, expr: 'MAX(RDAY)'}}",
            ));
            assert_eq!(checked, Vec::<yamaa_core::json::Value>::new());
            assert_eq!(
                built,
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "invalid_aggregate_context", "requirement": "REQ-0329",
                    "spec_paths": ["columns.N.derivation.aggregate.between"],
                    "context": {"expr": "MAX(RDAY)", "reason": "between narrows a qualified right side only"}
                })])
            );
            // The base driver's own record supplies the value, and so does a
            // column built for the row, here REF's day joined on the subject.
            for (value, expected) in [("SUBJ.SUBJDAY", "S1,2\nS2,1\n"), ("RDAY", "S1,1\nS2,2\n")] {
                assert_eq!(
                    run(&base(&count(value))),
                    (Vec::new(), Ok(format!("USUBJID,N\n{expected}")))
                );
            }
            // A range value that reads a named intermediate runs that
            // intermediate for the same row, so its own value is refused there.
            assert_eq!(
                run(&format!("{WINDOW}{}", base(&count("W.LO")))).1,
                Err(vec![yamaa_adapters::json!({
                    "phase": "validation", "condition": "unknown_field", "requirement": "REQ-0121",
                    "spec_paths": ["intermediates[0].between.value"],
                    "context": {"intermediate": "W", "identifier": "REF.REFDAY"}
                })])
            );
            // A column-phase aggregate evaluates for the rows of every template:
            // with templates driven by SUBJ and by REF, neither input drives all
            // of them, and each refusal is reported once.
            let rows = |templates: &str, value: &str| {
                format!("keys: [USUBJID, ORIGIN]\noutput: {{path: out.csv, columns: [USUBJID, ORIGIN, N]}}\ncolumns:\n  - {{name: USUBJID, type: str}}\n  - {{name: ORIGIN, type: str}}\n  - {{name: N, type: int, derivation: {}}}\nrows:\n{templates}", count(value))
            };
            let subj = "  - id: subj\n    dataset: SUBJ\n    derivations: {USUBJID: SUBJ.USUBJID, ORIGIN: {literal: SUBJ}}\n";
            let both = format!("{subj}  - id: ref\n    dataset: REF\n    derivations: {{USUBJID: REF.USUBJID, ORIGIN: {{literal: REF}}}}\n");
            for value in ["SUBJ.SUBJDAY", "REF.REFDAY"] {
                assert_eq!(
                    run(&rows(&both, value)),
                    (refused(value), Err(refused(value))),
                    "{value}"
                );
            }
            assert_eq!(
                run(&rows(subj, "SUBJ.SUBJDAY")),
                (
                    Vec::new(),
                    Ok("USUBJID,ORIGIN,N\nS1,SUBJ,2\nS2,SUBJ,1\n".into())
                )
            );
        }
    }
    mod completed_lookup_assertions {
        //! Independent completed-row assertion scope, order and whole-record truth.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn issues(rows: &[Issue]) -> yamaa_core::json::Value {
            yamaa_core::json::Value::Array(
                rows.iter()
                    .map(|issue| {
                        let mut value = issue.as_value();
                        value["context"] = issue.context.clone();
                        value
                    })
                    .collect(),
            )
        }
        #[test]
        fn complete_independent_lookup_assertions_keep_declaring_sites_and_prior_records() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/completed_lookup_assertions.json"
            ))
            .unwrap();
            for (name, case) in truth["cases"].as_object().unwrap() {
                for _ in 0..2 {
                    let fixture = Fixture(std::env::temp_dir().join(format!(
                        "yamaa-completed-assert-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    )));
                    fs::create_dir(&fixture.0).unwrap();
                    let entry = fixture.0.join("spec.yaml");
                    fs::write(&entry, case["specification"].as_str().unwrap()).unwrap();
                    for (path, content) in case["inputs"].as_object().unwrap() {
                        fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
                    }
                    let written = entry.to_str().unwrap();
                    let identity = || Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "completed-assert",
                        specification: written,
                        base_directory: ".",
                    };
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{name}: check"
                    );
                    let mut result = file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    )
                    .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
                    assert_eq!(
                        issues(&result.issues()),
                        case["issues"],
                        "{name}: complete findings"
                    );
                    let report = result.observations();
                    assert_eq!(
                        report["handler_counts"], case["handlers"],
                        "{name}: declared handler ledger"
                    );
                    let mut expected = case["verifications"].clone();
                    for record in expected.as_array_mut().unwrap() {
                        record["specification"] = written.into();
                    }
                    assert_eq!(
                        report["verifications"], expected,
                        "{name}: complete ordered verification records"
                    );
                    assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
                    if let Some(csv) = case.get("csv") {
                        assert!(result.output().is_some(), "{name}");
                        assert!(result.save().unwrap(), "{name}");
                        assert_eq!(
                            fs::read(fixture.0.join("result.csv")).unwrap(),
                            csv.as_str().unwrap().as_bytes(),
                            "{name}: exact CSV"
                        );
                        assert!(result.save().unwrap());
                    } else {
                        assert!(result.output().is_none(), "{name}");
                        assert!(result.save().is_err(), "{name}");
                        assert!(!fixture.0.join("result.csv").exists());
                    }
                }
            }
        }
    }
    mod declarative_case_intermediates {
        //! Complete case observations use the shared scalar source binder.
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
                    "yamaa-case-intermediate-{}-{}",
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
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-case-intermediate",
                        specification,
                        base_directory: ".",
                    },
                )
            }
            fn populate(&self) {
                fs::write(self.0.join("src.csv"), "ID,SELECT\n1,Y\n2,N\n3,Y\n").unwrap();
                fs::write(
                    self.0.join("donor.csv"),
                    "ID,SEQ,A,B\n1,1,first,\n1,2,second,present\n2,1,hidden,hidden\n",
                )
                .unwrap();
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
keys: [ID]
input:
  SRC: {path: src.csv, types: {ID: int}}
  DONOR: {path: donor.csv, types: {ID: int, SEQ: int}}
output: {path: result.csv, columns: [ID, A, B]}
intermediates:
  - id: P
    dataset: DONOR
    order_by: [DONOR.SEQ]
    keep: first
    no_match: {literal: null}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: SELECT, type: str, derivation: SRC.SELECT}
  - name: A
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.A
        - otherwise: {literal: skip}
  - name: B
    type: str
    derivation:
      case:
        - when: "SELECT = 'Y'"
          then: P.B
        - otherwise: {literal: skip}
"#;
        #[test]
        fn selected_case_fields_share_kept_donors_and_unselected_branches_have_no_handlers() {
            let fixture = Fixture::new();
            fixture.populate();
            let mut result = fixture.run(BODY).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("result.csv")).unwrap(),
                b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n"
            );
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.A.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.A.derivation.case[0].then.source.no_match","handler":"no_match","count":1},
                    {"spec_path":"columns.B.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":1},
                    {"spec_path":"columns.B.derivation.case[0].then.source.no_match","handler":"no_match","count":1},
                ])
            );
        }
        #[test]
        fn unchanged_first_ae_capture_matches_full_csv_and_complete_reached_handlers() {
            let fixture = Fixture::new();
            let original = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/adam-adtte-first-ae");
            fs::create_dir(fixture.0.join("input")).unwrap();
            for name in ["adsl.csv", "adae.csv"] {
                fs::copy(
                    original.join("input").join(name),
                    fixture.0.join("input").join(name),
                )
                .unwrap();
            }
            let written = fs::read(original.join("spec.yaml")).unwrap();
            let mut result = fixture.run(std::str::from_utf8(&written).unwrap()).unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(fixture.0.join("adtte.csv")).unwrap(),
                fs::read(original.join("expected/adtte.csv")).unwrap()
            );
            assert_eq!(fs::read(original.join("spec.yaml")).unwrap(), written);
            assert_eq!(
                result.observations()["handler_counts"],
                yamaa_adapters::json!([
                    {"spec_path":"columns.AEDT.derivation.source.multiple_matches","handler":"multiple_matches","count":2},
                    {"spec_path":"columns.AEDT.derivation.source.no_match","handler":"no_match","count":3},
                    {"spec_path":"columns.SRCSEQ.derivation.case[0].then.source.multiple_matches","handler":"multiple_matches","count":2}
                ])
            );
        }
        #[test]
        fn nested_named_source_absence_handlers_are_lazy_and_preserve_existing_missing_values() {
            let fixture = Fixture::new();
            fixture.populate();
            let body = BODY.replace(
                "then: P.A",
                "then: {source: {variable: P.A, absent: fallback}}",
            );
            for (body, expected, count) in [
                (
                    body.clone(),
                    b"ID,A,B\n1,first,\n2,skip,skip\n3,,\n".as_slice(),
                    0,
                ),
                (
                    body.replace("variable: P.A,", "variable: P.ABSENT,"),
                    b"ID,A,B\n1,fallback,\n2,skip,skip\n3,fallback,\n".as_slice(),
                    2,
                ),
            ] {
                let mut result = fixture.run(&body).unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(fixture.0.join("result.csv")).unwrap(), expected);
                assert_eq!(
                    result.observations()["handler_counts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|entry| entry["spec_path"]
                            == "columns.A.derivation.case[0].then.source.absent")
                        .unwrap(),
                    &yamaa_adapters::json!({"spec_path":"columns.A.derivation.case[0].then.source.absent","handler":"absent","count":count})
                );
            }
        }
    }
}
