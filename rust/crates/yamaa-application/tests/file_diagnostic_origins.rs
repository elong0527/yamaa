#![cfg(feature = "yaml")]
//! File contracts: diagnostic origins, file application, file entry and file
//! preparation. The cases a study directory states, with one call and its
//! issues or saved files, are `tests/fixtures/diagnostics/file_origins.json`'s
//! (#3778, #3798); the tests left here prove the seams their first comments
//! name.
mod file {
    mod diagnostic_origins {
        //! Native index paths resolve only through the exact held composed member IDs.
        use std::fs;
        use yamaa_adapters::{file_resources::Resources, file_workflow};
        use yamaa_core::project_function::Language;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let directory = crate::support::Scratch::canonical("diagnostic-origins");
                fs::create_dir_all(directory.join("parent")).unwrap();
                // A minimal project, which declares no function.
                fs::write(
                    directory.join("project.yaml"),
                    "yamaa: \"1.0\"\ntype: project\n",
                )
                .unwrap();
                Self(directory)
            }
            fn identity(&self, name: &str) -> String {
                let value = self.0.join(name).to_string_lossy().into_owned();
                #[cfg(windows)]
                let value = value.replace('\\', "/");
                value
            }
        }

        /// Seam: the prepared owner's declaring source and capture counter,
        /// which a case's rows do not show.
        #[test]
        fn ready_static_owner_returns_contributor_without_changing_public_issue_context() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"),
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - name: ID\n    type: str\n    label: Identifier\n    derivation: SRC.ID\n    verifications: [{matches: {pattern: '('}}]\n").unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                "yamaa: \"1.0\"\ntype: spec\nparent: parent/base.yaml\n",
            )
            .unwrap();
            let owner = file_workflow::select_with_optional_resources(
                Resources::for_entry(study.0.to_str().unwrap()).unwrap(),
                "spec.yaml",
                None,
                Some(Language::Python),
                yamaa_adapters::document::Closures::shipped().unwrap(),
            )
            .unwrap();
            let prepared = &owner;
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
            let issues = yamaa_adapters::specification_check::issues(prepared.run());
            assert_eq!(issues.len(), 1);
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&issues[0].context),
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
        use crate::support::issues::assert_issues;
        use crate::support::Edit as _;
        use std::fs;
        #[cfg(unix)]
        use yamaa_adapters::file_resources::Error;
        use yamaa_adapters::{
            file_application, file_configuration, file_preparation::FileSpecification,
            specification_report::Identity,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::domain_entry::Request;
        const VALID: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("file-application");
                Self(path)
            }
            fn text(&self, path: &str) -> String {
                let path = self.0.join(path).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            /// Write a minimal project, which declares no function, and return
            /// the project path a caller passes.
            fn project(&self) -> String {
                fs::write(
                    self.0.join("project.yaml"),
                    "yamaa: \"1.0\"\ntype: project\n",
                )
                .unwrap();
                self.text("project.yaml")
            }
            fn build(&self) -> file_application::Domain {
                let path = self.text("spec.yaml");
                crate::support::study::domain(
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
        fn identity(specification: &str) -> Identity<'_> {
            crate::support::study::identity("test", specification)
        }
        /// Seam: an input changed between preparation and the build, with the
        /// held snapshot and the run scope's capture counter.
        #[test]
        fn changed_source_retains_original_snapshot_and_ingest_issue_and_refuses_both_saves() {
            use yamaa_adapters::{
                file_application::Domain, file_preparation, specification_report,
                specification_run::PortError,
            };
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
            let attempt = prepared.build(
                &mut run_scope,
                &mut crate::support::ports::NoFunctions,
                Scope::default(),
            );
            assert!(matches!(
                &attempt.dataset.result,
                Err(PortError::Capture(
                    yamaa_adapters::file_resources::Error::Changed
                ))
            ));
            assert!(file_preparation::opaque_resource_failure(&attempt.dataset).is_none());
            assert_eq!(run_scope.capture_reads(), reads);
            assert_eq!(original.as_ref(), b"ID\n1\n");
            let result =
                specification_report::build_result(prepared.run(), &attempt, identity(&path), &[])
                    .unwrap();
            assert!(result.output().is_none());
            assert_eq!(result.issues().len(), 1);
            let finding = &result.issues()[0];
            assert_eq!(finding.phase(), "ingest");
            assert_eq!(finding.condition(), "resource_path_content_changed");
            assert_eq!(finding.rule(), Some("FILE-10"));
            assert_eq!(finding.spec_paths, ["input.SRC.path"]);
            assert_eq!(
                yamaa_core::json::Value::Object(finding.context.clone()),
                yamaa_adapters::json!({"dataset":"SRC","path":"input.csv"})
            );
            let mut domain = Domain::new(result, &prepared, &run_scope);
            for _ in 0..2 {
                assert!(domain.save().is_err());
            }
            assert_eq!(
                fs::read(study.0.join("output.csv")).unwrap(),
                b"prior saved output"
            );
        }

        /// Seam: files changed after the build, a save retried as its directory
        /// is made and removed, and the observation record.
        #[test]
        fn saved_bytes_use_retained_values_and_save_failure_can_be_retried() {
            let study = Study::new();
            fs::write(
                study.0.join("spec.yaml"),
                VALID.edit("path: output.csv", "path: absent/output.csv"),
            )
            .unwrap();
            fs::write(
                study.0.join("input.csv"),
                b"ID\n-9223372036854775808\n9223372036854775807\n",
            )
            .unwrap();
            let mut result = study.build();
            let before = result.output().unwrap().clone();
            let observations = result.observations();
            assert_eq!(observations["artifacts"], yamaa_adapters::json!([]));
            assert!(result.issues().is_empty());
            fs::write(study.0.join("input.csv"), b"changed").unwrap();
            fs::write(study.0.join("spec.yaml"), b"[changed").unwrap();
            assert!(!result.save().unwrap());
            assert_eq!(result.observations(), observations);
            // A missing directory is refused at the field that declares the
            // file, naming it as written (FILE-7, #3030 D189).
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].condition(), "resource_path_missing");
            assert_eq!(result.issues()[0].phase(), "output");
            assert_eq!(result.issues()[0].spec_paths, ["output.path"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&result.issues()[0].context),
                "{\"path\":\"absent/output.csv\"}"
            );
            assert_eq!(result.output().unwrap(), &before);
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
        /// Seam: capture counters across builds, with files changed between
        /// them.
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
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_err());
            assert_eq!(run_scope.capture_reads(), 1);
            fs::write(study.0.join("entry/input.csv"), b"ID\n1\n").unwrap();
            fs::write(study.0.join("yamaa-project.yaml"), b"[changed").unwrap();
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }
        /// Seam: absolute paths of the study directory, and an input changed
        /// and the entry removed between the build and its save.
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
                    VALID.edit("input.csv", &input).edit("output.csv", &output),
                )
                .unwrap();
                let path = study.text("entry/spec.yaml");
                let mut result = crate::support::study::domain(
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
        /// Seam: symbolic links, which a case does not stage, and a linked
        /// entry's typed preparation error.
        #[cfg(unix)]
        #[test]
        fn entry_and_outside_resource_links_remain_refused() {
            let study = Study::new();
            fs::create_dir(study.0.join("entry")).unwrap();
            fs::write(study.0.join("outside.csv"), b"ID\n1\n").unwrap();
            std::os::unix::fs::symlink("outside.csv", study.0.join("outside-link.csv")).unwrap();
            fs::write(
                study.0.join("entry/spec.yaml"),
                VALID.edit("input.csv", "../outside-link.csv"),
            )
            .unwrap();
            let path = study.text("entry/spec.yaml");
            let mut result = crate::support::study::domain(
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
            // FILE-8 classifies an authored study-source link before any byte read.
            assert_eq!(issues[0].condition(), "resource_path_symlink");
            assert_eq!(issues[0].rule(), Some("FILE-8"));
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&issues[0].context),
                "{\"dataset\":\"SRC\",\"path\":\"../outside-link.csv\"}"
            );
            std::os::unix::fs::symlink("spec.yaml", study.0.join("entry/link.yaml")).unwrap();
            let (resources, entry) =
                file_configuration::resources(&study.text("entry/link.yaml")).unwrap();
            assert!(matches!(
                FileSpecification::prepare(resources, &entry),
                Err(yamaa_adapters::file_preparation::Error::Entry(
                    Error::Symlink
                ))
            ));
        }
        /// Seam: the resources' and the preparation's capture counters.
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

        /// Seam: a link to a directory outside the study, replaced by a
        /// directory between two saves.
        #[cfg(unix)]
        #[test]
        fn publication_refuses_linked_parents_for_existing_and_absent_targets() {
            for existing in [false, true] {
                let study = Study::new();
                let outside = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    VALID.edit("output.csv", "link/output.csv"),
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
                assert_eq!(result.issues()[0].condition(), "resource_path_symlink");
                assert_eq!(result.issues()[0].spec_paths, ["output.path"]);
                assert_eq!(
                    yamaa_adapters::json_wire::encode_object(&result.issues()[0].context),
                    "{\"path\":\"link/output.csv\"}"
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

        /// Seam: a parent directory replaced by a link between the build and
        /// its save.
        #[cfg(unix)]
        #[test]
        fn publication_refuses_parent_link_substitution_after_build() {
            let study = Study::new();
            let outside = Study::new();
            fs::create_dir(study.0.join("parent")).unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                VALID.edit("output.csv", "parent/output.csv"),
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

        /// Seam: the selected root renamed and replaced by a link between the
        /// build and its save.
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
            let mut result = crate::support::study::domain(
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

        /// Seam: specifications generated at and over each compiler capacity.
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
                ("group_keys", 64, 65, "rows.r.group_by"),
                // No row_columns case: after pruning (LAYER-11) a row's
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
                                "  - {{name: C{i}, type: int, derivation: 1}}\n"
                            ));
                        }
                        // The generated columns must be live under pruning
                        // (LAYER-11) for the resolved-column capacity to be
                        // what the compiler refuses.
                        let projected = (1..actual)
                            .map(|i| format!("C{i}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        body = body.edit(
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
                        // (LAYER-11) discards it before the inputs capacity
                        // is counted.
                        body = body
                            .edit("input: {SRC: input.csv}", &format!("input: {{{inputs}}}"))
                            .edit(
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
                        body = body.edit(
                            "input: {SRC: input.csv}",
                            &format!("input: {{SRC: {{path: input.csv, types: {{{fields}}}}}}}"),
                        );
                    }
                    "projected_columns" => {
                        body = body.edit(
                            "columns: [ID]",
                            &format!("columns: [{}]", vec!["ID"; actual].join(", ")),
                        );
                    }
                    "source_operands" => {
                        body = body.edit(
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
                        crate::support::study::domain(
                            yamaa_core::project_function::Language::Python,
                            request(),
                            identity(&spec),
                        )
                        .err()
                        .expect("refused before execution"),
                    ] {
                        assert_eq!(issues.len(), 1, "{resource}, {environment:?}");
                        let issue = &issues[0];
                        assert_eq!(issue.phase(), "engine");
                        assert_eq!(issue.condition(), "engine_rejected");
                        assert_eq!(issue.rule(), None);
                        assert_eq!(issue.spec_paths, [path]);
                        assert_eq!(
                            yamaa_core::json::Value::Object(issue.context.clone()),
                            json!({
                                "step":"prepare", "code":"compiler_limit", "resource":resource, "limit":limit, "actual":actual,
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
                    let mut domain = crate::support::study::domain(
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

        /// Seam: specifications generated at the key and column limits, and the
        /// project service's build through a host-port double.
        #[test]
        fn row_shape_boundaries_are_per_template_and_preserve_authored_preflight_precedence() {
            use yamaa_adapters::json;
            use yamaa_core::limits::Limits;
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
                    crate::support::study::domain(
                        yamaa_core::project_function::Language::Python,
                        request(),
                        identity(&spec),
                    )
                    .err()
                    .unwrap(),
                ] {
                    assert_issues(
                        &issues,
                        &[
                            json!({"phase":"validation","condition":"invalid_field_type","rule":"ROW-4","spec_paths":["rows.r.group_by"],"context":{"row":"r","group_by":vec!["ID"; Limits::KEYS],"expected":"at least one driver variable, each once"}}),
                        ],
                        "a group_by at the key limit, on both routes",
                    );
                }
            }
            assert!(!study.0.join("input.csv").exists());
            assert!(!study.0.join("output.csv").exists());

            let groups = (0..Limits::KEYS)
                .map(|i| format!("SRC.G{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut body = String::from("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int}\n");
            for i in 1..Limits::COLUMNS {
                body.push_str(&format!("  - {{name: C{i}, type: int}}\n"));
            }
            body.push_str("rows:\n");
            for row in 1..=2 {
                let derivations = std::iter::once(format!("ID: {row}"))
                    .chain((1..Limits::COLUMNS).map(|i| format!("C{i}: 1")))
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
            let mut domain = crate::support::study::domain(
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
            let project = yamaa_adapters::project_application::prepare(
                &spec,
                &project_file,
                yamaa_core::project_function::Language::Python,
            )
            .unwrap();
            let attempt = project.build(
                &mut project.run_scope(None),
                &mut crate::support::ports::NoFunctions,
                Scope::default(),
            );
            assert!(attempt.boundary.is_ok());
            let result = yamaa_adapters::specification_report::build_result(
                project.run(),
                &attempt,
                identity(&spec),
                &[],
            )
            .unwrap();
            assert!(result.issues().is_empty());
            let table = result.output().unwrap();
            assert_eq!(table.names().collect::<Vec<_>>(), ["ID"]);
            let values = [Some(1), Some(2)].into_iter().collect();
            let expected = yamaa_adapters::public_table::PublicColumn::Int(values);
            assert_eq!(table.column(0), &expected);
            assert_eq!(result.files(), [("output.csv", b"ID\n1\n2\n".as_slice())]);
        }

        /// Seam: a chain of 65 generated layers.
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
                    format!("yamaa: \"1.0\"\ntype: spec\nparent: {parent}\n"),
                )
                .unwrap();
            }
            fs::write(
                study.0.join("spec.yaml"),
                "yamaa: \"1.0\"\ntype: spec\nparent: layer-0.yaml\n",
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
            let mut result = crate::support::study::domain(
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
    }
    mod file_entry {
        //! Entry capture reads each `schema`-linked producing specification once,
        //! as metadata. No producer is compiled, executed or followed further.
        use crate::support::Edit as _;
        use std::{collections::BTreeMap, fs, sync::Arc};
        use yamaa_adapters::{
            file_entry::{Error, RejectedEntry},
            file_preparation::{Error as DocumentError, FileSpecification, Rejected},
            file_resources::{Error as ResourceError, Resources},
            file_workflow::{self, SelectionError},
            host_resources::{Failure, MemoryResolver, Operation, Reply, Request, Resolver},
            project_application,
            resolved_artifact::model::{Location, ResourceRole},
        };
        use yamaa_core::{
            project_function::Language, specification::PrepareError, value::ColumnType,
        };

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let root = crate::support::Scratch::canonical("native-entry");
                Self(root)
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
        fn select(
            resources: Resources,
        ) -> Result<yamaa_adapters::file_workflow::Selected, SelectionError> {
            file_workflow::select_with_optional_resources(
                resources,
                "root.yaml",
                None,
                Some(Language::Python),
                yamaa_adapters::document::Closures::shipped().unwrap(),
            )
        }
        fn plain(
            result: Result<yamaa_adapters::file_workflow::Selected, SelectionError>,
        ) -> FileSpecification {
            match result {
                Ok(prepared) => prepared,
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
        ) -> Box<Rejected> {
            match result {
                Err(SelectionError::Compile(error)) => error,
                Ok(_) => panic!("expected a compilation rejection"),
                Err(error) => panic!("expected a compilation rejection: {error:?}"),
            }
        }
        fn yaml(input: &str, base: &str, output: &str) -> String {
            format!("yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - {{name: VALUE, type: float, label: Reported value, derivation: {base}.VALUE}}\noutput: {{path: {output}, columns: [ID, VALUE], decimals: 2}}\n")
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

        /// Seam: the capture counter, the compiled sources and the resolved
        /// artifact's producer-schema locations.
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
            // VALUE reads all three linked datasets so pruning (LAYER-11) keeps
            // every link in the resolved model.
            let root = yaml("{RIGHT: {path: right.csv, schema: right.yaml}, LEFT: {path: left.csv, schema: left.yaml}, AGAIN: {path: left.csv, schema: './left.yaml'}}", "RIGHT", "root.csv")
                .edit(
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

        /// Seam: the resolved artifact's resources: each one's declaring
        /// source, written path and location.
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
                &yaml("{P: ../data/p.csv}", "P", "root.csv")
                    .edit("type: spec\n", "type: spec\nparent: schema.yaml\n"),
            );
            study.write(
                "producer/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "elsewhere.csv"),
            );
            study.write(
                "producer/p.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ndomain: PRODUCER\n",
            );
            study.write(
                "root.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: origins/path.yaml\ndomain: ROOT\n",
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

        /// Seam: the typed entry failure: its resource error, origin, retained
        /// document, entry and capture counter.
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

        /// Seam: the failure's captured source bytes, kept after the file is
        /// removed.
        #[test]
        fn malformed_entry_and_failed_parent_raw_bytes_survive_without_a_prepared_document() {
            for parent in [false, true] {
                let study = Study::new();
                let malformed = "yamaa: \"1.0\"\ntype: spec\ndomain: [\n";
                let bad = if parent { "broken.yaml" } else { "root.yaml" };
                study.write(bad, malformed);
                if parent {
                    study.write(
                        "root.yaml",
                        "yamaa: \"1.0\"\ntype: spec\nparent: broken.yaml\ndomain: ROOT\n",
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

        /// Seam: resources with a metadata fallback root, which no operation of
        /// the replay selects, and the compiled source path.
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

        /// Seam: metadata captured, then changed before selection, and the
        /// typed failure and snapshot it keeps.
        #[test]
        fn already_captured_changed_metadata_keeps_original_snapshot_and_native_failure() {
            let study = Study::new();
            let before = yaml("{RAW: never-read.csv}", "RAW", "root.csv");
            study.write("root.yaml", &before);
            let mut resources = study.resources();
            resources.capture_metadata("root.yaml").unwrap();
            study.write("root.yaml", &before.edit("TEST", "NEXT"));
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
            assert_eq!(diagnostic.definition().rule, Some("FILE-10"));
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

        /// Seam: a selection without a project, its capture counter and its
        /// typed compilation rejection.
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
                    let PrepareError::Invalid(findings) = &failure.run().rejection.error else {
                        panic!("consumer compilation: {:?}", failure.run().rejection.error);
                    };
                    // Selected without a project, the consumer's call has no
                    // project to bind it (CT-13, #3030 D146).
                    assert!(findings.iter().any(|finding| finding
                        .diagnostic()
                        .definition()
                        .condition
                        == "project_binding_missing"));
                    assert_eq!(failure.capture_reads(), 2);
                }
                assert!(!study.0.join("never-read.csv").exists());
                assert!(!study.0.join("leaf.csv").exists());
                assert!(!study.0.join("root.csv").exists());
            }
        }

        /// Seam: the capture counter after a producer link is read, beside the
        /// rejection's first finding.
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
            let PrepareError::Invalid(findings) = &failure.run().rejection.error else {
                panic!("original redundant authority");
            };
            let finding = findings[0].diagnostic();
            assert_eq!(finding.definition().rule, Some("INPUT-6"));
            assert_eq!(finding.spec_paths, ["input.P.types.ID"]);
            assert_eq!(failure.capture_reads(), 2);
        }

        /// Seam: the capture counter of a refused self link, beside the
        /// rejection's first finding.
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
            let PrepareError::Invalid(findings) = &failure.run().rejection.error else {
                panic!("self link: {:?}", failure.run().rejection.error);
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

        /// Seam: symbolic links, which a case does not stage, and the typed
        /// resource errors.
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

        /// Seam: a Unix directory name holding a backslash.
        #[cfg(unix)]
        #[test]
        fn canonical_unix_root_with_literal_backslash_remains_an_identity() {
            let outer = Study::new();
            let root = outer.0.join("named\\root");
            fs::create_dir(&root).unwrap();
            let study = Study(crate::support::Scratch::adopt(root.clone()));
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

        /// A caller's resolver over a memory store, whose references are not
        /// the files' locations. It refuses with `failure` the capture of
        /// `stale` that holds a reference: the check of that file's snapshot.
        struct Changing {
            store: MemoryResolver,
            stale: &'static str,
            failure: Failure,
        }
        impl Resolver for Changing {
            fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
                if request.operation == Operation::Capture
                    && request.written == self.stale
                    && request.reference.is_some()
                {
                    return Err(self.failure);
                }
                self.store.request(request)
            }
        }

        /// Seam: a caller's resolver double that refuses a snapshot's
        /// verification.
        ///
        /// A file that changed or disappeared after it was read, found when
        /// the entry's snapshots are verified before selection, fails with
        /// its own condition at the field that declared it, the path as
        /// written (FILE-10, FILE-7), through a caller's resolver whose
        /// references are not the files' locations: the project file at
        /// `environment` and the entry at `specification`.
        #[test]
        fn a_file_changed_before_the_entry_is_verified_fails_at_its_field() {
            let changed = |field: &str, path: &str| {
                yamaa_adapters::json!({
                    "phase": "ingest",
                    "condition": "resource_path_content_changed",
                    "rule": "FILE-10",
                    "spec_paths": [field],
                    "context": {"path": path},
                })
            };
            for (stale, failure, expected) in [
                (
                    "project.yaml",
                    Failure::Changed,
                    changed("environment", "project.yaml"),
                ),
                (
                    "spec.yaml",
                    Failure::Changed,
                    changed("specification", "spec.yaml"),
                ),
                (
                    "project.yaml",
                    Failure::Missing,
                    yamaa_adapters::json!({
                        "phase": "validation",
                        "condition": "resource_path_missing",
                        "rule": "FILE-7",
                        "spec_paths": ["environment"],
                        "context": {"path": "project.yaml"},
                    }),
                ),
            ] {
                let store = MemoryResolver::default();
                store.insert(
                    "/",
                    "spec.yaml",
                    "spec:1",
                    Arc::from(yaml("{RAW: raw.csv}", "RAW", "out.csv").as_bytes()),
                );
                store.insert(
                    "/",
                    "project.yaml",
                    "project:1",
                    Arc::from(&b"yamaa: \"1.0\"\ntype: project\n"[..]),
                );
                let Err(error) = file_workflow::select_with_resolver(
                    yamaa_core::build::Request {
                        specification: "spec.yaml",
                        environment: Some("project.yaml"),
                    },
                    Some(Language::Python),
                    Arc::new(Changing {
                        store,
                        stale,
                        failure,
                    }),
                ) else {
                    panic!("{stale} {failure:?}: the file was selected");
                };
                assert_eq!(
                    crate::support::rows(&project_application::selection_failure(&error)),
                    [expected],
                    "{stale} {failure:?}"
                );
            }
        }
    }
    mod file_preparation {
        use crate::support::table_snapshot;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::json;
        use yamaa_adapters::{
            file_preparation::{Error, FileSpecification},
            file_resources::Resources,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::json::Value;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("file-preparation");
                for name in ["entry", "parent", "outside"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                Self(path)
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
        const VALID: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        /// The report rows of a refused preparation's language findings.
        fn failure(study: &Study, source: &str) -> Value {
            match study.prepare(source) {
                Ok(_) => panic!("expected preparation failure"),
                Err(error) => crate::support::preparation_findings(error),
            }
        }
        /// Seam: the capture counters, the retained entry bytes, and files
        /// changed between preparation and the builds.
        #[test]
        fn standalone_retains_entry_and_checks_without_study_capture() {
            let study = Study::new();
            let prepared = study.prepare(VALID).unwrap();
            assert_eq!(prepared.capture_reads(), 1);
            assert_eq!(prepared.run().document().source().bytes, VALID.as_bytes());
            assert!(yamaa_adapters::specification_check::issues(prepared.run()).is_empty());
            assert_eq!(prepared.capture_reads(), 1);
            // Compilation uses the retained original model rather than reopening entry YAML.
            fs::write(study.0.join("entry/spec.yaml"), b"[changed").unwrap();
            fs::write(study.0.join("entry/input.csv"), b"ID\n1\n").unwrap();
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }

        /// Seam: a Unix directory name holding a backslash.
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
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 2);
        }
        /// Seam: the prepared document's parents and rebased source path, and
        /// the capture counters.
        #[test]
        fn inherited_paths_rebase_lexically_and_parent_bytes_are_retained_once() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"), VALID).unwrap();
            let prepared = study
                .prepare("yamaa: \"1.0\"\ntype: spec\nparent: ../parent/./base.yaml\n")
                .unwrap();
            assert_eq!(prepared.capture_reads(), 2);
            assert_eq!(prepared.run().document().parents().len(), 1);
            assert_eq!(
                prepared.run().document().parents()[0].source().bytes,
                VALID.as_bytes()
            );
            assert_eq!(prepared.run().source().path, "../parent/input.csv");
            // No data file was needed for model preparation or declaration-only checking.
            assert!(yamaa_adapters::specification_check::issues(prepared.run()).is_empty());
            fs::write(study.0.join("parent/input.csv"), b"ID\n1\n").unwrap();
            let mut run_scope = prepared.run_scope(None);
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 3);
            assert!(prepared
                .build(
                    &mut run_scope,
                    &mut crate::support::ports::NoFunctions,
                    Scope::default()
                )
                .dataset
                .result
                .is_ok());
            assert_eq!(run_scope.capture_reads(), 3);
        }
        /// Seam: a symbolic link, and resources rooted at the entry directory,
        /// which no operation of the replay selects.
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
            // A linked parent fails at `parent` as any linked file a run reads,
            // naming the entry that writes it (FILE-8, LAYER-2).
            let actual = failure(&study, &format!("{VALID}parent: link.yaml\n"));
            assert_eq!(
                actual,
                json!([{"phase":"validation","condition":"resource_path_symlink","rule":"FILE-8","spec_paths":["parent"],"context":{"path":"link.yaml","source":study.resource_key("entry/spec.yaml")}}])
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
            assert_eq!(
                crate::support::retained_resource_message(error),
                "resource path outside approved roots"
            );
        }

        /// Seam: a file whose permissions refuse reading.
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
            let actual = crate::support::preparation_findings(error);
            assert_eq!(
                actual,
                json!([{"phase":"validation","condition":"parent_not_found","rule":"LAYER-2","spec_paths":["parent"],"context":{"path":"blocked.yaml","source":study.resource_key("entry/spec.yaml")}}])
            );
        }
        /// Seam: benchmarks' own files against their authored observation
        /// records, with the run scope's capture counter.
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
                    let attempt = prepared.build(
                        &mut run_scope,
                        &mut crate::support::ports::NoFunctions,
                        Scope::default(),
                    );
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
                        &[],
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

        const DICTIONARY_SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, X, Y]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: X, type: str, derivation: {mapping: {source: SRC.CODE, dict: dict.yaml}}}\n  - {name: Y, type: str, derivation: {mapping: {source: SRC.CODE, dict: ./dict.yaml}}}\n";
        /// Seam: the held dictionary bytes and the authored document model,
        /// with the dictionary changed before an in-memory run.
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
            let document = prepared.run().document().document();
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
            assert_eq!(
                table_snapshot::rows(response.table.as_ref().unwrap()),
                json!([[{"int":"1"},{"str":"answer"},{"str":"answer"}]])
            );
            assert_eq!(prepared.capture_reads(), 2);
            assert!(!study.0.join("entry/output.csv").exists());
        }
        /// Seam: the held dictionary's identity and an in-memory run of the
        /// prepared specification, with the capture counter.
        #[test]
        fn inherited_dictionary_resources_start_at_original_declaring_layer() {
            let study = Study::new();
            fs::write(study.0.join("parent/base.yaml"), DICTIONARY_SPEC).unwrap();
            fs::write(study.0.join("parent/dict.yaml"), "A: declared-parent\n").unwrap();
            fs::write(study.0.join("entry/dict.yaml"), "A: wrong-entry\n").unwrap();
            let prepared = study
                .prepare("yamaa: \"1.0\"\ntype: spec\nparent: ../parent/base.yaml\n")
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
            assert_eq!(
                table_snapshot::rows(response.table.as_ref().unwrap()),
                json!([[{"int":"1"},{"str":"declared-parent"},{"str":"declared-parent"}]])
            );
            assert!(!study.0.join("parent/input.csv").exists());
            assert!(!study.0.join("entry/output.csv").exists());
        }
        /// Seam: a symbolic link, which a case does not stage.
        ///
        /// FILE-8: a linked dictionary fails at its authored field, as a
        /// missing one or a directory does under FILE-7.
        #[cfg(unix)]
        #[test]
        fn dictionary_symlink_failure_names_authored_field_without_host_identity() {
            let study = Study::new();
            fs::write(study.0.join("parent/dict.yaml"), "A: answer\n").unwrap();
            std::os::unix::fs::symlink(
                study.0.join("parent/dict.yaml"),
                study.0.join("entry/dict.yaml"),
            )
            .unwrap();
            let actual = failure(&study, DICTIONARY_SPEC);
            assert_eq!(
                actual,
                json!([{"phase":"validation","condition":"resource_path_symlink","rule":"FILE-8","spec_paths":["columns.X.derivation.mapping.dict"],"context":{"path":"dict.yaml"}}])
            );
        }
    }
}
