#![cfg(feature = "yaml")]
//! File contracts: file workflow.
mod file {
    mod file_workflow {
        use crate::support::ports::NoFunctions;
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::{
            file_resources::Resources,
            file_workflow::{self, Error},
            project_application::{preparation_issues, IssueError},
            specification_check,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::project_function::Language;

        #[test]
        fn unavailable_environment_precedes_bad_entry_and_valid_environment_is_retained_before_missing_entry(
        ) {
            let study = Study::new();
            let entry = study.path("root.yaml");
            let project = study.path("missing-project.yaml");
            for entry_bytes in [None, Some("[invalid entry that must not be read")] {
                if let Some(bytes) = entry_bytes {
                    study.write("root.yaml", bytes);
                }
                // The project file is the caller's own path: it is refused
                // before anything is captured, the entry included (#3321).
                let rejection = file_workflow::select(&entry, &project, Language::Python)
                    .err()
                    .unwrap();
                let file_workflow::SelectionError::Preparation(Error::Caller { path, error }) =
                    &rejection
                else {
                    panic!("original unavailable project path")
                };
                // Its issue names the path as given (`workflow.json`).
                assert_eq!(path, &project);
                assert_eq!(*error, yamaa_adapters::file_resources::Error::Missing);
                assert_eq!(rejection.retained_sources().len(), 0);
            }
            fs::remove_file(study.0.join("root.yaml")).unwrap();
            // A valid project whose only environment entry names another host's
            // lock: the Python host retains the project without reading that lock.
            let project = study.path("project.yaml");
            let raw =
                "yamaa: \"1.0\"\ntype: project\nenvironments: {r: {lock: r/absent-renv.lock}}\n";
            study.write("project.yaml", raw);
            let file_workflow::SelectionError::Preparation(Error::Entry(owner)) =
                file_workflow::select(&entry, &project, Language::Python)
                    .err()
                    .unwrap()
            else {
                panic!("valid project precedes missing entry")
            };
            // The missing entry is the caller's own path, named as given; the
            // captured project stays held.
            assert!(matches!(
                owner.error(),
                yamaa_adapters::file_entry::Error::Caller {
                    path,
                    error: yamaa_adapters::file_resources::Error::Missing,
                } if *path == entry
            ));
            assert_eq!(owner.capture_reads(), 1);
            let sources = owner.captured_sources().collect::<Vec<_>>();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].0, project);
            assert_eq!(sources[0].1, raw.as_bytes());
            assert!(!study.0.join("input.csv").exists());
        }
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("workflow-check");
                let study = Self(path);
                // A project that declares no function.
                study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
                study
            }
            fn path(&self, written: &str) -> String {
                let path = self.0.join(written).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn write(&self, written: &str, text: &str) {
                let path = self.0.join(written);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            fn prepare(&self) -> yamaa_adapters::file_preparation::FileSpecification {
                match self.select() {
                    Ok(project) => project,
                    Err(error) => panic!("project preparation: {error:?}"),
                }
            }
            fn rejected(&self) -> Error {
                match self.select() {
                    Err(file_workflow::SelectionError::Preparation(error)) => error,
                    Ok(_) => panic!("expected preparation rejection"),
                    Err(error) => panic!("expected preparation rejection: {error:?}"),
                }
            }
            fn compile_rejection(&self) -> Box<yamaa_adapters::file_preparation::Rejected> {
                match self.select() {
                    Err(file_workflow::SelectionError::Compile(error)) => error,
                    Ok(_) => panic!("expected compilation rejection"),
                    Err(error) => panic!("expected compilation rejection: {error:?}"),
                }
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                if self.0.exists() {
                    fs::remove_dir_all(&self.0).unwrap();
                }
            }
        }
        fn yaml(input: &str, base: &str, output: &str, invalid: bool) -> String {
            let checks = if invalid {
                "\n    verifications: [{matches: {pattern: \"\\u00e9(\"}}]"
            } else {
                ""
            };
            format!("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - name: VALUE\n    type: str\n    label: Reported value\n    derivation: {base}.VALUE{checks}\noutput: {{path: {output}, columns: [ID, VALUE]}}\n")
        }

        #[test]
        fn seventeen_column_verifications_keep_their_identities_without_reading_data() {
            let study = Study::new();
            let mut verifications = String::from("    verifications:\n");
            for index in 0..17 {
                verifications.push_str(&format!("      - not_missing: {{id: CHECK{index}}}\n"));
            }
            let source = yaml("{RAW: never-read.csv}", "RAW", "result.csv", false).edit(
                "    derivation: RAW.VALUE\n",
                &format!("    derivation: RAW.VALUE\n{verifications}"),
            );
            study.write("root.yaml", &source);
            let project = study.prepare();
            assert_eq!(project.capture_reads(), 2);
            assert!(specification_check::issues(project.run()).is_empty());
            let compiled = project.run().compiled();
            for index in 0..17 {
                let path = format!("columns.VALUE.verifications[{index}].not_missing");
                assert_eq!(
                    compiled.verification_identity(&path),
                    Some(format!("CHECK{index}").as_str())
                );
                assert!(matches!(
                    compiled.column_check(&path),
                    Some(yamaa_core::dataset::Check::NotMissing {
                        when: None,
                        max_fraction: None
                    })
                ));
            }
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("result.csv").exists());
        }

        #[test]
        fn raw_schema_checks_keep_all_findings_and_public_checks_take_the_first_ten_thousand() {
            let study = Study::new();
            let mut source = yaml("{RAW: never-read.csv}", "RAW", "result.csv", false);
            // Independent unknown root fields are all observed by schema validation.
            // Payload compilers keep their separate first-invalid-prefix semantics.
            for index in 0..10_001 {
                source.push_str(&format!("extra_{index:05}: true\n"));
            }
            assert!(source.len() < 8 * 1024 * 1024);
            study.write("root.yaml", &source);

            let owner = study.rejected();
            let Error::Entry(entry) = &owner else {
                panic!("schema rejection owner")
            };
            assert_eq!(entry.capture_reads(), 2); // project and specification only
            let raw = preparation_issues(&owner).unwrap();
            assert_eq!(raw.len(), 10_001);
            // A 1.x specification's findings name the layer that writes the
            // field and the entry, here both the specification (LAYER-4).
            let root = study.path("root.yaml");
            for (index, issue) in raw.iter().enumerate() {
                let name = format!("extra_{index:05}");
                assert_eq!(
                    yamaa_adapters::report_storage::diagnostic_value(issue),
                    yamaa_adapters::json!({"phase": "validation", "condition": "unknown_field", "rule": "SCHEMA-13", "spec_paths": vec![name.clone()], "context": {"class":"root_class", "field":name, "identifier":name, "source": root, "entry": root}}),
                    "root field {index} keeps its complete finding in authored order"
                );
            }

            let public = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            );
            assert_eq!(public.len(), 10_000);
            assert_eq!(public, raw[..10_000]);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("result.csv").exists());
        }

        #[test]
        fn original_decode_diagnostics_remain_bounded_and_owned() {
            let study = Study::new();
            study.write("root.yaml", "\u{e9}\n");
            let owner = study.rejected();
            let expected = vec![
                yamaa_adapters::json!({"phase": "validation", "condition": "non_ascii_source", "rule": "SPEC-8", "spec_paths": ["$"], "context": {"line":1, "column":1, "path":study.path("root.yaml")}}),
            ];
            assert_eq!(
                crate::support::rows(&preparation_issues(&owner).unwrap()),
                expected
            );
            assert_eq!(
                crate::support::rows(&yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("project.yaml"),
                    Language::Python
                )),
                expected
            );
            // An entry and a project file that are not YAML fail at their
            // root (`workflow.json`); the held findings outlive the study.
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                crate::support::rows(&preparation_issues(&owner).unwrap()),
                expected
            );
        }

        #[test]
        fn metadata_failures_name_each_written_dependency_and_cause_before_study_reads() {
            let study = Study::new();
            // The project declares the lock, the function file and the
            // codelist file, each relative to itself (D8).
            study.write(
                "project.yaml",
                "yamaa: '1.0'\ntype: project\nfunctions: [absent-function.yaml]\nenvironments: {python: {lock: missing.lock}}\ncodelists: [absent-codelists.yaml]\n",
            );
            let entry = study.path("root.yaml");
            let project = study.path("project.yaml");
            let file_workflow::SelectionError::Preparation(error) =
                file_workflow::select(&entry, &project, Language::Python)
                    .err()
                    .unwrap()
            else {
                panic!("retained environment rejection")
            };
            // A file the project names that does not exist fails at the
            // field that names it, naming the path as written, as any file a
            // run reads does (FUNC-2, FILE-7, FILE-11, #3030 D107).
            let expected = [
                ("environments.python.lock", "missing.lock"),
                ("functions[0]", "absent-function.yaml"),
                ("codelists[0]", "absent-codelists.yaml"),
            ]
            .map(|(declaration, path)| {
                yamaa_adapters::json!({"phase": "validation", "condition": "resource_path_missing",
                    "rule": "FILE-7", "spec_paths": [declaration], "context": {"path": path}})
            });
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(crate::support::rows(&rows), expected);
            assert_eq!(
                rows,
                yamaa_adapters::project_application::check(&entry, &project, Language::Python),
            );
            let Error::Environment(owner) = &error else {
                unreachable!()
            };
            assert_eq!(owner.capture_reads(), 1); // the project alone
            assert!(!study.0.join("root.yaml").exists());
            // Files that exist but cannot be read as a lock or as YAML name
            // their causes (`workflow.json`).
        }

        #[test]
        fn per_source_resource_and_decoder_limits_keep_original_facts_without_partial_diagnostics()
        {
            use yamaa_adapters::{
                file_resources::Error as ResourceError,
                project_source::{CaptureFailure, Failure},
                specification_source::Error as SourceError,
                yaml_decode::DecodeFailure,
            };
            use yamaa_core::limits::{LimitExceeded, Limits};

            for structural in [false, true] {
                let study = Study::new();
                let project = "yamaa: \"1.0\"\ntype: project\ncodelists: [missing.yaml, ct.yaml]\n";
                study.write("project.yaml", project);
                let metadata = if structural {
                    format!(
                        "{}0{}",
                        "[".repeat(Limits::YAML_DEPTH + 1),
                        "]".repeat(Limits::YAML_DEPTH + 1),
                    )
                } else {
                    "#".repeat(Limits::SPEC_BYTES + 1)
                };
                study.write("ct.yaml", &metadata);
                let expected = if structural {
                    LimitExceeded {
                        resource: "yaml_depth",
                        limit: Limits::YAML_DEPTH,
                        actual: Some(Limits::YAML_DEPTH + 1),
                    }
                } else {
                    LimitExceeded {
                        resource: "spec_bytes",
                        limit: Limits::SPEC_BYTES,
                        actual: Some(Limits::SPEC_BYTES + 1),
                    }
                };
                let selection = file_workflow::select(
                    &study.path("root.yaml"),
                    &study.path("project.yaml"),
                    Language::Python,
                )
                .err()
                .unwrap();
                let file_workflow::SelectionError::Preparation(error) = &selection else {
                    panic!("retained metadata failure precedes entry reads")
                };
                let Error::Environment(owner) = error else {
                    panic!("original environment owner")
                };
                let yamaa_adapters::file_project::Error::Environment(Failure::Rejected(rejected)) =
                    owner.error()
                else {
                    panic!("original source failures remain owned: {:?}", owner.error())
                };
                assert_eq!(rejected.sources.len(), 2);
                assert_eq!(rejected.sources[0].written, "missing.yaml");
                assert!(matches!(
                    rejected.sources[0].error,
                    CaptureFailure::Port(ResourceError::Missing)
                ));
                assert_eq!(rejected.sources[1].written, "ct.yaml");
                let actual = match &rejected.sources[1].error {
                    CaptureFailure::Port(ResourceError::Limit(limit)) if !structural => limit,
                    CaptureFailure::Structural(SourceError::Decode {
                        error: DecodeFailure::Limit(limit),
                        ..
                    }) if structural => limit,
                    failure => panic!("retained typed limit: {failure:?}"),
                };
                assert_eq!(actual, &expected);
                assert_eq!(rejected.root.source().bytes, project.as_bytes());
                assert_eq!(owner.capture_reads(), if structural { 2 } else { 1 });
                let reads = owner.capture_reads();
                let held_root = rejected.root.source().bytes.as_ptr();
                let held_error = owner.error() as *const _;
                let IssueError::ResourceLimit(refusal) = preparation_issues(error).unwrap_err()
                else {
                    panic!("whole projection retains the typed refusal")
                };
                assert_eq!(refusal.resource, expected.resource);
                assert_eq!(refusal.limit, expected.limit);
                assert_eq!(refusal.actual, expected.actual);
                assert_eq!(refusal.spec_paths, ["codelists[1]"]);
                let expected_rows = refusal.issues(
                    yamaa_core::application_issue::Step::Environment,
                    yamaa_core::application_issue::Fault::ResourceLimit,
                );
                assert_eq!(
                    yamaa_adapters::project_application::check(
                        &study.path("root.yaml"),
                        &study.path("project.yaml"),
                        Language::Python,
                    ),
                    expected_rows,
                );
                let application_error = yamaa_adapters::project_application::prepare(
                    &study.path("root.yaml"),
                    &study.path("project.yaml"),
                    Language::Python,
                )
                .err()
                .unwrap();
                let yamaa_adapters::project_application::Error::Project(
                    yamaa_adapters::file_project::Error::Environment(Failure::Rejected(
                        application_owner,
                    )),
                ) = &application_error
                else {
                    panic!("public preparation retains the native environment failure")
                };
                assert_eq!(application_owner.sources.len(), 2);
                let held_application_root = application_owner.root.source().bytes.as_ptr();
                assert!(!study.0.join("root.yaml").exists());
                fs::remove_dir_all(&study.0).unwrap();
                for _ in 0..2 {
                    assert!(matches!(
                        preparation_issues(error),
                        Err(IssueError::ResourceLimit(again)) if again == refusal
                    ));
                    let rows = yamaa_adapters::project_application::selection_failure(&selection);
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].phase(), "engine");
                    assert_eq!(rows[0].condition(), "engine_rejected");
                    assert_eq!(rows[0].rule(), None);
                    assert_eq!(rows[0].spec_paths, ["codelists[1]"]);
                    assert_eq!(rows, expected_rows);
                    assert_eq!(
                        yamaa_core::json::Value::Object(rows[0].context.clone()),
                        yamaa_adapters::json!({
                            "step": "environment", "code": "resource_limit",
                            "resource": expected.resource, "limit": expected.limit,
                            "actual": expected.actual,
                        }),
                    );
                    assert_eq!(
                        yamaa_adapters::project_application::failure(&application_error),
                        rows,
                    );
                    assert_eq!(owner.capture_reads(), reads);
                    assert_eq!(owner.error() as *const _, held_error);
                    assert_eq!(rejected.root.source().bytes.as_ptr(), held_root);
                    assert_eq!(rejected.root.source().bytes, project.as_bytes());
                    assert_eq!(
                        application_owner.root.source().bytes.as_ptr(),
                        held_application_root,
                    );
                    assert_eq!(application_owner.root.source().bytes, project.as_bytes());
                }
            }
        }

        #[test]
        fn root_decoder_failures_keep_native_facts_across_public_views_without_rereads() {
            use yamaa_adapters::{
                file_project, project_application, project_source::Failure,
                specification_source::Error as SourceError, yaml_decode::DecodeFailure,
            };
            use yamaa_core::limits::Limits;

            for limited in [false, true] {
                let study = Study::new();
                let raw = if limited {
                    format!(
                        "{}0{}",
                        "[".repeat(Limits::YAML_DEPTH + 1),
                        "]".repeat(Limits::YAML_DEPTH + 1),
                    )
                } else {
                    "yamaa: \"1.0\"\ntype: project\ncodelists: [\n".to_owned()
                };
                study.write("project.yaml", &raw);
                let entry = study.path("root.yaml");
                let project = study.path("project.yaml");
                let selection = file_workflow::select(&entry, &project, Language::Python)
                    .err()
                    .unwrap();
                let file_workflow::SelectionError::Preparation(Error::Environment(owner)) =
                    &selection
                else {
                    panic!("root decoder failure precedes entry reads")
                };
                let file_project::Error::Environment(Failure::Root(SourceError::Decode {
                    identity,
                    error,
                })) = owner.error()
                else {
                    panic!("native decoder cause remains owned")
                };
                assert_eq!(identity, &project);
                let expected = match error {
                    DecodeFailure::Limit(bound) if limited => {
                        assert_eq!(
                            bound,
                            &Limits::yaml_depth(Limits::YAML_DEPTH + 1).unwrap_err()
                        );
                        yamaa_adapters::json!({"phase": "engine", "condition": "engine_rejected", "rule": null, "spec_paths": ["$"], "context": {
                            "step":"environment", "code":"resource_limit",
                            "resource":"yaml_depth", "limit":Limits::YAML_DEPTH,
                            "actual":Limits::YAML_DEPTH + 1,
                        }})
                    }
                    DecodeFailure::InvalidYaml { position, reason } if !limited => {
                        yamaa_adapters::json!({"phase": "validation", "condition": "invalid_yaml", "rule": "SCHEMA-3", "spec_paths": ["$"], "context": {
                            "line":position.line, "column":position.column, "reason":reason,
                            "path":project, "entry":project, "environment_path":"$",
                        }})
                    }
                    cause => panic!("expected retained decoder cause: {cause:?}"),
                };
                assert_eq!(owner.capture_reads(), 1);
                let (_, bytes) = owner.captured_sources().next().unwrap();
                assert_eq!(bytes, raw.as_bytes());
                let held_bytes = bytes.as_ptr();
                let application_error =
                    project_application::prepare(&entry, &project, Language::Python)
                        .err()
                        .unwrap();
                assert_eq!(
                    crate::support::rows(&project_application::check(
                        &entry,
                        &project,
                        Language::Python
                    )),
                    std::slice::from_ref(&expected)
                );
                assert!(!study.0.join("root.yaml").exists());
                fs::remove_dir_all(&study.0).unwrap();
                for _ in 0..2 {
                    assert_eq!(
                        crate::support::rows(&project_application::selection_failure(&selection)),
                        std::slice::from_ref(&expected)
                    );
                    assert_eq!(
                        crate::support::rows(&project_application::failure(&application_error)),
                        std::slice::from_ref(&expected)
                    );
                    assert_eq!(owner.capture_reads(), 1);
                    let (_, bytes) = owner.captured_sources().next().unwrap();
                    assert_eq!(bytes.as_ptr(), held_bytes);
                    assert_eq!(bytes, raw.as_bytes());
                }
            }
        }

        #[test]
        fn linked_diamond_checks_only_the_entry_and_reads_each_producer_once_without_data() {
            let study = Study::new();
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", true),
            );
            study.write(
                "left.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
                    "P",
                    "left.csv",
                    true,
                ),
            );
            study.write(
                "right.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml}}",
                    "P",
                    "right.csv",
                    false,
                ),
            );
            // VALUE reads all three linked datasets so pruning (LAYER-11) keeps
            // every link in the resolved model.
            study.write("root.yaml", &yaml("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml, key: [ID]}, Q: {path: right.csv, schema: './right.yaml', key: [ID]}}", "L", "root.csv", true).edit(
                "derivation: L.VALUE",
                "derivation: {first_available: {sources: [L.VALUE, R.VALUE, Q.VALUE]}}",
            ));
            let project = study.prepare();
            // The project, the entry and its two distinct producers. The
            // producers are neither compiled nor followed to leaf.yaml.
            assert_eq!(project.capture_reads(), 4);
            let expected = vec![yamaa_adapters::json!({
                "phase": "validation", "condition": "invalid_regex", "rule": "TEXT-6",
                "spec_paths": ["columns.VALUE.verifications[0].matches.pattern"],
                "context": {"pattern": "é("},
            })];
            assert_eq!(
                crate::support::rows(&specification_check::issues(project.run())),
                expected
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                crate::support::rows(&specification_check::issues(project.run())),
                expected
            );
            assert_eq!(
                project.run().captured_environment().root().source().bytes,
                b"yamaa: \"1.0\"\ntype: project\n"
            );
        }

        #[test]
        fn ordinary_check_rows_remain_identical_and_public_environment_paths_work() {
            let study = Study::new();
            study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "never-written.csv", true),
            );
            let ordinary = yamaa_adapters::project_application::prepare(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            )
            .unwrap();
            let expected = yamaa_adapters::specification_check::issues(ordinary.run());
            assert_eq!(expected.len(), 1);
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("project.yaml"),
                    Language::Python
                ),
                expected
            );
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("never-written.csv").exists());
        }

        #[test]
        fn linked_type_rejection_keeps_complete_authored_rows_and_original_owners() {
            let study = Study::new();
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            // The restated type is refused (`workflow.json`), and the rows
            // outlive the study.
            let error = study.compile_rejection();
            let rows = specification_check::rejected(error.run()).unwrap();
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(specification_check::rejected(error.run()).unwrap(), rows);
            assert_eq!(error.capture_reads(), 3);
            assert_eq!(error.retained_sources().len(), 3);
        }

        #[test]
        fn a_linked_artifact_path_may_differ_from_the_producer_output_path() {
            // A link supplies types only; the consumer reads its own path.
            let study = Study::new();
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: copy/stored.csv, schema: leaf.yaml}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            let project = study.prepare();
            assert!(specification_check::issues(project.run()).is_empty());
            assert_eq!(
                project.run().compiled().sources()[0].path,
                "copy/stored.csv"
            );
            assert_eq!(project.capture_reads(), 3);
        }

        #[test]
        fn inherited_producer_types_rejection_names_the_declaring_parent_on_check_and_domain() {
            let study = Study::new();
            study.write(
                "parent.yaml",
                "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: {path: input/dm.csv, types: {RANDDT: date}}\noutput:\n  path: adsl.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: DM.RANDDT\n",
            );
            study.write(
                "producer/dm.yaml",
                "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput:\n  RAW: {path: ../raw/dm_raw.csv, types: {RANDDT: date}}\noutput:\n  path: ../input/dm.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: RAW.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: RAW.RANDDT\n",
            );
            study.write(
                "root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\ninput:\n  DM: {schema: producer/dm.yaml}\n",
            );
            // The parent declares the restated type (`workflow.json`), and a
            // check and a build report the same rows.
            let error = study.compile_rejection();
            let rows = specification_check::rejected(error.run()).unwrap();
            assert_eq!(
                error
                    .run()
                    .document
                    .declaring_source(&rows[0].spec_paths[0]),
                Some(study.path("parent.yaml").as_str())
            );
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("project.yaml"),
                    Language::Python
                ),
                rows
            );
            let entry = study.path("root.yaml");
            let identity = || yamaa_adapters::specification_report::Identity {
                runtime: "test",
                runtime_version: "1",
                engine_version: "0.1.0",
                example: "test",
                specification: &entry,
                base_directory: ".",
            };
            assert_eq!(
                yamaa_adapters::file_application::check(
                    yamaa_core::project_function::Language::Python,
                    yamaa_core::domain_entry::Request {
                        specification: &entry,
                        environment: None,
                    },
                    identity(),
                ),
                rows
            );
            assert_eq!(
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    yamaa_core::domain_entry::Request {
                        specification: &entry,
                        environment: None,
                    },
                    identity(),
                )
                .err()
                .unwrap(),
                rows
            );
        }

        #[test]
        fn producer_compilation_and_back_links_are_not_consumer_failures_but_self_links_are() {
            for back_link in [false, true] {
                let study = Study::new();
                study.write(
                    "root.yaml",
                    &yaml(
                        "{P: {path: leaf.csv, schema: leaf.yaml}}",
                        "P",
                        "root.csv",
                        false,
                    ),
                );
                let leaf = if back_link {
                    yaml(
                        "{R: {path: root.csv, schema: './root.yaml'}}",
                        "R",
                        "leaf.csv",
                        false,
                    )
                } else {
                    yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .edit("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}")
                };
                study.write("leaf.yaml", &leaf);
                // The producer is never compiled and its links are never followed.
                let project = study.prepare();
                assert!(specification_check::issues(project.run()).is_empty());
                assert_eq!(project.capture_reads(), 3);
            }
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{R: {path: root.csv, schema: './root.yaml'}}",
                    "R",
                    "root.csv",
                    false,
                ),
            );
            // A self link is refused (`workflow.json`) after the project and
            // the entry are read.
            let error = study.compile_rejection();
            assert_eq!(error.capture_reads(), 2);
        }

        #[test]
        fn raw_environment_rejection_precedes_the_first_entry_read() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let raw = "yamaa: \"1.0\"\ntype: project\ncodelists: [\n";
            study.write("project.yaml", raw);
            let error = study.rejected();
            let Error::Environment(environment) = &error else {
                panic!("early environment");
            };
            assert_eq!(environment.capture_reads(), 1);
            assert_eq!(
                environment.captured_sources().next().unwrap().1,
                raw.as_bytes()
            );
            // Its issue names the path as written (`workflow.json`).
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                environment.captured_sources().next().unwrap().1,
                raw.as_bytes()
            );
        }

        #[test]
        fn supported_environment_and_original_inherited_schema_causes_are_complete() {
            let study = Study::new();
            // The environments key states the language; an renv lock under the
            // python key is the finding that language still decides.
            study.write(
                "project.yaml",
                "yamaa: '1.0'\ntype: project\nenvironments: {python: {lock: renv.lock}}\n",
            );
            study.write("renv.lock", r#"{"R":{"Version":"4.6.1"},"Packages":{}}"#);
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            // The project file holds the lock at `environments.python.lock`,
            // where its finding is reported (FUNC-3, `workflow.json`).
            let error = study.rejected();
            let Error::Environment(environment) = &error else {
                panic!("environment");
            };
            assert_eq!(environment.capture_reads(), 2); // project and lock
            study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
            study.write(
                "parent/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
                    .edit("type: str", "type: invalid"),
            );
            study.write(
                "root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: parent/base.yaml\ndomain: CHILD\n",
            );
            // The parent's schema finding names the parent (`workflow.json`),
            // and the findings outlive the study.
            let error = study.rejected();
            let rows = preparation_issues(&error).unwrap();
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(preparation_issues(&error).unwrap(), rows);
        }

        #[test]
        fn inherited_links_retain_the_declaring_layer_and_one_producer_document() {
            let study = Study::new();
            study.write(
                "producer/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "../leaf.csv", true),
            );
            study.write(
                "leaf.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: producer/base.yaml\ndomain: LEAF\n",
            );
            let parent = yaml("{P: {path: ../leaf.csv, schema: ../leaf.yaml}, Q: {path: ../leaf.csv, schema: '../leaf.yaml'}}", "P", "../root.csv", false)
        .edit("columns:\n", "intermediates: [{id: Q_REFERENCE, dataset: Q, key: [ID], require_match: true}]\ncolumns:\n")
        .edit("output:", "  - {name: OTHER, type: str, label: Other, derivation: Q_REFERENCE.VALUE}\noutput:")
        .edit("columns: [ID, VALUE]", "columns: [ID, VALUE, OTHER]");
            study.write("parents/base.yaml", &parent);
            study.write(
                "root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: parents/base.yaml\ndomain: ROOT\n",
            );
            let project = study.prepare();
            // The project, the entry, its parent and the one linked producer
            // with its own parent. The producer's invalid regex is never compiled.
            assert_eq!(project.capture_reads(), 5);
            assert!(specification_check::issues(project.run()).is_empty());
            let document = project.run().document();
            for dataset in ["P", "Q"] {
                assert_eq!(
                    document
                        .written_input_origin(dataset, "schema")
                        .unwrap()
                        .declaring_source,
                    study.path("parents/base.yaml")
                );
            }
            let compiled = project.run().compiled();
            assert_eq!(compiled.source_contract(0), compiled.source_contract(1));
            assert!(compiled.source_contract(0).is_some());
        }

        #[test]
        fn missing_linked_producer_keeps_exact_authored_finding_and_declaring_request() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: absent.csv, schema: missing.yaml}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            let error = study.rejected();
            let Error::Entry(entry) = &error else {
                panic!("entry rejection");
            };
            assert!(matches!(
                entry.error(),
                yamaa_adapters::file_entry::Error::Document(
                    yamaa_adapters::file_preparation::Error::ResourceFinding {
                        error: yamaa_adapters::file_resources::Error::Missing,
                        ..
                    }
                )
            ));
            assert_eq!(entry.capture_reads(), 2);
            assert_eq!(entry.origin().unwrap().dataset, "P");
            // A check reports the missing schema at its field
            // (`workflow.json`); the entry stays held.
            assert!(
                matches!(preparation_issues(&error), Err(IssueError::Original(original)) if std::ptr::eq(original, &error))
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert!(entry.document().is_some());
            assert_eq!(entry.captured_sources().count(), 2);
        }

        #[test]
        fn invalid_producer_contract_and_environment_child_failures_keep_original_metadata() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            let original = yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                .edit("    label: Reported value\n", "");
            study.write("leaf.yaml", &original);
            // A producer column without a label is refused at the link
            // (`workflow.json`).
            let error = study.compile_rejection();
            let rows = specification_check::rejected(error.run()).unwrap();
            // The project's lock and function file resolve beside it (D8).
            study.write(
                "project.yaml",
                "yamaa: \"1.0\"\ntype: project\nfunctions: [python/broken.yaml]\nenvironments: {python: {lock: python/uv.lock}}\n",
            );
            study.write("python/uv.lock", "version = 1\npackage = []\n");
            let malformed = "function: [\n";
            study.write("python/broken.yaml", malformed);
            let failure = study.rejected();
            let Error::Environment(environment) = &failure else {
                panic!("original environment");
            };
            // project, lock and the broken function file
            assert_eq!(environment.capture_reads(), 3);
            assert_eq!(
                environment
                    .captured_sources()
                    .find(|(path, _)| path == &study.path("python/broken.yaml"))
                    .unwrap()
                    .1,
                malformed.as_bytes()
            );
            // The broken function file names its listing and the project
            // (`workflow.json`).
            fs::remove_dir_all(&study.0).unwrap();
            assert!(error
                .retained_sources()
                .iter()
                .any(|(_, bytes)| bytes.as_ref() == original.as_bytes()));
            assert_eq!(specification_check::rejected(error.run()).unwrap(), rows);
            assert_eq!(
                environment
                    .captured_sources()
                    .find(|(path, _)| path == &study.path("python/broken.yaml"))
                    .unwrap()
                    .1,
                malformed.as_bytes()
            );
        }

        #[cfg(unix)]
        #[test]
        fn shared_native_workflow_keeps_literal_backslash_in_canonical_roots() {
            let outer = Study::new();
            let root = outer.0.join("named\\root");
            fs::create_dir(&root).unwrap();
            let study = Study(crate::support::Scratch::adopt(root));
            study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            let project = study.prepare();
            assert!(specification_check::issues(project.run()).is_empty());
            let artifact = project.resolved_specification().unwrap();
            let schema = artifact.entries()[0]
                .resources
                .iter()
                .find(|resource| resource.spec_path == "input.P.schema")
                .unwrap();
            assert_eq!(
                schema.location,
                yamaa_adapters::resolved_artifact::model::Location::Local {
                    path: study.path("leaf.yaml")
                }
            );
            assert_eq!(project.capture_reads(), 3);
        }

        #[test]
        fn ordinary_original_input_facts_survive_files_without_study_authority() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "never-written.csv", false),
            );
            let project = study.prepare();
            assert_eq!(project.capture_reads(), 2);
            let document = project.run().document();
            assert!(document.written_input_origin("RAW", "schema").is_none());
            assert!(project.run().compiled().source_contract(0).is_none());
            let original = fs::read(study.0.join("root.yaml")).unwrap();
            drop(study);
            assert_eq!(document.source().bytes, original);
            assert!(specification_check::issues(project.run()).is_empty());
        }

        #[test]
        fn inherited_original_producer_facts_use_the_contributing_input_layer() {
            let study = Study::new();
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "base.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml}}",
                    "P",
                    "consumer.csv",
                    false,
                ),
            );
            study.write(
                "root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ndomain: TEST\n",
            );
            let project = study.prepare();
            assert_eq!(project.capture_reads(), 4);
            let document = project.run().document();
            assert!(document
                .raw()
                .document
                .field(document.raw().document.root(), "input")
                .is_none());
            assert_eq!(
                document
                    .written_input_origin("P", "schema")
                    .unwrap()
                    .declaring_source,
                study.path("base.yaml")
            );
            assert!(project.run().compiled().source_contract(0).is_some());
            drop(study);
            assert!(specification_check::issues(project.run()).is_empty());
        }

        #[test]
        fn rejected_mistyped_schema_keeps_the_original_shipped_schema_cause_and_raw_entry() {
            let study = Study::new();
            let original = yaml(
                "{RAW: {path: never-read.csv, schema: 7}}",
                "RAW",
                "never-written.csv",
                false,
            );
            study.write("root.yaml", &original);
            let rejection = study.rejected();
            let Error::Entry(entry) = &rejection else {
                panic!("the original root rejection must remain owned");
            };
            assert_eq!(entry.capture_reads(), 2);
            assert!(entry.document().is_none());
            // The shipped schema's finding is `workflow.json`'s; it outlives the
            // study, and projecting it again reads nothing more.
            let findings = preparation_issues(&rejection).unwrap();
            drop(study);
            let retained = entry.retained_sources();
            assert_eq!(retained.len(), 2);
            assert!(retained
                .iter()
                .any(|(name, bytes)| name.ends_with("/root.yaml")
                    && bytes.as_ref() == original.as_bytes()));
            assert_eq!(preparation_issues(&rejection).unwrap(), findings);
            assert_eq!(entry.capture_reads(), 2);
        }

        impl Study {
            fn select(&self) -> Result<file_workflow::Selected, file_workflow::SelectionError> {
                file_workflow::select_with_resources(
                    Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
                    "root.yaml",
                    "project.yaml",
                    Language::Python,
                    yamaa_adapters::document::Closures::shipped().unwrap(),
                )
            }
            fn selection_rejection(&self) -> file_workflow::SelectionError {
                match self.select() {
                    Err(error) => error,
                    Ok(_) => panic!("expected selected preparation rejection"),
                }
            }
        }
        fn ordinary_report_id() -> yamaa_adapters::specification_report::Identity<'static> {
            yamaa_adapters::specification_report::Identity {
                runtime: "test",
                runtime_version: "1",
                engine_version: "0.1.0",
                example: "selected-workflow",
                specification: "root.yaml",
                base_directory: ".",
            }
        }

        #[test]
        fn selected_ordinary_keeps_complete_report_and_explicit_held_byte_save() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let selected = study.select().unwrap();
            assert_eq!(selected.capture_reads(), 2);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("root.csv").exists());
            // An independent study source is first created and read after preparation.
            study.write("never-read.csv", "ID,VALUE\n1,alpha\n2,beta\n");
            let ordinary = yamaa_adapters::file_project::prepare(
                Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                "root.yaml",
                "project.yaml",
                Language::Python,
                yamaa_adapters::document::Closures::shipped().unwrap(),
            )
            .unwrap();
            let mut selected_scope = selected.run_scope(None);
            let selected_attempt =
                selected.build(&mut selected_scope, &mut NoFunctions, Scope::default());
            let ordinary_attempt = ordinary.build(
                &mut ordinary.run_scope(None),
                &mut NoFunctions,
                Scope::default(),
            );
            assert_eq!(selected_scope.capture_reads(), 3);
            let selected_result = yamaa_adapters::specification_report::build_result(
                selected.run(),
                &selected_attempt,
                ordinary_report_id(),
                &[],
            )
            .unwrap();
            let ordinary_result = yamaa_adapters::specification_report::build_result(
                ordinary.run(),
                &ordinary_attempt,
                ordinary_report_id(),
                &[],
            )
            .unwrap();
            assert_eq!(selected_result.output(), ordinary_result.output());
            assert_eq!(
                selected_attempt
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .dataset
                    .rows(),
                &[
                    vec![
                        yamaa_core::value::Value::Int(1),
                        yamaa_core::value::Value::Str("alpha".into())
                    ],
                    vec![
                        yamaa_core::value::Value::Int(2),
                        yamaa_core::value::Value::Str("beta".into())
                    ]
                ]
            );
            assert_eq!(
                selected_result.observations(),
                ordinary_result.observations()
            );
            assert_eq!(selected_result.issues(), ordinary_result.issues());
            assert!(!study.0.join("root.csv").exists());
            let retained = selected.run();
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                retained.document().source().bytes,
                yaml("{RAW: never-read.csv}", "RAW", "root.csv", false).as_bytes()
            );
            assert_eq!(
                selected_result.saved_observations().unwrap()["artifacts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                selected_result.files(),
                [("root.csv", b"ID,VALUE\n1,alpha\n2,beta\n".as_slice())]
            );
        }

        #[test]
        fn selected_inherited_links_read_each_producer_once_with_shared_aliases() {
            let study = Study::new();
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "left.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml}}",
                    "P",
                    "left.csv",
                    false,
                ),
            );
            study.write(
                "right.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
                    "P",
                    "right.csv",
                    false,
                ),
            );
            let parent = yaml("{L: {path: ../left.csv, schema: ../left.yaml}, R: {path: ../right.csv, schema: ../right.yaml}, Q: {path: ../right.csv, schema: '../right.yaml'}}", "L", "../root.csv", true)
        .edit("columns:\n", "intermediates: [{id: RIGHT, dataset: R, key: [ID], require_match: true}, {id: ALIAS, dataset: Q, key: [ID], require_match: true}]\ncolumns:\n")
        .edit("output:", "  - {name: RIGHT_VALUE, type: str, label: Right value, derivation: RIGHT.VALUE}\n  - {name: ALIAS_VALUE, type: str, label: Alias value, derivation: ALIAS.VALUE}\noutput:")
        .edit("columns: [ID, VALUE]", "columns: [ID, VALUE, RIGHT_VALUE, ALIAS_VALUE]");
            study.write("parents/base.yaml", &parent);
            let entry = "yamaa: \"1.0\"\ntype: spec\nparent: parents/base.yaml\ndomain: ROOT\n";
            study.write("root.yaml", entry);
            let project = study.select().unwrap();
            // Project, entry, parent and the two distinct linked producers;
            // leaf.yaml is linked only by producers and is never read.
            assert_eq!(project.capture_reads(), 5);
            let document = project.run().document();
            assert_eq!(document.source().bytes, entry.as_bytes());
            assert!(document
                .raw()
                .document
                .field(document.raw().document.root(), "input")
                .is_none());
            let rows = specification_check::issues(project.run());
            assert_eq!(rows.len(), 1);
            assert_eq!(
                document.declaring_source(&rows[0].spec_paths[0]),
                Some(study.path("parents/base.yaml").as_str())
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(specification_check::issues(project.run()), rows);
            assert_eq!(project.capture_reads(), 5);
        }

        #[test]
        fn selected_ordinary_compile_failure_keeps_original_run_environment_and_raw_resources() {
            use file_workflow::SelectionError;
            let study = Study::new();
            let raw = yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
                .edit("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}");
            study.write("root.yaml", &raw);
            let SelectionError::Compile(rejected) = study.selection_rejection() else {
                panic!("original ordinary compiler rejection");
            };
            assert_eq!(rejected.capture_reads(), 2);
            assert_eq!(rejected.entry(), "root.yaml");
            // The default project lists no function file, so the call names an
            // unknown function (FUNC-4, #3030 D8, `workflow.json`).
            let rows = yamaa_adapters::specification_check::rejected(rejected.run()).unwrap();
            fs::remove_dir_all(&study.0).unwrap();
            let held = rejected.retained_sources();
            assert_eq!(held.len(), 2);
            assert!(held
                .iter()
                .any(|(identity, bytes)| identity.ends_with("root.yaml")
                    && bytes.as_ref() == raw.as_bytes()));
            assert_eq!(
                yamaa_adapters::specification_check::rejected(rejected.run()).unwrap(),
                rows
            );
            assert_eq!(
                rejected.run().captured.root().source().bytes,
                b"yamaa: \"1.0\"\ntype: project\n"
            );
        }

        #[test]
        fn selected_invalid_link_declarations_keep_their_typed_owners() {
            use file_workflow::SelectionError;
            // The entry's columns read the declared dataset (base), so the link
            // survives pruning (LAYER-11).
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{RAW: {path: never-read.csv, schema: 7}}",
                    "RAW",
                    "root.csv",
                    false,
                ),
            );
            // The shipped schema refuses the link (`workflow.json`), and so
            // does the restated type below.
            let SelectionError::Preparation(error @ Error::Entry(_)) = study.selection_rejection()
            else {
                panic!("owned original metadata rejection");
            };
            let rows = preparation_issues(&error).unwrap();
            let Error::Entry(owner) = &error else {
                unreachable!()
            };
            assert_eq!(owner.capture_reads(), 2);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(preparation_issues(&error).unwrap(), rows);
            assert_eq!(owner.capture_reads(), 2);

            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            let SelectionError::Compile(rejected) = study.selection_rejection() else {
                panic!("owned compilation rejection");
            };
            let rows = specification_check::rejected(rejected.run()).unwrap();
            assert_eq!(rejected.capture_reads(), 3);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(specification_check::rejected(rejected.run()).unwrap(), rows);
        }

        #[test]
        fn selected_raw_environment_and_root_failures_keep_exact_capture_prefixes() {
            use file_workflow::SelectionError;
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            study.write(
                "project.yaml",
                "yamaa: \"1.0\"\ntype: project\ncodelists: [\n",
            );
            let SelectionError::Preparation(Error::Environment(environment)) =
                study.selection_rejection()
            else {
                panic!("raw project rejection");
            };
            assert_eq!(environment.capture_reads(), 1);
            study.write("project.yaml", "yamaa: \"1.0\"\ntype: project\n");
            study.write("root.yaml", "[");
            // The raw root is refused as `workflow.json` records; its findings
            // outlive the study.
            let SelectionError::Preparation(root @ Error::Entry(_)) = study.selection_rejection()
            else {
                panic!("raw root rejection");
            };
            let rows = preparation_issues(&root).unwrap();
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(environment.retained_sources().len(), 1);
            assert_eq!(preparation_issues(&root).unwrap(), rows);
        }

        #[test]
        fn selected_missing_labels_refuse_while_producer_links_and_compiler_failures_do_not() {
            use file_workflow::SelectionError;
            for mode in 0..3 {
                let study = Study::new();
                study.write(
                    "root.yaml",
                    &yaml(
                        "{P: {path: leaf.csv, schema: leaf.yaml}}",
                        "P",
                        "root.csv",
                        false,
                    ),
                );
                let leaf = match mode {
                    0 => yaml(
                        "{P: {path: root.csv, schema: root.yaml}}",
                        "P",
                        "leaf.csv",
                        false,
                    ),
                    1 => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .edit("label: Identifier, ", "")
                        .edit("    label: Reported value\n", ""),
                    _ => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .edit("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}"),
                };
                study.write("leaf.yaml", &leaf);
                if mode != 1 {
                    // A link back to the consumer and a producer compiler failure
                    // are producer facts the consumer never compiles or follows.
                    let project = study.prepare();
                    assert!(specification_check::issues(project.run()).is_empty());
                    assert_eq!(project.capture_reads(), 3);
                    continue;
                }
                let SelectionError::Compile(rejected) = study.selection_rejection() else {
                    panic!("whole contract rejection");
                };
                let rows = specification_check::rejected(rejected.run()).unwrap();
                assert_eq!(
                    rows.iter()
                        .map(|r| (r.condition(), r.spec_paths[0].as_str()))
                        .collect::<Vec<_>>(),
                    [
                        (
                            "invalid_producer_contract",
                            "input.P.schema.columns.ID.label"
                        ),
                        (
                            "invalid_producer_contract",
                            "input.P.schema.columns.VALUE.label"
                        ),
                    ]
                );
                assert_eq!(rejected.capture_reads(), 3);
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(specification_check::rejected(rejected.run()).unwrap(), rows);
            }
        }

        #[test]
        fn selected_entry_paths_keep_absolute_environment_policy_and_early_owned_refusal() {
            use file_workflow::SelectionError;
            let study = Study::new();
            study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let project = file_workflow::select(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            )
            .unwrap();
            assert_eq!(project.capture_reads(), 2); // project + root; no configuration
                                                    // An empty project path is refused lexically, as the caller's own
                                                    // path, before anything is captured.
            let refused = file_workflow::select(&study.path("root.yaml"), "", Language::Python)
                .err()
                .unwrap();
            let SelectionError::Preparation(Error::Caller { path, error }) = &refused else {
                panic!("original lexical environment refusal");
            };
            assert_eq!(path, "");
            assert_eq!(*error, yamaa_adapters::file_resources::Error::InvalidPath);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(refused.retained_sources().len(), 0);
            assert_eq!(
                project.run().captured_environment().root().source().bytes,
                b"yamaa: \"1.0\"\ntype: project\n"
            );
        }

        #[test]
        fn absent_environment_selects_plain_route_without_capturing_or_inventing_metadata() {
            let study = Study::new();
            // Nearby project and environment files are deliberately unreadable as
            // YAML. Absence must neither discover them nor convert either into an
            // empty captured document.
            study.write("project.yaml", "[");
            study.write("environment.yaml", "[");
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
            );
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml}}",
                    "P",
                    "root.csv",
                    false,
                ),
            );
            let selected = file_workflow::select_request(
                yamaa_core::build::Request {
                    specification: &study.path("root.yaml"),
                    environment: None,
                },
                Some(Language::Python),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "original link admission: {:?}",
                    yamaa_adapters::project_application::selection_failure(&error)
                )
            });
            let prepared = selected;
            assert_eq!(prepared.capture_reads(), 2);
            assert!(prepared.run().compiled().source_contract(0).is_some());
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("leaf.csv").exists());
            assert!(!study.0.join("root.csv").exists());
            fs::remove_dir_all(&study.0).unwrap();
            assert!(yamaa_adapters::specification_check::issues(prepared.run()).is_empty());
        }

        #[test]
        fn plain_workflow_profile_inspection_keeps_physical_kind_before_study_capture() {
            use yamaa_core::diagnostic::ConditionCode as C;
            use yamaa_core::json::Value as JsonValue;
            use yamaa_core::specification::PrepareError;

            let study = Study::new();
            study.write("project.yaml", "[");
            study.write("environment.yaml", "[");
            study.write("parent/base.yaml", "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
            study.write(
                "entry/root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: ../parent/base.yaml\n",
            );
            fs::create_dir_all(study.0.join("parent/input")).unwrap();
            for (kind, expected) in [
                (0, C::ResourceNotRegularFile),
                (1, C::SourceUnknownProfile),
                (2, C::SourceUnknownProfile),
                (3, C::ResourceSymlink),
            ] {
                let input = study.0.join("parent/input/unknown");
                match kind {
                    0 => fs::create_dir(&input).unwrap(),
                    1 => fs::write(&input, b"ID\n1\n").unwrap(),
                    2 => (),
                    _ => {
                        fs::write(study.0.join("parent/input/target"), b"ID\n1\n").unwrap();
                        #[cfg(unix)]
                        std::os::unix::fs::symlink("target", &input).unwrap();
                        #[cfg(windows)]
                        std::os::windows::fs::symlink_file("target", &input).unwrap();
                    }
                }
                let Err(file_workflow::SelectionError::Compile(owner)) =
                    file_workflow::select_request(
                        yamaa_core::build::Request {
                            specification: &study.path("entry/root.yaml"),
                            environment: None,
                        },
                        Some(Language::Python),
                    )
                else {
                    panic!("plain route must retain its preparation rejection");
                };
                assert_eq!(
                    owner.capture_reads(),
                    2,
                    "only root and parent metadata were captured"
                );
                let PrepareError::Invalid(findings) = &owner.run().rejection.error else {
                    panic!("original compiler finding remains owned");
                };
                let issue = findings[0].diagnostic();
                assert_eq!(issue.code, expected);
                assert_eq!(issue.spec_paths, ["input.SRC.path"]);
                if matches!(kind, 0 | 3) {
                    assert_eq!(
                        issue.context["path"],
                        JsonValue::String("input/unknown".into())
                    );
                    assert_eq!(issue.context["dataset"], JsonValue::String("SRC".into()));
                }
                let sources = owner.retained_sources();
                assert_eq!(sources.len(), 2);
                assert!(sources
                    .iter()
                    .all(|(identity, _)| !identity.ends_with("project.yaml")
                        && !identity.ends_with("environment.yaml")
                        && !identity.ends_with("input/unknown")));
                match kind {
                    0 => fs::remove_dir(&input).unwrap(),
                    2 => (),
                    _ => fs::remove_file(&input).unwrap(),
                }
            }
        }
    }
}
