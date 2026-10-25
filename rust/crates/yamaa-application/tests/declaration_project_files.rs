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
        use crate::support::Edit as _;
        use std::fs;
        use std::path::{Path, PathBuf};

        use yamaa_adapters::document::{self, AdmittedDocument, Closures};
        use yamaa_adapters::project_source::{self, Failure, Kind};
        use yamaa_adapters::specification_source::{Error, Source};
        use yamaa_core::ast::{FileId, FileRef, Span};
        use yamaa_core::front_end::{lowering, v1};
        use yamaa_core::json::Value;
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

        #[test]
        fn a_codelist_integer_outside_the_int_range_fails_as_any_literal_does() {
            let study = converted_study();
            let wide = CODELISTS.edit("rank: 2", "rank: 99999999999999999999");
            // Written inline, in the project file itself.
            let project = fs::read_to_string(study.path("project.yaml")).unwrap();
            let inline = wide.replace('\n', "\n    ");
            study.write(
                "project.yaml",
                format!("{project}codelists:\n  - {}\n", inline.trim_end()).as_bytes(),
            );
            let Err(Failure::RootScalar { root, findings }) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("an inline rank outside the int range")
            };
            let issues =
                yamaa_application::project_environment_diagnostics::scalar_root(&root, &findings)
                    .unwrap();
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            let project = study.path("project.yaml").to_str().unwrap().to_owned();
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "integer_overflow", Some("VALUE-5"))
            );
            assert_eq!(
                issue.spec_paths,
                ["codelists[0].codelists[1].items[1].rank"]
            );
            let number = |text: &str| Value::Number(text.parse().unwrap());
            let text = |text: &str| Value::String(text.into());
            for (key, value) in [
                ("actual", number("99999999999999999999")),
                ("value", text("99999999999999999999")),
                ("minimum", number("-9223372036854775808")),
                ("maximum", number("9223372036854775807")),
                ("source", text(&project)),
                ("entry", text(&project)),
                ("environment_path", text("$")),
            ] {
                assert_eq!(issue.context[key], value, "{key}");
            }
            assert_eq!(issue.context.len(), 7);
            // Written in a codelist file the project lists.
            study.write(
                "project.yaml",
                format!(
                    "{}codelists: [ct/sdtm.yaml]\n",
                    fs::read_to_string(repository(
                        "docs/design/interface-1.0/examples/adam-adsl-bmi/project.yaml"
                    ))
                    .unwrap()
                )
                .as_bytes(),
            );
            study.write(
                "ct/sdtm.yaml",
                format!("yamaa: \"1.0\"\ntype: codelists\n{wide}").as_bytes(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("a listed codelist file's rank outside the int range")
            };
            let issues =
                yamaa_application::project_environment_diagnostics::scalar_sources(&rejected)
                    .unwrap();
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(issue.condition(), "integer_overflow");
            assert_eq!(issue.spec_paths, ["codelists[1].items[1].rank"]);
            assert_eq!(
                issue.context["source"],
                Value::String(study.path("ct/sdtm.yaml").to_str().unwrap().into())
            );
            assert_eq!(
                issue.context["environment_path"],
                Value::String("codelists[0]".into())
            );
        }

        #[test]
        fn a_listed_function_file_without_the_1_0_header_fails_in_that_file() {
            let study = converted_study();
            // A definition without a header where a function file belongs:
            // its header is malformed, so the finding is in that file, at its
            // missing `yamaa` (SCHEMA-2).
            let bmi = fs::read_to_string(study.path("functions/bmi.yaml")).unwrap();
            let body = bmi
                .lines()
                .filter(|line| !line.starts_with("yamaa:") && !line.starts_with("type:"))
                .collect::<Vec<_>>()
                .join("\n");
            study.write("functions/bmi.yaml", body.as_bytes());
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("a header-less function file")
            };
            let [failure] = rejected.sources.as_slice() else {
                panic!("one failed listed file")
            };
            assert_eq!(failure.origin, project_source::Origin::FunctionFile(0));
            assert_eq!(failure.written, "functions/bmi.yaml");
            let project_source::CaptureFailure::Structural(Error::Findings(captured)) =
                &failure.error
            else {
                panic!("a finding in the listed file")
            };
            assert_eq!(
                captured.source().identity,
                study.path("functions/bmi.yaml").to_str().unwrap()
            );
            let [finding] = captured.findings() else {
                panic!("one finding")
            };
            assert_eq!(finding.path, "yamaa");
            assert_eq!(
                finding.code.definition().condition,
                "missing_required_field"
            );
        }

        #[test]
        fn a_lock_of_another_kind_and_a_repeated_name_fail_where_they_are_declared() {
            let study = converted_study();
            let project = fs::read_to_string(study.path("project.yaml")).unwrap();
            // The R lock under Python's key, and a second file declaring the
            // function's name.
            study.write(
                "project.yaml",
                project
                    .edit(
                        "environments:\n  r:",
                        "environments:\n  python:\n    lock: r/renv.lock\n  r:",
                    )
                    .edit(
                        "- functions/bmi.yaml",
                        "- functions/bmi.yaml\n- functions/copy.yaml",
                    )
                    .as_bytes(),
            );
            study.write(
                "functions/copy.yaml",
                &fs::read(study.path("functions/bmi.yaml")).unwrap(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("a lock of another kind and a repeated name")
            };
            let issues =
                yamaa_application::project_environment_diagnostics::admission(&rejected).unwrap();
            let text = |text: &str| Value::String(text.into());
            let project = study.path("project.yaml").to_str().unwrap().to_owned();
            let copy = study
                .path("functions/copy.yaml")
                .to_str()
                .unwrap()
                .to_owned();
            let expected = [
                (
                    "environments.python.lock",
                    vec![
                        ("reason", text("lock_kind_mismatch")),
                        ("language", text("python")),
                        ("actual_lock_kind", text("renv")),
                        ("source", text(&project)),
                        ("entry", text(&project)),
                        ("environment_path", text("environments.python.lock")),
                    ],
                ),
                (
                    "name",
                    vec![
                        ("function", text("bmi")),
                        ("reason", text("duplicate_function")),
                        ("source", text(&copy)),
                        ("entry", text(&project)),
                        ("environment_path", text("functions[1]")),
                    ],
                ),
            ];
            assert_eq!(issues.len(), expected.len(), "{issues:?}");
            for (issue, (path, context)) in issues.iter().zip(expected) {
                assert_eq!(
                    (issue.condition(), issue.rule()),
                    ("function_file_invalid", Some("FUNC-2"))
                );
                assert_eq!(issue.spec_paths, [path]);
                assert_eq!(issue.context.len(), context.len(), "{path}");
                for (key, value) in context {
                    assert_eq!(issue.context[key], value, "{path}: {key}");
                }
            }
        }

        #[test]
        fn function_file_findings_name_the_function_file_and_its_listing() {
            let study = converted_study();
            let bmi = fs::read_to_string(study.path("functions/bmi.yaml")).unwrap();
            // A Python spelling of the R callable.
            study.write(
                "functions/bmi.yaml",
                bmi.edit("yamaabenchmarks::bmi", "yamaabenchmarks.bmi")
                    .as_bytes(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("an invalid R callable fails in either host")
            };
            let issues =
                yamaa_application::project_environment_diagnostics::admission(&rejected).unwrap();
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "function_file_invalid", Some("FUNC-2"))
            );
            assert_eq!(issue.spec_paths, ["implementations.r"]);
            let text = |text: &str| Value::String(text.into());
            for (key, value) in [
                ("function", text("bmi")),
                ("language", text("r")),
                ("reason", text("invalid_callable")),
                (
                    "source",
                    text(study.path("functions/bmi.yaml").to_str().unwrap()),
                ),
                ("entry", text(study.path("project.yaml").to_str().unwrap())),
                ("environment_path", text("functions[0]")),
            ] {
                assert_eq!(issue.context[key], value, "{key}");
            }
            // A date its declared type cannot read fails in its file too,
            // keeping the text it is written as (P-FN-1).
            study.write(
                "functions/bmi.yaml",
                bmi.edit("weight_kg\n  type: float", "weight_kg\n  type: date")
                    .replacen("weight_kg: 70.0", "weight_kg: '2024-02-30'", 1)
                    .as_bytes(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("an unreadable date")
            };
            let issues =
                yamaa_application::project_environment_diagnostics::scalar_sources(&rejected)
                    .unwrap();
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(issue.condition(), "function_file_invalid");
            assert_eq!(issue.spec_paths, ["tests[0].args.weight_kg"]);
            assert_eq!(issue.context["reason"], text("invalid_date"));
            assert_eq!(issue.context["value"], text("2024-02-30"));
            assert_eq!(
                issue.context["source"],
                text(study.path("functions/bmi.yaml").to_str().unwrap())
            );
            assert_eq!(issue.context["environment_path"], text("functions[0]"));
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
        use crate::support::Edit as _;
        use std::fs;

        use std::sync::Arc;
        use yamaa_adapters::{
            document::Closures,
            file_resources::Resources,
            json,
            project_lock::{Finding, Reason},
            project_source::Origin,
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
        use yamaa_core::project_environment::LockKind;
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

        #[test]
        fn a_listed_file_or_lock_that_cannot_be_read_fails_as_any_file_a_run_reads() {
            let missing = |path: &str, written: &str| {
                row(
                    "resource_path_missing",
                    "FILE-7",
                    path,
                    json!({"path": written}),
                )
            };
            for (name, field, written) in [
                ("functions/id.yaml", "functions[0]", "functions/id.yaml"),
                ("ct.yaml", "codelists[0]", "ct.yaml"),
                ("uv.lock", "environments.python.lock", "uv.lock"),
            ] {
                let study = Study::new();
                fs::remove_file(study.0.join(name)).unwrap();
                assert_eq!(
                    rows(&study.issues(Language::Python)),
                    [missing(field, written)],
                    "{name}"
                );
            }
            // Every failure is reported, in the order the files are read: the
            // lock, the function files, then the codelist files.
            let study = Study::new();
            for name in ["functions/id.yaml", "ct.yaml", "renv.lock"] {
                fs::remove_file(study.0.join(name)).unwrap();
            }
            assert_eq!(
                rows(&study.issues(Language::R)),
                [
                    missing("environments.r.lock", "renv.lock"),
                    missing("functions[0]", "functions/id.yaml"),
                    missing("codelists[0]", "ct.yaml"),
                ]
            );
            // A directory, with the path as written.
            let study = Study::new();
            fs::remove_file(study.0.join("functions/id.yaml")).unwrap();
            fs::create_dir(study.0.join("functions/id.yaml")).unwrap();
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [row(
                    "resource_path_not_regular_file",
                    "FILE-7",
                    "functions[0]",
                    json!({"path": "functions/id.yaml"}),
                )]
            );
            // A file that is read but is no YAML fails in that file, with the
            // project file and the listing.
            let study = Study::new();
            study.write("functions/id.yaml", "[unclosed\n");
            let issues = study.issues(Language::Python);
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(
                (issue.condition(), issue.rule(), issue.spec_paths.as_slice()),
                (
                    "invalid_yaml",
                    Some("SCHEMA-3"),
                    ["$".to_string()].as_slice()
                )
            );
            for (key, value) in [
                ("entry", study.identity("project.yaml")),
                ("path", "functions/id.yaml".into()),
                ("environment_path", "functions[0]".into()),
            ] {
                assert_eq!(issue.context[key], Value::String(value), "{key}");
            }
            // The written form is decided first (FILE-11): the project file's
            // own admission refuses it, before any listed file is read.
            for (written, condition) in [
                ("functions\\id.yaml", "resource_path_not_normalized"),
                ("https://example.org/id.yaml", "resource_path_uri_scheme"),
            ] {
                let study = Study::new();
                study.write(
                    "project.yaml",
                    &PROJECT.edit("[functions/id.yaml]", &format!("['{written}']")),
                );
                let project = study.identity("project.yaml");
                assert_eq!(
                    rows(&study.issues(Language::Python)),
                    [row(
                        condition,
                        "FILE-5",
                        "functions[0]",
                        json!({
                            "path": written,
                            "source": project,
                            "entry": project,
                            "environment_path": "$",
                        }),
                    )]
                );
            }
        }

        #[cfg(unix)]
        #[test]
        fn a_symbolic_link_in_a_listed_path_fails_as_any_file_a_run_reads() {
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

        #[test]
        fn a_lock_that_is_no_packaging_lock_fails_as_the_function_files_do() {
            let study = Study::new();
            study.write("renv.lock", "not a packaging lock\n");
            let project = study.identity("project.yaml");
            assert_eq!(
                rows(&study.issues(Language::R)),
                [row(
                    "function_file_invalid",
                    "FUNC-2",
                    "environments.r.lock",
                    json!({
                        "source": project,
                        "entry": project,
                        "path": "renv.lock",
                        "environment_path": "environments.r.lock",
                        "cause": "invalid_packaging_lock",
                        "reason": "invalid_packaging_lock",
                    }),
                )]
            );
        }

        #[test]
        fn a_listed_file_whose_header_the_engine_does_not_read_fails_at_the_field_that_lists_it() {
            let study = Study::new();
            let function = study.identity("functions/id.yaml");
            let codelists = study.identity("ct.yaml");
            // A version the engine does not read, beside a codelist file of
            // another type, listed after an inline source.
            study.write(
                "functions/id.yaml",
                &FUNCTION.edit("yamaa: \"1.0\"", "yamaa: \"1.1\""),
            );
            study.write(
                "project.yaml",
                &PROJECT.edit(
                    "codelists: [ct.yaml]",
                    "codelists:\n  - codelists: [{id: AGE, name: Age, data_type: integer, items: [{value: 1}]}]\n  - ct.yaml",
                ),
            );
            study.write(
                "ct.yaml",
                &CODELISTS.edit("type: codelists", "type: function"),
            );
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [
                    row(
                        "unsupported_language_version",
                        "SCHEMA-2",
                        "functions[0]",
                        json!({"source": function, "version": "1.1", "language_versions": ["1.0"]}),
                    ),
                    row(
                        "unexpected_file_type",
                        "SCHEMA-2",
                        "codelists[1]",
                        json!({"source": codelists, "type": "function", "expected": "codelists"}),
                    ),
                ]
            );
            // A codelist file where a function file belongs.
            study.write("functions/id.yaml", CODELISTS);
            study.write("ct.yaml", CODELISTS);
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [row(
                    "unexpected_file_type",
                    "SCHEMA-2",
                    "functions[0]",
                    json!({"source": function, "type": "codelists", "expected": "function"}),
                )]
            );
            // A file written in language 0.5, such as an environment file,
            // declares `schema_version`, which the engine does not read.
            study.write(
                "functions/id.yaml",
                "schema_version: '1.0'\nlock: uv.lock\n",
            );
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [row(
                    "unsupported_language_version",
                    "SCHEMA-2",
                    "functions[0]",
                    json!({"source": function, "version": "0.5", "language_versions": ["1.0"]}),
                )]
            );
            // A file without the header is a finding in that file.
            study.write(
                "functions/id.yaml",
                &FUNCTION.edit("yamaa: \"1.0\"\ntype: function\n", ""),
            );
            let issues = study.issues(Language::Python);
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(
                (issue.condition(), issue.spec_paths.as_slice()),
                ("missing_required_field", ["yamaa".to_string()].as_slice())
            );
            for (key, value) in [
                ("source", function.as_str()),
                ("entry", study.identity("project.yaml").as_str()),
                ("environment_path", "functions[0]"),
            ] {
                assert_eq!(issue.context[key], Value::String(value.into()), "{key}");
            }
        }

        /// CT-12: a codelist's `format_name` begins with `$` exactly when the
        /// codelist is `text`, whether the project file declares the source
        /// inline or lists a codelist file, and the failure is reported where
        /// it is written.
        #[test]
        fn a_format_name_whose_dollar_sign_disagrees_with_its_data_type_fails_where_written() {
            let study = Study::new();
            let project = study.identity("project.yaml");
            let codelists = study.identity("ct.yaml");
            study.write(
                "project.yaml",
                &PROJECT.edit(
                    "codelists: [ct.yaml]",
                    "codelists:\n  - codelists: [{id: SEX, name: Sex, format_name: SEXF, items: [{value: F}, {value: M}]}]\n  - ct.yaml",
                ),
            );
            study.write(
                "ct.yaml",
                "yamaa: \"1.0\"\ntype: codelists\ncodelists:\n  - {id: AGEGR1N, name: Age group, data_type: integer, format_name: $AGEGRN, items: [{value: 1}, {value: 2}]}\n  - {id: DOSE, name: Dose, data_type: float, format_name: DOSEF, items: [{value: 2.5}]}\n",
            );
            assert_eq!(
                rows(&study.issues(Language::Python)),
                [
                    row(
                        "value_not_permitted",
                        "CT-12",
                        "codelists[0].codelists[0].format_name",
                        json!({
                            "codelist": "SEX",
                            "value": "SEXF",
                            "data_type": "text",
                            "source": project,
                            "entry": project,
                            "environment_path": "codelists[0]",
                        }),
                    ),
                    row(
                        "value_not_permitted",
                        "CT-12",
                        "codelists[0].format_name",
                        json!({
                            "codelist": "AGEGR1N",
                            "value": "$AGEGRN",
                            "data_type": "integer",
                            "source": codelists,
                            "entry": project,
                            "environment_path": "codelists[1]",
                        }),
                    ),
                ]
            );
        }

        /// Activation of the project's one function, refusing as told.
        #[derive(Default)]
        struct Activation {
            reject_lock: bool,
            reject_binding: bool,
            mismatch: bool,
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = &'static str;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Self::Error> {
                let (written, kind, call) = match language {
                    Language::Python => ("uv.lock", LockKind::Uv, "program.id"),
                    Language::R => ("renv.lock", LockKind::Renv, "program::id"),
                };
                assert_eq!(
                    (lock.reference.written.as_str(), lock.reference.kind),
                    (written, kind)
                );
                assert_eq!(functions[0].call, call);
                if self.reject_lock {
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
                    }] => Scalar::Int(if self.mismatch { n + 1 } else { *n }),
                    _ => Scalar::Missing,
                })
            }
        }

        #[test]
        fn activation_reports_a_function_file_s_findings_in_that_file_and_its_lock_in_the_project()
        {
            let study = Study::new();
            let project_file = study.identity("project.yaml");
            let function = study.identity("functions/id.yaml");
            let identity = || Identity {
                runtime: "test",
                runtime_version: "1",
                engine_version: "0.1.0",
                example: "versioned-project",
                specification: "domain.yaml",
                base_directory: "entry",
            };
            for (host, lock, key, call) in [
                (Language::Python, "uv.lock", "python", "program.id"),
                (Language::R, "renv.lock", "r", "program::id"),
            ] {
                let project = study.prepare(reading(), host).unwrap();
                let mut run_scope = project.run_scope(None);
                // A lock verification failure, at the project's lock field.
                let mut activation = Activation {
                    reject_lock: true,
                    ..Default::default()
                };
                let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
                let Err(BoundaryFailure::Activation(Failure::Lock(error))) = &attempt.boundary
                else {
                    panic!("a lock failure")
                };
                let findings = [Finding {
                    package: "program".into(),
                    reason: Reason::VersionMismatch,
                    expected: vec!["1.0".into()],
                    actual: Some("2.0".into()),
                }];
                let facts = [Classified::Lock {
                    failure: error,
                    findings: &findings,
                }];
                let result =
                    specification_report::build_result(project.run(), &attempt, identity(), &facts)
                        .unwrap();
                let field = format!("environments.{key}.lock");
                assert_eq!(
                    rows(result.issues()),
                    [row(
                        "runtime_artifact_mismatch",
                        "FUNC-3",
                        &field,
                        json!({
                            "source": project_file,
                            "entry": project_file,
                            "lock_source": study.identity(lock),
                            "lock": lock,
                            "package": "program",
                            "reason": "version_mismatch",
                            "expected": ["1.0"],
                            "actual": "2.0",
                        }),
                    )],
                    "{host:?}"
                );
                // A binding failure, at the function file's callable for the
                // host.
                let mut activation = Activation {
                    reject_binding: true,
                    ..Default::default()
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
                assert_eq!(
                    rows(result.issues()),
                    [row(
                        "function_file_invalid",
                        "FUNC-2",
                        &format!("implementations.{key}"),
                        json!({
                            "function": "id",
                            "call": call,
                            "reason": "the callable is not installed",
                            "source": function,
                            "entry": project_file,
                            "environment_path": "functions[0]",
                        }),
                    )],
                    "{host:?}"
                );
                let activation = &result.observations()["activation"];
                assert_eq!(activation["bindings"][0]["source"], function.as_str());
                assert_eq!(activation["bindings"][0]["outcome"], "rejected");
                // A failed case, at its place in the function file.
                let mut activation = Activation {
                    mismatch: true,
                    ..Default::default()
                };
                let attempt = project.build(&mut run_scope, &mut activation, Scope::default());
                let result =
                    specification_report::build_result(project.run(), &attempt, identity(), &[])
                        .unwrap();
                let issues = result.issues();
                let [issue] = issues else {
                    panic!("one failed case: {issues:?}")
                };
                assert_eq!(
                    (issue.condition(), issue.rule(), issue.spec_paths.as_slice()),
                    (
                        "function_conformance_failed",
                        Some("FUNC-12"),
                        ["tests[0]".to_string()].as_slice()
                    )
                );
                for (key, value) in [
                    ("source", json!(function)),
                    ("entry", json!(project_file)),
                    ("environment_path", json!("functions[0]")),
                    ("case", json!("normal")),
                    ("expected", json!(7)),
                    ("actual", json!(8)),
                ] {
                    assert_eq!(issue.context[key], value, "{host:?}: {key}");
                }
                let tests = &result.observations()["activation"]["tests"];
                assert_eq!(tests[0]["source"], function.as_str());
                assert_eq!(tests[0]["outcome"], "result_mismatch");
            }
        }
    }
}
