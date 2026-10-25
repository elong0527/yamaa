#![cfg(feature = "yaml")]
//! Project contracts: domain submission.
mod project {
    mod domain_submission {
        use crate::support::ports::NoFunctions;
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_application::{file_application, specification_report::Identity};
        use yamaa_core::application_observer::Scope;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let root = crate::support::Scratch::new("domain-submission");
                Self(root.resolved(std::fs::canonicalize))
            }
            fn check(&self, raw: &str) -> Vec<yamaa_core::diagnostic::Diagnostic> {
                fs::write(self.0.join("spec.yaml"), raw).unwrap();
                let path = self.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
            }
        }
        fn identity(path: &str) -> Identity<'_> {
            crate::support::study::identity("submission", path)
        }
        /// An SDTM column's submission metadata: a 1.0 specification that
        /// declares it names its model (SUB-3), which requires the origin's
        /// source (SUB-15).
        const VALID: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: str\n    derivation: SRC.VALUE\n    submission: {length: 3, origins: [{type: Assigned, source: Sponsor}]}\n";
        /// One closed CDISC vocabulary, in the order of the data file that
        /// holds them all (P-VOC-1).
        fn vocabulary(name: &str) -> Vec<String> {
            use yamaa_core::schema::DocumentNode as N;
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("yaml/vocabulary/cdisc.yaml");
            let bytes = fs::read(path).unwrap();
            let decoded = yamaa_adapters::yaml_decode::decode_yaml(&bytes).unwrap();
            let document = &decoded.document;
            let entries = document
                .field(document.root(), "vocabularies")
                .and_then(|vocabularies| document.field(vocabularies, name))
                .and_then(|vocabulary| document.field(vocabulary, "values"))
                .unwrap_or_else(|| panic!("{name} has no values"));
            let N::Sequence(items) = &document.nodes()[entries] else {
                panic!("{name}: values is not a list")
            };
            items
                .iter()
                .map(|&item| match document.field(item, "value") {
                    Some(value) => match &document.nodes()[value] {
                        N::Text(text) => text.clone(),
                        other => panic!("{name}: {other:?}"),
                    },
                    None => panic!("{name}: an entry has no value"),
                })
                .collect()
        }
        /// Seam: the roles are read from the closed vocabulary's data file
        /// (P-VOC-1), so each value it lists is checked without a fixture
        /// case copying the list. SUB-9: every role the data file lists is
        /// admitted where the specification is read, and any other fails
        /// there, as `submission.json`'s three refused roles do.
        #[test]
        fn a_role_outside_the_nine_fails_where_the_specification_is_read() {
            let study = Study::new();
            let with_role =
                |role: &str| VALID.edit("submission: {", &format!("submission: {{role: {role}, "));
            let roles = vocabulary("role");
            assert_eq!(roles.len(), 9);
            for role in &roles {
                assert!(study.check(&with_role(role)).is_empty(), "{role}");
            }
            assert!(!study.0.join("output.csv").exists());
        }
        /// Seam: the standard names are read from the closed vocabulary's
        /// data file (P-VOC-1), so each value it lists is checked without a
        /// fixture case copying the list. CT-2: a terminology standard's name
        /// is one of the standard names the data file lists, refused where
        /// the project is read, before any document is generated, as
        /// `submission.json`'s CDISC CT is.
        #[test]
        fn a_terminology_standard_name_outside_the_vocabulary_fails_where_the_project_is_read() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            let check = |name: &str| {
                fs::write(
                    study.0.join("project.yaml"),
                    format!(
                        "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - standard: {{name: {name}, \
                         publishing_set: SDTM, version: '2026-03-27'}}\n    codelists:\n      \
                         - {{id: SEX, name: Sex, items: [{{value: F}}, {{value: M}}]}}\nsdtm:\n  \
                         specs: [spec.yaml]\n"
                    ),
                )
                .unwrap();
                let path = study.0.join("spec.yaml");
                let project = study.0.join("project.yaml");
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path.to_str().unwrap(),
                        environment: Some(project.to_str().unwrap()),
                    },
                    identity(path.to_str().unwrap()),
                )
            };
            let names = vocabulary("standard_name");
            for name in &names {
                assert!(check(name).is_empty(), "{name}");
            }
            assert!(!study.0.join("output.csv").exists());
        }
        /// Seam: single check rows of the report file a save writes, a
        /// refused one included (CHECK-22), found as substrings. A case
        /// states a report kind's rows whole and a saved file's bytes whole,
        /// and CHECK-23 leaves the order of one column's check rows open,
        /// so these rows cannot be authored as a case.
        /// SUB-26: a `str` column's date or time data type is a form each
        /// present completed value must have; an entry's data type decides
        /// the rows its where clause selects, and the column's every other row.
        #[test]
        fn date_and_time_data_types_check_each_completed_value_against_their_form() {
            fn domain(study: &Study, spec: &str, input: &str) -> file_application::Domain {
                fs::write(study.0.join("spec.yaml"), spec).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap()
            }
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, report: checks.csv, columns: [ID, LBTESTCD, LBDTC, LBORRES]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBTESTCD, type: str}\n  - {name: LBDTC, type: str, derivation: SRC.DTC, submission: {data_type: DTC_TYPE, origins: [{type: Collected, source: Investigator}]}}\n  - {name: LBORRES, type: str, derivation: SRC.RES, submission: {length: 20, origins: [{type: Collected, source: Investigator}], value_level: [{id: ONSET, where: {LBTESTCD: [ONSET]}, data_type: date}]}}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: onset\n    derivations: {LBTESTCD: \"'ONSET'\"}\n";
            let study = Study::new();
            // The column's type checks every row: a space is not ISO 8601, so
            // the run fails as `submission.json` records it.
            let mut run = domain(
                &study,
                &SPEC.edit("DTC_TYPE", "partialDatetime"),
                "ID,DTC,RES\n1,2025-03-04T08:30,2025-03-04\n2,2025-03-04 08:30,2025-03-05\n",
            );
            // The run report names the check `data_type` (CHECK-24).
            assert!(run.save().is_err());
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            assert!(
                checks.contains(
                    "output.csv,check,columns.LBDTC.submission.data_type,data_type,,LBDTC,SUB-26,error,violated,data_type_failed,2,4,"
                ),
                "{checks}"
            );
            fs::remove_file(study.0.join("checks.csv")).unwrap();
            // An entry's type checks only the rows its where clause selects
            // (`submission.json`). Every value of its form, and a missing
            // one, passes and saves.
            let mut run = domain(
                &study,
                &SPEC.edit("DTC_TYPE", "incompleteDatetime"),
                "ID,DTC,RES\n1,2025---04T08,2025-03-04\n2,,2025-03-05\n",
            );
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("output.csv")).unwrap(),
                "ID,LBTESTCD,LBDTC,LBORRES\n1,GLUC,2025---04T08,2025-03-04\n2,GLUC,,2025-03-05\n1,ONSET,2025---04T08,2025-03-04\n2,ONSET,,2025-03-05\n"
            );
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            for row in [
                "output.csv,check,columns.LBDTC.submission.data_type,data_type,,LBDTC,SUB-26,error,held,,0,4,,{}\n",
                "output.csv,check,columns.LBORRES.submission.value_level.ONSET.data_type,data_type,,LBORRES,SUB-26,error,held,,0,2,,{}\n",
            ] {
                assert!(checks.contains(row), "{row}\n{checks}");
            }
            // An entry selects its rows by comparing text, so its where
            // column must be a declared `str` column (SUB-12): the refusal
            // of an `int` one is `submission.json`'s.
        }

        const ROW_CT: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, RESULT, LBTESTCD]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: RESULT, type: str, derivation: SRC.VALUE, submission: {origins: [{type: Assigned, source: Sponsor}], codelist: DEFAULT, value_level: [{id: GLUC, where: {LBTESTCD: [GLUC]}, codelist: GLUCOSE}]}}\n  - {name: LBTESTCD, type: str}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: creatinine\n    derivations: {LBTESTCD: \"'CREAT'\"}\n";
        const CT_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - codelists:\n      - {id: DEFAULT, name: Default values, items: [{value: D}]}\n      - {id: GLUCOSE, name: Glucose values, items: [{value: G}]}\n      - {id: EXT, name: Extended values, extensible: true, items: [{value: X}]}\n      - {id: DICT, name: Dictionary, external: {dictionary: MedDRA, version: '27.1'}}\n";
        /// Build one specification on the project route; its run calls no
        /// project function, so the host-functions port is never entered.
        fn project_domain(path: &str, project_path: &str) -> file_application::Domain {
            let project = yamaa_application::file_workflow::select(
                path,
                project_path,
                yamaa_core::project_function::Language::Python,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{:?}",
                    yamaa_application::project_application::selection_failure(&error)
                )
            });
            let mut run_scope = project.run_scope(None);
            let attempt = project.build(&mut run_scope, &mut NoFunctions, Scope::default());
            let result = yamaa_application::specification_report::build_result(
                project.run(),
                &attempt,
                identity(path),
                &[],
            )
            .unwrap();
            file_application::Domain::new(result, &project, &run_scope)
        }

        /// Seam: the compiled specification's retained length bound and a
        /// hand-built check record projected through it, since no string a
        /// run can hold exceeds a bound this wide.
        #[test]
        fn wide_length_projection_borrows_the_exact_bound_from_compiled_metadata() {
            use yamaa_core::dataset::{CheckRecord, RowIdentity};
            use yamaa_core::{
                dataset::Check, diagnostic::ConditionCode, project_diagnostics::Fact, value::Value,
            };
            let study = Study::new();
            let bound = "340282366920938463463374607431768211456";
            fs::write(
                study.0.join("spec.yaml"),
                VALID.edit("length: 3", &format!("length: {bound}")),
            )
            .unwrap();
            let resources = yamaa_adapters::file_resources::Resources::new(
                study.0.to_str().unwrap(),
                study.0.to_str().unwrap(),
                &[],
            )
            .unwrap();
            let prepared = yamaa_application::file_preparation::FileSpecification::prepare(
                resources,
                "spec.yaml",
            )
            .unwrap();
            let compiled = prepared.run().compiled();
            let path = "columns.VALUE.verifications.max_length";
            let Some(Check::WideMaxLength(maximum)) = compiled.column_check(path) else {
                panic!("retained exact length bound");
            };
            assert_eq!(maximum.decimal(), bound);
            // This control tests the formatter's retained observation boundary without
            // pretending a physical native string can exceed this configured bound.
            let records = [CheckRecord {
                path: path.into(),
                code: ConditionCode::VerificationMaxLengthFailed,
                evaluated_count: 1,
                failed_count: 1,
                output_rows: 1,
                offending_rows: vec![RowIdentity {
                    position: 0,
                    values: vec![Value::Int(1)],
                }],
                codelist: None,
                inventory: None,
                row_count: None,
                entries: None,
                denominator: None,
            }];
            let projected = yamaa_core::verification_diagnostics::project(
                compiled,
                &records,
                yamaa_core::verification_diagnostics::Phase::Verification,
                "spec.yaml",
            )
            .unwrap();
            assert_eq!(
                projected.diagnostics[0].context["max"],
                Fact::Bound(maximum)
            );
            assert!(!study.0.join("input.csv").exists());
        }

        fn inventory_spec() -> String {
            VALID.edit("length: 3", "inventory_vocabulary: true, length: 10")
        }
        const INVENTORY_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nstudy: {id: STUDY, name: Study, description: Study dataset inventory, protocol_name: Protocol}\nsdtm:\n  standards: [{name: SDTMIG, version: '3.4', status: Final}]\n  specs: [catalog/renamed.yaml]\n  define: define.xml\n";
        /// A run admits a section that lists only its specs: the study and the
        /// section's standard and define are required only by Define.
        const SPECS_ONLY_PROJECT: &str =
            "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [catalog/renamed.yaml]\n";
        /// Seam: the selected project's inventory as values its API returns,
        /// the dataset names it holds and the documents it retained, which
        /// no issue row, table or saved file states. The build of each
        /// project and the value its inventory refuses are the
        /// `the_inventory_...` cases of `submission.json`.
        #[test]
        fn inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts(
        ) {
            use yamaa_core::project_function::Language;
            for (text, language) in [
                (INVENTORY_PROJECT, Language::Python),
                (INVENTORY_PROJECT, Language::R),
                (SPECS_ONLY_PROJECT, Language::Python),
                (SPECS_ONLY_PROJECT, Language::R),
            ] {
                let study = Study::new();
                fs::create_dir(study.0.join("catalog")).unwrap();
                fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
                fs::write(study.0.join("project.yaml"), text).unwrap();
                fs::write(
                    study.0.join("catalog/renamed.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                )
                .unwrap();
                fs::write(
                    study.0.join("catalog/parent.yaml"),
                    VALID
                        .edit("domain: TEST", "domain: DM")
                        .edit("input.csv", "never-read.csv"),
                )
                .unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let project_path = study.0.join("project.yaml");
                let project = yamaa_application::file_workflow::select(
                    path,
                    project_path.to_str().unwrap(),
                    language,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}",
                        yamaa_application::project_application::selection_failure(&error)
                    )
                });
                assert_eq!(project.run().environment().inventory().names(), ["DM"]);
                let retained = project.run().captured_environment().inventory_documents();
                assert_eq!(retained.len(), 1);
                assert!(retained[0]
                    .source()
                    .identity
                    .ends_with("catalog/renamed.yaml"));
                assert!(!study.0.join("catalog/never-read.csv").exists());
                assert!(!study.0.join("input.csv").exists());
            }
        }

        /// Seam: the report's row count and single check rows of the report
        /// file a save writes, a refused one included (CHECK-22), counted as
        /// substrings. A case states a report kind's rows whole and a saved
        /// file's bytes whole, and CHECK-23 leaves the order of one column's
        /// check rows open, so these rows cannot be authored as a case.
        #[test]
        fn value_codelist_checks_report_one_row_per_declaring_path_in_success_and_failure() {
            for fail in [false, true] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.edit("path: output.csv", "path: output.csv, report: checks.csv"),
                )
                .unwrap();
                let project = if fail {
                    CT_PROJECT.to_owned()
                } else {
                    CT_PROJECT.edit("items: [{value: G}]", "items: [{value: D}]")
                };
                fs::write(study.0.join("project.yaml"), project).unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let project = study.0.join("project.yaml");
                let mut run = project_domain(path, project.to_str().unwrap());
                assert_eq!(run.issues().len(), usize::from(fail));
                // The run, its input, both row templates, the value-level
                // selection check, the entry's warning that it matches a row
                // (SUB-13) and one check row per bound codelist; a successful
                // run adds its three columns.
                assert_eq!(run.run_report().rows(), if fail { 8 } else { 11 });
                assert!(!study.0.join("checks.csv").exists());
                let saved = run.save();
                assert_eq!(saved.is_ok(), !fail);
                let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
                for path in [
                    ",check,columns.RESULT.submission.codelist,allowed_values,,RESULT,CT-7,error,",
                    ",check,columns.RESULT.submission.value_level.GLUC.codelist,allowed_values,,RESULT,CT-7,error,",
                    ",check,columns.RESULT.submission.value_level,value_level,,RESULT,SUB-13,error,held,,0,4,,{}",
                    ",check,columns.RESULT.submission.value_level.GLUC,value_level,,RESULT,SUB-13,warning,held,,0,1,,{}",
                ] {
                    assert_eq!(checks.matches(path).count(), 1, "{checks}");
                }
                assert_eq!(checks.matches("CT-7").count(), 2);
                assert_eq!(study.0.join("output.csv").exists(), !fail);
            }
        }

        fn value_length_spec(length: &str) -> String {
            ROW_CT
                .edit("codelist: DEFAULT", "length: 3")
                .edit("codelist: GLUCOSE", &format!("length: {length}"))
        }
        /// Seam: the report's row count, which tells one shared length check
        /// from one per bound. A case states a report kind's rows whole, and
        /// CHECK-23 leaves the path and place of a shared check among one
        /// column's check rows open; the save also writes the declared
        /// report, whose bytes a case would have to state whole.
        #[test]
        fn equal_per_value_lengths_run_one_shared_check_and_wide_bounds_keep_exact_metadata() {
            for length in ["3", "340282366920938463463374607431768211456"] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    value_length_spec(length)
                        .edit("path: output.csv", "path: output.csv, report: checks.csv"),
                )
                .unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                assert!(run.issues().is_empty(), "{:?}", run.issues());
                // The run, its input, two row templates and three columns,
                // then the two value-level checks and one shared length check
                // or one check per bound.
                assert_eq!(run.run_report().rows(), if length == "3" { 10 } else { 11 });
                assert!(run.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("output.csv")).unwrap(),
                    "ID,RESULT,LBTESTCD\n1,é字😀,GLUC\n2,,GLUC\n1,é字😀,CREAT\n2,,CREAT\n"
                );
            }
        }
        fn domain(study: &Study, raw: &str, input: &str) -> file_application::Domain {
            fs::write(study.0.join("spec.yaml"), raw).unwrap();
            fs::write(study.0.join("input.csv"), input).unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap()
        }
        /// The SPEC_PATH, SEVERITY and CONDITION of each violated check row of a
        /// run's report, in report order.
        fn violated_checks(
            report: &yamaa_adapters::public_table::PublicTable,
        ) -> Vec<(String, String, String)> {
            use arrow_array::{cast::AsArray, Array};
            let cell = |array: &dyn Array, row: usize| {
                if array.is_null(row) {
                    String::new()
                } else if let Some(strings) = array.as_string_opt::<i32>() {
                    strings.value(row).to_owned()
                } else {
                    array.as_string_view().value(row).to_owned()
                }
            };
            let mut violated = Vec::new();
            for batch in report.batches() {
                let column = |name: &str| batch.column_by_name(name).unwrap().as_ref();
                for row in 0..batch.num_rows() {
                    if cell(column("OUTCOME"), row) == "violated" {
                        violated.push((
                            cell(column("SPEC_PATH"), row),
                            cell(column("SEVERITY"), row),
                            cell(column("CONDITION"), row),
                        ));
                    }
                }
            }
            violated
        }
        /// Entries over two where columns: one by test code, one by specimen;
        /// nothing static rules out a glucose urine row.
        const SELECTED: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, LBTESTCD, LBSPEC, RESULT], report: report.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBTESTCD, type: str}\n  - {name: LBSPEC, type: str, derivation: SRC.SPEC}\n  - name: RESULT\n    type: str\n    derivation: SRC.VALUE\n    submission:\n      origins: [{type: Assigned, source: Sponsor}]\n      value_level:\n        - {id: GLUC, where: {LBTESTCD: [GLUC]}, mandatory: true}\n        - {id: URINE, where: {LBSPEC: [URINE]}, length: 2}\n        - {id: ALB, where: {LBTESTCD: [ALB], LBSPEC: [SERUM]}}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: creatinine\n    derivations: {LBTESTCD: \"'CREAT'\"}\n";
        /// Seam: the refused save of a specification that declares a report
        /// still writes the report (CHECK-22), whose bytes a case would have
        /// to state whole, and no output. The check that passes and the
        /// failure naming both entries are `submission.json`'s.
        #[test]
        fn a_row_two_entries_select_fails_with_its_keys_and_both_entries() {
            let study = Study::new();
            let mut run = domain(&study, SELECTED, "ID,SPEC,VALUE\n1,SERUM,5\n2,URINE,7\n");
            assert!(run.save().is_err());
            assert!(study.0.join("report.csv").is_file());
            assert!(!study.0.join("output.csv").exists());
        }
        /// Seam: the violated check rows of a run's report, and single check
        /// rows of the report file its save writes, counted as substrings. A
        /// case states a report kind's rows whole, and CHECK-23 leaves the
        /// order of one column's check rows open, so these rows cannot be
        /// authored as a case.
        #[test]
        fn an_entry_that_matches_no_row_is_a_warning_and_scoped_checks_read_their_rows() {
            let study = Study::new();
            // Only serum is collected: URINE and ALB select no row, which warns
            // without failing. The warning needs no report file: every run's
            // report carries it, and it is never an issue.
            let quiet = SELECTED.edit(", report: report.csv", "");
            assert!(study.check(&quiet).is_empty());
            let input = "ID,SPEC,VALUE\n1,SERUM,5\n2,SERUM,toolong\n";
            let mut run = domain(&study, &quiet, input);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert_eq!(
                violated_checks(run.run_report()),
                [
                    (
                        "columns.RESULT.submission.value_level.URINE".to_owned(),
                        "warning".to_owned(),
                        "value_level_unmatched".to_owned()
                    ),
                    (
                        "columns.RESULT.submission.value_level.ALB".to_owned(),
                        "warning".to_owned(),
                        "value_level_unmatched".to_owned()
                    ),
                ]
            );
            assert!(run.save().unwrap());
            assert!(!study.0.join("report.csv").exists());
            let study = Study::new();
            let mut run = domain(&study, SELECTED, input);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            let report = fs::read_to_string(study.0.join("report.csv")).unwrap();
            for row in [
                "output.csv,check,columns.RESULT.submission.value_level,value_level,,RESULT,SUB-13,error,held,,0,4,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.GLUC,value_level,,RESULT,SUB-13,warning,held,,0,1,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.URINE,value_level,,RESULT,SUB-13,warning,violated,value_level_unmatched,1,1,[{}],\"{\"\"column\"\":\"\"RESULT\"\",\"\"entry\"\":\"\"URINE\"\"}\"",
                "output.csv,check,columns.RESULT.submission.value_level.ALB,value_level,,RESULT,SUB-13,warning,violated,value_level_unmatched,1,1,[{}],\"{\"\"column\"\":\"\"RESULT\"\",\"\"entry\"\":\"\"ALB\"\"}\"",
                "output.csv,check,columns.RESULT.submission.value_level.GLUC.mandatory,not_missing,,RESULT,SUB-11,error,held,,0,2,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.URINE.length,max_length,,RESULT,CHECK-8,error,held,,0,0,,{}",
            ] {
                assert_eq!(report.matches(row).count(), 1, "{report}");
            }
            // The glucose entry's `mandatory` decides the glucose rows alone
            // (`submission.json`); its failure refuses the save.
            let study = Study::new();
            let mut run = domain(&study, SELECTED, "ID,SPEC,VALUE\n1,SERUM,\n");
            assert!(run.save().is_err());
        }
    }
}
