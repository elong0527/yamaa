#![cfg(feature = "yaml")]
//! File contracts.
#[allow(dead_code)]
mod support;
mod file {
    mod file_project {
        use crate::support::project_schemas as schemas;
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{
            file_project::{Error, FileProject},
            file_resources::{Error as ResourceError, Resources},
            project_run::BoundaryFailure,
            project_source::{CaptureFailure, Failure as EnvironmentFailure, Origin},
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            project_activation::{ActivationPort, Failure, LockObservation},
            specification_run::PortError,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::Language,
            table::{TableAccess, ValueRef},
            value::Value,
        };

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-file-project-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                for name in ["env", "entry", "parent", "outside"] {
                    fs::create_dir(path.join(name)).unwrap();
                }
                let study = Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap());
                study.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions: {id: id.yaml}\ncodelists: [ct.yaml]\n");
                study.write("uv.lock", "version = 1\npackage = []\n");
                study.write("env/id.yaml", "function: program.id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n");
                study.write(
                    "env/ct.yaml",
                    "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
                );
                study.write("entry/domain.yaml", DOMAIN);
                study
            }
            fn text(&self, name: &str) -> String {
                self.0.join(name).to_str().unwrap().into()
            }
            fn write(&self, name: &str, bytes: &str) {
                fs::write(self.0.join(name), bytes).unwrap();
            }
            fn prepare(&self, root: &str, roots: &[String]) -> Result<FileProject, Error> {
                FileProject::prepare(
                    Resources::new(root, &self.text("entry"), roots).unwrap(),
                    "domain.yaml",
                    "../env/environment.yaml",
                    Language::Python,
                    schemas::domain_schema(),
                    schemas::environment_schema(),
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
        const DOMAIN: &str = "schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";

        #[test]
        fn project_preparation_rejects_unknown_suffix_directory_before_activation() {
            let study = Study::new();
            fs::create_dir(study.0.join("entry/input.unknown")).unwrap();
            study.write(
                "entry/domain.yaml",
                &DOMAIN.replace("input.csv", "input.unknown"),
            );
            let Err(Error::Compile(run)) = study.prepare(study.0.to_str().unwrap(), &[]) else {
                panic!("directory finding")
            };
            let yamaa_core::specification::PrepareError::Invalid(findings) = &run.rejection.error
            else {
                panic!("invalid findings")
            };
            let issue = findings[0].diagnostic();
            assert_eq!(
                issue.definition().condition,
                "resource_path_not_regular_file"
            );
            assert_eq!(issue.spec_paths, ["input.SRC.path"]);
            assert!(!study.0.join("entry/output.csv").exists());
        }

        #[test]
        fn public_preparation_accepts_native_absolute_environment_paths_without_study_reads() {
            let study = Study::new();
            study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
            let project = yamaa_adapters::project_application::prepare(
                &study.text("entry/domain.yaml"),
                &study.text("env/environment.yaml"),
                Language::Python,
            )
            .unwrap();
            assert!(project.run().check_diagnostics().is_empty());
            assert!(!study.0.join("entry/input.csv").exists());
            // Entry, environment, lock, function and codelist metadata only.
            assert_eq!(project.capture_reads(), 5);
        }

        #[derive(Default)]
        struct Activation {
            trace: Vec<&'static str>,
            reject: bool,
            reject_binding: bool,
            mismatch: bool,
        }
        impl ActivationPort for Activation {
            type Handle = ();
            type Error = &'static str;
            fn verify_lock(
                &mut self,
                _: Language,
                lock: &LockReference,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Self::Error> {
                assert_eq!(lock.kind, LockKind::Uv);
                assert_eq!(lock.written, "../uv.lock");
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
            let study = Study::new();
            // Resource identities use portable separators; the host path passed to
            // preparation keeps its native spelling.
            let identity = |name| {
                let path = study.text(name);
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            };
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            let reads = project.capture_reads();
            let mut activation = Activation {
                mismatch: true,
                ..Default::default()
            };
            let attempt = project.build(&mut activation, Scope::default());
            let run = project.retained_run();
            study.write("env/id.yaml", "changed after preparation");
            study.write("env/environment.yaml", "changed root after preparation");
            drop(project);
            let result = yamaa_adapters::project_report::build_result(
                &run,
                &attempt,
                yamaa_adapters::specification_report::Identity {
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
            let context: yamaa_core::json::Value = result.issues()[0].context.clone();
            assert_eq!(context["source"], identity("env/id.yaml"));
            assert_eq!(context["entry"], identity("env/environment.yaml"));
            assert_eq!(context["environment_path"], "functions.id");
            assert_eq!(context["actual"], 8);
            assert_eq!(context["expected"], 7);
            assert_eq!(
                result.observations()["source_reads"],
                yamaa_adapters::json!([])
            );
            assert_eq!(activation.trace, ["lock", "bind", "call", "call"]);
            assert_eq!(reads, 5);
            assert_eq!(
                run.captured_environment().captures()[0]
                    .document
                    .source()
                    .identity,
                identity("env/id.yaml")
            );
        }

        #[test]
        fn project_functions_build_without_any_adjacent_packaging_lock() {
            let study = Study::new();
            study.write(
        "env/environment.yaml",
        "schema_version: '1.0'\nlanguage: python\nfunctions: {id: id.yaml}\ncodelists: [ct.yaml]\n",
    );
            fs::remove_file(study.0.join("uv.lock")).unwrap();
            study.write("entry/input.csv", "ID\n1\n");
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            assert!(project.run().environment().lock().is_none());
            assert!(project.run().captured_environment().lock().is_none());
            let mut activation = Activation {
                reject: true,
                ..Default::default()
            };
            let attempt = project.build(&mut activation, Scope::default());
            assert!(attempt.boundary.is_ok());
            assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
            assert_eq!(activation.trace, ["bind", "call", "call", "call"]);
            assert_eq!(
                attempt
                    .dataset
                    .result
                    .unwrap()
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                &[vec![Value::Int(1), Value::Int(1)]]
            );
        }

        #[test]
        fn functions_without_a_lock_still_reject_bindings_and_cases_before_study_reads() {
            let study = Study::new();
            study.write(
                "env/environment.yaml",
                "schema_version: '1.0'\nlanguage: python\nfunctions: {id: id.yaml}\ncodelists: [ct.yaml]\n",
            );
            fs::remove_file(study.0.join("uv.lock")).unwrap();
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            let reads = project.capture_reads();
            assert!(!study.0.join("entry/input.csv").exists());
            for reject_binding in [true, false] {
                let mut activation = Activation {
                    reject: true,
                    reject_binding,
                    mismatch: true,
                    ..Default::default()
                };
                let attempt = project.build(&mut activation, Scope::default());
                if reject_binding {
                    assert!(matches!(
                        attempt.boundary,
                        Err(BoundaryFailure::Activation(Failure::Bindings(_)))
                    ));
                    assert_eq!(activation.trace, ["bind"]);
                } else {
                    assert!(matches!(
                        attempt.boundary,
                        Err(BoundaryFailure::Activation(Failure::Tests(_)))
                    ));
                    assert_eq!(activation.trace, ["bind", "call", "call"]);
                }
                assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
                assert!(attempt.dataset.sources.is_empty());
                assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
                assert_eq!(project.capture_reads(), reads);
            }
        }

        #[test]
        fn native_project_retains_metadata_and_reuses_exact_snapshots_with_fresh_activation() {
            let study = Study::new();
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            assert_eq!(project.capture_reads(), 5);
            let captures = project.run().captured_environment();
            assert_eq!(
                captures.root().source().bytes,
                fs::read(study.0.join("env/environment.yaml")).unwrap()
            );
            assert_eq!(
                captures.lock().unwrap().source.bytes,
                b"version = 1\npackage = []\n"
            );
            assert_eq!(captures.captures().len(), 2);
            assert_eq!(captures.captures()[0].origin, Origin::Function("id".into()));
            assert!(captures.captures()[0]
                .document
                .source()
                .identity
                .ends_with("/env/id.yaml"));
            assert!(project.run().check_diagnostics().is_empty());
            let root_bytes = captures.root().source().bytes.as_ptr();
            let lock_bytes = captures.lock().unwrap().source.bytes.as_ptr();
            let definition = project.run().environment().functions()[0]
                .definition()
                .tests
                .as_ptr();
            // Metadata bytes belong to the prepared capability, independently of later
            // filesystem changes. Study bytes are first captured only after activation.
            study.write("env/environment.yaml", "[changed");
            study.write("env/id.yaml", "[changed");
            study.write("uv.lock", "changed");
            study.write("entry/domain.yaml", "[changed");
            study.write("entry/input.csv", "ID\n1\n");
            let mut activation = Activation::default();
            let first = project.build(&mut activation, Scope::default());
            assert!(first.boundary.is_ok());
            assert_eq!(first.activation.lock, LockObservation::Verified);
            assert_eq!(project.capture_reads(), 6);
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
            let second = project.build(&mut activation, Scope::default());
            assert!(second.boundary.is_ok());
            assert_eq!(project.capture_reads(), 6);
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

        #[test]
        fn native_project_activation_rejection_precedes_even_a_missing_study_file() {
            let study = Study::new();
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            let mut activation = Activation {
                reject: true,
                ..Default::default()
            };
            let attempt = project.build(&mut activation, Scope::default());
            assert!(matches!(
                attempt.boundary,
                Err(BoundaryFailure::Activation(Failure::Lock(
                    "original lock rejection"
                )))
            ));
            assert!(attempt.dataset.sources.is_empty());
            assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
            assert_eq!(project.capture_reads(), 5);
            assert_eq!(activation.trace, ["lock"]);
        }

        #[test]
        fn installed_port_factory_borrows_the_exact_prepared_lock_on_each_build() {
            struct Borrowed<'a> {
                lock: &'a [u8],
                expected: *const u8,
                inner: Activation,
            }
            impl ActivationPort for Borrowed<'_> {
                type Handle = ();
                type Error = &'static str;
                fn verify_lock(
                    &mut self,
                    language: Language,
                    lock: &LockReference,
                    functions: &[ProjectFunctionIdentity],
                ) -> Result<(), Self::Error> {
                    assert_eq!(self.lock.as_ptr(), self.expected);
                    assert_eq!(self.lock, b"version = 1\npackage = []\n");
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
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            let expected = project
                .run()
                .captured_environment()
                .lock()
                .unwrap()
                .source
                .bytes
                .as_ptr();
            study.write("entry/input.csv", "ID\n1\n");
            let mut constructed = 0;
            for _ in 0..2 {
                let attempt = project.build_with(
                    |run| {
                        constructed += 1;
                        Borrowed {
                            lock: &run.captured_environment().lock().unwrap().source.bytes,
                            expected,
                            inner: Activation::default(),
                        }
                    },
                    Scope::default(),
                );
                assert!(attempt.boundary.is_ok());
                assert_eq!(attempt.activation.lock, LockObservation::Verified);
                assert!(attempt.dataset.result.unwrap().result.is_ok());
            }
            assert_eq!(constructed, 2);
            assert_eq!(project.capture_reads(), 6);
        }

        #[test]
        fn native_project_keeps_approved_roots_for_metadata_and_study_without_expanding_them() {
            let study = Study::new();
            // The selected entry root does not authorize its sibling environment. The
            // same explicit approved fallback used by ordinary builds makes it available.
            let Err(Error::Resource(ResourceError::OutsideRoots)) =
                study.prepare(&study.text("entry"), &[])
            else {
                panic!("environment outside roots")
            };
            let mut project = study
                .prepare(&study.text("entry"), &[study.text("")])
                .unwrap();
            study.write("input.csv", "ID\n1\n");
            let attempt = project.build(&mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert!(attempt.dataset.sources[0].read.source.path == "input.csv");

            study.write("env/environment.yaml", "schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\nfunctions: {id: ../../unapproved.yaml}\n");
            let Err(Error::Environment(EnvironmentFailure::Rejected(rejected))) =
                study.prepare(&study.text(""), &[])
            else {
                panic!("unapproved definition")
            };
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].written, "../../unapproved.yaml");
            assert!(matches!(
                rejected.sources[0].error,
                CaptureFailure::Port(ResourceError::OutsideRoots)
            ));
            assert!(rejected.lock.is_some());
        }

        #[test]
        fn native_project_inheritance_retains_parent_authorship_and_selected_entry_source_paths() {
            let study = Study::new();
            study.write(
                "parent/base.yaml",
                DOMAIN.replace("input.csv", "data.csv").as_str(),
            );
            study.write("parent/data.csv", "ID\n1\n");
            study.write(
                "entry/domain.yaml",
                "schema_version: '1.0'\nparents: ../parent/base.yaml\n",
            );
            let mut project = study.prepare(&study.text(""), &[]).unwrap();
            assert_eq!(project.capture_reads(), 6);
            let document = project.run().document();
            assert_eq!(document.parents().len(), 1);
            assert_eq!(document.written_source_path("SRC"), Some("data.csv"));
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            let attempt = project.build(&mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert_eq!(project.capture_reads(), 7);
        }

        #[test]
        fn selected_ordinary_environment_owner_preserves_fresh_activation_and_inherited_data_origins(
        ) {
            use yamaa_adapters::file_workflow::{self, Selected};
            let study = Study::new();
            study.write("parent/base.yaml", &DOMAIN.replace("input.csv", "data.csv"));
            study.write(
                "entry/domain.yaml",
                "schema_version: '1.0'\nparents: ../parent/base.yaml\n",
            );
            let Selected::Ordinary(mut project) = file_workflow::select_with_resources(
                Resources::new(&study.text(""), &study.text("entry"), &[]).unwrap(),
                "domain.yaml",
                "../env/environment.yaml",
                Language::Python,
                schemas::domain_schema(),
                schemas::environment_schema(),
            )
            .unwrap() else {
                panic!("ordinary capability");
            };
            assert_eq!(project.capture_reads(), 6);
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            // A fresh activation still refuses before the absent authored study source.
            let mut failed = Activation {
                reject: true,
                ..Default::default()
            };
            let rejection = project.build(&mut failed, Scope::default());
            assert_eq!(failed.trace, ["lock"]);
            assert!(rejection.dataset.sources.is_empty());
            assert_eq!(project.capture_reads(), 6);
            study.write("parent/data.csv", "ID\n1\n");
            let mut host = Activation::default();
            let first = project.build(&mut host, Scope::default());
            let second = project.build(&mut host, Scope::default());
            assert!(first.boundary.is_ok() && second.boundary.is_ok());
            assert_eq!(
                host.trace,
                ["lock", "bind", "call", "call", "call", "lock", "bind", "call", "call", "call"]
            );
            assert_eq!(project.capture_reads(), 7);
            assert!(std::sync::Arc::ptr_eq(
                first.dataset.sources[0].snapshot.as_ref().unwrap(),
                second.dataset.sources[0].snapshot.as_ref().unwrap()
            ));
            let retained = project.retained_run();
            drop(project);
            // Metadata and environment retention survives all other owners. The original
            // tests and lock stay in this run; repeated builds never replaced them.
            assert_eq!(retained.captured_environment().captures().len(), 2);
            assert_eq!(
                retained.document().parents()[0].source().bytes,
                DOMAIN.replace("input.csv", "data.csv").as_bytes()
            );
            assert_eq!(
                retained.captured_environment().lock().unwrap().source.bytes,
                b"version = 1\npackage = []\n"
            );
        }
    }
    mod file_publication {
        #![cfg(unix)]
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::file_publication::{Error, Publisher};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Directory(PathBuf);
        impl Directory {
            fn new() -> Self {
                let directory = std::env::temp_dir().join(format!(
                    "yamaa-native-publication-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&directory).unwrap();
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
        impl Drop for Directory {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
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
            assert!(matches!(
                Publisher::new("declared.csv", directory.0.to_str().unwrap()),
                Err(Error::InvalidTarget)
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
                &directory.0,
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
                Err(Error::InvalidTarget)
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
                Err(Error::InvalidTarget)
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
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::file_resources::{Error, Resources};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-native-file-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
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
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
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
            use yamaa_core::resource::ResourceFailure;
            use yamaa_core::specification_run::SpecificationSourcePort;
            assert_eq!(
                resources.resource_failure(&Error::Changed),
                Some(ResourceFailure::ContentChanged)
            );
            assert_eq!(
                resources.resource_failure(&Error::Limit(
                    yamaa_core::limits::Limits::source_bytes(
                        yamaa_core::limits::Limits::SOURCE_BYTES + 1
                    )
                    .unwrap_err(),
                )),
                Some(ResourceFailure::Boundary(
                    yamaa_core::limits::LimitExceeded {
                        resource: "source_bytes",
                        limit: 10 * 1024 * 1024 * 1024,
                        actual: Some(10 * 1024 * 1024 * 1024 + 1),
                    }
                ))
            );
            for opaque in [Error::Unreadable, Error::InvalidPath] {
                assert_eq!(resources.resource_failure(&opaque), None);
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
        fn hard_link_aliases_share_a_snapshot_and_every_accepted_alias_is_verified() {
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
            assert_eq!(resources.capture("b"), Err(Error::Changed));
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
                <Resources as yamaa_core::specification_run::SpecificationSourcePort>::resource_failure(
                    &resources,
                    inspected.as_ref().unwrap_err()
                ),
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
                <Resources as yamaa_core::specification_run::SpecificationSourcePort>::resource_failure(
                    &resources,
                    inspected.as_ref().unwrap_err()
                ),
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
        fn a_changed_snapshot_stays_failed_for_its_path_while_a_new_path_captures_fresh_bytes() {
            let study = Study::new();
            fs::write(study.path("project/spec/a"), b"original").unwrap();
            fs::hard_link(study.path("project/spec/a"), study.path("project/spec/c")).unwrap();
            let mut resources = study.resources();
            let (first, _) = resources.capture("a").unwrap();
            fs::write(study.path("project/spec/a"), b"changed!").unwrap();
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            let (next, created) = resources.capture("c").unwrap();
            assert!(created);
            assert_eq!(next.as_ref(), b"changed!");
            assert_eq!(first.as_ref(), b"original");
            assert_eq!(resources.capture_reads(), 2);
            assert_eq!(resources.capture("a"), Err(Error::Changed));
            assert!(!resources.capture("c").unwrap().1);
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
    mod file_workflow {
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{
            file_resources::Resources,
            file_workflow::{self, Error},
            producer_check, shipped_schema,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::project_function::Language;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        #[test]
        fn unavailable_environment_precedes_bad_entry_and_valid_environment_is_retained_before_missing_entry(
        ) {
            let study = Study::new();
            let entry = study.path("root.yaml");
            let environment = study.path("missing-environment.yaml");
            for entry_bytes in [None, Some("[invalid entry that must not be read")] {
                if let Some(bytes) = entry_bytes {
                    study.write("root.yaml", bytes);
                }
                let file_workflow::SelectionError::Preparation(Error::Environment(owner)) =
                    file_workflow::select(&entry, &environment, Language::Python)
                        .err()
                        .unwrap()
                else {
                    panic!("original unavailable environment owner")
                };
                assert!(matches!(
                    owner.error(),
                    yamaa_adapters::file_project::Error::Resource(
                        yamaa_adapters::file_resources::Error::Missing
                    )
                ));
                assert_eq!(owner.capture_reads(), 0);
                assert_eq!(owner.captured_sources().count(), 0);
                let rejection = Error::Environment(owner);
                let rows = producer_check::rejected(&rejection, Language::Python).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].condition, "project_environment_missing");
                assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0694"));
                assert_eq!(rows[0].spec_paths, ["environment"]);
                assert_eq!(
                    rows[0].context,
                    yamaa_adapters::json!({"cause":"missing", "path":environment})
                );
                let Error::Environment(owner) = &rejection else {
                    unreachable!()
                };
                assert_eq!(owner.capture_reads(), 0);
                assert_eq!(owner.captured_sources().count(), 0);
            }
            fs::remove_file(study.0.join("root.yaml")).unwrap();
            let environment = study.path("environment.yaml");
            study.write(
                "environment.yaml",
                "schema_version: '1.0'\nlanguage: python\n",
            );
            let file_workflow::SelectionError::Preparation(Error::Graph(owner)) =
                file_workflow::select(&entry, &environment, Language::Python)
                    .err()
                    .unwrap()
            else {
                panic!("valid environment precedes missing entry")
            };
            assert!(matches!(
                owner.error(),
                yamaa_adapters::file_graph::Error::Document(
                    yamaa_adapters::file_preparation::Error::Resource(
                        yamaa_adapters::file_resources::Error::Missing
                    )
                )
            ));
            assert_eq!(owner.capture_reads(), 1);
            let sources = owner.captured_sources().collect::<Vec<_>>();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].0, environment);
            assert_eq!(sources[0].1, b"schema_version: '1.0'\nlanguage: python\n");
            assert!(!study.0.join("input.csv").exists());
        }
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-workflow-check-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                let study = Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap());
                study.write("environment.yaml", "schema_version: '1.0'\n");
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
            fn prepare(&self) -> Result<yamaa_adapters::file_graph::FileGraph, Error> {
                file_workflow::prepare_with_resources(
                    Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
                    "root.yaml",
                    "environment.yaml",
                    Language::Python,
                    shipped_schema::capture().unwrap(),
                    shipped_schema::capture_environment().unwrap(),
                )
            }
            fn rejected(&self) -> Error {
                match self.prepare() {
                    Err(error) => error,
                    Ok(_) => panic!("expected rejection"),
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
            format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n  - name: VALUE\n    type: str\n    label: Reported value\n    derivation: {base}.VALUE{checks}\noutput: {{path: {output}, columns: [ID, VALUE]}}\n")
        }

        #[test]
        fn seventeen_column_verifications_keep_their_identities_without_reading_data() {
            let study = Study::new();
            let mut verifications = String::from("    verifications:\n");
            for index in 0..17 {
                verifications.push_str(&format!("      - not_missing: {{id: CHECK{index}}}\n"));
            }
            let source = yaml("{RAW: never-read.csv}", "RAW", "result.csv", false).replace(
                "    derivation: RAW.VALUE\n",
                &format!("    derivation: RAW.VALUE\n{verifications}"),
            );
            study.write("root.yaml", &source);
            let graph = study.prepare().unwrap();
            assert_eq!(graph.capture_reads(), 2);
            assert!(producer_check::prepared(&graph).unwrap().is_empty());
            let compiled = yamaa_core::specification::PreparedSpecification::prepare(
                graph.graph().nodes()[0].document.model(),
            )
            .unwrap();
            for index in 0..17 {
                let path = format!("columns.VALUE.verifications[{index}].not_missing");
                assert_eq!(
                    compiled.verification_identity(&path),
                    Some(format!("CHECK{index}").as_str())
                );
                assert!(matches!(
                    compiled.column_check(&path),
                    Some(yamaa_core::dataset::Check::NotMissing)
                ));
            }
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("result.csv").exists());
        }

        #[test]
        fn raw_schema_checks_keep_all_findings_and_public_checks_take_the_first_ten_thousand() {
            use yamaa_adapters::issue_rows::Issue;

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
            let Error::Graph(graph) = &owner else {
                panic!("schema rejection owner")
            };
            assert_eq!(graph.capture_reads(), 2); // environment and specification only
            let raw = producer_check::rejected(&owner, Language::Python).unwrap();
            assert_eq!(raw.len(), 10_001);
            for (index, issue) in raw.iter().enumerate() {
                let name = format!("extra_{index:05}");
                assert_eq!(
                    issue,
                    &Issue {
                        phase: "validation".into(),
                        condition: "unknown_field".into(),
                        requirement: Some("REQ-0285".into()),
                        spec_paths: vec![name.clone()],
                        context: yamaa_adapters::json!({"class":"root_class", "field":name}),
                        source_span: None,
                        operand_route: None,
                    },
                    "root field {index} keeps its complete finding in authored order"
                );
            }

            let public = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("environment.yaml"),
                Language::Python,
            );
            assert_eq!(public.len(), 10_000);
            assert_eq!(public, raw[..10_000]);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("result.csv").exists());
        }

        #[test]
        fn original_decode_diagnostics_remain_bounded_and_owned() {
            use yamaa_adapters::issue_rows::Issue;
            let study = Study::new();
            study.write("root.yaml", "\u{e9}\n");
            let owner = study.rejected();
            let expected = vec![Issue {
                phase: "validation".into(),
                condition: "non_ascii_source".into(),
                requirement: Some("REQ-0705".into()),
                spec_paths: vec!["$".into()],
                context: yamaa_adapters::json!({"line":1, "column":1, "path":study.path("root.yaml")}),
                source_span: None,
                operand_route: None,
            }];
            assert_eq!(
                producer_check::rejected(&owner, Language::Python).unwrap(),
                expected
            );
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("environment.yaml"),
                    Language::Python
                ),
                expected
            );
            study.write("root.yaml", "[");
            let malformed = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("environment.yaml"),
                Language::Python,
            );
            assert_eq!(malformed.len(), 1);
            assert_eq!(
                (
                    &*malformed[0].phase,
                    &*malformed[0].condition,
                    &malformed[0].spec_paths
                ),
                ("validation", "invalid_yaml", &vec!["$".to_owned()])
            );
            study.write("environment.yaml", "[");
            let environment = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("environment.yaml"),
                Language::Python,
            );
            assert_eq!(environment.len(), 1);
            assert_eq!(environment[0].condition, "invalid_yaml");
            let context: yamaa_core::json::Value = environment[0].context.clone();
            assert_eq!(context["path"], study.path("environment.yaml"));
            assert_eq!(context["environment_path"], "$");
            assert!(context["line"].as_u64().is_some());
            assert!(context["column"].as_u64().is_some());
            assert!(context["reason"].as_str().is_some());
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                producer_check::rejected(&owner, Language::Python).unwrap(),
                expected
            );
        }

        #[test]
        fn metadata_failures_name_each_written_dependency_and_cause_before_study_reads() {
            let study = Study::new();
            study.write(
        "environment.yaml",
        "schema_version: '1.0'\nlanguage: python\nlock: missing.lock\nfunctions: {id: absent-function.yaml}\ncodelists: [absent-codelists.yaml]\n",
    );
            let entry = study.path("root.yaml");
            let environment = study.path("environment.yaml");
            let file_workflow::SelectionError::Preparation(error) =
                file_workflow::select(&entry, &environment, Language::Python)
                    .err()
                    .unwrap()
            else {
                panic!("retained environment rejection")
            };
            let expected = [
                ("lock", "missing.lock"),
                ("functions.id", "absent-function.yaml"),
                ("codelists[0]", "absent-codelists.yaml"),
            ];
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert_eq!(rows.len(), expected.len());
            for (row, (declaration, path)) in rows.iter().zip(expected) {
                assert_eq!(row.condition, "project_environment_invalid");
                assert_eq!(row.spec_paths, [declaration]);
                let context: yamaa_core::json::Value = row.context.clone();
                assert_eq!(context["path"], path);
                assert_eq!(context["cause"], "missing");
                assert_eq!(context["environment_path"], declaration);
            }
            assert_eq!(
                rows,
                yamaa_adapters::project_application::check(&entry, &environment, Language::Python),
            );
            let Error::Environment(owner) = &error else {
                unreachable!()
            };
            assert_eq!(owner.capture_reads(), 1);
            assert!(!study.0.join("root.yaml").exists());

            study.write("missing.lock", "invalid packaging lock");
            study.write("absent-function.yaml", "[");
            study.write("absent-codelists.yaml", "[");
            let rows =
                yamaa_adapters::project_application::check(&entry, &environment, Language::Python);
            assert_eq!(rows.len(), 3);
            let context: yamaa_core::json::Value = rows[0].context.clone();
            assert_eq!(context["cause"], "invalid_packaging_lock");
            for (row, path) in rows[1..]
                .iter()
                .zip(["absent-function.yaml", "absent-codelists.yaml"])
            {
                assert_eq!(row.condition, "invalid_yaml");
                let context: yamaa_core::json::Value = row.context.clone();
                assert_eq!(context["path"], path);
                assert!(context["reason"].as_str().is_some());
                assert!(context["line"].as_u64().is_some());
            }
        }

        #[test]
        fn retained_environment_schema_diagnostics_preserve_wide_integer_tokens() {
            const DIGITS: &str = "92233720368547758081234567890";
            let study = Study::new();
            study.write(
                "environment.yaml",
                &format!("schema_version: {DIGITS}\nlanguage: python\n"),
            );
            let error = study.rejected();
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition, "schema_version_mismatch");
            assert_eq!(rows[0].spec_paths, ["schema_version"]);
            let context: yamaa_core::json::Value = rows[0].context.clone();
            assert_eq!(context["actual"].as_number().unwrap().as_str(), DIGITS);
            assert_eq!(context["expected"], "1.0");
            assert!(yamaa_adapters::json_wire::encode(&rows[0].context)
                .contains(&format!("\"actual\":{DIGITS}")));
            assert_eq!(
                rows,
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("environment.yaml"),
                    Language::Python,
                ),
            );
            let Error::Environment(owner) = &error else {
                panic!("root environment finding keeps its retained owner")
            };
            assert_eq!(owner.capture_reads(), 1);
            assert!(!study.0.join("root.yaml").exists());
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                producer_check::rejected(&error, Language::Python).unwrap(),
                rows,
            );
            assert_eq!(owner.capture_reads(), 1);
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
                let environment = "schema_version: '1.0'\ncodelists: [missing.yaml, ct.yaml]\n";
                study.write("environment.yaml", environment);
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
                    &study.path("environment.yaml"),
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
                assert_eq!(rejected.root.source().bytes, environment.as_bytes());
                assert_eq!(owner.capture_reads(), if structural { 2 } else { 1 });
                let reads = owner.capture_reads();
                let held_root = rejected.root.source().bytes.as_ptr();
                let held_error = owner.error() as *const _;
                let producer_check::Error::ResourceLimit(refusal) =
                    producer_check::rejected(error, Language::Python).unwrap_err()
                else {
                    panic!("whole projection retains the typed refusal")
                };
                assert_eq!(refusal.resource, expected.resource);
                assert_eq!(refusal.limit, expected.limit);
                assert_eq!(refusal.actual, expected.actual);
                assert_eq!(refusal.spec_paths, ["codelists[1]"]);
                let expected_rows = refusal.issues("environment", "resource_limit");
                assert_eq!(
                    yamaa_adapters::project_application::check(
                        &study.path("root.yaml"),
                        &study.path("environment.yaml"),
                        Language::Python,
                    ),
                    expected_rows,
                );
                let application_error = yamaa_adapters::project_application::prepare(
                    &study.path("root.yaml"),
                    &study.path("environment.yaml"),
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
                        producer_check::rejected(error, Language::Python),
                        Err(producer_check::Error::ResourceLimit(again)) if again == refusal
                    ));
                    let rows = yamaa_adapters::project_application::selection_failure(
                        &selection,
                        Language::Python,
                    );
                    assert_eq!(rows.len(), 1);
                    assert_eq!(rows[0].phase, "engine");
                    assert_eq!(rows[0].condition, "engine_rejected");
                    assert_eq!(rows[0].requirement, None);
                    assert_eq!(rows[0].spec_paths, ["codelists[1]"]);
                    assert_eq!(rows, expected_rows);
                    assert_eq!(
                        rows[0].context.clone(),
                        yamaa_adapters::json!({
                            "stage": "environment", "code": "resource_limit",
                            "resource": expected.resource, "limit": expected.limit,
                            "actual": expected.actual,
                        }),
                    );
                    assert_eq!(
                        yamaa_adapters::project_application::failure(
                            &application_error,
                            Language::Python,
                        ),
                        rows,
                    );
                    assert_eq!(owner.capture_reads(), reads);
                    assert_eq!(owner.error() as *const _, held_error);
                    assert_eq!(rejected.root.source().bytes.as_ptr(), held_root);
                    assert_eq!(rejected.root.source().bytes, environment.as_bytes());
                    assert_eq!(
                        application_owner.root.source().bytes.as_ptr(),
                        held_application_root,
                    );
                    assert_eq!(
                        application_owner.root.source().bytes,
                        environment.as_bytes(),
                    );
                }
            }
        }

        #[test]
        fn root_decoder_failures_keep_native_facts_across_public_views_without_rereads() {
            use yamaa_adapters::{
                file_project, issue_rows::Issue, project_application, project_source::Failure,
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
                    "schema_version: '1.0'\nfunctions: [\n".to_owned()
                };
                study.write("environment.yaml", &raw);
                let entry = study.path("root.yaml");
                let environment = study.path("environment.yaml");
                let selection = file_workflow::select(&entry, &environment, Language::Python)
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
                assert_eq!(identity, &environment);
                let expected = match error {
                    DecodeFailure::Limit(bound) if limited => {
                        assert_eq!(
                            bound,
                            &Limits::yaml_depth(Limits::YAML_DEPTH + 1).unwrap_err()
                        );
                        Issue {
                            phase: "engine".into(),
                            condition: "engine_rejected".into(),
                            requirement: None,
                            spec_paths: vec!["$".into()],
                            context: yamaa_adapters::json!({
                                "stage":"environment", "code":"resource_limit",
                                "resource":"yaml_depth", "limit":Limits::YAML_DEPTH,
                                "actual":Limits::YAML_DEPTH + 1,
                            }),
                            source_span: None,
                            operand_route: None,
                        }
                    }
                    DecodeFailure::InvalidYaml { position, reason } if !limited => Issue {
                        phase: "validation".into(),
                        condition: "invalid_yaml".into(),
                        requirement: Some("REQ-0280".into()),
                        spec_paths: vec!["$".into()],
                        context: yamaa_adapters::json!({
                            "line":position.line, "column":position.column, "reason":reason,
                            "path":environment, "entry":environment, "environment_path":"$",
                        }),
                        source_span: None,
                        operand_route: None,
                    },
                    cause => panic!("expected retained decoder cause: {cause:?}"),
                };
                assert_eq!(owner.capture_reads(), 1);
                let (_, bytes) = owner.captured_sources().next().unwrap();
                assert_eq!(bytes, raw.as_bytes());
                let held_bytes = bytes.as_ptr();
                let application_error =
                    project_application::prepare(&entry, &environment, Language::Python)
                        .err()
                        .unwrap();
                assert_eq!(
                    project_application::check(&entry, &environment, Language::Python),
                    std::slice::from_ref(&expected)
                );
                assert!(!study.0.join("root.yaml").exists());
                fs::remove_dir_all(&study.0).unwrap();
                for _ in 0..2 {
                    assert_eq!(
                        project_application::selection_failure(&selection, Language::Python),
                        std::slice::from_ref(&expected)
                    );
                    assert_eq!(
                        project_application::failure(&application_error, Language::Python),
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
        fn supplied_environment_directory_names_its_path_and_file_kind_before_entry_reads() {
            let study = Study::new();
            fs::create_dir(study.0.join("directory.yaml")).unwrap();
            let path = study.path("directory.yaml");
            let rows = yamaa_adapters::project_application::check(
                &study.path("absent-spec.yaml"),
                &path,
                Language::Python,
            );
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition, "project_environment_invalid");
            assert_eq!(rows[0].spec_paths, ["environment"]);
            let context: yamaa_core::json::Value = rows[0].context.clone();
            assert_eq!(context["path"], path);
            assert_eq!(context["cause"], "not_regular_file");
        }

        #[test]
        fn complete_original_diamond_checks_each_canonical_node_once_without_data() {
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
            study.write("root.yaml", &yaml("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml}, Q: {path: right.csv, schema: './right.yaml'}}", "L", "root.csv", true));
            let graph = study.prepare().unwrap();
            assert_eq!(graph.capture_reads(), 5); // one environment + four canonical nodes
            assert_eq!(graph.graph().metadata().order(), [3, 1, 2, 0]);
            let expected = ["root.yaml", "left.yaml", "leaf.yaml"].map(|name| yamaa_adapters::issue_rows::Issue {
        phase: "validation".into(), condition: "invalid_regex".into(), requirement: Some("REQ-0827".into()),
        spec_paths: vec!["columns.VALUE.verifications[0].matches.pattern".into()],
        context: yamaa_adapters::json!({"pattern":"é(", "source":study.path(name), "entry":study.path("root.yaml")}),
        source_span: None, operand_route: None,
    });
            assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
            assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(producer_check::prepared(&graph).unwrap(), expected);
            assert_eq!(
                graph.environment().root().source().bytes,
                b"schema_version: '1.0'\n"
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
                &study.path("environment.yaml"),
                Language::Python,
            )
            .unwrap();
            let expected = yamaa_adapters::project_check::prepared(ordinary.run()).unwrap();
            assert_eq!(expected.len(), 1);
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("environment.yaml"),
                    Language::Python
                ),
                expected
            );
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("never-written.csv").exists());
        }

        #[test]
        fn producer_typed_rejections_keep_complete_authored_rows_and_original_owners() {
            for (input, requirement, condition, path) in [
                (
                    "{P: {path: wrong.csv, schema: leaf.yaml}}",
                    "REQ-0534",
                    "producer_output_path_mismatch",
                    "input.P.path",
                ),
                (
                    "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
                    "REQ-0523",
                    "redundant_field_type",
                    "input.P.types.ID",
                ),
            ] {
                let study = Study::new();
                study.write(
                    "leaf.yaml",
                    &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
                );
                study.write("root.yaml", &yaml(input, "P", "root.csv", false));
                let error = study.rejected();
                let rows = producer_check::rejected(&error, Language::Python).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].condition, condition);
                assert_eq!(rows[0].requirement.as_deref(), Some(requirement));
                assert_eq!(
                    rows[0].spec_paths,
                    if requirement == "REQ-0534" {
                        vec![path, "input.P.schema"]
                    } else {
                        vec![path]
                    }
                );
                assert_eq!(rows[0].context.clone()["source"], study.path("root.yaml"));
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(
                    producer_check::rejected(&error, Language::Python).unwrap(),
                    rows
                );
                let Error::Graph(graph) = &error else {
                    panic!("retained graph");
                };
                assert_eq!(graph.capture_reads(), 3);
                assert_eq!(graph.documents().len(), 2);
                assert_eq!(graph.captured_sources().count(), 3);
            }
        }

        #[test]
        fn inherited_producer_types_rejection_names_the_declaring_parent_on_check_and_domain() {
            let study = Study::new();
            study.write(
                "parent.yaml",
                "schema_version: \"1.0\"\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: {path: input/dm.csv, types: {RANDDT: date}}\noutput:\n  path: adsl.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: DM.RANDDT\n",
            );
            study.write(
                "producer/dm.yaml",
                "schema_version: \"1.0\"\ndomain: DM\nkeys: [USUBJID]\ninput:\n  RAW: {path: ../raw/dm_raw.csv, types: {RANDDT: date}}\noutput:\n  path: ../input/dm.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: RAW.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: RAW.RANDDT\n",
            );
            study.write(
                "root.yaml",
                "schema_version: \"1.0\"\nparents: parent.yaml\ninput:\n  DM: {schema: producer/dm.yaml}\n",
            );
            let error = study.rejected();
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition, "redundant_field_type");
            assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0523"));
            assert_eq!(rows[0].spec_paths, ["input.DM.types.RANDDT"]);
            assert_eq!(
                rows[0].context.clone(),
                yamaa_adapters::json!({"dataset":"DM", "declaring_sources":[study.path("parent.yaml")], "entry":study.path("root.yaml"), "field":"RANDDT", "source":study.path("root.yaml"), "type":"date"})
            );
            assert_eq!(
                yamaa_adapters::project_application::check(
                    &study.path("root.yaml"),
                    &study.path("environment.yaml"),
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
                    yamaa_core::domain_entry::Request {
                        specification: &entry,
                        environment: None,
                    },
                    identity(),
                ),
                rows
            );
            assert_eq!(
                yamaa_adapters::file_application::domain(
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
        fn actual_compilation_failure_and_cycle_are_projected_without_recompilation() {
            for cycle in [false, true] {
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
                let leaf = if cycle {
                    yaml(
                        "{R: {path: root.csv, schema: './root.yaml'}}",
                        "R",
                        "leaf.csv",
                        false,
                    )
                } else {
                    yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}")
                };
                study.write("leaf.yaml", &leaf);
                let error = study.rejected();
                let rows = producer_check::rejected(&error, Language::Python).unwrap();
                assert!(!rows.is_empty());
                assert_eq!(
                    rows[0].condition,
                    if cycle {
                        "producer_workflow_cycle"
                    } else {
                        "unknown_project_function"
                    }
                );
                assert_eq!(
                    rows[0].requirement.as_deref(),
                    Some(if cycle { "REQ-0534" } else { "REQ-0698" })
                );
                let Error::Graph(graph) = &error else {
                    panic!("graph");
                };
                assert_eq!(graph.capture_reads(), 3);
                assert_eq!(graph.documents().len(), 2);
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(
                    producer_check::rejected(&error, Language::Python).unwrap(),
                    rows
                );
            }
        }

        #[test]
        fn raw_environment_rejection_precedes_the_first_graph_read() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let raw = "schema_version: '1.0'\nfunctions: [\n";
            study.write("environment.yaml", raw);
            let error = study.rejected();
            let Error::Environment(environment) = &error else {
                panic!("early environment");
            };
            assert_eq!(environment.capture_reads(), 1);
            assert_eq!(
                environment.captured_sources().next().unwrap().1,
                raw.as_bytes()
            );
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition, "invalid_yaml");
            let context: yamaa_core::json::Value = rows[0].context.clone();
            assert_eq!(context["path"], "environment.yaml");
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                environment.captured_sources().next().unwrap().1,
                raw.as_bytes()
            );
        }

        #[test]
        fn supported_environment_and_original_inherited_schema_causes_are_complete() {
            let study = Study::new();
            study.write("environment.yaml", "schema_version: '1.0'\nlanguage: r\n");
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let error = study.rejected();
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert!(rows
                .iter()
                .any(|r| r.requirement.as_deref() == Some("REQ-0696")));
            let Error::Environment(environment) = &error else {
                panic!("environment");
            };
            assert_eq!(environment.capture_reads(), 1);
            study.write("environment.yaml", "schema_version: '1.0'\n");
            study.write(
                "parent/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
                    .replace("type: str", "type: invalid"),
            );
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparents: [parent/base.yaml]\ndomain: CHILD\n",
            );
            let error = study.rejected();
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert!(!rows.is_empty());
            assert!(rows
                .iter()
                .any(|r| r.context.clone()["source"] == study.path("parent/base.yaml")));
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                producer_check::rejected(&error, Language::Python).unwrap(),
                rows
            );
        }

        #[test]
        fn inherited_graph_findings_retain_the_actual_declaring_layer_after_owner_files_drop() {
            let study = Study::new();
            study.write(
                "producer/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "../leaf.csv", true),
            );
            study.write(
                "leaf.yaml",
                "schema_version: '1.0'\nparents: [producer/base.yaml]\ndomain: LEAF\n",
            );
            let parent = yaml("{P: {path: ../leaf.csv, schema: ../leaf.yaml}, Q: {path: ../leaf.csv, schema: '../leaf.yaml'}}", "P", "../root.csv", false)
        .replace("columns:\n", "intermediates: [{id: Q_REFERENCE, dataset: Q}]\ncolumns:\n")
        .replace("output:", "  - {name: OTHER, type: str, label: Other, derivation: Q_REFERENCE.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, OTHER]");
            study.write("parents/base.yaml", &parent);
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparents: [parents/base.yaml]\ndomain: ROOT\n",
            );
            let graph = study.prepare().unwrap();
            assert_eq!(graph.capture_reads(), 5);
            let nodes = graph.graph().nodes();
            assert!(std::sync::Arc::ptr_eq(
                &nodes[0].producers[0].document,
                &nodes[0].producers[1].document
            ));
            let expected = yamaa_adapters::json!({"declaring_sources":[study.path("producer/base.yaml")],
        "source":study.path("leaf.yaml"), "entry":study.path("root.yaml"), "pattern":"é("});
            let rows = producer_check::prepared(&graph).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].context, expected);
            assert_eq!(
                graph.graph().metadata().nodes()[0].producers()[0]
                    .schema_origin()
                    .declaring_source(),
                study.path("parents/base.yaml")
            );
            let retained = graph.graph().nodes()[1].document.clone();
            drop(graph);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                retained.declaring_source("columns.VALUE.verifications[0].matches.pattern"),
                Some(study.path("producer/base.yaml").as_str())
            );
        }

        #[test]
        fn unsupported_native_capture_keeps_exact_refusal_and_declaring_request() {
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
            let Error::Graph(graph) = &error else {
                panic!("graph");
            };
            assert!(matches!(
                graph.error(),
                yamaa_adapters::file_graph::Error::Resource(
                    yamaa_adapters::file_resources::Error::Missing
                )
            ));
            assert_eq!(graph.capture_reads(), 2);
            assert_eq!(graph.origin().unwrap().dataset, "P");
            assert!(
                matches!(producer_check::rejected(&error, Language::Python), Err(producer_check::Error::Original(original)) if std::ptr::eq(original, &error))
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(graph.documents().len(), 1);
            assert_eq!(graph.captured_sources().count(), 2);
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
                .replace("    label: Reported value\n", "");
            study.write("leaf.yaml", &original);
            let error = study.rejected();
            let rows = producer_check::rejected(&error, Language::Python).unwrap();
            assert!(rows
                .iter()
                .any(|row| row.requirement.as_deref() == Some("REQ-0534")
                    && row.condition == "invalid_producer_contract"));
            study.write(
        "environment.yaml",
        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {id: broken.yaml}\n",
    );
            study.write("uv.lock", "version = 1\npackage = []\n");
            let malformed = "function: [\n";
            study.write("broken.yaml", malformed);
            let failure = study.rejected();
            let Error::Environment(environment) = &failure else {
                panic!("original environment");
            };
            assert_eq!(environment.capture_reads(), 3);
            assert_eq!(
                environment
                    .captured_sources()
                    .find(|(path, _)| path == &study.path("broken.yaml"))
                    .unwrap()
                    .1,
                malformed.as_bytes()
            );
            let failures = producer_check::rejected(&failure, Language::Python).unwrap();
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].condition, "invalid_yaml");
            let context: yamaa_core::json::Value = failures[0].context.clone();
            assert_eq!(context["path"], "broken.yaml");
            assert_eq!(context["environment_path"], "functions.id");
            fs::remove_dir_all(&study.0).unwrap();
            let Error::Graph(graph) = &error else {
                panic!("original contract");
            };
            assert_eq!(graph.documents()[1].source().bytes, original.as_bytes());
            assert_eq!(
                producer_check::rejected(&error, Language::Python).unwrap(),
                rows
            );
            assert_eq!(
                environment
                    .captured_sources()
                    .find(|(path, _)| path == &study.path("broken.yaml"))
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
            let study = Study(root);
            study.write("environment.yaml", "schema_version: '1.0'\n");
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
            let graph = study.prepare().unwrap();
            assert!(producer_check::prepared(&graph).unwrap().is_empty());
            assert_eq!(
                graph.graph().nodes()[0].producers[0].schema_identity,
                study.path("leaf.yaml")
            );
            assert_eq!(graph.capture_reads(), 3);
        }

        #[test]
        fn ordinary_original_input_facts_survive_files_without_study_authority() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "never-written.csv", false),
            );
            let graph = study.prepare().unwrap();
            assert_eq!(graph.capture_reads(), 2);
            assert_eq!(graph.graph().nodes().len(), 1);
            assert!(graph.graph().nodes()[0].producers.is_empty());
            let document = &graph.graph().nodes()[0].document;
            let raw = document.normalization_input();
            let input = raw.field(raw.root(), "input").unwrap();
            let declaration = raw.field(input, "RAW").unwrap();
            assert!(!yamaa_core::producer_admission::is_producer_input(
                raw,
                declaration
            ));
            let original = fs::read(study.0.join("root.yaml")).unwrap();
            drop(study);
            assert_eq!(document.source().bytes, original);
            assert!(producer_check::prepared(&graph).unwrap().is_empty());
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
                "schema_version: '1.0'\nparents: [base.yaml]\ndomain: TEST\n",
            );
            let graph = study.prepare().unwrap();
            assert_eq!(graph.capture_reads(), 4);
            assert_eq!(graph.graph().nodes().len(), 2);
            assert_eq!(graph.graph().nodes()[0].producers.len(), 1);
            let document = &graph.graph().nodes()[0].document;
            assert!(document
                .raw()
                .document
                .field(document.raw().document.root(), "input")
                .is_none());
            let raw = document.normalization_input();
            let input = raw.field(raw.root(), "input").unwrap();
            let declaration = raw.field(input, "P").unwrap();
            assert!(yamaa_core::producer_admission::is_producer_input(
                raw,
                declaration
            ));
            assert_eq!(
                document
                    .written_input_origin("P", "schema")
                    .unwrap()
                    .declaring_source,
                study.path("base.yaml")
            );
            drop(study);
            assert!(producer_check::prepared(&graph).unwrap().is_empty());
        }

        #[test]
        fn rejected_null_schema_keeps_the_original_shipped_schema_cause_and_raw_entry() {
            let study = Study::new();
            let original = yaml(
                "{RAW: {path: never-read.csv, schema: null}}",
                "RAW",
                "never-written.csv",
                false,
            );
            study.write("root.yaml", &original);
            let rejection = study.rejected();
            let Error::Graph(graph) = &rejection else {
                panic!("the original root rejection must remain owned");
            };
            assert_eq!(graph.capture_reads(), 2);
            assert!(graph.documents().is_empty());
            drop(study);
            let retained = graph.retained_sources();
            assert_eq!(retained.len(), 2);
            assert!(retained
                .iter()
                .any(|(name, bytes)| name.ends_with("/root.yaml")
                    && bytes.as_ref() == original.as_bytes()));
            let findings = producer_check::rejected(&rejection, Language::Python).unwrap();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].condition, "invalid_field_type");
            assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0287"));
            assert_eq!(findings[0].spec_paths, ["input.RAW.schema"]);
            assert_eq!(graph.capture_reads(), 2);
        }

        impl Study {
            fn select(&self) -> Result<file_workflow::Selected, file_workflow::SelectionError> {
                file_workflow::select_with_resources(
                    Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
                    "root.yaml",
                    "environment.yaml",
                    Language::Python,
                    shipped_schema::capture().unwrap(),
                    shipped_schema::capture_environment().unwrap(),
                )
            }
            fn selection_rejection(&self) -> file_workflow::SelectionError {
                match self.select() {
                    Err(error) => error,
                    Ok(_) => panic!("expected selected preparation rejection"),
                }
            }
        }
        struct NoActivation;
        impl yamaa_core::project_activation::ActivationPort for NoActivation {
            type Handle = ();
            type Error = ();
            fn verify_lock(
                &mut self,
                _: Language,
                _: &yamaa_core::project_environment::LockReference,
                _: &[yamaa_core::function_signature::ProjectFunctionIdentity],
            ) -> Result<(), ()> {
                panic!("functionless workflow must not activate")
            }
            fn bind(
                &mut self,
                _: &yamaa_core::function_signature::ProjectFunctionIdentity,
                _: &yamaa_core::function_signature::LogicalSignature,
            ) -> Result<(), ()> {
                panic!("functionless workflow must not bind")
            }
            fn invoke(
                &mut self,
                _: &(),
                _: &[yamaa_core::function_invocation::Argument<'_>],
            ) -> Result<yamaa_core::value::Value, yamaa_core::function_invocation::HostError<()>>
            {
                panic!("functionless workflow must not call")
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
            use file_workflow::Selected;
            use yamaa_core::specification_output::ArtifactPort;
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let Selected::Ordinary(mut selected) = study.select().unwrap() else {
                panic!("ordinary capability");
            };
            assert_eq!(selected.capture_reads(), 2);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("root.csv").exists());
            // An independent study source is first created and read after preparation.
            study.write("never-read.csv", "ID,VALUE\n1,alpha\n2,beta\n");
            let mut ordinary = yamaa_adapters::file_project::FileProject::prepare(
                Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                "root.yaml",
                "environment.yaml",
                Language::Python,
                shipped_schema::capture().unwrap(),
                shipped_schema::capture_environment().unwrap(),
            )
            .unwrap();
            let selected_attempt = selected.build(&mut NoActivation, Scope::default());
            let ordinary_attempt = ordinary.build(&mut NoActivation, Scope::default());
            assert_eq!(selected.capture_reads(), 3);
            let selected_result = yamaa_adapters::project_report::build_result(
                selected.run(),
                &selected_attempt,
                ordinary_report_id(),
                &[],
            )
            .unwrap();
            let ordinary_result = yamaa_adapters::project_report::build_result(
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
            struct Save(Vec<(String, Vec<u8>)>);
            impl ArtifactPort for Save {
                type Error = ();
                fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
                    self.0.push((path.into(), bytes.into()));
                    Ok(())
                }
            }
            let retained = selected.retained_run();
            drop(selected);
            drop(ordinary);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                retained.document().source().bytes,
                yaml("{RAW: never-read.csv}", "RAW", "root.csv", false).as_bytes()
            );
            let mut publisher = Save(vec![]);
            assert_eq!(
                selected_result.save(&mut publisher).unwrap()["artifacts"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                publisher.0,
                [("root.csv".into(), b"ID,VALUE\n1,alpha\n2,beta\n".to_vec())]
            );
        }

        #[test]
        fn selected_inherited_producer_seeds_original_closure_with_shared_aliases_and_diamond() {
            use file_workflow::Selected;
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
        .replace("columns:\n", "intermediates: [{id: RIGHT, dataset: R}, {id: ALIAS, dataset: Q}]\ncolumns:\n")
        .replace("output:", "  - {name: RIGHT_VALUE, type: str, label: Right value, derivation: RIGHT.VALUE}\n  - {name: ALIAS_VALUE, type: str, label: Alias value, derivation: ALIAS.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, RIGHT_VALUE, ALIAS_VALUE]");
            study.write("parents/base.yaml", &parent);
            let entry = "schema_version: '1.0'\nparents: [parents/base.yaml]\ndomain: ROOT\n";
            study.write("root.yaml", entry);
            let Selected::Producer(graph) = study.select().unwrap() else {
                panic!("inherited producer capability");
            };
            assert_eq!(graph.capture_reads(), 6); // environment, root, parent, left/right/leaf
            let nodes = graph.graph().nodes();
            assert_eq!(nodes.len(), 4);
            assert_eq!(nodes[0].document.source().bytes, entry.as_bytes());
            assert!(nodes[0]
                .document
                .raw()
                .document
                .field(nodes[0].document.raw().document.root(), "input")
                .is_none());
            assert!(std::sync::Arc::ptr_eq(
                &nodes[0].producers[1].document,
                &nodes[0].producers[2].document
            ));
            assert!(std::sync::Arc::ptr_eq(
                &nodes[1].producers[0].document,
                &nodes[2].producers[0].document
            ));
            assert_eq!(graph.graph().metadata().order(), [3, 1, 2, 0]);
            let rows = producer_check::prepared(&graph).unwrap();
            assert_eq!(rows.len(), 1);
            let context: yamaa_core::json::Value = rows[0].context.clone();
            assert_eq!(
                context["declaring_sources"],
                yamaa_adapters::json!([study.path("parents/base.yaml")])
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(producer_check::prepared(&graph).unwrap(), rows);
            assert_eq!(graph.capture_reads(), 6);
        }

        #[test]
        fn selected_ordinary_compile_failure_keeps_original_run_environment_and_raw_resources() {
            use file_workflow::SelectionError;
            let study = Study::new();
            let raw = yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
                .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}");
            study.write("root.yaml", &raw);
            let SelectionError::Ordinary(rejected) = study.selection_rejection() else {
                panic!("original ordinary compiler rejection");
            };
            assert_eq!(rejected.capture_reads(), 2);
            assert_eq!(rejected.entry(), "root.yaml");
            let rows = yamaa_adapters::project_check::rejected(rejected.run()).unwrap();
            assert_eq!(rows[0].condition, "unknown_project_function");
            assert_eq!(rows[0].requirement.as_deref(), Some("REQ-0698"));
            fs::remove_dir_all(&study.0).unwrap();
            let held = rejected.retained_sources();
            assert_eq!(held.len(), 2);
            assert!(held
                .iter()
                .any(|(identity, bytes)| identity.ends_with("root.yaml")
                    && bytes.as_ref() == raw.as_bytes()));
            assert_eq!(
                yamaa_adapters::project_check::rejected(rejected.run()).unwrap(),
                rows
            );
            assert_eq!(
                rejected.run().captured.root().source().bytes,
                b"schema_version: '1.0'\n"
            );
        }

        #[test]
        fn selected_invalid_original_declarations_never_fall_back_to_ordinary_compilation() {
            use file_workflow::SelectionError;
            for (input, expected) in [
                ("{RAW: {path: never-read.csv, schema: null}}", "REQ-0287"),
                ("{P: {path: wrong.csv, schema: leaf.yaml}}", "REQ-0534"),
                (
                    "{P: {path: leaf.csv, schema: leaf.yaml, types: {ID: int}}}",
                    "REQ-0523",
                ),
            ] {
                let study = Study::new();
                study.write("root.yaml", &yaml(input, "RAW", "root.csv", false));
                study.write(
                    "leaf.yaml",
                    &yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false),
                );
                let SelectionError::Preparation(error @ Error::Graph(_)) =
                    study.selection_rejection()
                else {
                    panic!("owned original metadata/graph rejection");
                };
                let rows = producer_check::rejected(&error, Language::Python).unwrap();
                assert!(
                    rows.iter()
                        .any(|r| r.requirement.as_deref() == Some(expected)),
                    "expected {expected}; actual {rows:?}"
                );
                let Error::Graph(owner) = &error else {
                    unreachable!()
                };
                let reads = owner.capture_reads();
                assert_eq!(reads, if expected == "REQ-0287" { 2 } else { 3 });
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(
                    producer_check::rejected(&error, Language::Python).unwrap(),
                    rows
                );
                assert_eq!(owner.capture_reads(), reads);
            }
        }

        #[test]
        fn selected_raw_environment_and_root_failures_keep_exact_capture_prefixes() {
            use file_workflow::SelectionError;
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            study.write("environment.yaml", "schema_version: '1.0'\nfunctions: [\n");
            let SelectionError::Preparation(Error::Environment(environment)) =
                study.selection_rejection()
            else {
                panic!("raw environment rejection");
            };
            assert_eq!(environment.capture_reads(), 1);
            study.write("environment.yaml", "schema_version: '1.0'\n");
            study.write("root.yaml", "[");
            let SelectionError::Preparation(root @ Error::Graph(_)) = study.selection_rejection()
            else {
                panic!("raw root rejection");
            };
            let rows = producer_check::rejected(&root, Language::Python).unwrap();
            assert_eq!(rows[0].condition, "invalid_yaml");
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(environment.retained_sources().len(), 1);
            assert_eq!(
                producer_check::rejected(&root, Language::Python).unwrap(),
                rows
            );
        }

        #[test]
        fn selected_producer_cycle_missing_labels_and_compiler_failures_keep_typed_whole_owners() {
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
                        .replace("label: Identifier, ", "")
                        .replace("    label: Reported value\n", ""),
                    _ => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}"),
                };
                study.write("leaf.yaml", &leaf);
                let SelectionError::Preparation(error @ Error::Graph(_)) =
                    study.selection_rejection()
                else {
                    panic!("whole producer rejection");
                };
                let rows = producer_check::rejected(&error, Language::Python).unwrap();
                let requirement = ["REQ-0534", "REQ-0534", "REQ-0698"][mode];
                assert!(
                    rows.iter()
                        .any(|r| r.requirement.as_deref() == Some(requirement)),
                    "{rows:?}"
                );
                let Error::Graph(owner) = &error else {
                    unreachable!()
                };
                assert_eq!(owner.capture_reads(), 3);
                assert_eq!(owner.documents().len(), 2);
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(
                    producer_check::rejected(&error, Language::Python).unwrap(),
                    rows
                );
            }
        }

        #[test]
        fn selected_entry_paths_keep_absolute_environment_policy_and_early_owned_refusal() {
            use file_workflow::{Selected, SelectionError};
            let study = Study::new();
            study.write("yamaa-project.yaml", "version: '1.0'\ndata_roots: []\n");
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let Selected::Ordinary(project) = file_workflow::select(
                &study.path("root.yaml"),
                &study.path("environment.yaml"),
                Language::Python,
            )
            .unwrap() else {
                panic!("configured ordinary capability");
            };
            assert_eq!(project.capture_reads(), 2); // environment + root; no configuration
            let Err(SelectionError::Preparation(Error::Environment(owner))) =
                file_workflow::select(&study.path("root.yaml"), "", Language::Python)
            else {
                panic!("original lexical environment refusal");
            };
            assert_eq!(owner.capture_reads(), 0);
            assert!(matches!(
                owner.error(),
                yamaa_adapters::file_project::Error::Resource(
                    yamaa_adapters::file_resources::Error::InvalidPath
                )
            ));
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(owner.retained_sources().len(), 0);
            assert_eq!(
                project.run().captured_environment().root().source().bytes,
                b"schema_version: '1.0'\n"
            );
        }

        #[test]
        fn absent_environment_selects_producer_without_capturing_or_inventing_metadata() {
            let study = Study::new();
            // A nearby environment is deliberately unreadable as YAML. Absence must
            // neither discover it nor convert it into an empty captured document.
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
                Language::Python,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "original producer admission: {:?}",
                    yamaa_adapters::project_application::selection_failure(
                        &error,
                        Language::Python
                    )
                )
            });
            let file_workflow::Selected::Producer(graph) = selected else {
                panic!("schema link selects producer");
            };
            assert_eq!(graph.capture_reads(), 2);
            assert!(graph.environment().root_optional().is_none());
            assert!(graph.environment().lock().is_none());
            assert!(graph.environment().captures().is_empty());
            assert!(graph
                .graph()
                .metadata()
                .environment()
                .functions()
                .is_empty());
            assert_eq!(graph.captured_sources().count(), 2);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("leaf.csv").exists());
            assert!(!study.0.join("root.csv").exists());
            fs::remove_dir_all(&study.0).unwrap();
            assert!(producer_check::prepared(&graph).unwrap().is_empty());
        }

        #[test]
        fn plain_workflow_profile_inspection_keeps_physical_kind_before_study_capture() {
            use yamaa_core::diagnostic::ConditionCode as C;
            use yamaa_core::json::Value as JsonValue;
            use yamaa_core::specification::PrepareError;

            let study = Study::new();
            study.write("environment.yaml", "[");
            study.write("parent/base.yaml", "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
            study.write(
                "entry/root.yaml",
                "schema_version: '1.0'\nparents: [../parent/base.yaml]\n",
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
                let Err(file_workflow::SelectionError::Plain(owner)) =
                    file_workflow::select_request(
                        yamaa_core::build::Request {
                            specification: &study.path("entry/root.yaml"),
                            environment: None,
                        },
                        Language::Python,
                    )
                else {
                    panic!("plain route must retain its preparation rejection");
                };
                assert_eq!(
                    owner.capture_reads(),
                    2,
                    "only root and parent metadata were captured"
                );
                let yamaa_adapters::file_preparation::Error::Compile(
                    yamaa_adapters::specification_run::Error::Prepare(PrepareError::Invalid(
                        findings,
                    )),
                ) = owner.error()
                else {
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
                    .all(|(identity, _)| !identity.ends_with("environment.yaml")
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
