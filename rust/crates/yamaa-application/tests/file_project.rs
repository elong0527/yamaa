#![cfg(feature = "yaml")]
//! File contracts: file project, file publication and file resources.
mod file {
    mod file_project {
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::{
            file_resources::{Error as ResourceError, Resources},
            project_source::{CaptureFailure, Failure as EnvironmentFailure, Origin},
        };
        use yamaa_application::{
            file_preparation::FileSpecification,
            file_project::{self, Error},
            specification_run::BoundaryFailure,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation::{Failure, LockObservation},
            specification_run::PortError,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::LockKind,
            project_function::Language,
            table::{TableAccess, ValueRef},
            value::Value,
        };

        struct Study(crate::support::Scratch);
        impl Study {
            fn empty() -> Self {
                let path = crate::support::Scratch::canonical("file-project");
                for name in ["env", "entry", "parent", "outside"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                Self(path)
            }
            /// The project lists its codelist file and its function file and
            /// declares the Python lock, each relative to itself (D8).
            fn new() -> Self {
                let study = Self::empty();
                study.write("project.yaml", PROJECT);
                study.write("uv.lock", "version = 1\npackage = []\n");
                study.write("env/id.yaml", FUNCTION);
                study.write(
                    "ct.yaml",
                    "yamaa: \"1.0\"\ntype: codelists\ncodelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
                );
                study.write("entry/domain.yaml", DOMAIN);
                study
            }
            fn text(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().into()
            }
            /// Resource identities use portable separators; the host path passed
            /// to preparation keeps its native spelling.
            fn identity(&self, name: &str) -> String {
                let path = self.text(name);
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn write(&self, name: &str, bytes: &str) {
                fs::write(self.0.join(name), bytes).unwrap();
            }
            fn prepare(&self, root: &str, roots: &[String]) -> Result<FileSpecification, Error> {
                file_project::prepare(
                    Resources::new(root, &self.text("entry"), roots).unwrap(),
                    "domain.yaml",
                    "../project.yaml",
                    Language::Python,
                    yamaa_adapters::document::Closures::shipped().unwrap(),
                )
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                if let Err(error) = fs::remove_dir_all(&self.0) {
                    if !std::thread::panicking() {
                        panic!(
                            "failed to remove study directory {}: {error}",
                            self.0.display()
                        );
                    }
                }
            }
        }
        const DOMAIN: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";
        const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\ncodelists: [ct.yaml]\nfunctions: [env/id.yaml]\nenvironments: {python: {lock: uv.lock}}\n";
        const FUNCTION: &str = "yamaa: \"1.0\"\ntype: function\nname: id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\nimplementations: {python: program.id}\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n";

        #[test]
        fn public_preparation_accepts_native_absolute_environment_paths_without_study_reads() {
            // Seam: the public preparation's capture reads, which a check does not return.
            let study = Study::new();
            study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
            let project = yamaa_application::project_application::prepare(
                &study.text("entry/domain.yaml"),
                &study.text("project.yaml"),
                Language::Python,
            )
            .unwrap();
            assert!(project.run().check_diagnostics().is_empty());
            assert!(!study.0.join("entry/input.csv").exists());
            // Entry, project, lock, function and codelist metadata only.
            assert_eq!(project.capture_reads(), 5);
        }

        #[derive(Default)]
        struct Activation {
            trace: Vec<&'static str>,
            reject: bool,
            reject_binding: bool,
            mismatch: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = &'static str;
            fn verify_lock(
                &mut self,
                _: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Self::Error> {
                assert_eq!(lock.reference.kind, LockKind::Uv);
                assert_eq!(lock.reference.written, "uv.lock");
                assert_eq!(functions[0].call, "program.id");
                self.trace.push("lock");
                if self.reject {
                    Err("original lock rejection")
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Self::Error> {
                self.trace.push("bind");
                if self.reject_binding {
                    Err("original binding rejection")
                } else {
                    Ok(())
                }
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Self::Error>> {
                self.trace.push("call");
                let [Argument { name: "x", value }] = args else {
                    panic!("exact named argument")
                };
                match value {
                    ValueRef::Int(n) => Ok(Value::Int(if self.mismatch { n + 1 } else { *n })),
                    ValueRef::Missing => Ok(Value::Missing),
                    _ => panic!("integer or admitted missing"),
                }
            }
        }

        #[test]
        fn project_report_uses_held_external_function_origins_after_metadata_files_change() {
            // Seam: metadata files changed after preparation, which a case cannot change between
            // its preparation and its build.
            let study = Study::new();
            let identity = |name| study.identity(name);
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            let reads = project.capture_reads();
            let mut activation = Activation {
                mismatch: true,
                ..Default::default()
            };
            let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
            study.write("env/id.yaml", "changed after preparation");
            study.write("project.yaml", "changed root after preparation");
            let result = yamaa_application::specification_report::build_result(
                project.run(),
                &attempt,
                yamaa_application::specification_report::Identity {
                    runtime: "test",
                    runtime_version: "1",
                    engine_version: "0.1.0",
                    example: "external-project",
                    specification: "domain.yaml",
                    base_directory: "entry",
                },
                &[],
            )
            .unwrap();
            assert_eq!(result.issues().len(), 1);
            assert_eq!(result.issues()[0].spec_paths, ["tests[0]"]);
            let context = yamaa_core::json::Value::Object(result.issues()[0].context.clone());
            // The function file is the source, the project the entry and its
            // listing the environment path (FUNC-12).
            assert_eq!(context["source"], identity("env/id.yaml"));
            assert_eq!(context["entry"], identity("project.yaml"));
            assert_eq!(context["environment_path"], "functions[0]");
            assert_eq!(context["actual"], 8);
            assert_eq!(context["expected"], 7);
            assert_eq!(
                result.observations()["source_reads"],
                yamaa_adapters::json!([])
            );
            assert_eq!(activation.trace, ["lock", "bind", "call", "call"]);
            assert_eq!(reads, 5);
            assert_eq!(
                project.run().captured_environment().captures()[0]
                    .document
                    .source()
                    .identity,
                identity("env/id.yaml")
            );
        }

        #[test]
        fn native_project_retains_metadata_and_reuses_exact_snapshots_with_fresh_activation() {
            // Seam: one prepared project built twice after its files change: its held bytes by
            // address, the shared snapshot and the capture reads.
            let study = Study::new();
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 5);
            let captures = project.run().captured_environment();
            assert_eq!(
                captures.root().source().bytes,
                fs::read(study.0.join("project.yaml")).unwrap()
            );
            assert_eq!(
                captures.lock().unwrap().source.bytes,
                b"version = 1\npackage = []\n"
            );
            // The project's function file and codelist file are its captures.
            assert_eq!(captures.captures().len(), 2);
            assert_eq!(captures.captures()[0].origin, Origin::FunctionFile(0));
            assert!(captures.captures()[0]
                .document
                .source()
                .identity
                .ends_with("/env/id.yaml"));
            assert_eq!(captures.captures()[1].origin, Origin::Codelist(0));
            assert_eq!(
                captures.captures()[1].document.source().identity,
                study.identity("ct.yaml")
            );
            assert!(project.run().check_diagnostics().is_empty());
            let root_bytes = captures.root().source().bytes.as_ptr();
            let lock_bytes = captures.lock().unwrap().source.bytes.as_ptr();
            let definition = project.run().environment().functions()[0]
                .definition()
                .tests
                .as_ptr();
            // Metadata bytes belong to the prepared capability, independently of later
            // filesystem changes. Study bytes are first captured only after activation.
            study.write("project.yaml", "[changed");
            study.write("env/id.yaml", "[changed");
            study.write("uv.lock", "changed");
            study.write("entry/domain.yaml", "[changed");
            study.write("entry/input.csv", "ID\n1\n");
            let mut activation = Activation::default();
            let first = project.build(&mut run_scope, &mut activation, Scope::default());
            assert!(first.boundary.is_ok());
            assert_eq!(first.activation.lock, LockObservation::Verified);
            assert_eq!(run_scope.capture_reads(), 6);
            assert_eq!(
                first
                    .dataset
                    .result
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .dataset
                    .rows(),
                &[vec![Value::Int(1), Value::Int(1)]]
            );
            let second = project.build(&mut run_scope, &mut activation, Scope::default());
            assert!(second.boundary.is_ok());
            assert_eq!(run_scope.capture_reads(), 6);
            assert_eq!(second.dataset.sources[0].read.snapshots_created, Some(0));
            assert!(std::sync::Arc::ptr_eq(
                first.dataset.sources[0].snapshot.as_ref().unwrap(),
                second.dataset.sources[0].snapshot.as_ref().unwrap()
            ));
            assert_eq!(
                second.dataset.sources[0]
                    .table
                    .as_ref()
                    .unwrap()
                    .cell(0, 0)
                    .unwrap(),
                ValueRef::Int(1)
            );
            assert_eq!(
                activation.trace,
                ["lock", "bind", "call", "call", "call", "lock", "bind", "call", "call", "call"]
            );
            assert_eq!(
                project
                    .run()
                    .captured_environment()
                    .root()
                    .source()
                    .bytes
                    .as_ptr(),
                root_bytes
            );
            assert_eq!(
                project
                    .run()
                    .captured_environment()
                    .lock()
                    .unwrap()
                    .source
                    .bytes
                    .as_ptr(),
                lock_bytes
            );
            assert_eq!(
                project.run().environment().functions()[0]
                    .definition()
                    .tests
                    .as_ptr(),
                definition
            );
        }

        /// Seam: held environment locks, typed activation boundaries and observations,
        /// and dataset/capture state after refusal; public replay cases own rows and logs.
        #[test]
        fn project_functions_build_without_any_adjacent_packaging_lock() {
            let study = Study::new();
            study.write(
                "project.yaml",
                &PROJECT.edit("environments: {python: {lock: uv.lock}}\n", ""),
            );
            fs::remove_file(study.0.join("uv.lock")).unwrap();
            study.write("entry/input.csv", "ID\n1\n");
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert!(project.run().environment().lock().is_none());
            assert!(project.run().captured_environment().lock().is_none());
            let mut activation = Activation {
                reject: true,
                ..Default::default()
            };
            let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
            assert!(attempt.boundary.is_ok());
            assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
        }
        /// Seam: held environment locks, typed activation boundaries and observations,
        /// and dataset/capture state after refusal; public replay cases own rows and logs.
        #[test]
        fn functions_without_a_lock_still_reject_bindings_and_cases_before_study_reads() {
            let study = Study::new();
            study.write(
                "project.yaml",
                &PROJECT.edit("environments: {python: {lock: uv.lock}}\n", ""),
            );
            fs::remove_file(study.0.join("uv.lock")).unwrap();
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            let reads = project.capture_reads();
            assert!(!study.0.join("entry/input.csv").exists());
            for reject_binding in [true, false] {
                let mut activation = Activation {
                    reject: true,
                    reject_binding,
                    mismatch: true,
                    ..Default::default()
                };
                let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
                if reject_binding {
                    assert!(matches!(
                        attempt.boundary,
                        Err(BoundaryFailure::Activation(Failure::Bindings(_)))
                    ));
                } else {
                    assert!(matches!(
                        attempt.boundary,
                        Err(BoundaryFailure::Activation(Failure::Tests(_)))
                    ));
                }
                assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
                assert!(attempt.dataset.sources.is_empty());
                assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
                assert_eq!(run_scope.capture_reads(), reads);
            }
        }
        /// Seam: held environment locks, typed activation boundaries and observations,
        /// and dataset/capture state after refusal; public replay cases own rows and logs.
        #[test]
        fn native_project_activation_rejection_precedes_even_a_missing_study_file() {
            let study = Study::new();
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            let mut activation = Activation {
                reject: true,
                ..Default::default()
            };
            let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
            assert!(matches!(
                attempt.boundary,
                Err(BoundaryFailure::Activation(Failure::Lock(
                    "original lock rejection"
                )))
            ));
            assert!(attempt.dataset.sources.is_empty());
            assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
            assert_eq!(run_scope.capture_reads(), 5);
        }

        /// Each build gives the host-functions port the exact lock bytes the
        /// prepared project holds, without copying them.
        #[test]
        fn each_build_verifies_the_lock_bytes_the_prepared_project_holds() {
            // Seam: the lock bytes the port receives, by address, on each of two builds.
            struct Held {
                expected: *const u8,
                inner: Activation,
            }
            impl HostFunctions for Held {
                type Handle = ();
                type Error = &'static str;
                fn verify_lock(
                    &mut self,
                    language: Language,
                    lock: Lock<'_>,
                    functions: &[ProjectFunctionIdentity],
                ) -> Result<(), Self::Error> {
                    assert_eq!(lock.bytes.as_ptr(), self.expected);
                    assert_eq!(lock.bytes, b"version = 1\npackage = []\n");
                    self.inner.verify_lock(language, lock, functions)
                }
                fn bind(
                    &mut self,
                    identity: &ProjectFunctionIdentity,
                    signature: &LogicalSignature,
                ) -> Result<(), Self::Error> {
                    self.inner.bind(identity, signature)
                }
                fn invoke(
                    &mut self,
                    handle: &(),
                    args: &[Argument<'_>],
                ) -> Result<Value, HostError<Self::Error>> {
                    self.inner.invoke(handle, args)
                }
            }
            let study = Study::new();
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            let expected = project
                .run()
                .captured_environment()
                .lock()
                .unwrap()
                .source
                .bytes
                .as_ptr();
            study.write("entry/input.csv", "ID\n1\n");
            let mut port = Held {
                expected,
                inner: Activation::default(),
            };
            for _ in 0..2 {
                let attempt = project.build(&mut run_scope, &mut port, Scope::default());
                assert!(attempt.boundary.is_ok());
                assert_eq!(attempt.activation.lock, LockObservation::Verified);
                assert!(attempt.dataset.result.unwrap().result.is_ok());
            }
            let verified = port.inner.trace.iter().filter(|&&effect| effect == "lock");
            assert_eq!(verified.count(), 2, "one verification per build");
            assert_eq!(run_scope.capture_reads(), 6);
        }

        #[test]
        fn native_project_keeps_approved_roots_for_metadata_and_study_without_expanding_them() {
            // Seam: the native resources' approved roots, which the file route does not take.
            let study = Study::new();
            // The selected entry root does not authorize the project beside it. The
            // same explicit approved fallback used by ordinary builds makes it available.
            let Err(Error::Resource(ResourceError::OutsideRoots)) =
                study.prepare(&study.text("entry"), &[])
            else {
                panic!("project outside roots")
            };
            let project = study
                .prepare(&study.text("entry"), &[study.text("")])
                .unwrap();
            let mut run_scope = project.run_scope(None);
            study.write("input.csv", "ID\n1\n");
            let attempt =
                project.build(&mut run_scope, &mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert!(attempt.dataset.sources[0].read.source.path == "input.csv");

            // A function file the project lists above the approved root is
            // refused.
            study.write(
                "project.yaml",
                &PROJECT.edit(
                    "functions: [env/id.yaml]",
                    "functions: [../unapproved.yaml]",
                ),
            );
            let Err(Error::Environment(EnvironmentFailure::Rejected(rejected))) =
                study.prepare(&study.text(""), &[])
            else {
                panic!("unapproved definition")
            };
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::FunctionFile(0));
            assert_eq!(rejected.sources[0].written, "../unapproved.yaml");
            assert!(matches!(
                rejected.sources[0].error,
                CaptureFailure::Port(ResourceError::OutsideRoots)
            ));
            assert!(rejected.lock.is_some());
        }

        #[test]
        fn native_project_inheritance_retains_parent_authorship_and_selected_entry_source_paths() {
            // Seam: the prepared document's parents, its written and resolved source paths and the
            // capture reads.
            let study = Study::new();
            study.write(
                "parent/base.yaml",
                DOMAIN.edit("input.csv", "data.csv").as_str(),
            );
            study.write("parent/data.csv", "ID\n1\n");
            study.write(
                "entry/domain.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: ../parent/base.yaml\n",
            );
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 6);
            let document = project.run().document();
            assert_eq!(document.parents().len(), 1);
            assert_eq!(document.written_source_path("SRC"), Some("data.csv"));
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            let attempt =
                project.build(&mut run_scope, &mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert_eq!(run_scope.capture_reads(), 7);
        }

        #[test]
        fn selected_ordinary_environment_owner_preserves_fresh_activation_and_inherited_data_origins(
        ) {
            // Seam: one selection built three times, the last two sharing a snapshot, and the bytes
            // it retains.
            use yamaa_application::file_workflow;
            let study = Study::new();
            study.write("parent/base.yaml", &DOMAIN.edit("input.csv", "data.csv"));
            study.write(
                "entry/domain.yaml",
                "yamaa: \"1.0\"\ntype: spec\nparent: ../parent/base.yaml\n",
            );
            let project = file_workflow::select_with_resources(
                Resources::new(&study.text(""), &study.text("entry"), &[]).unwrap(),
                "domain.yaml",
                "../project.yaml",
                Language::Python,
                yamaa_adapters::document::Closures::shipped().unwrap(),
            )
            .unwrap();
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 6);
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            // A fresh activation still refuses before the absent authored study source.
            let mut failed = Activation {
                reject: true,
                ..Default::default()
            };
            let rejection = project.build(&mut run_scope, &mut failed, Scope::default());
            assert_eq!(failed.trace, ["lock"]);
            assert!(rejection.dataset.sources.is_empty());
            assert_eq!(run_scope.capture_reads(), 6);
            study.write("parent/data.csv", "ID\n1\n");
            let mut host = Activation::default();
            let first = project.build(&mut run_scope, &mut host, Scope::default());
            let second = project.build(&mut run_scope, &mut host, Scope::default());
            assert!(first.boundary.is_ok() && second.boundary.is_ok());
            assert_eq!(
                host.trace,
                ["lock", "bind", "call", "call", "call", "lock", "bind", "call", "call", "call"]
            );
            assert_eq!(run_scope.capture_reads(), 7);
            assert!(std::sync::Arc::ptr_eq(
                first.dataset.sources[0].snapshot.as_ref().unwrap(),
                second.dataset.sources[0].snapshot.as_ref().unwrap()
            ));
            let retained = project.run();
            // The original tests and lock stay in this run; repeated builds
            // never replaced them.
            assert_eq!(retained.captured_environment().captures().len(), 2);
            assert_eq!(
                retained.document().parents()[0].source().bytes,
                DOMAIN.edit("input.csv", "data.csv").as_bytes()
            );
            assert_eq!(
                retained.captured_environment().lock().unwrap().source.bytes,
                b"version = 1\npackage = []\n"
            );
        }

        #[test]
        fn a_section_that_lists_only_specs_reads_none_of_them() {
            // Seam: the prepared project's capture reads. The run's issues and
            // output are project_activation.json's
            // a_section_that_lists_only_specs_admits_a_run_without_study_standard_or_define.
            let study = Study::new();
            study.write(
                "project.yaml",
                &format!("{PROJECT}sdtm: {{specs: [entry/domain.yaml]}}\n"),
            );
            study.write("entry/input.csv", "ID\n1\n");
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            // Define alone composes the section; a run reads none of its specs.
            assert_eq!(project.capture_reads(), 5);
            let attempt =
                project.build(&mut run_scope, &mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert_eq!(run_scope.capture_reads(), 6);
        }
    }
    mod file_publication {
        #![cfg(unix)]
        use std::{fs, path::PathBuf};
        use yamaa_adapters::file_publication::{Error, Publisher};
        struct Directory(crate::support::Scratch);
        impl Directory {
            fn new() -> Self {
                let directory = crate::support::Scratch::new("native-publication");
                Self(directory)
            }
            fn target(&self) -> PathBuf {
                self.0.join("output.csv")
            }
            fn publisher(&self) -> Publisher {
                Publisher::new("declared.csv", self.target().to_str().unwrap()).unwrap()
            }
            fn names(&self) -> Vec<String> {
                let mut names: Vec<_> = fs::read_dir(&self.0)
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
                    .collect();
                names.sort();
                names
            }
        }
        #[test]
        fn physical_target_comparison_survives_selected_parent_aliases_and_rename() {
            let directory = Directory::new();
            let selected = directory.0.join("selected");
            let moved = directory.0.join("moved");
            fs::create_dir(&selected).unwrap();
            let first =
                Publisher::new("one", selected.join("output.csv").to_str().unwrap()).unwrap();
            fs::rename(&selected, &moved).unwrap();
            let alias = Publisher::new("two", moved.join("output.csv").to_str().unwrap()).unwrap();
            assert!(first.same_target(&alias).unwrap());
            let other_name =
                Publisher::new("three", moved.join("other.csv").to_str().unwrap()).unwrap();
            assert!(!first.same_target(&other_name).unwrap());
            fs::create_dir(&selected).unwrap();
            let replacement =
                Publisher::new("four", selected.join("output.csv").to_str().unwrap()).unwrap();
            assert!(!first.same_target(&replacement).unwrap());
            assert!(fs::read_dir(&selected).unwrap().next().is_none());
            assert!(fs::read_dir(&moved).unwrap().next().is_none());
            // Replacement names stay distinct even if current files share an inode.
            fs::write(moved.join("output.csv"), b"prior").unwrap();
            fs::hard_link(moved.join("output.csv"), moved.join("other.csv")).unwrap();
            assert!(!first.same_target(&other_name).unwrap());
        }
        #[cfg(target_os = "macos")]
        #[test]
        fn macos_absent_case_aliases_and_unproven_unicode_names_are_refused_without_writes() {
            let directory = Directory::new();
            let first = directory.publisher();
            let alias =
                Publisher::new("alias", directory.0.join("OUTPUT.csv").to_str().unwrap()).unwrap();
            // The release host's ordinary APFS volume is case-insensitive. A native
            // witness establishes that fact independently of the comparer under test.
            fs::write(directory.0.join("case-probe"), b"witness").unwrap();
            let case_sensitive = !directory.0.join("CASE-PROBE").exists();
            fs::remove_file(directory.0.join("case-probe")).unwrap();
            assert_eq!(first.same_target(&alias).unwrap(), !case_sensitive);
            let unicode =
                Publisher::new("unicode", directory.0.join("\u{e9}.csv").to_str().unwrap())
                    .unwrap();
            assert!(matches!(
                first.same_target(&unicode),
                Err(Error::InvalidTarget)
            ));
            assert!(directory.names().is_empty());
        }
        #[test]
        fn explicit_saves_replace_complete_bytes_and_leave_no_temporary_files() {
            let directory = Directory::new();
            fs::write(directory.target(), b"old").unwrap();
            fs::hard_link(directory.target(), directory.0.join("old.csv")).unwrap();
            let mut publisher = directory.publisher();
            for content in [
                b"complete\n".as_slice(),
                b"".as_slice(),
                b"UTF-8: \xc3\xa9\n".as_slice(),
            ] {
                publisher.publish("declared.csv", content).unwrap();
                assert_eq!(fs::read(directory.target()).unwrap(), content);
                assert_eq!(fs::read(directory.0.join("old.csv")).unwrap(), b"old");
                assert_eq!(directory.names(), vec!["old.csv", "output.csv"]);
            }
        }
        #[test]
        fn paths_and_capacity_are_checked_before_any_target_change() {
            let directory = Directory::new();
            fs::write(directory.target(), b"old").unwrap();
            let mut publisher = directory.publisher();
            assert!(matches!(
                publisher.publish("different.csv", b"new"),
                Err(Error::PathMismatch)
            ));
            // Byte-capacity exact/one-over coverage lives with the private selected
            // limit in file_publication's unit controls; no giant payload is allocated.
            assert_eq!(fs::read(directory.target()).unwrap(), b"old");
            assert_eq!(directory.names(), vec!["output.csv"]);
            assert!(matches!(
                Publisher::new("declared.csv", "relative.csv"),
                Err(Error::InvalidTarget)
            ));
            // A directory where the file belongs cannot be reached (FILE-7).
            assert!(matches!(
                Publisher::new("declared.csv", directory.0.to_str().unwrap()),
                Err(Error::Unreachable(
                    yamaa_core::resource::ResourceFailure::NotRegularFile
                ))
            ));
        }
        #[test]
        fn directory_suffixes_are_not_normalized_into_file_targets() {
            let directory = Directory::new();
            fs::write(directory.target(), b"old").unwrap();
            for name in ["new.csv", "output.csv"] {
                for suffix in ["/", "/.", "/.."] {
                    let target = format!("{}{suffix}", directory.0.join(name).display());
                    assert!(
                        matches!(
                            Publisher::new("declared.csv", &target),
                            Err(Error::InvalidTarget)
                        ),
                        "directory spelling was accepted: {target}"
                    );
                    assert_eq!(fs::read(directory.target()).unwrap(), b"old");
                    assert_eq!(directory.names(), vec!["output.csv"]);
                }
            }
        }
        #[test]
        fn native_publication_respects_an_exclusive_selected_parent_lock() {
            use rustix::fs::{self as native, FlockOperation, Mode, OFlags};
            let directory = Directory::new();
            fs::write(directory.target(), b"old").unwrap();
            let lock = native::open(
                directory.0.path(),
                OFlags::RDONLY | OFlags::DIRECTORY,
                Mode::empty(),
            )
            .unwrap();
            native::flock(&lock, FlockOperation::LockExclusive).unwrap();
            let mut publisher = directory.publisher();
            assert!(matches!(
                publisher.publish("declared.csv", b"new"),
                Err(Error::Io(_))
            ));
            assert_eq!(fs::read(directory.target()).unwrap(), b"old");
            assert_eq!(directory.names(), vec!["output.csv"]);
            native::flock(&lock, FlockOperation::Unlock).unwrap();
            publisher.publish("declared.csv", b"new").unwrap();
            assert_eq!(fs::read(directory.target()).unwrap(), b"new");
            assert_eq!(directory.names(), vec!["output.csv"]);
        }
        #[test]
        fn another_completed_save_is_intentionally_replaced_by_the_next_explicit_save() {
            let directory = Directory::new();
            let mut first = directory.publisher();
            let mut second = directory.publisher();
            second.publish("declared.csv", b"intervening").unwrap();
            first
                .publish("declared.csv", b"selected replacement")
                .unwrap();
            assert_eq!(
                fs::read(directory.target()).unwrap(),
                b"selected replacement"
            );
            assert_eq!(directory.names(), vec!["output.csv"]);
        }
        #[test]
        fn a_new_link_or_directory_is_refused_without_following_or_replacing_it() {
            let directory = Directory::new();
            let mut publisher = directory.publisher();
            let other = directory.0.join("other");
            fs::write(&other, b"retained").unwrap();
            std::os::unix::fs::symlink(&other, directory.target()).unwrap();
            assert!(matches!(
                publisher.publish("declared.csv", b"new"),
                Err(Error::Unreachable(
                    yamaa_core::resource::ResourceFailure::Symlink
                ))
            ));
            assert_eq!(fs::read(&other).unwrap(), b"retained");
            assert!(fs::symlink_metadata(directory.target())
                .unwrap()
                .file_type()
                .is_symlink());
            fs::remove_file(directory.target()).unwrap();
            fs::create_dir(directory.target()).unwrap();
            assert!(matches!(
                publisher.publish("declared.csv", b"new"),
                Err(Error::Unreachable(
                    yamaa_core::resource::ResourceFailure::NotRegularFile
                ))
            ));
            assert_eq!(directory.names().len(), 2);
        }
        #[test]
        fn selected_parent_descriptor_retains_authority_after_renaming() {
            let directory = Directory::new();
            let parent = directory.0.join("selected");
            fs::create_dir(&parent).unwrap();
            let mut publisher =
                Publisher::new("declared.csv", parent.join("output.csv").to_str().unwrap())
                    .unwrap();
            let moved = directory.0.join("moved");
            fs::rename(&parent, &moved).unwrap();
            let other = directory.0.join("other");
            fs::create_dir(&other).unwrap();
            std::os::unix::fs::symlink(&other, &parent).unwrap();
            publisher.publish("declared.csv", b"selected").unwrap();
            assert_eq!(fs::read(moved.join("output.csv")).unwrap(), b"selected");
            assert!(!other.join("output.csv").exists());
        }
        #[test]
        fn permission_failure_preserves_the_prior_artifact_and_allows_explicit_retry() {
            use std::os::unix::fs::PermissionsExt;
            let directory = Directory::new();
            fs::write(directory.target(), b"old").unwrap();
            let mut publisher = directory.publisher();
            let permissions = fs::metadata(&directory.0).unwrap().permissions();
            fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o500)).unwrap();
            let probe = directory.0.join("dac-probe");
            if let Ok(file) = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)
            {
                drop(file);
                fs::set_permissions(&directory.0, permissions).unwrap();
                fs::remove_file(probe).unwrap();
                eprintln!("process bypasses DAC; publication permission case not exercised");
                return;
            }
            let result = publisher.publish("declared.csv", b"new");
            fs::set_permissions(&directory.0, permissions).unwrap();
            assert!(matches!(result, Err(Error::Io(_))));
            assert_eq!(fs::read(directory.target()).unwrap(), b"old");
            assert_eq!(directory.names(), vec!["output.csv"]);
            publisher.publish("declared.csv", b"new").unwrap();
            assert_eq!(fs::read(directory.target()).unwrap(), b"new");
        }
    }
    mod file_resources {
        #[cfg(unix)]
        use std::path::Path;
        use std::{fs, path::PathBuf};
        use yamaa_adapters::file_resources::{Error, Resources};
        use yamaa_core::resolver::{self, Name, Resolver};
        use yamaa_core::resource::ResourceFailure;
        use yamaa_core::specification::{SourceDeclaration, SourceProfile};

        /// The finding a failed read of an input reports through the
        /// resolver port, or `None` when the failure stays opaque.
        fn input_failure(resources: &Resources, error: &Error) -> Option<ResourceFailure> {
            let input = SourceDeclaration {
                name: "SRC".into(),
                path: "source.csv".into(),
                types: vec![],
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            resources
                .failure(Name::Input(&input), error)
                .and_then(resolver::Failure::resource)
        }

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::new("native-file");
                for child in ["project/spec", "project/data", "additional", "outside"] {
                    fs::create_dir_all(path.join(child)).unwrap();
                }
                Self(path)
            }
            fn path(&self, name: &str) -> PathBuf {
                self.0.join(name)
            }
            fn text(&self, name: &str) -> String {
                self.path(name).to_str().unwrap().into()
            }
            fn resource_key(&self, name: &str) -> String {
                // Expected protocol identities use forward separators, independently
                // of the native spelling.
                let path = yamaa_adapters::canonical_path::canonicalize(self.path(name))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn resources(&self) -> Resources {
                Resources::new(
                    &self.text("project"),
                    &self.text("project/spec"),
                    &[self.text("additional")],
                )
                .unwrap()
            }
        }

        #[test]
        fn declaration_locations_use_first_anchor_without_opening_absent_artifacts() {
            let study = Study::new();
            fs::write(study.path("project/spec/producer.yaml"), b"metadata").unwrap();
            // A fallback artifact exists, but a declaration still names the first
            // anchor beneath the originating specification, including absent parents.
            fs::write(study.path("additional/output.csv"), b"never captured").unwrap();
            let resources = study.resources();
            let declaring = resources.resolve("producer.yaml").unwrap();
            let expected = declaring.rsplit_once('/').unwrap().0;
            assert_eq!(
                resources.location_from(&declaring, "output.csv").unwrap(),
                format!("{expected}/output.csv")
            );
            assert_eq!(
                resources
                    .location_from(&declaring, "absent/output.csv")
                    .unwrap(),
                format!("{expected}/absent/output.csv")
            );
            assert_eq!(
                resources.resolve_from(&declaring, "absent/output.csv"),
                Err(Error::Missing)
            );
            assert_eq!(resources.capture_reads(), 0);
            assert!(!study.path("project/spec/output.csv").exists());
            assert!(!study.path("project/spec/absent").exists());
        }

        #[test]
        fn declaration_locations_keep_approved_containment_and_file_intent() {
            let study = Study::new();
            fs::write(study.path("project/spec/producer.yaml"), b"metadata").unwrap();
            let resources = study.resources();
            let declaring = resources.resolve("producer.yaml").unwrap();
            use yamaa_core::resource::ResourceFailure as R;
            for (written, error) in [
                ("", Error::WrittenForm(R::EmptySegment)),
                ("output.csv/", Error::WrittenForm(R::EmptySegment)),
                ("output.csv/.", Error::InvalidPath),
                ("output.csv/..", Error::InvalidPath),
                ("folder\\output.csv", Error::WrittenForm(R::Backslash)),
                (
                    "https://example.invalid/output.csv",
                    Error::WrittenForm(R::UriScheme),
                ),
            ] {
                assert_eq!(
                    resources.location_from(&declaring, written),
                    Err(error),
                    "{written}"
                );
            }
            assert_eq!(
                resources.location_from(&declaring, "../../outside/output.csv"),
                Err(Error::OutsideRoots)
            );
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn metadata_inspection_and_capture_reuse_only_exact_retained_bytes() {
            let study = Study::new();
            fs::write(study.path("project/data/source.csv"), b"ID,V\n1,2\n").unwrap();
            let mut resources = study.resources();
            resources.inspect("../data/source.csv").unwrap();
            assert_eq!(resources.capture_reads(), 0);
            let (first, created) = resources.capture("../data/source.csv").unwrap();
            assert!(created);
            assert_eq!(first.as_ref(), b"ID,V\n1,2\n");
            let (again, created) = resources.capture("../data/source.csv").unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&first, &again));
            assert_eq!(resources.capture_reads(), 1);
            fs::write(study.path("project/data/source.csv"), b"ID,V\n1,3\n").unwrap();
            assert_eq!(resources.capture("../data/source.csv"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"ID,V\n1,2\n");
            assert_eq!(resources.capture_reads(), 1);
            assert_eq!(
                input_failure(&resources, &Error::Changed),
                Some(ResourceFailure::ContentChanged)
            );
            assert_eq!(
                input_failure(
                    &resources,
                    &Error::Limit(
                        yamaa_core::limits::Limits::source_bytes(
                            yamaa_core::limits::Limits::SOURCE_BYTES + 1
                        )
                        .unwrap_err(),
                    )
                ),
                Some(ResourceFailure::Boundary(
                    yamaa_core::limits::LimitExceeded {
                        resource: "source_bytes",
                        limit: 10 * 1024 * 1024 * 1024,
                        actual: Some(10 * 1024 * 1024 * 1024 + 1),
                    }
                ))
            );
            for opaque in [Error::Unreadable, Error::InvalidPath] {
                assert_eq!(input_failure(&resources, &opaque), None);
            }
        }

        #[test]
        fn cached_growth_and_shrink_are_changes_without_replacing_the_snapshot() {
            let study = Study::new();
            let name = study.path("project/spec/a");
            fs::write(&name, b"12").unwrap();
            let mut resources = study.resources();
            assert!(resources.capture("a").unwrap().1);
            fs::write(&name, b"123").unwrap();
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            fs::write(&name, b"1").unwrap();
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            assert_eq!(resources.capture_reads(), 1);
        }

        #[test]
        fn empty_file_and_later_attempts_share_the_capture() {
            let study = Study::new();
            fs::write(study.path("project/spec/empty"), b"").unwrap();
            let mut resources = study.resources();
            assert!(resources.capture("empty").unwrap().1);
            assert!(!resources.capture("empty").unwrap().1);
            assert_eq!(resources.capture_reads(), 1);
        }

        #[test]
        fn relative_fallback_order_is_base_then_project_then_declared_data_roots() {
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"base").unwrap();
            fs::write(study.path("project/a"), b"project").unwrap();
            fs::write(study.path("additional/a"), b"additional").unwrap();
            fs::write(study.path("project/b"), b"project").unwrap();
            fs::write(study.path("additional/b"), b"additional").unwrap();
            fs::write(study.path("additional/c"), b"additional").unwrap();
            let mut resources = study.resources();
            assert_eq!(resources.capture("a").unwrap().0.as_ref(), b"base");
            assert_eq!(resources.capture("b").unwrap().0.as_ref(), b"project");
            assert_eq!(resources.capture("c").unwrap().0.as_ref(), b"additional");
            fs::create_dir(study.path("project/spec/d")).unwrap();
            fs::write(study.path("project/d"), b"later").unwrap();
            assert_eq!(resources.capture("d"), Err(Error::NotRegularFile));
        }

        #[cfg(unix)]
        #[test]
        fn links_are_terminal_and_paths_cannot_escape_the_selected_roots() {
            let study = Study::new();
            fs::write(study.path("outside/secret"), b"outside").unwrap();
            fs::write(study.path("project/link"), b"later").unwrap();
            std::os::unix::fs::symlink(
                study.path("outside/secret"),
                study.path("project/spec/link"),
            )
            .unwrap();
            std::os::unix::fs::symlink(study.path("outside"), study.path("project/spec/directory"))
                .unwrap();
            let mut resources = study.resources();
            assert_eq!(resources.inspect("link"), Err(Error::Symlink));
            assert_eq!(resources.capture("link"), Err(Error::Symlink));
            assert_eq!(resources.capture("directory/secret"), Err(Error::Symlink));
            assert_eq!(
                resources.capture("../../outside/secret"),
                Err(Error::OutsideRoots)
            );
            assert_eq!(
                resources.capture(&study.text("outside/secret")),
                Err(Error::OutsideRoots)
            );
            use yamaa_core::resource::ResourceFailure as R;
            for (malformed, error) in [
                ("", Error::WrittenForm(R::EmptySegment)),
                ("a//b", Error::WrittenForm(R::EmptySegment)),
                ("a\\b", Error::WrittenForm(R::Backslash)),
                ("https://example.org/a", Error::WrittenForm(R::UriScheme)),
                ("/tmp/../a", Error::InvalidPath),
                ("a:é", Error::WrittenForm(R::UriScheme)),
                ("a\0b", Error::InvalidPath),
            ] {
                assert_eq!(resources.inspect(malformed), Err(error), "{malformed:?}");
            }
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn hard_link_aliases_share_a_snapshot_and_each_is_verified_at_its_own_path() {
            // A snapshot's key is the canonical path the walk reached; the
            // physical file it reports is a hint that binds a hard link to
            // the held snapshot of the same file (FILE-9).
            let study = Study::new();
            let first = study.path("project/spec/a");
            let second = study.path("additional/b");
            fs::write(&first, b"original").unwrap();
            fs::hard_link(&first, &second).unwrap();
            let mut resources = study.resources();
            let (a, _) = resources.capture("a").unwrap();
            let (b, created) = resources.capture("b").unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&a, &b));
            fs::remove_file(&first).unwrap();
            fs::write(&first, b"changed!").unwrap();
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            assert_eq!(resources.capture("b").unwrap(), (b, false));
            assert_eq!(resources.capture_reads(), 1);
        }

        #[cfg(unix)]
        #[test]
        fn selected_root_descriptor_survives_renaming_without_following_a_replacement_link() {
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"accepted").unwrap();
            fs::write(study.path("outside/a"), b"outside").unwrap();
            let mut resources = study.resources();
            fs::rename(study.path("project"), study.path("moved")).unwrap();
            std::os::unix::fs::symlink(study.path("outside"), study.path("project")).unwrap();
            assert_eq!(resources.capture("a").unwrap().0.as_ref(), b"accepted");
            assert_eq!(
                resources
                    .capture(&study.text("project/spec/a"))
                    .unwrap()
                    .0
                    .as_ref(),
                b"accepted"
            );
        }

        #[test]
        fn an_outside_declaring_directory_can_reach_an_approved_root_or_fall_back() {
            let study = Study::new();
            fs::write(study.path("project/a"), b"approved").unwrap();
            fs::write(study.path("outside/a"), b"outside").unwrap();
            let mut resources =
                Resources::new(&study.text("project"), &study.text("outside"), &[]).unwrap();
            assert_eq!(
                resources.capture("../project/a").unwrap().0.as_ref(),
                b"approved"
            );
            assert_eq!(resources.capture("a").unwrap().0.as_ref(), b"approved");
            assert_eq!(resources.capture_reads(), 1);
        }

        #[cfg(unix)]
        #[test]
        fn authored_absolute_root_aliases_and_nested_roots_keep_their_selected_authority() {
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"selected").unwrap();
            std::os::unix::fs::symlink(study.path("project"), study.path("alias")).unwrap();
            let mut resources = Resources::new(
                &format!("{}/./", study.text("alias")),
                &study.text("project/spec"),
                &[study.text("project/spec")],
            )
            .unwrap();
            assert_eq!(
                resources
                    .capture(&study.text("alias/spec/a"))
                    .unwrap()
                    .0
                    .as_ref(),
                b"selected"
            );
            assert_eq!(resources.capture("a").unwrap().0.as_ref(), b"selected");
            assert_eq!(resources.capture_reads(), 1);
            assert_eq!(
                resources.inspect(Path::new(".").to_str().unwrap()),
                Err(Error::NotRegularFile)
            );
        }

        #[cfg(unix)]
        #[test]
        fn permission_failures_are_terminal_before_any_later_fallback_is_read() {
            use std::os::unix::fs::PermissionsExt;
            let study = Study::new();
            let earlier = study.path("project/spec/blocked");
            fs::write(&earlier, b"earlier").unwrap();
            fs::write(study.path("project/blocked"), b"later").unwrap();
            let mut resources = study.resources();
            let permissions = fs::metadata(&earlier).unwrap().permissions();
            fs::set_permissions(&earlier, fs::Permissions::from_mode(0o0)).unwrap();
            if fs::File::open(&earlier).is_ok() {
                fs::set_permissions(&earlier, permissions).unwrap();
                eprintln!("permission checks bypassed by this process; DAC case not exercised");
                return;
            }
            let inspected = resources.inspect("blocked");
            let captured = resources.capture("blocked");
            fs::set_permissions(&earlier, permissions).unwrap();
            assert_eq!(
                input_failure(&resources, inspected.as_ref().unwrap_err()),
                None
            );
            assert_eq!(inspected, Err(Error::Unreadable));
            assert_eq!(captured, Err(Error::Unreadable));
            assert_eq!(resources.capture_reads(), 0);
            assert_eq!(resources.capture("blocked").unwrap().0.as_ref(), b"earlier");
        }

        #[cfg(unix)]
        #[test]
        fn unsearchable_directory_does_not_fall_back_to_a_different_file() {
            use std::os::unix::fs::PermissionsExt;
            let study = Study::new();
            let earlier = study.path("project/spec/private");
            fs::create_dir(&earlier).unwrap();
            fs::write(earlier.join("blocked"), b"earlier").unwrap();
            fs::create_dir(study.path("project/private")).unwrap();
            fs::write(study.path("project/private/blocked"), b"later").unwrap();
            let mut resources = study.resources();
            let permissions = fs::metadata(&earlier).unwrap().permissions();
            fs::set_permissions(&earlier, fs::Permissions::from_mode(0o400)).unwrap();
            if fs::metadata(earlier.join("blocked")).is_ok() {
                fs::set_permissions(&earlier, permissions).unwrap();
                eprintln!("permission checks bypassed by this process; DAC case not exercised");
                return;
            }
            let inspected = resources.inspect("private/blocked");
            let captured = resources.capture("private/blocked");
            fs::set_permissions(&earlier, permissions).unwrap();
            assert_eq!(
                input_failure(&resources, inspected.as_ref().unwrap_err()),
                None
            );
            assert_eq!(inspected, Err(Error::Unreadable));
            assert_eq!(captured, Err(Error::Unreadable));
            assert_eq!(resources.capture_reads(), 0);
        }

        #[test]
        fn an_unlinked_snapshot_does_not_poison_a_new_path_to_the_remaining_file() {
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"original").unwrap();
            fs::hard_link(study.path("project/spec/a"), study.path("project/spec/c")).unwrap();
            let mut resources = study.resources();
            let (first, _) = resources.capture("a").unwrap();
            fs::remove_file(study.path("project/spec/a")).unwrap();
            let (next, created) = resources.capture("c").unwrap();
            assert!(created);
            assert_eq!(next.as_ref(), b"original");
            assert_eq!(first.as_ref(), b"original");
            assert_eq!(resources.capture_reads(), 2);
            assert_eq!(resources.capture("a"), Err(Error::Missing));
            assert!(!resources.capture("c").unwrap().1);
        }

        #[test]
        fn a_file_rewritten_after_one_name_was_captured_is_a_change_under_its_other_names() {
            // FILE-9: two names of one physical file never observe different
            // bytes, so a rewrite between their captures is a change.
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"original").unwrap();
            fs::hard_link(study.path("project/spec/a"), study.path("project/spec/c")).unwrap();
            let mut resources = study.resources();
            let (first, _) = resources.capture("a").unwrap();
            fs::write(study.path("project/spec/a"), b"changed!").unwrap();
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            assert_eq!(resources.capture("c"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"original");
            assert_eq!(resources.capture_reads(), 1);
            // Removing the captured name does not make the rewrite readable:
            // `c` still reaches the file the snapshot was taken from.
            fs::remove_file(study.path("project/spec/a")).unwrap();
            assert_eq!(resources.capture("c"), Err(Error::Changed));
            assert_eq!(resources.capture("a"), Err(Error::Missing));
            assert_eq!(resources.capture("c"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"original");
            assert_eq!(resources.capture_reads(), 1);
            // A scope that starts after the change reads the current bytes.
            let mut later = study.resources();
            assert_eq!(later.capture("c").unwrap().0.as_ref(), b"changed!");
        }

        #[test]
        fn different_written_paths_to_the_same_key_are_each_retained_and_verified() {
            let study = Study::new();
            fs::write(study.path("project/a"), b"retained").unwrap();
            let mut resources = study.resources();
            let (first, _) = resources.capture("../a").unwrap();
            let (again, created) = resources.capture("a").unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&first, &again));
            fs::write(study.path("project/spec/a"), b"shadowed").unwrap();
            assert_eq!(resources.capture("../a"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"retained");
            assert_eq!(resources.capture_reads(), 1);
        }

        #[test]
        fn declaring_file_views_resolve_metadata_and_share_held_bytes_before_selecting_data_base() {
            let study = Study::new();
            fs::write(study.path("project/spec/entry.yaml"), b"entry").unwrap();
            fs::write(study.path("project/data/parent.yaml"), b"parent").unwrap();
            fs::write(study.path("project/data/source.csv"), b"source").unwrap();
            let mut resources = study.resources();
            let entry = resources.resolve("entry.yaml").unwrap();
            let parent = resources
                .resolve_from(&entry, "../data/parent.yaml")
                .unwrap();
            assert_eq!(parent, study.resource_key("project/data/parent.yaml"));
            assert_eq!(resources.capture_reads(), 0);
            let (first, created) = resources
                .capture_metadata_from(&entry, "../data/parent.yaml")
                .unwrap();
            assert!(created && first.as_ref() == b"parent");
            let (again, created) = resources
                .capture_metadata_from(&parent, "parent.yaml")
                .unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&first, &again));
            assert_eq!(resources.capture_reads(), 1);
            resources.select_entry_base(&parent).unwrap();
            assert_eq!(
                resources.capture("source.csv").unwrap().0.as_ref(),
                b"source"
            );
            assert_eq!(resources.capture_reads(), 2);
        }

        #[test]
        fn every_declaring_base_and_written_alias_is_verified_when_one_fallback_changes() {
            let study = Study::new();
            fs::write(study.path("project/data/layer.yaml"), b"layer").unwrap();
            fs::write(study.path("project/a"), b"retained").unwrap();
            let mut resources = study.resources();
            let layer = resources.resolve("../data/layer.yaml").unwrap();
            let (first, created) = resources.capture_from(&layer, "../a").unwrap();
            assert!(created);
            let (again, created) = resources.capture("a").unwrap();
            assert!(!created && std::sync::Arc::ptr_eq(&first, &again));
            fs::write(study.path("project/spec/a"), b"shadowed").unwrap();
            assert_eq!(resources.capture_from(&layer, "../a"), Err(Error::Changed));
            assert_eq!(first.as_ref(), b"retained");
            assert_eq!(resources.capture_reads(), 1);
        }

        #[cfg(unix)]
        #[test]
        fn declaring_file_views_keep_approved_roots_and_terminal_link_failures() {
            let study = Study::new();
            fs::write(study.path("project/spec/entry.yaml"), b"entry").unwrap();
            fs::write(study.path("outside/secret"), b"outside").unwrap();
            std::os::unix::fs::symlink(
                study.path("outside/secret"),
                study.path("project/data/link"),
            )
            .unwrap();
            let mut resources = study.resources();
            let entry = resources.resolve("entry.yaml").unwrap();
            assert_eq!(
                resources.capture_from(&entry, "../../outside/secret"),
                Err(Error::OutsideRoots)
            );
            assert_eq!(
                resources.resolve_from(&entry, "../data/link"),
                Err(Error::Symlink)
            );
            assert_eq!(
                resources.capture_from("relative.yaml", "entry.yaml"),
                Err(Error::InvalidPath)
            );
            assert_eq!(resources.capture_reads(), 0);
        }
    }
}
