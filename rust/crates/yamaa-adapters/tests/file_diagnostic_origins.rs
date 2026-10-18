#![cfg(feature = "yaml")]
//! File contracts.
#[allow(dead_code)]
mod support;
mod file {
    mod diagnostic_origins {
        //! Native index paths resolve only through the exact held composed member IDs.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_resources::Resources, file_workflow, shipped_schema};
        use yamaa_core::build::Prepared;
        use yamaa_core::project_function::Language;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let directory = std::env::temp_dir().join(format!(
                    "yamaa-diagnostic-origins-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(directory.join("parent")).unwrap();
                // A minimal project linking a Python environment file.
                fs::write(
                    directory.join("project.yaml"),
                    "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                )
                .unwrap();
                fs::write(
                    directory.join("environment.yaml"),
                    "schema_version: '1.0'\n",
                )
                .unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(directory).unwrap())
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

        #[test]
        fn physical_row_indices_preserve_literal_parent_ids_and_refuse_invalid_slots() {
            for id in ["ordinary", "dot.id", "bracket[999]", "both.[999]"] {
                let study = Study::new();
                fs::write(study.0.join("parent/base.yaml"), format!(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: out.csv, columns: [ID]}}\ncolumns: [{{name: ID, type: str}}]\nrows:\n  - {{id: clean, dataset: SRC, derivations: {{ID: SRC.ID}}}}\n  - {{id: '{id}', dataset: SRC, filter: 'ID >', derivations: {{ID: SRC.ID}}}}\n")).unwrap();
                fs::write(
                    study.0.join("spec.yaml"),
                    "schema_version: '1.0'\nparent: parent/base.yaml\n",
                )
                .unwrap();
                let owner = file_workflow::select_with_optional_resources(
                    Resources::for_entry(study.0.to_str().unwrap()).unwrap(),
                    "spec.yaml",
                    None,
                    Language::Python,
                    shipped_schema::capture().unwrap(),
                    shipped_schema::capture_project().unwrap(),
                )
                .unwrap();
                let Prepared::Plain(prepared) = &owner else {
                    panic!("plain owner");
                };
                let document = prepared.run().document();
                let parent = study.identity("parent/base.yaml");
                assert_eq!(
                    document.declaring_source("rows[1].filter"),
                    Some(parent.as_str())
                );
                assert_eq!(
                    document.declaring_source(&format!("rows.{id}.filter")),
                    Some(parent.as_str())
                );
                for invalid in [
                    "rows[2].filter",
                    "rows[-1].filter",
                    "rows[01].filter",
                    "rows[+1].filter",
                    "rows[1]x",
                ] {
                    assert_eq!(document.declaring_source(invalid), None, "{invalid}");
                }
                assert_eq!(prepared.capture_reads(), 2);
                let findings = prepared.run().check_diagnostics();
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].spec_paths, ["rows[1].filter"]);
                assert_eq!(
                    document.declaring_source(&findings[0].spec_paths[0]),
                    Some(parent.as_str())
                );
                let issues =
                    yamaa_adapters::specification_check::issue_rows(prepared.run()).unwrap();
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].phase, "validation");
                assert_eq!(issues[0].condition, "invalid_expression");
                assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0188"));
                assert_eq!(issues[0].spec_paths, ["rows[1].filter"]);
                assert_eq!(
                    issues[0].context.clone(),
                    yamaa_adapters::json!({"predicate": "ID >", "position": 4})
                );
                assert_eq!(
                    yamaa_adapters::project_application::check(
                        &study.identity("spec.yaml"),
                        &study.identity("project.yaml"),
                        Language::Python,
                    ),
                    issues
                );
                assert_eq!(prepared.capture_reads(), 2);
                assert!(!study.0.join("parent/input.csv").exists());
                assert!(!study.0.join("parent/out.csv").exists());
            }
        }

        #[test]
        fn ready_static_owner_returns_contributor_without_changing_public_issue_context() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"),
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - name: ID\n    type: str\n    label: Identifier\n    derivation: SRC.ID\n    verifications: [{matches: {pattern: '('}}]\n").unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                "schema_version: '1.0'\nparent: parent/base.yaml\n",
            )
            .unwrap();
            let owner = file_workflow::select_with_optional_resources(
                Resources::for_entry(study.0.to_str().unwrap()).unwrap(),
                "spec.yaml",
                None,
                Language::Python,
                shipped_schema::capture().unwrap(),
                shipped_schema::capture_project().unwrap(),
            )
            .unwrap();
            let Prepared::Plain(prepared) = &owner else {
                panic!("plain owner");
            };
            let before = prepared.capture_reads();
            let parent = study.identity("parent/base.yaml");
            let findings = prepared.run().check_diagnostics();
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].spec_paths,
                ["columns.ID.verifications[0].matches.pattern"]
            );
            assert_eq!(
                prepared
                    .run()
                    .document()
                    .declaring_source(&findings[0].spec_paths[0]),
                Some(parent.as_str())
            );
            let issues = yamaa_adapters::specification_check::issue_rows(prepared.run()).unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"pattern\":\"(\"}"
            );
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.identity("spec.yaml"),
                    &study.identity("project.yaml"),
                    Language::Python,
                ),
                issues
            );
            assert_eq!(prepared.capture_reads(), before);
            assert!(!study.0.join("parent/input.csv").exists());
            assert!(!study.0.join("parent/out.csv").exists());
        }
    }
    mod file_application {
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        #[cfg(unix)]
        use yamaa_adapters::file_resources::Error;
        use yamaa_adapters::{
            file_application, file_configuration, file_preparation::FileSpecification,
            specification_report::Identity,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::domain_entry::Request;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-file-application-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn text(&self, path: &str) -> String {
                let path = self.0.join(path).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            /// Write a minimal project that links a functionless Python
            /// environment file, and return the project path a caller passes.
            fn project(&self) -> String {
                fs::write(
                    self.0.join("project.yaml"),
                    "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                )
                .unwrap();
                fs::write(self.0.join("environment.yaml"), "schema_version: '1.0'\n").unwrap();
                self.text("project.yaml")
            }
            fn build(&self) -> file_application::Domain {
                let path = self.text("spec.yaml");
                file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .unwrap()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn identity(specification: &str) -> Identity<'_> {
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification,
                base_directory: ".",
            }
        }
        #[test]
        fn changed_source_retains_original_snapshot_and_ingest_issue_and_refuses_both_saves() {
            use yamaa_adapters::{
                file_preparation, specification_report, specification_run::PortError,
            };
            struct NoPublication;
            impl specification_report::ArtifactPort for NoPublication {
                type Error = ();
                fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
                    panic!("changed source published");
                }
            }
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
            fs::write(study.0.join("output.csv"), b"prior saved output").unwrap();
            let path = study.text("spec.yaml");
            let (mut resources, entry) = file_configuration::resources(&path).unwrap();
            let original = resources.capture("input.csv").unwrap().0;
            let prepared = FileSpecification::prepare(resources, &entry).unwrap();
            let reads = prepared.capture_reads();
            fs::write(study.0.join("input.csv"), b"ID\n2\n").unwrap();
            let mut run_scope = prepared.run_scope(None);
            let attempt = prepared.build(&mut run_scope, Scope::default());
            assert!(matches!(
                &attempt.result,
                Err(PortError::Capture(
                    yamaa_adapters::file_resources::Error::Changed
                ))
            ));
            assert!(file_preparation::opaque_resource_failure(&attempt).is_none());
            assert_eq!(run_scope.capture_reads(), reads);
            assert_eq!(original.as_ref(), b"ID\n1\n");
            let result =
                specification_report::build_result(prepared.run(), &attempt, identity(&path))
                    .unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            let finding = &result.issues()[0];
            assert_eq!(finding.phase, "ingest");
            assert_eq!(finding.condition, "resource_path_content_changed");
            assert_eq!(finding.requirement.as_deref(), Some("REQ-0788"));
            assert_eq!(finding.spec_paths, ["input.SRC.path"]);
            assert_eq!(
                finding.context.clone(),
                yamaa_adapters::json!({"dataset":"SRC","path":"input.csv"})
            );
            for _ in 0..2 {
                assert!(matches!(
                    result.save(&mut NoPublication),
                    Err(yamaa_core::specification_output::SaveError::FailedBuild)
                ));
            }
            assert_eq!(
                fs::read(study.0.join("output.csv")).unwrap(),
                b"prior saved output"
            );
        }
        #[test]
        fn environment_and_invalid_source_profile_precede_entry_and_study_reads() {
            let study = Study::new();
            let path = study.text("absent.yaml");
            let project = study.text("absent-project.yaml");
            let issues = file_application::check(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: Some(&project),
                },
                identity(&path),
            );
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].phase, "validation");
            assert_eq!(issues[0].condition, "project_environment_missing");
            assert_eq!(
                issues[0].context,
                yamaa_adapters::json!({
                    "cause": "missing", "path": project,
                })
            );
            assert_eq!(issues[0].spec_paths, ["environment"]);
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0694"));
            fs::write(
                study.0.join("spec.yaml"),
                VALID.replace("input: {SRC: input.csv}", "input: {SRC: input.unknown}"),
            )
            .unwrap();
            let path = study.text("spec.yaml");
            let issues = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .err()
            .unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "source_profile_unknown");
            assert_eq!(issues[0].phase, "validation");
            assert_eq!(issues[0].requirement, Some("REQ-0852".into()));
            assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"dataset\":\"SRC\",\"path\":\"input.unknown\"}"
            );
            assert!(!study.0.join("input.csv").exists());
            assert!(!study.0.join("input.unknown").exists());
        }
        #[test]
        fn static_check_does_not_need_study_data_and_failed_build_cannot_save() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            let path = study.text("spec.yaml");
            assert!(file_application::check(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None
                },
                identity(&path)
            )
            .is_empty());
            let mut result = study.build();
            assert!(result.output().is_none());
            assert!(!result.issues().is_empty());
            assert!(result.save().is_err());
            assert_eq!(fs::read_dir(&study.0).unwrap().count(), 1);
        }

        #[test]
        fn unknown_suffix_directory_precedes_profile_rejection_through_the_file_entry() {
            let study = Study::new();
            fs::create_dir(study.0.join("input.unknown")).unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                VALID.replace("input.csv", "input.unknown"),
            )
            .unwrap();
            let path = study.text("spec.yaml");
            let issues = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .err()
            .unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "resource_path_not_regular_file");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0785"));
            assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"dataset\":\"SRC\",\"path\":\"input.unknown\"}"
            );
            fs::remove_dir(study.0.join("input.unknown")).unwrap();
            fs::write(study.0.join("input.unknown"), b"ID\n1\n").unwrap();
            let issues = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .err()
            .unwrap();
            assert_eq!(issues[0].condition, "source_profile_unknown");
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn saved_bytes_use_retained_values_and_save_failure_can_be_retried() {
            let study = Study::new();
            fs::write(
                study.0.join("spec.yaml"),
                VALID.replace("path: output.csv", "path: absent/output.csv"),
            )
            .unwrap();
            fs::write(
                study.0.join("input.csv"),
                b"ID\n-9223372036854775808\n9223372036854775807\n",
            )
            .unwrap();
            let mut result = study.build();
            let before = result.output().unwrap().to_vec();
            let observations = result.observations();
            assert_eq!(observations["artifacts"], yamaa_adapters::json!([]));
            assert!(result.issues().is_empty());
            fs::write(study.0.join("input.csv"), b"changed").unwrap();
            fs::write(study.0.join("spec.yaml"), b"[changed").unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(result.observations(), observations);
            assert_eq!(result.issues().len(), 1);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&result.issues()[0].context),
                "{\"code\":\"invalid_target\",\"stage\":\"output\"}"
            );
            assert_eq!(result.output().unwrap(), before);
            fs::create_dir(study.0.join("absent")).unwrap();
            for _ in 0..2 {
                assert!(result.save().unwrap());
                let published = result.observations();
                assert_eq!(
                    published["artifacts"][0]["content"],
                    "ID\n-9223372036854775808\n9223372036854775807\n"
                );
                let mut build = published;
                build["artifacts"] = yamaa_adapters::json!([]);
                assert_eq!(build, observations);
                assert!(result.issues().is_empty());
                assert_eq!(
                    fs::read(study.0.join("absent/output.csv")).unwrap(),
                    b"ID\n-9223372036854775808\n9223372036854775807\n"
                );
            }
            assert_eq!(fs::read_dir(study.0.join("absent")).unwrap().count(), 1);
            fs::remove_file(study.0.join("absent/output.csv")).unwrap();
            fs::remove_dir(study.0.join("absent")).unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(result.observations(), observations);
        }
        #[test]
        fn project_configuration_is_ignored_and_missing_input_has_no_fallback() {
            let study = Study::new();
            fs::create_dir(study.0.join("entry")).unwrap();
            fs::create_dir(study.0.join("extra")).unwrap();
            fs::write(
                study.0.join("yamaa-project.yaml"),
                "version: '1.0'\ndata_roots: [extra]\n",
            )
            .unwrap();
            fs::write(study.0.join("entry/spec.yaml"), VALID).unwrap();
            fs::write(study.0.join("extra/input.csv"), b"ID\n9\n").unwrap();
            fs::write(study.0.join("input.csv"), b"ID\n8\n").unwrap();
            let (resources, entry) =
                file_configuration::resources(&study.text("entry/spec.yaml")).unwrap();
            assert_eq!(resources.capture_reads(), 0);
            let prepared = FileSpecification::prepare(resources, &entry).unwrap();
            assert_eq!(prepared.capture_reads(), 1);
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_err());
            assert_eq!(run_scope.capture_reads(), 1);
            fs::write(study.0.join("entry/input.csv"), b"ID\n1\n").unwrap();
            fs::write(study.0.join("yamaa-project.yaml"), b"[changed").unwrap();
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }
        #[test]
        fn parent_and_absolute_local_paths_build_and_save_held_bytes() {
            for absolute in [false, true] {
                let study = Study::new();
                fs::create_dir(study.0.join("entry")).unwrap();
                fs::write(study.0.join("outside.csv"), b"ID\n1\n").unwrap();
                let input = if absolute {
                    study.text("outside.csv").replace('\\', "/")
                } else {
                    "../outside.csv".to_owned()
                };
                let output = if absolute {
                    study.text("output.csv").replace('\\', "/")
                } else {
                    "../output.csv".to_owned()
                };
                fs::write(
                    study.0.join("entry/spec.yaml"),
                    VALID
                        .replace("input.csv", &input)
                        .replace("output.csv", &output),
                )
                .unwrap();
                let path = study.text("entry/spec.yaml");
                let mut result = file_application::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .unwrap();
                assert!(result.issues().is_empty());
                assert!(!study.0.join("output.csv").exists());
                fs::write(study.0.join("outside.csv"), b"ID\n999\n").unwrap();
                fs::remove_file(study.0.join("entry/spec.yaml")).unwrap();
                assert!(result.save().unwrap());
                assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n1\n");
            }
        }
        #[cfg(unix)]
        #[test]
        fn entry_and_outside_resource_links_remain_refused() {
            let study = Study::new();
            fs::create_dir(study.0.join("entry")).unwrap();
            fs::write(study.0.join("outside.csv"), b"ID\n1\n").unwrap();
            std::os::unix::fs::symlink("outside.csv", study.0.join("outside-link.csv")).unwrap();
            fs::write(
                study.0.join("entry/spec.yaml"),
                VALID.replace("input.csv", "../outside-link.csv"),
            )
            .unwrap();
            let path = study.text("entry/spec.yaml");
            let mut result = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &path,
                    environment: None,
                },
                identity(&path),
            )
            .unwrap();
            assert!(result.output().is_none());
            assert!(result.save().is_err());
            let issues = result.issues();
            // REQ-0783 classifies an authored study-source link before any byte read.
            assert_eq!(issues[0].condition, "resource_path_symlink");
            assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0783"));
            assert_eq!(
                yamaa_adapters::json_wire::encode(&issues[0].context),
                "{\"dataset\":\"SRC\",\"path\":\"../outside-link.csv\"}"
            );
            std::os::unix::fs::symlink("spec.yaml", study.0.join("entry/link.yaml")).unwrap();
            let (resources, entry) =
                file_configuration::resources(&study.text("entry/link.yaml")).unwrap();
            assert!(matches!(
                FileSpecification::prepare(resources, &entry),
                Err(yamaa_adapters::file_preparation::Error::Resource(
                    Error::Symlink
                ))
            ));
        }
        #[test]
        fn malformed_project_configuration_is_not_discovered_or_captured() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            for config in [
                "version: '99.0'\n",
                "version: '1.0'\nunknown: true\n",
                "[not yaml",
            ] {
                fs::write(study.0.join("yamaa-project.yaml"), config).unwrap();
                let (resources, entry) =
                    file_configuration::resources(&study.text("spec.yaml")).unwrap();
                assert_eq!(resources.capture_reads(), 0);
                let prepared = FileSpecification::prepare(resources, &entry).unwrap();
                assert_eq!(prepared.capture_reads(), 1);
            }
        }

        #[cfg(unix)]
        #[test]
        fn publication_refuses_linked_parents_for_existing_and_absent_targets() {
            for existing in [false, true] {
                let study = Study::new();
                let outside = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    VALID.replace("output.csv", "link/output.csv"),
                )
                .unwrap();
                fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
                if existing {
                    fs::write(outside.0.join("output.csv"), b"retained").unwrap();
                }
                std::os::unix::fs::symlink(&outside.0, study.0.join("link")).unwrap();
                let mut result = study.build();
                assert!(result.output().is_some());
                assert!(!result.save().unwrap());
                assert_eq!(
                    yamaa_adapters::json_wire::encode(&result.issues()[0].context),
                    "{\"code\":\"invalid_target\",\"stage\":\"output\"}"
                );
                if existing {
                    assert_eq!(fs::read(outside.0.join("output.csv")).unwrap(), b"retained");
                }
                assert_eq!(
                    fs::read_dir(&outside.0).unwrap().count(),
                    usize::from(existing)
                );
                fs::remove_file(study.0.join("link")).unwrap();
                fs::create_dir(study.0.join("link")).unwrap();
                assert!(result.save().unwrap());
                assert!(result.issues().is_empty());
                assert_eq!(
                    fs::read(study.0.join("link/output.csv")).unwrap(),
                    b"ID\n1\n"
                );
            }
        }

        #[cfg(unix)]
        #[test]
        fn publication_refuses_parent_link_substitution_after_build() {
            let study = Study::new();
            let outside = Study::new();
            fs::create_dir(study.0.join("parent")).unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                VALID.replace("output.csv", "parent/output.csv"),
            )
            .unwrap();
            fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
            let mut result = study.build();
            fs::rename(study.0.join("parent"), study.0.join("held")).unwrap();
            std::os::unix::fs::symlink(&outside.0, study.0.join("parent")).unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
            assert_eq!(fs::read_dir(study.0.join("held")).unwrap().count(), 0);
        }

        #[cfg(unix)]
        #[test]
        fn publication_retains_the_selected_physical_root_after_name_substitution() {
            let study = Study::new();
            let outside = Study::new();
            let root = study.0.join("selected");
            fs::create_dir(&root).unwrap();
            fs::write(root.join("spec.yaml"), VALID).unwrap();
            fs::write(root.join("input.csv"), b"ID\n1\n").unwrap();
            let path = root.join("spec.yaml");
            let path = path.to_str().unwrap();
            let mut result = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap();
            fs::rename(&root, study.0.join("retained")).unwrap();
            std::os::unix::fs::symlink(&outside.0, &root).unwrap();
            assert!(result.save().unwrap());
            assert_eq!(
                fs::read(study.0.join("retained/output.csv")).unwrap(),
                b"ID\n1\n"
            );
            assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
        }

        #[test]
        fn compiler_refusals_retain_the_same_capacity_on_plain_and_environment_routes() {
            use yamaa_adapters::json;
            for (resource, limit, actual, path) in [
                ("columns", 1024, 1025, "columns"),
                ("inputs", 16, 17, "input"),
                ("source_fields", 1024, 1025, "input.SRC.types"),
                ("projected_columns", 1024, 1025, "output.columns"),
                (
                    "source_operands",
                    64,
                    65,
                    "columns.ID.derivation.first_available.sources",
                ),
                ("group_keys", 64, 65, "rows[0].group_by"),
                // No row_columns case: after pruning (REQ-0638) a row's
                // derivations survive only for live columns, and 1025 live
                // columns refuse at the columns capacity first — an inherited
                // specification behaves identically, so the
                // row_columns limit cannot be isolated here.
            ] {
                let study = Study::new();
                let mut body = String::from(VALID);
                match resource {
                    "columns" => {
                        for i in 1..actual {
                            body.push_str(&format!(
                                "  - {{name: C{i}, type: int, derivation: {{literal: 1}}}}\n"
                            ));
                        }
                        // The generated columns must be live under pruning
                        // (REQ-0638) for the resolved-column capacity to be
                        // what the compiler refuses.
                        let projected = (1..actual)
                            .map(|i| format!("C{i}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        body = body.replace(
                            "columns: [ID]}",
                            &format!("columns: [ID, {projected}]}}"),
                        );
                    }
                    "inputs" => {
                        let inputs = (0..actual)
                            .map(|i| format!("S{i}: input.csv"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let sources = (0..actual)
                            .map(|i| format!("S{i}.ID"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        // Every declared input must be read, or pruning
                        // (REQ-0638) discards it before the inputs capacity
                        // is counted.
                        body = body
                            .replace("input: {SRC: input.csv}", &format!("input: {{{inputs}}}"))
                            .replace(
                                "derivation: SRC.ID",
                                &format!(
                                    "derivation: {{first_available: {{sources: [{sources}]}}}}"
                                ),
                            );
                    }
                    "source_fields" => {
                        let fields = (0..actual)
                            .map(|i| format!("F{i}: str"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        body = body.replace(
                            "input: {SRC: input.csv}",
                            &format!("input: {{SRC: {{path: input.csv, types: {{{fields}}}}}}}"),
                        );
                    }
                    "projected_columns" => {
                        body = body.replace(
                            "columns: [ID]",
                            &format!("columns: [{}]", vec!["ID"; actual].join(", ")),
                        );
                    }
                    "source_operands" => {
                        body = body.replace(
                            "derivation: SRC.ID",
                            &format!(
                                "derivation: {{first_available: {{sources: [{}]}}}}",
                                vec!["SRC.ID"; actual].join(", ")
                            ),
                        );
                    }
                    "group_keys" => body.push_str(&format!(
                        "rows:\n  - {{id: r, dataset: SRC, filter: 'ID >', group_by: [{}], derivations: {{ID: SRC.ID}}}}\n",
                        vec!["ID"; actual].join(", ")
                    )),
                    _ => unreachable!(),
                }
                fs::write(study.0.join("spec.yaml"), &body).unwrap();
                let spec = study.text("spec.yaml");
                let project = study.project();
                for environment in [None, Some(project.as_str())] {
                    let request = || Request {
                        specification: &spec,
                        environment,
                    };
                    for issues in [
                        file_application::check(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(&spec),
                        ),
                        file_application::domain(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(&spec),
                        )
                        .err()
                        .expect("refused before execution"),
                    ] {
                        assert_eq!(issues.len(), 1, "{resource}, {environment:?}");
                        let issue = &issues[0];
                        assert_eq!(issue.phase, "engine");
                        assert_eq!(issue.condition, "engine_rejected");
                        assert_eq!(issue.requirement, None);
                        assert_eq!(issue.spec_paths, [path]);
                        assert_eq!(
                            issue.context.clone(),
                            json!({
                                "stage":"prepare", "code":"compiler_limit", "resource":resource, "limit":limit, "actual":actual,
                            })
                        );
                    }
                }
                if resource == "source_operands" {
                    let admitted = body.replace(
                        &vec!["SRC.ID"; actual].join(", "),
                        &vec!["SRC.ID"; limit].join(", "),
                    );
                    fs::write(study.0.join("spec.yaml"), admitted).unwrap();
                    fs::write(study.0.join("input.csv"), "ID\n1\n").unwrap();
                    for environment in [None, Some(project.as_str())] {
                        assert!(file_application::check(
                            yamaa_core::project_function::Language::Python,
                            Request {
                                specification: &spec,
                                environment,
                            },
                            identity(&spec)
                        )
                        .is_empty());
                    }
                    let mut domain = file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        Request {
                            specification: &spec,
                            environment: None,
                        },
                        identity(&spec),
                    )
                    .unwrap();
                    assert!(domain.issues().is_empty());
                    assert!(domain.save().unwrap());
                    assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n1\n");
                }
            }
        }

        #[test]
        fn row_shape_boundaries_are_per_template_and_preserve_authored_preflight_precedence() {
            use yamaa_adapters::json;
            use yamaa_core::{
                limits::Limits,
                table::{TableAccess, ValueRef},
                value::ColumnType,
            };
            let study = Study::new();
            let spec = study.text("spec.yaml");
            let project_file = study.project();
            // At the cardinality boundary ordinary group validation still wins
            // over the deferred invalid filter; one more group is refused above.
            fs::write(
                study.0.join("spec.yaml"),
                format!(
                    "{VALID}rows:\n  - {{id: r, dataset: SRC, filter: 'ID >', group_by: [{}], derivations: {{ID: SRC.ID}}}}\n",
                    vec!["ID"; Limits::KEYS].join(", "),
                ),
            )
            .unwrap();
            for environment in [None, Some(project_file.as_str())] {
                let request = || Request {
                    specification: &spec,
                    environment,
                };
                for issues in [
                    file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(&spec),
                    ),
                    file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(&spec),
                    )
                    .err()
                    .unwrap(),
                ] {
                    assert_eq!(issues.len(), 1);
                    assert_eq!(issues[0].phase, "validation");
                    assert_eq!(issues[0].condition, "invalid_field_type");
                    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0065"));
                    assert_eq!(issues[0].spec_paths, ["rows[0].group_by"]);
                    assert_eq!(
                        issues[0].context.clone(),
                        json!({"row":"r", "group_by":vec!["ID"; Limits::KEYS]}),
                    );
                }
            }
            assert!(!study.0.join("input.csv").exists());
            assert!(!study.0.join("output.csv").exists());

            let groups = (0..Limits::KEYS)
                .map(|i| format!("SRC.G{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut body = String::from("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int}\n");
            for i in 1..Limits::COLUMNS {
                body.push_str(&format!("  - {{name: C{i}, type: int}}\n"));
            }
            body.push_str("rows:\n");
            for row in 1..=2 {
                let derivations = std::iter::once(format!("ID: {{literal: {row}}}"))
                    .chain((1..Limits::COLUMNS).map(|i| format!("C{i}: {{literal: 1}}")))
                    .collect::<Vec<_>>()
                    .join(", ");
                body.push_str(&format!("  - {{id: r{row}, dataset: SRC, group_by: [{groups}], derivations: {{{derivations}}}}}\n"));
            }
            fs::write(study.0.join("spec.yaml"), body).unwrap();
            let header = (0..Limits::KEYS)
                .map(|i| format!("G{i}"))
                .collect::<Vec<_>>()
                .join(",");
            fs::write(
                study.0.join("input.csv"),
                format!("{header}\n{}\n", vec!["0"; Limits::KEYS].join(",")),
            )
            .unwrap();
            // Both templates independently reach 64 group keys and 1024 row
            // derivations. Their combined counts are not planner quotas.
            for environment in [None, Some(project_file.as_str())] {
                let request = || Request {
                    specification: &spec,
                    environment,
                };
                assert!(file_application::check(
                    yamaa_core::project_function::Language::Python,
                    request(),
                    identity(&spec)
                )
                .is_empty());
            }
            let mut domain = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &spec,
                    environment: None,
                },
                identity(&spec),
            )
            .unwrap();
            assert!(domain.issues().is_empty());
            assert!(domain.save().unwrap());
            assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n1\n2\n");

            // Native::project deliberately has no installed host. Exercise the
            // environment build through the public project service and a host
            // port that proves this functionless specification never activates.
            struct NoFunctions;
            impl yamaa_core::project_activation::ActivationPort for NoFunctions {
                type Handle = ();
                type Error = ();
                fn verify_lock(
                    &mut self,
                    _: yamaa_core::project_function::Language,
                    _: &yamaa_core::project_environment::LockReference,
                    _: &[yamaa_core::function_signature::ProjectFunctionIdentity],
                ) -> Result<(), ()> {
                    panic!("functionless row build must not verify a lock")
                }
                fn bind(
                    &mut self,
                    _: &yamaa_core::function_signature::ProjectFunctionIdentity,
                    _: &yamaa_core::function_signature::LogicalSignature,
                ) -> Result<(), ()> {
                    panic!("functionless row build must not bind")
                }
                fn invoke(
                    &mut self,
                    _: &(),
                    _: &[yamaa_core::function_invocation::Argument<'_>],
                ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<()>>
                {
                    panic!("functionless row build must not invoke")
                }
            }
            let project = yamaa_adapters::project_application::prepare(
                &spec,
                &project_file,
                yamaa_core::project_function::Language::Python,
            )
            .unwrap();
            let attempt = project.build(
                &mut project.run_scope(None),
                &mut NoFunctions,
                Scope::default(),
            );
            assert!(attempt.boundary.is_ok());
            let result = yamaa_adapters::project_report::build_result(
                project.run(),
                &attempt,
                identity(&spec),
                &[],
            )
            .unwrap();
            assert!(result.issues().is_empty());
            let table =
                yamaa_adapters::ipc_table::decode_snapshot(result.output().unwrap()).unwrap();
            assert_eq!(
                table.schema(),
                &crate::support::table_schema(&[("ID", ColumnType::Int)])
            );
            assert_eq!(table.row_count(), 2);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(1));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Int(2));
            struct Save(Vec<(String, Vec<u8>)>);
            impl yamaa_core::specification_output::ArtifactPort for Save {
                type Error = ();
                fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
                    self.0.push((path.into(), bytes.into()));
                    Ok(())
                }
            }
            let mut saved = Save(Vec::new());
            result.save(&mut saved).unwrap();
            assert_eq!(saved.0, [("output.csv".into(), b"ID\n1\n2\n".to_vec())]);
        }

        #[test]
        fn iterative_inheritance_admits_sixty_five_layers_during_check_and_build() {
            let study = Study::new();
            fs::write(study.0.join("base.yaml"), VALID).unwrap();
            fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
            for index in 0..65 {
                let parent = if index < 64 {
                    format!("layer-{}.yaml", index + 1)
                } else {
                    "base.yaml".into()
                };
                fs::write(
                    study.0.join(format!("layer-{index}.yaml")),
                    format!("schema_version: '1.0'\nparent: {parent}\n"),
                )
                .unwrap();
            }
            fs::write(
                study.0.join("spec.yaml"),
                "schema_version: '1.0'\nparent: layer-0.yaml\n",
            )
            .unwrap();
            let spec = study.text("spec.yaml");
            let project = study.project();
            for environment in [None, Some(project.as_str())] {
                assert!(file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &spec,
                        environment
                    },
                    identity(&spec)
                )
                .is_empty());
            }
            let mut result = file_application::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: &spec,
                    environment: None,
                },
                identity(&spec),
            )
            .unwrap();
            assert!(result.issues().is_empty());
            assert!(result.output().is_some());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n1\n");
        }

        #[test]
        fn wide_integer_in_source_keeps_its_exact_language_finding_on_both_public_routes() {
            let study = Study::new();
            let digits = "9".repeat(4097);
            fs::write(
                study.0.join("spec.yaml"),
                format!("schema_version: {digits}\n"),
            )
            .unwrap();
            let spec = study.text("spec.yaml");
            let project = study.project();
            for environment in [None, Some(project.as_str())] {
                let request = || Request {
                    specification: &spec,
                    environment,
                };
                for issues in [
                    file_application::check(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(&spec),
                    ),
                    file_application::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(&spec),
                    )
                    .err()
                    .unwrap(),
                ] {
                    assert_eq!(issues.len(), 1);
                    let issue = &issues[0];
                    assert_eq!(issue.phase, "validation");
                    assert_eq!(issue.condition, "schema_version_mismatch");
                    assert_eq!(issue.requirement.as_deref(), Some("REQ-0281"));
                    assert_eq!(issue.spec_paths, ["schema_version"]);
                    assert_eq!(
                        yamaa_adapters::json_wire::encode(&issue.context),
                        format!(r#"{{"actual":{digits},"expected":"1.0"}}"#)
                    );
                }
            }
        }

        #[test]
        fn catastrophic_pattern_is_an_ordinary_verification_failure() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, TERM]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: TERM\n    type: str\n    derivation: SRC.TERM\n    verifications: [{matches: {pattern: '(a+)+b'}}]\n").unwrap();
            fs::write(
                study.0.join("input.csv"),
                format!("ID,TERM\n1,{}\n", "a".repeat(32)),
            )
            .unwrap();
            // (a+)+b cannot match a run of a's. Matching is linear, so the value
            // simply fails the check rather than exhausting a step budget.
            let result = study.build();
            assert!(result.output().is_none());
            let findings = result.issues();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].phase, "verification");
            assert_eq!(findings[0].condition, "matches_failed");
            assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0379"));
            assert_eq!(
                findings[0].spec_paths,
                ["columns.TERM.verifications[0].matches"]
            );
            let context: yamaa_core::json::Value = findings[0].context.clone();
            assert_eq!(
                context,
                yamaa_adapters::json!({"column":"TERM","failure_count":1,"keys":[{"ID":1}]})
            );
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn data_driven_like_preserves_false_and_unicode_truth_at_publication() {
            let study = Study::new();
            fs::write(
                study.0.join("spec.yaml"),
                format!(
                    "{}rows:\n  - id: selected\n    dataset: SRC\n    filter: SRC.A LIKE SRC.B\n    derivations: {{ID: SRC.ID}}\n",
                    VALID.replace(", derivation: SRC.ID", "")
                ),
            )
            .unwrap();
            fs::write(
                study.0.join("input.csv"),
                format!("ID,A,B\n1,{},{}\n", "a".repeat(100), "%a".repeat(100) + "b"),
            )
            .unwrap();
            let mut result = study.build();
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.output().is_some());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n");

            // A fresh build retains ordinary Unicode full-match truth.
            fs::write(study.0.join("input.csv"), "ID,A,B\n1,😀,_\n2,a,b\n").unwrap();
            let mut result = study.build();
            assert!(result.issues().is_empty());
            assert!(result.save().unwrap());
            assert_eq!(fs::read(study.0.join("output.csv")).unwrap(), b"ID\n1\n");
        }
    }
    mod file_entry {
        //! Entry capture reads each `schema`-linked producing specification once,
        //! as metadata. No producer is compiled, executed or followed further.
        use std::{
            collections::BTreeMap,
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{
            file_entry::{Error, RejectedEntry},
            file_preparation::{Error as DocumentError, FileSpecification, RejectedPlain},
            file_resources::{Error as ResourceError, Resources},
            file_workflow::{self, SelectionError},
            resolved_artifact::model::{Location, ResourceRole},
            shipped_schema,
        };
        use yamaa_core::{
            build::Prepared, project_function::Language, specification::PrepareError,
            value::ColumnType,
        };

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let root = std::env::temp_dir().join(format!(
                    "yamaa-native-entry-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&root).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
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
            fn resources(&self) -> Resources {
                Resources::new(&self.path(""), &self.path(""), &[]).unwrap()
            }
            fn select(&self) -> Result<yamaa_adapters::file_workflow::Selected, SelectionError> {
                select(self.resources())
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        fn select(
            resources: Resources,
        ) -> Result<yamaa_adapters::file_workflow::Selected, SelectionError> {
            file_workflow::select_with_optional_resources(
                resources,
                "root.yaml",
                None,
                Language::Python,
                shipped_schema::capture().unwrap(),
                shipped_schema::capture_project().unwrap(),
            )
        }
        fn plain(
            result: Result<yamaa_adapters::file_workflow::Selected, SelectionError>,
        ) -> FileSpecification {
            match result {
                Ok(Prepared::Plain(prepared)) => prepared,
                Ok(Prepared::Ordinary(_)) => panic!("plain owner"),
                Err(error) => panic!("plain preparation: {error:?}"),
            }
        }
        fn rejected_entry(
            result: Result<yamaa_adapters::file_workflow::Selected, SelectionError>,
        ) -> Box<RejectedEntry> {
            match result {
                Err(SelectionError::Preparation(file_workflow::Error::Entry(error))) => error,
                Ok(_) => panic!("expected an entry rejection"),
                Err(error) => panic!("expected an entry rejection: {error:?}"),
            }
        }
        fn rejected_plain(
            result: Result<yamaa_adapters::file_workflow::Selected, SelectionError>,
        ) -> Box<RejectedPlain> {
            match result {
                Err(SelectionError::Plain(error)) => error,
                Ok(_) => panic!("expected a compilation rejection"),
                Err(error) => panic!("expected a compilation rejection: {error:?}"),
            }
        }
        fn yaml(input: &str, base: &str, output: &str) -> String {
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: float, label: Reported value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE], decimals: 2}}\n")
        }
        fn leaf(study: &Study) {
            study.write(
                "leaf.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv"),
            );
        }
        fn schema_locations(prepared: &FileSpecification) -> BTreeMap<String, String> {
            let artifact = prepared.resolved_specification().unwrap();
            artifact.entries()[0]
                .resources
                .iter()
                .filter(|resource| resource.role == ResourceRole::ProducerSchema)
                .map(|resource| {
                    let Location::Local { path } = &resource.location;
                    (resource.spec_path.clone(), path.clone())
                })
                .collect()
        }

        #[test]
        fn aliased_links_share_one_document_and_producer_links_are_not_followed() {
            let study = Study::new();
            leaf(&study);
            study.write(
                "left.yaml",
                &yaml(
                    "{LEAF: {path: leaf.csv, schema: './leaf.yaml'}}",
                    "LEAF",
                    "left.csv",
                ),
            );
            study.write(
                "right.yaml",
                &yaml(
                    "{LEAF: {path: leaf.csv, schema: leaf.yaml}}",
                    "LEAF",
                    "right.csv",
                ),
            );
            // VALUE reads all three linked datasets so pruning (REQ-0638) keeps
            // every link in the resolved model.
            let root = yaml("{RIGHT: {path: right.csv, schema: right.yaml}, LEFT: {path: left.csv, schema: left.yaml}, AGAIN: {path: left.csv, schema: './left.yaml'}}", "RIGHT", "root.csv")
                .replace(
                    "derivation: RIGHT.VALUE",
                    "derivation: {first_available: {sources: [RIGHT.VALUE, LEFT.VALUE, AGAIN.VALUE]}}",
                );
            study.write("root.yaml", &root);
            let prepared = plain(study.select());
            // The entry and its two distinct producers; the producers' own
            // `schema` link to leaf.yaml is never read.
            assert_eq!(prepared.capture_reads(), 3);
            let paths = prepared.resolved_specification().unwrap().entries().len();
            assert_eq!(paths, 1);
            let compiled = prepared.run().compiled();
            for (index, source) in compiled.sources().iter().enumerate() {
                assert_eq!(
                    source.types,
                    [
                        ("ID".to_owned(), ColumnType::Int),
                        ("VALUE".to_owned(), ColumnType::Float)
                    ]
                );
                assert!(compiled.source_contract(index).is_some());
            }
            assert_eq!(
                schema_locations(&prepared),
                BTreeMap::from([
                    ("input.AGAIN.schema".to_owned(), study.path("left.yaml")),
                    ("input.LEFT.schema".to_owned(), study.path("left.yaml")),
                    ("input.RIGHT.schema".to_owned(), study.path("right.yaml")),
                ])
            );
            for path in [
                "never-read.csv",
                "leaf.csv",
                "left.csv",
                "right.csv",
                "root.csv",
            ] {
                assert!(!study.0.join(path).exists());
            }
        }

        #[test]
        fn inherited_schema_and_input_origins_resolve_independently() {
            let study = Study::new();
            study.write(
                "origins/schema.yaml",
                &yaml(
                    "{P: {path: ../data/p.csv, schema: '../producer/p.yaml'}}",
                    "P",
                    "unused.csv",
                ),
            );
            // The chain contributes origins/schema.yaml, then origins/path.yaml,
            // then the entry.
            study.write(
                "origins/path.yaml",
                &yaml("{P: ../data/p.csv}", "P", "root.csv").replacen(
                    "schema_version: '1.0'\n",
                    "schema_version: '1.0'\nparent: schema.yaml\n",
                    1,
                ),
            );
            study.write(
                "producer/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "elsewhere.csv"),
            );
            study.write(
                "producer/p.yaml",
                "schema_version: '1.0'\nparent: base.yaml\ndomain: PRODUCER\n",
            );
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparent: origins/path.yaml\ndomain: ROOT\n",
            );
            let prepared = plain(study.select());
            assert_eq!(prepared.capture_reads(), 5);
            let artifact = prepared.resolved_specification().unwrap();
            let resources = &artifact.entries()[0].resources;
            let schema = resources
                .iter()
                .find(|resource| resource.spec_path == "input.P.schema")
                .unwrap();
            assert_eq!(schema.declaring_source, study.path("origins/schema.yaml"));
            assert_eq!(schema.written, "../producer/p.yaml");
            let Location::Local { path } = &schema.location;
            assert_eq!(path, &study.path("producer/p.yaml"));
            let input = resources
                .iter()
                .find(|resource| resource.spec_path == "input.P.path")
                .unwrap();
            assert_eq!(input.declaring_source, study.path("origins/path.yaml"));
            // The producer's own output path is not consulted.
            let compiled = prepared.run().compiled();
            assert_eq!(compiled.sources()[0].types.len(), 2);
            assert!(!study.0.join("data").exists());
        }

        #[test]
        fn missing_producer_keeps_captured_consumer_and_original_declaration() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: absent.csv, schema: missing.yaml}}",
                    "P",
                    "root.csv",
                ),
            );
            let failure = rejected_entry(study.select());
            let Error::Document(DocumentError::ResourceFinding { error, diagnostic }) =
                failure.error()
            else {
                panic!("classified link failure: {:?}", failure.error());
            };
            assert_eq!(*error, ResourceError::Missing);
            assert_eq!(diagnostic.definition().condition, "resource_path_missing");
            assert_eq!(diagnostic.spec_paths, ["input.P.schema"]);
            assert_eq!(
                diagnostic.context["path"],
                yamaa_core::json::Value::String("missing.yaml".into())
            );
            assert_eq!(failure.origin().unwrap().dataset, "P");
            assert!(failure.document().is_some());
            assert_eq!(failure.entry(), "root.yaml");
            assert_eq!(failure.capture_reads(), 1);
        }

        #[test]
        fn malformed_entry_and_failed_parent_raw_bytes_survive_without_a_prepared_document() {
            for parent in [false, true] {
                let study = Study::new();
                let malformed = "schema_version: '1.0'\ndomain: [\n";
                let bad = if parent { "broken.yaml" } else { "root.yaml" };
                study.write(bad, malformed);
                if parent {
                    study.write(
                        "root.yaml",
                        "schema_version: '1.0'\nparent: broken.yaml\ndomain: ROOT\n",
                    );
                }
                let failure = rejected_entry(study.select());
                assert!(matches!(failure.error(), Error::Document(_)));
                assert_eq!(failure.entry(), "root.yaml");
                assert!(failure.document().is_none());
                assert_eq!(failure.capture_reads(), if parent { 2 } else { 1 });
                let captures = failure.captured_sources().collect::<BTreeMap<_, _>>();
                assert_eq!(captures[&study.path(bad)], malformed.as_bytes());
                fs::remove_file(study.0.join(bad)).unwrap();
                assert_eq!(
                    failure
                        .captured_sources()
                        .find(|(p, _)| p == &study.path(bad))
                        .unwrap()
                        .1,
                    malformed.as_bytes()
                );
            }
        }

        #[test]
        fn metadata_fallback_does_not_redirect_declared_artifact_locations() {
            let study = Study::new();
            study.write(
                "entry/root.yaml",
                &yaml(
                    "{P: {path: ../fallback/p.csv, schema: p.yaml}}",
                    "P",
                    "root.csv",
                ),
            );
            study.write(
                "fallback/p.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "elsewhere.csv"),
            );
            let resources = Resources::new(
                &study.path(""),
                &study.path("entry"),
                &[study.path("fallback")],
            )
            .unwrap();
            // The producing specification is found through the metadata
            // fallback; the consumer still reads its own declared artifact path.
            let prepared = plain(select(resources));
            let compiled = prepared.run().compiled();
            assert!(compiled.source_contract(0).is_some());
            assert_eq!(compiled.sources()[0].path, "../fallback/p.csv");
            assert_eq!(prepared.capture_reads(), 2);
            assert!(!study.0.join("fallback/p.csv").exists());
        }

        #[test]
        fn already_captured_changed_metadata_keeps_original_snapshot_and_native_failure() {
            let study = Study::new();
            let before = yaml("{RAW: never-read.csv}", "RAW", "root.csv");
            study.write("root.yaml", &before);
            let mut resources = study.resources();
            resources.capture_metadata("root.yaml").unwrap();
            study.write("root.yaml", &before.replace("TEST", "NEXT"));
            let failure = rejected_entry(select(resources));
            let Error::Document(DocumentError::ResourceFinding { error, diagnostic }) =
                failure.error()
            else {
                panic!(
                    "changed metadata retains its original native cause and authored diagnostic"
                );
            };
            assert_eq!(*error, ResourceError::Changed);
            assert_eq!(diagnostic.definition().phase, "ingest");
            assert_eq!(
                diagnostic.definition().condition,
                "resource_path_content_changed"
            );
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0788"));
            assert_eq!(diagnostic.spec_paths, ["specification"]);
            assert_eq!(
                diagnostic.context["path"],
                yamaa_core::json::Value::String("root.yaml".into())
            );
            assert_eq!(failure.capture_reads(), 1);
            assert_eq!(
                failure.captured_sources().next().unwrap().1,
                before.as_bytes()
            );
        }

        #[test]
        fn producer_derivations_are_never_compiled_but_consumer_failures_remain() {
            for producer in [false, true] {
                let study = Study::new();
                leaf(&study);
                let root = yaml("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "root.csv");
                study.write("root.yaml", &root);
                let path = if producer { "leaf.yaml" } else { "root.yaml" };
                let original = fs::read_to_string(study.0.join(path)).unwrap();
                let bad = original.replace(
                    if producer { "RAW.VALUE" } else { "P.VALUE" },
                    "{function: {name: missing, args: {x: 1}}}",
                );
                study.write(path, &bad);
                let result = study.select();
                if producer {
                    // Only the output contract is read from a linked producer.
                    let prepared = plain(result);
                    assert_eq!(prepared.capture_reads(), 2);
                } else {
                    let failure = rejected_plain(result);
                    let DocumentError::Compile(yamaa_adapters::specification_run::Error::Prepare(
                        PrepareError::Invalid(findings),
                    )) = failure.error()
                    else {
                        panic!("consumer compilation: {:?}", failure.error());
                    };
                    assert!(findings.iter().any(|finding| finding
                        .diagnostic()
                        .definition()
                        .condition
                        == "unknown_project_function"));
                    assert_eq!(failure.capture_reads(), 2);
                }
                assert!(!study.0.join("never-read.csv").exists());
                assert!(!study.0.join("leaf.csv").exists());
                assert!(!study.0.join("root.csv").exists());
            }
        }

        #[test]
        fn original_redundant_types_keep_req_0523_after_link_capture() {
            let study = Study::new();
            leaf(&study);
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
                    "P",
                    "root.csv",
                ),
            );
            let failure = rejected_plain(study.select());
            let DocumentError::Compile(yamaa_adapters::specification_run::Error::Prepare(
                PrepareError::Invalid(findings),
            )) = failure.error()
            else {
                panic!("original redundant authority");
            };
            let finding = findings[0].diagnostic();
            assert_eq!(finding.definition().requirement, Some("REQ-0523"));
            assert_eq!(finding.spec_paths, ["input.P.types.ID"]);
            assert_eq!(failure.capture_reads(), 2);
        }

        #[test]
        fn a_link_naming_the_consuming_specification_is_refused() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml(
                    "{SELF: {path: root.csv, schema: './root.yaml'}}",
                    "SELF",
                    "root.csv",
                ),
            );
            let failure = rejected_plain(study.select());
            let DocumentError::Compile(yamaa_adapters::specification_run::Error::Prepare(
                PrepareError::Invalid(findings),
            )) = failure.error()
            else {
                panic!("self link: {:?}", failure.error());
            };
            let finding = findings[0].diagnostic();
            assert_eq!(finding.definition().condition, "invalid_producer_contract");
            assert_eq!(finding.spec_paths, ["input.SELF.schema"]);
            assert_eq!(
                finding.context["reason"],
                yamaa_core::json::Value::String("self_reference".into())
            );
            assert_eq!(failure.capture_reads(), 1);
        }

        #[cfg(unix)]
        #[test]
        fn symlink_and_escape_schema_failures_are_terminal_and_artifact_symlinks_are_not_opened() {
            use std::os::unix::fs::symlink;
            let study = Study::new();
            leaf(&study);
            symlink("leaf.yaml", study.0.join("link.yaml")).unwrap();
            for schema in ["link.yaml", "../escape.yaml"] {
                study.write(
                    "root.yaml",
                    &yaml(
                        &format!("{{P: {{path: leaf.csv, schema: '{schema}'}}}}"),
                        "P",
                        "root.csv",
                    ),
                );
                let failure = rejected_entry(study.select());
                match (schema, failure.error()) {
                    (
                        "link.yaml",
                        Error::Document(DocumentError::ResourceFinding { error, diagnostic }),
                    ) => {
                        assert_eq!(*error, ResourceError::Symlink);
                        assert_eq!(diagnostic.definition().condition, "resource_path_symlink");
                        assert_eq!(diagnostic.spec_paths, ["input.P.schema"]);
                    }
                    ("../escape.yaml", Error::Document(DocumentError::Resource(error))) => {
                        assert_eq!(*error, ResourceError::OutsideRoots);
                    }
                    (_, error) => panic!("terminal link failure: {error:?}"),
                }
                assert_eq!(failure.capture_reads(), 1);
            }
            symlink("not-present", study.0.join("leaf.csv")).unwrap();
            symlink("not-present", study.0.join("never-read.csv")).unwrap();
            study.write(
                "root.yaml",
                &yaml("{P: {path: leaf.csv, schema: leaf.yaml}}", "P", "root.csv"),
            );
            let prepared = plain(study.select());
            assert_eq!(prepared.capture_reads(), 2);
            assert!(fs::symlink_metadata(study.0.join("leaf.csv"))
                .unwrap()
                .file_type()
                .is_symlink());
        }

        #[cfg(unix)]
        #[test]
        fn canonical_unix_root_with_literal_backslash_remains_an_identity() {
            let outer = Study::new();
            let root = outer.0.join("named\\root");
            fs::create_dir(&root).unwrap();
            let study = Study(root.clone());
            leaf(&study);
            study.write(
                "root.yaml",
                &yaml(
                    "{P: {path: leaf.csv, schema: './leaf.yaml'}}",
                    "P",
                    "root.csv",
                ),
            );
            let prepared = plain(study.select());
            assert_eq!(
                schema_locations(&prepared),
                BTreeMap::from([(
                    "input.P.schema".to_owned(),
                    root.join("leaf.yaml").to_str().unwrap().to_owned()
                )])
            );
            assert_eq!(prepared.capture_reads(), 2);
        }
    }
    mod file_preparation {
        use crate::support::ipc_snapshot;
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::json;
        use yamaa_adapters::{
            file_preparation::{Error, FileSpecification},
            file_resources::Resources,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::json::Value;
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-file-preparation-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                for name in ["entry", "parent", "outside"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn text(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().into()
            }
            fn resource_key(&self, name: &str) -> String {
                // Protocol identities use forward separators; native IO keeps the
                // host spelling.
                let path = self.text(name);
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn prepare(&self, source: &str) -> Result<FileSpecification, Error> {
                fs::write(self.0.join("entry/spec.yaml"), source).unwrap();
                FileSpecification::prepare(
                    Resources::new(&self.text(""), &self.text("entry"), &[]).unwrap(),
                    "spec.yaml",
                )
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        fn failure(study: &Study, source: &str) -> String {
            match study.prepare(source) {
                Ok(_) => panic!("expected preparation failure"),
                Err(error) => error.into_message(),
            }
        }
        #[test]
        fn standalone_retains_entry_and_checks_without_study_capture() {
            let study = Study::new();
            let prepared = study.prepare(VALID).unwrap();
            assert_eq!(prepared.capture_reads(), 1);
            assert_eq!(prepared.run().document().source().bytes, VALID.as_bytes());
            assert_eq!(
                yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
                "[]"
            );
            assert_eq!(prepared.capture_reads(), 1);
            // Compilation uses the retained original model rather than reopening entry YAML.
            fs::write(study.0.join("entry/spec.yaml"), b"[changed").unwrap();
            fs::write(study.0.join("entry/input.csv"), b"ID\n1\n").unwrap();
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }

        #[cfg(unix)]
        #[test]
        fn canonical_entry_identity_retains_a_selected_root_with_a_backslash_name() {
            let study = Study::new();
            let root = study.0.join("named\\root");
            fs::create_dir(&root).unwrap();
            fs::write(root.join("spec.yaml"), VALID).unwrap();
            fs::write(root.join("input.csv"), b"ID\n1\n").unwrap();
            let resources =
                Resources::new(root.to_str().unwrap(), root.to_str().unwrap(), &[]).unwrap();
            let prepared = FileSpecification::prepare(resources, "spec.yaml").unwrap();
            assert_eq!(
                prepared.run().document().source().identity,
                fs::canonicalize(root.join("spec.yaml"))
                    .unwrap()
                    .to_str()
                    .unwrap()
            );
            assert_eq!(prepared.capture_reads(), 1);
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }
        #[test]
        fn entry_version_finding_precedes_parent_and_study_authority() {
            let study = Study::new();
            let actual: Value = yamaa_adapters::json_wire::decode(&failure(
                &study,
                &format!(
                    "{}parent: ../outside/missing.yaml\n",
                    VALID.replace("'1.0'", "'99.0'")
                ),
            ))
            .unwrap();
            assert_eq!(
                actual,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0245","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":study.resource_key("entry/spec.yaml"),"entry":study.resource_key("entry/spec.yaml")}}]}})
            );
        }
        #[test]
        fn missing_parent_retains_complete_authored_reference_finding() {
            let study = Study::new();
            let actual: Value = yamaa_adapters::json_wire::decode(&failure(
                &study,
                &format!("{VALID}parent: absent.yaml\n"),
            ))
            .unwrap();
            assert_eq!(
                actual,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parent"],"context":{"path":"absent.yaml","source":study.resource_key("entry/spec.yaml")}}]}})
            );
        }
        #[test]
        fn invalid_parent_preserves_complete_layer_context() {
            let study = Study::new();
            fs::write(
                study.0.join("parent/base.yaml"),
                "schema_version: '1.0'\ndomain: bad-name\n",
            )
            .unwrap();
            let actual: Value = yamaa_adapters::json_wire::decode(&failure(
                &study,
                &format!("{VALID}parent: ../parent/base.yaml\n"),
            ))
            .unwrap();
            assert_eq!(
                actual,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":study.resource_key("parent/base.yaml"),"entry":study.resource_key("entry/spec.yaml")}}]}})
            );
        }
        #[test]
        fn inherited_paths_rebase_lexically_and_parent_bytes_are_retained_once() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"), VALID).unwrap();
            let prepared = study
                .prepare("schema_version: '1.0'\nparent: ../parent/./base.yaml\n")
                .unwrap();
            assert_eq!(prepared.capture_reads(), 2);
            assert_eq!(prepared.run().document().parents().len(), 1);
            assert_eq!(
                prepared.run().document().parents()[0].source().bytes,
                VALID.as_bytes()
            );
            assert_eq!(prepared.run().source().path, "../parent/input.csv");
            // No data file was needed for model preparation or declaration-only checking.
            assert_eq!(
                yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
                "[]"
            );
            fs::write(study.0.join("parent/input.csv"), b"ID\n1\n").unwrap();
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 3);
            assert!(prepared
                .build(&mut run_scope, Scope::default())
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 3);
        }
        #[cfg(unix)]
        #[test]
        fn selected_read_roots_and_no_link_authority_remain_enforced() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"), VALID).unwrap();
            std::os::unix::fs::symlink(
                study.0.join("parent/base.yaml"),
                study.0.join("entry/link.yaml"),
            )
            .unwrap();
            assert_eq!(
                failure(&study, &format!("{VALID}parent: link.yaml\n")),
                "resource path contains a symbolic link"
            );
            let resources =
                Resources::new(&study.text("entry"), &study.text("entry"), &[]).unwrap();
            fs::write(
                study.0.join("entry/spec.yaml"),
                format!("{VALID}parent: ../parent/base.yaml\n"),
            )
            .unwrap();
            let error = match FileSpecification::prepare(resources, "spec.yaml") {
                Ok(_) => panic!("root escape accepted"),
                Err(error) => error,
            };
            assert_eq!(error.into_message(), "resource path outside approved roots");
        }

        #[cfg(unix)]
        #[test]
        fn unreadable_parent_is_unavailable_without_fallback_or_study_capture() {
            use std::os::unix::fs::PermissionsExt;
            let study = Study::new();
            let parent = study.0.join("entry/blocked.yaml");
            fs::write(&parent, VALID).unwrap();
            fs::write(study.0.join("blocked.yaml"), VALID).unwrap();
            let permissions = fs::metadata(&parent).unwrap().permissions();
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o0)).unwrap();
            if fs::File::open(&parent).is_ok() {
                fs::set_permissions(&parent, permissions).unwrap();
                eprintln!("process bypasses DAC; unreadable-parent case not exercised");
                return;
            }
            let result = study.prepare(&format!("{VALID}parent: blocked.yaml\n"));
            fs::set_permissions(&parent, permissions).unwrap();
            let error = match result {
                Ok(_) => panic!("unreadable parent accepted through fallback"),
                Err(error) => error,
            };
            let actual: Value = yamaa_adapters::json_wire::decode(&error.into_message()).unwrap();
            assert_eq!(
                actual,
                json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parent"],"context":{"path":"blocked.yaml","source":study.resource_key("entry/spec.yaml")}}]}})
            );
        }
        #[test]
        fn all_original_file_builds_preserve_full_reports_and_cached_counters() {
            use yamaa_adapters::specification_report::{self, Identity};
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            let names = [
                "negative-zero-division",
                "negative-integer-overflow",
                "adam-adlb-ordered-sum",
                "schema-window-functions",
                "schema-inheritance",
                "schema-lookup",
                "negative-formula-flag",
                "negative-row-aggregate",
                "negative-row-no-prior",
                "negative-source-missing-field",
                "negative-source-trivial-filter",
                "negative-paired-dates",
                "negative-not-missing-age",
                "negative-implausible-age",
                "negative-invalid-sex",
                "negative-sex-code",
                "negative-matches-bad-pattern",
            ];
            for name in names {
                let case = fs::canonicalize(root.join(name)).unwrap();
                let entry = if name == "schema-inheritance" {
                    "spec_study.yaml"
                } else {
                    "spec.yaml"
                };
                let prepared = FileSpecification::prepare(
                    Resources::new(case.to_str().unwrap(), case.to_str().unwrap(), &[]).unwrap(),
                    entry,
                )
                .unwrap();
                let preparation_reads = if name == "schema-inheritance" { 3 } else { 1 };
                assert_eq!(prepared.capture_reads(), preparation_reads, "{name}");
                let mut expected: Value = yamaa_adapters::json_wire::decode_slice(
                    &fs::read(
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join(format!("tests/fixtures/specs/{name}.json")),
                    )
                    .unwrap(),
                )
                .unwrap();
                expected["artifacts"] = json!([]);
                let expected_reads = expected["source_reads"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|read| read["snapshots_created"].as_u64().unwrap_or(0) as usize)
                    .sum::<usize>();
                let mut run_scope = prepared.run_scope(None);
                for repeated in [false, true] {
                    if repeated {
                        for read in expected["source_reads"].as_array_mut().unwrap() {
                            if !read["snapshots_created"].is_null() {
                                read["snapshots_created"] = json!(0);
                            }
                        }
                    }
                    let attempt = prepared.build(&mut run_scope, Scope::default());
                    let result = specification_report::build_result(
                        prepared.run(),
                        &attempt,
                        Identity {
                            runtime: "python",
                            runtime_version: "fixture-runtime",
                            engine_version: "fixture-engine",
                            example: name,
                            specification: entry,
                            base_directory: ".",
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        result.observations(),
                        expected,
                        "{name} repeated={repeated}"
                    );
                    assert_eq!(
                        run_scope.capture_reads(),
                        preparation_reads + expected_reads,
                        "{name}"
                    );
                }
            }
        }

        const DICTIONARY_SPEC: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, X, Y]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: X, type: str, derivation: {mapping: {source: SRC.CODE, dict: dict.yaml}}}\n  - {name: Y, type: str, derivation: {mapping: {source: SRC.CODE, dict: ./dict.yaml}}}\n";
        #[test]
        fn dictionary_resources_are_held_once_and_original_authored_model_is_retained() {
            let study = Study::new();
            fs::write(study.0.join("entry/dict.yaml"), "A: answer\n").unwrap();
            let prepared = study.prepare(DICTIONARY_SPEC).unwrap();
            assert_eq!(prepared.capture_reads(), 2);
            assert_eq!(prepared.run().document().dictionary_sources().count(), 1);
            assert_eq!(
                prepared
                    .run()
                    .document()
                    .dictionary_sources()
                    .next()
                    .unwrap()
                    .bytes,
                b"A: answer\n"
            );
            let document = prepared.run().document().model().document();
            let columns = document.field(document.root(), "columns").unwrap();
            let yamaa_core::schema::DocumentNode::Sequence(columns) = &document.nodes()[columns]
            else {
                panic!()
            };
            let column = columns[1];
            let derivation = document.field(column, "derivation").unwrap();
            let value = document.field(derivation, "value").unwrap();
            let mapping = document.field(value, "mapping").unwrap();
            let dict = document.field(mapping, "dict").unwrap();
            assert_eq!(
                document.nodes()[dict],
                yamaa_core::schema::DocumentNode::Text("dict.yaml".into())
            );
            fs::write(study.0.join("entry/dict.yaml"), b"A: changed\n").unwrap();
            let response = prepared.run().execute_csv(b"ID,CODE\n1,A\n").unwrap();
            assert!(response.table.is_some());
            let outcome: Value = yamaa_adapters::json_wire::decode(
                &ipc_snapshot::table_snapshot(response.table.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(
                outcome["rows"],
                json!([[{"int":"1"},{"str":"answer"},{"str":"answer"}]])
            );
            assert_eq!(prepared.capture_reads(), 2);
            assert!(!study.0.join("entry/output.csv").exists());
        }
        #[test]
        fn inherited_dictionary_resources_start_at_original_declaring_layer() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"), DICTIONARY_SPEC).unwrap();
            fs::write(study.0.join("parent/dict.yaml"), "A: declared-parent\n").unwrap();
            fs::write(study.0.join("entry/dict.yaml"), "A: wrong-entry\n").unwrap();
            let prepared = study
                .prepare("schema_version: '1.0'\nparent: ../parent/base.yaml\n")
                .unwrap();
            assert_eq!(prepared.capture_reads(), 3);
            let dictionary = prepared
                .run()
                .document()
                .dictionary_sources()
                .next()
                .unwrap();
            assert_eq!(dictionary.bytes, b"A: declared-parent\n");
            assert!(dictionary.identity.ends_with("/parent/dict.yaml"));
            let response = prepared.run().execute_csv(b"ID,CODE\n1,A\n").unwrap();
            let outcome: Value = yamaa_adapters::json_wire::decode(
                &ipc_snapshot::table_snapshot(response.table.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(
                outcome["rows"],
                json!([[{"int":"1"},{"str":"declared-parent"},{"str":"declared-parent"}]])
            );
            assert!(!study.0.join("parent/input.csv").exists());
            assert!(!study.0.join("entry/output.csv").exists());
        }
        #[test]
        fn dictionary_missing_and_directory_failures_name_authored_field_without_host_identity() {
            let study = Study::new();
            for (path, condition) in [
                ("absent.yaml", "resource_path_missing"),
                (".", "resource_path_not_regular_file"),
            ] {
                let actual: Value = yamaa_adapters::json_wire::decode(&failure(
                    &study,
                    &DICTIONARY_SPEC.replace("dict.yaml", path),
                ))
                .unwrap();
                assert_eq!(
                    actual,
                    json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":condition,"requirement":"REQ-0785","spec_paths":["columns.X.derivation.mapping.dict"],"context":{"path":path}}]}})
                );
            }
        }
        #[test]
        fn dictionary_invalid_content_retains_original_typed_schema_context_before_study() {
            let study = Study::new();
            fs::write(study.0.join("entry/dict.yaml"), "7: answer\n").unwrap();
            let actual: Value =
                yamaa_adapters::json_wire::decode(&failure(&study, DICTIONARY_SPEC)).unwrap();
            let diagnostics = actual["outcome"]["diagnostics"].as_array().unwrap();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0]["condition"], "invalid_field_type");
            assert_eq!(
                diagnostics[0]["spec_paths"],
                json!(["columns.X.derivation.mapping.dict.key(7)"])
            );
            assert_eq!(
                diagnostics[0]["context"],
                json!({"expected":"str","actual":"int"})
            );
            assert!(!study.0.join("entry/input.csv").exists());
        }
        #[cfg(unix)]
        #[test]
        fn dictionary_symlink_policy_retains_original_resource_error() {
            let study = Study::new();
            fs::write(study.0.join("parent/dict.yaml"), "A: answer\n").unwrap();
            std::os::unix::fs::symlink(
                study.0.join("parent/dict.yaml"),
                study.0.join("entry/dict.yaml"),
            )
            .unwrap();
            let error = match study.prepare(DICTIONARY_SPEC) {
                Ok(_) => panic!(),
                Err(error) => error,
            };
            assert_eq!(
                error.into_message(),
                "resource path contains a symbolic link"
            );
        }
    }
}
