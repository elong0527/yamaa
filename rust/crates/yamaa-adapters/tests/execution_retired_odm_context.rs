#![cfg(feature = "yaml")]
//! Execution contracts.
#[allow(dead_code)]
mod support;
mod execution {
    mod retired_odm_context {
        //! Original retired ODM-context operands through the shared public file engine.
        //! Generic source reads keep their ordinary relation semantics; only `odm`
        //! expressions select the fixed eleven-field ODM interpretation.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::json;
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        struct Fixture(PathBuf);
        impl Fixture {
            fn new(specification: &str, input: &str) -> Self {
                static NEXT: AtomicUsize = AtomicUsize::new(0);
                let root = std::env::temp_dir().join(format!(
                    "yamaa-retired-odm-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                fs::write(root.join("spec.yaml"), specification).unwrap();
                fs::write(root.join("odm.csv"), input).unwrap();
                Self(root)
            }
            fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
                let specification = self.0.join("spec.yaml");
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
                        example: "retired-odm-context",
                        specification,
                        base_directory: ".",
                    },
                )
            }
            fn saved(&self, expected: &str) -> Value {
                let mut domain = self.run().unwrap();
                assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                let report = domain.observations();
                assert!(domain.save().unwrap());
                assert_eq!(
                    fs::read(self.0.join("out.csv")).unwrap(),
                    expected.as_bytes()
                );
                report
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        fn specification(key: &str, kind: &str, expression: &str, input: &str) -> String {
            format!(
        "schema_version: '1.0'\ndomain: OUT\nkeys: [{key}]\ninput: {{ODM: {input}}}\noutput: {{path: out.csv, columns: [{key}, VALUE]}}\ncolumns:\n  - {{name: {key}, type: str, derivation: ODM.{key}}}\n  - {{name: VALUE, type: {kind}, derivation: {expression}}}\n"
    )
        }
        fn issue_rows(issues: &[Issue]) -> Value {
            Value::Array(
                issues
                    .iter()
                    .map(|issue| {
                        let mut value = issue.as_value();
                        value["context"] = issue.context.clone();
                        value
                    })
                    .collect(),
            )
        }

        const ITEMS: &str = "StudyOID,SubjectKey,ItemOID,Value\nS1,001,IT.DM.SEX,Male\nS1,001,IT.DM.AGE,34\nS1,001,IT.DM.ARM,Placebo\n";

        #[test]
        fn original_filters_mapping_and_first_available_read_their_own_collected_records() {
            for (expression, expected) in [
        ("{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.AGE'\"}}", "34"),
        ("{mapping: {source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.SEX'\"}, dict: {Male: M, Female: F}, missing: U}}", "M"),
        ("{first_available: {sources: [{variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.ARM'\"}], missing: Unassigned}}", "Placebo"),
        ("{first_available: {sources: [{variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.RACE'\"}], missing: Unassigned}}", "Unassigned"),
    ] {
        let fixture = Fixture::new(&specification("SubjectKey", "str", expression, "odm.csv"), ITEMS);
        fixture.saved(&format!("SubjectKey,VALUE\n001,{expected}\n"));
    }
            let fixture = Fixture::new(
        &specification("SubjectKey", "str", "{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.RACE'\", absent: fallback}}", "odm.csv"),
        ITEMS,
    );
            let report = fixture.saved("SubjectKey,VALUE\n001,\n");
            assert_eq!(
                report["handler_counts"],
                json!([{
                    "spec_path":"columns.VALUE.derivation.source.absent", "handler":"absent", "count":0
                }])
            );
        }

        #[test]
        fn original_collected_conflicts_count_values_after_the_source_filter() {
            for (expression, input, count) in [
        ("ODM.Value", ITEMS, 3),
        (
            "{source: {variable: ODM.Value, filter: \"ODM.ItemOID = 'IT.DM.SEX'\"}}",
            "StudyOID,SubjectKey,ItemOID,Value\nS1,001,IT.DM.SEX,Male\nS1,001,IT.DM.SEX,Female\n",
            2,
        ),
    ] {
        let fixture = Fixture::new(
            &specification("SubjectKey", "str", expression, "odm.csv"),
            input,
        );
        let mut domain = fixture.run().unwrap();
        assert_eq!(
            issue_rows(&domain.issues()),
            json!([{
                "phase":"derivation", "condition":"multiple_values_per_key", "requirement":"REQ-0075",
                "spec_paths":["columns.VALUE.derivation.source"],
                "context":{"identifier":"ODM.Value", "value_count":count, "keys":[{"SubjectKey":"001"}]}
            }])
        );
        assert!(domain.output().is_none());
        assert!(domain.save().is_err());
        assert!(!fixture.0.join("out.csv").exists());
    }
        }

        #[test]
        fn original_selected_missing_values_never_fall_back_to_unselected_records() {
            let expression =
                "{source: {variable: ODM.Value, filter: \"ODM.RecordKind = 'TARGET'\"}}";
            for (rows, expected) in [
                ("G1,UNRELATED,unrelated\nG1,TARGET,\n", ""),
                ("G1,UNRELATED,unrelated\nG1,TARGET,\nG1,TARGET,\n", ""),
                (
                    "G1,UNRELATED,unrelated\nG1,TARGET,\nG1,TARGET,target\n",
                    "target",
                ),
                ("G1,OTHER,other\nG1,TARGET,\nG1,UNRELATED,x\n", ""),
                ("G1,UNRELATED,x\nG1,TARGET,\nG1,OTHER,other\n", ""),
                ("G1,TARGET,\nG1,UNRELATED,x\nG1,OTHER,other\n", ""),
            ] {
                let fixture = Fixture::new(
                    &specification("GroupID", "str", expression, "odm.csv"),
                    &format!("GroupID,RecordKind,Value\n{rows}"),
                );
                fixture.saved(&format!("GroupID,VALUE\nG1,{expected}\n"));
            }
        }

        #[test]
        fn original_source_filter_cannot_read_another_relation() {
            let fixture = Fixture::new(
                &specification(
                    "SubjectKey",
                    "str",
                    "{source: {variable: ODM.Value, filter: \"OTHER.ItemOID = 'IT.DM.AGE'\"}}",
                    "odm.csv",
                ),
                ITEMS,
            );
            let issues = match fixture.run() {
                Ok(domain) => {
                    assert!(domain.output().is_none());
                    domain.issues().to_vec()
                }
                Err(issues) => issues,
            };
            assert_eq!(
                issue_rows(&issues),
                json!([{
                    "phase":"validation", "condition":"unknown_field", "requirement":"REQ-0132",
                    "spec_paths":["columns.VALUE.derivation.source.filter"],
                    "context":{"identifier":"OTHER.ItemOID", "dataset":"ODM"}
                }])
            );
            assert!(!fixture.0.join("out.csv").exists());
        }

        #[test]
        fn original_repeated_dates_are_one_value_and_distinct_dates_keep_the_exact_count() {
            let specification = specification(
                "SubjectKey",
                "date",
                "ODM.VISITDT",
                "{path: odm.csv, types: {VISITDT: date}}",
            );
            let first = "StudyOID,SubjectKey,VISITDT,ItemOID,Value\nS1,001,2025-01-02,IT.A,a\nS1,001,2025-01-02,IT.B,b\n";
            let fixture = Fixture::new(&specification, first);
            fixture.saved("SubjectKey,VALUE\n001,2025-01-02\n");
            let fixture = Fixture::new(
                &specification,
                &first.replace("2025-01-02,IT.B", "2025-01-03,IT.B"),
            );
            let domain = fixture.run().unwrap();
            assert_eq!(
                issue_rows(&domain.issues()),
                json!([{
                    "phase":"derivation", "condition":"multiple_values_per_key", "requirement":"REQ-0075",
                    "spec_paths":["columns.VALUE.derivation.source"],
                    "context":{"identifier":"ODM.VISITDT", "value_count":2, "keys":[{"SubjectKey":"001"}]}
                }])
            );
        }

        const HEADER: &str = "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value";
        const RECORD: &str = "S1,M1,001,SCREENING,1,FO.DM,1,IG.DM,1,IT.DM.AGE,34";

        #[test]
        fn original_binding_fixture_keeps_output_names_literal_fields_and_complete_source_count() {
            use yamaa_adapters::{
                shipped_schema, specification_run::PreparedRun, specification_source::Source,
            };
            use yamaa_core::{
                reference_binding::{Binding, Catalog, Dataset, Field},
                specification::BindError,
                table::{Column, TableSchema},
                value::ColumnType,
            };
            let original = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../benchmarks/schema-odm-form-items/spec.yaml");
            let document = shipped_schema::capture()
                .unwrap()
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: fs::read(original).unwrap(),
                })
                .unwrap();
            let run = PreparedRun::prepare(document).unwrap();
            let compiled = run.compiled();
            let source = TableSchema::new(
                HEADER
                    .split(',')
                    .map(|name| Column {
                        name: name.into(),
                        kind: ColumnType::Str,
                    })
                    .collect(),
            )
            .unwrap();
            assert!(matches!(
                compiled.bind_sources(&[]),
                Err(BindError::SourceCount)
            ));
            assert!(matches!(
                compiled.bind_sources(&[&source, &source]),
                Err(BindError::SourceCount)
            ));
            compiled.bind_sources(&[&source]).unwrap();
            let outputs = compiled
                .output()
                .columns()
                .iter()
                .map(|column| Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let fields = source
                .columns()
                .iter()
                .map(|column| Field {
                    name: &column.name,
                    column_type: column.kind,
                })
                .collect::<Vec<_>>();
            let catalog = Catalog::compile(
                &outputs,
                &[Dataset {
                    name: "ODM",
                    fields: &fields,
                }],
            )
            .unwrap();
            assert_eq!(
                catalog.bind("LBDTC"),
                Some(Binding::Output {
                    column: 7,
                    column_type: ColumnType::Date
                })
            );
            assert_eq!(
                catalog.bind("ODM.StudyOID"),
                Some(Binding::Dataset {
                    dataset: 0,
                    field: 0,
                    column_type: ColumnType::Str
                })
            );
            for name in ["UNKNOWN", "OTHER.Value", "ODM.IT.LB.LBDTC", "ODM.AGE"] {
                assert_eq!(catalog.bind(name), None, "{name}");
            }
        }

        #[test]
        fn original_fixed_odm_csv_canonicalizes_only_schema_names_and_ignores_vendor_fields() {
            let fixture = Fixture::new(
                &specification("SubjectKey", "str", "{odm: ODM.IT.DM.AGE}", "odm.csv"),
                &format!("{},SITEID\n{RECORD},SITE-9\n", HEADER.to_ascii_uppercase()),
            );
            let report = fixture.saved("SubjectKey,VALUE\n001,34\n");
            let source = &report["tables"][0];
            assert_eq!(
                source["columns"],
                json!(HEADER.split(',').collect::<Vec<_>>())
            );
            assert_eq!(source["types"], json!(vec!["str"; 11]));
            assert_eq!(
                source["rows"][0],
                json!(RECORD
                    .split(',')
                    .map(|value| json!({"type":"str", "value":value}))
                    .collect::<Vec<_>>())
            );
        }

        #[test]
        fn original_missing_identifier_reports_first_csv_record_and_complete_failure_count() {
            let missing = RECORD.replacen("001", "", 1);
            let fixture = Fixture::new(
                &specification("SubjectKey", "str", "{odm: ODM.IT.DM.AGE}", "odm.csv"),
                &format!("{HEADER}\n{RECORD}\n{missing}\n{missing}\n"),
            );
            let issues = match fixture.run() {
                Ok(domain) => {
                    assert!(domain.output().is_none());
                    domain.issues().to_vec()
                }
                Err(issues) => issues,
            };
            assert_eq!(
                issue_rows(&issues),
                json!([{
                    "phase":"ingest", "condition":"odm_schema_value_missing", "requirement":"REQ-1276",
                    "spec_paths":["input.ODM.path"],
                    "context":{"dataset":"ODM", "field":"SubjectKey", "record":3, "records":2}
                }])
            );
            assert!(!fixture.0.join("out.csv").exists());
        }
    }
    mod row_count_lookups {
        //! Original row-count verification lookups and declaration ordering.
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
        fn row_count_lookups_preserve_typed_scopes_metadata_and_filter_order() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["row_count_lookups"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(std::env::temp_dir().join(format!(
                    "yamaa-row-count-lookups-{}-{}",
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
                    example: "row_count_lookups",
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
                let mut result = file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                )
                .unwrap();
                assert_eq!(
                    issues(&result.issues()),
                    case["issues"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                let expected = case["verifications"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(|mut record| {
                        record["specification"] = yamaa_adapters::json!(path);
                        record
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    result.observations()["verifications"],
                    yamaa_adapters::json!(expected),
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["name"])
                );
                assert_eq!(
                    result.observations()["handler_counts"],
                    case["handler_counts"],
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
    mod row_derivations {
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
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    identity,
                )
                .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
            }
            fn check(&self) -> Vec<yamaa_adapters::issue_rows::Issue> {
                let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &specification,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "grouped-reducer-check",
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

        fn specification(columns: &str, rows: &str, output: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID, SECTION]\ninput: {{SRC: {{path: input.csv, types: {{I: int, D: date, R: date}}}}}}\noutput: {{path: output.csv, columns: {output}}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n  - {{name: SECTION, type: str}}\n{columns}rows:\n{rows}")
        }
        fn saved(fixture: &Fixture) -> Vec<u8> {
            let mut result = fixture.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            fs::read(fixture.0.join("output.csv")).unwrap()
        }
        #[test]
        fn row_cases_numeric_and_temporal_share_dependency_lowering() {
            let columns="  - {name: VALUE, type: int}\n  - {name: PICK, type: str}\n  - {name: DAY, type: int}\n  - {name: TEXT, type: str}\n";
            // Authored consumer-before-producer order proves that row-local reads use
            // the dependency graph, and that nested branches see converted values.
            let rows="  - id: one\n    derivations:\n      SECTION: {literal: one}\n      TEXT: {str_concat: {sources: [{case: [{when: 'VALUE = 9', then: {literal: nine}}, {otherwise: {literal: other}}]}, {source: PICK}]}}\n      DAY: {study_day: {date: SRC.D, reference: SRC.R}}\n      PICK: {first_available: {sources: [SRC.TEXT, ID], missing: none}}\n      VALUE: {case: [{when: 'SRC.I > 0', then: {compute: {expr: 'POWER(SRC.I, 2)'}}}, {otherwise: {literal: 0}}]}\n";
            let f = bespoke(
                &specification(columns, rows, "[ID, SECTION, VALUE, PICK, DAY, TEXT]"),
                b"ID,I,D,R,TEXT\na,3,2025-01-02,2025-01-01,x\nb,-2,2024-12-31,2025-01-01,\n",
            );
            assert_eq!(
                saved(&f),
                b"ID,SECTION,VALUE,PICK,DAY,TEXT\na,one,9,x,2,ninex\nb,one,0,b,-1,otherb\n"
            );
        }
        #[test]
        fn each_row_template_has_its_own_filtered_windows_and_chained_passes() {
            let columns="  - {name: VALUE, type: int, derivation: SRC.I}\n  - {name: SEQ, type: int}\n  - {name: NEXT, type: int}\n  - {name: DOUBLE, type: int}\n";
            let derivations="      VALUE: SRC.I\n      SEQ: {row_number: {window: {order_by: [VALUE]}}}\n      DOUBLE: {compute: {expr: 'SEQ * 2'}}\n      NEXT: {row_value: {source: DOUBLE, offset: 1, window: {order_by: [SEQ]}}}\n";
            let rows=format!("  - id: one\n    filter: VALUE > 0\n    derivations:\n      SECTION: {{literal: one}}\n{derivations}  - id: two\n    filter: VALUE > 1\n    derivations:\n      SECTION: {{literal: two}}\n{derivations}");
            let f = bespoke(
                &specification(columns, &rows, "[ID, SECTION, VALUE, SEQ, DOUBLE, NEXT]"),
                b"ID,I,D,R,TEXT\na,3,,,\nb,0,,,\nc,1,,,\nd,2,,,\n",
            );
            assert_eq!(saved(&f),b"ID,SECTION,VALUE,SEQ,DOUBLE,NEXT\na,one,3,3,6,\nc,one,1,1,2,4\nd,one,2,2,4,6\na,two,3,2,4,\nd,two,2,1,2,4\n");
        }
        #[test]
        fn grouped_row_case_reads_group_keys_and_converted_reductions() {
            let columns = "  - {name: TOTAL, type: int}\n  - {name: TEXT, type: str}\n";
            let rows="  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      SECTION: {literal: grouped}\n      TEXT: {case: [{when: 'TOTAL > 5', then: {source: {variable: SRC.ID}}}, {otherwise: {literal: low}}]}\n      TOTAL: {aggregate: {expr: 'SUM(SRC.I)'}}\n";
            let f = bespoke(
                &specification(columns, rows, "[ID, SECTION, TOTAL, TEXT]"),
                b"ID,I,D,R,TEXT\na,3,,,\nb,1,,,\na,4,,,\n",
            );
            assert_eq!(
                saved(&f),
                b"ID,SECTION,TOTAL,TEXT\na,grouped,7,a\nb,grouped,1,low\n"
            );
        }

        #[test]
        fn grouped_numeric_reducers_skip_missing_arguments_and_still_reject_present_text() {
            for reducer in ["SUM", "MEAN"] {
                let source = format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv, types: {{V: str}}}}}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\ncolumns: [{{name: ID, type: str}}, {{name: VALUE, type: float}}]\nrows:\n  - id: groups\n    group_by: [SRC.ID]\n    derivations:\n      ID: SRC.ID\n      VALUE: {{aggregate: {{expr: '{reducer}(SRC.V)'}}}}\n"
        );
                for (input, expected) in [
                    (b"ID,V\na,\na,\n".as_slice(), b"ID,VALUE\na,\n".as_slice()),
                    (b"ID,V\n".as_slice(), b"ID,VALUE\n".as_slice()),
                ] {
                    let fixture = bespoke(&source, input);
                    assert!(fixture.check().is_empty());
                    assert_eq!(saved(&fixture), expected, "{reducer}");
                }
                for input in [
                    b"ID,V\na,text\n".as_slice(),
                    b"ID,V\na,\na,text\n".as_slice(),
                ] {
                    let fixture = bespoke(&source, input);
                    assert!(fixture.check().is_empty());
                    let mut result = fixture.run();
                    assert_eq!(result.issues().len(), 1, "{reducer}: {:?}", result.issues());
                    let issue = &result.issues()[0];
                    assert_eq!(issue.phase, "validation");
                    assert_eq!(issue.condition, "incompatible_input_type");
                    assert_eq!(issue.requirement.as_deref(), Some("REQ-0510"));
                    assert_eq!(issue.spec_paths, ["rows[0].derivations.VALUE.aggregate"]);
                    assert_eq!(
                        issue.context.clone(),
                        yamaa_adapters::json!({"expr": format!("{reducer}(SRC.V)"),
                    "reducer": reducer, "source": "SRC.V", "expected": "numeric", "actual": "str"})
                    );
                    assert!(result.output().is_none());
                    assert!(matches!(result.save(), Err(file_application::FailedBuild)));
                    assert!(!fixture.0.join("output.csv").exists());
                }
            }
        }

        #[test]
        fn invalid_grouped_case_source_and_window_filter_refuse_publication() {
            let columns = "  - {name: VALUE, type: int}\n";
            let rows="  - id: grouped\n    group_by: [SRC.ID]\n    derivations:\n      SECTION: {literal: grouped}\n      VALUE: {case: [{when: 'SRC.I > 0', then: {literal: 1}}, {otherwise: {literal: 0}}]}\n";
            let f = bespoke(
                &specification(columns, rows, "[ID, SECTION, VALUE]"),
                b"ID,I,D,R,TEXT\na,3,,,\n",
            );
            // Non-grouped driver fields are statically bound after ingestion, never read by a predicate.
            let issues =
                attempt(&f).map_or_else(|issues| issues, |result| result.issues().to_vec());
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "ungrouped_driver_field");
            assert!(!f.0.join("output.csv").exists());
            let columns="  - {name: VALUE, type: int, derivation: SRC.I}\n  - {name: SEQ, type: int}\n  - {name: AFTER, type: int}\n";
            let rows="  - id: one\n    filter: AFTER > 0\n    derivations:\n      SECTION: {literal: one}\n      SEQ: {row_number: {window: {order_by: [VALUE]}}}\n      AFTER: {compute: {expr: 'SEQ + 1'}}\n";
            let f = bespoke(
                &specification(columns, rows, "[ID, SECTION, SEQ]"),
                b"ID,I,D,R,TEXT\na,3,,,\n",
            );
            let issues =
                attempt(&f).map_or_else(|issues| issues, |result| result.issues().to_vec());
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "phase_boundary");
            assert_eq!(
                issues[0].context.clone()["available_phase"],
                "window_derivation"
            );
            assert!(!f.0.join("output.csv").exists());
        }
        #[test]
        fn intermediate_identity_keeps_column_key_windows_over_all_constructed_templates() {
            let body = "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID, SEQ]\ninput:\n  SRC: {path: input.csv, types: {VALUE: int}}\n  D: donor.csv\nintermediates: [{id: P, dataset: D, key: ID}]\noutput: {path: output.csv, columns: [ID, SEQ, VALUE, PICK]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: SRC.VALUE}\n  - {name: SEQ, type: int, derivation: {row_number: {window: {group_by: [ID], order_by: [VALUE]}}}}\n  - {name: PICK, type: str, derivation: P.PICK}\nrows:\n  - {id: first, dataset: SRC, filter: \"SRC.KIND = 'A'\", derivations: {}}\n  - {id: second, dataset: SRC, filter: \"SRC.KIND = 'B'\", derivations: {}}\n";
            let f = bespoke(body, b"ID,KIND,VALUE\nx,A,4\nx,B,1\n");
            fs::write(f.0.join("donor.csv"), b"ID,PICK\nx,kept\n").unwrap();
            assert_eq!(saved(&f), b"ID,SEQ,VALUE,PICK\nx,2,4,kept\nx,1,1,kept\n");
        }
        #[test]
        fn grouped_row_column_defaults_use_the_shared_secondary_source_key_binding() {
            let body = "schema_version: '1.0'\ndomain: TEST\nbase: SRC\nkeys: [ID]\ninput:\n  SRC: {path: input.csv, types: {VALUE: int}}\n  D: {path: donor.csv, types: {REFERENCE: int}}\noutput: {path: output.csv, columns: [ID, TOTAL, REFERENCE]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: TOTAL, type: int}\n  - {name: REFERENCE, type: int, derivation: D.REFERENCE}\nrows:\n  - id: grouped\n    dataset: SRC\n    group_by: [SRC.ID]\n    derivations: {TOTAL: {aggregate: {expr: 'SUM(SRC.VALUE)'}}}\n";
            let f = bespoke(body, b"ID,VALUE\na,4\na,1\nb,2\n");
            fs::write(f.0.join("donor.csv"), b"ID,REFERENCE\na,10\nb,20\n").unwrap();
            assert_eq!(saved(&f), b"ID,TOTAL,REFERENCE\na,5,10\nb,2,20\n");
            fs::write(f.0.join("donor.csv"), b"ID,REFERENCE\na,10\na,10\nb,20\n").unwrap();
            let mut result = f.run();
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            assert_eq!(result.issues()[0].condition, "multiple_matches");
            assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0127"));
        }
        #[test]
        fn unchanged_vs_units_benchmark_runs_row_cases_and_numeric_branches() {
            let f = Fixture::new("sdtm-vs-units");
            let mut result = f.run();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(f.0.join("vs.csv")).unwrap(),
                fs::read(f.0.join("expected/vs.csv")).unwrap()
            );
        }

        #[test]
        fn column_and_case_scopes_use_the_shared_dispatch_with_distinct_nested_values() {
            let source="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv}}\noutput: {path: output.csv, columns: [ID, VALUE, TEXT]}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {compute: {expr: 'EXP(0) + LN(1) + POWER(3, 2)'}}}\n  - name: TEXT\n    type: str\n    derivation:\n      case:\n        - when: VALUE = 10\n          then: {str_concat: {sources: [{literal: 'prefix:'}, {first_available: {sources: [ID], missing: fallback}}]}}\n        - otherwise: {literal: wrong}\n";
            let f = bespoke(source, b"ID\na\n");
            assert_eq!(saved(&f), b"ID,VALUE,TEXT\na,10,prefix:a\n");
        }

        #[test]
        fn distinct_stored_drivers_group_in_their_own_schemas_and_complete_global_windows() {
            let source = r#"schema_version: '1.0'
domain: TEST
keys: [ID, SECTION]
input:
  ALPHA: {path: input.csv, types: {ID: int, S: int}}
  BETA: {path: beta.csv, types: {OTHER: int, AMOUNT: float}}
output: {path: output.csv, columns: [ID, SECTION, SEQ, GLOBAL, TOTAL]}
columns:
- {name: ID, type: int}
- {name: SECTION, type: str}
- {name: SEQ, type: int}
- {name: GLOBAL, type: int, derivation: {row_number: {window: {order_by: [ID, SECTION]}}}}
- {name: TOTAL, type: float}
rows:
- id: records
  dataset: ALPHA
  filter: 'ID > 1'
  derivations:
    ID: ALPHA.ID
    SECTION: {literal: A}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
    TOTAL: {literal: 0}
- id: groups
  dataset: BETA
  group_by: [BETA.OTHER]
  derivations:
    ID: BETA.OTHER
    SECTION: {literal: B}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
    TOTAL: {aggregate: {expr: 'SUM(BETA.AMOUNT)'}}
"#;
            let fixture = bespoke(source, b"ID,S\n3,7\n1,8\n2,9\n");
            fs::write(
                fixture.0.join("beta.csv"),
                b"PAD,OTHER,AMOUNT\nx,10,3\ny,7,5\nz,10,4\n",
            )
            .unwrap();
            assert_eq!(
                saved(&fixture),
                b"ID,SECTION,SEQ,GLOBAL,TOTAL\n3,A,1,2,0\n2,A,2,1,0\n10,B,1,4,7\n7,B,2,3,5\n"
            );
        }

        #[test]
        fn record_filter_promotes_numeric_defaults_and_their_complete_dependencies() {
            let fixture = bespoke(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID, SECTION]
input: {SRC: {path: input.csv, types: {I: int}}}
output: {path: output.csv, columns: [ID, SECTION, VALUE]}
columns:
- {name: ID, type: int}
- {name: SECTION, type: str, derivation: {literal: S}}
- {name: VALUE, type: float, derivation: {compute: {expr: 'POWER(ID, 2)'}}}
rows:
- id: selected
  filter: 'VALUE > 4'
  derivations: {ID: SRC.I}
"#,
                b"I\n1\n3\n2\n4\n",
            );
            assert_eq!(saved(&fixture), b"ID,SECTION,VALUE\n3,S,9\n4,S,16\n");
        }

        #[test]
        fn grouped_filters_read_completed_candidate_windows() {
            let fixture = bespoke(
                r#"schema_version: '1.0'
domain: TEST
keys: [ID]
input: {SRC: {path: input.csv, types: {I: int, V: float}}}
output: {path: output.csv, columns: [ID, TOTAL, SEQ]}
columns:
- {name: ID, type: int}
- {name: TOTAL, type: float}
- {name: SEQ, type: int}
rows:
- id: selected
  group_by: [SRC.I]
  filter: 'SEQ = 2 AND TOTAL = 7'
  derivations:
    ID: SRC.I
    TOTAL: {aggregate: {expr: 'SUM(SRC.V)'}}
    SEQ: {row_number: {window: {order_by: [{variable: ID, direction: desc}]}}}
"#,
                b"I,V\n3,8\n2,3\n1,5\n2,4\n",
            );
            // REQ-0058/REQ-0059 run grouped filters after the complete row graph,
            // including windows. REQ-0068's pre-window restriction is ungrouped.
            assert_eq!(saved(&fixture), b"ID,TOTAL,SEQ\n2,7,2\n");
        }
    }
    mod row_filter_paths {
        //! Static and build diagnostics use declaration indices, never arbitrary row IDs.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::json;
        use yamaa_adapters::{
            file_application, file_preparation::FileSpecification, file_resources::Resources,
            issue_rows::Issue, specification_report::Identity,
        };
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-row-filter-path-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(path.join("parent")).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn identity(&self, name: &str) -> String {
                let value = self.0.join(name).to_string_lossy().into_owned();
                #[cfg(windows)]
                let value = value.replace('\\', "/");
                value
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn issues(rows: &[Issue]) -> Value {
            Value::Array(
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
        fn check_and_build_retain_index_paths_and_literal_id_inherited_provenance() {
            for inherited in [false, true] {
                for row_id in ["ordinary", "dot.id", "bracket[999]"] {
                    let study = Study::new();
                    let entry = study.0.join("spec.yaml");
                    let text = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {{path: input.csv}}}}\noutput: {{path: result.csv, columns: [ID]}}\ncolumns: [{{name: ID, type: str}}]\nrows:\n  - {{id: clean, dataset: SRC, derivations: {{ID: SRC.ID}}}}\n  - {{id: '{row_id}', dataset: SRC, filter: 'ID >', derivations: {{ID: SRC.ID}}}}\n");
                    let owner = if inherited {
                        study.0.join("parent/base.yaml")
                    } else {
                        entry.clone()
                    };
                    fs::write(&owner, text).unwrap();
                    fs::write(owner.parent().unwrap().join("input.csv"), "ID\na\n").unwrap();
                    if inherited {
                        fs::write(
                            &entry,
                            "schema_version: '1.0'\nparents: [parent/base.yaml]\n",
                        )
                        .unwrap();
                    }
                    // Frozen pre-static row-filter grammar truth carries these two public
                    // context fields. Human declaring-file attribution uses held provenance.
                    let context = json!({"predicate":"ID >","position":4});
                    let prepared = FileSpecification::prepare(
                        Resources::new(study.0.to_str().unwrap(), study.0.to_str().unwrap(), &[])
                            .unwrap(),
                        "spec.yaml",
                    )
                    .unwrap();
                    let owner_identity = study.identity(if inherited {
                        "parent/base.yaml"
                    } else {
                        "spec.yaml"
                    });
                    assert_eq!(
                        prepared
                            .run()
                            .document()
                            .declaring_source(&format!("rows.{row_id}.filter")),
                        Some(owner_identity.as_str())
                    );
                    assert_eq!(prepared.capture_reads(), if inherited { 2 } else { 1 });
                    let expected = json!([{"phase":"validation","condition":"invalid_predicate","requirement":"REQ-0188","spec_paths":["rows[1].filter"],"context":context}]);
                    let written = entry.to_str().unwrap();
                    let request = || Request {
                        specification: written,
                        environment: None,
                    };
                    let identity = || Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "independent-row-filter-attribution",
                        specification: written,
                        base_directory: ".",
                    };
                    assert_eq!(
                        issues(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        expected,
                        "{row_id}: check inherited={inherited}"
                    );
                    let built = file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    )
                    .unwrap();
                    assert_eq!(
                        issues(&built.issues()),
                        expected,
                        "{row_id}: build inherited={inherited}"
                    );
                    assert!(built.output().is_none());
                    assert!(!study.0.join("result.csv").exists());
                }
            }
        }
    }
    mod row_intermediate_operands {
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
            fn new(source: &str) -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-row-intermediate-operands-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                fs::write(path.join("spec.yaml"), source).unwrap();
                fs::write(
                    path.join("pp.csv"),
                    "USUBJID,PPTESTCD,DATE\nS1,AUCLST,2030-01-03\nS2,CMAX,2030-02-04\n",
                )
                .unwrap();
                fs::write(path.join("ex.csv"), "USUBJID,EXSPID,EXROUTE,DATE,DTM,N\nS1,PER1-D1,ORAL,2030-01-01,2030-01-01T08:00:00,1.25\nS2,PER2-D1,INTRAVENOUS,2030-02-01,2030-02-01T09:00:00,-2.25\n").unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn path(&self) -> String {
                self.0.join("spec.yaml").to_str().unwrap().into()
            }
            fn run(&self) -> file_application::Domain {
                let path = self.path();
                file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .unwrap_or_else(|issues| panic!("preparation {issues:?}"))
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
                example: "row-intermediate-operands",
                specification: path,
                base_directory: ".",
            }
        }
        fn specification(expression: &str, kind: &str, row_derivation: bool) -> String {
            let declaration = if row_derivation {
                format!("  - {{name: X, type: {kind}}}\nrows:\n  - id: pp_records\n    dataset: PP\n    derivations: {{X: {expression}}}\n")
            } else {
                format!("  - {{name: X, type: {kind}, derivation: {expression}}}\nrows:\n  - id: pp_records\n    dataset: PP\n    derivations: {{}}\n")
            };
            format!("schema_version: '1.0'\ndomain: ADPP\nkeys: [USUBJID]\ninput:\n  PP: {{path: pp.csv, types: {{DATE: date}}}}\n  EX: {{path: ex.csv, types: {{DATE: date, N: float}}}}\nintermediates:\n  - id: DOSE\n    dataset: EX\n    key: [USUBJID]\ncolumns:\n  - {{name: USUBJID, type: str, derivation: PP.USUBJID}}\n{declaration}output: {{path: output.csv, columns: [USUBJID, X]}}\n")
        }

        #[test]
        fn row_and_column_operations_read_named_intermediate_fields() {
            for (expression, kind, values) in [
                (
                    "{str_extract: {source: DOSE.EXSPID, pattern: '^PER([0-9]+)', group: 1}}",
                    "str",
                    "S1,1\nS2,2\n",
                ),
                (
                    "{str_template: '{PP.PPTESTCD}/{DOSE.EXROUTE}'}",
                    "str",
                    "S1,AUCLST/ORAL\nS2,CMAX/INTRAVENOUS\n",
                ),
                (
                    "{str_case: {source: DOSE.EXROUTE, to: lower}}",
                    "str",
                    "S1,oral\nS2,intravenous\n",
                ),
                (
                    "{str_pad: {source: DOSE.EXSPID, width: 9}}",
                    "str",
                    "S1,  PER1-D1\nS2,  PER2-D1\n",
                ),
                (
                    "{to_date: {source: DOSE.DTM}}",
                    "date",
                    "S1,2030-01-01\nS2,2030-02-01\n",
                ),
                (
                    "{datetime_impute: {source: DOSE.DTM, time: first}}",
                    "datetime",
                    "S1,2030-01-01T08:00:00\nS2,2030-02-01T09:00:00\n",
                ),
                (
                    "{round_half_away_from_zero: {source: DOSE.N, digits: 1}}",
                    "float",
                    "S1,1.3\nS2,-2.3\n",
                ),
                (
                    "{date_diff: {start: DOSE.DATE, end: PP.DATE, unit: day}}",
                    "int",
                    "S1,2\nS2,3\n",
                ),
            ] {
                for row_derivation in [false, true] {
                    let source = specification(expression, kind, row_derivation);
                    let study = Study::new(&source);
                    let path = study.path();
                    assert!(file_application::check(
                        yamaa_core::project_function::Language::Python,
                        Request {
                            specification: &path,
                            environment: None
                        },
                        identity(&path)
                    )
                    .is_empty());
                    let mut result = study.run();
                    assert!(
                        result.issues().is_empty(),
                        "{expression}, row={row_derivation}: {:?}",
                        result.issues()
                    );
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read_to_string(study.0.join("output.csv")).unwrap(),
                        format!("USUBJID,X\n{values}"),
                        "{expression}, row={row_derivation}"
                    );
                }
            }
        }

        #[test]
        fn contains_reads_lookup_and_preserves_boolean_conversion_diagnostic() {
            for row_derivation in [false, true] {
                let study = Study::new(&specification(
                    "{str_contains: {source: DOSE.EXSPID, pattern: '^PER'}}",
                    "str",
                    row_derivation,
                ));
                let result = study.run();
                let issues = result.issues();
                assert_eq!(issues.len(), 1, "{issues:?}");
                for issue in issues {
                    assert_eq!(issue.condition, "conversion_failed");
                    assert_eq!(issue.requirement.as_deref(), Some("REQ-0013"));
                    assert_eq!(issue.context.clone()["value"], true);
                }
                assert!(result.output().is_none());
            }
        }

        #[test]
        fn lookup_scalar_operands_preserve_completed_keys_and_missing_field_findings() {
            let source =
                specification("{str_case: {source: DOSE.EXROUTE, to: lower}}", "str", true)
                    .replace("derivations: {X:", "derivations: {USUBJID: PP.USUBJID, X:")
                    .replace(
                        "  - {name: USUBJID, type: str, derivation: PP.USUBJID}",
                        "  - {name: USUBJID, type: str}",
                    );
            let study = Study::new(&source);
            assert!(study.run().issues().is_empty());

            let source = specification(
                "{str_extract: {source: DOSE.MISSING, pattern: '(.)'}}",
                "str",
                false,
            );
            let study = Study::new(&source);
            let issues = study.run().issues();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition, "unknown_field");
            assert_eq!(
                issues[0].spec_paths,
                ["columns.X.derivation.str_extract.source"]
            );
            assert!(study.run().output().is_none());
        }
    }
    mod row_join_keys {
        //! Implicit row joins read raw driver keys; column and named lookups retain their own key scope.
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
        fn rows(issues: &[Issue]) -> yamaa_core::json::Value {
            issues.iter().map(|row| yamaa_adapters::json!({
        "phase": row.phase, "condition": row.condition, "requirement": row.requirement,
        "spec_paths": row.spec_paths, "context": row.context.clone()
    })).collect()
        }
        #[test]
        fn row_joins_preserve_original_driver_key_scope_and_constructed_output_keys() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["row_join_keys"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture(std::env::temp_dir().join(format!(
                    "yamaa-row-join-keys-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                )));
                fs::create_dir(&fixture.0).unwrap();
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
                    example: "row-join-keys",
                    specification: path,
                    base_directory: ".",
                };
                assert_eq!(
                    rows(&file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity()
                    )),
                    case["check_issues"]
                );
                for (input, bytes) in case["inputs"].as_object().unwrap() {
                    let input = fixture.0.join(input);
                    fs::create_dir_all(input.parent().unwrap()).unwrap();
                    fs::write(input, bytes.as_str().unwrap()).unwrap();
                }
                for _ in 0..2 {
                    assert_eq!(
                        rows(&file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity()
                        )),
                        case["check_issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    let mut result = file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    )
                    .unwrap();
                    assert_eq!(
                        rows(&result.issues()),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    if let Some(csv) = case["csv"].as_str() {
                        assert!(result.output().is_some());
                        assert!(result.save().unwrap());
                        assert_eq!(
                            fs::read(fixture.0.join("out.csv")).unwrap(),
                            csv.as_bytes(),
                            "{}",
                            yamaa_adapters::json_wire::encode(&case["name"])
                        );
                        fs::remove_file(fixture.0.join("out.csv")).unwrap();
                    } else {
                        assert!(result.output().is_none());
                        assert!(result.save().is_err());
                        assert!(!fixture.0.join("out.csv").exists());
                    }
                }
            }
        }
    }
    mod scoped_windows_checks {
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
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-scoped-window-checks-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn benchmark(name: &str) -> (Self, PathBuf) {
                fn copy(source: &Path, destination: &Path) {
                    fs::create_dir_all(destination).unwrap();
                    for entry in fs::read_dir(source).unwrap() {
                        let entry = entry.unwrap();
                        if entry.file_type().unwrap().is_dir() {
                            copy(&entry.path(), &destination.join(entry.file_name()));
                        } else {
                            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                        }
                    }
                }
                let study = Self::new();
                let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../benchmarks")
                    .join(name);
                copy(&root.join("input"), &study.0.join("input"));
                fs::copy(root.join("spec.yaml"), study.0.join("spec.yaml")).unwrap();
                (study, root)
            }
            fn build(&self) -> file_application::Domain {
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
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
                        example: "scopes",
                        specification: &path,
                        base_directory: ".",
                    },
                )
                .unwrap()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn original_row_count_fractions_keep_exact_primary_and_governed_log_bytes() {
            let (study, root) = Study::benchmark("schema-row-count-fractions");
            let mut result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            for name in ["adsl.csv", "adsl-checks.csv", "adsl-warnings.csv"] {
                assert_eq!(
                    fs::read(study.0.join(name)).unwrap(),
                    fs::read(root.join("expected").join(name)).unwrap(),
                    "{name}"
                );
            }
        }
        #[test]
        fn original_unnamed_grouped_count_reports_two_retained_groups_and_counts() {
            let (study, _) = Study::benchmark("negative-count-unnamed");
            let result = study.build();
            assert!(result.output().is_none());
            let issues = result.issues();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition, "row_count_failed");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0385"));
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["failure_count"], 2);
            assert_eq!(context["counts"], yamaa_adapters::json!([2, 0]));
            assert_eq!(context["keys"].as_array().unwrap().len(), 2);
        }
        #[test]
        fn original_driver_order_and_ordinal_windows_match_unchanged_saved_outputs() {
            for (name, output) in [
                ("schema-source-ordinal", "records.csv"),
                ("adam-adfa-fever", "adfa.csv"),
                ("sdtm-ec-collected-exposure", "ec.csv"),
                ("sdtm-ex-combination", "ex.csv"),
                ("sdtm-fa-reactogenicity-diary", "fa.csv"),
                ("sdtm-relrec-links", "relrec.csv"),
            ] {
                let (study, root) = Study::benchmark(name);
                let mut result = study.build();
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap(), "{name}");
                assert_eq!(
                    fs::read(study.0.join(output)).unwrap(),
                    fs::read(root.join("expected").join(output)).unwrap(),
                    "{name}"
                );
            }
        }
        #[test]
        fn when_binds_groups_without_changing_the_fraction_denominator() {
            let study = Study::new();
            fs::write(
                study.0.join("input.csv"),
                "I,G,F,B\n1,A,Y,Y\n2,A,N,\n3,B,N,\n4,B,N,\n",
            )
            .unwrap();
            fs::write(study.0.join("spec.yaml"),r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: F, type: str, derivation: SRC.F}
  - {name: B, type: str, derivation: SRC.B}
output: {path: out.csv, columns: [I, G, F, B], verification_log: checks.csv}
verifications:
  - row_count: {group_by: [G], filter: "F = 'Y'", when: "B = 'Y'", min_fraction: 0.5, max_fraction: 0.5}
"#).unwrap();
            let mut result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert!(fs::read_to_string(study.0.join("checks.csv"))
                .unwrap()
                .contains("held,,2,0,{}"));
        }

        #[test]
        fn grouped_warning_retains_all_counts_in_logs_and_samples_only_issue_display() {
            let study = Study::new();
            fs::write(
                study.0.join("input.csv"),
                "I,G,F\n1,A,N\n2,B,N\n3,C,N\n4,D,N\n5,E,N\n6,F,N\n",
            )
            .unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                r#"schema_version: '1.0'
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: F, type: str, derivation: SRC.F}
output: {path: out.csv, columns: [I, G, F], warning_log: warnings.csv}
verifications:
  - row_count: {group_by: [G], filter: "F = 'Y'", min_fraction: 1, severity: warning}
"#,
            )
            .unwrap();
            let mut result = study.build();
            assert!(result.issues().is_empty());
            let check = &result.observations()["verifications"][0];
            assert_eq!(
                check["failure"]["context"]["counts"],
                yamaa_adapters::json!([0, 0, 0, 0, 0])
            );
            assert_eq!(
                check["failure"]["log_context"]["counts"],
                yamaa_adapters::json!([0, 0, 0, 0, 0, 0])
            );
            assert_eq!(
                check["failure"]["log_context"]["denominators"],
                yamaa_adapters::json!([1, 1, 1, 1, 1, 1])
            );
            assert_eq!(
                check["failure"]["offending_keys"].as_array().unwrap().len(),
                6
            );
            assert!(result.save().unwrap());
            assert!(fs::read_to_string(study.0.join("warnings.csv"))
                .unwrap()
                .contains("[0,0,0,0,0,0]"));
        }

        #[test]
        fn original_row_filters_read_named_lookup_state_after_candidate_derivations() {
            for (name, file) in [
                ("adam-adqs-missed-visit-locf", "adqs.csv"),
                ("adam-advs-windows", "advs.csv"),
            ] {
                let (study, root) = Study::benchmark(name);
                let mut result = study.build();
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(study.0.join(file)).unwrap(),
                    fs::read(root.join("expected").join(file)).unwrap(),
                    "{name}"
                );
            }
        }

        #[test]
        fn original_named_intermediate_driver_preserves_filtered_source_order_and_csv_truth() {
            let (study, root) = Study::benchmark("adam-adae-serious-events");
            let mut result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("adae.csv")).unwrap(),
                fs::read(root.join("expected/adae.csv")).unwrap()
            );
        }

        #[test]
        fn named_driver_reads_current_record_and_groups_filtered_rows_in_first_occurrence_order() {
            for grouped in [false, true] {
                let study = Study::new();
                fs::write(
                    study.0.join("input.csv"),
                    "I,G,V,K\n1,B,10,Y\n2,A,20,N\n3,A,30,Y\n4,B,40,Y\n5,C,50,N\n",
                )
                .unwrap();
                let keys = if grouped { "G" } else { "I" };
                let (group, derivations, expected) = if grouped {
                    ("    group_by: [DRV.G]\n", "      I: {literal: 0}\n      G: DRV.G\n      V: {aggregate: {expr: 'SUM(DRV.V)'}}\n", "I,G,V\n0,B,50\n0,A,30\n")
                } else {
                    ("", "      I: {source: {variable: DRV.I}}\n      G: {str_case: {source: DRV.G, to: lower}}\n      V: {source: {variable: DRV.V}}\n", "I,G,V\n1,b,10\n3,a,30\n4,b,40\n")
                };
                fs::write(study.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nkeys: [{keys}]\ninput: {{SRC: {{path: input.csv, types: {{I: int, V: int}}}}}}\nintermediates:\n  - {{id: DRV, dataset: SRC, filter: \"SRC.K = 'Y'\"}}\ncolumns:\n  - {{name: I, type: int}}\n  - {{name: G, type: str}}\n  - {{name: V, type: int}}\nrows:\n  - id: selected\n    dataset: DRV\n{group}    derivations:\n{derivations}output: {{path: out.csv, columns: [I, G, V]}}\n")).unwrap();
                let mut result = study.build();
                assert!(
                    result.issues().is_empty(),
                    "grouped={grouped}: {:?}",
                    result.issues()
                );
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("out.csv")).unwrap(),
                    expected
                );
            }
        }

        #[test]
        fn named_driver_column_projection_rejects_hidden_scalar_and_grouping_fields() {
            for (group, derive, expected_path) in [
                (
                    "",
                    "I: {source: {variable: DRV.I}}",
                    "rows[0].derivations.I.source",
                ),
                (
                    "    group_by: [DRV.I]\n",
                    "I: {literal: 1}",
                    "rows[0].group_by[0]",
                ),
            ] {
                let study = Study::new();
                fs::write(study.0.join("input.csv"), "I,V\n1,10\n").unwrap();
                fs::write(study.0.join("spec.yaml"), format!("schema_version: '1.0'\ndomain: TEST\nkeys: [I]\ninput: {{SRC: {{path: input.csv, types: {{I: int, V: int}}}}}}\nintermediates:\n  - {{id: DRV, dataset: SRC, columns: [V]}}\ncolumns:\n  - {{name: I, type: int}}\nrows:\n  - id: selected\n    dataset: DRV\n{group}    derivations:\n      {derive}\noutput: {{path: out.csv, columns: [I]}}\n")).unwrap();
                let result = study.build();
                assert!(result.output().is_none());
                assert!(
                    result.issues().iter().any(|issue| {
                        issue.condition == "unknown_field"
                            && issue.spec_paths.iter().any(|path| path == expected_path)
                    }),
                    "{:?}",
                    result.issues()
                );
                assert!(!study.0.join("out.csv").exists());
            }
        }

        // Preserve the authored inputs and assertions from the runtime tests removed
        // with the Python evaluator. These cases exercise the shared public build;
        // they do not restore the retired Python planner or its extension hooks.
        #[test]
        fn original_filtered_missing_targets_keep_their_value_in_every_arrival_order() {
            let source = r#"schema_version: "1.0"
domain: EXAMPLE
keys: [GROUP_ID]
input:
  SOURCE: input.csv
output:
  path: example.csv
  columns: [GROUP_ID, TARGET_VALUE]
  order_by: [GROUP_ID]
columns:
  - name: GROUP_ID
    type: str
    label: Group Identifier
    derivation: SOURCE.GroupID
  - name: TARGET_VALUE
    type: str
    label: Target Value
    derivation:
      source:
        filter: "SOURCE.RecordKind = 'TARGET'"
        variable: SOURCE.Value
"#;
            for records in [
                [
                    "G1,UNRELATED,g1-unrelated",
                    "G1,TARGET,",
                    "G2,UNRELATED,g2-unrelated",
                    "G2,TARGET,g2-target",
                ],
                [
                    "G1,TARGET,",
                    "G1,UNRELATED,g1-unrelated",
                    "G2,TARGET,g2-target",
                    "G2,UNRELATED,g2-unrelated",
                ],
                [
                    "G2,UNRELATED,g2-unrelated",
                    "G1,UNRELATED,g1-unrelated",
                    "G2,TARGET,g2-target",
                    "G1,TARGET,",
                ],
            ] {
                let study = Study::new();
                fs::write(study.0.join("spec.yaml"), source).unwrap();
                fs::write(
                    study.0.join("input.csv"),
                    format!("GroupID,RecordKind,Value\n{}\n", records.join("\n")),
                )
                .unwrap();
                let mut result = study.build();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(!study.0.join("example.csv").exists());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(study.0.join("example.csv")).unwrap(),
                    b"GROUP_ID,TARGET_VALUE\nG1,\nG2,g2-target\n"
                );
            }
        }

        fn original_key_identity(source: &str, columns: &str, types: &str) -> String {
            format!(
        "schema_version: '1.0'\ndomain: OUT\ninput: {{SRC: {{path: input.csv, types: {types}}}}}\nkeys: [B, A]\noutput: {{path: out.csv, columns: {columns}}}\ncolumns:\n{source}"
    )
        }

        fn save_csv(study: &Study, mut result: file_application::Domain, path: &str) -> Vec<u8> {
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(!study.0.join(path).exists());
            assert!(result.save().unwrap());
            fs::read(study.0.join(path)).unwrap()
        }

        #[test]
        fn original_reordered_key_names_preserve_typed_values_and_dependencies() {
            for (source, types, input, expected) in [
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "{X: str, Y: str}",
            "X,Y\nfirst,second\nnext,third\nfirst,second\n",
            "A,B\nfirst,second\nnext,third\n",
        ),
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: int, derivation: SRC.Y}\n",
            "{X: str, Y: int}",
            "X,Y\nfirst,2\nnext,3\nfirst,2\n",
            "A,B\nfirst,2\nnext,3\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: int, derivation: {compute: {expr: 'A + 1'}}}\n",
            "{X: int}",
            "X\n4\n2\n4\n",
            "A,B\n4,5\n2,3\n",
        ),
    ] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            original_key_identity(source, "[A, B]", types),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let result = study.build();
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert_eq!(save_csv(&study, result, "out.csv"), expected.as_bytes());
    }
        }

        #[test]
        fn original_missing_key_identity_is_distinct_from_user_text_and_record_order() {
            for (source, columns, types, input) in [
        (
            "  - {name: A, type: str, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "[A, B]",
            "{X: str, Y: str}",
            "X,Y\n,present\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n",
            "[A, B]",
            "{X: int, Y: str}",
            "X,Y\n1,__missing_key__\n,present\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n  - {name: VALUE, type: str, derivation: SRC.Z}\n",
            "[A, B, VALUE]",
            "{X: int, Y: str, Z: str}",
            "X,Y,Z\n,present,missing\n0,__missing_key__,valid\n",
        ),
        (
            "  - {name: A, type: int, derivation: SRC.X}\n  - {name: B, type: str, derivation: SRC.Y}\n  - {name: VALUE, type: str, derivation: SRC.Z}\n",
            "[A, B, VALUE]",
            "{X: int, Y: str, Z: str}",
            "X,Y,Z\n1,__missing_key__,valid\n,present,missing\n",
        ),
    ] {
        let study = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            original_key_identity(source, columns, types),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), input).unwrap();
        let mut result = study.build();
        let issues = result.issues();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].condition, "missing_key");
        assert_eq!(issues[0].spec_paths, ["keys[1]"]);
        assert_eq!(
            issues[0].context.clone(),
            yamaa_adapters::json!({"column": "A", "missing_count": 1,
                "keys": [{"B": "present", "A": null}]})
        );
        assert!(result.output().is_none());
        assert!(result.save().is_err());
        assert!(!study.0.join("out.csv").exists());
    }
        }

        fn original_window_spec(
            extra_columns: &str,
            extra_derivations: &str,
            window: &str,
        ) -> String {
            format!(
        "schema_version: '1.0'\ndomain: OUT\nbase: SRC\ninput: {{SRC: {{path: input.csv, types: {{G: str, X: float, S: int}}}}}}\nkeys: [GRP, SEQ]\noutput: {{path: out.csv, columns: [GRP, SEQ, VAL, PREV]}}\ncolumns:\n  - {{name: GRP, type: str}}\n  - {{name: SEQ, type: int}}\n  - {{name: VAL, type: float}}\n  - {{name: PREV, type: float}}\n{extra_columns}rows:\n  - id: visits\n    derivations:\n      GRP: SRC.G\n      SEQ: SRC.S\n      VAL: SRC.X\n      PREV: {{row_value: {{source: VAL, offset: -1, window: {window}}}}}\n{extra_derivations}"
    )
        }

        fn original_visits(study: &Study, source: &str) -> file_application::Domain {
            fs::write(study.0.join("spec.yaml"), source).unwrap();
            fs::write(
                study.0.join("input.csv"),
                "G,X,S\na,1,1\na,2,2\na,3,3\nb,10,1\nb,20,2\n",
            )
            .unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            result
        }

        fn float_column(result: &file_application::Domain, name: &str) -> Vec<Option<f64>> {
            let table = result.output_table().unwrap();
            let column = table
                .columns()
                .iter()
                .position(|field| field.name() == name)
                .unwrap();
            table
                .batches()
                .iter()
                .flat_map(|batch| {
                    batch
                        .column(column)
                        .as_any()
                        .downcast_ref::<arrow_array::Float64Array>()
                        .unwrap()
                        .iter()
                })
                .collect()
        }

        #[test]
        fn original_row_window_lags_keep_partition_ties_and_template_ownership() {
            let expected = vec![None, Some(1.0), Some(2.0), None, Some(10.0)];
            for window in [
                "{group_by: [GRP], order_by: [SEQ]}",
                "{group_by: [GRP], order_by: [GRP]}",
            ] {
                let study = Study::new();
                let result = original_visits(&study, &original_window_spec("", "", window));
                assert_eq!(float_column(&result, "PREV"), expected);
            }
            let study = Study::new();
            let source = original_window_spec("", "", "{order_by: [SEQ]}").replace(
                "  - id: visits\n",
                "  - id: a_only\n    filter: \"SRC.G = 'a'\"\n",
            );
            let second = source
                .split_once("rows:\n")
                .unwrap()
                .1
                .replace("a_only", "b_only")
                .replace("SRC.G = 'a'", "SRC.G = 'b'");
            let result = original_visits(&study, &format!("{source}{second}"));
            assert_eq!(float_column(&result, "PREV"), expected);
        }

        #[test]
        fn original_row_window_chains_complete_scalars_before_later_window_reads() {
            let window = "{group_by: [GRP], order_by: [SEQ]}";
            for (columns, derivations, column, expected) in [
        (
            "  - {name: PREV2, type: float}\n",
            "      PREV2: {row_value: {source: PREV, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n",
            "PREV2",
            vec![None, None, Some(1.0), None, None],
        ),
        (
            "  - {name: CHG, type: float}\n  - {name: DOUBLED, type: float}\n",
            "      DOUBLED: {compute: {expr: 'CHG * 2'}}\n      CHG: {compute: {expr: 'VAL - PREV'}}\n",
            "DOUBLED",
            vec![None, Some(2.0), Some(2.0), None, Some(20.0)],
        ),
        (
            "  - {name: DOUBLED, type: float}\n  - {name: BAD, type: float}\n",
            "      DOUBLED: {compute: {expr: 'PREV * 2'}}\n      BAD: {row_value: {source: DOUBLED, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n",
            "BAD",
            vec![None, None, Some(2.0), None, None],
        ),
    ] {
        let study = Study::new();
        let source = original_window_spec(columns, derivations, window)
            .replace("columns: [GRP, SEQ, VAL, PREV]", &format!("columns: [GRP, SEQ, VAL, PREV, {column}]"));
        let result = original_visits(&study, &source);
        assert_eq!(float_column(&result, column), expected);
    }
        }

        #[test]
        fn original_windows_in_one_case_column_keep_distinct_orderings() {
            let study = Study::new();
            let source = original_window_spec("", "", "{group_by: [GRP], order_by: [SEQ]}")
        .replace("  - {name: PREV, type: float}", r#"  - name: PREV
    type: float
    derivation:
      case:
        - when: SEQ = 1
          then: {row_value: {source: VAL, offset: 1, window: {group_by: [GRP], order_by: [SEQ]}}}
        - otherwise: {row_value: {source: VAL, offset: 1, window: {group_by: [GRP], order_by: [{variable: SEQ, direction: desc}]}}}"#)
        .replace("      PREV: {row_value: {source: VAL, offset: -1, window: {group_by: [GRP], order_by: [SEQ]}}}\n", "");
            let result = original_visits(&study, &source);
            assert_eq!(
                float_column(&result, "PREV"),
                [Some(2.0), Some(1.0), Some(2.0), Some(20.0), Some(10.0)]
            );
        }

        #[test]
        fn original_source_ordinals_follow_physical_swaps_without_adding_output_fields() {
            for labels in [["ROW_B", "ROW_A"], ["ROW_A", "ROW_B"]] {
                let (study, _) = Study::benchmark("schema-source-ordinal");
                fs::write(
                    study.0.join("input/source.csv"),
                    format!(
                        "GroupID,RecordLabel,Include\nG1,{},Y\nG1,{},Y\n",
                        labels[0], labels[1]
                    ),
                )
                .unwrap();
                let result = study.build();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert_eq!(
                    save_csv(&study, result, "records.csv"),
                    format!(
                        "GROUP_ID,ROW_SEQUENCE,RECORD_LABEL,SOURCE_ORDINAL\nG1,1,{},1\nG1,2,{},2\n",
                        labels[0], labels[1]
                    )
                    .as_bytes()
                );
            }
            let (study, _) = Study::benchmark("schema-source-ordinal");
            let source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
            let (before, after) = source.split_once("  - name: SOURCE_ORDINAL\n").unwrap();
            let rows = after.split_once("rows:\n").unwrap().1;
            let source = format!("{before}rows:\n{rows}").replace(", SOURCE_ORDINAL]", "]");
            fs::write(study.0.join("spec.yaml"), source).unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL\nG1,1,ROW_B\nG1,2,ROW_A\nG1,3,ROW_A\nG2,1,ROW_B\nG2,2,ROW_A\n"
    );
        }

        #[test]
        fn original_ordinal_windows_keep_input_positions_across_reused_templates() {
            let (study, _) = Study::benchmark("schema-source-ordinal");
            let mut source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
            source.push_str(
        "  - id: second\n    dataset: SOURCE\n    filter: SOURCE.SourceOrdinal = 1\n    derivations: {}\n",
    );
            fs::write(study.0.join("spec.yaml"), source).unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL,SOURCE_ORDINAL\nG1,1,ROW_B,2\nG1,2,ROW_A,5\nG1,3,ROW_A,6\nG2,1,ROW_B,1\nG2,2,ROW_B,1\nG2,3,ROW_A,4\n"
    );
        }

        #[test]
        fn original_ordinal_group_reductions_run_before_group_numbering() {
            let (study, _) = Study::benchmark("schema-source-ordinal");
            fs::write(
                study.0.join("spec.yaml"),
                r#"schema_version: "1.0"
domain: RECORDS
keys: [GROUP_ID]
input:
  SOURCE: {path: input/source.csv, ordinal: SourceOrdinal}
output:
  path: records.csv
  columns: [GROUP_ID, SOURCE_ORDINAL, ROW_SEQUENCE]
  order_by: [GROUP_ID, ROW_SEQUENCE]
columns:
  - {name: GROUP_ID, type: str, derivation: SOURCE.GroupID}
  - {name: ROW_SEQUENCE, type: int}
  - {name: SOURCE_ORDINAL, type: int}
rows:
  - id: included
    dataset: SOURCE
    group_by: [SOURCE.GroupID]
    derivations:
      SOURCE_ORDINAL: {aggregate: "MIN(SOURCE.SourceOrdinal)"}
      ROW_SEQUENCE: {row_number: {window: {order_by: [SOURCE_ORDINAL]}}}
"#,
            )
            .unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
                save_csv(&study, result, "records.csv"),
                b"GROUP_ID,SOURCE_ORDINAL,ROW_SEQUENCE\nG1,2,2\nG2,1,1\n"
            );
        }
    }
    mod temporal_arithmetic {
        //! The same authored conformance vectors run through Rust, installed Python and R.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::domain_entry::Request;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-temporal-{}-{}",
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
        fn assert_issues(issues: &[Issue], expected: &yamaa_core::json::Value, name: &str) {
            if expected.is_null() {
                assert!(issues.is_empty(), "{name}: {issues:?}");
            } else {
                assert!(
                    issues.iter().any(|issue| {
                        issue.phase == expected["phase"].as_str().unwrap()
                            && issue.condition == expected["condition"].as_str().unwrap()
                            && issue.requirement.as_deref() == expected["requirement"].as_str()
                            && issue.spec_paths == [expected["spec_path"].as_str().unwrap()]
                    }),
                    "{name}: {issues:?}"
                );
            }
        }

        #[test]
        fn temporal_operations_match_shared_host_truth_before_and_after_source_capture() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["temporal_arithmetic"]
                .as_array()
                .unwrap()
            {
                let name = case["name"].as_str().unwrap();
                let fixture = Fixture::new();
                let entry = fixture.0.join("spec.yaml");
                fs::write(&entry, case["spec"].as_str().unwrap()).unwrap();
                let specification = entry.to_str().unwrap();
                let identity = || Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: name,
                    specification,
                    base_directory: ".",
                };
                let request = || Request {
                    specification,
                    environment: None,
                };
                // Invalid types are rejected while every input file is still absent.
                assert_issues(
                    &file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(),
                    ),
                    &case["check_issue"],
                    name,
                );
                for (path, text) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(path), text.as_str().unwrap()).unwrap();
                }
                match file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(),
                ) {
                    Ok(mut domain) => {
                        assert_issues(&domain.issues(), &case["run_issue"], name);
                        let reached: u64 = domain.observations()["handler_counts"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|item| item["handler"] == "invalid")
                            .map(|item| item["count"].as_u64().unwrap())
                            .sum();
                        assert_eq!(reached, case["invalid_count"].as_u64().unwrap(), "{name}");
                        if let Some(csv) = case["csv"].as_str() {
                            assert!(domain.save().unwrap(), "{name}");
                            assert_eq!(
                                fs::read(fixture.0.join("result.csv")).unwrap(),
                                csv.as_bytes(),
                                "{name}"
                            );
                        } else {
                            assert!(domain.output().is_none(), "{name}");
                            assert!(!fixture.0.join("result.csv").exists());
                        }
                    }
                    Err(issues) => assert_issues(&issues, &case["run_issue"], name),
                }
            }
        }
    }
    mod typed_intermediate_derivations {
        //! Independent YAML, donor bytes and exact saved truth for per-record aggregates.
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
                    "yamaa-typed-intermediates-{}-{}",
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
        #[test]
        fn typed_intermediate_conversion_and_static_checks_match_independent_host_truth() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["original-specifications"]["typed_intermediates"]
                .as_array()
                .unwrap()
            {
                let fixture = Fixture::new();
                let name = case["name"].as_str().unwrap();
                fs::write(fixture.0.join("spec.yaml"), case["spec"].as_str().unwrap()).unwrap();
                // Checking uses declaration types and never reads the absent CSV files.
                let specification = fixture.0.join("spec.yaml").to_str().unwrap().to_owned();
                let issues = file_application::check(
                    yamaa_core::project_function::Language::Python,
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
                );
                if let Some(condition) = case["check_condition"].as_str() {
                    assert!(
                        issues.iter().any(|issue| issue.condition == condition),
                        "{name}: {issues:?}"
                    );
                    if condition == "incompatible_input_type" {
                        let issue = issues
                            .iter()
                            .find(|issue| issue.condition == condition)
                            .unwrap();
                        assert_eq!(issue.requirement.as_deref(), Some("REQ-0323"));
                        assert_eq!(issue.spec_paths, ["intermediates[0].key"]);
                    }
                } else {
                    assert!(issues.is_empty(), "{name}: {issues:?}");
                }
                for (path, content) in case["inputs"].as_object().unwrap() {
                    fs::write(fixture.0.join(path), content.as_str().unwrap()).unwrap();
                }
                if let Some(expected) = case["csv"].as_str() {
                    assert_eq!(fixture.save(), expected.as_bytes(), "{name}");
                }
                match fixture.run() {
                    Ok(domain) => {
                        let issues = domain.issues();
                        if let Some(condition) = case["run_condition"].as_str() {
                            assert!(
                                issues.iter().any(|issue| issue.condition == condition),
                                "{name}: {issues:?}"
                            );
                        } else {
                            assert!(issues.is_empty(), "{name}: {issues:?}");
                        }
                        let observations = domain.observations();
                        let count: u64 = observations["handler_counts"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|handler| handler["handler"] == "unconvertible")
                            .map(|handler| handler["count"].as_u64().unwrap())
                            .sum();
                        assert_eq!(
                            count,
                            case["unconvertible_count"].as_u64().unwrap(),
                            "{name}: {}",
                            yamaa_adapters::json_wire::encode(&observations)
                        );
                    }
                    Err(issues) => {
                        let condition = case["run_condition"].as_str().unwrap();
                        assert!(
                            issues.iter().any(|issue| issue.condition == condition),
                            "{name}: {issues:?}"
                        );
                    }
                }
            }
        }

        #[test]
        fn transitive_nested_conversion_handlers_survive_success_and_failure() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            let case = truth["families"]["original-specifications"]["typed_intermediates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["name"] == "nested-conversion-ledger")
                .unwrap();
            // QSV -> FIRST -> MIDDLE -> DEEP crosses several donor counters.
            let specification = case["spec"]
        .as_str()
        .unwrap()
        .replace(
            "intermediates:\n",
            "intermediates:\n  - id: DEEP\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: {literal: bad}, unconvertible: 2}\n    order_by: [VS.VSSEQ]\n    keep: first\n  - id: MIDDLE\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: DEEP.PICK\n    order_by: [VS.VSSEQ]\n    keep: first\n",
        )
        .replace(
            "  - id: FIRST\n    dataset: VS\n    key: {USUBJID: QS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: {literal: bad}, unconvertible: 2}",
            "  - id: FIRST\n    dataset: VS\n    key: {USUBJID: QS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: MIDDLE.PICK, unconvertible: 2}",
        );
            for (replacement, expected_count) in [("2", 3), ("bad-replacement", 1)] {
                let fixture = Fixture::new();
                for (path, content) in case["inputs"].as_object().unwrap() {
                    let mut content = content.as_str().unwrap().to_owned();
                    if path == "qs.csv" {
                        // The outer donor's own typed handler must still count once.
                        content.push_str("S01,WEEK 9223372036854775808,0\n");
                    }
                    fs::write(fixture.0.join(path), content).unwrap();
                }
                fs::write(
                    fixture.0.join("spec.yaml"),
                    specification.replacen(
                        "unconvertible: 2",
                        &format!("unconvertible: {replacement}"),
                        1,
                    ),
                )
                .unwrap();
                let mut domain = fixture.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                if replacement == "2" {
                    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                    assert!(domain.save().unwrap());
                    assert_eq!(
                        fs::read(fixture.0.join("result.csv")).unwrap(),
                        case["csv"].as_str().unwrap().as_bytes(),
                    );
                } else {
                    assert!(
                        domain
                            .issues()
                            .iter()
                            .any(|issue| issue.condition == "conversion_failed"
                                && issue.spec_paths
                                    == ["intermediates[0].derivations.PICK.unconvertible"]),
                        "{:?}",
                        domain.issues()
                    );
                }
                let observations = domain.observations();
                let counts = observations["handler_counts"].as_array().unwrap();
                assert_eq!(
                    counts
                        .iter()
                        .find(|entry| entry["spec_path"]
                            == "intermediates[0].derivations.PICK.unconvertible")
                        .unwrap()["count"],
                    expected_count,
                );
                assert_eq!(
                    counts
                        .iter()
                        .find(|entry| entry["spec_path"]
                            == "intermediates[3].derivations.QSVISITN.unconvertible")
                        .unwrap()["count"],
                    1,
                );
                assert_eq!(
                    counts
                        .iter()
                        .filter(|entry| entry["handler"] == "unconvertible")
                        .map(|entry| entry["count"].as_u64().unwrap())
                        .sum::<u64>(),
                    expected_count + 1,
                );
            }
        }
    }
}
