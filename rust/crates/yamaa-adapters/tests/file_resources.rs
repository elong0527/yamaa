//! File contracts.
#[allow(dead_code)]
mod support;
mod file {
    #[cfg(feature = "yaml")]
    mod local_paths {
        #[test]
        fn entry_relative_shared_program_uses_each_study_input_and_saves_held_artifacts() {
            use yamaa_adapters::{file_application, specification_report::Identity};
            use yamaa_core::domain_entry::Request;
            let study = Study::new();
            fs::write(study.0.join("shared/base.yaml"), "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: input/data.csv, relative_to: entry, types: {ID: int}}}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\noutput: {path: output/result.csv, relative_to: entry, columns: [ID], warning_log: output/warnings.csv, verification_log: output/verifications.csv}\n").unwrap();
            fs::create_dir(study.0.join("shared/input")).unwrap();
            fs::write(study.0.join("shared/input/data.csv"), b"ID\n999\n").unwrap();
            let mut results = Vec::new();
            for (name, bytes) in [("first", b"ID\n1\n"), ("second", b"ID\n2\n")] {
                fs::create_dir_all(study.0.join(name).join("input")).unwrap();
                fs::create_dir_all(study.0.join(name).join("output")).unwrap();
                fs::write(study.0.join(name).join("input/data.csv"), bytes).unwrap();
                fs::write(
                    study.0.join(name).join("spec.yaml"),
                    "schema_version: '1.0'\nparent: ../shared/base.yaml\ndomain: STUDY\n",
                )
                .unwrap();
                let path = study.text(&format!("{name}/spec.yaml"));
                let result = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: "test",
                        example: "test",
                        specification: &path,
                        base_directory: ".",
                    },
                )
                .unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(result.output().is_some());
                results.push((name, bytes, result));
            }
            fs::write(study.0.join("shared/base.yaml"), "[changed shared program").unwrap();
            for (name, _, _) in &results {
                fs::remove_file(study.0.join(name).join("input/data.csv")).unwrap();
                fs::remove_file(study.0.join(name).join("spec.yaml")).unwrap();
            }
            for (name, bytes, result) in &mut results {
                for _ in 0..2 {
                    assert!(result.save().unwrap());
                    assert_eq!(
                        fs::read(study.0.join(*name).join("output/result.csv")).unwrap(),
                        *bytes
                    );
                    assert!(study.0.join(*name).join("output/warnings.csv").is_file());
                    assert!(study
                        .0
                        .join(*name)
                        .join("output/verifications.csv")
                        .is_file());
                }
            }
            assert!(!study.0.join("shared/output").exists());
        }

        #[test]
        fn composed_relative_input_base_can_change_without_redeclaring_the_inherited_path() {
            let study = Study::new();
            fs::write(study.0.join("shared/base.yaml"), "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: data.csv, relative_to: entry}}\noutput: {path: result.csv, relative_to: entry, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n").unwrap();
            fs::write(study.0.join("entry/data.csv"), b"ID\n1\n").unwrap();
            fs::write(study.0.join("shared/data.csv"), b"ID\n9\n").unwrap();
            for (mode, expected) in [
                ("entry", "data.csv"),
                ("declaring", "../shared/data.csv"),
                ("null", "../shared/data.csv"),
            ] {
                fs::write(study.0.join("entry/spec.yaml"), format!("schema_version: '1.0'\nparent: ../shared/base.yaml\ninput: {{SRC: {{relative_to: {mode}}}}}\n")).unwrap();
                let (resources, entry) =
                    file_configuration::resources(&study.text("entry/spec.yaml")).unwrap();
                let mut prepared = FileSpecification::prepare(resources, &entry).unwrap();
                assert_eq!(prepared.run().source().path, expected);
                let origin = prepared
                    .run()
                    .document()
                    .written_input_origin("SRC", "path")
                    .unwrap();
                assert_eq!(origin.declaring_source, study.text("shared/base.yaml"));
                assert_eq!(
                    origin.base_source,
                    study.text(if mode == "entry" {
                        "entry/spec.yaml"
                    } else {
                        "shared/base.yaml"
                    })
                );
                assert!(prepared.build(Scope::default()).result.is_ok());
            }
        }

        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{
            file_configuration,
            file_preparation::FileSpecification,
            file_resources::{Error, Resources},
        };
        use yamaa_core::application_observer::Scope;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-local-paths-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                for name in ["entry", "shared", "outputs"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn text(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().replace('\\', "/")
            }
            fn resources(&self) -> Resources {
                Resources::for_entry(&self.text("entry")).unwrap()
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn complete_output_replacement_resets_the_base_for_primary_and_both_sidecars() {
            use yamaa_adapters::{file_application, specification_report::Identity};
            use yamaa_core::{domain_entry::Request, schema::DocumentNode};

            for inherited_base in ["", "relative_to: entry, "] {
                let study = Study::new();
                fs::write(study.0.join("shared/base.yaml"), format!("schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {{SRC: {{path: data.csv, types: {{ID: int}}}}}}\ncolumns: [{{name: ID, type: int, derivation: SRC.ID}}]\noutput: {{path: ignored.csv, {inherited_base}columns: [ID], warning_log: ignored-warning.csv, verification_log: ignored-verification.csv}}\n")).unwrap();
                fs::write(study.0.join("shared/replacement.yaml"), "schema_version: '1.0'\nparent: base.yaml\noutput: {path: result.csv, columns: [ID], warning_log: warnings.csv, verification_log: verifications.csv}\n").unwrap();
                fs::write(
                    study.0.join("entry/spec.yaml"),
                    "schema_version: '1.0'\nparent: ../shared/replacement.yaml\n",
                )
                .unwrap();
                fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
                let path = study.text("entry/spec.yaml");
                let (resources, entry) = file_configuration::resources(&path).unwrap();
                let prepared = FileSpecification::prepare(resources, &entry).unwrap();
                let document = prepared.run().document().model().document();
                let output = document.field(document.root(), "output").unwrap();
                for (field, written) in [
                    ("path", "result.csv"),
                    ("warning_log", "warnings.csv"),
                    ("verification_log", "verifications.csv"),
                ] {
                    let node = document.field(output, field).unwrap();
                    assert_eq!(
                        document.nodes()[node],
                        DocumentNode::Text(format!("../shared/{written}"))
                    );
                }
                let base = document.field(output, "relative_to").unwrap();
                assert_eq!(
                    document.nodes()[base],
                    DocumentNode::Text("declaring".into())
                );
                let origin = prepared.run().document().written_output_origin().unwrap();
                assert_eq!(
                    origin.declaring_source,
                    study.text("shared/replacement.yaml")
                );
                assert_eq!(origin.base_source, study.text("shared/replacement.yaml"));
                assert_eq!(origin.written, "result.csv");
                drop(prepared);

                let mut result = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: "test",
                        example: "test",
                        specification: &path,
                        base_directory: ".",
                    },
                )
                .unwrap();
                assert!(result.issues().is_empty(), "{:?}", result.issues());
                assert!(!study.0.join("shared/result.csv").exists());
                assert!(result.save().unwrap());
                assert_eq!(
                    fs::read(study.0.join("shared/result.csv")).unwrap(),
                    b"ID\n7\n"
                );
                for written in ["result.csv", "warnings.csv", "verifications.csv"] {
                    assert!(study.0.join("shared").join(written).is_file());
                    assert!(!study.0.join("entry").join(written).exists());
                }
                for directory in ["shared", "entry"] {
                    for written in [
                        "ignored.csv",
                        "ignored-warning.csv",
                        "ignored-verification.csv",
                    ] {
                        assert!(!study.0.join(directory).join(written).exists());
                    }
                }
            }
        }

        #[test]
        fn output_selector_or_path_alone_cannot_patch_an_inherited_output() {
            use yamaa_adapters::{file_application, specification_report::Identity};
            use yamaa_core::domain_entry::Request;

            let study = Study::new();
            fs::write(study.0.join("shared/base.yaml"), "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\noutput: {path: result.csv, relative_to: entry, columns: [ID], warning_log: warnings.csv, verification_log: verifications.csv}\n").unwrap();
            for (output, expected) in [
                (
                    "{relative_to: declaring}",
                    vec!["output.columns", "output.path"],
                ),
                ("{path: another.csv}", vec!["output.columns"]),
            ] {
                fs::write(
                    study.0.join("entry/spec.yaml"),
                    format!(
                        "schema_version: '1.0'\nparent: ../shared/base.yaml\noutput: {output}\n"
                    ),
                )
                .unwrap();
                let path = study.text("entry/spec.yaml");
                let issues = file_application::check(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: "test",
                        example: "test",
                        specification: &path,
                        base_directory: ".",
                    },
                );
                assert_eq!(issues.len(), expected.len(), "{issues:?}");
                assert!(issues
                    .iter()
                    .all(|issue| issue.condition == "missing_required_field"));
                let mut paths = issues
                    .iter()
                    .flat_map(|issue| issue.spec_paths.iter().map(String::as_str))
                    .collect::<Vec<_>>();
                paths.sort_unstable();
                assert_eq!(paths, expected);
            }
        }

        #[test]
        fn public_check_and_build_validate_written_paths_before_source_profiles() {
            use yamaa_adapters::{file_application, specification_report::Identity};
            use yamaa_core::domain_entry::Request;
            let study = Study::new();
            for (written, requirement) in [
                ("input\\x.csv", "REQ-0776"),
                ("C:\\input.csv", "REQ-0776"),
                ("input//x.csv", "REQ-0777"),
                ("input/x.csv/", "REQ-0777"),
            ] {
                for expanded in [false, true] {
                    let source = if expanded {
                        format!("{{path: '{written}'}}")
                    } else {
                        format!("'{written}'")
                    };
                    let spec = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: {source}}}\ncolumns: [{{name: ID, type: int, derivation: SRC.ID}}]\noutput: {{path: output.csv, columns: [ID]}}\n");
                    fs::write(study.0.join("entry/spec.yaml"), spec).unwrap();
                    let path = study.text("entry/spec.yaml");
                    let request = || Request {
                        specification: &path,
                        environment: None,
                    };
                    let identity = || Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: "test",
                        example: "test",
                        specification: &path,
                        base_directory: ".",
                    };
                    let checked = file_application::check(request(), identity());
                    let built = file_application::domain(request(), identity()).map_or_else(
                        |issues| issues,
                        |result| {
                            assert!(result.output().is_none());
                            result.issues()
                        },
                    );
                    for issues in [checked, built] {
                        assert_eq!(issues.len(), 1, "{written:?}, expanded={expanded}");
                        let issue = &issues[0];
                        assert_eq!(issue.phase, "validation");
                        assert_eq!(issue.condition, "resource_path_not_normalized");
                        assert_eq!(issue.requirement.as_deref(), Some(requirement));
                        assert_eq!(issue.spec_paths, ["input.SRC.path"]);
                        assert_eq!(
                            issue.context.clone(),
                            yamaa_adapters::json!({"dataset":"SRC","path":written})
                        );
                    }
                    assert!(!study.0.join("entry/output.csv").exists());
                }
            }
        }

        #[test]
        fn public_uri_and_symlink_sources_keep_their_normative_cause_and_authored_path() {
            use yamaa_adapters::{file_application, specification_report::Identity};
            use yamaa_core::domain_entry::Request;
            let study = Study::new();
            fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink("../shared/data.csv", study.0.join("entry/link.csv"))
                .unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(
                "../shared/data.csv",
                study.0.join("entry/link.csv"),
            )
            .unwrap();
            for (written, condition, requirement) in [
                (
                    "https://example.invalid/source.csv",
                    "resource_path_uri_scheme",
                    "REQ-0775",
                ),
                ("link.csv", "resource_path_symlink", "REQ-0783"),
            ] {
                let spec = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: '{written}'}}\ncolumns: [{{name: ID, type: int, derivation: SRC.ID}}]\noutput: {{path: output.csv, columns: [ID]}}\n");
                fs::write(study.0.join("entry/spec.yaml"), spec).unwrap();
                let path = study.text("entry/spec.yaml");
                let issues = file_application::domain(
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: "test",
                        example: "test",
                        specification: &path,
                        base_directory: ".",
                    },
                )
                .map_or_else(
                    |issues| issues,
                    |mut result| {
                        assert!(result.output().is_none());
                        assert!(result.save().is_err());
                        result.issues()
                    },
                );
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].phase, "validation");
                assert_eq!(issues[0].condition, condition);
                assert_eq!(issues[0].requirement.as_deref(), Some(requirement));
                assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
                assert_eq!(
                    issues[0].context.clone(),
                    yamaa_adapters::json!({"dataset":"SRC","path":written})
                );
                assert!(!study.0.join("entry/output.csv").exists());
            }
        }

        #[test]
        fn inherited_parent_keeps_its_own_input_and_output_locations_without_configuration() {
            let study = Study::new();
            let parent = "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\noutput: {path: ../outputs/out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let child = "schema_version: '1.0'\nparent: ../shared/base.yaml\ndomain: CHILD\n";
            fs::write(study.0.join("entry/child.yaml"), child).unwrap();
            fs::write(study.0.join("shared/base.yaml"), parent).unwrap();
            fs::write(study.0.join("entry/data.csv"), b"ID\n999\n").unwrap();
            fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
            fs::write(
                study.0.join("yamaa-project.yaml"),
                b"[invalid configuration",
            )
            .unwrap();
            let (resources, entry) =
                file_configuration::resources(&study.text("entry/child.yaml")).unwrap();
            assert_eq!(resources.capture_reads(), 0);
            let mut prepared = FileSpecification::prepare(resources, &entry).unwrap();
            let origin = prepared.run().document().written_output_origin().unwrap();
            assert_eq!(origin.declaring_source, study.text("shared/base.yaml"));
            assert_eq!(origin.written, "../outputs/out.csv");
            assert_eq!(prepared.run().source().path, "../shared/data.csv");
            assert_eq!(
                prepared.run().compiled().output_path(),
                "../outputs/out.csv"
            );
            assert_eq!(prepared.capture_reads(), 2);
            assert!(prepared.build(Scope::default()).result.is_ok());
            assert_eq!(prepared.capture_reads(), 3);
            assert!(!study.0.join("outputs/out.csv").exists());
        }

        #[test]
        fn parent_and_absolute_dot_aliases_share_bytes_and_detect_later_changes() {
            let study = Study::new();
            fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
            fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
            let mut resources = study.resources();
            let declaring = resources.resolve("root.yaml").unwrap();
            let relative = "../shared/data.csv";
            let absolute = study.text("shared/../shared/./data.csv");
            let (first, fresh) = resources.capture_from(&declaring, relative).unwrap();
            assert!(fresh);
            let (second, fresh) = resources.capture_from(&declaring, &absolute).unwrap();
            assert!(!fresh);
            assert!(std::sync::Arc::ptr_eq(&first, &second));
            assert_eq!(resources.capture_reads(), 1);
            assert_eq!(
                resources.resolve_from(&declaring, relative).unwrap(),
                study.text("shared/data.csv")
            );
            fs::write(study.0.join("shared/data.csv"), b"ID\n8\n").unwrap();
            assert_eq!(
                resources.capture_from(&declaring, relative).err(),
                Some(Error::Changed)
            );
            assert_eq!(first.as_ref(), b"ID\n7\n");
            assert_eq!(resources.capture_reads(), 1);
        }

        #[test]
        fn absent_artifact_location_has_no_fallback_and_nonregular_input_is_terminal() {
            let study = Study::new();
            fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
            fs::write(study.0.join("data.csv"), b"unrelated parent file").unwrap();
            let resources = study.resources();
            let declaring = resources.resolve("root.yaml").unwrap();
            assert_eq!(
                resources.inspect_from(&declaring, "data.csv"),
                Err(Error::Missing)
            );
            assert_eq!(
                resources
                    .location_from(&declaring, "../outputs/held.csv")
                    .unwrap(),
                study.text("outputs/held.csv")
            );
            assert!(!study.0.join("outputs/held.csv").exists());
            assert_eq!(
                resources.inspect_from(&declaring, "../shared"),
                Err(Error::NotRegularFile)
            );
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn outside_parent_and_file_links_remain_terminal() {
            let study = Study::new();
            fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
            fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink("shared", study.0.join("linked-directory")).unwrap();
                std::os::unix::fs::symlink("shared/data.csv", study.0.join("linked-file.csv"))
                    .unwrap();
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir("shared", study.0.join("linked-directory"))
                    .unwrap();
                std::os::windows::fs::symlink_file(
                    "shared/data.csv",
                    study.0.join("linked-file.csv"),
                )
                .unwrap();
            }
            let resources = study.resources();
            let declaring = resources.resolve("root.yaml").unwrap();
            for written in ["../linked-directory/data.csv", "../linked-file.csv"] {
                assert_eq!(
                    resources.inspect_from(&declaring, written),
                    Err(Error::Symlink)
                );
            }
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn local_path_admission_still_rejects_uri_backslash_and_empty_segments() {
            use yamaa_core::resource::ResourceFailure;
            let study = Study::new();
            fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
            let resources = study.resources();
            let declaring = resources.resolve("root.yaml").unwrap();
            for (written, failure) in [
                ("", ResourceFailure::EmptySegment),
                ("file:/data.csv", ResourceFailure::UriScheme),
                ("https://example.org/data.csv", ResourceFailure::UriScheme),
                ("../shared\\data.csv", ResourceFailure::Backslash),
                ("../shared//data.csv", ResourceFailure::EmptySegment),
                ("../shared/data.csv/", ResourceFailure::EmptySegment),
            ] {
                assert_eq!(
                    resources.inspect_from(&declaring, written),
                    Err(Error::WrittenForm(failure)),
                    "{written}"
                );
            }
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn metadata_refused_alias_cannot_become_a_snapshot_verification_dependency() {
            use yamaa_core::limits::Limits;
            let study = Study::new();
            let source = study.0.join("entry/source.csv");
            let alias = study.0.join("entry/alias.csv");
            let content = vec![b'x'; Limits::SPEC_BYTES + 1];
            fs::write(&source, &content).unwrap();
            fs::hard_link(&source, &alias).unwrap();
            let mut resources = study.resources();
            let (snapshot, created) = resources.capture("source.csv").unwrap();
            assert!(created);
            for name in ["source.csv", "alias.csv"] {
                assert_eq!(
                    resources.capture_metadata(name),
                    Err(Error::Limit(Limits::spec_bytes(content.len()).unwrap_err()))
                );
            }
            fs::remove_file(alias).unwrap();
            let (again, created) = resources.capture("source.csv").unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&snapshot, &again));
            assert_eq!(snapshot.as_ref(), content);
            assert_eq!(resources.capture_reads(), 1);
        }

        #[test]
        fn unicode_names_and_binary_snapshots_keep_exact_bytes_without_reopening() {
            let study = Study::new();
            let name = "donn\u{e9}es-\u{1f9ea}.csv";
            let content = b"ID\r\n001\x1a\x00\r\n";
            fs::write(study.0.join("entry").join(name), content).unwrap();
            let mut resources = study.resources();
            let (snapshot, created) = resources.capture(name).unwrap();
            assert!(created);
            assert_eq!(snapshot.as_ref(), content);
            let (again, created) = resources.capture(name).unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&snapshot, &again));
            assert_eq!(resources.capture_reads(), 1);
        }

        #[cfg(windows)]
        #[test]
        fn retired_python_junctions_reject_descent_and_keep_caller_selected_root_authority() {
            use std::os::windows::process::CommandExt;
            for name in ["store&data", "store & data"] {
                let study = Study::new();
                fs::create_dir(study.0.join(name)).unwrap();
                fs::write(study.0.join(name).join("dm.csv"), b"ID\n001\n").unwrap();
                let link = study.text("entry/link&data");
                let target = study.text(name);
                // Both operands are generated fixture paths, quoted as literal cmd
                // operands. Ampersands and spaces must never become shell commands.
                let output = std::process::Command::new("cmd")
                    .arg("/d")
                    .arg("/c")
                    .raw_arg(format!("mklink /J \"{link}\" \"{target}\""))
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                let mut resources = study.resources();
                assert_eq!(resources.capture("link&data/dm.csv"), Err(Error::Symlink));
                assert_eq!(resources.capture_reads(), 0);
                let mut selected = Resources::for_entry(&link).unwrap();
                let (bytes, created) = selected.capture("dm.csv").unwrap();
                assert!(created);
                assert_eq!(bytes.as_ref(), b"ID\n001\n");
                drop(selected);
                fs::remove_dir(study.0.join("entry/link&data")).unwrap();
            }
        }
    }
    #[cfg(feature = "yaml")]
    mod shared_define {
        //! Independent fixed-byte DM and structural AE/VS shared-generation witnesses.
        #[test]
        fn entry_relative_inherited_output_supplies_json_capture_and_publication_targets() {
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            fs::create_dir(temp.0.join("parents")).unwrap();
            fs::create_dir_all(temp.0.join("children/artifacts")).unwrap();
            let parent = fs::read_to_string(temp.0.join("spec.yaml"))
                .unwrap()
                .replace(
                    "path: dm.csv",
                    "path: artifacts/dm.csv\n  relative_to: entry",
                );
            fs::write(temp.0.join("parents/base.yaml"), parent).unwrap();
            fs::write(
                temp.0.join("children/spec.yaml"),
                "schema_version: '1.0'\nparent: ../parents/base.yaml\n",
            )
            .unwrap();
            let environment = fs::read_to_string(temp.0.join("environment.yaml"))
                .unwrap()
                .replace("  - spec.yaml", "  - children/spec.yaml")
                .replace(
                    "define: define.xml",
                    "define: children/artifacts/define.xml",
                );
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            fs::copy(
                root().join("benchmarks/sdtm-dm-metadata/expected/dm.csv"),
                temp.0.join("children/artifacts/dm.csv"),
            )
            .unwrap();
            let mut result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(std::str::from_utf8(result.artifacts().nth(1).unwrap().1)
                .unwrap()
                .contains("xlink:href=\"dm.json\""));
            assert!(result.save().unwrap());
            assert!(temp.0.join("children/artifacts/dm.json").is_file());
            assert!(!temp.0.join("parents/artifacts").exists());
        }

        use std::{
            fs,
            path::{Path, PathBuf},
            sync::atomic::{AtomicU64, Ordering},
        };
        use yamaa_adapters::file_define::{Define, PublicationState};
        use yamaa_core::project_function::Language;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        struct Temp(PathBuf);
        impl Temp {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-define-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
            }
            fn fixture(name: &str, published: bool) -> Self {
                let this = Self::new();
                let original = root().join("benchmarks").join(name);
                fs::copy(original.join("spec.yaml"), this.0.join("spec.yaml")).unwrap();
                if name == "sdtm-lb-metadata" {
                    fs::copy(
                        original.join("submission-spec.yaml"),
                        this.0.join("submission-spec.yaml"),
                    )
                    .unwrap();
                    fs::copy(
                        original.join("submission-environment.yaml"),
                        this.0.join("environment.yaml"),
                    )
                    .unwrap();
                    if published {
                        fs::copy(original.join("expected/lb.csv"), this.0.join("lb.csv")).unwrap();
                    }
                } else {
                    fs::copy(
                        original.join("environment.yaml"),
                        this.0.join("environment.yaml"),
                    )
                    .unwrap();
                }
                if published && name != "sdtm-lb-metadata" {
                    fs::copy(original.join("expected/dm.csv"), this.0.join("dm.csv")).unwrap();
                }
                this
            }
            fn run(&self, created: &str) -> Define {
                Define::prepare(
                    self.0.join("environment.yaml").to_str().unwrap(),
                    "sdtm",
                    Some(created),
                    "0.3.0",
                    Language::Python,
                )
            }
        }
        #[test]
        fn lb_value_metadata_uses_original_published_bytes_and_family_valid_completion() {
            let temp = Temp::fixture("sdtm-lb-metadata", true);
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let artifacts: Vec<_> = run
                .artifacts()
                .map(|(name, bytes, state)| (name.to_string(), bytes.to_vec(), state))
                .collect();
            assert_eq!(
                artifacts.iter().map(|a| a.0.as_str()).collect::<Vec<_>>(),
                ["lb.json", "define.xml"]
            );
            let xml = std::str::from_utf8(&artifacts[1].1).unwrap();
            assert_eq!(xml.matches("<def:ValueListDef ").count(), 1);
            assert_eq!(xml.matches("<def:WhereClauseDef ").count(), 2);
            assert_eq!(xml.matches("<ItemDef ").count(), 13);
            for (code, source, list) in [
                ("GLUC", "Vendor", "CL_GLUCOSE"),
                ("CREAT", "Sponsor", "CL_CREATININE"),
            ] {
                let begin = xml
                    .find(&format!("<ItemDef OID=\"IT.LB.LBORRESU.{code}\""))
                    .unwrap();
                let item = &xml[begin..begin + xml[begin..].find("</ItemDef>").unwrap()];
                // Every unit of both test codes is `mg/dL`, five bytes (SUB-24).
                assert!(item.contains("Length=\"5\""), "{item}");
                assert!(
                    item.contains(&format!("CodeListOID=\"CL.{list}\"")),
                    "{item}"
                );
                assert!(item.contains(&format!("Source=\"{source}\"")));
                assert!(xml.contains(&format!("<CheckValue>{code}</CheckValue>")));
            }
            assert!(!temp.0.join("input").exists());
            let reads = run.capture_reads();
            for file in ["spec.yaml", "submission-spec.yaml", "environment.yaml"] {
                fs::remove_file(temp.0.join(file)).unwrap();
            }
            fs::write(temp.0.join("lb.csv"), b"changed after preparation").unwrap();
            assert!(run.save().unwrap());
            assert_eq!(reads, run.capture_reads());
            for artifact in artifacts {
                assert_eq!(fs::read(temp.0.join(artifact.0)).unwrap(), artifact.1);
            }
        }
        #[test]
        fn original_lb_source_rejection_and_per_value_published_vocabulary_have_zero_writes() {
            let temp = Temp::fixture("sdtm-lb-metadata", true);
            let overlay = fs::read_to_string(temp.0.join("submission-spec.yaml")).unwrap();
            // Drop the completion's value_level block only, so the creatinine
            // entry inherits the original Investigator source.
            let mut stripped = String::new();
            let mut in_values = false;
            for line in overlay.split_inclusive('\n') {
                if line.trim_end() == "      value_level:" {
                    in_values = true;
                } else if !(in_values && line.starts_with("        ")) {
                    in_values = false;
                    stripped.push_str(line);
                }
            }
            assert!(!stripped.contains("value_level"));
            fs::write(temp.0.join("submission-spec.yaml"), stripped).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(
                run.issues()
                    .iter()
                    .any(|i| i.requirement.as_deref() == Some("REQ-0919")
                        && i.condition == "origin_source_not_admitted"),
                "{:?}",
                run.issues()
            );
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            assert!(!temp.0.join("lb.json").exists());
            let temp = Temp::fixture("sdtm-lb-metadata", true);
            let csv = fs::read_to_string(temp.0.join("lb.csv"))
                .unwrap()
                .replace("95,mg/dL,95", "95,mmol/L,95");
            fs::write(temp.0.join("lb.csv"), csv).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(
                run.issues()
                    .iter()
                    .any(|i| i.condition == "allowed_values_failed"
                        && yamaa_adapters::json_wire::encode(&i.context).contains("CL_GLUCOSE")
                        && yamaa_adapters::json_wire::encode(&i.context).contains("GLUC")),
                "{:?}",
                run.issues()
            );
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            assert!(!temp.0.join("lb.json").exists());
        }
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fn root() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .canonicalize()
                .unwrap()
        }
        #[test]
        fn dm_both_goldens_without_input_execution_and_save_uses_held_bytes() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let expected = root().join("benchmarks/sdtm-dm-metadata/expected");
            let artifacts: Vec<_> = run.artifacts().collect();
            assert_eq!(artifacts.len(), 2);
            assert_eq!(
                artifacts[0],
                (
                    "dm.json",
                    fs::read(expected.join("dm.json")).unwrap().as_slice(),
                    PublicationState::Ready
                )
            );
            assert_eq!(
                artifacts[1],
                (
                    "define.xml",
                    fs::read(expected.join("define.xml")).unwrap().as_slice(),
                    PublicationState::Ready
                )
            );
            assert!(!temp.0.join("input").exists());
            assert!(!temp.0.join("define.xml").exists());
            assert!(!temp.0.join("dm.json").exists());
            let reads = run.capture_reads();
            fs::remove_file(temp.0.join("spec.yaml")).unwrap();
            fs::remove_file(temp.0.join("environment.yaml")).unwrap();
            fs::write(temp.0.join("dm.csv"), b"changed after preparation").unwrap();
            assert!(run.save().is_ok());
            assert_eq!(reads, run.capture_reads());
            assert_eq!(
                fs::read(temp.0.join("define.xml")).unwrap(),
                fs::read(expected.join("define.xml")).unwrap()
            );
            assert_eq!(
                fs::read(temp.0.join("dm.json")).unwrap(),
                fs::read(expected.join("dm.json")).unwrap()
            );
            assert!(!run.save().unwrap());
        }

        #[test]
        fn xml_text_findings_keep_field_owners_inheritance_and_render_order_in_both_hosts() {
            let truth: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "../../../tests/host-fixtures/cases.json"
            ))
            .unwrap();
            for case in truth["families"]["public-define"]["xml_text_cases"]
                .as_array()
                .unwrap()
            {
                for host in [Language::Python, Language::R] {
                    let temp = Temp::fixture("sdtm-dm-metadata", false);
                    for (filename, changes) in [
                        ("spec.yaml", "specification_mutations"),
                        ("environment.yaml", "environment_mutations"),
                    ] {
                        let path = temp.0.join(filename);
                        let mut text = fs::read_to_string(&path).unwrap();
                        for change in case[changes].as_array().unwrap() {
                            let before = change[0].as_str().unwrap();
                            assert!(text.contains(before));
                            text = text.replacen(before, change[1].as_str().unwrap(), 1);
                        }
                        if filename == "environment.yaml" {
                            text = text.replace("  transport: dataset_json\n", "");
                        }
                        fs::write(path, text).unwrap();
                    }
                    if case["inherit_specification"].as_bool().unwrap() {
                        fs::rename(temp.0.join("spec.yaml"), temp.0.join("base.yaml")).unwrap();
                        fs::write(
                            temp.0.join("spec.yaml"),
                            "schema_version: '1.0'\nparent: base.yaml\n",
                        )
                        .unwrap();
                    }
                    let mut run = Define::prepare(
                        temp.0.join("environment.yaml").to_str().unwrap(),
                        "sdtm",
                        Some("2026-09-11T00:00:00"),
                        "0.3.0",
                        host,
                    );
                    let actual: Vec<_> = run.issues().iter().map(|issue| yamaa_adapters::json!({
                "phase": issue.phase, "condition": issue.condition, "requirement": issue.requirement,
                "spec_paths": issue.spec_paths,
                "context": issue.context.clone()
            })).collect();
                    assert_eq!(
                        yamaa_adapters::json!(actual),
                        case["issues"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["name"])
                    );
                    assert!(!temp.0.join("define.xml").exists());
                    assert!(!temp.0.join("dm.json").exists());
                    if actual.is_empty() {
                        let [(name, content, state)] = run.artifacts().collect::<Vec<_>>()[..]
                        else {
                            panic!("one held XML artifact")
                        };
                        assert_eq!((name, state), ("define.xml", PublicationState::Ready));
                        for fragment in case["xml_fragments"].as_array().unwrap() {
                            assert!(std::str::from_utf8(content)
                                .unwrap()
                                .contains(fragment.as_str().unwrap()));
                        }
                        let content = content.to_vec();
                        assert!(run.save().unwrap());
                        assert_eq!(fs::read(temp.0.join("define.xml")).unwrap(), content);
                    } else {
                        assert!(run.save().is_err());
                        assert_eq!(run.artifacts().len(), 0);
                    }
                    assert!(!temp.0.join("input").exists());
                    assert!(!temp.0.join("dm.csv").exists());
                }
            }
        }
        #[test]
        fn ae_and_vs_metadata_only_fixed_time_projection_has_independent_structure() {
            for (name, id, count, has_acrf, codelist) in [
                ("sdtm-ae-coding", "AE", 7, true, "MEDDRA"),
                (
                    "sdtm-vs-epoch-from-subject-elements",
                    "VS",
                    8,
                    false,
                    "EPOCH",
                ),
            ] {
                let temp = Temp::fixture(name, false);
                let run = temp.run("2026-09-30T00:00:00");
                assert!(run.issues().is_empty(), "{:?}", run.issues());
                let artifacts: Vec<_> = run.artifacts().collect();
                assert_eq!(artifacts.len(), 1);
                let xml = std::str::from_utf8(artifacts[0].1).unwrap();
                assert!(xml.contains("CreationDateTime=\"2026-09-30T00:00:00\""));
                assert!(xml.contains("SourceSystemVersion=\"1.0\""));
                assert!(xml.contains(&format!("OID=\"IG.{id}\" Name=\"{id}\"")));
                assert_eq!(xml.matches("<ItemDef ").count(), count);
                assert_eq!(xml.contains("<def:AnnotatedCRF>"), has_acrf);
                assert!(xml.contains(&format!("OID=\"CL.{codelist}\"")));
                assert!(!temp
                    .0
                    .join(format!("{}.csv", id.to_ascii_lowercase()))
                    .exists());
            }
        }
        #[test]
        fn semantic_failure_and_target_admission_have_zero_writes() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let spec = fs::read_to_string(temp.0.join("spec.yaml"))
                .unwrap()
                .replace("length: 2", "length: 0");
            fs::write(temp.0.join("spec.yaml"), spec).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run
                .issues()
                .iter()
                .any(|i| i.condition == "submission_length_invalid"));
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            assert!(!temp.0.join("dm.json").exists());
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty());
            fs::create_dir(temp.0.join("define.xml")).unwrap();
            assert!(run.save().is_err());
            assert!(run.failure().is_some());
            assert!(!temp.0.join("dm.json").exists());
            assert!(run.issues().iter().any(|i| i.phase == "engine"
                && i.condition == "engine_rejected"
                && i.requirement.is_none()
                && yamaa_adapters::json_wire::encode(&i.context)
                    .contains("publication_target_admission")));
            assert!(matches!(
                run.failure(),
                Some(yamaa_adapters::file_define::Failure::TargetAdmission { index: 1, .. })
            ));
            let states: Vec<_> = run.artifacts().map(|(_, _, state)| state).collect();
            assert_eq!(
                states,
                [PublicationState::Stopped, PublicationState::Failed]
            );
        }
        #[test]
        fn transport_targets_equal_after_lowercasing_are_refused_before_overwriting_a_sentinel() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let target = temp.0.join("dm.json");
            fs::write(&target, b"original publication sentinel").unwrap();
            // Under Other a lower-case dataset name is admitted, and its
            // transport file is named for it in lower case beside the document.
            let environment = temp.0.join("environment.yaml");
            fs::write(
                &environment,
                fs::read_to_string(&environment)
                    .unwrap()
                    .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml")
                    .replace("context: Submission", "context: Other"),
            )
            .unwrap();
            fs::write(
                temp.0.join("second.yaml"),
                fs::read_to_string(temp.0.join("spec.yaml"))
                    .unwrap()
                    .replace("domain: DM", "domain: dm")
                    .replace("path: dm.csv", "path: second.csv"),
            )
            .unwrap();
            fs::write(temp.0.join("define.xml"), b"original Define sentinel").unwrap();
            let mut names: Vec<_> = fs::read_dir(&temp.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            let mut result = temp.run("2026-09-11T00:00:00");
            assert_eq!(result.issues().len(), 1, "{:?}", result.issues());
            let issue = &result.issues()[0];
            assert_eq!(issue.phase, "validation");
            assert_eq!(issue.condition, "transport_path_collision");
            assert_eq!(issue.requirement.as_deref(), Some("REQ-1226"));
            assert_eq!(issue.spec_paths, ["sdtm.specs[1]"]);
            assert!(yamaa_adapters::json_wire::encode(&issue.context).contains("dm.json"));
            let reads = result.capture_reads();
            assert!(result.save().is_err());
            assert_eq!(reads, result.capture_reads());
            assert_eq!(fs::read(target).unwrap(), b"original publication sentinel");
            assert_eq!(
                fs::read(temp.0.join("define.xml")).unwrap(),
                b"original Define sentinel"
            );
            assert!(result
                .artifacts()
                .all(|(_, _, state)| state == PublicationState::Stopped));
            let mut after: Vec<_> = fs::read_dir(&temp.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            after.sort();
            assert_eq!(after, names, "no publication residue");
        }
        #[test]
        fn explicit_section_selector_datetime_and_missing_published_artifact_are_refused() {
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run
                .issues()
                .iter()
                .any(|i| i.condition == "producer_contract_mismatch"));
            assert!(run.save().is_err());
            let run = temp.run("2026-09-11");
            assert!(run.issues().iter().any(|i| i.spec_paths == ["created"]));
            let run = Define::prepare(
                temp.0.join("environment.yaml").to_str().unwrap(),
                "SDTM",
                None,
                "0.3.0",
                Language::Python,
            );
            assert!(run.issues().iter().any(|i| i.spec_paths == ["standard"]));
        }
        #[test]
        fn inherited_original_output_supplies_both_json_capture_and_publication_target() {
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            for directory in ["parents", "children", "artifacts"] {
                fs::create_dir(temp.0.join(directory)).unwrap();
            }
            let parent = fs::read_to_string(temp.0.join("spec.yaml"))
                .unwrap()
                .replace("path: dm.csv", "path: ../artifacts/dm.csv");
            fs::write(temp.0.join("parents/base.yaml"), parent).unwrap();
            fs::write(
                temp.0.join("children/spec.yaml"),
                "schema_version: '1.0'\nparent: ../parents/base.yaml\n",
            )
            .unwrap();
            let environment = fs::read_to_string(temp.0.join("environment.yaml"))
                .unwrap()
                .replace("  - spec.yaml", "  - children/spec.yaml")
                .replace("define: define.xml", "define: artifacts/define.xml");
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            fs::copy(
                root().join("benchmarks/sdtm-dm-metadata/expected/dm.csv"),
                temp.0.join("artifacts/dm.csv"),
            )
            .unwrap();
            let mut result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let xml = std::str::from_utf8(result.artifacts().nth(1).unwrap().1).unwrap();
            assert!(xml.contains("xlink:href=\"dm.json\""));
            assert!(std::str::from_utf8(result.artifacts().next().unwrap().1)
                .unwrap()
                .contains("\"metaDataRef\": \"define.xml\""));
            let reads = result.capture_reads();
            assert!(result.save().unwrap());
            assert!(temp.0.join("artifacts/dm.json").is_file());
            assert!(!temp.0.join("children/artifacts/dm.json").exists());
            assert!(!temp.0.join("dm.json").exists());
            assert_eq!(result.capture_reads(), reads);
        }
        #[test]
        fn mutual_references_and_published_contract_fail_before_any_publication() {
            // JSON-1 places the transport file beside the document wherever the
            // artifact is published, so the pair always references one directory.
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            fs::create_dir(temp.0.join("artifacts")).unwrap();
            fs::rename(temp.0.join("dm.csv"), temp.0.join("artifacts/dm.csv")).unwrap();
            let spec = fs::read_to_string(temp.0.join("spec.yaml"))
                .unwrap()
                .replace("path: dm.csv", "path: artifacts/dm.csv");
            fs::write(temp.0.join("spec.yaml"), spec).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().nth(1).unwrap().1).unwrap();
            assert!(xml.contains("xlink:href=\"dm.json\""));
            assert!(run.save().unwrap());
            assert!(temp.0.join("dm.json").is_file());
            assert!(!temp.0.join("artifacts/dm.json").exists());
            // Without a transport the leaf names the artifact, which must stay
            // inside the document's directory (DEFINE-16).
            let environment = fs::read_to_string(temp.0.join("environment.yaml"))
                .unwrap()
                .replace("  transport: dataset_json\n", "")
                .replace("define: define.xml", "define: package/define.xml");
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run
                .issues()
                .iter()
                .any(|i| i.condition == "artifact_outside_document"
                    && i.requirement.as_deref() == Some("REQ-1021")));
            assert!(run.save().is_err());
            assert!(!temp.0.join("package").exists());
            // REQ1230 assigns a stale output contract to REQ0535, before JSON schema.
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let csv = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
            fs::write(
                temp.0.join("dm.csv"),
                csv.replace("STUDYID,USUBJID", "USUBJID,STUDYID"),
            )
            .unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run
                .issues()
                .iter()
                .any(|i| i.condition == "producer_contract_mismatch"
                    && i.requirement.as_deref() == Some("REQ-0535")));
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            // A missing declared field is the same contract cause, before typed CSV
            // conversion can misclassify the absent field as a scalar/lookup problem.
            fs::write(temp.0.join("dm.csv"), csv.replace("STUDYID", "WRONG")).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run
                .issues()
                .iter()
                .any(|issue| issue.requirement.as_deref() == Some("REQ-0535")));
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            // REQ0957 preserves the verification phase, offending value and row key.
            fs::write(temp.0.join("dm.csv"), csv.replace(",F,USA", ",Z,USA")).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            let issue = run
                .issues()
                .iter()
                .find(|i| i.requirement.as_deref() == Some("REQ-0957"))
                .unwrap();
            assert_eq!(issue.phase, "verification");
            let context: yamaa_core::json::Value = issue.context.clone();
            assert_eq!(context["value"], "Z");
            assert!(context["key_values"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "CATH-UCSD-0001"));
            assert!(run.save().is_err());
            assert!(!temp.0.join("dm.json").exists());
        }
        #[test]
        fn integer_terminology_items_admit_equal_float_published_values() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .replace("name: AGE\n    type: int", "name: AGE\n    type: float")
        .replace(
            "core: Exp\n      role: Record Qualifier\n      length: 3",
            "core: Exp\n      role: Record Qualifier\n      length: 3\n      significant_digits: 3\n      codelist: AGE_VALUES",
        );
            fs::write(temp.0.join("spec.yaml"), spec).unwrap();
            let environment = fs::read_to_string(temp.0.join("environment.yaml")).unwrap().replace("sdtm:\n", "- codelists:\n  - id: AGE_VALUES\n    name: Ages\n    data_type: float\n    items: [{value: 34}, {value: 67}, {value: 52}]\nsdtm:\n");
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            let result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let data: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode_slice(result.artifacts().next().unwrap().1)
                    .unwrap();
            assert_eq!(data["columns"][5]["dataType"], "float");
            assert_eq!(data["rows"][0][5].as_f64(), Some(34.0));
        }
        #[test]
        fn unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files() {
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            let environment = fs::read_to_string(temp.0.join("environment.yaml")).unwrap().replace("  transport: dataset_json\n", "").replace("sdtm:\n", "- codelists:\n  - id: UNUSED\n    name: Used only in another submission\n    data_type: text\n    items: [{value: OTHER}]\nsdtm:\n");
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            let result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let xml = std::str::from_utf8(result.artifacts().next().unwrap().1).unwrap();
            assert!(!xml.contains("UNUSED"));
            assert_eq!(xml.matches("<CodeList ").count(), 4);
            assert!(xml.contains("xlink:href=\"acrf.pdf\""));
            assert!(!temp.0.join("acrf.pdf").exists());
            assert!(!temp.0.join("input").exists());
        }
        #[test]
        fn independent_metadata_refutations_preserve_shared_conditions_and_zero_writes() {
            for (from, to, condition, requirement) in [
                (
                    "derivation: DM_RAW.SEX\n",
                    "derivation: DM_RAW.SEX\n    verifications:\n      - allowed_values:\n          values: [F, M, U]\n",
                    "redundant_declaration",
                    "REQ-0955",
                ),
                // USUBJID assembles collected identifiers, which refutes Protocol.
                (
                    "type: Derived",
                    "type: Protocol",
                    "origin_contradicts_derivation",
                    "REQ-0922",
                ),
                (
                    "domain: DM",
                    "domain: DM\nmetadata: {label: duplicate}",
                    "reserved_metadata_key",
                    "REQ-0910",
                ),
            ] {
                let temp = Temp::fixture("sdtm-dm-metadata", false);
                let spec = fs::read_to_string(temp.0.join("spec.yaml"))
                    .unwrap()
                    .replace(from, to);
                fs::write(temp.0.join("spec.yaml"), spec).unwrap();
                let mut result = temp.run("2026-09-11T00:00:00");
                assert!(
                    result.issues().iter().any(|i| i.condition == condition
                        && i.requirement.as_deref() == Some(requirement)),
                    "{:?}",
                    result.issues()
                );
                assert!(result.save().is_err());
                assert!(!temp.0.join("define.xml").exists());
                assert!(!temp.0.join("dm.json").exists());
            }
            let temp = Temp::fixture("sdtm-ae-coding", false);
            let spec = fs::read_to_string(temp.0.join("spec.yaml"))
                .unwrap()
                .replace(
                "codelist: MEDDRA\n      origin:\n        type: Assigned\n        source: Sponsor",
                "codelist: MEDDRA\n      origin:\n        type: Collected\n        source: Vendor",
            );
            fs::write(temp.0.join("spec.yaml"), spec).unwrap();
            let result = temp.run("2026-09-30T00:00:00");
            assert!(
                result
                    .issues()
                    .iter()
                    .any(|i| i.condition == "origin_contradicts_derivation"
                        && i.spec_paths[0].contains("AEDECOD")),
                "{:?}",
                result.issues()
            );
        }
        /// Rewrite the DM fixture's metadata-only environment for one section.
        fn metadata_only(temp: &Temp, section: &str, context: &str) {
            let environment = fs::read_to_string(temp.0.join("environment.yaml"))
                .unwrap()
                .replace("  transport: dataset_json\n", "")
                .replace("context: Submission", &format!("context: {context}"))
                .replace(
                    "sdtm:\n  standard:\n    name: SDTMIG",
                    &format!(
                        "{section}:\n  standard:\n    name: {}",
                        if section == "send" {
                            "SENDIG"
                        } else {
                            "SDTMIG"
                        }
                    ),
                );
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
        }
        fn replace_spec(temp: &Temp, from: &str, to: &str) {
            let spec = fs::read_to_string(temp.0.join("spec.yaml")).unwrap();
            assert!(spec.contains(from), "{from}");
            fs::write(temp.0.join("spec.yaml"), spec.replacen(from, to, 1)).unwrap();
        }
        fn item<'a>(xml: &'a str, oid: &str) -> &'a str {
            let begin = xml.find(&format!("<ItemDef OID=\"{oid}\"")).unwrap();
            &xml[begin..begin + xml[begin..].find("</ItemDef>").unwrap()]
        }
        #[test]
        fn collected_origins_name_their_form_page_under_submission() {
            // SITEID is collected by the investigator on the first form page.
            let paged = "role: Record Qualifier\n      length: 10\n      origin:\n        type: Collected\n        source: Investigator\n        documents:\n          - document: acrf\n            pages: {type: PhysicalRef, refs: \"1\"}\n";
            let pageless = "role: Record Qualifier\n      length: 10\n      origin:\n        type: Collected\n        source: Investigator\n";
            let unpaged = format!("{pageless}        documents: [acrf]\n");
            for (replacement, context, path) in [
                (pageless, "Submission", Some("SITEID.submission.origin")),
                (
                    unpaged.as_str(),
                    "Submission",
                    Some("SITEID.submission.origin.documents"),
                ),
                (pageless, "Other", None),
                (unpaged.as_str(), "Other", None),
            ] {
                let temp = Temp::fixture("sdtm-dm-metadata", false);
                replace_spec(&temp, paged, replacement);
                metadata_only(&temp, "sdtm", context);
                let mut run = temp.run("2026-09-11T00:00:00");
                if let Some(path) = path {
                    assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
                    let issue = &run.issues()[0];
                    assert_eq!(
                        (
                            issue.phase.as_str(),
                            issue.condition.as_str(),
                            issue.requirement.as_deref()
                        ),
                        ("validation", "origin_document_missing", Some("REQ-0893"))
                    );
                    assert_eq!(issue.spec_paths, [format!("sdtm.specs[0].columns.{path}")]);
                    assert!(run.save().is_err());
                    assert!(!temp.0.join("define.xml").exists());
                } else {
                    // Under Other the reference to the form needs no page.
                    assert!(run.issues().is_empty(), "{:?}", run.issues());
                    let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
                    assert!(xml.contains("def:Context=\"Other\""));
                    assert!(
                        item(xml, "IT.DM.SITEID").contains("<def:DocumentRef leafID=\"LF.acrf\"/>")
                    );
                }
            }
        }
        #[test]
        fn not_available_and_send_sources_follow_the_family_pairs() {
            let assigned =
                "codelist: AGEU\n      origin:\n        type: Assigned\n        source: Sponsor\n";
            let not_available = "codelist: AGEU\n      origin:\n        type: Not Available\n        description: Age units were not collected.\n";
            // An sdtm Not Available declares no source and says why.
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            replace_spec(&temp, assigned, not_available);
            metadata_only(&temp, "sdtm", "Submission");
            let run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
            let ageu = item(xml, "IT.DM.AGEU");
            assert!(
                ageu.contains("<def:Origin Type=\"Not Available\">"),
                "{ageu}"
            );
            assert!(ageu.contains(">Age units were not collected.</TranslatedText>"));
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            replace_spec(
                &temp,
                assigned,
                &not_available.replace("description", "source: Sponsor\n        description"),
            );
            metadata_only(&temp, "sdtm", "Submission");
            let run = temp.run("2026-09-11T00:00:00");
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            assert_eq!(run.issues()[0].condition, "origin_source_not_admitted");
            assert_eq!(
                run.issues()[0].spec_paths,
                ["sdtm.specs[0].columns.AGEU.submission.origin"]
            );
            // send takes the sources the sdtm table admits, or none.
            let send = |temp: &Temp| {
                Define::prepare(
                    temp.0.join("environment.yaml").to_str().unwrap(),
                    "send",
                    Some("2026-09-11T00:00:00"),
                    "0.3.0",
                    Language::Python,
                )
            };
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            metadata_only(&temp, "send", "Submission");
            let run = send(&temp);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
            assert!(item(xml, "IT.DM.SITEID")
                .contains("<def:Origin Type=\"Collected\" Source=\"Investigator\">"));
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            metadata_only(&temp, "send", "Submission");
            replace_spec(
                &temp,
                assigned,
                "codelist: AGEU\n      origin:\n        type: Assigned\n",
            );
            let run = send(&temp);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
            assert!(item(xml, "IT.DM.AGEU").contains("<def:Origin Type=\"Assigned\"/>"));
            for origin in [
                "codelist: AGEU\n      origin:\n        type: Assigned\n        source: Investigator\n",
                not_available.replace("description", "source: Sponsor\n        description").as_str(),
            ] {
                let temp = Temp::fixture("sdtm-dm-metadata", false);
                metadata_only(&temp, "send", "Submission");
                replace_spec(&temp, assigned, origin);
                let run = send(&temp);
                assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
                assert_eq!(run.issues()[0].condition, "origin_source_not_admitted");
                assert_eq!(
                    run.issues()[0].spec_paths,
                    ["send.specs[0].columns.AGEU.submission.origin"]
                );
            }
        }
        const STUDY_LB: &str = "schema_version: '1.0'\ndomain: LB\nkeys: [USUBJID, LBTESTCD]\ninput: {RAW: raw.csv}\noutput: {path: lb.csv, columns: [USUBJID, LBTESTCD]}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: RAW.USUBJID\n    submission: {core: Req, length: 20, origin: {type: Assigned, source: Sponsor}}\n  - name: LBTESTCD\n    type: str\n    label: Lab Test or Examination Short Name\n    derivation: RAW.LBTESTCD\n    submission: {core: Req, length: 8, origin: {type: Assigned, source: Sponsor}}\nsubmission: {label: Laboratory Test Results, class: FINDINGS, structure: One record per lab test per subject, repeating: true}\n";
        const STUDY_ADLB: &str = "schema_version: '1.0'\ndomain: ADLB\nkeys: [USUBJID, PARAMCD]\ninput: {LB: lb.csv}\noutput: {path: adlb.csv, columns: [USUBJID, PARAMCD, SRCDOM]}\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: LB.USUBJID\n    submission: {core: Req, mandatory: true, length: 20, origin: {type: Predecessor, description: LB.USUBJID}}\n  - name: PARAMCD\n    type: str\n    label: Parameter Code\n    derivation: LB.LBTESTCD\n    submission: {core: Req, mandatory: true, length: 8, origin: {type: Predecessor, description: LB.LBTESTCD}}\n  - name: SRCDOM\n    type: str\n    label: Source Data\n    derivation: {literal: LB}\n    submission: {core: Perm, mandatory: false, length: 8, inventory_vocabulary: true, origin: {type: Assigned}}\nsubmission: {label: Laboratory Analysis Dataset, class: BASIC DATA STRUCTURE, structure: One record per subject per parameter, repeating: true}\n";
        const STUDY_SDTM: &str = "sdtm:\n  standard: {name: SDTMIG, version: '3.4', status: Final}\n  specs: [sdtm/lb.yaml]\n  define: sdtm/define.xml\n";
        const STUDY_ADAM: &str = "schema_version: '1.0'\nstudy: {id: STUDY, name: Study, description: Cross-section witness, protocol_name: Protocol}\nadam:\n  standard: {name: ADaMIG, version: '1.3', status: Final}\n  specs: [adlb.yaml]\n  define: define.xml\n  transport: dataset_json\n  source_system_version: '1.0'\n";
        fn cross_section(adlb: &str, published: &str, sdtm: bool) -> (Temp, Define) {
            let temp = Temp::new();
            fs::create_dir(temp.0.join("sdtm")).unwrap();
            fs::write(temp.0.join("sdtm/lb.yaml"), STUDY_LB).unwrap();
            fs::write(temp.0.join("adlb.yaml"), adlb).unwrap();
            fs::write(temp.0.join("adlb.csv"), published).unwrap();
            let environment = if sdtm {
                format!("{STUDY_ADAM}{STUDY_SDTM}")
            } else {
                STUDY_ADAM.into()
            };
            fs::write(temp.0.join("environment.yaml"), environment).unwrap();
            let run = Define::prepare(
                temp.0.join("environment.yaml").to_str().unwrap(),
                "adam",
                Some("2026-09-30T00:00:00"),
                "0.3.0",
                Language::Python,
            );
            (temp, run)
        }
        #[test]
        fn predecessor_labels_and_the_dataset_inventory_span_every_section() {
            // ADLB copies LB from the sdtm section: USUBJID keeps its name and
            // label, PARAMCD renames LBTESTCD with its own label, and SRCDOM
            // names the sdtm section's LB.
            let published = "USUBJID,PARAMCD,SRCDOM\nS01-001,GLUC,LB\n";
            let (temp, mut run) = cross_section(STUDY_ADLB, published, true);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let artifacts: Vec<_> = run
                .artifacts()
                .map(|(name, bytes, _)| (name.to_owned(), bytes.to_vec()))
                .collect();
            assert_eq!(
                artifacts.iter().map(|a| a.0.as_str()).collect::<Vec<_>>(),
                ["adlb.json", "define.xml"]
            );
            let xml = std::str::from_utf8(&artifacts[1].1).unwrap();
            let paramcd = item(xml, "IT.ADLB.PARAMCD");
            assert!(paramcd.contains(">Parameter Code</TranslatedText>"));
            assert!(
                paramcd.contains("<def:Origin Type=\"Predecessor\">"),
                "{paramcd}"
            );
            assert!(paramcd.contains(">LB.LBTESTCD</TranslatedText>"));
            assert!(
                !xml.contains("IG.LB"),
                "the sdtm section is read, not represented"
            );
            assert!(run.save().unwrap());
            assert!(temp.0.join("adlb.json").is_file());
            assert!(!temp.0.join("sdtm/define.xml").exists());
            // A copy that keeps the predecessor's name keeps its label.
            let (temp, mut run) = cross_section(
                &STUDY_ADLB.replacen(
                    "label: Unique Subject Identifier",
                    "label: Subject Identifier",
                    1,
                ),
                published,
                true,
            );
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            let issue = &run.issues()[0];
            assert_eq!(
                (
                    issue.phase.as_str(),
                    issue.condition.as_str(),
                    issue.requirement.as_deref()
                ),
                ("validation", "predecessor_label_mismatch", Some("SUB-20"))
            );
            assert_eq!(issue.spec_paths, ["adam.specs[0].columns.USUBJID.label"]);
            assert_eq!(
                issue.context,
                yamaa_adapters::json!({"detail": "USUBJID", "label": "Subject Identifier", "predecessor": "LB.USUBJID", "predecessor_label": "Unique Subject Identifier"})
            );
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            // A name outside the study's inventory fails the published recheck.
            let (_, run) = cross_section(STUDY_ADLB, &published.replace(",LB\n", ",XX\n"), true);
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            assert_eq!(
                (
                    run.issues()[0].condition.as_str(),
                    run.issues()[0].requirement.as_deref()
                ),
                ("allowed_values_failed", Some("REQ-1156"))
            );
            assert_eq!(run.issues()[0].context["value"], "XX");
            // Without the sdtm section, LB names no dataset of the study and
            // no predecessor column is declared to compare.
            let (_, run) = cross_section(
                &STUDY_ADLB.replacen(
                    "label: Unique Subject Identifier",
                    "label: Subject Identifier",
                    1,
                ),
                published,
                false,
            );
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            assert_eq!(run.issues()[0].condition, "allowed_values_failed");
            assert_eq!(run.issues()[0].context["value"], "LB");
        }
    }
}
