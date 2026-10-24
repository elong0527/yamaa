#![cfg(feature = "yaml")]
//! Execution contracts read from shared truth: retired ODM context, row
//! count lookups, row derivations, row join keys, scoped windows checks,
//! temporal arithmetic and typed intermediate derivations. The family's
//! other cases are in `tests/fixtures/diagnostics/`: `odm_source_reads.json`,
//! `row_derivations.json`, `row_intermediate_operands.json` and
//! `row_scopes.json` (#3798). The tests left here read shared benchmark or
//! host-fixture truth, or prove the API and observation seams their comments
//! name. The remaining benchmark-copy cases predate benchmark staging in
//! the replay (#3875) and remain a follow-up of #3798.
mod execution {
    mod retired_odm_context {
        //! Original retired ODM-context operands through the shared public file engine.
        //! Generic source reads keep their ordinary relation semantics; only `odm`
        //! expressions select the fixed eleven-field ODM interpretation.
        //!
        //! Its other cases are `odm_source_reads.json`'s (#3798). The two
        //! tests here stay beside the replay, each for the seam its first
        //! comment names.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_adapters::json;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new(specification: &str, input: &str) -> Self {
                let root = crate::support::Scratch::new("retired-odm");
                fs::write(root.join("spec.yaml"), specification).unwrap();
                fs::write(root.join("odm.csv"), input).unwrap();
                Self(root)
            }
            fn run(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
                let specification = self.0.join("spec.yaml");
                let specification = specification.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification,
                        environment: None,
                    },
                    crate::support::study::identity("retired-odm-context", specification),
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

        fn specification(key: &str, kind: &str, expression: &str, input: &str) -> String {
            format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: OUT\nkeys: [{key}]\ninput: {{ODM: {input}}}\noutput: {{path: out.csv, columns: [{key}, VALUE]}}\ncolumns:\n  - {{name: {key}, type: str, derivation: ODM.{key}}}\n  - {{name: VALUE, type: {kind}, derivation: {expression}}}\n"
    )
        }

        const HEADER: &str = "StudyOID,MetaDataVersionOID,SubjectKey,StudyEventOID,StudyEventRepeatKey,FormOID,FormRepeatKey,ItemGroupOID,ItemGroupRepeatKey,ItemOID,Value";
        const RECORD: &str = "S1,M1,001,SCREENING,1,FO.DM,1,IG.DM,1,IT.DM.AGE,34";

        /// A unit seam: the catalog that `schema-odm-form-items`'s prepared
        /// specification binds, and the number of sources it takes, which no
        /// run of the benchmark shows.
        #[test]
        fn original_binding_fixture_keeps_output_names_literal_fields_and_complete_source_count() {
            use yamaa_adapters::specification_run::PreparedRun;
            use yamaa_core::{
                reference_binding::{Binding, Catalog, Dataset, Field},
                specification::BindError,
                table::{Column, TableSchema},
                value::ColumnType,
            };
            let original =
                crate::support::benchmarks::path("schema-odm-form-items").join("spec.yaml");
            // The benchmark in the language it is written in (#3422).
            let document = crate::support::specification_run::prepare(&fs::read(original).unwrap());
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

        /// A seam the replay cannot reach: the source table the run's
        /// observations record, which no replay operation reports.
        /// `schema-odm-item-sample` reads schema names written in capitals
        /// beside vendor fields. No benchmark states the source table a run
        /// reports: the eleven schema fields under their own names, the vendor
        /// field left out.
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
    }
    mod row_count_lookups {
        //! Original row-count verification lookups and declaration ordering.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read, and it compares the observations'
        //! verification records, which no replay operation reports.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn issues(rows: &[yamaa_core::diagnostic::Diagnostic]) -> yamaa_core::json::Value {
            yamaa_adapters::json!(rows
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>())
        }
        /// No benchmark counts rows through a lookup in a row count's filter
        /// or `when`. The cases are the host fixtures' shared truth, which the
        /// Python and R suites read too.
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
                let fixture = Fixture(crate::support::Scratch::new("row-count-lookups"));
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
                let identity = || crate::support::study::identity("row_count_lookups", path);
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
                let mut result = crate::support::study::domain(
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
        //! Its other cases are `row_derivations.json`'s (#3798). This test
        //! stays a benchmark reader through `support::benchmarks`, which
        //! checks that benchmark's own authored expected files.
        /// `sdtm-vs-units` builds its rows through row-template cases and
        /// numeric branches, and states the CSV it saves.
        #[test]
        fn unchanged_vs_units_benchmark_runs_row_cases_and_numeric_branches() {
            crate::support::benchmarks::reproduce("sdtm-vs-units");
        }
    }
    mod row_join_keys {
        //! Implicit row joins read raw driver keys; column and named lookups retain their own key scope.
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read, and it checks each specification
        //! before and after its inputs exist and runs each case twice, where a
        //! replay case writes its files once and runs once.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        fn rows(issues: &[Diagnostic]) -> yamaa_core::json::Value {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect()
        }
        /// No benchmark states `check`'s issues beside the run's, as each of
        /// these cases does. The cases are the host fixtures' shared truth,
        /// which the Python and R suites read too.
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
                let fixture = Fixture(crate::support::Scratch::new("row-join-keys"));
                let path = fixture.0.join("spec.yaml");
                fs::write(&path, case["specification"].as_str().unwrap()).unwrap();
                let path = path.to_str().unwrap();
                let request = || Request {
                    specification: path,
                    environment: None,
                };
                let identity = || crate::support::study::identity("row-join-keys", path);
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
                    let mut result = crate::support::study::domain(
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
        //! Its other cases are `row_scopes.json`'s (#3798). The tests here
        //! stay as benchmark readers, benchmark-copy cases, or checks of the
        //! observations' verification record, which no replay operation
        //! reports. The replay can stage benchmarks since #3875; the four
        //! benchmark-copy tests are a follow-up of #3798.
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("scoped-window-checks");
                Self(path)
            }
            /// A study holding a copy of the benchmark `name`'s specification
            /// and inputs, for a test that runs or edits them itself.
            fn benchmark(name: &str) -> Self {
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
                let root = crate::support::benchmarks::path(name);
                copy(&root.join("input"), &study.0.join("input"));
                fs::copy(root.join("spec.yaml"), study.0.join("spec.yaml")).unwrap();
                study
            }
            fn build(&self) -> file_application::Domain {
                let path = self.0.join("spec.yaml").to_str().unwrap().to_owned();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    crate::support::study::identity("scopes", &path),
                )
                .unwrap()
            }
        }
        /// A benchmark reader (`support::benchmarks::reproduce`).
        /// `schema-row-count-fractions` states the dataset it saves and its
        /// report: row counts bounded by share, with and without groups, and
        /// one at `warning`.
        #[test]
        fn original_row_count_fractions_keep_exact_primary_and_report_bytes() {
            crate::support::benchmarks::reproduce("schema-row-count-fractions");
        }
        /// A run of a copy of the benchmark's own files.
        /// `negative-count-unnamed`'s `error.yaml` states the finding this run
        /// reports, and leaves out that it is the run's only issue.
        #[test]
        fn original_unnamed_grouped_count_reports_two_retained_groups_and_counts() {
            let study = Study::benchmark("negative-count-unnamed");
            let result = study.build();
            assert!(result.output().is_none());
            let issues = result.issues();
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition(), "row_count_failed");
            assert_eq!(issues[0].rule(), Some("CHECK-13"));
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["failure_count"], 2);
            assert_eq!(context["counts"], yamaa_adapters::json!([2, 0]));
            assert_eq!(context["keys"].as_array().unwrap().len(), 2);
        }
        /// A benchmark reader (`support::benchmarks::reproduce`).
        /// Each benchmark builds its rows in its drivers' stored order,
        /// numbered by a window or a source ordinal where it numbers them, and
        /// states the CSV it saves.
        #[test]
        fn original_driver_order_and_ordinal_windows_match_unchanged_saved_outputs() {
            for name in [
                "schema-source-ordinal",
                "adam-adfa-fever",
                "sdtm-ec-collected-exposure",
                "sdtm-ex-combination",
                "sdtm-fa-reactogenicity-diary",
                "sdtm-relrec-links",
            ] {
                crate::support::benchmarks::reproduce(name);
            }
        }

        /// The observations' verification record, which no replay operation
        /// reports: the counts its issue display samples beside the ones it
        /// keeps.
        /// `schema-row-count-fractions` reports a warning row count without
        /// groups. No benchmark reports a grouped one, whose report keeps
        /// every group's count while its issue samples them.
        #[test]
        fn grouped_warning_retains_all_counts_in_the_report_and_samples_only_issue_display() {
            let study = Study::new();
            fs::write(
                study.0.join("input.csv"),
                "I,G,F\n1,A,N\n2,B,N\n3,C,N\n4,D,N\n5,E,N\n6,F,N\n",
            )
            .unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                r#"yamaa: "1.0"
type: spec
domain: TEST
keys: [I]
input: {SRC: input.csv}
columns:
  - {name: I, type: int, derivation: SRC.I}
  - {name: G, type: str, derivation: SRC.G}
  - {name: F, type: str, derivation: SRC.F}
output: {path: out.csv, columns: [I, G, F], report: report.csv}
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
            assert!(fs::read_to_string(study.0.join("report.csv"))
                .unwrap()
                .contains("[0,0,0,0,0,0]"));
        }

        /// A benchmark reader (`support::benchmarks::reproduce`).
        /// Each benchmark filters a row template on a named lookup's state
        /// after the template's candidate derivations, and states the CSV it
        /// saves.
        #[test]
        fn original_row_filters_read_named_lookup_state_after_candidate_derivations() {
            for name in ["adam-adqs-missed-visit-locf", "adam-advs-windows"] {
                crate::support::benchmarks::reproduce(name);
            }
        }

        /// A benchmark reader (`support::benchmarks::reproduce`).
        /// `adam-adae-serious-events` builds rows from a filtered named
        /// intermediate in its records' stored order, and states the CSV it
        /// saves.
        #[test]
        fn original_named_intermediate_driver_preserves_filtered_source_order_and_csv_truth() {
            crate::support::benchmarks::reproduce("adam-adae-serious-events");
        }

        // Preserve the authored inputs and assertions from the runtime tests removed
        // with the Python evaluator. These cases exercise the shared public build;
        // they do not restore the retired Python planner or its extension hooks.
        fn save_csv(study: &Study, mut result: file_application::Domain, path: &str) -> Vec<u8> {
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(!study.0.join(path).exists());
            assert!(result.save().unwrap());
            fs::read(study.0.join(path)).unwrap()
        }

        /// Runs of a copy of the benchmark's own files, edited.
        /// `schema-source-ordinal` numbers its records as stored. No benchmark
        /// swaps records to move their numbers, or orders rows by an ordinal
        /// it does not output.
        #[test]
        fn original_source_ordinals_follow_physical_swaps_without_adding_output_fields() {
            for labels in [["ROW_B", "ROW_A"], ["ROW_A", "ROW_B"]] {
                let study = Study::benchmark("schema-source-ordinal");
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
            let study = Study::benchmark("schema-source-ordinal");
            let source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
            let (before, after) = source.split_once("  - name: SOURCE_ORDINAL\n").unwrap();
            let rows = after.split_once("rows:\n").unwrap().1;
            let source = format!("{before}rows:\n{rows}").edit(", SOURCE_ORDINAL]", "]");
            fs::write(study.0.join("spec.yaml"), source).unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL\nG1,1,ROW_B\nG1,2,ROW_A\nG1,3,ROW_A\nG2,1,ROW_B\nG2,2,ROW_A\n"
    );
        }

        /// A run of a copy of the benchmark's own files, edited.
        /// `schema-source-ordinal` builds its rows in one template. No
        /// benchmark reads an ordinal input in a second template.
        #[test]
        fn original_ordinal_windows_keep_input_positions_across_reused_templates() {
            let study = Study::benchmark("schema-source-ordinal");
            let mut source = fs::read_to_string(study.0.join("spec.yaml")).unwrap();
            source.push_str(
                "  - id: second\n    dataset: SOURCE\n    filter: SOURCE.SourceOrdinal = 1\n",
            );
            fs::write(study.0.join("spec.yaml"), source).unwrap();
            let result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert_eq!(
        save_csv(&study, result, "records.csv"),
        b"GROUP_ID,ROW_SEQUENCE,RECORD_LABEL,SOURCE_ORDINAL\nG1,1,ROW_B,2\nG1,2,ROW_A,5\nG1,3,ROW_A,6\nG2,1,ROW_B,1\nG2,2,ROW_B,1\nG2,3,ROW_A,4\n"
    );
        }

        /// A run of the benchmark's own input under another specification.
        /// No benchmark reduces an ordinal in a grouped template.
        #[test]
        fn original_ordinal_group_reductions_run_before_group_numbering() {
            let study = Study::benchmark("schema-source-ordinal");
            fs::write(
                study.0.join("spec.yaml"),
                r#"yamaa: "1.0"
type: spec
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
        //!
        //! This stays beside the replay (#3798): its truth is a host-fixture
        //! family both installed hosts read, and it checks each specification
        //! before its inputs exist, then runs it with them.
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("retired-odm-temporal");
                Self(path)
            }
        }
        fn assert_issues(issues: &[Diagnostic], expected: &yamaa_core::json::Value, name: &str) {
            if expected.is_null() {
                assert!(issues.is_empty(), "{name}: {issues:?}");
            } else {
                assert!(
                    issues.iter().any(|issue| {
                        issue.phase() == expected["phase"].as_str().unwrap()
                            && issue.condition() == expected["condition"].as_str().unwrap()
                            && issue.rule() == expected["rule"].as_str()
                            && issue.spec_paths == [expected["spec_path"].as_str().unwrap()]
                    }),
                    "{name}: {issues:?}"
                );
            }
        }

        /// No benchmark counts a temporal operation's `invalid` answers, as
        /// each of these cases does. The cases are the host fixtures' shared
        /// truth, which the Python and R suites read too.
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
                let identity = || crate::support::study::identity(name, specification);
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
                match crate::support::study::domain(
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
        //!
        //! These stay beside the replay (#3798): their truth is a host-fixture
        //! family both installed hosts read, which the second test edits, and
        //! the first checks each specification before its inputs exist.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::file_application;
        use yamaa_core::domain_entry::Request;
        struct Fixture(crate::support::Scratch);
        impl Fixture {
            fn new() -> Self {
                let path = crate::support::Scratch::new("typed-intermediates");
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
            fn save(&self) -> Vec<u8> {
                let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
                assert!(domain.issues().is_empty(), "{:?}", domain.issues());
                assert!(domain.save().unwrap());
                fs::read(self.0.join("result.csv")).unwrap()
            }
        }
        /// No benchmark counts an intermediate derivation's `unconvertible`
        /// answers, as each of these cases does. The cases are the host
        /// fixtures' shared truth, which the Python and R suites read too.
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
                    crate::support::study::identity(name, &specification),
                );
                if let Some(condition) = case["check_condition"].as_str() {
                    assert!(
                        issues.iter().any(|issue| issue.condition() == condition),
                        "{name}: {issues:?}"
                    );
                    if condition == "incompatible_input_type" {
                        let issue = issues
                            .iter()
                            .find(|issue| issue.condition() == condition)
                            .unwrap();
                        assert_eq!(issue.rule(), Some("LOOKUP-9"));
                        assert_eq!(issue.spec_paths, ["intermediates.QSV.key"]);
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
                                issues.iter().any(|issue| issue.condition() == condition),
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
                            issues.iter().any(|issue| issue.condition() == condition),
                            "{name}: {issues:?}"
                        );
                    }
                }
            }
        }

        /// No benchmark counts `unconvertible` answers, here through donor
        /// counters three typed derivations deep; the specification edits the
        /// host fixtures' `nested-conversion-ledger` case.
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
        .edit(
            "intermediates:\n",
            "intermediates:\n  - id: DEEP\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: \"'bad'\", unconvertible: 2}\n    order_by: [VS.VSSEQ]\n    keep: first\n  - id: MIDDLE\n    dataset: VS\n    key: {USUBJID: VS.USUBJID}\n    derivations:\n      PICK: DEEP.PICK\n    order_by: [VS.VSSEQ]\n    keep: first\n",
        )
        .edit(
            "  - id: FIRST\n    dataset: VS\n    key: {USUBJID: QS.USUBJID}\n    derivations:\n      PICK: {type: int, derivation: \"'bad'\", unconvertible: 2}",
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
                            .any(|issue| issue.condition() == "conversion_failed"
                                && issue.spec_paths
                                    == ["intermediates.DEEP.derivations.PICK.unconvertible"]),
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
                            == "intermediates.DEEP.derivations.PICK.unconvertible")
                        .unwrap()["count"],
                    expected_count,
                );
                assert_eq!(
                    counts
                        .iter()
                        .find(|entry| entry["spec_path"]
                            == "intermediates.QSV.derivations.QSVISITN.unconvertible")
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
