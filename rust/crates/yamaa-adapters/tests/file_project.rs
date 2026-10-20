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
                // The project declares its codelists beside itself and links the
                // Python environment file, whose lock and function paths resolve
                // from env/.
                study.write("project.yaml", PROJECT);
                study.write(
                    "env/environment.yaml",
                    "schema_version: '1.0'\nlock: ../uv.lock\nfunctions: {id: id.yaml}\n",
                );
                study.write("uv.lock", "version = 1\npackage = []\n");
                study.write("env/id.yaml", "function: program.id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n");
                study.write(
                    "ct.yaml",
                    "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
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
            fn prepare(&self, root: &str, roots: &[String]) -> Result<FileProject, Error> {
                FileProject::prepare(
                    Resources::new(root, &self.text("entry"), roots).unwrap(),
                    "domain.yaml",
                    "../project.yaml",
                    Language::Python,
                    schemas::domain_schema(),
                    schemas::project_schemas(),
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
        const PROJECT: &str = "schema_version: '1.0'\nenvironments: {python: env/environment.yaml}\ncodelists: [ct.yaml]\n";

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
                &study.text("project.yaml"),
                Language::Python,
            )
            .unwrap();
            assert!(project.run().check_diagnostics().is_empty());
            assert!(!study.0.join("entry/input.csv").exists());
            // Entry, project, environment, lock, function and codelist metadata only.
            assert_eq!(project.capture_reads(), 6);
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
            let identity = |name| study.identity(name);
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            let reads = project.capture_reads();
            let mut activation = Activation {
                mismatch: true,
                ..Default::default()
            };
            let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
            let run = project.retained_run();
            study.write("env/id.yaml", "changed after preparation");
            study.write(
                "env/environment.yaml",
                "changed environment after preparation",
            );
            study.write("project.yaml", "changed root after preparation");
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
            let context = yamaa_core::json::Value::Object(result.issues()[0].context.clone());
            // The external function file is the source; the project is the entry.
            assert_eq!(context["source"], identity("env/id.yaml"));
            assert_eq!(context["entry"], identity("project.yaml"));
            assert_eq!(context["environment_path"], "functions.id");
            assert_eq!(context["actual"], 8);
            assert_eq!(context["expected"], 7);
            assert_eq!(
                result.observations()["source_reads"],
                yamaa_adapters::json!([])
            );
            assert_eq!(activation.trace, ["lock", "bind", "call", "call"]);
            assert_eq!(reads, 6);
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
                "schema_version: '1.0'\nfunctions: {id: id.yaml}\n",
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
                "schema_version: '1.0'\nfunctions: {id: id.yaml}\n",
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
                assert_eq!(run_scope.capture_reads(), reads);
            }
        }

        #[test]
        fn native_project_retains_metadata_and_reuses_exact_snapshots_with_fresh_activation() {
            let study = Study::new();
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 6);
            let captures = project.run().captured_environment();
            assert_eq!(
                captures.root().source().bytes,
                fs::read(study.0.join("project.yaml")).unwrap()
            );
            let environment = captures.environment_file().unwrap();
            assert_eq!(
                environment.source().bytes,
                fs::read(study.0.join("env/environment.yaml")).unwrap()
            );
            assert_eq!(
                captures.lock().unwrap().source.bytes,
                b"version = 1\npackage = []\n"
            );
            // The linked environment file is held beside the project root, not
            // among the function and codelist captures.
            assert_eq!(captures.captures().len(), 2);
            assert_eq!(captures.captures()[0].origin, Origin::Function("id".into()));
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
            let environment_bytes = environment.source().bytes.as_ptr();
            let lock_bytes = captures.lock().unwrap().source.bytes.as_ptr();
            let definition = project.run().environment().functions()[0]
                .definition()
                .tests
                .as_ptr();
            // Metadata bytes belong to the prepared capability, independently of later
            // filesystem changes. Study bytes are first captured only after activation.
            study.write("project.yaml", "[changed");
            study.write("env/environment.yaml", "[changed");
            study.write("env/id.yaml", "[changed");
            study.write("uv.lock", "changed");
            study.write("entry/domain.yaml", "[changed");
            study.write("entry/input.csv", "ID\n1\n");
            let mut activation = Activation::default();
            let first = project.build(&mut run_scope, &mut activation, Scope::default());
            assert!(first.boundary.is_ok());
            assert_eq!(first.activation.lock, LockObservation::Verified);
            assert_eq!(run_scope.capture_reads(), 7);
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
            assert_eq!(run_scope.capture_reads(), 7);
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
                    .environment_file()
                    .unwrap()
                    .source()
                    .bytes
                    .as_ptr(),
                environment_bytes
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
            assert_eq!(run_scope.capture_reads(), 6);
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
            let mut constructed = 0;
            for _ in 0..2 {
                let attempt = project.build_with(
                    &mut run_scope,
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
            assert_eq!(run_scope.capture_reads(), 7);
        }

        #[test]
        fn native_project_keeps_approved_roots_for_metadata_and_study_without_expanding_them() {
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

            // A function path resolving from env/ above the approved root is refused.
            study.write(
                "env/environment.yaml",
                "schema_version: '1.0'\nlock: ../uv.lock\nfunctions: {id: ../../unapproved.yaml}\n",
            );
            let Err(Error::Environment(EnvironmentFailure::Rejected(rejected))) =
                study.prepare(&study.text(""), &[])
            else {
                panic!("unapproved definition")
            };
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::Function("id".into()));
            assert_eq!(rejected.sources[0].written, "../../unapproved.yaml");
            assert!(matches!(
                rejected.sources[0].error,
                CaptureFailure::Port(ResourceError::OutsideRoots)
            ));
            assert!(rejected.environment_file.is_some());
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
                "schema_version: '1.0'\nparent: ../parent/base.yaml\n",
            );
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 7);
            let document = project.run().document();
            assert_eq!(document.parents().len(), 1);
            assert_eq!(document.written_source_path("SRC"), Some("data.csv"));
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            let attempt =
                project.build(&mut run_scope, &mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert_eq!(run_scope.capture_reads(), 8);
        }

        #[test]
        fn selected_ordinary_environment_owner_preserves_fresh_activation_and_inherited_data_origins(
        ) {
            use yamaa_adapters::file_workflow::{self, Selected};
            let study = Study::new();
            study.write("parent/base.yaml", &DOMAIN.replace("input.csv", "data.csv"));
            study.write(
                "entry/domain.yaml",
                "schema_version: '1.0'\nparent: ../parent/base.yaml\n",
            );
            let Selected::Ordinary(project) = file_workflow::select_with_resources(
                Resources::new(&study.text(""), &study.text("entry"), &[]).unwrap(),
                "domain.yaml",
                "../project.yaml",
                Language::Python,
                schemas::domain_schema(),
                schemas::project_schemas(),
            )
            .unwrap() else {
                panic!("ordinary capability");
            };
            let mut run_scope = project.run_scope(None);
            assert_eq!(project.capture_reads(), 7);
            assert_eq!(project.run().compiled().source().path, "../parent/data.csv");
            // A fresh activation still refuses before the absent authored study source.
            let mut failed = Activation {
                reject: true,
                ..Default::default()
            };
            let rejection = project.build(&mut run_scope, &mut failed, Scope::default());
            assert_eq!(failed.trace, ["lock"]);
            assert!(rejection.dataset.sources.is_empty());
            assert_eq!(run_scope.capture_reads(), 7);
            study.write("parent/data.csv", "ID\n1\n");
            let mut host = Activation::default();
            let first = project.build(&mut run_scope, &mut host, Scope::default());
            let second = project.build(&mut run_scope, &mut host, Scope::default());
            assert!(first.boundary.is_ok() && second.boundary.is_ok());
            assert_eq!(
                host.trace,
                ["lock", "bind", "call", "call", "call", "lock", "bind", "call", "call", "call"]
            );
            assert_eq!(run_scope.capture_reads(), 8);
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

        #[test]
        fn each_host_opens_only_the_environment_file_its_own_project_key_names() {
            use yamaa_adapters::{file_workflow, project_application::preparation_issues};
            let study = Study::new();
            // The R entry names a file that is not YAML. Python preparation never
            // opens it, and R preparation never opens the Python environment file,
            // its lock or its function.
            fs::create_dir(study.0.join("r")).unwrap();
            study.write("r/environment.yaml", "[");
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: env/environment.yaml, r: r/environment.yaml}\ncodelists: [ct.yaml]\n",
            );
            let project = study.prepare(&study.text(""), &[]).unwrap();
            // Project, Python environment, lock, function, codelist and entry.
            assert_eq!(project.capture_reads(), 6);
            assert!(project.run().check_diagnostics().is_empty());
            assert_eq!(
                project
                    .run()
                    .captured_environment()
                    .environment_file()
                    .unwrap()
                    .source()
                    .identity,
                study.identity("env/environment.yaml")
            );

            let Err(file_workflow::SelectionError::Preparation(error)) =
                file_workflow::select_with_resources(
                    Resources::new(&study.text(""), &study.text("entry"), &[]).unwrap(),
                    "domain.yaml",
                    "../project.yaml",
                    Language::R,
                    schemas::domain_schema(),
                    schemas::project_schemas(),
                )
            else {
                panic!("the R host opens and rejects only its own entry")
            };
            let file_workflow::Error::Environment(owner) = &error else {
                panic!("original environment owner")
            };
            let Error::Environment(EnvironmentFailure::Rejected(rejected)) = owner.error() else {
                panic!("retained source failure: {:?}", owner.error())
            };
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::Environment(Language::R));
            assert_eq!(rejected.sources[0].written, "r/environment.yaml");
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            // The project, the R environment file and the project's codelist only;
            // the entry is never read after the environment rejection.
            assert_eq!(
                owner
                    .captured_sources()
                    .map(|(path, _)| path)
                    .collect::<Vec<_>>(),
                ["project.yaml", "r/environment.yaml", "ct.yaml"].map(|name| study.identity(name))
            );
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "invalid_yaml");
            let context = yamaa_core::json::Value::Object(rows[0].context.clone());
            assert_eq!(context["entry"], study.identity("project.yaml"));
            assert_eq!(context["path"], "r/environment.yaml");
            assert_eq!(context["environment_path"], "environments.r");
        }

        #[test]
        fn environment_file_paths_resolve_beside_it_and_project_paths_beside_the_project() {
            use yamaa_adapters::project_application;
            let study = Study::new();
            // The environment file in env/ writes its lock and function beside
            // itself; the project writes its codelist beside itself. Each decoy
            // sits beside the other declaring file and fails preparation if read.
            study.write(
                "env/environment.yaml",
                "schema_version: '1.0'\nlock: uv.lock\nfunctions: {id: id.yaml}\n",
            );
            study.write("env/uv.lock", "version = 1\npackage = []\n");
            study.write("uv.lock", "not a packaging lock");
            study.write("id.yaml", "[");
            study.write("env/ct.yaml", "[");
            let entry = study.text("entry/domain.yaml");
            let path = study.text("project.yaml");
            let project = project_application::prepare(&entry, &path, Language::Python).unwrap();
            assert_eq!(project.capture_reads(), 6);
            let captured = project.run().captured_environment();
            assert_eq!(
                captured.root().source().identity,
                study.identity("project.yaml")
            );
            assert_eq!(
                captured.environment_file().unwrap().source().identity,
                study.identity("env/environment.yaml")
            );
            assert_eq!(
                captured.lock().unwrap().source.identity,
                study.identity("env/uv.lock")
            );
            assert_eq!(
                captured
                    .captures()
                    .iter()
                    .map(|capture| (
                        capture.origin.clone(),
                        capture.document.source().identity.clone()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (Origin::Function("id".into()), study.identity("env/id.yaml")),
                    (Origin::Codelist(0), study.identity("ct.yaml")),
                ]
            );
            // No fallback: without env/uv.lock, a valid lock beside the project is
            // still not the lock the environment file declares.
            fs::remove_file(study.0.join("env/uv.lock")).unwrap();
            study.write("uv.lock", "version = 1\npackage = []\n");
            let rows = project_application::check(&entry, &path, Language::Python);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "project_environment_invalid");
            assert_eq!(rows[0].rule(), Some("FUNC-2"));
            assert_eq!(rows[0].spec_paths, ["lock"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({
                    "entry": study.identity("project.yaml"), "path": "uv.lock",
                    "environment_path": "lock", "cause": "missing",
                })
            );
        }

        #[test]
        fn a_section_that_lists_only_specs_admits_runs_without_study_standard_or_define() {
            let study = Study::new();
            study.write(
                "project.yaml",
                &format!("{PROJECT}sdtm: {{specs: [entry/domain.yaml]}}\n"),
            );
            study.write("entry/input.csv", "ID\n1\n");
            let project = study.prepare(&study.text(""), &[]).unwrap();
            let mut run_scope = project.run_scope(None);
            assert!(project.run().check_diagnostics().is_empty());
            // Define alone composes the section; a run reads none of its specs.
            assert_eq!(project.capture_reads(), 6);
            let attempt =
                project.build(&mut run_scope, &mut Activation::default(), Scope::default());
            assert!(attempt.boundary.is_ok());
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
            assert_eq!(run_scope.capture_reads(), 7);
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
            // Once no held name reaches the file, another name of it captures
            // its current bytes as a snapshot of its own.
            fs::remove_file(study.path("project/spec/a")).unwrap();
            let (next, created) = resources.capture("c").unwrap();
            assert!(created);
            assert_eq!(next.as_ref(), b"changed!");
            assert_eq!(resources.capture("a"), Err(Error::Missing));
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
            project_application::{preparation_issues, IssueError},
            project_check, shipped_schema,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::project_function::Language;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
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
                let file_workflow::SelectionError::Preparation(Error::Environment(owner)) =
                    file_workflow::select(&entry, &project, Language::Python)
                        .err()
                        .unwrap()
                else {
                    panic!("original unavailable project owner")
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
                let rows = preparation_issues(&rejection).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].condition(), "project_environment_missing");
                assert_eq!(rows[0].rule(), Some("FUNC-1"));
                assert_eq!(rows[0].spec_paths, ["environment"]);
                assert_eq!(
                    yamaa_core::json::Value::Object(rows[0].context.clone()),
                    yamaa_adapters::json!({"cause":"missing", "path":project})
                );
                let Error::Environment(owner) = &rejection else {
                    unreachable!()
                };
                assert_eq!(owner.capture_reads(), 0);
                assert_eq!(owner.captured_sources().count(), 0);
            }
            fs::remove_file(study.0.join("root.yaml")).unwrap();
            // A valid project whose only environment entry names another host's
            // file: the Python host retains the project without opening that file.
            let project = study.path("project.yaml");
            let raw = "schema_version: '1.0'\nenvironments: {r: r/absent-environment.yaml}\n";
            study.write("project.yaml", raw);
            let file_workflow::SelectionError::Preparation(Error::Entry(owner)) =
                file_workflow::select(&entry, &project, Language::Python)
                    .err()
                    .unwrap()
            else {
                panic!("valid project precedes missing entry")
            };
            assert!(matches!(
                owner.error(),
                yamaa_adapters::file_entry::Error::Document(
                    yamaa_adapters::file_preparation::Error::Resource(
                        yamaa_adapters::file_resources::Error::Missing
                    )
                )
            ));
            assert_eq!(owner.capture_reads(), 1);
            let sources = owner.captured_sources().collect::<Vec<_>>();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].0, project);
            assert_eq!(sources[0].1, raw.as_bytes());
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
                // A project that links no environment file.
                study.write("project.yaml", "schema_version: '1.0'\n");
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
            fn prepare(&self) -> Box<yamaa_adapters::file_project::FileProject> {
                match self.select() {
                    Ok(file_workflow::Selected::Ordinary(project)) => project,
                    Ok(_) => panic!("project capability"),
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
            fn compile_rejection(&self) -> Box<yamaa_adapters::file_project::RejectedOrdinary> {
                match self.select() {
                    Err(file_workflow::SelectionError::Ordinary(error)) => error,
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
            let project = study.prepare();
            assert_eq!(project.capture_reads(), 2);
            assert!(project_check::prepared(project.run()).is_empty());
            let compiled = yamaa_core::specification::PreparedSpecification::prepare(
                project.run().document().model(),
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
            for (index, issue) in raw.iter().enumerate() {
                let name = format!("extra_{index:05}");
                assert_eq!(
                    yamaa_adapters::report_storage::diagnostic_value(issue),
                    yamaa_adapters::json!({"phase": "validation", "condition": "unknown_field", "rule": "SCHEMA-13", "spec_paths": vec![name.clone()], "context": {"class":"root_class", "field":name}}),
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
            study.write("root.yaml", "[");
            let malformed = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            );
            assert_eq!(malformed.len(), 1);
            assert_eq!(
                (
                    malformed[0].phase(),
                    malformed[0].condition(),
                    &malformed[0].spec_paths
                ),
                ("validation", "invalid_yaml", &vec!["$".to_owned()])
            );
            study.write("project.yaml", "[");
            let project = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            );
            assert_eq!(project.len(), 1);
            assert_eq!(project[0].condition(), "invalid_yaml");
            let context = yamaa_core::json::Value::Object(project[0].context.clone());
            assert_eq!(context["path"], study.path("project.yaml"));
            assert_eq!(context["environment_path"], "$");
            assert!(context["line"].as_u64().is_some());
            assert!(context["column"].as_u64().is_some());
            assert!(context["reason"].as_str().is_some());
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                crate::support::rows(&preparation_issues(&owner).unwrap()),
                expected
            );
        }

        #[test]
        fn metadata_failures_name_each_written_dependency_and_cause_before_study_reads() {
            let study = Study::new();
            // The environment file declares the lock and function; the project
            // declares the codelist and links the environment file.
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [absent-codelists.yaml]\n",
            );
            study.write(
                "environment.yaml",
                "schema_version: '1.0'\nlock: missing.lock\nfunctions: {id: absent-function.yaml}\n",
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
            let expected = [
                ("lock", "missing.lock"),
                ("functions.id", "absent-function.yaml"),
                ("codelists[0]", "absent-codelists.yaml"),
            ];
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(rows.len(), expected.len());
            for (row, (declaration, path)) in rows.iter().zip(expected) {
                assert_eq!(row.condition(), "project_environment_invalid");
                assert_eq!(row.spec_paths, [declaration]);
                let context = yamaa_core::json::Value::Object(row.context.clone());
                assert_eq!(context["entry"], project);
                assert_eq!(context["path"], path);
                assert_eq!(context["cause"], "missing");
                assert_eq!(context["environment_path"], declaration);
            }
            assert_eq!(
                rows,
                yamaa_adapters::project_application::check(&entry, &project, Language::Python),
            );
            let Error::Environment(owner) = &error else {
                unreachable!()
            };
            assert_eq!(owner.capture_reads(), 2); // project and environment file
            assert!(!study.0.join("root.yaml").exists());

            study.write("missing.lock", "invalid packaging lock");
            study.write("absent-function.yaml", "[");
            study.write("absent-codelists.yaml", "[");
            let rows =
                yamaa_adapters::project_application::check(&entry, &project, Language::Python);
            assert_eq!(rows.len(), 3);
            let context = yamaa_core::json::Value::Object(rows[0].context.clone());
            assert_eq!(context["cause"], "invalid_packaging_lock");
            for (row, path) in rows[1..]
                .iter()
                .zip(["absent-function.yaml", "absent-codelists.yaml"])
            {
                assert_eq!(row.condition(), "invalid_yaml");
                let context = yamaa_core::json::Value::Object(row.context.clone());
                assert_eq!(context["path"], path);
                assert!(context["reason"].as_str().is_some());
                assert!(context["line"].as_u64().is_some());
            }
        }

        #[test]
        fn missing_or_invalid_linked_environment_files_report_the_project_key_that_names_them() {
            let study = Study::new();
            let entry = study.path("root.yaml");
            let project = study.path("project.yaml");
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\n",
            );
            // Each host opens only the file its own key names and reports it there.
            for (host, key) in [(Language::Python, "python"), (Language::R, "r")] {
                let declaration = format!("environments.{key}");
                let expected = vec![
                    yamaa_adapters::json!({"phase": "validation", "condition": "project_environment_invalid", "rule": "FUNC-2", "spec_paths": vec![declaration.clone()], "context": {
                        "entry": project, "path": format!("{key}/environment.yaml"),
                        "environment_path": declaration, "cause": "missing",
                    }}),
                ];
                let file_workflow::SelectionError::Preparation(error) =
                    file_workflow::select(&entry, &project, host).err().unwrap()
                else {
                    panic!("retained environment rejection")
                };
                assert_eq!(
                    crate::support::rows(&preparation_issues(&error).unwrap()),
                    expected
                );
                assert_eq!(
                    crate::support::rows(&yamaa_adapters::project_application::check(
                        &entry, &project, host
                    )),
                    expected
                );
                let Error::Environment(owner) = &error else {
                    unreachable!()
                };
                assert_eq!(owner.capture_reads(), 1); // the project alone
            }
            // A present environment file decodes against the environment root,
            // where language is no longer a field. Its schema finding names the
            // environment file as source and the project as entry.
            study.write(
                "python/environment.yaml",
                "schema_version: '1.0'\nlanguage: python\n",
            );
            let error = study.rejected();
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(
                crate::support::rows(&rows),
                [
                    yamaa_adapters::json!({"phase": "validation", "condition": "unknown_field", "rule": "SCHEMA-13", "spec_paths": ["language"], "context": {
                        "class": "environment_class", "field": "language",
                        "source": study.path("python/environment.yaml"), "entry": project,
                        "environment_path": "environments.python",
                    }})
                ]
            );
            assert_eq!(
                rows,
                yamaa_adapters::project_application::check(&entry, &project, Language::Python)
            );
            let Error::Environment(owner) = &error else {
                panic!("original environment owner")
            };
            assert_eq!(owner.capture_reads(), 2); // project and Python environment file
            assert!(!study.0.join("root.yaml").exists());
        }

        #[test]
        fn project_function_calls_without_a_host_environment_report_the_undeclared_environment() {
            let study = Study::new();
            // Two live columns call the same project function.
            study.write("root.yaml", "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns:\n  - {name: ID, type: int, derivation: RAW.ID}\n  - {name: FIRST, type: str, derivation: {function: {name: lookup, args: {x: RAW.VALUE}}}}\n  - {name: SECOND, type: str, derivation: {function: {name: lookup, args: {x: RAW.ID}}}}\noutput: {path: never-written.csv, columns: [ID, FIRST, SECOND]}\n");
            let entry = study.path("root.yaml");
            let project = study.path("project.yaml");
            let calls = [
                "columns.FIRST.derivation.function.name",
                "columns.SECOND.derivation.function.name",
            ];
            let rows = |condition: &str, rule: &str, context: yamaa_core::json::Value| {
                calls
                    .map(|path| yamaa_adapters::json!({"phase": "validation", "condition": condition, "rule": rule, "spec_paths": [path], "context": context.clone()}))
                    .to_vec()
            };
            let undeclared = |language: &str| {
                rows(
                    "project_environment_missing",
                    "FUNC-1",
                    yamaa_adapters::json!({"function": "lookup", "language": language, "cause": "undeclared"}),
                )
            };
            let unknown = rows(
                "unknown_project_function",
                "FUNC-4",
                yamaa_adapters::json!({"function": "lookup"}),
            );
            // A project that links no environment file: each call names the
            // missing environment for the host's own key.
            for (host, language) in [(Language::Python, "python"), (Language::R, "r")] {
                assert_eq!(
                    crate::support::rows(&yamaa_adapters::project_application::check(
                        &entry, &project, host
                    )),
                    undeclared(language)
                );
            }
            // Linking only a Python environment that declares no functions: the
            // Python host reports an unknown function, while the R host still has
            // no environment and never opens the Python file.
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
            );
            study.write("environment.yaml", "schema_version: '1.0'\n");
            assert_eq!(
                crate::support::rows(&yamaa_adapters::project_application::check(
                    &entry,
                    &project,
                    Language::Python
                )),
                unknown
            );
            assert_eq!(
                crate::support::rows(&yamaa_adapters::project_application::check(
                    &entry,
                    &project,
                    Language::R
                )),
                undeclared("r")
            );
            let Err(file_workflow::SelectionError::Ordinary(rejected)) =
                file_workflow::select(&entry, &project, Language::R)
            else {
                panic!("ordinary compiler rejection")
            };
            assert_eq!(rejected.capture_reads(), 2); // project and entry only
            assert!(rejected
                .retained_sources()
                .iter()
                .all(|(identity, _)| !identity.ends_with("/environment.yaml")));
            // Without any project there is no environment to name.
            assert_eq!(
                crate::support::rows(&yamaa_adapters::file_application::check(
                    Language::Python,
                    yamaa_core::domain_entry::Request {
                        specification: &entry,
                        environment: None,
                    },
                    yamaa_adapters::specification_report::Identity {
                        runtime: "test",
                        runtime_version: "1",
                        engine_version: "0.1.0",
                        example: "test",
                        specification: &entry,
                        base_directory: ".",
                    },
                )),
                unknown
            );
            assert!(!study.0.join("never-read.csv").exists());
        }

        #[test]
        fn retained_project_and_environment_schema_diagnostics_preserve_wide_integer_tokens() {
            const DIGITS: &str = "92233720368547758081234567890";
            let wide = format!("schema_version: {DIGITS}\n");
            // First the project root itself, then the environment file it links.
            for linked in [false, true] {
                let study = Study::new();
                let (source, declaration) = if linked {
                    study.write(
                        "project.yaml",
                        "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                    );
                    study.write("environment.yaml", &wide);
                    (study.path("environment.yaml"), "environments.python")
                } else {
                    study.write("project.yaml", &wide);
                    (study.path("project.yaml"), "$")
                };
                let error = study.rejected();
                let rows = preparation_issues(&error).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].condition(), "schema_version_mismatch");
                assert_eq!(rows[0].spec_paths, ["schema_version"]);
                let context = yamaa_core::json::Value::Object(rows[0].context.clone());
                assert_eq!(context["actual"].as_number().unwrap().as_str(), DIGITS);
                assert_eq!(context["expected"], "1.0");
                assert_eq!(context["source"], source);
                assert_eq!(context["entry"], study.path("project.yaml"));
                assert_eq!(context["environment_path"], declaration);
                assert!(yamaa_adapters::json_wire::encode_object(&rows[0].context)
                    .contains(&format!("\"actual\":{DIGITS}")));
                assert_eq!(
                    rows,
                    yamaa_adapters::project_application::check(
                        &study.path("root.yaml"),
                        &study.path("project.yaml"),
                        Language::Python,
                    ),
                );
                let Error::Environment(owner) = &error else {
                    panic!("schema finding keeps its retained environment owner")
                };
                let reads = if linked { 2 } else { 1 };
                assert_eq!(owner.capture_reads(), reads);
                assert!(!study.0.join("root.yaml").exists());
                fs::remove_dir_all(&study.0).unwrap();
                assert_eq!(preparation_issues(&error).unwrap(), rows);
                assert_eq!(owner.capture_reads(), reads);
            }
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
                let project = "schema_version: '1.0'\ncodelists: [missing.yaml, ct.yaml]\n";
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
                    yamaa_core::application_issue::Stage::Environment,
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
                            "stage": "environment", "code": "resource_limit",
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
                    "schema_version: '1.0'\ncodelists: [\n".to_owned()
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
                            "stage":"environment", "code":"resource_limit",
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
            assert_eq!(rows[0].condition(), "project_environment_invalid");
            assert_eq!(rows[0].spec_paths, ["environment"]);
            let context = yamaa_core::json::Value::Object(rows[0].context.clone());
            assert_eq!(context["path"], path);
            assert_eq!(context["cause"], "not_regular_file");
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
            study.write("root.yaml", &yaml("{L: {path: left.csv, schema: left.yaml}, R: {path: right.csv, schema: right.yaml}, Q: {path: right.csv, schema: './right.yaml'}}", "L", "root.csv", true).replace(
                "derivation: L.VALUE",
                "derivation: {first_available: {sources: [L.VALUE, R.VALUE, Q.VALUE]}}",
            ));
            let project = study.prepare();
            // One environment, the entry and its two distinct producers. The
            // producers are neither compiled nor followed to leaf.yaml.
            assert_eq!(project.capture_reads(), 4);
            let expected = vec![yamaa_adapters::json!({
                "phase": "validation", "condition": "invalid_regex", "rule": "TEXT-6",
                "spec_paths": ["columns.VALUE.verifications[0].matches.pattern"],
                "context": {"pattern": "é("},
            })];
            assert_eq!(
                crate::support::rows(&project_check::prepared(project.run())),
                expected
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(
                crate::support::rows(&project_check::prepared(project.run())),
                expected
            );
            assert_eq!(
                project.run().captured_environment().root().source().bytes,
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
                &study.path("project.yaml"),
                Language::Python,
            )
            .unwrap();
            let expected = yamaa_adapters::project_check::prepared(ordinary.run());
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
            let error = study.compile_rejection();
            let rows = project_check::rejected(error.run()).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "redundant_field_type");
            assert_eq!(rows[0].rule(), Some("INPUT-6"));
            assert_eq!(rows[0].spec_paths, ["input.P.types.ID"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({"dataset":"P", "field":"ID", "type":"int"})
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(project_check::rejected(error.run()).unwrap(), rows);
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
            assert!(project_check::prepared(project.run()).is_empty());
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
                "schema_version: \"1.0\"\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  DM: {path: input/dm.csv, types: {RANDDT: date}}\noutput:\n  path: adsl.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: DM.RANDDT\n",
            );
            study.write(
                "producer/dm.yaml",
                "schema_version: \"1.0\"\ndomain: DM\nkeys: [USUBJID]\ninput:\n  RAW: {path: ../raw/dm_raw.csv, types: {RANDDT: date}}\noutput:\n  path: ../input/dm.csv\n  columns: [USUBJID, RANDDT]\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: RAW.USUBJID\n  - name: RANDDT\n    type: date\n    label: Date of Randomization\n    derivation: RAW.RANDDT\n",
            );
            study.write(
                "root.yaml",
                "schema_version: \"1.0\"\nparent: parent.yaml\ninput:\n  DM: {schema: producer/dm.yaml}\n",
            );
            let error = study.compile_rejection();
            let rows = project_check::rejected(error.run()).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "redundant_field_type");
            assert_eq!(rows[0].rule(), Some("INPUT-6"));
            assert_eq!(rows[0].spec_paths, ["input.DM.types.RANDDT"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({"dataset":"DM", "field":"RANDDT", "type":"date"})
            );
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
                yamaa_adapters::file_application::domain(
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
                        .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}")
                };
                study.write("leaf.yaml", &leaf);
                // The producer is never compiled and its links are never followed.
                let project = study.prepare();
                assert!(project_check::prepared(project.run()).is_empty());
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
            let error = study.compile_rejection();
            let rows = project_check::rejected(error.run()).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "invalid_producer_contract");
            assert_eq!(rows[0].rule(), Some("INPUT-10"));
            assert_eq!(rows[0].spec_paths, ["input.R.schema"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({"dataset":"R", "reason":"self_reference"})
            );
            assert_eq!(error.capture_reads(), 2);
        }

        #[test]
        fn raw_environment_rejection_precedes_the_first_entry_read() {
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let raw = "schema_version: '1.0'\ncodelists: [\n";
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
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "invalid_yaml");
            let context = yamaa_core::json::Value::Object(rows[0].context.clone());
            assert_eq!(context["path"], "project.yaml");
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
            // python key is the environment finding that language still decides.
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
            );
            study.write(
                "environment.yaml",
                "schema_version: '1.0'\nlock: renv.lock\n",
            );
            study.write("renv.lock", r#"{"R":{"Version":"4.6.1"},"Packages":{}}"#);
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            let error = study.rejected();
            let rows = preparation_issues(&error).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "project_environment_invalid");
            assert_eq!(rows[0].rule(), Some("FUNC-2"));
            assert_eq!(rows[0].spec_paths, ["lock"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({
                    "reason": "lock_kind_mismatch", "language": "python",
                    "actual_lock_kind": "renv", "source": study.path("environment.yaml"),
                    "entry": study.path("project.yaml"), "environment_path": "lock",
                })
            );
            let Error::Environment(environment) = &error else {
                panic!("environment");
            };
            assert_eq!(environment.capture_reads(), 3); // project, environment and lock
            study.write("project.yaml", "schema_version: '1.0'\n");
            study.write(
                "parent/base.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false)
                    .replace("type: str", "type: invalid"),
            );
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparent: parent/base.yaml\ndomain: CHILD\n",
            );
            let error = study.rejected();
            let rows = preparation_issues(&error).unwrap();
            assert!(!rows.is_empty());
            assert!(rows
                .iter()
                .any(|r| r.context.clone()["source"] == study.path("parent/base.yaml")));
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
                "schema_version: '1.0'\nparent: producer/base.yaml\ndomain: LEAF\n",
            );
            let parent = yaml("{P: {path: ../leaf.csv, schema: ../leaf.yaml}, Q: {path: ../leaf.csv, schema: '../leaf.yaml'}}", "P", "../root.csv", false)
        .replace("columns:\n", "intermediates: [{id: Q_REFERENCE, dataset: Q, require_match: true}]\ncolumns:\n")
        .replace("output:", "  - {name: OTHER, type: str, label: Other, derivation: Q_REFERENCE.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, OTHER]");
            study.write("parents/base.yaml", &parent);
            study.write(
                "root.yaml",
                "schema_version: '1.0'\nparent: parents/base.yaml\ndomain: ROOT\n",
            );
            let project = study.prepare();
            // One environment, the entry, its parent and the one linked producer
            // with its own parent. The producer's invalid regex is never compiled.
            assert_eq!(project.capture_reads(), 5);
            assert!(project_check::prepared(project.run()).is_empty());
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
            assert!(
                matches!(preparation_issues(&error), Err(IssueError::Original(original)) if std::ptr::eq(original, &error))
            );
            let rows = yamaa_adapters::project_application::check(
                &study.path("root.yaml"),
                &study.path("project.yaml"),
                Language::Python,
            );
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "resource_path_missing");
            assert_eq!(rows[0].spec_paths, ["input.P.schema"]);
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({"path":"missing.yaml"})
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
                .replace("    label: Reported value\n", "");
            study.write("leaf.yaml", &original);
            let error = study.compile_rejection();
            let rows = project_check::rejected(error.run()).unwrap();
            assert!(rows.iter().any(|row| row.rule() == Some("INPUT-10")
                && row.condition() == "invalid_producer_contract"
                && row.spec_paths == ["input.P.schema.columns.VALUE.label"]));
            // The linked environment file's lock and function resolve beside it.
            study.write(
                "project.yaml",
                "schema_version: '1.0'\nenvironments: {python: python/environment.yaml}\n",
            );
            study.write(
                "python/environment.yaml",
                "schema_version: '1.0'\nlock: uv.lock\nfunctions: {id: broken.yaml}\n",
            );
            study.write("python/uv.lock", "version = 1\npackage = []\n");
            let malformed = "function: [\n";
            study.write("python/broken.yaml", malformed);
            let failure = study.rejected();
            let Error::Environment(environment) = &failure else {
                panic!("original environment");
            };
            // project, environment, lock and the broken function file
            assert_eq!(environment.capture_reads(), 4);
            assert_eq!(
                environment
                    .captured_sources()
                    .find(|(path, _)| path == &study.path("python/broken.yaml"))
                    .unwrap()
                    .1,
                malformed.as_bytes()
            );
            let failures = preparation_issues(&failure).unwrap();
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].condition(), "invalid_yaml");
            let context = yamaa_core::json::Value::Object(failures[0].context.clone());
            assert_eq!(context["entry"], study.path("project.yaml"));
            assert_eq!(context["path"], "broken.yaml");
            assert_eq!(context["environment_path"], "functions.id");
            fs::remove_dir_all(&study.0).unwrap();
            assert!(error
                .retained_sources()
                .iter()
                .any(|(_, bytes)| bytes.as_ref() == original.as_bytes()));
            assert_eq!(project_check::rejected(error.run()).unwrap(), rows);
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
            let study = Study(root);
            study.write("project.yaml", "schema_version: '1.0'\n");
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
            assert!(project_check::prepared(project.run()).is_empty());
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
            assert!(project_check::prepared(project.run()).is_empty());
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
                "schema_version: '1.0'\nparent: base.yaml\ndomain: TEST\n",
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
            assert!(project_check::prepared(project.run()).is_empty());
        }

        #[test]
        fn rejected_mistyped_schema_keeps_the_original_shipped_schema_cause_and_raw_entry() {
            let study = Study::new();
            // A null schema is absent, so the input is an ordinary source.
            study.write(
                "root.yaml",
                &yaml(
                    "{RAW: {path: never-read.csv, schema: null}}",
                    "RAW",
                    "never-written.csv",
                    false,
                ),
            );
            let plain = study.prepare();
            assert!(plain.run().compiled().source_contract(0).is_none());
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
            drop(study);
            let retained = entry.retained_sources();
            assert_eq!(retained.len(), 2);
            assert!(retained
                .iter()
                .any(|(name, bytes)| name.ends_with("/root.yaml")
                    && bytes.as_ref() == original.as_bytes()));
            let findings = preparation_issues(&rejection).unwrap();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].condition(), "invalid_field_type");
            assert_eq!(findings[0].rule(), Some("SCHEMA-12"));
            assert_eq!(findings[0].spec_paths, ["input.RAW.schema"]);
            assert_eq!(entry.capture_reads(), 2);
        }

        impl Study {
            fn select(&self) -> Result<file_workflow::Selected, file_workflow::SelectionError> {
                file_workflow::select_with_resources(
                    Resources::new(&self.path(""), &self.path(""), &[]).unwrap(),
                    "root.yaml",
                    "project.yaml",
                    Language::Python,
                    shipped_schema::capture().unwrap(),
                    shipped_schema::capture_project().unwrap(),
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
            let Selected::Ordinary(selected) = study.select().unwrap() else {
                panic!("ordinary capability");
            };
            assert_eq!(selected.capture_reads(), 2);
            assert!(!study.0.join("never-read.csv").exists());
            assert!(!study.0.join("root.csv").exists());
            // An independent study source is first created and read after preparation.
            study.write("never-read.csv", "ID,VALUE\n1,alpha\n2,beta\n");
            let ordinary = yamaa_adapters::file_project::FileProject::prepare(
                Resources::new(&study.path(""), &study.path(""), &[]).unwrap(),
                "root.yaml",
                "project.yaml",
                Language::Python,
                shipped_schema::capture().unwrap(),
                shipped_schema::capture_project().unwrap(),
            )
            .unwrap();
            let mut selected_scope = selected.run_scope(None);
            let selected_attempt =
                selected.build(&mut selected_scope, &mut NoActivation, Scope::default());
            let ordinary_attempt = ordinary.build(
                &mut ordinary.run_scope(None),
                &mut NoActivation,
                Scope::default(),
            );
            assert_eq!(selected_scope.capture_reads(), 3);
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
        fn selected_inherited_links_read_each_producer_once_with_shared_aliases() {
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
        .replace("columns:\n", "intermediates: [{id: RIGHT, dataset: R, require_match: true}, {id: ALIAS, dataset: Q, require_match: true}]\ncolumns:\n")
        .replace("output:", "  - {name: RIGHT_VALUE, type: str, label: Right value, derivation: RIGHT.VALUE}\n  - {name: ALIAS_VALUE, type: str, label: Alias value, derivation: ALIAS.VALUE}\noutput:")
        .replace("columns: [ID, VALUE]", "columns: [ID, VALUE, RIGHT_VALUE, ALIAS_VALUE]");
            study.write("parents/base.yaml", &parent);
            let entry = "schema_version: '1.0'\nparent: parents/base.yaml\ndomain: ROOT\n";
            study.write("root.yaml", entry);
            let Selected::Ordinary(project) = study.select().unwrap() else {
                panic!("project capability");
            };
            // Environment, entry, parent and the two distinct linked producers;
            // leaf.yaml is linked only by producers and is never read.
            assert_eq!(project.capture_reads(), 5);
            let document = project.run().document();
            assert_eq!(document.source().bytes, entry.as_bytes());
            assert!(document
                .raw()
                .document
                .field(document.raw().document.root(), "input")
                .is_none());
            let rows = project_check::prepared(project.run());
            assert_eq!(rows.len(), 1);
            assert_eq!(
                document.declaring_source(&rows[0].spec_paths[0]),
                Some(study.path("parents/base.yaml").as_str())
            );
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(project_check::prepared(project.run()), rows);
            assert_eq!(project.capture_reads(), 5);
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
            // The default project links no Python environment, so the call
            // reports the undeclared environment rather than an unknown function.
            let rows = yamaa_adapters::project_check::rejected(rejected.run()).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].condition(), "project_environment_missing");
            assert_eq!(rows[0].rule(), Some("FUNC-1"));
            assert_eq!(
                rows[0].spec_paths,
                ["columns.VALUE.derivation.function.name"]
            );
            assert_eq!(
                yamaa_core::json::Value::Object(rows[0].context.clone()),
                yamaa_adapters::json!({"function": "missing", "language": "python", "cause": "undeclared"})
            );
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
            let SelectionError::Preparation(error @ Error::Entry(_)) = study.selection_rejection()
            else {
                panic!("owned original metadata rejection");
            };
            let rows = preparation_issues(&error).unwrap();
            assert!(rows.iter().any(|r| r.rule() == Some("SCHEMA-12")));
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
            let SelectionError::Ordinary(rejected) = study.selection_rejection() else {
                panic!("owned compilation rejection");
            };
            let rows = project_check::rejected(rejected.run()).unwrap();
            assert!(rows.iter().any(|r| r.rule() == Some("INPUT-6")));
            assert_eq!(rejected.capture_reads(), 3);
            fs::remove_dir_all(&study.0).unwrap();
            assert_eq!(project_check::rejected(rejected.run()).unwrap(), rows);
        }

        #[test]
        fn selected_raw_environment_and_root_failures_keep_exact_capture_prefixes() {
            use file_workflow::SelectionError;
            let study = Study::new();
            study.write(
                "root.yaml",
                &yaml("{RAW: never-read.csv}", "RAW", "root.csv", false),
            );
            study.write("project.yaml", "schema_version: '1.0'\ncodelists: [\n");
            let SelectionError::Preparation(Error::Environment(environment)) =
                study.selection_rejection()
            else {
                panic!("raw project rejection");
            };
            assert_eq!(environment.capture_reads(), 1);
            study.write("project.yaml", "schema_version: '1.0'\n");
            study.write("root.yaml", "[");
            let SelectionError::Preparation(root @ Error::Entry(_)) = study.selection_rejection()
            else {
                panic!("raw root rejection");
            };
            let rows = preparation_issues(&root).unwrap();
            assert_eq!(rows[0].condition(), "invalid_yaml");
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
                        .replace("label: Identifier, ", "")
                        .replace("    label: Reported value\n", ""),
                    _ => yaml("{RAW: never-read.csv}", "RAW", "leaf.csv", false)
                        .replace("RAW.VALUE", "{function: {name: missing, args: {x: 1}}}"),
                };
                study.write("leaf.yaml", &leaf);
                if mode != 1 {
                    // A link back to the consumer and a producer compiler failure
                    // are producer facts the consumer never compiles or follows.
                    let project = study.prepare();
                    assert!(project_check::prepared(project.run()).is_empty());
                    assert_eq!(project.capture_reads(), 3);
                    continue;
                }
                let SelectionError::Ordinary(rejected) = study.selection_rejection() else {
                    panic!("whole contract rejection");
                };
                let rows = project_check::rejected(rejected.run()).unwrap();
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
                assert_eq!(project_check::rejected(rejected.run()).unwrap(), rows);
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
                &study.path("project.yaml"),
                Language::Python,
            )
            .unwrap() else {
                panic!("configured ordinary capability");
            };
            assert_eq!(project.capture_reads(), 2); // project + root; no configuration
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
                Language::Python,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "original link admission: {:?}",
                    yamaa_adapters::project_application::selection_failure(&error)
                )
            });
            let file_workflow::Selected::Plain(prepared) = selected else {
                panic!("a schema link keeps the plain route");
            };
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
            study.write("parent/base.yaml", "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input/unknown}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n");
            study.write(
                "entry/root.yaml",
                "schema_version: '1.0'\nparent: ../parent/base.yaml\n",
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
