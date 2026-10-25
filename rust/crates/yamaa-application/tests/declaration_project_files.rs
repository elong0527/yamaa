#![cfg(feature = "yaml")]
//! The 1.0 front end for project, function and codelist files (P1-8; D8,
//! D16, P-TYPE-1).
//!
//! A 1.0 project lists its function files and holds each host language's
//! lock; a function file names its function and the callable of each host it
//! implements; a codelist file holds one codelist source. Each lowers into its
//! root of `yamaa_core::ast::project`, and a project prepares through them, on
//! every public route (#3030 D76).
mod declaration {
    mod project_files {
        use std::fs;
        use std::path::{Path, PathBuf};

        use yamaa_adapters::document::{self, AdmittedDocument, Closures};
        use yamaa_adapters::project_source::{self, Failure, Kind};
        use yamaa_adapters::specification_source::Source;
        use yamaa_core::ast::{FileId, FileRef, Span};
        use yamaa_core::front_end::{lowering, v1};
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{self, Captured, File, Located, Name, Resolver};
        use yamaa_core::schema::SchemaSource;

        fn repository(path: &str) -> PathBuf {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(path)
        }

        fn source(path: &Path) -> Source {
            Source {
                identity: path.to_str().unwrap().into(),
                bytes: fs::read(path).unwrap(),
            }
        }

        /// A 1.0 file admitted at the version it declares, as an engine that
        /// reads 1.0 files admits it.
        fn admit(path: &Path, expected: FileType) -> AdmittedDocument {
            let closures = Closures::shipped().unwrap();
            document::decode(source(path), expected, Some(LANGUAGE_VERSION), &closures)
                .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()))
                .admit()
                .unwrap()
        }

        /// The written position of each node of an admitted 1.0 file.
        fn positions(document: &AdmittedDocument) -> lowering::Positions {
            let locations = &document.raw().locations;
            lowering::Positions {
                files: vec![FileRef {
                    identity: document.source().identity.clone(),
                }],
                nodes: document
                    .normalized()
                    .origins
                    .iter()
                    .map(|origin| match origin.source {
                        SchemaSource::Input => locations.get(origin.node).map(|at| Span {
                            file: FileId(0),
                            line: at.line as u32,
                            column: at.column as u32,
                        }),
                        SchemaSource::Default { .. } => None,
                    })
                    .collect(),
            }
        }

        /// The converter's output for two benchmarks, which its tests checked
        /// it reproduced: one with a function both hosts implement and an R
        /// lock only, and one with inline codelists, a study and a section.
        /// Each lowers with the written position of every node.
        #[test]
        fn the_converted_project_files_lower_with_their_written_positions() {
            // Seam: the AST each converted file lowers into and the written position of its nodes,
            // which no issue row shows.
            let lower = |path: &Path| {
                let admitted = admit(path, FileType::Project);
                let project = v1::project::project(
                    &admitted.normalized().document,
                    Some(&positions(&admitted)),
                )
                .unwrap();
                assert_eq!(project.header.version.text, "1.0");
                assert!(project.header.meta.span.is_known());
                project
            };
            let converted =
                repository("docs/design/interface-1.0/examples/adam-adsl-bmi/project.yaml");
            let project = lower(&converted);
            // The project lists its function file and holds the R lock alone.
            let files = project
                .functions
                .iter()
                .flatten()
                .map(|declaration| converted.parent().unwrap().join(&declaration.file.text))
                .collect::<Vec<_>>();
            assert_eq!(files.len(), 1);
            assert!(project.environment(Language::Python).is_none());
            assert!(project
                .environment(Language::R)
                .and_then(|environment| environment.lock.as_ref())
                .is_some_and(|lock| lock.meta.span.is_known()));
            let bmi = admit(&files[0], FileType::Function);
            let function =
                v1::project::function(&bmi.normalized().document, Some(&positions(&bmi))).unwrap();
            assert_eq!(function.name.name, "bmi");
            assert_eq!(
                function
                    .implementations
                    .iter()
                    .map(|implementation| implementation.language)
                    .collect::<Vec<_>>(),
                [Language::Python, Language::R]
            );
            // Every node of a function file has a written position.
            for implementation in &function.implementations {
                assert!(implementation.callable.meta.span.is_known());
                assert_eq!(
                    function.paths.render(implementation.callable.meta.path),
                    format!("implementations.{}", implementation.language.key())
                );
            }
            let project = lower(&repository(
                "docs/design/interface-1.0/examples/sdtm-dm-metadata/project.yaml",
            ));
            assert!(project.functions.is_none());
            assert!(!project.codelists.is_empty());
            let declarations = project.submission.as_ref().unwrap();
            assert!(declarations.study.is_some());
            assert!(!declarations.sections.is_empty());
        }

        /// A temporary study directory.
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("project-files");
                Self(path)
            }
            fn write(&self, name: &str, bytes: &[u8]) {
                let path = self.0.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            fn path(&self, name: &str) -> PathBuf {
                self.0.join(name)
            }
            fn source(&self, name: &str) -> Source {
                source(&self.path(name))
            }
        }

        /// Reads each requested file beside the file that declares it, and
        /// records what was read.
        #[derive(Default)]
        struct Files {
            read: Vec<(Kind, String)>,
            /// The bytes of the file last located.
            held: Vec<u8>,
        }
        impl Files {
            /// The held source the double serves for the file a project names.
            fn answer(
                &mut self,
                kind: Kind,
                declaring: &str,
                written: &str,
            ) -> Result<Source, std::io::Error> {
                self.read.push((kind, written.into()));
                let path = Path::new(declaring).parent().unwrap().join(written);
                let source = Source {
                    identity: path.to_str().unwrap().into(),
                    bytes: fs::read(&path)?,
                };
                Ok(source)
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for Files {
            type Error = std::io::Error;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Self::Error> {
                let Name::Written {
                    file: File::Project(kind),
                    declaring,
                    written,
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let source = self.answer(kind, declaring, written)?;
                self.held = source.bytes;
                Ok(Located {
                    display_path: source.identity.clone(),
                    identity: source.identity,
                })
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Self::Error> {
                Ok(Captured::resource(
                    std::mem::take(&mut self.held).into(),
                    true,
                ))
            }
            fn failure(&self, _: Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Self::Error> {
                panic!("a project file is never rebased")
            }
        }

        fn prepare(
            study: &Study,
            host: Language,
            files: &mut Files,
        ) -> Result<project_source::PreparedEnvironment, Failure<std::io::Error>> {
            project_source::prepare(
                Closures::shipped().unwrap(),
                study.source("project.yaml"),
                host,
                files,
            )
        }

        /// The converted `adam-adsl-bmi` project, its function file and the
        /// R lock it keeps.
        fn converted_study() -> Study {
            let study = Study::new();
            let example = |name: &str| {
                fs::read(repository(&format!(
                    "docs/design/interface-1.0/examples/adam-adsl-bmi/{name}"
                )))
                .unwrap()
            };
            study.write("project.yaml", &example("project.yaml"));
            study.write("functions/bmi.yaml", &example("functions/bmi.yaml"));
            study.write(
                "r/renv.lock",
                &fs::read(repository("benchmarks/adam-adsl-bmi/r/renv.lock")).unwrap(),
            );
            study
        }

        #[test]
        fn each_host_prepares_a_converted_project_through_its_function_files() {
            // Seam: the files each host's preparation reads, in order, and the execution
            // environment it prepares, which a check does not return.
            for (host, read) in [
                (
                    Language::Python,
                    vec![(Kind::Function, "functions/bmi.yaml")],
                ),
                (
                    Language::R,
                    vec![
                        (Kind::Lock, "r/renv.lock"),
                        (Kind::Function, "functions/bmi.yaml"),
                    ],
                ),
            ] {
                let study = converted_study();
                let mut files = Files::default();
                let prepared = prepare(&study, host, &mut files).unwrap();
                // Only the host's own lock is read; Python declares none.
                assert_eq!(
                    files
                        .read
                        .iter()
                        .map(|(kind, written)| (*kind, written.as_str()))
                        .collect::<Vec<_>>(),
                    read
                );
                assert_eq!(prepared.lock.is_some(), host == Language::R);
                assert_eq!(prepared.origins.functions, ["bmi"]);
                assert_eq!(prepared.origins.function_files, [0]);
                let execution = prepared.environment.into_execution();
                let [bmi] = execution.functions() else {
                    panic!("one project function")
                };
                assert_eq!(bmi.language(), Some(host));
                assert_eq!(bmi.missing_implementation(), None);
                assert_eq!(
                    bmi.call(),
                    match host {
                        Language::Python => "yamaa_benchmarks.bmi",
                        Language::R => "yamaabenchmarks::bmi",
                    }
                );
            }
        }

        /// A codelist source as a codelist file writes it, without its header
        /// (P-TYPE-1). No benchmark writes one.
        const CODELISTS: &str = "standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2024-03-29'}\ncodelists:\n  - id: SEX\n    name: Sex\n    alias: C66731\n    format_name: $SEX\n    items:\n      - {value: F, decode: Female, alias: C16576}\n      - {value: M, decode: Male, alias: C20197}\n  - id: AGEGR\n    name: Age Group\n    data_type: integer\n    items: [{value: 1, decode: Young, rank: 1}, {value: 2, decode: Old, rank: 2}]\n  - id: MEDDRA\n    name: Adverse Event Dictionary\n    external: {dictionary: MedDRA, version: '26.0'}\n";

        #[test]
        fn a_project_reads_a_codelist_file_it_lists_relative_to_itself() {
            // Seam: the file the preparation reads last and the catalogue it prepares, which a
            // check does not return.
            let study = converted_study();
            study.write(
                "ct/sdtm.yaml",
                format!("yamaa: \"1.0\"\ntype: codelists\n{CODELISTS}").as_bytes(),
            );
            // A 1.0 project lists the file, relative to the project file.
            let project = fs::read_to_string(study.path("project.yaml")).unwrap();
            study.write(
                "project.yaml",
                format!("{project}codelists: [ct/sdtm.yaml]\n").as_bytes(),
            );
            let mut files = Files::default();
            let prepared = prepare(&study, Language::Python, &mut files).unwrap();
            assert_eq!(
                files
                    .read
                    .last()
                    .map(|(kind, written)| (*kind, written.as_str())),
                Some((Kind::Codelist, "ct/sdtm.yaml"))
            );
            assert_eq!(prepared.origins.codelists, [0]);
            let execution = prepared.environment.into_execution();
            for id in ["SEX", "AGEGR", "MEDDRA"] {
                assert!(execution.catalogue().get(id).is_some(), "{id}");
            }
        }
    }

    /// Public reads of a 1.0 project's dependencies: the route every public
    /// call prepares a project through, whose shipped schemas read 1.x files
    /// (#3030 D76). A listed file or
    /// lock that cannot be read fails as any file a run reads, at the field
    /// that names it (FUNC-2, FILE-11); a listed file whose header the engine
    /// does not read fails at that field (SCHEMA-2); and a function file's
    /// activation findings are reported in that file, its lock's in the
    /// project file (FUNC-2, FUNC-3, FUNC-12).
    mod project_reads {
        use std::fs;

        use std::sync::Arc;
        use yamaa_adapters::{
            document::Closures, file_resources::Resources, json, project_source::Origin,
        };
        use yamaa_application::{
            file_preparation::FileSpecification,
            file_project, file_workflow, project_application,
            project_function_diagnostics::HostDetails,
            project_report::Classified,
            specification_report::{self, Identity},
            specification_run::BoundaryFailure,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::function_invocation::{Argument, HostError};
        use yamaa_core::function_signature::{LogicalSignature, ProjectFunctionIdentity};
        use yamaa_core::host_functions::{HostFunctions, Lock};
        use yamaa_core::json::Value;
        use yamaa_core::language::FileType;
        use yamaa_core::project_activation::Failure;
        use yamaa_core::project_function::Language;
        use yamaa_core::table::ValueRef;

        const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/id.yaml]\nenvironments:\n  python: {lock: uv.lock}\n  r: {lock: renv.lock}\ncodelists: [ct.yaml]\n";
        const FUNCTION: &str = "yamaa: \"1.0\"\ntype: function\nname: id\ndescription: Integer identity.\nparams: [{name: x, type: int, accepts_missing: true}]\nreturns: int\nmay_return_missing: true\nimplementations: {python: program.id, r: program::id}\ntests:\n  - {id: normal, covers: [normal, boundary], args: {x: 7}, result: 7}\n  - {id: missing, covers: ['accepted-missing:x', nullable-output], args: {x: null}, result: null}\n";
        const CODELISTS: &str = "yamaa: \"1.0\"\ntype: codelists\ncodelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n";
        /// A 1.0 specification calling the project's function, which the 1.0
        /// project supplies.
        const DOMAIN: &str = "yamaa: '1.0'\ntype: spec\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: int, derivation: {function: {name: id, args: {x: SRC.ID}}}}\noutput: {path: output.csv, columns: [ID, VALUE]}\n";

        /// The shipped 1.0 closures, which public calls read files with.
        fn reading() -> Closures {
            Closures::shipped().unwrap()
        }

        /// A 1.0 project with one function file, a lock for each host and a
        /// codelist file, and a 1.0 specification in `entry/` that calls the
        /// function.
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::canonical("project-reads");
                fs::create_dir_all(path.join("entry")).unwrap();
                let study = Self(path);
                study.write("project.yaml", PROJECT);
                study.write("functions/id.yaml", FUNCTION);
                study.write("uv.lock", "version = 1\npackage = []\n");
                study.write(
                    "renv.lock",
                    "{\"R\": {\"Version\": \"4.6.1\"}, \"Packages\": {}}\n",
                );
                study.write("ct.yaml", CODELISTS);
                study.write("entry/domain.yaml", DOMAIN);
                study
            }
            fn write(&self, name: &str, text: &str) {
                let path = self.0.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            /// The resource identity of `name`, written with portable
            /// separators.
            fn identity(&self, name: &str) -> String {
                let path = self.0.join(name).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn resources(&self) -> Resources {
                Resources::new(
                    self.0.to_str().unwrap(),
                    self.0.join("entry").to_str().unwrap(),
                    &[],
                )
                .unwrap()
            }
            fn prepare(
                &self,
                schemas: Closures,
                host: Language,
            ) -> Result<FileSpecification, file_project::Error> {
                file_project::prepare(
                    self.resources(),
                    "domain.yaml",
                    "../project.yaml",
                    host,
                    schemas,
                )
            }
            /// The issues of a refused preparation, which a build and a check
            /// of the specification report alike.
            fn issues(&self, host: Language) -> Vec<Diagnostic> {
                let Err(error) = self.prepare(reading(), host) else {
                    panic!("a refused preparation")
                };
                let built =
                    project_application::failure(&project_application::Error::Project(error));
                let Err(selected) = file_workflow::select_with_resources(
                    self.resources(),
                    "domain.yaml",
                    "../project.yaml",
                    host,
                    reading(),
                ) else {
                    panic!("a refused selection")
                };
                assert_eq!(project_application::selection_failure(&selected), built);
                built
            }
        }

        /// Condition, rule, spec paths and context of each issue.
        fn rows(
            issues: &[Diagnostic],
        ) -> Vec<(&'static str, Option<&'static str>, Vec<String>, Value)> {
            issues
                .iter()
                .map(|issue| {
                    (
                        issue.condition(),
                        issue.rule(),
                        issue.spec_paths.clone(),
                        Value::Object(issue.context.clone()),
                    )
                })
                .collect()
        }

        fn row(
            condition: &'static str,
            rule: &'static str,
            path: &str,
            context: Value,
        ) -> (&'static str, Option<&'static str>, Vec<String>, Value) {
            (condition, Some(rule), vec![path.into()], context)
        }

        #[test]
        fn public_calls_read_a_1_0_project_and_only_the_files_it_names() {
            // Seam: the shared closures' identity and the capture reads and captures a prepared
            // project holds, which a check does not return.
            let study = Study::new();
            // The shipped closures are admitted once and shared (D76).
            assert!(Arc::ptr_eq(
                reading().get(FileType::Project),
                Closures::shipped().unwrap().get(FileType::Project)
            ));
            // The public route reads the project, the host's lock, its
            // function file and its codelist file, and only those (PROJECT-2).
            for (host, lock) in [(Language::Python, "uv.lock"), (Language::R, "renv.lock")] {
                let project = study.prepare(reading(), host).unwrap();
                assert!(project.run().check_diagnostics().is_empty());
                // The entry, the project file, the lock, the function file
                // and the codelist file.
                assert_eq!(project.capture_reads(), 5);
                let captured = project.run().captured_environment();
                assert_eq!(
                    captured.lock().unwrap().source.identity,
                    study.identity(lock)
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
                        (Origin::FunctionFile(0), study.identity("functions/id.yaml")),
                        (Origin::Codelist(0), study.identity("ct.yaml")),
                    ]
                );
                let origin = captured.function_origin(0).unwrap();
                assert_eq!(origin.source, study.identity("functions/id.yaml"));
                assert_eq!(origin.declaration(), "functions[0]");
            }
            // The other host's lock is never read.
            fs::remove_file(study.0.join("renv.lock")).unwrap();
            assert!(study.prepare(reading(), Language::Python).is_ok());
        }

        #[cfg(unix)]
        #[test]
        fn a_symbolic_link_in_a_listed_path_fails_as_any_file_a_run_reads() {
            // Seam: a symbolic link, which a case's staged files cannot hold.
            let study = Study::new();
            fs::rename(
                study.0.join("functions/id.yaml"),
                study.0.join("functions/real.yaml"),
            )
            .unwrap();
            std::os::unix::fs::symlink("real.yaml", study.0.join("functions/id.yaml")).unwrap();
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [row(
                    "resource_path_symlink",
                    "FILE-8",
                    "functions[0]",
                    json!({"path": "functions/id.yaml"}),
                )]
            );
        }

        #[cfg(unix)]
        #[test]
        fn an_unclassified_read_failure_refuses_the_preparation_opaquely() {
            // Seam: a file its permissions make unreadable, which a case's staged files cannot
            // hold.
            use std::os::unix::fs::PermissionsExt;
            let study = Study::new();
            let function = study.0.join("functions/id.yaml");
            fs::set_permissions(&function, fs::Permissions::from_mode(0o0)).unwrap();
            if fs::File::open(&function).is_ok() {
                eprintln!("permission checks bypassed by this process; DAC case not exercised");
                return;
            }
            // Beside a lock that cannot be read either, the whole
            // preparation is refused with the one opaque resource refusal.
            fs::remove_file(study.0.join("uv.lock")).unwrap();
            let issues = study.issues(Language::Python);
            fs::set_permissions(&function, fs::Permissions::from_mode(0o644)).unwrap();
            let [issue] = issues.as_slice() else {
                panic!("one refusal: {issues:?}")
            };
            assert_eq!((issue.condition(), issue.rule()), ("engine_rejected", None));
            assert!(issue.spec_paths.is_empty());
            assert_eq!(
                Value::Object(issue.context.clone()),
                json!({"step": "environment", "code": "resource_unreadable"})
            );
        }

        /// Activation of the project's one function: a binding it refuses or
        /// a case whose result differs.
        struct Activation {
            reject_binding: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = &'static str;
            fn verify_lock(
                &mut self,
                _: Language,
                _: Lock<'_>,
                _: &[ProjectFunctionIdentity],
            ) -> Result<(), Self::Error> {
                Ok(())
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Self::Error> {
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
            ) -> Result<yamaa_core::value::Value, HostError<Self::Error>> {
                use yamaa_core::value::Value as Scalar;
                Ok(match args {
                    [Argument {
                        value: ValueRef::Int(n),
                        ..
                    }] => Scalar::Int(n + 1),
                    _ => Scalar::Missing,
                })
            }
        }

        #[test]
        fn activation_records_name_the_function_file_of_each_binding_and_case() {
            // Seam: the activation observations of a refused build, which the
            // replay does not return. The issues each refusal reports are
            // project_activation.json's binding and case cases.
            let study = Study::new();
            let function = study.identity("functions/id.yaml");
            let identity = || Identity {
                runtime: "test",
                runtime_version: "1",
                engine_version: "0.1.0",
                example: "versioned-project",
                specification: "domain.yaml",
                base_directory: "entry",
            };
            for host in [Language::Python, Language::R] {
                let project = study.prepare(reading(), host).unwrap();
                let mut run_scope = project.run_scope(None);
                let mut activation = Activation {
                    reject_binding: true,
                };
                let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
                let Err(BoundaryFailure::Activation(Failure::Bindings(failures))) =
                    &attempt.boundary
                else {
                    panic!("a binding failure")
                };
                let facts = [Classified::Host {
                    failure: &failures[0].error,
                    details: HostDetails::Rejected {
                        reason: "the callable is not installed",
                        returned: None,
                    },
                }];
                let result =
                    specification_report::build_result(project.run(), &attempt, identity(), &facts)
                        .unwrap();
                let activation = &result.observations()["activation"];
                assert_eq!(activation["bindings"][0]["source"], function.as_str());
                assert_eq!(activation["bindings"][0]["outcome"], "rejected");
                let mut activation = Activation {
                    reject_binding: false,
                };
                let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
                let result =
                    specification_report::build_result(project.run(), &attempt, identity(), &[])
                        .unwrap();
                let tests = &result.observations()["activation"]["tests"];
                assert_eq!(tests[0]["source"], function.as_str(), "{host:?}");
                assert_eq!(tests[0]["outcome"], "result_mismatch", "{host:?}");
            }
        }
    }
}
