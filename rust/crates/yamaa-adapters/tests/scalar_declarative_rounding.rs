#![cfg(feature = "yaml")]
//! Scalar contracts.
#[allow(dead_code)]
mod support;
mod scalar {
    mod declarative_rounding {
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::json;
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        const POSITIVE: &str =
            include_str!("../../../../benchmarks/schema-round-half-away-from-zero-ties/spec.yaml");
        const INPUT: &[u8] = include_bytes!(
            "../../../../benchmarks/schema-round-half-away-from-zero-ties/input/adsl.csv"
        );
        const EXPECTED: &str = include_str!(
            "../../../../benchmarks/schema-round-half-away-from-zero-ties/expected/adsl.csv"
        );
        const NEGATIVE: &str = include_str!(
    "../../../../benchmarks/negative-round-half-away-from-zero-non-numeric-source/spec.yaml"
);
        struct Study(PathBuf);
        impl Study {
            fn new(source: &str, input: bool) -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-rounding-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                fs::write(path.join("spec.yaml"), source).unwrap();
                if input {
                    fs::create_dir(path.join("input")).unwrap();
                    fs::write(path.join("input/adsl.csv"), INPUT).unwrap();
                }
                Self(fs::canonicalize(path).unwrap())
            }
            fn path(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().into()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn identity(path: &str) -> Identity<'_> {
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification: path,
                base_directory: ".",
            }
        }
        #[test]
        fn original_rounding_yaml_reaches_the_public_lifecycle_and_independent_csv_truth() {
            let study = Study::new(POSITIVE, false);
            let path = study.path();
            let request = Request {
                specification: &path,
                environment: None,
            };
            assert!(file_application::check(request, identity(&path)).is_empty());
            fs::create_dir(study.0.join("input")).unwrap();
            fs::write(study.0.join("input/adsl.csv"), INPUT).unwrap();
            let result = file_application::domain(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_expected(&result);
            assert!(!study.0.join("adsl.csv").exists());
        }
        #[test]
        fn original_nonnumeric_source_rejects_before_missing_study_data_and_has_no_formula_context()
        {
            let study = Study::new(NEGATIVE, false);
            let path = study.path();
            let request = Request {
                specification: &path,
                environment: None,
            };
            let findings = file_application::check(request, identity(&path));
            assert_eq!(findings.len(), 1, "{findings:?}");
            let finding = &findings[0];
            assert_eq!(finding.phase, "validation");
            assert_eq!(finding.condition, "incompatible_input_type");
            assert_eq!(finding.requirement.as_deref(), Some("REQ-0418"));
            assert_eq!(
                finding.spec_paths,
                ["columns.RNDVAL.derivation.round_half_away_from_zero.source"]
            );
            assert_eq!(
                yamaa_adapters::json_wire::decode(&finding.context).unwrap(),
                json!({"source":"SEX", "expected":"numeric", "actual":"str"})
            );
            assert_eq!(
                file_application::domain(
                    Request {
                        specification: &path,
                        environment: None
                    },
                    identity(&path)
                )
                .err()
                .unwrap(),
                findings
            );
            assert!(!study.0.join("input").exists());
        }
        #[test]
        fn malformed_digits_precede_unknown_source_and_missing_study_data() {
            let source = NEGATIVE
                .replace("source: SEX", "source: ABSENT")
                .replace("digits: 1", "digits: true");
            let study = Study::new(&source, false);
            let path = study.path();
            let findings = file_application::check(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            );
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].phase, "validation");
            assert_eq!(findings[0].condition, "invalid_field_type");
            assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0287"));
            assert_eq!(
                findings[0].spec_paths,
                ["columns.RNDVAL.derivation.round_half_away_from_zero.digits"]
            );
            assert!(!study.0.join("input").exists());
        }

        #[test]
        fn undeclared_rounding_relation_is_classified_with_actual_source_schema() {
            let study = Study::new(
                &POSITIVE.replace("source: VAL1", "source: OTHER.VAL1"),
                true,
            );
            let path = study.path();
            let findings = file_application::domain(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .unwrap()
            .issues()
            .to_vec();
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].condition, "unknown_field");
            assert_eq!(
                findings[0].spec_paths,
                ["columns.RND1.derivation.round_half_away_from_zero.source"]
            );
            assert_eq!(findings[0].context, "{\"identifier\":\"OTHER.VAL1\"}");
            assert!(!study.0.join("adsl.csv").exists());
        }

        fn assert_expected(result: &file_application::Domain) {
            let table = result.output_table().unwrap();
            let mut lines = EXPECTED.lines();
            let names = lines.next().unwrap().split(',').collect::<Vec<_>>();
            let rows = lines
                .map(|line| line.split(',').collect::<Vec<_>>())
                .collect::<Vec<_>>();
            assert_eq!(table.rows(), rows.len());
            assert_eq!(table.columns().len(), names.len());
            for (column, field) in table.columns().iter().enumerate() {
                assert_eq!(field.name(), names[column]);
                match field.data_type() {
                    arrow_schema::DataType::Utf8 => assert_eq!(
                        table
                            .batches()
                            .iter()
                            .flat_map(|batch| batch
                                .column(column)
                                .as_any()
                                .downcast_ref::<arrow_array::StringArray>()
                                .unwrap()
                                .iter()
                                .map(|value| value.map(str::to_owned)))
                            .collect::<Vec<_>>(),
                        rows.iter()
                            .map(|row| (!row[column].is_empty()).then(|| row[column].to_owned()))
                            .collect::<Vec<_>>()
                    ),
                    arrow_schema::DataType::Float64 => assert_eq!(
                        table
                            .batches()
                            .iter()
                            .flat_map(|batch| batch
                                .column(column)
                                .as_any()
                                .downcast_ref::<arrow_array::Float64Array>()
                                .unwrap()
                                .iter())
                            .collect::<Vec<_>>(),
                        rows.iter()
                            .map(|row| (!row[column].is_empty())
                                .then(|| row[column].parse::<f64>().unwrap()))
                            .collect::<Vec<_>>()
                    ),
                    _ => panic!("unexpected public type for {}", field.name()),
                }
            }
        }
        fn qualified(source: &str, lookup: bool) {
            let study = Study::new(source, true);
            if lookup {
                fs::write(study.0.join("input/lookup.csv"), INPUT).unwrap();
            }
            let path = study.path();
            let issues = file_application::check(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            );
            assert!(issues.is_empty(), "{issues:?}");
            let result = file_application::domain(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .unwrap();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_expected(&result);
        }
        #[test]
        fn admitted_driver_and_named_intermediate_operands_keep_independent_rounding_truth() {
            qualified(
                &POSITIVE
                    .replace(
                        "SOURCE: input/adsl.csv",
                        "SOURCE: {path: input/adsl.csv, types: {VAL1: float, VAL0: float}}",
                    )
                    .replace("source: VAL1", "source: SOURCE.VAL1"),
                false,
            );
            let source = POSITIVE.replace("SOURCE: input/adsl.csv", "SOURCE: input/adsl.csv\n  LOOKUP: {path: input/lookup.csv, types: {VAL1: float}}\nbase: SOURCE\nintermediates:\n  - id: SELECTED\n    dataset: LOOKUP\n    key: [STUDYID, USUBJID]\n    no_match: null").replace("source: VAL1", "source: SELECTED.VAL1");
            qualified(&source, true);
        }
        #[test]
        fn nested_case_and_row_template_rounding_keep_the_same_independent_expected_rows() {
            let source = POSITIVE.replace("round_half_away_from_zero:\n        source: VAL1\n        digits: 1", "case:\n        - when: \"VAL1 IS NOT NULL\"\n          then: {round_half_away_from_zero: {source: VAL1, digits: 1}}\n        - otherwise: {literal: null}");
            qualified(&source, false);
            let source = format!("{POSITIVE}\nrows:\n  - id: RECORDS\n    dataset: SOURCE\n    derivations:\n      RND1: {{round_half_away_from_zero: {{source: VAL1, digits: 1}}}}\n");
            qualified(&source, false);
        }
        #[test]
        fn unselected_case_uses_static_known_type_validation_before_study_capture() {
            let source = NEGATIVE.replace("round_half_away_from_zero:\n        source: SEX\n        digits: 1", "case:\n        - when: \"SEX = 'NEVER'\"\n          then: {round_half_away_from_zero: {source: SEX, digits: 1}}\n        - otherwise: {literal: null}");
            let study = Study::new(&source, false);
            let path = study.path();
            let findings = file_application::check(
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            );
            assert_eq!(findings.len(), 1, "{findings:?}");
            assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0418"));
            assert_eq!(
                findings[0].spec_paths,
                ["columns.RNDVAL.derivation.case[0].then.round_half_away_from_zero.source"]
            );
            assert_eq!(
                yamaa_adapters::json_wire::decode(&findings[0].context).unwrap(),
                json!({"source":"SEX","expected":"numeric","actual":"str"})
            );
            assert!(!study.0.join("input").exists());
        }
    }
    mod declarative_temporal {
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new(name: &str, input: bool) -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-temporal-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                let original = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                fs::copy(original.join("spec.yaml"), path.join("spec.yaml")).unwrap();
                if input {
                    copy_directory(&original.join("input"), &path.join("input"));
                }
                Self(fs::canonicalize(path).unwrap())
            }
            fn path(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().into()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn copy_directory(source: &Path, target: &Path) {
            fs::create_dir(target).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let destination = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_directory(&entry.path(), &destination);
                } else {
                    fs::copy(entry.path(), destination).unwrap();
                }
            }
        }
        fn identity(path: &str) -> Identity<'_> {
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification: path,
                base_directory: ".",
            }
        }
        #[test]
        fn original_temporal_documents_keep_independent_complete_csv_and_explicit_save() {
            for name in ["adam-adsl-duration", "adam-adsl-analysis-age"] {
                let study = Study::new(name, true);
                let path = study.path();
                let check = file_application::check(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                );
                assert!(check.is_empty(), "{name}: {check:?}");
                let mut result = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .unwrap();
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                let expected = fs::read(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../../benchmarks")
                        .join(name)
                        .join("expected/adsl.csv"),
                )
                .unwrap();
                assert_eq!(
                    result.output_table().unwrap().rows(),
                    expected.iter().filter(|&&byte| byte == b'\n').count() - 1,
                    "{name}"
                );
                assert!(!study.0.join("adsl.csv").exists());
                assert!(result.save().unwrap());
                assert_eq!(fs::read(study.0.join("adsl.csv")).unwrap(), expected);
            }
        }
        #[test]
        fn original_temporal_static_failures_keep_complete_independent_diagnostics_before_study_read(
        ) {
            for name in [
                "negative-date-diff-endpoints",
                "negative-date-diff-units",
                "negative-study-day-datetime",
                "negative-epoch-day-non-date",
            ] {
                let study = Study::new(name, false);
                let path = study.path();
                let findings = file_application::check(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                );
                let expected = expected_yaml(
                    &fs::read(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../../benchmarks")
                            .join(name)
                            .join("expected/error.yaml"),
                    )
                    .unwrap(),
                );
                assert_eq!(findings.len(), 1, "{name}: {findings:?}");
                let finding = &findings[0];
                let actual = yamaa_adapters::json!({"phase":finding.phase, "condition":finding.condition, "requirement":finding.requirement, "spec_paths":finding.spec_paths, "context":yamaa_adapters::json_wire::decode(&finding.context).unwrap()});
                assert_eq!(actual, expected, "{name}");
                let failed = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .err()
                .unwrap();
                assert_eq!(failed, findings);
                assert!(!study.0.join("input").exists());
                assert!(!study.0.join("adsl.csv").exists());
            }
        }

        fn expected_yaml(bytes: &[u8]) -> yamaa_core::json::Value {
            use yamaa_core::schema::{Document, DocumentNode as N};
            fn node(document: &Document, index: usize) -> yamaa_core::json::Value {
                match &document.nodes()[index] {
                    N::Text(text) => yamaa_adapters::json!(text),
                    N::Null => yamaa_core::json::Value::Null,
                    N::Sequence(items) => items.iter().map(|id| node(document, *id)).collect(),
                    N::Mapping(entries) => entries
                        .iter()
                        .map(|(key, value)| {
                            (
                                node(document, *key).as_str().unwrap().to_owned(),
                                node(document, *value),
                            )
                        })
                        .collect::<yamaa_core::json::Map<_, _>>()
                        .into(),
                    other => panic!("unexpected independent diagnostic scalar: {other:?}"),
                }
            }
            let decoded = yamaa_adapters::yaml_decode::decode_yaml(bytes).unwrap();
            node(&decoded.document, decoded.document.root())
        }

        #[test]
        fn study_day_epoch_and_nested_row_cases_execute_typed_driver_operands_and_missing_values() {
            let source = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  SRC: {path: input.csv, types: {ID: int, D: date, R: date}}\noutput: {path: result.csv, columns: [ID, SD, EPOCH]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: SD\n    type: int\n    derivation: {study_day: {date: SRC.D, reference: SRC.R}}\n  - name: EPOCH\n    type: int\n    derivation: {to_epoch_day: {source: SRC.D}}\n";
            let input = b"ID,D,R\n1,2023-12-31,2024-01-01\n2,2024-01-01,2024-01-01\n3,2024-01-02,2024-01-01\n4,,2024-01-01\n5,2024-01-01,\n";
            let expected = b"ID,SD,EPOCH\n1,-1,19722\n2,1,19723\n3,2,19724\n4,,\n5,,19723\n";
            for source in [source.into(), source.replace("derivation: {study_day: {date: SRC.D, reference: SRC.R}}", "derivation: {case: [{when: 'ID > 0', then: {study_day: {date: SRC.D, reference: SRC.R}}}, {otherwise: {literal: null}}]}"), format!("{source}rows:\n  - id: records\n    dataset: SRC\n    derivations:\n      SD: {{study_day: {{date: SRC.D, reference: SRC.R}}}}\n")] {
        let study = Study::new("adam-adsl-duration", false);
        fs::write(study.0.join("spec.yaml"), source).unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let path = study.path();
        let issues = file_application::check(Request { specification: &path, environment: None }, identity(&path));
        assert!(issues.is_empty(), "{issues:?}");
        let mut result = file_application::domain(Request { specification: &path, environment: None }, identity(&path)).unwrap();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(!study.0.join("result.csv").exists());
        assert!(result.save().unwrap());
        assert_eq!(fs::read(study.0.join("result.csv")).unwrap(), expected);
    }
        }

        #[test]
        fn row_temporal_replay_independent_reference_preserves_defaults_and_post_capture_types() {
            // Complete independent captures, including absent-input/type-error double faults.
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(r###"{
  "bare-default-to_epoch_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      to_epoch_day:\n        source: D1\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP\na,1970-01-01,1970-01-01,0\nb,2000-01-01,1970-01-01,10957\n"
  },
  "missing-input-row-invalid-to_epoch_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      to_epoch_day:\n        source: SRC.D\n",
    "inputs": {},
    "findings": [
      {
        "phase": "validation",
        "condition": "resource_path_missing",
        "spec_paths": [
          "input.SRC.path"
        ],
        "requirement": "REQ-0785",
        "context": {
          "dataset": "SRC",
          "path": "src.csv"
        }
      }
    ],
    "handler_counts": []
  },
  "row-invalid-to_epoch_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      to_epoch_day:\n        source: SRC.D\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [
      {
        "phase": "validation",
        "condition": "incompatible_input_type",
        "spec_paths": [
          "rows[0].derivations.EP.to_epoch_day.source"
        ],
        "requirement": "REQ-0606",
        "context": {
          "source": "SRC.D",
          "expected": "date",
          "actual": "str"
        }
      }
    ],
    "handler_counts": []
  },
  "bare-default-study_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      study_day:\n        date: D2\n        reference: D1\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP\na,1970-01-01,1970-01-01,1\nb,2000-01-01,1970-01-01,-10957\n"
  },
  "missing-input-row-invalid-study_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      study_day:\n        date: SRC.D\n        reference: SRC.D\n",
    "inputs": {},
    "findings": [
      {
        "phase": "validation",
        "condition": "resource_path_missing",
        "spec_paths": [
          "input.SRC.path"
        ],
        "requirement": "REQ-0785",
        "context": {
          "dataset": "SRC",
          "path": "src.csv"
        }
      }
    ],
    "handler_counts": []
  },
  "row-invalid-study_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      study_day:\n        date: SRC.D\n        reference: SRC.D\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [
      {
        "phase": "validation",
        "condition": "incompatible_input_type",
        "spec_paths": [
          "rows[0].derivations.EP.study_day.date"
        ],
        "requirement": "REQ-0606",
        "context": {
          "source": "SRC.D",
          "expected": "date",
          "actual": "str"
        }
      },
      {
        "phase": "validation",
        "condition": "incompatible_input_type",
        "spec_paths": [
          "rows[0].derivations.EP.study_day.reference"
        ],
        "requirement": "REQ-0606",
        "context": {
          "source": "SRC.D",
          "expected": "date",
          "actual": "str"
        }
      }
    ],
    "handler_counts": []
  },
  "bare-default-date_diff": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      date_diff:\n        start: D1\n        end: D2\n        unit: day\n        bounds: exclusive\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP\na,1970-01-01,1970-01-01,0\nb,2000-01-01,1970-01-01,-10957\n"
  },
  "missing-input-row-invalid-date_diff": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      date_diff:\n        start: SRC.D\n        end: SRC.D\n        unit: day\n        bounds: exclusive\n",
    "inputs": {},
    "findings": [
      {
        "phase": "validation",
        "condition": "resource_path_missing",
        "spec_paths": [
          "input.SRC.path"
        ],
        "requirement": "REQ-0785",
        "context": {
          "dataset": "SRC",
          "path": "src.csv"
        }
      }
    ],
    "handler_counts": []
  },
  "row-invalid-date_diff": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types: {}\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\nrows:\n- id: dates\n  derivations:\n    EP:\n      date_diff:\n        start: SRC.D\n        end: SRC.D\n        unit: day\n        bounds: exclusive\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [
      {
        "phase": "validation",
        "condition": "incompatible_input_type",
        "spec_paths": [
          "rows[0].derivations.EP.date_diff.start"
        ],
        "requirement": "REQ-0606",
        "context": {
          "source": "SRC.D",
          "expected": "date",
          "actual": "str"
        }
      },
      {
        "phase": "validation",
        "condition": "incompatible_input_type",
        "spec_paths": [
          "rows[0].derivations.EP.date_diff.end"
        ],
        "requirement": "REQ-0606",
        "context": {
          "source": "SRC.D",
          "expected": "date",
          "actual": "str"
        }
      }
    ],
    "handler_counts": []
  },
  "nested-default-to_epoch_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\n  - NEXT\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\n  derivation:\n    to_epoch_day:\n      source: D1\n- name: NEXT\n  type: int\nrows:\n- id: dates\n  derivations:\n    NEXT:\n      compute:\n        expr: EP + 1\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP,NEXT\na,1970-01-01,1970-01-01,0,1\nb,2000-01-01,1970-01-01,10957,10958\n"
  },
  "nested-default-study_day": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\n  - NEXT\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\n  derivation:\n    study_day:\n      date: D2\n      reference: D1\n- name: NEXT\n  type: int\nrows:\n- id: dates\n  derivations:\n    NEXT:\n      compute:\n        expr: EP + 1\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP,NEXT\na,1970-01-01,1970-01-01,1,2\nb,2000-01-01,1970-01-01,-10957,-10956\n"
  },
  "nested-default-date_diff": {
    "specification": "schema_version: '1.0'\ndomain: TEST\nkeys:\n- ID\ninput:\n  SRC:\n    path: src.csv\n    types:\n      D: date\n      E: date\noutput:\n  path: result.csv\n  columns:\n  - ID\n  - D1\n  - D2\n  - EP\n  - NEXT\ncolumns:\n- name: ID\n  type: str\n  derivation: SRC.ID\n- name: D1\n  type: date\n  derivation: SRC.D\n- name: D2\n  type: date\n  derivation:\n    literal: '1970-01-01'\n- name: EP\n  type: int\n  derivation:\n    date_diff:\n      start: D1\n      end: D2\n      unit: day\n      bounds: exclusive\n- name: NEXT\n  type: int\nrows:\n- id: dates\n  derivations:\n    NEXT:\n      compute:\n        expr: EP + 1\n",
    "inputs": {
      "src.csv": "ID,D,E\na,1970-01-01,1970-01-02\nb,2000-01-01,2000-01-02\n"
    },
    "findings": [],
    "handler_counts": [],
    "csv": "ID,D1,D2,EP,NEXT\na,1970-01-01,1970-01-01,0,1\nb,2000-01-01,1970-01-01,-10957,-10956\n"
  }
}"###).unwrap();
            for (name, truth) in reference.as_object().unwrap() {
                let study = Study::new("adam-adsl-duration", false);
                fs::write(
                    study.0.join("spec.yaml"),
                    truth["specification"].as_str().unwrap(),
                )
                .unwrap();
                for (name, bytes) in truth["inputs"].as_object().unwrap() {
                    fs::write(study.0.join(name), bytes.as_str().unwrap()).unwrap();
                }
                let path = study.path();
                let result = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                );
                let (issues, mut success) = match result {
                    Ok(result) => (result.issues().to_vec(), Some(result)),
                    Err(issues) => (issues, None),
                };
                let actual: Vec<_> = issues
                    .iter()
                    .map(|finding| {
                        yamaa_adapters::json!({
                            "phase": finding.phase, "condition": finding.condition,
                            "requirement": finding.requirement, "spec_paths": finding.spec_paths,
                            "context": yamaa_adapters::json_wire::decode(&finding.context).unwrap()
                        })
                    })
                    .collect();
                assert_eq!(yamaa_adapters::json!(actual), truth["findings"], "{name}");
                assert!(!study.0.join("result.csv").exists(), "{name}");
                if let Some(csv) = truth["csv"].as_str() {
                    let result = success.as_mut().unwrap();
                    assert!(result.output_table().is_some());
                    assert_eq!(
                        result.observations()["handler_counts"],
                        truth["handler_counts"],
                        "{name}"
                    );
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(study.0.join("result.csv")).unwrap(),
                        csv.as_bytes(),
                        "{name}"
                    );
                } else {
                    if let Some(result) = success.as_mut() {
                        assert!(result.output_table().is_none(), "{name}");
                        assert!(result.save().is_err(), "{name}");
                    }
                    assert!(!study.0.join("result.csv").exists(), "{name}");
                }
            }
        }
    }
    mod declarative_temporal_imputation {
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
                    "yamaa-temporal-imputation-{}-{}",
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
        #[test]
        fn unchanged_original_fixtures_qualify_imputation_precision_and_conversion_bytes() {
            for (name, output) in [("schema-birth-date", "adsl.csv"), ("sdtm-dm-age", "dm.csv")] {
                let fixture = Fixture::new(name);
                let mut result = fixture.run();
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(fixture.0.join(output)).unwrap(),
                    fs::read(fixture.0.join("expected").join(output)).unwrap(),
                    "{name}"
                );
            }
        }
        #[test]
        fn original_negative_fixtures_retain_complete_normative_diagnostics() {
            for name in [
                "negative-impute-bad-month",
                "negative-impute-unreachable-month",
                "negative-impute-missing-month",
                "negative-impute-day-rule",
                "negative-impute-bad-day",
                "negative-date-conversion",
            ] {
                let fixture = Fixture::new(name);
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
                        assert!(result.output().is_none(), "{name}");
                        result.issues().to_vec()
                    }
                    Err(issues) => issues,
                };
                let expected = golden(&fixture);
                let actual = &issues[0];
                assert_eq!(actual.phase, expected["phase"], "{name}");
                assert_eq!(actual.condition, expected["condition"], "{name}");
                assert_eq!(
                    actual.requirement.as_deref(),
                    expected["requirement"].as_str(),
                    "{name}"
                );
                assert_eq!(
                    yamaa_adapters::json_wire::project(&actual.spec_paths).unwrap(),
                    expected["spec_paths"],
                    "{name}"
                );
                assert_eq!(
                    yamaa_adapters::json_wire::decode(&actual.context).unwrap(),
                    expected["context"],
                    "{name}"
                );
            }
        }

        /// Original error context is a required subset, as in the conformance adapter.
        /// Check every unchanged required fact and separately compare the complete
        /// independently captured reference finding, including additional operation facts.
        #[test]
        fn unchanged_partial_oracles_and_complete_captured_reference_facts_both_match() {
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/temporal_imputation_reference.json"),
            )
            .unwrap();
            for name in [
                "negative-impute-bad-source",
                "negative-precision-bad-source",
                "negative-datetime-impute-bad-source",
                "negative-datetime-precision-bad-source",
            ] {
                let fixture = Fixture::new(name);
                let result = fixture.run();
                assert!(result.output().is_none());
                assert_eq!(result.issues().len(), 1);
                let issue = &result.issues()[0];
                let actual = yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()});
                assert_eq!(actual, reference[name][0], "{name}");
                let expected = golden(&fixture);
                for field in ["phase", "condition", "requirement", "spec_paths"] {
                    assert_eq!(actual[field], expected[field], "{name}: {field}");
                }
                for (key, value) in expected["context"].as_object().unwrap() {
                    assert_eq!(actual["context"][key], *value, "{name}: context.{key}");
                }
                assert!(!expected["context"]
                    .as_object()
                    .unwrap()
                    .contains_key("operation"));
                assert!(actual["context"]["operation"].is_string());
            }
        }

        #[test]
        fn row_imputation_and_column_precision_use_original_binding_and_completed_values() {
            let fixture = Fixture::new("schema-birth-date");
            fs::write(
                fixture.0.join("spec.yaml"),
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, DTM, PR, DATE]}
columns:
  - {name: ID, type: int}
  - {name: DTM, type: datetime}
  - name: PR
    type: str
    derivation: {datetime_precision: {source: DTM, missing: null}}
  - name: DATE
    type: date
    derivation: {to_date: {source: DTM}}
rows:
  - id: records
    dataset: SRC
    derivations:
      ID: SRC.ID
      DTM: {datetime_impute: {source: SRC.TEXT, time: last, missing: null, invalid: null}}
"#,
            )
            .unwrap();
            fs::write(
                fixture.0.join("input.csv"),
                b"ID,TEXT\n1,2024-02-29\n2,2024-02-29T12:13\n3,\n",
            )
            .unwrap();
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(fixture.0.join("output.csv")).unwrap(),b"ID,DTM,PR,DATE\n1,2024-02-29T23:59:59,D,2024-02-29\n2,2024-02-29T12:13:00,S,2024-02-29\n3,,,\n");
        }
        #[test]
        fn nested_case_conversion_reads_only_its_selected_completed_datetime() {
            let fixture = Fixture::new("schema-birth-date");
            fs::write(
                fixture.0.join("spec.yaml"),
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: input.csv}
output: {path: output.csv, columns: [ID, DTM, PR, DATE]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - name: DTM
    type: datetime
    derivation: {datetime_impute: {source: SRC.TEXT, time: last, missing: null, invalid: null}}
  - name: PR
    type: str
    derivation: {datetime_precision: {source: DTM, missing: null}}
  - name: DATE
    type: date
    derivation:
      case:
        - when: "PR = 'D'"
          then: {to_date: {source: DTM}}
        - otherwise: {literal: null}
"#,
            )
            .unwrap();
            fs::write(
                fixture.0.join("input.csv"),
                b"ID,TEXT\n1,2024-02-29\n2,2024-02-29T12:13\n3,\n",
            )
            .unwrap();
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
        fs::read(fixture.0.join("output.csv")).unwrap(),
        b"ID,DTM,PR,DATE\n1,2024-02-29T23:59:59,D,2024-02-29\n2,2024-02-29T12:13:00,S,\n3,,,\n"
    );
        }
    }
    mod declarative_text_case {
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
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
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
                let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                    include_str!("fixtures/text_case_reference.json"),
                )
                .unwrap();
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
                "ID,TEXT\n1,FOLLOW-UP\n2,\u{00df}\u{0130}\u{00e9}\u{1f642}AbC\n3,a\0BC\n4,\n"
                    .as_bytes(),
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
            let reference: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(
                include_str!("fixtures/text_case_reference.json"),
            )
            .unwrap();
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
                        yamaa_adapters::json!({"phase":"validation","condition":"incompatible_input_type","requirement":"REQ-0308","spec_paths":["rows[0].derivations.TEXT.str_case.source"],"context":{"source":"SRC.TEXT","expected":"str","actual":kind}})
                    ]
                );
                assert!(!fixture.0.join("output.csv").exists());
            }
        }
    }
    mod declarative_text_extract {
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
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
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
                yamaa_adapters::json!({
                    "phase":cause["phase"],"condition":cause["condition"],"requirement":cause["requirement"],
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
            let spec = yamaa_adapters::json!({"schema_version":"1.0","domain":"TEST","keys":["ID"],"input":{"SRC":"input.csv"},"output":{"path":"output.csv","columns":names},"columns":columns});
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let document = schema
                .prepare_standalone(yamaa_adapters::specification_source::Source {
                    identity: "spec.yaml".into(),
                    bytes: yamaa_adapters::json_wire::encode(&spec).into_bytes(),
                })
                .unwrap();
            let Err(yamaa_adapters::specification_run::Error::Prepare(
                yamaa_core::specification::PrepareError::Invalid(findings),
            )) = yamaa_adapters::specification_run::PreparedRun::prepare(document)
            else {
                panic!("all malformed patterns must retain their language findings")
            };
            assert_eq!(findings.len(), 60);
            let expected = (0..60).map(|index| yamaa_adapters::json!({
        "phase":"validation", "condition":"invalid_regex", "requirement":"REQ-0827",
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
                    "yamaa-str-pad-{}-{}",
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
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
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
                let identity = Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: name,
                    specification: &specification,
                    base_directory: ".",
                };
                let mut result = match file_application::domain(
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
                        // Keep the complete independent loader capture in the fixture.
                        // Native schema diagnostics still omit these loader provenance fields.
                        let context = expected["context"].as_object_mut().unwrap();
                        assert_eq!(
                            context.remove("source").unwrap(),
                            "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                        );
                        assert_eq!(
                            context.remove("entry").unwrap(),
                            "/private/tmp/yamaa-root-text-pad-audit/width-noninteger/spec.yaml"
                        );
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
            let identity = Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "negative-str-pad-width",
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
                Ok(_) => panic!("invalid width must fail before input capture"),
            };
            assert_eq!(issues.len(), 1);
            assert_eq!(issue_json(&issues[0]), golden(&fixture));
        }
    }
    mod declarative_text_template {
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
            yamaa_adapters::json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":yamaa_adapters::json_wire::decode(&issue.context).unwrap()})
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
                let schema = yamaa_adapters::shipped_schema::capture().unwrap();
                let document = schema
                    .prepare_standalone(yamaa_adapters::specification_source::Source {
                        identity: "spec.yaml".into(),
                        bytes: yamaa_adapters::json_wire::encode(&spec).into_bytes(),
                    })
                    .unwrap();
                let Err(yamaa_adapters::specification_run::Error::Prepare(
                    yamaa_core::specification::PrepareError::Invalid(findings),
                )) = yamaa_adapters::specification_run::PreparedRun::prepare(document)
                else {
                    panic!("all malformed templates must retain their language findings")
                };
                assert_eq!(findings.len(), paths.len());
                let expected = paths.iter().map(|path| yamaa_adapters::json!({
            "phase":"validation", "condition":"invalid_string_template", "requirement":"REQ-0461",
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
            // The added columns must be live under pruning (REQ-0638) for
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
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let document = schema
                .prepare_standalone(yamaa_adapters::specification_source::Source {
                    identity: "spec.yaml".into(),
                    bytes: yamaa_adapters::json_wire::encode(&spec).into_bytes(),
                })
                .unwrap();
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
            use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
            for (year, month, day, days) in [
                (1, 1, 1, -719162.0_f64),
                (1970, 1, 1, 0.0),
                (2000, 2, 29, 11016.0),
                (9999, 12, 31, 2932896.0),
            ] {
                let date = Date::new(year, month, day, DatePrecision::Year).unwrap();
                let bytes = encode(Value::Date(date));
                assert_eq!(bytes.tag, 5);
                assert_eq!(bytes.payload, days.to_le_bytes());
                let Value::Date(decoded) = decode(5, &bytes.payload).unwrap() else {
                    panic!("date")
                };
                assert_eq!(decoded.collected_precision(), DatePrecision::Day);
                assert_eq!(
                    decode(5, &bytes.payload),
                    Ok(Value::Date(
                        Date::new(year, month, day, DatePrecision::Day).unwrap()
                    ))
                );
            }
            for (year, month, day, hour, minute, second, epoch) in [
                (1, 1, 1, 0, 0, 0, -62135596800.0_f64),
                (1969, 12, 31, 23, 59, 59, -1.0),
                (1970, 1, 1, 0, 0, 0, 0.0),
                (9999, 12, 31, 23, 59, 59, 253402300799.0),
            ] {
                let date = Date::new(year, month, day, DatePrecision::Day).unwrap();
                let value =
                    DateTime::new(date, hour, minute, second, DateTimePrecision::Day).unwrap();
                let bytes = encode(Value::DateTime(value));
                assert_eq!(bytes.tag, 6);
                assert_eq!(bytes.payload, epoch.to_le_bytes());
                let Value::DateTime(decoded) = decode(6, &bytes.payload).unwrap() else {
                    panic!("datetime")
                };
                assert_eq!(decoded.collected_precision(), DateTimePrecision::Second);
                assert_eq!(
                    decode(6, &bytes.payload),
                    Ok(Value::DateTime(
                        DateTime::new(date, hour, minute, second, DateTimePrecision::Second)
                            .unwrap()
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
