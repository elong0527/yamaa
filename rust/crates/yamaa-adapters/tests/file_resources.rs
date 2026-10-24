//! File contracts: local paths and shared Define-XML. The cases that write
//! files, make one call and compare what it reports are
//! `fixtures/diagnostics/local_paths.json` and `shared_define.json`; each
//! test here proves a seam the replay cannot reach, which its first comment
//! names, or stays by design (#3778).
mod file {
    #[cfg(feature = "yaml")]
    mod local_paths {
        use crate::support::issues::assert_issues;
        /// Seam: the save, after the shared program and each entry's input
        /// and specification change or go, writes the bytes each build held,
        /// twice; a case stages its files once, before its one call.
        #[test]
        fn entry_relative_shared_program_uses_each_study_input_and_saves_held_artifacts() {
            use yamaa_adapters::specification_report::Identity;
            use yamaa_core::domain_entry::Request;
            let study = Study::new();
            fs::write(study.0.join("shared/base.yaml"), "yamaa: \"1.0\"\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: input/data.csv, relative_to: entry, types: {ID: int}}}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\noutput: {path: output/result.csv, relative_to: entry, columns: [ID], report: output/report.csv}\n").unwrap();
            fs::create_dir(study.0.join("shared/input")).unwrap();
            fs::write(study.0.join("shared/input/data.csv"), b"ID\n999\n").unwrap();
            let mut results = Vec::new();
            for (name, bytes) in [("first", b"ID\n1\n"), ("second", b"ID\n2\n")] {
                fs::create_dir_all(study.0.join(name).join("input")).unwrap();
                fs::create_dir_all(study.0.join(name).join("output")).unwrap();
                fs::write(study.0.join(name).join("input/data.csv"), bytes).unwrap();
                fs::write(
                    study.0.join(name).join("spec.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\ndomain: STUDY\n",
                )
                .unwrap();
                let path = study.text(&format!("{name}/spec.yaml"));
                let result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
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
                    assert!(study.0.join(*name).join("output/report.csv").is_file());
                }
            }
            assert!(!study.0.join("shared/output").exists());
        }

        /// Seam: the prepared run's resolved source path and the written
        /// input origin (`FileSpecification`), which no operation reports.
        #[test]
        fn composed_relative_input_base_can_change_without_redeclaring_the_inherited_path() {
            let study = Study::new();
            fs::write(study.0.join("shared/base.yaml"), "yamaa: \"1.0\"\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: data.csv, relative_to: entry}}\noutput: {path: result.csv, relative_to: entry, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n").unwrap();
            fs::write(study.0.join("entry/data.csv"), b"ID\n1\n").unwrap();
            fs::write(study.0.join("shared/data.csv"), b"ID\n9\n").unwrap();
            // Clearing the inherited base restores its default, the declaring
            // file (P-CLR-1).
            for (mode, layer, expected) in [
                ("entry", "input: {SRC: {relative_to: entry}}", "data.csv"),
                (
                    "declaring",
                    "input: {SRC: {relative_to: declaring}}",
                    "../shared/data.csv",
                ),
                (
                    "cleared",
                    "clear: [input.SRC.relative_to]",
                    "../shared/data.csv",
                ),
            ] {
                fs::write(
                    study.0.join("entry/spec.yaml"),
                    format!("yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\n{layer}\n"),
                )
                .unwrap();
                let (resources, entry) =
                    file_configuration::resources(&study.text("entry/spec.yaml")).unwrap();
                let prepared = FileSpecification::prepare(resources, &entry).unwrap();
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
                assert!(prepared
                    .build(
                        &mut prepared.run_scope(None),
                        &mut crate::support::ports::NoFunctions,
                        Scope::default()
                    )
                    .dataset
                    .result
                    .is_ok());
            }
        }

        use std::fs;
        use yamaa_adapters::{
            file_configuration,
            file_preparation::FileSpecification,
            file_resources::{Error, Resources},
        };
        use yamaa_core::application_observer::Scope;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("local-paths");
                for name in ["entry", "shared", "outputs"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                Self(path)
            }
            fn text(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().replace('\\', "/")
            }
            fn resources(&self) -> Resources {
                Resources::for_entry(&self.text("entry")).unwrap()
            }
        }

        /// Seam: the composed document's output fields and its written output
        /// origin, which no operation reports; its save also writes a run
        /// report whose bytes the test does not compare, and a case's
        /// `saved` states every file whole.
        #[test]
        fn complete_output_replacement_resets_the_base_for_primary_and_report_files() {
            use yamaa_adapters::specification_report::Identity;
            use yamaa_core::{domain_entry::Request, schema::DocumentNode};

            for inherited_base in ["", "relative_to: entry, "] {
                let study = Study::new();
                fs::write(study.0.join("shared/base.yaml"), format!("yamaa: \"1.0\"\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {{SRC: {{path: data.csv, types: {{ID: int}}}}}}\ncolumns: [{{name: ID, type: int, derivation: SRC.ID}}]\noutput: {{path: ignored.csv, {inherited_base}columns: [ID], report: ignored-report.csv}}\n")).unwrap();
                fs::write(study.0.join("shared/replacement.yaml"), "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\noutput: {path: result.csv, columns: [ID], report: report.csv}\n").unwrap();
                fs::write(
                    study.0.join("entry/spec.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/replacement.yaml\n",
                )
                .unwrap();
                fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
                let path = study.text("entry/spec.yaml");
                let (resources, entry) = file_configuration::resources(&path).unwrap();
                let prepared = FileSpecification::prepare(resources, &entry).unwrap();
                let document = prepared.run().document().document();
                let output = document.field(document.root(), "output").unwrap();
                for (field, written) in [("path", "result.csv"), ("report", "report.csv")] {
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

                let mut result = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
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
                for written in ["result.csv", "report.csv"] {
                    assert!(study.0.join("shared").join(written).is_file());
                    assert!(!study.0.join("entry").join(written).exists());
                }
                for directory in ["shared", "entry"] {
                    for written in ["ignored.csv", "ignored-report.csv"] {
                        assert!(!study.0.join(directory).join(written).exists());
                    }
                }
            }
        }

        /// Seam: a symbolic link in the study directory, which a case's
        /// staged files cannot hold. A link is found when the file is
        /// inspected, after the layers are composed, so its finding names no
        /// layer; the URI source refused where its layer is read is
        /// `local_paths.json::a_uri_input_source_keeps_its_normative_cause_and_authored_path`.
        #[test]
        fn public_symlink_sources_keep_their_normative_cause_and_authored_path() {
            use yamaa_adapters::specification_report::Identity;
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
            let spec = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: 'link.csv'}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\noutput: {path: output.csv, columns: [ID]}\n";
            fs::write(study.0.join("entry/spec.yaml"), spec).unwrap();
            let path = study.text("entry/spec.yaml");
            let issues = crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
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
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"resource_path_symlink","rule":"FILE-8","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":"link.csv"}}),
                ],
                "link.csv",
            );
            assert!(!study.0.join("entry/output.csv").exists());
        }

        /// Seam: the prepared run's capture counters and resolved locations,
        /// which no operation reports.
        #[test]
        fn inherited_parent_keeps_its_own_input_and_output_locations_without_configuration() {
            let study = Study::new();
            let parent = "yamaa: \"1.0\"\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\noutput: {path: ../outputs/out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
            let child = "yamaa: \"1.0\"\ntype: spec\nparent: ../shared/base.yaml\ndomain: CHILD\n";
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
            let prepared = FileSpecification::prepare(resources, &entry).unwrap();
            let origin = prepared.run().document().written_output_origin().unwrap();
            assert_eq!(origin.declaring_source, study.text("shared/base.yaml"));
            assert_eq!(origin.written, "../outputs/out.csv");
            assert_eq!(prepared.run().source().path, "../shared/data.csv");
            assert_eq!(
                prepared.run().compiled().output_path(),
                "../outputs/out.csv"
            );
            assert_eq!(prepared.capture_reads(), 2);
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
            assert!(!study.0.join("outputs/out.csv").exists());
        }

        /// Seam: the `Resources` port's captures, their shared snapshot and
        /// their read counter, which no operation reports.
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

        /// Seam: the `Resources` port's inspections and locations, which no
        /// operation reports.
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

        /// Seam: the `Resources` port's inspection of symbolic links, which a
        /// case's staged files cannot hold.
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

        /// Seam: the `Resources` port's admission of a written path, by its
        /// internal failure, which no operation reports.
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

        /// Seam: a hard link, which a case's staged files cannot hold, and the
        /// `Resources` port's snapshots.
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

        /// Seam: the `Resources` port's snapshot of binary bytes and its read
        /// counter, which no operation reports.
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

        /// Seam: a junction that `mklink` makes, which a case's staged files
        /// cannot hold, and the `Resources` port's captures.
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
        //! Independent fixed-byte DM and structural AE/VS shared-generation
        //! witnesses. The compositions that compare what one call reports, its
        //! issues and the files its save publishes, are the cases of
        //! `fixtures/diagnostics/shared_define.json`; the tests here compare
        //! fragments of a generated document, which a case states only whole,
        //! or what a composition holds and reads between its preparation and
        //! its save, or stay by design (#3778).
        use crate::support::Edit as _;
        use std::{
            fs,
            path::{Path, PathBuf},
        };
        use yamaa_adapters::file_define::{Define, PublicationState};
        use yamaa_core::{project_environment::Submission, project_function::Language};
        struct Temp(crate::support::Scratch);
        impl Temp {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("define");
                Self(path)
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
                        original.join("submission-project.yaml"),
                        this.0.join("project.yaml"),
                    )
                    .unwrap();
                    if published {
                        fs::copy(original.join("expected/lb.csv"), this.0.join("lb.csv")).unwrap();
                    }
                } else {
                    fs::copy(original.join("project.yaml"), this.0.join("project.yaml")).unwrap();
                }
                if published && name != "sdtm-lb-metadata" {
                    fs::copy(original.join("expected/dm.csv"), this.0.join("dm.csv")).unwrap();
                }
                this
            }
            /// Rewrite the copied project file; every replaced text must exist.
            fn edit_project(&self, changes: &[(&str, &str)]) {
                let path = self.0.join("project.yaml");
                let mut text = fs::read_to_string(&path).unwrap();
                for (before, after) in changes {
                    assert!(text.contains(before), "{before:?}");
                    text = text.replacen(before, after, 1);
                }
                fs::write(path, text).unwrap();
            }
            fn run(&self, created: &str) -> Define {
                Define::prepare(
                    self.0.join("project.yaml").to_str().unwrap(),
                    Submission::Sdtm,
                    Some(created),
                    "0.3.0",
                    Some(Language::Python),
                    None,
                )
            }
        }
        /// Seam: what the save writes after the composition's sources change
        /// or go, and that it reads nothing again; the test also compares
        /// fragments and counts of the generated Define-XML, which a case
        /// states only whole, and no authored file holds this document.
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
            // SDTMIG's required variables are mandatory, the pivoted result and
            // collection date are collected, and the numeric standard result
            // is derived by its method.
            for reference in [
                "<ItemRef ItemOID=\"IT.LB.DOMAIN\" OrderNumber=\"1\" Mandatory=\"Yes\"/>",
                "<ItemRef ItemOID=\"IT.LB.LBTESTCD\" OrderNumber=\"5\" Mandatory=\"Yes\"/>",
                "<ItemRef ItemOID=\"IT.LB.LBTEST\" OrderNumber=\"6\" Mandatory=\"Yes\"/>",
                "<ItemRef ItemOID=\"IT.LB.LBORRES\" OrderNumber=\"7\" Mandatory=\"No\"/>",
                "<ItemRef ItemOID=\"IT.LB.LBSTRESN\" OrderNumber=\"9\" Mandatory=\"No\" MethodOID=\"MT.LB.LBSTRESN\"/>",
                "<ItemRef ItemOID=\"IT.LB.LBDTC\" OrderNumber=\"11\" Mandatory=\"No\"/>",
            ] {
                assert!(xml.contains(reference), "{reference}");
            }
            for (column, origin) in [
                ("LBORRES", "Type=\"Collected\" Source=\"Vendor\""),
                ("LBSTRESN", "Type=\"Derived\" Source=\"Sponsor\""),
                ("LBDTC", "Type=\"Collected\" Source=\"Vendor\""),
            ] {
                let begin = xml
                    .find(&format!("<ItemDef OID=\"IT.LB.{column}\""))
                    .unwrap();
                let item = &xml[begin..begin + xml[begin..].find("</ItemDef>").unwrap()];
                assert!(item.contains(&format!("<def:Origin {origin}/>")), "{item}");
            }
            assert_eq!(xml.matches("<MethodDef ").count(), 2);
            assert!(!temp.0.join("input").exists());
            let reads = run.capture_reads();
            for file in ["spec.yaml", "submission-spec.yaml", "project.yaml"] {
                fs::remove_file(temp.0.join(file)).unwrap();
            }
            fs::write(temp.0.join("lb.csv"), b"changed after preparation").unwrap();
            assert!(run.save().unwrap());
            assert_eq!(reads, run.capture_reads());
            for artifact in artifacts {
                assert_eq!(fs::read(temp.0.join(artifact.0)).unwrap(), artifact.1);
            }
        }
        /// Stays beside the replay by design (#3778): it finds its issue by
        /// substrings of the context, the value-level codelist and the test
        /// code it names. The original source the creatinine entry inherits
        /// is refused in
        /// `shared_define.json::an_lb_value_entry_inheriting_the_original_investigator_source_is_refused_with_zero_writes`.
        #[test]
        fn a_published_unit_outside_its_value_level_codelist_is_refused_with_zero_writes() {
            let temp = Temp::fixture("sdtm-lb-metadata", true);
            let csv = fs::read_to_string(temp.0.join("lb.csv"))
                .unwrap()
                .edit("95,mg/dL,95", "95,mmol/L,95");
            fs::write(temp.0.join("lb.csv"), csv).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(
                run.issues()
                    .iter()
                    .any(|i| i.condition() == "allowed_values_failed"
                        && yamaa_adapters::json_wire::encode_object(&i.context)
                            .contains("CL_GLUCOSE")
                        && yamaa_adapters::json_wire::encode_object(&i.context).contains("GLUC")),
                "{:?}",
                run.issues()
            );
            assert!(run.save().is_err());
            assert!(!temp.0.join("define.xml").exists());
            assert!(!temp.0.join("lb.json").exists());
        }
        fn root() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .canonicalize()
                .unwrap()
        }
        /// Seam: what the save writes after the composition's sources change
        /// or go, which it holds from its preparation without reading again,
        /// and a repeated save that restores a removed document; a case stages
        /// its files once, before its one call.
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
            fs::remove_file(temp.0.join("project.yaml")).unwrap();
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
            // A repeated save rewrites the same held bytes without reading a
            // source again, so a deleted document is restored unchanged.
            fs::remove_file(temp.0.join("define.xml")).unwrap();
            assert!(run.save().unwrap());
            assert_eq!(reads, run.capture_reads());
            assert_eq!(
                fs::read(temp.0.join("define.xml")).unwrap(),
                fs::read(expected.join("define.xml")).unwrap()
            );
        }

        /// Stays beside the replay: it is the Rust reader of the `public-define`
        /// host-fixture family's `xml_text_cases`, which both installed hosts
        /// read too.
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
                    // Only Define-XML is generated: the section omits `transport`.
                    temp.edit_project(&[("  transport: dataset_json\n", "")]);
                    for (filename, changes) in [
                        ("spec.yaml", "specification_mutations"),
                        ("project.yaml", "project_mutations"),
                    ] {
                        let path = temp.0.join(filename);
                        let mut text = fs::read_to_string(&path).unwrap();
                        for change in case[changes].as_array().unwrap() {
                            let before = change[0].as_str().unwrap();
                            assert!(text.contains(before));
                            text = text.replacen(before, change[1].as_str().unwrap(), 1);
                        }
                        fs::write(path, text).unwrap();
                    }
                    if case["inherit_specification"].as_bool().unwrap() {
                        fs::rename(temp.0.join("spec.yaml"), temp.0.join("base.yaml")).unwrap();
                        fs::write(
                            temp.0.join("spec.yaml"),
                            "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\n",
                        )
                        .unwrap();
                    }
                    let mut run = Define::prepare(
                        temp.0.join("project.yaml").to_str().unwrap(),
                        Submission::Sdtm,
                        Some("2026-09-11T00:00:00"),
                        "0.3.0",
                        Some(host),
                        None,
                    );
                    let actual: Vec<_> = run
                        .issues()
                        .iter()
                        .map(crate::support::study::issue_row)
                        .collect();
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
        /// Stays beside the replay: it compares fragments and counts of the
        /// generated Define-XML, which a case states only whole, and neither
        /// benchmark holds an expected document.
        #[test]
        fn ae_and_vs_metadata_only_fixed_time_projection_has_independent_structure() {
            for (name, id, count, codelist) in [
                ("sdtm-ae-coding", "AE", 7, "MEDDRA"),
                ("sdtm-vs-epoch-from-subject-elements", "VS", 8, "EPOCH"),
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
                // Each project declares the annotated case report form its
                // collected values reference.
                assert!(xml.contains("<def:AnnotatedCRF>"));
                assert!(xml.contains(&format!("OID=\"CL.{codelist}\"")));
                if id == "AE" {
                    // SDTMIG requires the dictionary-derived term, a synonym
                    // of the reported term.
                    assert!(xml.contains(
                        "<ItemRef ItemOID=\"IT.AE.AEDECOD\" OrderNumber=\"6\" Mandatory=\"Yes\" Role=\"Synonym Qualifier\""
                    ));
                } else {
                    // The investigator collects the result and its date-time,
                    // and under Other each references the annotated form with
                    // no page. EPOCH is a permissible timing variable the
                    // sponsor derives from the subject's elements.
                    for fragment in [
                        "<ItemRef ItemOID=\"IT.VS.VSORRES\" OrderNumber=\"6\" Mandatory=\"No\" Role=\"Result Qualifier\"/>",
                        "<ItemRef ItemOID=\"IT.VS.VSDTC\" OrderNumber=\"7\" Mandatory=\"No\" Role=\"Timing\"/>",
                        "<ItemRef ItemOID=\"IT.VS.EPOCH\" OrderNumber=\"8\" Mandatory=\"No\" Role=\"Timing\" MethodOID=\"MT.VS.EPOCH\"/>",
                    ] {
                        assert!(xml.contains(fragment), "{fragment}");
                    }
                    let collected = "<def:Origin Type=\"Collected\" Source=\"Investigator\">\n          <def:DocumentRef leafID=\"LF.acrf\"/>\n        </def:Origin>";
                    for (oid, origin) in [
                        ("IT.VS.VSORRES", collected),
                        ("IT.VS.VSDTC", collected),
                        (
                            "IT.VS.EPOCH",
                            "<def:Origin Type=\"Derived\" Source=\"Sponsor\"/>",
                        ),
                    ] {
                        assert!(item(xml, oid).contains(origin), "{oid}");
                    }
                }
                assert!(!temp
                    .0
                    .join(format!("{}.csv", id.to_ascii_lowercase()))
                    .exists());
            }
        }
        /// Seam: a target that becomes a directory between the composition
        /// and its save, which a case's staged files cannot change, since the
        /// composition admits its targets when it is prepared, and the
        /// failure and publication states the save records. A semantic
        /// failure's zero writes are
        /// `shared_define.json::a_semantic_failure_writes_nothing`.
        #[test]
        fn a_target_refused_at_save_stops_the_composition_with_zero_writes() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let mut run = temp.run("2026-09-11T00:00:00");
            assert!(run.issues().is_empty());
            fs::create_dir(temp.0.join("define.xml")).unwrap();
            assert!(run.save().is_err());
            assert!(run.failure().is_some());
            assert!(!temp.0.join("dm.json").exists());
            // A directory at the target is refused as the walk refuses it, at
            // the section's `define`, naming the file as written (FILE-7,
            // #3030 D189).
            let text = |i: &yamaa_core::diagnostic::Diagnostic, key: &str| {
                i.context
                    .get(key)
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            };
            assert!(run.issues().iter().any(|i| i.phase() == "output"
                && i.condition() == "resource_path_not_regular_file"
                && i.rule() == Some("FILE-7")
                && i.spec_paths == ["sdtm.define"]
                && text(i, "path").as_deref() == Some("define.xml")));
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
        /// Stays beside the replay by design (#3778): its finding names the
        /// colliding file in a context that names both files by their absolute
        /// paths, which it compares by a substring. It also proves what the
        /// refused save reads and the publication states it records.
        #[test]
        fn transport_targets_equal_after_lowercasing_are_refused_before_overwriting_a_sentinel() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let target = temp.0.join("dm.json");
            fs::write(&target, b"original publication sentinel").unwrap();
            // Under Other a lower-case dataset name is admitted, and its
            // transport file is named for it in lower case beside the document.
            temp.edit_project(&[
                ("  - spec.yaml", "  - spec.yaml\n  - second.yaml"),
                ("sdtm:\n", "sdtm:\n  context: Other\n"),
            ]);
            fs::write(
                temp.0.join("second.yaml"),
                fs::read_to_string(temp.0.join("spec.yaml"))
                    .unwrap()
                    .edit("domain: DM", "domain: dm")
                    .edit("path: dm.csv", "path: second.csv"),
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
            assert_eq!(issue.phase(), "validation");
            assert_eq!(issue.condition(), "transport_path_collision");
            assert_eq!(issue.rule(), Some("JSON-2"));
            assert_eq!(issue.spec_paths, ["sdtm.specs[1]"]);
            assert!(yamaa_adapters::json_wire::encode_object(&issue.context).contains("dm.json"));
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
        /// Stays beside the replay by design (#3778): it reads one member of
        /// the context's key values. The relocated dataset, the artifact
        /// outside the document's directory and the stale published contract
        /// are `shared_define.json`'s
        /// `a_transport_file_is_published_beside_the_document_wherever_its_dataset_is`,
        /// `an_artifact_outside_the_documents_directory_is_refused_without_a_transport`,
        /// `a_published_dataset_with_reordered_columns_fails_its_producer_contract`
        /// and `a_published_dataset_missing_a_declared_column_fails_its_producer_contract`.
        #[test]
        fn a_published_value_outside_its_codelist_keeps_the_verification_phase_and_row_key() {
            // REQ0957 preserves the verification phase, offending value and row key.
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let csv = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
            fs::write(temp.0.join("dm.csv"), csv.edit(",F,USA", ",Z,USA")).unwrap();
            let mut run = temp.run("2026-09-11T00:00:00");
            let issue = run
                .issues()
                .iter()
                .find(|i| i.rule() == Some("CT-7"))
                .unwrap();
            assert_eq!(issue.phase(), "verification");
            let context = yamaa_core::json::Value::Object(issue.context.clone());
            assert_eq!(context["value"], "Z");
            assert!(context["key_values"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "CATH-UCSD-0001"));
            assert!(run.save().is_err());
            assert!(!temp.0.join("dm.json").exists());
        }
        /// Stays beside the replay: it reads two values of the generated
        /// Dataset-JSON document, which a case states only whole.
        #[test]
        fn integer_terminology_items_admit_equal_float_published_values() {
            let temp = Temp::fixture("sdtm-dm-metadata", true);
            let spec = fs::read_to_string(temp.0.join("spec.yaml"))
        .unwrap()
        .edit("name: AGE\n    type: int", "name: AGE\n    type: float")
        .edit(
            "core: Exp\n      role: Record Qualifier\n      length: 3",
            "core: Exp\n      role: Record Qualifier\n      length: 3\n      significant_digits: 3\n      codelist: AGE_VALUES",
        );
            fs::write(temp.0.join("spec.yaml"), spec).unwrap();
            temp.edit_project(&[(
                "study:\n",
                "- codelists:\n  - id: AGE_VALUES\n    name: Ages\n    data_type: float\n    items: [{value: 34}, {value: 67}, {value: 52}]\nstudy:\n",
            )]);
            let result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let data: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode_slice(result.artifacts().next().unwrap().1)
                    .unwrap();
            assert_eq!(data["columns"][5]["dataType"], "float");
            assert_eq!(data["rows"][0][5].as_f64(), Some(34.0));
        }
        /// Stays beside the replay: it compares fragments and a count of the
        /// generated Define-XML, which a case states only whole.
        #[test]
        fn unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files() {
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            temp.edit_project(&[
                ("  transport: dataset_json\n", ""),
                ("study:\n", "- codelists:\n  - id: UNUSED\n    name: Used only in another submission\n    data_type: text\n    items: [{value: OTHER}]\nstudy:\n"),
            ]);
            let result = temp.run("2026-09-11T00:00:00");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            let xml = std::str::from_utf8(result.artifacts().next().unwrap().1).unwrap();
            assert!(!xml.contains("UNUSED"));
            assert_eq!(xml.matches("<CodeList ").count(), 4);
            assert!(xml.contains("xlink:href=\"acrf.pdf\""));
            assert!(!temp.0.join("acrf.pdf").exists());
            assert!(!temp.0.join("input").exists());
        }
        /// Stays beside the replay by design (#3778): it finds its issue by a
        /// substring of its first spec path, and asserts that a plain lookup
        /// raises no such issue. The DM refutations are `shared_define.json`'s
        /// `an_allowed_values_check_beside_its_codelist_is_redundant`,
        /// `an_assembled_identifier_refutes_a_protocol_origin` and
        /// `a_label_in_the_metadata_mapping_is_a_reserved_key`.
        #[test]
        fn a_plain_dictionary_lookup_refutes_no_collected_origin_and_a_deriving_lookup_does() {
            // The coded terms are a plain dictionary lookup, which copies the
            // record it selects and refutes no origin; a lookup that derives
            // fields computes what it exposes and refutes Collected.
            for (derives, refuted) in [(false, false), (true, true)] {
                let temp = Temp::fixture("sdtm-ae-coding", false);
                let mut spec = fs::read_to_string(temp.0.join("spec.yaml"))
                    .unwrap()
                    .replace(
                    "codelist: MEDDRA\n      origins:\n        - type: Assigned\n          source: Sponsor",
                    "codelist: MEDDRA\n      origins:\n        - type: Collected\n          source: Vendor",
                );
                if derives {
                    spec = spec.edit(
                        "    absent: NOT CODED\n",
                        "    absent: NOT CODED\n    derivations:\n      LLTUPPER: {str_case: {source: LLTNAME, to: upper}}\n",
                    );
                }
                fs::write(temp.0.join("spec.yaml"), spec).unwrap();
                let result = temp.run("2026-09-30T00:00:00");
                assert_eq!(
                    result
                        .issues()
                        .iter()
                        .any(|i| i.condition() == "origin_contradicts_derivation"
                            && i.spec_paths[0].contains("AEDECOD")),
                    refuted,
                    "{:?}",
                    result.issues()
                );
            }
        }
        /// Rewrite the DM fixture's project to compose Define-XML alone for
        /// one section under `context`. A send section lists a specification
        /// of its own model (SUB-23).
        fn metadata_only(temp: &Temp, section: &str, context: &str) {
            let guide = if section == "send" {
                "SENDIG"
            } else {
                "SDTMIG"
            };
            let header = format!("{section}:\n  context: {context}\n");
            let standard = format!("  - name: {guide}\n");
            temp.edit_project(&[
                ("  transport: dataset_json\n", ""),
                ("sdtm:\n", header.as_str()),
                ("  - name: SDTMIG\n", standard.as_str()),
            ]);
            if section == "send" {
                replace_spec(temp, "model: SDTM\n", "model: SEND\n");
            }
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
        /// Stays beside the replay: it compares fragments of the generated
        /// Define-XML, which a case states only whole. Under Submission the
        /// page is required: `shared_define.json`'s
        /// `a_collected_origin_without_its_form_is_refused_under_submission`
        /// and `a_collected_origin_citing_its_form_without_a_page_is_refused_under_submission`.
        #[test]
        fn collected_origins_cite_their_form_without_a_page_under_other() {
            // SITEID is collected by the investigator on the first form page.
            let paged = "role: Record Qualifier\n      length: 10\n      origins:\n        - type: Collected\n          source: Investigator\n          documents:\n            - document: acrf\n              pages: {type: PhysicalRef, refs: \"1\"}\n";
            let pageless = "role: Record Qualifier\n      length: 10\n      origins:\n        - type: Collected\n          source: Investigator\n";
            let unpaged = format!("{pageless}          documents: [acrf]\n");
            for replacement in [pageless, unpaged.as_str()] {
                let temp = Temp::fixture("sdtm-dm-metadata", false);
                replace_spec(&temp, paged, replacement);
                metadata_only(&temp, "sdtm", "Other");
                let run = temp.run("2026-09-11T00:00:00");
                // Under Other the reference to the form needs no page.
                assert!(run.issues().is_empty(), "{:?}", run.issues());
                let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
                assert!(xml.contains("def:Context=\"Other\""));
                assert!(item(xml, "IT.DM.SITEID").contains("<def:DocumentRef leafID=\"LF.acrf\"/>"));
            }
        }
        /// Stays beside the replay: it compares fragments of the generated
        /// Define-XML, which a case states only whole. The sources neither
        /// family admits are `shared_define.json`'s
        /// `a_not_available_origin_is_not_admitted_under_sdtm`,
        /// `a_not_available_origin_with_a_source_is_not_admitted_under_sdtm`,
        /// `an_assigned_origin_from_the_investigator_is_not_admitted_under_send`
        /// and `a_not_available_origin_with_a_source_is_not_admitted_under_send`.
        #[test]
        fn send_keeps_an_unsourced_not_available_origin_and_an_unsourced_assigned_one() {
            let assigned =
                "codelist: AGEU\n      origins:\n        - type: Assigned\n          source: Sponsor\n";
            let not_available = "codelist: AGEU\n      origins:\n        - type: Not Available\n          description: Age units were not collected.\n";
            // send takes the sources the sdtm table admits, or none, and keeps
            // a Not Available origin that declares no source and says why.
            let send = |temp: &Temp| {
                Define::prepare(
                    temp.0.join("project.yaml").to_str().unwrap(),
                    Submission::Send,
                    Some("2026-09-11T00:00:00"),
                    "0.3.0",
                    Some(Language::Python),
                    None,
                )
            };
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            replace_spec(&temp, assigned, not_available);
            metadata_only(&temp, "send", "Submission");
            let run = send(&temp);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
            let ageu = item(xml, "IT.DM.AGEU");
            assert!(
                ageu.contains("<def:Origin Type=\"Not Available\">"),
                "{ageu}"
            );
            assert!(ageu.contains(">Age units were not collected.</TranslatedText>"));
            assert!(item(xml, "IT.DM.SITEID")
                .contains("<def:Origin Type=\"Collected\" Source=\"Investigator\">"));
            let temp = Temp::fixture("sdtm-dm-metadata", false);
            metadata_only(&temp, "send", "Submission");
            replace_spec(
                &temp,
                assigned,
                "codelist: AGEU\n      origins:\n        - type: Assigned\n",
            );
            let run = send(&temp);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            let xml = std::str::from_utf8(run.artifacts().next().unwrap().1).unwrap();
            assert!(item(xml, "IT.DM.AGEU").contains("<def:Origin Type=\"Assigned\"/>"));
        }
        /// A document of `shared_define.json`, which holds the cross-section
        /// study's files for its cases and this test alike.
        fn document(name: &str) -> String {
            let fixture: yamaa_core::json::Value = yamaa_adapters::json_wire::decode(include_str!(
                "fixtures/diagnostics/shared_define.json"
            ))
            .unwrap();
            fixture["documents"][name].as_str().unwrap().to_owned()
        }
        /// Stays beside the replay: it compares fragments of the generated
        /// Define-XML, which a case states only whole. The refusals across
        /// the sections are `shared_define.json`'s
        /// `a_predecessor_column_keeps_its_predecessors_label`,
        /// `a_published_name_outside_the_study_inventory_fails_its_recheck` and
        /// `without_the_sdtm_section_a_predecessor_names_no_dataset_of_the_study`.
        #[test]
        fn predecessor_labels_and_the_dataset_inventory_span_every_section() {
            // ADLB copies LB from the sdtm section: USUBJID keeps its name and
            // label, PARAMCD renames LBTESTCD with its own label, and SRCDOM
            // names the sdtm section's LB.
            let temp = Temp::new();
            fs::create_dir(temp.0.join("sdtm")).unwrap();
            fs::write(temp.0.join("sdtm/lb.yaml"), document("study_lb")).unwrap();
            fs::write(temp.0.join("adlb.yaml"), document("study_adlb")).unwrap();
            fs::write(
                temp.0.join("adlb.csv"),
                "USUBJID,PARAMCD,SRCDOM\nS01-001,GLUC,LB\n",
            )
            .unwrap();
            fs::write(
                temp.0.join("project.yaml"),
                document("cross_section_project"),
            )
            .unwrap();
            let mut run = Define::prepare(
                temp.0.join("project.yaml").to_str().unwrap(),
                Submission::Adam,
                Some("2026-09-30T00:00:00"),
                "0.3.0",
                Some(Language::Python),
                None,
            );
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
        }
    }
}
