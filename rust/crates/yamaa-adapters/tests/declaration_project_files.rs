#![cfg(feature = "yaml")]
//! The 1.0 front end for project, function and codelist files (P1-8; D8,
//! D16, P-TYPE-1).
//!
//! A 1.0 project lists its function files and holds each host language's
//! lock; a function file names its function and the callable of each host it
//! implements; a codelist file holds one codelist source. Each lowers into the
//! roots a 0.5 project, environment and codelist file lower into, and a
//! project prepares through them. Until the engine reads 1.0 files (#3030
//! D76) only tests reach this path.
mod declaration {
    mod project_files {
        use std::fmt::Debug;
        use std::fs;
        use std::path::{Component, Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use yamaa_adapters::document::{self, AdmittedDocument, Closures, Decoded};
        use yamaa_adapters::project_source::{
            self, Failure, Kind, ProjectCapturePort, Reply, Request,
        };
        use yamaa_adapters::shipped_schema;
        use yamaa_adapters::specification_source::{CapturedDocument, Error, Source};
        use yamaa_core::ast::{
            FileId, FileRef, Function, FunctionSource, Notation, Project, Span, Surface,
        };
        use yamaa_core::front_end::{v0_5, v1};
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::project_function::Language;
        use yamaa_core::schema::SchemaSource;

        static NEXT: AtomicUsize = AtomicUsize::new(0);

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
            let Decoded::Versioned(document) =
                document::decode(source(path), expected, Some(LANGUAGE_VERSION), &closures)
                    .unwrap()
            else {
                panic!("{} declares the 1.x header", path.display())
            };
            document.admit().unwrap()
        }

        /// The written position of each node of an admitted 1.0 file.
        fn positions(document: &AdmittedDocument) -> v0_5::Positions {
            let locations = &document.raw().locations;
            v0_5::Positions {
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

        /// A 0.5 project file, structurally prepared as a public call does.
        fn legacy_project(path: &Path) -> CapturedDocument {
            shipped_schema::capture_project()
                .unwrap()
                .project
                .prepare_structural(source(path))
                .unwrap()
        }

        fn legacy_environment(path: &Path) -> CapturedDocument {
            shipped_schema::capture_environment()
                .unwrap()
                .prepare_structural(source(path))
                .unwrap()
        }

        /// A value's declarations as text: its pretty debug form without the
        /// metas and path arenas, which differ only by position and spelling.
        fn declarations(value: &impl Debug) -> String {
            let text = format!("{value:#?}");
            let mut kept = Vec::new();
            let mut skipping = None;
            for line in text.lines() {
                let field = line.trim_start();
                let indent = line.len() - field.len();
                if let Some(depth) = skipping {
                    if indent == depth && field.starts_with('}') {
                        skipping = None;
                    }
                    continue;
                }
                if field.starts_with("meta: Meta {") || field.starts_with("paths: Paths {") {
                    skipping = Some(indent);
                    continue;
                }
                kept.push(line);
            }
            kept.join("\n")
        }

        /// `path` with its `.` and `..` segments resolved lexically.
        fn normal(path: &Path) -> PathBuf {
            let mut normal = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::ParentDir => {
                        normal.pop();
                    }
                    Component::CurDir => {}
                    other => normal.push(other),
                }
            }
            normal
        }

        /// `written`, resolved from the directory of `declaring`, as a path
        /// relative to the directory `project` is in.
        fn resolved(project: &Path, declaring: &Path, written: &str) -> PathBuf {
            normal(&declaring.parent().unwrap().join(written))
                .strip_prefix(normal(project.parent().unwrap()))
                .unwrap()
                .to_path_buf()
        }

        /// The 0.5 project at `legacy`, lowered with the environment file it
        /// links for `host`, when it links one.
        fn legacy_root(legacy: &Path, host: Language) -> (Project, Option<PathBuf>) {
            let captured = legacy_project(legacy);
            let mut project =
                v0_5::project::project(&captured.normalized().document, None).unwrap();
            let linked = project
                .environment(host)
                .and_then(|environment| environment.file.as_ref())
                .map(|file| legacy.parent().unwrap().join(&file.text));
            if let Some(path) = &linked {
                let environment = legacy_environment(path);
                v0_5::project::environment(
                    &environment.normalized().document,
                    None,
                    host,
                    &mut project,
                )
                .unwrap();
            }
            (project, linked)
        }

        /// The converted project at `converted` lowers to the roots its 0.5
        /// source at `legacy` lowers to, for either host, apart from spans and
        /// renamed spellings: the header, each lock's spelling relative to its
        /// new declaring file, the environment files D8 removes, and the
        /// functions they declared, which are now function files naming each
        /// host's callable. Returns the function files the project lists.
        fn compare(legacy: &Path, converted: &Path) -> Vec<PathBuf> {
            let admitted = admit(converted, FileType::Project);
            let project =
                v1::project::project(&admitted.normalized().document, Some(&positions(&admitted)))
                    .unwrap();
            assert_eq!(project.header.surface, Surface::V1 { minor: 0 });
            assert!(project.header.meta.span.is_known());
            let files = project
                .functions
                .iter()
                .flatten()
                .map(|declaration| {
                    assert_eq!(declaration.key, None);
                    let FunctionSource::File(written) = &declaration.source else {
                        panic!("a 1.0 project lists function files")
                    };
                    converted.parent().unwrap().join(&written.text)
                })
                .collect::<Vec<_>>();
            let functions = files
                .iter()
                .map(|path| {
                    let admitted = admit(path, FileType::Function);
                    v1::project::function(&admitted.normalized().document, None).unwrap()
                })
                .collect::<Vec<_>>();
            for host in [Language::Python, Language::R] {
                let (old, linked) = legacy_root(legacy, host);
                assert_eq!(project.header.version.text, old.header.version.text);
                assert_eq!(
                    declarations(&project.codelists),
                    declarations(&old.codelists)
                );
                assert_eq!(
                    declarations(&project.submission),
                    declarations(&old.submission)
                );
                // A lock moves from the environment file to the project
                // file, respelled from there; a host without one has no entry.
                let lock = |project: &Project, declaring: &Path, root: &Path| {
                    project
                        .environment(host)
                        .and_then(|environment| environment.lock.as_ref())
                        .map(|lock| resolved(root, declaring, &lock.text))
                };
                assert_eq!(
                    lock(&project, converted, converted),
                    linked
                        .as_deref()
                        .and_then(|linked| lock(&old, linked, legacy)),
                    "{host:?}"
                );
                assert!(project
                    .environments
                    .iter()
                    .all(|environment| environment.file.is_none() && environment.lock.is_some()));
                // Each definition the host's environment file declares is
                // the function file of its name, holding the host's callable.
                let definitions = old
                    .functions
                    .iter()
                    .flatten()
                    .map(|declaration| match &declaration.source {
                        FunctionSource::Inline(definition) => definition.as_ref().clone(),
                        FunctionSource::File(_) => panic!("benchmarks define functions inline"),
                    })
                    .collect::<Vec<Function>>();
                assert_eq!(definitions.len(), functions.len());
                for (definition, file) in definitions.iter().zip(&functions) {
                    // The comparison keeps every declaration.
                    assert!(declarations(definition).contains(&definition.description.text));
                    let mut file = file.clone();
                    assert!(file.header.is_some());
                    file.header = None;
                    file.implementations
                        .retain(|implementation| implementation.language == host);
                    assert_eq!(declarations(&file), declarations(definition), "{host:?}");
                }
            }
            files
        }

        #[test]
        fn the_converted_project_files_lower_to_the_roots_of_their_sources() {
            // The converter's output for two benchmarks, which
            // `tools/tests/test_migrate.py` checks it reproduces: one with a
            // function both hosts implement and an R lock only, and one with
            // inline codelists, a study and a section.
            let files = compare(
                &repository("benchmarks/adam-adsl-bmi/project.yaml"),
                &repository("docs/design/interface-1.0/examples/adam-adsl-bmi/project.yaml"),
            );
            assert_eq!(files.len(), 1);
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
                    function
                        .paths
                        .render(implementation.callable.meta.path, Notation::V0_5),
                    format!("implementations.{}", implementation.language.key())
                );
            }
            assert!(compare(
                &repository("benchmarks/sdtm-dm-metadata/project.yaml"),
                &repository("docs/design/interface-1.0/examples/sdtm-dm-metadata/project.yaml"),
            )
            .is_empty());
        }

        /// A temporary study directory.
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-project-files-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&path).unwrap();
                Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
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
        impl Drop for Study {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        /// Reads each requested file beside the file that declares it, and
        /// records what was read.
        #[derive(Default)]
        struct Files {
            read: Vec<(Kind, String)>,
        }
        impl ProjectCapturePort for Files {
            type Error = std::io::Error;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                self.read.push((request.kind, request.written.into()));
                let path = Path::new(request.declaring_source)
                    .parent()
                    .unwrap()
                    .join(request.written);
                let source = Source {
                    identity: path.to_str().unwrap().into(),
                    bytes: fs::read(&path)?,
                };
                Ok(match request.kind {
                    Kind::Lock => Reply::Lock {
                        kind: yamaa_adapters::project_lock::kind(&source.bytes).unwrap(),
                        source,
                    },
                    _ => Reply::Document(source),
                })
            }
        }

        fn prepare(
            study: &Study,
            host: Language,
            files: &mut Files,
        ) -> Result<project_source::PreparedEnvironment, Failure<std::io::Error>> {
            project_source::prepare_versioned(
                shipped_schema::capture_project().unwrap(),
                (Closures::shipped().unwrap(), LANGUAGE_VERSION),
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
                assert!(prepared.environment_file.is_none());
                assert_eq!(prepared.lock.is_some(), host == Language::R);
                assert_eq!(prepared.origins.functions, ["bmi"]);
                assert_eq!(prepared.origins.function_files, [0]);
                let execution = prepared.environment.into_execution();
                let [bmi] = execution.functions() else {
                    panic!("one project function")
                };
                assert_eq!(bmi.language(), host);
                assert!(bmi.implemented());
                assert_eq!(
                    bmi.call(),
                    match host {
                        Language::Python => "yamaa_benchmarks.bmi",
                        Language::R => "yamaabenchmarks::bmi",
                    }
                );
            }
        }

        /// A codelist source written as a file: header-less in 0.5, and
        /// under its own header in 1.0 (P-TYPE-1). No benchmark writes one.
        const CODELISTS: &str = "standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2024-03-29'}\ncodelists:\n  - id: SEX\n    name: Sex\n    alias: C66731\n    format_name: $SEX\n    items:\n      - {value: F, decode: Female, alias: C16576}\n      - {value: M, decode: Male, alias: C20197}\n  - id: AGEGR\n    name: Age Group\n    data_type: integer\n    items: [{value: 1, decode: Young, rank: 1}, {value: 2, decode: Old, rank: 2}]\n  - id: MEDDRA\n    name: Adverse Event Dictionary\n    external: {dictionary: MedDRA, version: '26.0'}\n";

        #[test]
        fn a_codelist_file_lowers_to_the_source_its_0_5_spelling_does() {
            let study = converted_study();
            study.write("ct/legacy.yaml", CODELISTS.as_bytes());
            study.write(
                "ct/sdtm.yaml",
                format!("yamaa: \"1.0\"\ntype: codelists\n{CODELISTS}").as_bytes(),
            );
            let legacy = shipped_schema::capture_project()
                .unwrap()
                .project
                .prepare_named_class(study.source("ct/legacy.yaml"), "codelist_source_class")
                .unwrap();
            let legacy = v0_5::project::codelists(&legacy.normalized().document, None).unwrap();
            let admitted = admit(&study.path("ct/sdtm.yaml"), FileType::Codelists);
            let mut file = v1::project::codelists(
                &admitted.normalized().document,
                Some(&positions(&admitted)),
            )
            .unwrap();
            assert!(file.header.take().is_some());
            assert_eq!(declarations(&file), declarations(&legacy));
            assert!(declarations(&legacy).contains("MedDRA"));
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
            let wide = CODELISTS.replace("rank: 2", "rank: 99999999999999999999");
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
                yamaa_adapters::project_environment_diagnostics::scalar_root(&root, &findings)
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
                ("minimum", number("-9223372036854775808")),
                ("maximum", number("9223372036854775807")),
                ("source", text(&project)),
                ("entry", text(&project)),
                ("environment_path", text("$")),
            ] {
                assert_eq!(issue.context[key], value, "{key}");
            }
            assert_eq!(issue.context.len(), 6);
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
                yamaa_adapters::project_environment_diagnostics::scalar_sources(&rejected).unwrap();
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
            // A 0.5 definition, without a header, where a function file
            // belongs: its header is malformed, so the finding is in that
            // file, at its missing `yamaa` (SCHEMA-2).
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
                    .replace(
                        "environments:\n  r:",
                        "environments:\n  python:\n    lock: r/renv.lock\n  r:",
                    )
                    .replace(
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
                yamaa_adapters::project_environment_diagnostics::admission(&rejected).unwrap();
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
                bmi.replace("yamaabenchmarks::bmi", "yamaabenchmarks.bmi")
                    .as_bytes(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("an invalid R callable fails in either host")
            };
            let issues =
                yamaa_adapters::project_environment_diagnostics::admission(&rejected).unwrap();
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
                bmi.replace("weight_kg\n  type: float", "weight_kg\n  type: date")
                    .replacen("weight_kg: 70.0", "weight_kg: '2024-02-30'", 1)
                    .as_bytes(),
            );
            let Err(Failure::Rejected(rejected)) =
                prepare(&study, Language::Python, &mut Files::default())
            else {
                panic!("an unreadable date")
            };
            let issues =
                yamaa_adapters::project_environment_diagnostics::scalar_sources(&rejected).unwrap();
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
}
