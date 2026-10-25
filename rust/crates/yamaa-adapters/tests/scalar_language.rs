#![cfg(feature = "yaml")]
//! Scalar contracts: scalar language, secondary scalar operands and run report.
mod scalar {
    mod scalar_language {
        // Every scalar operation a run saves, refuses or checks is a case of
        // `fixtures/diagnostics/scalar_operations.json`. The test left here
        // reads benchmark truth.
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
        /// Already data: four benchmarks' own `expected/error.yaml`, whose
        /// context each issue holds as a subset, the mixed ones run with no
        /// input directory.
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
                // An expected error.yaml compares its context as a subset:
                // the issue holds each key it writes, with its value, and
                // may record more.
                let actual = issues
                    .iter()
                    .map(|issue| {
                        let mut row = issue_json(issue);
                        let recorded = row["context"].as_object().cloned().unwrap_or_default();
                        let written = expected["context"].as_object().cloned().unwrap_or_default();
                        row["context"] = yamaa_core::json::Value::Object(
                            recorded
                                .into_iter()
                                .filter(|(key, _)| written.contains_key(key))
                                .collect(),
                        );
                        row
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, vec![expected], "{name}");
            }
        }
    }
    mod secondary_scalar_operands {
        //! Implicit joins bind completed keys, preserve laziness and keep source scopes.
        //! Every case these tests do not keep is in
        //! `fixtures/diagnostics/secondary_operands.json`.
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
                crate::support::study::domain(
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
        /// Already data: three benchmarks' own `expected/` CSV bytes, each
        /// reproduced from a copy of its benchmark.
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
        /// Seam: every issue of the refusal is a validation issue, among
        /// issues no test authored; a case states rows found among the
        /// issues (`among`), not a property of every issue.
        #[test]
        fn forbidden_foreign_numeric_identifier_is_classified_before_donor_evaluation() {
            let fixture = Fixture::new();
            fs::write(fixture.0.join("a.csv"), b"ID,TEXT\na,valid\n").unwrap();
            fs::write(fixture.0.join("b.csv"), b"ID,N\na,1\n").unwrap();
            fs::write(fixture.0.join("spec.yaml"),"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {A: a.csv, B: {path: b.csv, key: [ID], types: {N: int}}}\nbase: A\noutput: {path: result.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: str, derivation: A.ID}\n  - {name: VALUE, type: int}\nrows:\n  - id: alpha\n    dataset: A\n    derivations:\n      VALUE: 'B.N + 1'\n").unwrap();
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
                        && issue.spec_paths == ["rows.alpha.derivations.VALUE"]),
                "{issues:?}"
            );
            assert!(!fixture.0.join("result.csv").exists());
            let original = fs::read_to_string(fixture.0.join("spec.yaml")).unwrap();
            for (operation, path) in [
                (
                    "{case: [{when: 'B.N = 1', then: 1}, {otherwise: 0}]}",
                    "rows.alpha.derivations.VALUE.case[0].when",
                ),
                (
                    "{flag: {condition: 'B.N = 1', true_value: 1, false_value: 0}}",
                    "rows.alpha.derivations.VALUE.flag.condition",
                ),
            ] {
                fs::write(
                    fixture.0.join("spec.yaml"),
                    original.edit("'B.N + 1'", operation),
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

        /// Seam: three handler counts found among the run's whole ledger
        /// (`observations()["handler_counts"]`), whose other entries no test
        /// authored, while a case states a report kind's rows whole.
        ///
        /// B holds no record with SEQ 99, so ABSENT's filtered read finds no
        /// record for a or c, and its `absent` answers both (NAME-3, D67).
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
                r#"yamaa: '1.0'
type: spec
domain: TEST
keys: [ID]
input: {A: a.csv, B: {path: b.csv, key: [ID], types: {SEQ: int}}}
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
          - otherwise: "'SKIP'"
      FIRST:
        case:
          - when: "A.USE = 'Y'"
            then:
              first_available:
                sources:
                  - {variable: B.TEXT, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                  - {variable: A.FALLBACK, filter: "A.USE = 'Y'"}
                missing: NONE
          - otherwise: "'SKIP'"
      MAP:
        case:
          - when: "A.USE = 'Y'"
            then:
              mapping:
                source: {variable: B.NUM, filter: "B.USE = 'Y' AND B.SEQ = 10"}
                dict: {'1': 10, '2': 20}
                missing: 0
          - otherwise: 0
      ABSENT:
        case:
          - when: "A.USE = 'Y'"
            then: {source: {variable: B.TEXT, filter: "B.SEQ = 99", absent: EDGE}}
          - otherwise: "'SKIP'"
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
                    "rows.records.derivations.TEXT.case[0].then.source.multiple_matches",
                    "multiple_matches",
                    1,
                ),
                (
                    "rows.records.derivations.MAP.case[0].then.mapping.missing",
                    "missing",
                    1,
                ),
                (
                    "rows.records.derivations.ABSENT.case[0].then.source.absent",
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
            let case = "TEXT:\n        case:\n          - when: \"A.USE = 'Y'\"\n            then: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}\n          - otherwise: \"'SKIP'\"";
            fs::write(fixture.0.join("spec.yaml"), original.replace(case, "TEXT: {source: {variable: B.TEXT, filter: \"B.USE = 'Y'\", order_by: [B.SEQ], keep: last}}" )).unwrap();
            assert_eq!(fixture.saved(),b"ID,TEXT,FIRST,MAP,ABSENT\na,late,late,20,EDGE\nb,hidden2,SKIP,0,SKIP\nc,,fallback,0,EDGE\n");
        }
    }
    mod run_report {
        // Every report a run builds or saves, and what a save publishes, is a
        // case of `fixtures/diagnostics/run_report.json`. These tests keep
        // the seams a case cannot state: what a save returns and records, and
        // a save repeated on one held result.
        use crate::support::Edit as _;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn benchmark() -> (Self, String) {
                let path = crate::support::Scratch::new("report-witness");
                fs::create_dir_all(path.join("input")).unwrap();
                let study = Self(path.resolved(std::fs::canonicalize));
                let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks/schema-verification-log");
                fs::copy(source.join("input/dm.csv"), study.0.join("input/dm.csv")).unwrap();
                let raw = fs::read_to_string(source.join("spec.yaml")).unwrap();
                (study, raw)
            }
            fn build(&self, raw: &str) -> file_application::Domain {
                fs::write(self.0.join("spec.yaml"), raw).unwrap();
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("report", &path),
                )
                .unwrap()
            }
        }
        const HEADER: &str = "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,COUNT,TOTAL,KEYS,DETAILS\n";
        /// Seam: the observation record a save returns, its `artifacts` and
        /// the whole record against `fixtures/specs/schema-verification-log.json`,
        /// and repeated saves of one held result after its input and its
        /// specification are gone, which no case states: the replay saves
        /// once and compares the files written.
        #[test]
        fn original_report_benchmark_matches_the_unchanged_goldens_without_build_writes() {
            let (study, raw) = Study::benchmark();
            let mut result = study.build(&raw);
            assert!(result.issues().is_empty());
            assert!(result.output().is_some());
            // One run, input and base row, two column rows and three check rows.
            assert_eq!(result.run_report().rows(), 8);
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
        /// Seam: a save the whole-target preflight refuses returns `Ok(false)`
        /// and writes nothing, and the same held result saves once the
        /// missing directory exists; the replay saves a result once.
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
            assert!(fs::read_to_string(study.0.join("adsl-checks.csv"))
                .unwrap()
                .starts_with(&format!("{HEADER}absent/adsl.csv,run,output,")));
        }
        /// Seam: a Parquet report's bytes, its integer column types and the
        /// governed records its observation holds, which a case's `saved`
        /// text cannot state.
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
            let table = result.run_report();
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
