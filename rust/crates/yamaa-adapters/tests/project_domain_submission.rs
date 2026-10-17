#![cfg(feature = "yaml")]
//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod domain_submission {
        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::application_observer::Scope;
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let root = std::env::temp_dir().join(format!(
                    "yamaa-domain-submission-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&root).unwrap();
                Self(fs::canonicalize(root).unwrap())
            }
            fn check(&self, raw: &str) -> Vec<yamaa_adapters::issue_rows::Issue> {
                fs::write(self.0.join("spec.yaml"), raw).unwrap();
                let path = self.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                file_application::check(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
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
                example: "submission",
                specification: path,
                base_directory: ".",
            }
        }
        fn copy(source: &Path, target: &Path) {
            fs::create_dir_all(target).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let path = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &path)
                } else {
                    fs::copy(entry.path(), path).unwrap();
                }
            }
        }
        const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: str\n    derivation: SRC.VALUE\n    submission: {length: 3, origin: {type: Assigned}}\n";
        #[test]
        fn submission_fields_do_not_change_any_original_domain_artifact() {
            let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for (example, artifact) in [
                ("sdtm-ae-coding", "ae.csv"),
                ("sdtm-dm-metadata", "dm.csv"),
                ("sdtm-lb-metadata", "lb.csv"),
                ("sdtm-vs-epoch-from-subject-elements", "vs.csv"),
            ] {
                let case = repository.join("benchmarks").join(example);
                let study = Study::new();
                copy(&case, &study.0);
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap_or_else(|issues| panic!("{example}: {issues:?}"));
                assert!(run.issues().is_empty(), "{example}: {:?}", run.issues());
                run.save().unwrap();
                assert_eq!(
                    fs::read(study.0.join(artifact)).unwrap(),
                    fs::read(case.join("expected").join(artifact)).unwrap(),
                    "{example}"
                );
            }
        }
        #[test]
        fn metadata_admission_has_exact_causes_before_study_access() {
            let study = Study::new();
            assert!(study.check(VALID).is_empty());
            let issues = study.check(&format!("{VALID}\nmetadata: {{label: forbidden}}\n"));
            assert_eq!(issues.len(), 1);
            assert_eq!(
                (
                    &issues[0].phase,
                    &issues[0].condition,
                    issues[0].requirement.as_deref()
                ),
                (
                    &"validation".into(),
                    &"reserved_metadata_key".into(),
                    Some("REQ-0910")
                )
            );
            assert_eq!(issues[0].spec_paths, ["metadata.label"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"key\":\"label\",\"level\":\"root\"}"
            );
            let issues =
                study.check(&VALID.replace("length: 3", "length: -99999999999999999999999999999"));
            assert_eq!(issues[0].condition, "submission_length_invalid");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0911"));
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"column\":\"VALUE\",\"length\":-99999999999999999999999999999}"
            );
            let issues =
                study.check(&VALID.replace("origin: {type: Assigned}", "origin: {type: Derived}"));
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| issue.condition.as_str())
                    .collect::<Vec<_>>(),
                ["method_missing", "origin_contradicts_derivation"]
            );
            let raw = VALID.replace(
                "submission: {length: 3",
                "verifications: {max_length: {max: 4}}\n    submission: {length: 3",
            );
            let issues = study.check(&raw);
            assert_eq!(issues[0].condition, "declared_length_conflict");
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"column\":\"VALUE\",\"length\":3,\"max_length\":4}"
            );
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn declared_length_is_checked_once_over_unicode_scalar_values() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,abcd\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let mut run = file_application::domain(
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap();
            let issues = run.issues();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "length_failed");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0378"));
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"max\":3}"
            );
            assert!(run.save().is_err());
            assert!(!study.0.join("output.csv").exists());
        }

        const ROW_CT: &str = "schema_version: '1.0'\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, RESULT, LBTESTCD]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: RESULT, type: str, derivation: SRC.VALUE, submission: {origin: {type: Assigned}, codelist: DEFAULT}}\n  - {name: LBTESTCD, type: str}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: {literal: GLUC}}\n    submission: {RESULT: {codelist: GLUCOSE}}\n  - id: creatinine\n    derivations: {LBTESTCD: {literal: CREAT}}\n";
        const CT_ENV: &str = "schema_version: '1.0'\ncodelists:\n  - codelists:\n      - {id: DEFAULT, name: Default values, items: [{value: D}]}\n      - {id: GLUCOSE, name: Glucose values, items: [{value: G}]}\n      - {id: EXT, name: Extended values, extensible: true, items: [{value: X}]}\n      - {id: DICT, name: Dictionary, external: {dictionary: MedDRA, version: '27.1'}}\n";
        #[test]
        fn per_value_codelists_inherit_origins_and_check_completed_discriminators() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), ROW_CT).unwrap();
            fs::write(study.0.join("environment.yaml"), CT_ENV).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,G\n2,D\n3,\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let environment = study.0.join("environment.yaml");
            let mut run = project_domain(path, environment.to_str().unwrap());
            assert_eq!(run.issues().len(), 2, "{:?}", run.issues());
            for issue in run.issues() {
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    ("verification", "allowed_values_failed", Some("REQ-0957"))
                );
                assert_eq!(issue.spec_paths, ["columns.RESULT.submission.codelist"]);
            }
            let contexts = run
                .issues()
                .iter()
                .map(|issue| issue.context.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                contexts[0],
                yamaa_adapters::json!({"column":"RESULT", "codelist":"DEFAULT", "failure_count":1, "keys":[{"ID":1,"LBTESTCD":"CREAT"}], "values":["G"]})
            );
            assert_eq!(
                contexts[1],
                yamaa_adapters::json!({"column":"RESULT", "codelist":"GLUCOSE", "failure_count":1, "keys":[{"ID":2,"LBTESTCD":"GLUC"}], "values":["D"]})
            );
            assert!(run.save().is_err());
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn per_value_external_and_extensible_bindings_exclude_default_enforcement() {
            for codelist in ["EXT", "DICT"] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.replace("codelist: GLUCOSE", &format!("codelist: {codelist}")),
                )
                .unwrap();
                fs::write(study.0.join("environment.yaml"), CT_ENV).unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let environment = study.0.join("environment.yaml");
                let mut run = project_domain(path, environment.to_str().unwrap());
                assert!(run.issues().is_empty(), "{codelist}: {:?}", run.issues());
                assert!(run.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("output.csv")).unwrap(),
                    "ID,RESULT,LBTESTCD\n1,D,GLUC\n2,,GLUC\n1,D,CREAT\n2,,CREAT\n"
                );
            }
        }
        #[test]
        fn per_value_codelist_unknown_and_type_failures_retain_the_authored_row_path() {
            for (codelist, expected) in [
                ("UNKNOWN", "unknown_codelist"),
                ("INT", "codelist_type_mismatch"),
            ] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.replace("codelist: GLUCOSE", &format!("codelist: {codelist}")),
                )
                .unwrap();
                fs::write(study.0.join("environment.yaml"), format!("{CT_ENV}      - {{id: INT, name: Numeric values, data_type: integer, items: [{{value: 1}}]}}\n")).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let environment = study.0.join("environment.yaml");
                let host = yamaa_core::project_function::Language::Python;
                let Err(error) = yamaa_adapters::file_workflow::select(
                    path,
                    environment.to_str().unwrap(),
                    host,
                ) else {
                    panic!("invalid value codelist was admitted");
                };
                let issues = yamaa_adapters::project_application::selection_failure(&error, host);
                assert_eq!(issues.len(), 1, "{issues:?}");
                assert_eq!(issues[0].condition, expected);
                assert_eq!(
                    issues[0].spec_paths,
                    ["rows.glucose.submission.RESULT.codelist"]
                );
                assert!(!study.0.join("input.csv").exists());
            }
        }

        struct NoActivation;
        impl yamaa_core::project_activation::ActivationPort for NoActivation {
            type Handle = ();
            type Error = ();
            fn verify_lock(
                &mut self,
                _: yamaa_core::project_function::Language,
                _: &yamaa_core::project_environment::LockReference,
                _: &[yamaa_core::function_signature::ProjectFunctionIdentity],
            ) -> Result<(), ()> {
                panic!("no callable definitions")
            }
            fn bind(
                &mut self,
                _: &yamaa_core::function_signature::ProjectFunctionIdentity,
                _: &yamaa_core::function_signature::LogicalSignature,
            ) -> Result<(), ()> {
                panic!("no callable definitions")
            }
            fn invoke(
                &mut self,
                _: &(),
                _: &[yamaa_core::function_invocation::Argument<'_>],
            ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<()>>
            {
                panic!("no callable definitions")
            }
        }
        fn project_domain(path: &str, environment: &str) -> file_application::Domain {
            let yamaa_adapters::file_workflow::Selected::Ordinary(mut project) =
                yamaa_adapters::file_workflow::select(
                    path,
                    environment,
                    yamaa_core::project_function::Language::Python,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}",
                        yamaa_adapters::project_application::selection_failure(
                            &error,
                            yamaa_core::project_function::Language::Python
                        )
                    )
                })
            else {
                panic!("ordinary metadata capability")
            };
            let attempt = project.build(&mut NoActivation, Scope::default());
            let result = yamaa_adapters::project_report::build_result(
                project.run(),
                &attempt,
                identity(path),
                &[],
            )
            .unwrap();
            file_application::Domain::from_project(result, &project).unwrap()
        }

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
                VALID.replace("length: 3", &format!("length: {bound}")),
            )
            .unwrap();
            let resources = yamaa_adapters::file_resources::Resources::new(
                study.0.to_str().unwrap(),
                study.0.to_str().unwrap(),
                &[],
            )
            .unwrap();
            let prepared = yamaa_adapters::file_preparation::FileSpecification::prepare(
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
            VALID.replace("length: 3", "inventory_vocabulary: true, length: 10")
        }
        const INVENTORY_ENV: &str = "schema_version: '1.0'\nstudy: {id: STUDY, name: Study, description: Study dataset inventory, protocol_name: Protocol}\nsdtm:\n  standard: {name: SDTMIG, version: '3.4', status: Final}\n  specs: [catalog/renamed.yaml]\n  define: define.xml\n";
        #[test]
        fn absent_study_inventory_checks_nonmissing_values_and_keeps_missing_values_admitted() {
            for input in ["ID,VALUE\n1,DM\n2,\n", "ID,VALUE\n1,\n"] {
                let study = Study::new();
                fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let run = file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                if input.contains("DM") {
                    assert_eq!(run.issues().len(), 1);
                    assert_eq!(
                        (
                            run.issues()[0].phase.as_str(),
                            run.issues()[0].condition.as_str(),
                            run.issues()[0].requirement.as_deref()
                        ),
                        ("verification", "allowed_values_failed", Some("REQ-1156"))
                    );
                    assert_eq!(
                        run.issues()[0].spec_paths,
                        ["columns.VALUE.submission.inventory_vocabulary"]
                    );
                    assert_eq!(yamaa_adapters::json_wire::encode(&run.issues()[0].context),"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":1}],\"values\":[\"DM\"]}");
                } else {
                    assert!(run.issues().is_empty(), "{:?}", run.issues());
                }
            }
        }
        #[test]
        fn inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts(
        ) {
            for language in [
                yamaa_core::project_function::Language::Python,
                yamaa_core::project_function::Language::R,
            ] {
                let study = Study::new();
                fs::create_dir(study.0.join("catalog")).unwrap();
                fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
                fs::write(study.0.join("environment.yaml"), INVENTORY_ENV).unwrap();
                fs::write(
                    study.0.join("catalog/renamed.yaml"),
                    "schema_version: '1.0'\nparents: [parent.yaml]\n",
                )
                .unwrap();
                fs::write(
                    study.0.join("catalog/parent.yaml"),
                    VALID
                        .replace("domain: TEST", "domain: DM")
                        .replace("input.csv", "never-read.csv"),
                )
                .unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let environment = study.0.join("environment.yaml");
                let yamaa_adapters::file_workflow::Selected::Ordinary(mut project) =
                    yamaa_adapters::file_workflow::select(
                        path,
                        environment.to_str().unwrap(),
                        language,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "{:?}",
                            yamaa_adapters::project_application::selection_failure(
                                &error, language
                            )
                        )
                    })
                else {
                    panic!("ordinary inventory capability")
                };
                assert_eq!(project.run().environment().inventory().names(), ["DM"]);
                let retained = project.run().captured_environment().inventory_documents();
                assert_eq!(retained.len(), 1);
                assert!(retained[0]
                    .source()
                    .identity
                    .ends_with("catalog/renamed.yaml"));
                assert!(!study.0.join("catalog/never-read.csv").exists());
                assert!(!study.0.join("input.csv").exists());
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,renamed\n3,\n").unwrap();
                let attempt = project.build(&mut NoActivation, Scope::default());
                let result = yamaa_adapters::project_report::build_result(
                    project.run(),
                    &attempt,
                    identity(path),
                    &[],
                )
                .unwrap();
                let run = file_application::Domain::from_project(result, &project).unwrap();
                assert_eq!(run.issues().len(), 1);
                assert_eq!(run.issues()[0].requirement.as_deref(), Some("REQ-1156"));
                assert_eq!(yamaa_adapters::json_wire::encode(&run.issues()[0].context),"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"values\":[\"renamed\"]}");
                assert!(!study.0.join("define.xml").exists());
            }
        }
        #[test]
        fn value_inventory_override_inherits_base_metadata_and_selects_only_its_test_code() {
            let study = Study::new();
            let raw = ROW_CT
                .replace("codelist: DEFAULT", "inventory_vocabulary: true")
                .replace("codelist: GLUCOSE", "inventory_vocabulary: false");
            fs::write(study.0.join("spec.yaml"), raw).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let run = file_application::domain(
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap();
            assert_eq!(run.issues().len(), 1);
            assert_eq!(yamaa_adapters::json_wire::encode(&run.issues()[0].context),"{\"column\":\"RESULT\",\"failure_count\":1,\"keys\":[{\"ID\":1,\"LBTESTCD\":\"CREAT\"}],\"values\":[\"DM\"]}");
        }

        #[test]
        fn multiple_value_codelist_records_keep_the_shared_path_in_success_and_failure_logs() {
            for fail in [false, true] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.replace(
                        "path: output.csv",
                        "path: output.csv, verification_log: checks.csv",
                    ),
                )
                .unwrap();
                let environment = if fail {
                    CT_ENV.to_owned()
                } else {
                    CT_ENV.replace("items: [{value: G}]", "items: [{value: D}]")
                };
                fs::write(study.0.join("environment.yaml"), environment).unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let environment = study.0.join("environment.yaml");
                let mut run = project_domain(path, environment.to_str().unwrap());
                assert_eq!(run.issues().len(), usize::from(fail));
                assert_eq!(run.log_table(false).unwrap().rows(), 2);
                assert!(!study.0.join("checks.csv").exists());
                let saved = run.save();
                assert_eq!(saved.is_ok(), !fail);
                let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
                assert_eq!(
                    checks.matches("columns.RESULT.submission.codelist").count(),
                    2
                );
                assert_eq!(checks.matches("REQ-0957").count(), 2);
                assert_eq!(study.0.join("output.csv").exists(), !fail);
            }
        }

        fn value_length_spec(length: &str) -> String {
            ROW_CT
                .replace("codelist: DEFAULT", "length: 3")
                .replace("codelist: GLUCOSE", &format!("length: {length}"))
        }
        #[test]
        fn per_value_lengths_replace_inherited_bounds_only_for_their_completed_test_code() {
            for (bound, input, expected_path, expected_key, maximum) in [
                (
                    "5",
                    "ID,VALUE\n1,abcde\n2,é字😀\n",
                    "columns.RESULT.verifications.max_length",
                    "CREAT",
                    3,
                ),
                (
                    "2",
                    "ID,VALUE\n1,abc\n2,é字\n3,\n",
                    "rows.glucose.submission.RESULT.length",
                    "GLUC",
                    2,
                ),
            ] {
                let study = Study::new();
                fs::write(study.0.join("spec.yaml"), value_length_spec(bound)).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
                let issue = &run.issues()[0];
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    ("verification", "length_failed", Some("REQ-0378"))
                );
                assert_eq!(issue.spec_paths, [expected_path]);
                assert_eq!(
                    issue.context.clone(),
                    yamaa_adapters::json!({"column":"RESULT","failure_count":1,"keys":[{"ID":1,"LBTESTCD":expected_key}],"max":maximum})
                );
                assert!(run.save().is_err());
            }
        }
        #[test]
        fn equal_per_value_lengths_run_one_shared_check_and_wide_bounds_keep_exact_metadata() {
            for length in ["3", "340282366920938463463374607431768211456"] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    value_length_spec(length).replace(
                        "path: output.csv",
                        "path: output.csv, verification_log: checks.csv",
                    ),
                )
                .unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = file_application::domain(
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                assert!(run.issues().is_empty(), "{:?}", run.issues());
                assert_eq!(
                    run.log_table(false).unwrap().rows(),
                    if length == "3" { 1 } else { 2 }
                );
                assert!(run.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("output.csv")).unwrap(),
                    "ID,RESULT,LBTESTCD\n1,é字😀,GLUC\n2,,GLUC\n1,é字😀,CREAT\n2,,CREAT\n"
                );
            }
        }
        #[test]
        fn effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data(
        ) {
            for (declaration, condition, path) in [
                (
                    "length: -999999999999999999999999999999",
                    "submission_length_invalid",
                    "rows.glucose.submission.RESULT.length",
                ),
                (
                    "length: 0",
                    "submission_length_invalid",
                    "rows.glucose.submission.RESULT.length",
                ),
                (
                    "data_type: float",
                    "submission_data_type_not_admitted",
                    "rows.glucose.submission.RESULT",
                ),
                (
                    "significant_digits: -999999999999999999999999999999",
                    "submission_length_invalid",
                    "rows.glucose.submission.RESULT.significant_digits",
                ),
                (
                    "mandatory: true",
                    "mandatory_not_enforced",
                    "rows.glucose.submission.RESULT",
                ),
            ] {
                let study = Study::new();
                let raw = ROW_CT
                    .replace("codelist: DEFAULT", "length: 3")
                    .replace("codelist: GLUCOSE", declaration);
                let issues = study.check(&raw);
                assert_eq!(issues.len(), 1, "{declaration}: {issues:?}");
                assert_eq!(issues[0].condition, condition);
                assert_eq!(issues[0].spec_paths, [path]);
                assert!(!study.0.join("input.csv").exists());
            }
            let study = Study::new();
            let raw = value_length_spec("4").replace(
                "derivation: SRC.VALUE, submission",
                "derivation: SRC.VALUE, verifications: {max_length: {max: 3}}, submission",
            );
            let issues = study.check(&raw);
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(
                (
                    issues[0].condition.as_str(),
                    issues[0].requirement.as_deref()
                ),
                ("declared_length_conflict", Some("REQ-0913"))
            );
            assert_eq!(issues[0].spec_paths, ["rows.glucose.submission.RESULT"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"column\":\"RESULT\",\"length\":4,\"max_length\":3}"
            );
        }

        #[test]
        fn effective_value_fields_inherit_full_metadata_and_refute_inapplicable_lengths_and_digits()
        {
            let root = "\nsubmission: {label: Laboratory Results, class: FINDINGS, structure: One record per test, repeating: true}\n";
            let base = format!(
                "{}{root}",
                value_length_spec("3").replace(
                    "origin: {type: Assigned}, length: 3",
                    "core: Perm, origin: {type: Assigned}, length: 3"
                )
            );
            let study = Study::new();
            let baseline = study.check(&base);
            assert!(baseline.is_empty(), "{baseline:?}");
            for (entry, field) in [
                ("data_type: date", "data_type"),
                ("significant_digits: 2", "field"),
            ] {
                let raw = base.replace(
                    "submission: {RESULT: {length: 3}}",
                    &format!("submission: {{RESULT: {{{entry}}}}}"),
                );
                let issues = study.check(&raw);
                assert_eq!(issues.len(), 1, "{entry}: {issues:?}");
                assert_eq!(
                    (
                        issues[0].condition.as_str(),
                        issues[0].requirement.as_deref()
                    ),
                    ("submission_length_not_applicable", Some("REQ-0912"))
                );
                assert_eq!(issues[0].spec_paths, ["rows.glucose.submission.RESULT"]);
                let context = issues[0].context.clone();
                assert_eq!(context["column"], "RESULT");
                assert_eq!(
                    context[field],
                    if field == "field" {
                        "significant_digits"
                    } else {
                        "date"
                    }
                );
            }
            assert!(!study.0.join("input.csv").exists());
        }
    }
    mod producer_admission {
        use std::sync::Arc;
        use yamaa_adapters::{
            producer_admission::{prepare, Error, SuppliedProducer},
            shipped_schema,
            specification_source::{InheritancePort, PreparedDocument, Source},
        };
        use yamaa_core::inheritance::{Source as FileIdentity, SourceError};
        use yamaa_core::{producer_admission::Error as CoreError, specification::PrepareError};
        const PRODUCER:&str="schema_version: '1.0'\ndomain: PROD\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: RAW.VALUE}\noutput: {path: './produced.csv', columns: [ID, VALUE]}\n";
        const CONSUMER:&str="schema_version: '1.0'\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './produced.csv', schema: nested/producer.yaml}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
        fn standalone(identity: &str, text: &str) -> Arc<PreparedDocument> {
            Arc::new(
                shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: identity.into(),
                        bytes: text.as_bytes().to_vec(),
                    })
                    .unwrap(),
            )
        }
        fn supplied(document: Arc<PreparedDocument>) -> SuppliedProducer {
            SuppliedProducer {
                dataset: "SRC".into(),
                schema_identity: "/producer.yaml".into(),
                source_identity: "/artifacts/produced.csv".into(),
                output_identity: "/artifacts/produced.csv".into(),
                document,
            }
        }
        #[test]
        fn original_documents_and_distinct_metadata_remain_held_without_study_authority() {
            let consumer = standalone("/consumer.yaml", CONSUMER);
            let producer = standalone("/producer.yaml", PRODUCER);
            let admitted = prepare(
                Arc::clone(&consumer),
                vec![supplied(Arc::clone(&producer))],
                None,
            )
            .unwrap();
            drop(consumer);
            drop(producer);
            assert_eq!(admitted.consumer().source().bytes, CONSUMER.as_bytes());
            assert_eq!(
                admitted.producers()[0].document.source().bytes,
                PRODUCER.as_bytes()
            );
            let declaration = &admitted.metadata().producers()[0];
            assert_eq!(
                declaration.schema_origin().written(),
                "nested/producer.yaml"
            );
            assert_eq!(
                declaration.schema_origin().declaring_source(),
                "/consumer.yaml"
            );
            assert_eq!(declaration.output_origin().written(), "./produced.csv");
            assert_eq!(
                declaration
                    .contract()
                    .fields()
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "VALUE"]
            );
        }
        struct Parent {
            text: Vec<u8>,
            calls: Vec<String>,
        }
        impl InheritancePort for Parent {
            type Error = ();
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<FileIdentity, SourceError<()>> {
                self.calls
                    .push(format!("canonicalize:{declaring}:{written}"));
                assert_eq!(written, "parent.yaml");
                Ok(FileIdentity {
                    identity: if declaring == "/producer.yaml" {
                        "/producer-parent.yaml"
                    } else {
                        "/parent.yaml"
                    }
                    .into(),
                    display_path: "parent.yaml".into(),
                })
            }
            fn capture(&mut self, source: &FileIdentity) -> Result<Vec<u8>, SourceError<()>> {
                self.calls.push(format!("capture:{}", source.identity));
                Ok(self.text.clone())
            }
            fn rebase(
                &mut self,
                layer: &FileIdentity,
                _: &FileIdentity,
                written: &str,
            ) -> Result<String, ()> {
                self.calls
                    .push(format!("rebase:{}:{written}", layer.identity));
                Ok(if layer.identity.ends_with("parent.yaml") {
                    format!("internal/parent/{written}")
                } else {
                    written.into()
                })
            }
        }
        fn inherited(identity: &str, entry: &str, parent: &str) -> (Arc<PreparedDocument>, Parent) {
            let mut port = Parent {
                text: parent.as_bytes().to_vec(),
                calls: vec![],
            };
            let document = shipped_schema::prepare(
                Source {
                    identity: identity.into(),
                    bytes: entry.as_bytes().to_vec(),
                },
                identity.into(),
                &mut port,
            )
            .unwrap();
            (Arc::new(document), port)
        }
        #[test]
        fn inherited_schema_artifact_and_output_paths_keep_independent_authored_layers() {
            let entry="schema_version: '1.0'\nparents: parent.yaml\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './child.csv'}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
            let parent =
        "schema_version: '1.0'\ninput: {SRC: {path: parent.csv, schema: '../producer.yaml'}}\n";
            let (consumer, consumer_port) = inherited("/consumer.yaml", entry, parent);
            let producer_entry = PRODUCER
                .replace(
                    "schema_version: '1.0'",
                    "schema_version: '1.0'\nparents: parent.yaml",
                )
                .replace("output: {path: './produced.csv', columns: [ID, VALUE]}", "");
            let producer_parent =
                "schema_version: '1.0'\noutput: {path: '../stored.csv', columns: [ID, VALUE]}\n";
            let (producer, producer_port) =
                inherited("/producer.yaml", &producer_entry, producer_parent);
            let before = (consumer_port.calls.clone(), producer_port.calls.clone());
            let admitted = prepare(Arc::clone(&consumer), vec![supplied(producer)], None).unwrap();
            let declaration = &admitted.metadata().producers()[0];
            assert_eq!(
                declaration.schema_path(),
                "internal/parent/../producer.yaml"
            );
            assert_eq!(declaration.schema_origin().written(), "../producer.yaml");
            assert_eq!(
                declaration.schema_origin().declaring_source(),
                "/parent.yaml"
            );
            assert_eq!(declaration.input_origin().written(), "./child.csv");
            assert_eq!(
                declaration.input_origin().declaring_source(),
                "/consumer.yaml"
            );
            assert_eq!(declaration.output_origin().written(), "../stored.csv");
            assert_eq!(
                declaration.output_origin().declaring_source(),
                "/producer-parent.yaml"
            );
            assert_eq!(
                declaration.contract().path(),
                "internal/parent/../stored.csv"
            );
            assert_eq!(consumer.parents()[0].source().bytes, parent.as_bytes());
            assert_eq!(
                admitted.producers()[0].document.parents()[0].source().bytes,
                producer_parent.as_bytes()
            );
            assert_eq!(before, (consumer_port.calls, producer_port.calls));
        }
        #[test]
        fn missing_and_contradictory_metadata_fail_before_capability() {
            let consumer = standalone("/consumer.yaml", CONSUMER);
            let producer = standalone("/producer.yaml", PRODUCER);
            assert!(matches!(
                prepare(Arc::clone(&consumer), vec![], None),
                Err(Error::Admission(CoreError::Invalid(_)))
            ));
            let mut wrong = supplied(Arc::clone(&producer));
            wrong.schema_identity = "/wrong.yaml".into();
            assert!(matches!(
                prepare(Arc::clone(&consumer), vec![wrong], None),
                Err(Error::Admission(CoreError::Invalid(_)))
            ));
            assert!(prepare(consumer, vec![supplied(producer)], None).is_ok());
        }
        #[test]
        fn ordinary_preparation_remains_unsupported_and_inline_types_are_portable() {
            let consumer = standalone("/consumer.yaml", CONSUMER);
            let redundant = CONSUMER.replace(
                "schema: nested/producer.yaml",
                "schema: nested/producer.yaml, types: {ID: int}",
            );
            let redundant = standalone("/consumer.yaml", &redundant);
            let error = prepare(
                redundant,
                vec![supplied(standalone("/producer.yaml", PRODUCER))],
                None,
            );
            let Err(Error::Admission(CoreError::Invalid(findings))) = error else {
                panic!("redundant authority")
            };
            assert_eq!(findings[0].definition().requirement, Some("REQ-0523"));
            assert_eq!(findings[0].spec_paths, ["input.SRC.types.ID"]);
            assert!(matches!(
                yamaa_core::domain::check(consumer.model()),
                Err(PrepareError::Unsupported(_))
            ));
        }
        #[test]
        fn producer_must_use_the_same_captured_root_closure() {
            use yamaa_adapters::specification_source::CapturedSchema;
            let consumer = standalone("/consumer.yaml", CONSUMER);
            let shipped = shipped_schema::capture().unwrap();
            let mut sources = shipped.sources().to_vec();
            sources[0]
                .bytes
                .extend_from_slice(b"\n# independent changed closure\n");
            let different = CapturedSchema::admit(sources, 0).unwrap();
            let producer = Arc::new(
                different
                    .prepare_standalone(Source {
                        identity: "/producer.yaml".into(),
                        bytes: PRODUCER.as_bytes().to_vec(),
                    })
                    .unwrap(),
            );
            let Err(Error::Admission(CoreError::Invalid(findings))) =
                prepare(Arc::clone(&consumer), vec![supplied(producer)], None)
            else {
                panic!("captured root mismatch")
            };
            assert_eq!(findings[0].definition().requirement, Some("REQ-0534"));
            assert_eq!(findings[0].spec_paths, ["input.SRC.schema"]);
        }
        #[test]
        fn shared_producer_identities_require_the_same_original_byte_snapshot() {
            let two=CONSUMER.replace("schema: nested/producer.yaml}}", "schema: nested/producer.yaml}, OTHER: {path: './produced.csv', schema: nested/producer.yaml}}")+"base: SRC\n";
            let consumer = standalone("/consumer.yaml", &two);
            let original = standalone("/producer.yaml", PRODUCER);
            // Equal normalized contracts cannot hide different captured source bytes.
            let changed = standalone(
                "/producer.yaml",
                &(PRODUCER.to_owned() + "# different captured bytes\n"),
            );
            let mut other = supplied(changed);
            other.dataset = "OTHER".into();
            let Err(Error::Admission(CoreError::Invalid(findings))) =
                prepare(consumer, vec![supplied(original), other], None)
            else {
                panic!("contradictory source snapshot")
            };
            assert_eq!(findings[0].definition().requirement, Some("REQ-0534"));
            assert_eq!(findings[0].spec_paths, ["input.OTHER.schema"]);
            assert_eq!(
                findings[0].context["reason"],
                yamaa_core::json::Value::String("contradictory_snapshot".into())
            );
        }
        #[test]
        fn undeclared_and_schema_less_supplied_metadata_retain_the_portable_cause() {
            let producer = standalone("/producer.yaml", PRODUCER);
            for (text, dataset) in [
                (CONSUMER.to_owned(), "ABSENT"),
                (
                    CONSUMER.replace(", schema: nested/producer.yaml", ""),
                    "SRC",
                ),
                (
                    CONSUMER.replace(
                        "{path: './produced.csv', schema: nested/producer.yaml}",
                        "produced.csv",
                    ),
                    "SRC",
                ),
            ] {
                let consumer = standalone("/consumer.yaml", &text);
                let mut supplied = supplied(Arc::clone(&producer));
                supplied.dataset = dataset.into();
                let Err(Error::Admission(CoreError::Invalid(findings))) =
                    prepare(consumer, vec![supplied], None)
                else {
                    panic!("undeclared metadata diagnostic")
                };
                assert_eq!(findings[0].definition().requirement, Some("REQ-0534"));
                assert_eq!(findings[0].spec_paths, [format!("input.{dataset}.schema")]);
                assert_eq!(
                    findings[0].context["reason"],
                    yamaa_core::json::Value::String("undeclared_metadata".into())
                );
            }
            // The shipped structural root rejects explicit-null schema before this
            // metadata boundary; retain that original finding rather than bypassing it.
            let null = CONSUMER.replace("schema: nested/producer.yaml", "schema: null");
            let Err(yamaa_adapters::specification_source::Error::Findings(findings)) =
                shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: "/consumer.yaml".into(),
                        bytes: null.into_bytes(),
                    })
            else {
                panic!("original structural null refusal")
            };
            assert_eq!(findings.findings()[0].path, "input.SRC.schema");
            assert_eq!(
                findings.findings()[0].definition().requirement,
                Some("REQ-0287")
            );
        }
    }
    mod producer_graph {
        use std::sync::Arc;
        use yamaa_adapters::{
            producer_admission::SuppliedProducer,
            producer_graph::{prepare, Error, SuppliedNode},
            shipped_schema,
            specification_source::{InheritancePort, PreparedDocument, Source},
        };
        use yamaa_core::inheritance::{Source as Identity, SourceError};
        use yamaa_core::{
            producer_graph::Error as CoreError,
            project_environment::{Draft, Environment, ExecutionEnvironment},
            project_function::Language,
            specification::PrepareError,
        };

        fn environment() -> ExecutionEnvironment {
            Environment::admit(
                Language::Python,
                Draft {
                    language: None,
                    lock: None,
                    functions: None,
                    codelists: vec![],
                    has_study: false,
                    submissions: vec![],
                },
            )
            .unwrap()
            .into_execution()
        }
        fn text(input: &str, base: &str, output: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: float, label: Reported value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE], decimals: 2}}\n")
        }
        fn document(identity: &str, text: &str) -> Arc<PreparedDocument> {
            Arc::new(
                shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: identity.into(),
                        bytes: text.as_bytes().to_vec(),
                    })
                    .unwrap(),
            )
        }
        fn link(
            dataset: &str,
            document: &Arc<PreparedDocument>,
            artifact: &str,
        ) -> SuppliedProducer {
            SuppliedProducer {
                dataset: dataset.into(),
                schema_identity: document.source().identity.clone(),
                source_identity: artifact.into(),
                output_identity: artifact.into(),
                document: Arc::clone(document),
            }
        }
        fn node(
            document: &Arc<PreparedDocument>,
            producers: Vec<SuppliedProducer>,
        ) -> SuppliedNode {
            SuppliedNode {
                document: Arc::clone(document),
                producers,
            }
        }

        #[test]
        fn recursive_original_documents_are_owned_once_and_all_alias_origins_remain_held() {
            let leaf_text = text("{RAW: never-read.csv}", "RAW", "leaf.csv");
            let left_text = text(
                "{LEAF: {path: leaf.csv, schema: './leaf.yaml'}}",
                "LEAF",
                "left.csv",
            );
            let right_text = text(
                "{LEAF: {path: leaf.csv, schema: leaf.yaml}}",
                "LEAF",
                "right.csv",
            );
            // VALUE reads all three producer datasets so pruning (REQ-0638)
            // keeps every supplied link declared in the resolved model.
            let root_text = text("{RIGHT: {path: right.csv, schema: right.yaml}, LEFT: {path: left.csv, schema: left.yaml}, AGAIN: {path: left.csv, schema: './left.yaml'}}", "RIGHT", "root.csv")
                .replace(
                    "derivation: RIGHT.VALUE",
                    "derivation: {first_available: {sources: [RIGHT.VALUE, LEFT.VALUE, AGAIN.VALUE]}}",
                );
            let leaf = document("/leaf.yaml", &leaf_text);
            let left = document("/left.yaml", &left_text);
            let right = document("/right.yaml", &right_text);
            let root = document("/root.yaml", &root_text);
            let graph = prepare(
                "/root.yaml",
                vec![
                    node(&left, vec![link("LEAF", &leaf, "/leaf.csv")]),
                    node(
                        &root,
                        vec![
                            link("AGAIN", &left, "/left.csv"),
                            link("LEFT", &left, "/left.csv"),
                            link("RIGHT", &right, "/right.csv"),
                        ],
                    ),
                    node(&leaf, vec![]),
                    node(&right, vec![link("LEAF", &leaf, "/leaf.csv")]),
                ],
                environment(),
            )
            .unwrap();
            drop((root, left, right, leaf));
            assert_eq!(graph.metadata().root(), 1);
            assert_eq!(graph.metadata().order(), [2, 3, 0, 1]);
            assert_eq!(graph.nodes().len(), 4);
            assert_eq!(
                graph.nodes()[1].document.source().bytes,
                root_text.as_bytes()
            );
            assert_eq!(
                graph.nodes()[2].document.source().bytes,
                leaf_text.as_bytes()
            );
            assert!(Arc::ptr_eq(
                &graph.nodes()[0].producers[0].document,
                &graph.nodes()[2].document
            ));
            assert!(Arc::ptr_eq(
                &graph.nodes()[3].producers[0].document,
                &graph.nodes()[2].document
            ));
            let aliases = graph.metadata().nodes()[1].producers();
            assert_eq!(
                aliases
                    .iter()
                    .map(|p| (
                        p.dataset(),
                        p.schema_origin().declaring_source(),
                        p.schema_origin().written()
                    ))
                    .collect::<Vec<_>>(),
                [
                    ("RIGHT", "/root.yaml", "right.yaml"),
                    ("LEFT", "/root.yaml", "left.yaml"),
                    ("AGAIN", "/root.yaml", "./left.yaml")
                ]
            );
            assert!(matches!(
                yamaa_core::domain::check(graph.nodes()[1].document.model()),
                Err(PrepareError::Unsupported(_))
            ));
        }

        #[test]
        fn incomplete_duplicate_replaced_and_unreachable_supplied_nodes_are_denied() {
            let p = document("/p.yaml", &text("{RAW: never-read.csv}", "RAW", "p.csv"));
            let root = document(
                "/root.yaml",
                &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
            );
            let replaced = document(
                "/p.yaml",
                &text("{RAW: different-never-read.csv}", "RAW", "p.csv"),
            );
            let extra = document(
                "/extra.yaml",
                &text("{RAW: never-read.csv}", "RAW", "extra.csv"),
            );
            for (nodes, reason) in [
                (
                    vec![node(&root, vec![link("P", &p, "/p.csv")])],
                    "missing_producer_node",
                ),
                (
                    vec![
                        node(&root, vec![link("P", &p, "/p.csv")]),
                        node(&p, vec![]),
                        node(&p, vec![]),
                    ],
                    "duplicate_identity",
                ),
                (
                    vec![
                        node(&root, vec![link("P", &replaced, "/p.csv")]),
                        node(&p, vec![]),
                    ],
                    "contradictory_producer_document",
                ),
            ] {
                assert!(
                    matches!(prepare("/root.yaml", nodes, environment()), Err(Error::Boundary { reason: actual, .. }) if actual == reason)
                );
            }
            assert!(matches!(
                prepare(
                    "/root.yaml",
                    vec![
                        node(&root, vec![link("P", &p, "/p.csv")]),
                        node(&p, vec![]),
                        node(&extra, vec![])
                    ],
                    environment()
                ),
                Err(Error::Graph(CoreError::Boundary {
                    reason: "unreachable_node",
                    ..
                }))
            ));
        }

        struct Parent {
            bytes: Vec<u8>,
            calls: usize,
        }
        impl InheritancePort for Parent {
            type Error = ();
            fn canonicalize(
                &mut self,
                _: &str,
                written: &str,
            ) -> Result<Identity, SourceError<()>> {
                self.calls += 1;
                assert_eq!(written, "parent.yaml");
                Ok(Identity {
                    identity: "/shared-parent.yaml".into(),
                    display_path: "parent.yaml".into(),
                })
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<()>> {
                self.calls += 1;
                Ok(self.bytes.clone())
            }
            fn rebase(&mut self, _: &Identity, _: &Identity, written: &str) -> Result<String, ()> {
                self.calls += 1;
                Ok(written.into())
            }
        }
        fn inherited(identity: &str, text: &str, parent: &[u8]) -> (Arc<PreparedDocument>, Parent) {
            let mut port = Parent {
                bytes: parent.into(),
                calls: 0,
            };
            let bytes = text.replace("domain: TEST", "parents: parent.yaml\ndomain: TEST");
            let prepared = shipped_schema::prepare(
                Source {
                    identity: identity.into(),
                    bytes: bytes.into_bytes(),
                },
                identity.into(),
                &mut port,
            )
            .unwrap();
            (Arc::new(prepared), port)
        }

        #[test]
        fn snapshots_shared_only_across_distinct_branches_must_hold_identical_bytes() {
            let (a, a_port) = inherited(
                "/a.yaml",
                &text("{RAW: never-read.csv}", "RAW", "a.csv"),
                b"schema_version: '1.0'\n# first retained parent\n",
            );
            let (b, b_port) = inherited(
                "/b.yaml",
                &text("{RAW: never-read.csv}", "RAW", "b.csv"),
                b"schema_version: '1.0'\n# different retained parent\n",
            );
            let left = document(
                "/left.yaml",
                &text("{A: {path: a.csv, schema: a.yaml}}", "A", "left.csv"),
            );
            let right = document(
                "/right.yaml",
                &text("{B: {path: b.csv, schema: b.yaml}}", "B", "right.csv"),
            );
            let root = document("/root.yaml", &text("{LEFT: {path: left.csv, schema: left.yaml}, RIGHT: {path: right.csv, schema: right.yaml}}", "LEFT", "root.csv"));
            let before = (a_port.calls, b_port.calls);
            assert!(matches!(
                prepare(
                    "/root.yaml",
                    vec![
                        node(
                            &root,
                            vec![
                                link("LEFT", &left, "/left.csv"),
                                link("RIGHT", &right, "/right.csv")
                            ]
                        ),
                        node(&left, vec![link("A", &a, "/a.csv")]),
                        node(&right, vec![link("B", &b, "/b.csv")]),
                        node(&a, vec![]),
                        node(&b, vec![])
                    ],
                    environment()
                ),
                Err(Error::Boundary {
                    reason: "contradictory_snapshot",
                    ..
                })
            ));
            assert_eq!(before, (a_port.calls, b_port.calls));
        }

        #[test]
        fn equal_shared_parent_snapshots_and_written_inherited_origins_remain_retained() {
            let producer_text = text("{RAW: never-read.csv}", "RAW", "p.csv").replace(
                "output: {path: p.csv, columns: [ID, VALUE], decimals: 2}\n",
                "",
            );
            let parent = b"schema_version: '1.0'\noutput: {path: p.csv, columns: [ID, VALUE], decimals: 2}\n# retained parent\n";
            let (p, port) = inherited("/p.yaml", &producer_text, parent);
            let (root, root_port) = inherited(
                "/root.yaml",
                &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
                parent,
            );
            let before = (port.calls, root_port.calls);
            let graph = prepare(
                "/root.yaml",
                vec![node(&root, vec![link("P", &p, "/p.csv")]), node(&p, vec![])],
                environment(),
            )
            .unwrap();
            assert_eq!(
                graph.nodes()[1].document.parents()[0].source().bytes,
                parent
            );
            assert_eq!(
                graph.nodes()[0].document.parents()[0].source().bytes,
                parent
            );
            assert_eq!(
                graph.metadata().nodes()[0].producers()[0]
                    .output_origin()
                    .declaring_source(),
                "/shared-parent.yaml"
            );
            assert_eq!(
                graph.metadata().nodes()[0].producers()[0]
                    .output_origin()
                    .written(),
                "p.csv"
            );
            assert_eq!(before, (port.calls, root_port.calls));
        }

        #[test]
        fn changed_schema_bytes_are_rejected_before_capability() {
            let p_text = text("{RAW: never-read.csv}", "RAW", "p.csv");
            let root = document(
                "/root.yaml",
                &text("{P: {path: p.csv, schema: p.yaml}}", "P", "root.csv"),
            );
            let mut sources = shipped_schema::capture().unwrap().sources().to_vec();
            sources[0]
                .bytes
                .extend_from_slice(b"\n# changed schema closure\n");
            let schema =
                yamaa_adapters::specification_source::CapturedSchema::admit(sources, 0).unwrap();
            let changed = Arc::new(
                schema
                    .prepare_standalone(Source {
                        identity: "/p.yaml".into(),
                        bytes: p_text.into_bytes(),
                    })
                    .unwrap(),
            );
            assert!(matches!(
                prepare(
                    "/root.yaml",
                    vec![
                        node(&root, vec![link("P", &changed, "/p.csv")]),
                        node(&changed, vec![])
                    ],
                    environment()
                ),
                Err(Error::Boundary {
                    reason: "schema_mismatch",
                    ..
                })
            ));
        }

        #[test]
        fn original_inline_type_conflict_is_not_erased_by_complete_graph_admission() {
            let p = document("/p.yaml", &text("{RAW: never-read.csv}", "RAW", "p.csv"));
            let root = document(
                "/root.yaml",
                &text(
                    "{P: {path: p.csv, schema: p.yaml, types: {ID: int}}}",
                    "P",
                    "root.csv",
                ),
            );
            let Err(Error::Graph(CoreError::Admission {
                error: yamaa_core::producer_admission::Error::Invalid(findings),
                ..
            })) = prepare(
                "/root.yaml",
                vec![node(&root, vec![link("P", &p, "/p.csv")]), node(&p, vec![])],
                environment(),
            )
            else {
                panic!("original conflicting authority")
            };
            assert_eq!(findings[0].definition().condition, "redundant_field_type");
            assert_eq!(findings[0].definition().requirement, Some("REQ-0523"));
            assert_eq!(findings[0].spec_paths, ["input.P.types.ID"]);
        }
    }
    mod project_activation_diagnostics {
        use crate::support::project_call_compiler as support;
        use std::rc::Rc;
        use yamaa_adapters::project_activation_diagnostics::{
            binding_issues, test_issues, DefinitionSource, Error, HostDetails,
        };
        use yamaa_core::{
            function_invocation::{Argument, FailureKind, HostError, InvocationFailure},
            project_activation::{self, ActivationPort, BindingFailure, Failure, TestFailure},
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::{Case, Definition, Function, Language},
            value::{ColumnType, FiniteFloat, Value},
        };

        struct Port {
            payload: Rc<str>,
            bindings_fail: bool,
            calls_fail: bool,
            calls: usize,
            bindings: usize,
        }
        impl ActivationPort for Port {
            type Handle = ();
            type Error = Rc<str>;
            fn verify_lock(
                &mut self,
                _: Language,
                _: &LockReference,
                _: &[ProjectFunctionIdentity],
            ) -> Result<(), Rc<str>> {
                Ok(())
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Rc<str>> {
                self.bindings += 1;
                if self.bindings_fail {
                    Err(self.payload.clone())
                } else {
                    Ok(())
                }
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Rc<str>>> {
                self.calls += 1;
                if self.calls_fail {
                    return Err(HostError::Raised(self.payload.clone()));
                }
                let [Argument {
                    value: yamaa_core::table::ValueRef::Int(value),
                    ..
                }] = args
                else {
                    panic!("exact argument")
                };
                Ok(Value::Int(if *value == 7 { 8 } else { *value }))
            }
        }
        fn port() -> Port {
            Port {
                payload: Rc::from("é\0🙂 original"),
                bindings_fail: false,
                calls_fail: false,
                calls: 0,
                bindings: 0,
            }
        }
        fn lock() -> LockReference {
            LockReference {
                written: "uv.lock".into(),
                kind: LockKind::Uv,
            }
        }
        fn sources(functions: &[Function; 2]) -> [DefinitionSource<'_>; 2] {
            [
                DefinitionSource {
                    function: &functions[0],
                    path: "functions.first",
                },
                DefinitionSource {
                    function: &functions[1],
                    path: "functions.second",
                },
            ]
        }
        #[test]
        fn real_activation_mismatches_keep_every_case_in_selected_order_without_retry() {
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("actual case mismatch")
            };
            assert_eq!((port.bindings, port.calls), (2, 4));
            assert_eq!(failures.len(), 2);
            let issues = test_issues(&sources(&functions), &failures, &[None, None]).unwrap();
            assert_eq!(issues.len(), 2);
            for (index, issue) in issues.iter().enumerate() {
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    (
                        "validation",
                        "function_conformance_failed",
                        Some("REQ-0703")
                    )
                );
                assert_eq!(
                    issue.spec_paths,
                    [format!("{}.tests[0]", sources(&functions)[index].path)]
                );
                let context: yamaa_core::json::Value = issue.context.clone();
                assert_eq!(context["function"], functions[index].definition().name);
                assert_eq!(context["case"], "normal");
                assert_eq!(context["args"]["x"], 7);
                assert_eq!(context["expected"], 7);
                assert_eq!(context["actual"], 8);
                assert!(context.get("contract_version").is_none());
                assert!(context.get("implementation_version").is_none());
            }
            assert_eq!(
                test_issues(&sources(&functions), &failures, &[None, None]).unwrap(),
                issues
            );
            assert_eq!((port.bindings, port.calls), (2, 4));
        }
        #[test]
        fn actual_binding_and_invocation_errors_remain_original_non_send_payloads() {
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            port.bindings_fail = true;
            let Failure::Bindings(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("binding failures")
            };
            assert_eq!((port.bindings, port.calls), (2, 0));
            let Err(Error::Opaque(held)) =
                binding_issues(&sources(&functions), &failures, &[None, None])
            else {
                panic!("opaque binding flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
            let host = Some(HostDetails::Exception {
                class: "ValueError",
                message: "é\0🙂",
            });
            let issues = binding_issues(&sources(&functions), &failures, &[host, host]).unwrap();
            assert_eq!(issues.len(), 2);
            assert_eq!(issues[0].spec_paths, ["functions.first.function"]);
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0695"));
            port.bindings_fail = false;
            port.calls_fail = true;
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("call failures")
            };
            assert_eq!(failures.len(), 4);
            let details = [None, None, None, None];
            let Err(Error::Opaque(held)) = test_issues(&sources(&functions), &failures, &details)
            else {
                panic!("opaque test flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
            let issues =
                test_issues(&sources(&functions), &failures, &[host, host, host, host]).unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["failure"]["condition"], "function_call_failed");
            assert_eq!(context["failure"]["requirement"], "REQ-0701");
            assert_eq!(context["failure"]["context"]["host_message"], "é\0🙂");
            assert_eq!(port.calls, 4);
        }
        #[test]
        fn contradictory_indices_identity_result_and_duplicate_observations_return_no_issues() {
            let functions = [support::function("first"), support::function("second")];
            let selected = sources(&functions);
            for (function, case, actual, expected) in
                [(2, 0, 8, 7), (0, 9, 8, 7), (0, 0, 8, 9), (0, 0, 7, 7)]
            {
                let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                    function,
                    case,
                    actual: Value::Int(actual),
                    expected: Value::Int(expected),
                }];
                assert!(matches!(
                    test_issues(&selected, &failures, &[None]),
                    Err(Error::InvalidContext)
                ));
            }
            let failure: TestFailure<()> = TestFailure::Invocation {
                function: 0,
                case: 0,
                error: InvocationFailure {
                    identity: ProjectFunctionIdentity {
                        name: "second".into(),
                        call: "project.second".into(),
                    },
                    kind: FailureKind::BooleanResult,
                },
            };
            assert!(matches!(
                test_issues(&selected, &[failure], &[None]),
                Err(Error::InvalidContext)
            ));
            let failures = [
                BindingFailure {
                    function: 0,
                    error: (),
                },
                BindingFailure {
                    function: 0,
                    error: (),
                },
            ];
            let host = Some(HostDetails::Rejected {
                reason: "binding rejected",
                returned: None,
            });
            assert!(matches!(
                binding_issues(&selected, &failures, &[host, host]),
                Err(Error::InvalidContext)
            ));
        }
        #[test]
        fn complete_projection_keeps_long_escaped_paths_and_host_details() {
            let functions = [support::function("first"), support::function("second")];
            let selected = sources(&functions);
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: Value::Int(8),
                expected: Value::Int(7),
            }];
            assert_eq!(test_issues(&selected, &failures, &[None]).unwrap().len(), 1);
            let escaped = "\0".repeat(1_400_000);
            let huge = [DefinitionSource {
                function: &functions[0],
                path: &escaped,
            }];
            let issues = test_issues(&huge, &failures, &[None]).unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].spec_paths, [format!("{escaped}.tests[0]")]);
            let duplicate = vec![
                TestFailure::<()>::Result {
                    function: 0,
                    case: 0,
                    actual: Value::Int(8),
                    expected: Value::Int(7),
                },
                TestFailure::Result {
                    function: 0,
                    case: 0,
                    actual: Value::Int(8),
                    expected: Value::Int(7),
                },
            ];
            assert!(matches!(
                test_issues(&selected, &duplicate, &[None, None]),
                Err(Error::InvalidContext)
            ));
            let text = "é\0🙂".repeat(8193);
            let issues = binding_issues(
                &selected,
                &[BindingFailure {
                    function: 0,
                    error: (),
                }],
                &[Some(HostDetails::Exception {
                    class: "ValueError",
                    message: &text,
                })],
            )
            .unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["host_error"], "ValueError");
            assert_eq!(context["host_message"], text);
            assert!(context.get("host_details_truncated").is_none());
        }

        #[test]
        fn authored_float_bits_and_complete_scalar_values_remain_exact() {
            let float = |value| Value::Float(FiniteFloat::new(value).unwrap());
            let function = Function::admit(
                Language::Python,
                Definition {
                    name: "zero".into(),
                    function: "project.zero".into(),
                    description: "Zero".into(),
                    params: vec![],
                    returns: ColumnType::Float,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "zero".into(),
                        covers: vec![
                            "normal".into(),
                            "boundary".into(),
                            "numeric-comparison".into(),
                        ],
                        args: vec![],
                        result: float(0.0),
                    }],
                },
            )
            .unwrap();
            let selected = [DefinitionSource {
                function: &function,
                path: "functions.zero",
            }];
            let wrong: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: float(1.0),
                expected: float(-0.0),
            }];
            assert!(matches!(
                test_issues(&selected, &wrong, &[None]),
                Err(Error::InvalidContext)
            ));
            let correct: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: float(1.0),
                expected: float(0.0),
            }];
            let issues = test_issues(&selected, &correct, &[None]).unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(
                context["expected"].as_f64().unwrap().to_bits(),
                0.0_f64.to_bits()
            );
            assert_eq!(function.definition().tests[0].result, float(0.0));

            let integer = support::function("integer");
            let selected = [DefinitionSource {
                function: &integer,
                path: "functions.integer",
            }];
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 1,
                actual: Value::Int(i64::MAX),
                expected: Value::Int(i64::MIN),
            }];
            let issues = test_issues(&selected, &failures, &[None]).unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["expected"].as_i64(), Some(i64::MIN));
            assert_eq!(context["args"]["x"].as_i64(), Some(i64::MIN));
            assert_eq!(context["actual"].as_i64(), Some(i64::MAX));

            let text = Function::admit(
                Language::Python,
                Definition {
                    name: "text".into(),
                    function: "project.text".into(),
                    description: "Text".into(),
                    params: vec![],
                    returns: ColumnType::Str,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "text".into(),
                        covers: vec!["normal".into(), "boundary".into()],
                        args: vec![],
                        result: Value::Str("authored é\0🙂".into()),
                    }],
                },
            )
            .unwrap();
            let selected = [DefinitionSource {
                function: &text,
                path: "functions.text",
            }];
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: Value::Str("actual é\0🙂".into()),
                expected: Value::Str("authored é\0🙂".into()),
            }];
            let issues = test_issues(&selected, &failures, &[None]).unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["expected"], "authored é\0🙂");
            assert_eq!(context["actual"], "actual é\0🙂");
        }
    }
    mod project_codelist_diagnostics {
        use crate::support::project_codelist_compiler as support;
        use std::{cell::Cell, convert::Infallible};
        use support::{environment, schema, spec};
        use yamaa_adapters::project_codelist_diagnostics::{codelist_issues, Error};
        use yamaa_core::dataset::{CheckRecord, CodelistObservation, ExecutionError, RowIdentity};
        use yamaa_core::table::{CellError, TableAccess, TableSchema, ValueRef};
        use yamaa_core::{specification::PreparedSpecification, value::Value};
        struct Source {
            schema: TableSchema,
            reads: Cell<usize>,
        }
        impl TableAccess for Source {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                1
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                assert_eq!(row, 0);
                self.reads.set(self.reads.get() + 1);
                Ok(match column {
                    0 => ValueRef::Int(i64::MAX),
                    1 => ValueRef::Str("é\0🙂"),
                    _ => panic!("later source read"),
                })
            }
        }
        fn compiled() -> PreparedSpecification {
            PreparedSpecification::prepare_with_environment(
                &spec("SEX", "str", None, false, None),
                &environment(false),
            )
            .unwrap()
        }
        fn record(value: Value) -> CheckRecord {
            CheckRecord {
                path: "columns.CODE.submission.codelist".into(),
                code: yamaa_core::diagnostic::ConditionCode::VerificationCodelistFailed,
                evaluated_count: 2,
                failed_count: 1,
                output_rows: 2,
                offending_rows: vec![RowIdentity {
                    position: 1,
                    values: vec![Value::Int(i64::MAX)],
                }],
                inventory: None,

                row_count: None,
                codelist: Some(CodelistObservation {
                    id: "SEX".into(),
                    values: vec![value],
                }),
            }
        }
        #[test]
        fn complete_retained_values_and_full_width_keys_are_projected_without_output_or_sources() {
            let compiled = compiled();
            let source = Source {
                schema: schema(),
                reads: Cell::new(0),
            };
            let plan = compiled.bind(&source.schema).unwrap();
            let ExecutionError::VerificationFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                )
                .result
                .unwrap_err()
            else {
                panic!("actual fixed-list failure")
            };
            let reads = source.reads.get();
            let before = records[0].codelist.as_ref().unwrap().values.as_ptr();
            let issues = codelist_issues(&compiled, &records).unwrap();
            assert_eq!(issues.len(), 1);
            let issue = &issues[0];
            assert_eq!(
                (
                    issue.phase.as_str(),
                    issue.condition.as_str(),
                    issue.requirement.as_deref()
                ),
                ("verification", "allowed_values_failed", Some("REQ-0957"))
            );
            assert_eq!(issue.spec_paths, ["columns.CODE.submission.codelist"]);
            let context: yamaa_core::json::Value = issue.context.clone();
            assert_eq!(context["column"], "CODE");
            assert_eq!(context["codelist"], "SEX");
            assert_eq!(context["keys"][0]["ID"].as_i64(), Some(i64::MAX));
            assert_eq!(context["values"], yamaa_adapters::json!(["é\0🙂"]));
            assert_eq!(codelist_issues(&compiled, &records).unwrap(), issues);
            assert_eq!(
                before,
                records[0].codelist.as_ref().unwrap().values.as_ptr()
            );
            assert_eq!(source.reads.get(), reads);
        }
        #[test]
        fn contradictory_observation_geometry_never_returns_partial_or_invented_issues() {
            let compiled = compiled();
            let original = record(Value::Str("bad".into()));
            for case in 0..6 {
                let mut wrong = original.clone();
                match case {
                    0 => wrong.codelist = None,
                    1 => wrong.failed_count = 2,
                    2 => wrong.codelist.as_mut().unwrap().id = "OTHER".into(),
                    3 => wrong.offending_rows[0].values.clear(),
                    4 => wrong.codelist.as_mut().unwrap().values[0] = Value::Missing,
                    _ => {
                        wrong.code =
                            yamaa_core::diagnostic::ConditionCode::VerificationAllowedValuesFailed
                    }
                }
                assert_eq!(
                    codelist_issues(&compiled, &[wrong]),
                    Err(Error::InvalidObservation)
                );
            }
            let mut passing = original;
            passing.failed_count = 0;
            passing.offending_rows.clear();
            passing.codelist.as_mut().unwrap().values.clear();
            assert!(codelist_issues(&compiled, &[passing]).unwrap().is_empty());
        }
        #[test]
        fn complete_escaped_values_and_large_retained_observations_are_projected() {
            let compiled = compiled();
            let text = "\0".repeat(1_500_000);
            let records = vec![record(Value::Str(text.clone()))];
            let issues = codelist_issues(&compiled, &records).unwrap();
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["values"], yamaa_adapters::json!([text]));
            let mut many = record(Value::Str("bad".into()));
            many.failed_count = 65_537;
            many.evaluated_count = 65_537;
            many.output_rows = 65_537;
            many.codelist.as_mut().unwrap().values = vec![Value::Int(1); 65_537];
            many.offending_rows = (0..65_537)
                .map(|position| RowIdentity {
                    position,
                    values: vec![Value::Int(i64::MAX)],
                })
                .collect();
            let issues = codelist_issues(&compiled, &[many]).unwrap();
            assert_eq!(issues.len(), 1);
            let context: yamaa_core::json::Value = issues[0].context.clone();
            assert_eq!(context["failure_count"], 65_537);
            assert_eq!(context["values"].as_array().unwrap().len(), 65_537);
            assert_eq!(context["keys"].as_array().unwrap().len(), 65_537);
            assert_eq!(context["keys"][65_536]["ID"].as_i64(), Some(i64::MAX));
        }
    }
    mod project_environment_diagnostics {
        use std::{rc::Rc, sync::Arc};
        use yamaa_adapters::{
            project_environment_diagnostics::{admission, Error as ProjectionError},
            project_source::{
                self, CaptureFailure, Failure, Kind, ProjectCapturePort, Reply, Request,
            },
            specification_source::{CapturedSchema, Source},
        };
        use yamaa_core::json::Value;
        use yamaa_core::{project_environment::LockKind, project_function::Language};
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        fn schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit_root(
                vec![
                    source(
                        "schema_environment.yaml",
                        include_str!("fixtures/environment-candidate/schema_environment.yaml"),
                    ),
                    source(
                        "schema_shared.yaml",
                        include_str!("fixtures/environment-candidate/schema_shared.yaml"),
                    ),
                ],
                0,
                "environment_class",
            )
            .unwrap()
        }
        struct Port {
            payload: Rc<()>,
            same_identity: bool,
            reads: usize,
            wide: bool,
            shape: bool,
        }
        impl ProjectCapturePort for Port {
            type Error = Rc<()>;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Rc<()>> {
                self.reads += 1;
                if request.written == "bad.yaml" {
                    return Err(Rc::clone(&self.payload));
                }
                Ok(match request.kind {
            Kind::Lock => Reply::Lock { source:source("held/uv.lock","version = 1\n"),kind:LockKind::Uv },
            Kind::Function if self.shape => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: []\ncontract_version: '1'\nbinding: {call: project.constant}\n")),
            Kind::Codelist if self.shape => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\nlegacy: true\n")),
            Kind::Function if self.wide => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 123456789012345678901234567890}]\n")),
            Kind::Codelist if self.wide => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n")),
            Kind::Function => Reply::Document(source(if self.same_identity { "study/environment.yaml" } else { "held/function.yaml" }, "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n")),
            Kind::Codelist => Reply::Document(source("held/ct.yaml","codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n")),
        })
            }
        }
        fn rejected(
            same_identity: bool,
        ) -> (Box<project_source::RejectedEnvironment<Rc<()>>>, Rc<()>) {
            let payload = Rc::new(());
            let mut port = Port {
                payload: Rc::clone(&payload),
                same_identity,
                reads: 0,
                wide: false,
                shape: false,
            };
            let root=source("study/environment.yaml","schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n");
            let Failure::Rejected(rejected) =
                project_source::prepare(schema(), root, Language::Python, &mut port).unwrap_err()
            else {
                panic!("retained rejection")
            };
            assert_eq!(port.reads, 4);
            (rejected, payload)
        }
        fn context(issue: &yamaa_adapters::issue_rows::Issue) -> Value {
            issue.context.clone()
        }
        #[test]
        fn static_admission_restores_external_and_inline_origins_after_an_earlier_capture_failure()
        {
            let (rejected, payload) = rejected(false);
            assert_eq!(rejected.origins.codelists, [1, 2]);
            let issues = admission(&rejected).unwrap();
            let function = issues
                .iter()
                .find(|i| context(i)["reason"] == "invalid_case_result")
                .unwrap();
            assert_eq!(function.spec_paths, ["tests[0].result"]);
            assert_eq!(context(function)["source"], "held/function.yaml");
            assert_eq!(context(function)["entry"], "study/environment.yaml");
            assert_eq!(context(function)["environment_path"], "functions.first");
            assert_eq!(context(function)["value"], true);
            let lists = issues
                .iter()
                .filter(|i| i.requirement.as_deref() == Some("REQ-0950"))
                .collect::<Vec<_>>();
            assert_eq!(lists.len(), 2);
            assert_eq!(lists[0].spec_paths, ["codelists[0].items[0].value"]);
            assert_eq!(context(lists[0])["source"], "held/ct.yaml");
            assert_eq!(context(lists[0])["environment_path"], "codelists[1]");
            assert_eq!(
                lists[1].spec_paths,
                ["codelists[2].codelists[0].items[0].value"]
            );
            assert_eq!(context(lists[1])["source"], "study/environment.yaml");
            assert_eq!(context(lists[1])["environment_path"], "codelists[2]");
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!("held host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
            let again = admission(&rejected).unwrap();
            assert_eq!(again, issues);
        }
        #[test]
        fn captured_external_document_ownership_does_not_depend_on_identity_text_equality() {
            let (rejected, _) = rejected(true);
            let issues = admission(&rejected).unwrap();
            let function = issues
                .iter()
                .find(|i| context(i)["reason"] == "invalid_case_result")
                .unwrap();
            assert_eq!(function.spec_paths, ["tests[0].result"]);
            assert_eq!(context(function)["source"], "study/environment.yaml");
            assert_eq!(context(function)["environment_path"], "functions.first");
        }
        #[test]
        fn invalid_origins_refuse_while_retaining_original_rejection() {
            let (mut rejected, payload) = rejected(false);
            assert_eq!(admission(&rejected).unwrap().len(), 5);
            rejected.origins.functions.clear();
            assert_eq!(admission(&rejected), Err(ProjectionError::InvalidOrigin));
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!("held host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
        }

        #[test]
        fn scalar_decode_diagnostics_keep_every_case_and_the_complete_function_name() {
            use yamaa_adapters::project_environment_diagnostics::scalar_root;
            let name = "f".repeat(100);
            let cases = (0..40)
        .map(|n| {
            format!("{{id: case-{n}, covers: [normal], args: {{}}, result: 9223372036854775808}}")
        })
        .collect::<Vec<_>>()
        .join(",");
            let body = format!("schema_version: '1.0'\nfunctions:\n  ? {name}\n  : {{function: project.constant, description: Constant, params: [], returns: int, tests: [{cases}]}}\n");
            let mut port = Port {
                payload: Rc::new(()),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: false,
            };
            let Err(Failure::RootScalar { root, findings }) = project_source::prepare(
                schema(),
                source("study/environment.yaml", &body),
                Language::Python,
                &mut port,
            ) else {
                panic!("retained scalar findings")
            };
            assert_eq!(findings.len(), 40);
            let issues = scalar_root(&root, &findings).unwrap();
            assert_eq!(issues.len(), 40);
            for (index, issue) in issues.iter().enumerate() {
                assert_eq!(
                    issue.spec_paths,
                    [format!("functions.{name}.tests[{index}].result")]
                );
                assert_eq!(
                    yamaa_adapters::json_wire::encode(&context(issue)["value"]),
                    "9223372036854775808"
                );
                assert_eq!(context(issue)["reason"], "integer_out_of_range");
            }
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn root_scalar_issues_keep_arbitrary_integers_and_invalid_temporal_text_without_dependency_reads(
        ) {
            use yamaa_adapters::project_environment_diagnostics::scalar_root;
            let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n";
            let mut port = Port {
                payload: Rc::new(()),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: false,
            };
            let Failure::RootScalar { root, findings } = project_source::prepare(
                schema(),
                source("held/environment.yaml", body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("complete root scalars")
            };
            let issues = scalar_root(&root, &findings).unwrap();
            assert_eq!(issues.len(), 3);
            assert_eq!(issues[0].spec_paths, ["functions.wide.tests[0].result"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&context(&issues[0])["value"]),
                "123456789012345678901234567890"
            );
            assert_eq!(context(&issues[1])["value"], "2024-02-30");
            assert_eq!(context(&issues[2])["value"], "2024-01-01T25:00:00");
            for issue in &issues {
                assert_eq!(
                    (
                        issue.phase.as_str(),
                        issue.condition.as_str(),
                        issue.requirement.as_deref()
                    ),
                    (
                        "validation",
                        "project_environment_invalid",
                        Some("REQ-0695")
                    )
                );
                assert_eq!(context(issue)["source"], "held/environment.yaml");
                assert_eq!(context(issue)["entry"], "held/environment.yaml");
                assert_eq!(context(issue)["environment_path"], "$");
            }
            assert_eq!(scalar_root(&root, &findings).unwrap(), issues);
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn failed_external_scalar_issues_keep_each_document_origin_and_original_other_failures() {
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            let payload = Rc::new(());
            let mut port = Port {
                payload: Rc::clone(&payload),
                same_identity: false,
                reads: 0,
                wide: true,
                shape: false,
            };
            let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
            let Failure::Rejected(rejected) = project_source::prepare(
                schema(),
                source("study/environment.yaml", body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("independent source scalars")
            };
            let issues = scalar_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 2);
            assert_eq!(issues[0].spec_paths, ["tests[0].result"]);
            assert_eq!(context(&issues[0])["source"], "held/function.yaml");
            assert_eq!(context(&issues[0])["environment_path"], "functions.first");
            assert_eq!(issues[1].spec_paths, ["codelists[0].items[0].value"]);
            assert_eq!(context(&issues[1])["source"], "held/ct.yaml");
            assert_eq!(context(&issues[1])["environment_path"], "codelists[1]");
            for issue in &issues {
                assert_eq!(
                    yamaa_adapters::json_wire::encode(&context(issue)["value"]),
                    "123456789012345678901234567890"
                );
                assert_eq!(context(issue)["entry"], "study/environment.yaml");
            }
            let CaptureFailure::Port(original) = &rejected.sources[1].error else {
                panic!("held original failure")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(port.reads, 4);
            assert_eq!(scalar_sources(&rejected).unwrap(), issues);
            assert_eq!(port.reads, 4);
        }

        #[test]
        fn root_schema_issues_keep_actual_closed_fields_and_source_without_dependency_reads() {
            use yamaa_adapters::{
                project_environment_diagnostics::schema_root,
                specification_source::Error as SourceError,
            };
            let mut port = Port {
                payload: Rc::new(()),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: false,
            };
            let body = "schema_version: '1.0'\nparents: []\nruntime: {}\nversion: '1'\n";
            let Failure::Root(SourceError::Findings(captured)) = project_source::prepare(
                schema(),
                source("held/environment.yaml", body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("retained root schema findings")
            };
            let issues = schema_root(&captured).unwrap();
            assert_eq!(issues.len(), 3);
            for (issue, field) in issues.iter().zip(["parents", "runtime", "version"]) {
                assert_eq!(issue.spec_paths, [field]);
                assert_eq!(issue.condition, "unknown_field");
                assert_eq!(issue.requirement, Some("REQ-0285".into()));
                assert_eq!(context(issue)["field"], field);
                assert_eq!(context(issue)["class"], "environment_class");
                assert_eq!(context(issue)["source"], "held/environment.yaml");
                assert_eq!(context(issue)["entry"], "held/environment.yaml");
                assert_eq!(context(issue)["environment_path"], "$");
            }
            assert_eq!(schema_root(&captured).unwrap(), issues);
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn failed_external_schema_issues_use_held_arenas_and_keep_original_other_failures() {
            use yamaa_adapters::project_environment_diagnostics::schema_sources;
            let payload = Rc::new(());
            let mut port = Port {
                payload: Rc::clone(&payload),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: true,
            };
            let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
            let Failure::Rejected(rejected) = project_source::prepare(
                schema(),
                source("study/environment.yaml", body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("independent captured schema failures")
            };
            let issues = schema_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 3);
            for (issue, field, class, source, declaration) in [
                (
                    &issues[0],
                    "contract_version",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions.first",
                ),
                (
                    &issues[1],
                    "binding",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions.first",
                ),
                (
                    &issues[2],
                    "legacy",
                    "codelist_source_class",
                    "held/ct.yaml",
                    "codelists[1]",
                ),
            ] {
                assert_eq!(issue.spec_paths, [format!("<normalization>.{field}")]);
                assert_eq!(issue.condition, "unknown_field");
                assert_eq!(issue.requirement, Some("REQ-0285".into()));
                assert_eq!(context(issue)["field"], field);
                assert_eq!(context(issue)["class"], class);
                assert_eq!(context(issue)["source"], source);
                assert_eq!(context(issue)["entry"], "study/environment.yaml");
                assert_eq!(context(issue)["environment_path"], declaration);
            }
            let CaptureFailure::Port(original) = &rejected.sources[1].error else {
                panic!("held original host failure")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(schema_sources(&rejected).unwrap(), issues);
            assert_eq!(port.reads, 4);
        }

        #[test]
        fn invalid_inline_scalars_do_not_mask_independent_root_header_findings() {
            use yamaa_adapters::project_environment_diagnostics::scalar_root_with_host;
            for language in ["", "language: r\n"] {
                let body = format!("schema_version: '1.0'\n{language}functions: {{bad: {{function: project.constant, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal], args: {{}}, result: 123456789012345678901234567890}}]}}}}\nsdtm: {{standard: {{name: SDTMIG, version: '3.4', status: Final}}, specs: [], define: define.xml}}\n");
                let mut port = Port {
                    payload: Rc::new(()),
                    same_identity: false,
                    reads: 0,
                    wide: false,
                    shape: false,
                };
                let Failure::RootScalar { root, findings } = project_source::prepare(
                    schema(),
                    source("held/environment.yaml", &body),
                    Language::Python,
                    &mut port,
                )
                .unwrap_err() else {
                    panic!("held root scalars")
                };
                let issues = scalar_root_with_host(&root, &findings, Language::Python).unwrap();
                assert_eq!(issues.len(), 3);
                let expected = ["language", "study", "functions.bad.tests[0].result"];
                assert_eq!(
                    issues
                        .iter()
                        .map(|i| i.spec_paths[0].as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                let mismatch = if language.is_empty() {
                    None
                } else {
                    Some(&issues[0])
                };
                if let Some(issue) = mismatch {
                    assert_eq!(issue.requirement.as_deref(), Some("REQ-0696"));
                    assert_eq!(context(issue)["declared"], "r");
                    assert_eq!(context(issue)["runner"], "python");
                }
                assert_eq!(context(&issues[1])["reason"], "missing_study");
                for issue in &issues {
                    assert_eq!(context(issue)["source"], "held/environment.yaml");
                    assert_eq!(context(issue)["environment_path"], "$");
                }
                assert_eq!(
                    scalar_root_with_host(&root, &findings, Language::Python).unwrap(),
                    issues
                );
                assert_eq!(port.reads, 0);
            }
        }

        #[test]
        fn large_capture_counts_preserve_complete_reports_and_original_origins() {
            use yamaa_adapters::project_environment_diagnostics::{scalar_sources, schema_sources};
            for written in ["bad.yaml", "ct.yaml"] {
                let payload = Rc::new(());
                let mut port = Port {
                    payload: Rc::clone(&payload),
                    same_identity: false,
                    reads: 0,
                    wide: false,
                    shape: false,
                };
                let declarations = vec![written; 129].join(", ");
                let body = format!(
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\ncodelists: [{declarations}]\n"
        );
                let Failure::Rejected(rejected) = project_source::prepare(
                    schema(),
                    source("study/environment.yaml", &body),
                    Language::Python,
                    &mut port,
                )
                .unwrap_err() else {
                    panic!("retained valid capture records")
                };
                assert_eq!(port.reads, 130);
                if written == "bad.yaml" {
                    assert_eq!(rejected.sources.len(), 129);
                    assert!(scalar_sources(&rejected).unwrap().is_empty());
                    assert!(schema_sources(&rejected).unwrap().is_empty());
                    let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                        panic!("original payload")
                    };
                    assert!(Rc::ptr_eq(original, &payload));
                } else {
                    assert_eq!(rejected.captures.len(), 129);
                    let issues = admission(&rejected).unwrap();
                    let mut wanted = vec![];
                    for index in 0..129 {
                        if index > 0 {
                            wanted.push((
                                index,
                                "duplicate_define_identifier",
                                Some("id"),
                                "codelists[0].id",
                            ));
                            wanted.push((
                                index,
                                "duplicate_define_identifier",
                                Some("name"),
                                "codelists[0].name",
                            ));
                        }
                        wanted.push((
                            index,
                            "codelist_shape_invalid",
                            None,
                            "codelists[0].items[0].value",
                        ));
                    }
                    assert_eq!(issues.len(), 129 + 2 * 128);
                    for (issue, (index, condition, field, path)) in issues.iter().zip(wanted) {
                        let context = context(issue);
                        assert_eq!(issue.condition, condition);
                        assert_eq!(issue.spec_paths, [path]);
                        assert_eq!(context["field"].as_str(), field);
                        assert_eq!(context["environment_path"], format!("codelists[{index}]"));
                        assert_eq!(context["source"], "held/ct.yaml");
                        if field.is_none() {
                            assert_eq!(context["value"], true);
                        }
                    }
                }
                assert_eq!(port.reads, 130);
            }
        }
    }
}
