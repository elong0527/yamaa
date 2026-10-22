//! Preparation contracts: project preparation.
mod preparation {
    mod project_preparation {
        //! Independent application truth over native captures and normalized document fixtures.
        use crate::support::ports::*;
        use yamaa_core::project_preparation::*;
        use yamaa_core::{
            limits::Limits,
            project_environment::{Finding, LockKind, Undeclared},
            project_function::Language,
            project_function_document::Kind as ScalarKind,
            schema::{Document, DocumentNode as N},
            value::Value,
        };

        use crate::support::document::Tree;

        fn function(result: &str) -> Tree {
            Tree::map(vec![
                ("function", Tree::text("project.constant")),
                ("description", Tree::text("A constant")),
                ("params", Tree::List(vec![])),
                ("returns", Tree::text("int")),
                ("may_return_missing", Tree::boolean(false)),
                ("comparison_decimals", Tree::integer("4")),
                (
                    "tests",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("ordinary")),
                        (
                            "covers",
                            Tree::List(vec![Tree::text("normal"), Tree::text("boundary")]),
                        ),
                        ("args", Tree::map(vec![])),
                        ("result", Tree::integer(result)),
                    ])]),
                ),
            ])
        }
        fn codelist(id: &str, value: Tree) -> Tree {
            Tree::map(vec![(
                "codelists",
                Tree::List(vec![Tree::map(vec![
                    ("id", Tree::text(id)),
                    ("name", Tree::text(id)),
                    ("data_type", Tree::text("text")),
                    ("extensible", Tree::boolean(false)),
                    (
                        "items",
                        Tree::List(vec![Tree::map(vec![
                            ("value", value),
                            ("extended", Tree::boolean(false)),
                        ])]),
                    ),
                ])]),
            )])
        }
        /// The caller-supplied 1.0 project file lists two function files,
        /// holds each host language's lock and declares an inline codelist
        /// source before a codelist file, each path relative to the project
        /// file (D8).
        fn project(inline_value: Tree) -> Tree {
            Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("project")),
                (
                    "functions",
                    Tree::List(vec![Tree::text("first.yaml"), Tree::text("last.yaml")]),
                ),
                (
                    "environments",
                    Tree::map(vec![
                        (
                            "python",
                            Tree::map(vec![("lock", Tree::text("python/uv.lock"))]),
                        ),
                        ("r", Tree::map(vec![("lock", Tree::text("r/renv.lock"))])),
                    ]),
                ),
                (
                    "codelists",
                    Tree::List(vec![
                        codelist("INLINE", inline_value),
                        Tree::text("ct.yaml"),
                    ]),
                ),
            ])
        }
        /// A 0.5 project file instead links one environment file per host
        /// language and declares its own codelists, study and sections.
        fn legacy_project(inline_value: Tree) -> Tree {
            Tree::map(vec![
                (
                    "environments",
                    Tree::map(vec![
                        ("python", Tree::text("python/environment.yaml")),
                        ("r", Tree::text("r/environment.yaml")),
                    ]),
                ),
                (
                    "codelists",
                    Tree::List(vec![
                        codelist("INLINE", inline_value),
                        Tree::text("ct.yaml"),
                    ]),
                ),
                ("study", Tree::Scalar(N::Null)),
                ("adam", Tree::Scalar(N::Null)),
            ])
        }
        /// A linked environment file holds only installed code: a lock and functions.
        fn legacy_environment(inline_result: &str) -> Tree {
            Tree::map(vec![
                ("lock", Tree::text("lockfile")),
                (
                    "functions",
                    Tree::map(vec![
                        ("first", Tree::text("first.yaml")),
                        ("inline", function(inline_result)),
                        ("last", Tree::text("last.yaml")),
                    ]),
                ),
            ])
        }
        fn source(identity: &str, bytes: &[u8]) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.into(),
            }
        }
        #[derive(Debug)]
        struct Opaque(Box<str>);
        type Outcome = Result<
            PreparedEnvironment<MetadataDocument>,
            Failure<Opaque, MetadataDocument, Opaque>,
        >;
        /// A 1.0 project's preparation, scripted capture by capture.
        struct Fixture {
            host: Language,
            source: Source,
            root: Document,
            log: EventLog,
            port: ScriptedMetadata<Opaque>,
            decoder: ScriptedMetadataDecoder<Opaque>,
        }
        impl Fixture {
            fn new() -> Self {
                let log = EventLog::default();
                Self {
                    host: Language::Python,
                    source: source("/author/project.yaml", b"held root bytes"),
                    root: project(Tree::text("F")).document(),
                    port: ScriptedMetadata::new(log.clone()),
                    decoder: ScriptedMetadataDecoder::new(log.clone()),
                    log,
                }
            }
            /// The authored path and held identity of the host's own lock.
            fn lock(&self) -> (&'static str, &'static str) {
                match self.host {
                    Language::Python => ("python/uv.lock", "/held/python/uv.lock"),
                    Language::R => ("r/renv.lock", "/held/r/renv.lock"),
                }
            }
            /// Script the root decode and every capture in request order: the
            /// host's lock (0), the function files (1, 2) and the codelist
            /// file (3), each requested from the project file (D8). The lock
            /// is a uv lock, and each function file implements only Python.
            fn script(
                &mut self,
                failing: &[usize],
                interrupt: Option<usize>,
                decoding: Option<usize>,
                malformed: Option<usize>,
            ) -> Vec<usize> {
                self.decoder.steps.push_back(MetadataDecodeStep {
                    source: self.source.clone(),
                    target: DecodeTarget::Project,
                    outcome: Ok(self.root.clone()),
                });
                self.port.interrupt_if = |error| error.0.as_ref() == "interrupt";
                let project = self.source.identity.clone();
                let (lock, held) = self.lock();
                let mut payloads = vec![];
                for (index, (kind, written, identity, raw)) in [
                    (Kind::Lock, lock, held, b"uv".as_slice()),
                    (
                        Kind::Function,
                        "first.yaml",
                        "/held/first.yaml",
                        b"first".as_slice(),
                    ),
                    (
                        Kind::Function,
                        "last.yaml",
                        "/held/last.yaml",
                        b"last".as_slice(),
                    ),
                    (Kind::Codelist, "ct.yaml", "/held/ct.yaml", b"ct".as_slice()),
                ]
                .into_iter()
                .enumerate()
                {
                    let failed = failing.contains(&index) || interrupt == Some(index);
                    let outcome = if failed {
                        let error = Opaque(
                            if interrupt == Some(index) {
                                "interrupt"
                            } else {
                                written
                            }
                            .into(),
                        );
                        payloads.push(error.0.as_ptr() as usize);
                        Err(error)
                    } else {
                        let captured = source(identity, raw);
                        Ok(if (kind == Kind::Lock) != (malformed == Some(index)) {
                            Reply::Lock {
                                source: captured,
                                kind: LockKind::Uv,
                            }
                        } else {
                            Reply::Document(captured)
                        })
                    };
                    self.port.steps.push_back(MetadataCaptureStep {
                        kind,
                        declaring_source: project.clone(),
                        written: written.into(),
                        outcome,
                    });
                    if kind != Kind::Lock && !failed && malformed != Some(index) {
                        let (target, document) = match kind {
                            Kind::Function => (
                                DecodeTarget::FunctionFile,
                                function_file(
                                    written.trim_end_matches(".yaml"),
                                    &[("python", "project.constant")],
                                    "7",
                                )
                                .document(),
                            ),
                            _ => (DecodeTarget::CodelistFile, codelist_file().document()),
                        };
                        let outcome = if decoding == Some(index) {
                            let error = Opaque(written.into());
                            payloads.push(error.0.as_ptr() as usize);
                            Err(error)
                        } else {
                            Ok(document)
                        };
                        self.decoder.steps.push_back(MetadataDecodeStep {
                            source: source(identity, raw),
                            target,
                            outcome,
                        });
                    }
                }
                payloads
            }
            fn run(self) -> Outcome {
                let Self {
                    host,
                    source,
                    mut decoder,
                    mut port,
                    ..
                } = self;
                prepare(&mut decoder, source, Some(host), &mut port)
            }
        }
        /// Every effect of preparing the fixture's 1.0 project for Python.
        fn full_effects() -> Vec<String> {
            [
                "decode Project /author/project.yaml",
                "capture python/uv.lock",
                "capture first.yaml",
                "decode FunctionFile /held/first.yaml",
                "capture last.yaml",
                "decode FunctionFile /held/last.yaml",
                "capture ct.yaml",
                "decode CodelistFile /held/ct.yaml",
            ]
            .map(String::from)
            .into()
        }
        /// A 0.5 project's preparation, for the tests whose subject is an
        /// environment file (#3448).
        struct LegacyFixture {
            host: Language,
            source: Source,
            root: Document,
            environment: Document,
            /// The linked environment file fails admission of its own
            /// declarations, so none of its paths are requested.
            environment_rejected: bool,
            log: EventLog,
            port: ScriptedMetadata<Opaque>,
            decoder: ScriptedMetadataDecoder<Opaque>,
        }
        impl LegacyFixture {
            fn new() -> Self {
                let log = EventLog::default();
                Self {
                    host: Language::Python,
                    source: source("/author/project.yaml", b"held root bytes"),
                    root: legacy_project(Tree::text("F")).document(),
                    environment: legacy_environment("7").document(),
                    environment_rejected: false,
                    port: ScriptedMetadata::new(log.clone()),
                    decoder: ScriptedMetadataDecoder::new(log.clone()),
                    log,
                }
            }
            /// The authored link and held identity of the host's environment file.
            fn linked(&self) -> (&'static str, &'static str) {
                match self.host {
                    Language::Python => {
                        ("python/environment.yaml", "/held/python/environment.yaml")
                    }
                    Language::R => ("r/environment.yaml", "/held/r/environment.yaml"),
                }
            }
            /// Link an environment file whose inline function fails scalar admission.
            fn reject_environment(&mut self) {
                self.environment = legacy_environment("9223372036854775808").document();
                self.environment_rejected = true;
            }
            /// Script the root decode and every capture in request order: the
            /// linked environment file (0), its lock (1) and function files (2, 3),
            /// then the project codelist file (4). Lock and function paths resolve
            /// from the environment file and are requested only after it admits.
            fn script(
                &mut self,
                failing: &[usize],
                interrupt: Option<usize>,
                decoding: Option<usize>,
                malformed: Option<usize>,
            ) -> Vec<usize> {
                self.decoder.steps.push_back(MetadataDecodeStep {
                    source: self.source.clone(),
                    target: DecodeTarget::Project,
                    outcome: Ok(self.root.clone()),
                });
                self.port.interrupt_if = |error| error.0.as_ref() == "interrupt";
                let project = self.source.identity.clone();
                let (linked, held) = self.linked();
                let unlinked = self.environment_rejected
                    || failing.contains(&0)
                    || decoding == Some(0)
                    || malformed == Some(0);
                let mut payloads = vec![];
                for (index, (kind, declaring, written, identity, raw)) in [
                    (
                        Kind::Environment,
                        project.as_str(),
                        linked,
                        held,
                        b"environment".as_slice(),
                    ),
                    (
                        Kind::Lock,
                        held,
                        "lockfile",
                        "/held/uv.lock",
                        b"uv".as_slice(),
                    ),
                    (
                        Kind::Function,
                        held,
                        "first.yaml",
                        "/held/first.yaml",
                        b"first".as_slice(),
                    ),
                    (
                        Kind::Function,
                        held,
                        "last.yaml",
                        "/held/last.yaml",
                        b"last".as_slice(),
                    ),
                    (
                        Kind::Codelist,
                        project.as_str(),
                        "ct.yaml",
                        "/held/ct.yaml",
                        b"ct".as_slice(),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    if unlinked && (1..=3).contains(&index) {
                        continue;
                    }
                    let failed = failing.contains(&index) || interrupt == Some(index);
                    let outcome = if failed {
                        let error = Opaque(
                            if interrupt == Some(index) {
                                "interrupt"
                            } else {
                                written
                            }
                            .into(),
                        );
                        payloads.push(error.0.as_ptr() as usize);
                        Err(error)
                    } else {
                        let captured = source(identity, raw);
                        Ok(if (kind == Kind::Lock) != (malformed == Some(index)) {
                            Reply::Lock {
                                source: captured,
                                kind: LockKind::Uv,
                            }
                        } else {
                            Reply::Document(captured)
                        })
                    };
                    self.port.steps.push_back(MetadataCaptureStep {
                        kind,
                        declaring_source: declaring.into(),
                        written: written.into(),
                        outcome,
                    });
                    if kind != Kind::Lock && !failed && malformed != Some(index) {
                        let (target, document) = match kind {
                            Kind::Environment => {
                                (DecodeTarget::Environment, self.environment.clone())
                            }
                            Kind::Function => (DecodeTarget::Function, function("7").document()),
                            _ => (
                                DecodeTarget::Codelist,
                                codelist("SEX", Tree::text("F")).document(),
                            ),
                        };
                        let outcome = if decoding == Some(index) {
                            let error = Opaque(written.into());
                            payloads.push(error.0.as_ptr() as usize);
                            Err(error)
                        } else {
                            Ok(document)
                        };
                        self.decoder.steps.push_back(MetadataDecodeStep {
                            source: source(identity, raw),
                            target,
                            outcome,
                        });
                    }
                }
                payloads
            }
            fn run(self) -> Outcome {
                let Self {
                    host,
                    source,
                    mut decoder,
                    mut port,
                    ..
                } = self;
                prepare(&mut decoder, source, Some(host), &mut port)
            }
        }
        fn effects(log: &EventLog) -> Vec<String> {
            log.events()
                .iter()
                .map(|event| match event {
                    Event::MetadataCapture { written, .. } => format!("capture {written}"),
                    Event::MetadataDecode {
                        target, identity, ..
                    } => format!("decode {target:?} {identity}"),
                    other => panic!("unexpected authority: {other:?}"),
                })
                .collect()
        }

        #[test]
        fn engine_owns_complete_order_semantics_and_original_allocations_on_every_preparation() {
            for _ in 0..2 {
                let mut fixture = Fixture::new();
                fixture.script(&[], None, None, None);
                let root_bytes = fixture.source.bytes.as_ptr();
                let root_nodes = fixture.decoder.steps[0]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .nodes()
                    .as_ptr();
                let function_nodes = fixture.decoder.steps[1]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .nodes()
                    .as_ptr();
                let Reply::Lock { source: lock, .. } =
                    fixture.port.steps[0].outcome.as_ref().unwrap()
                else {
                    panic!()
                };
                let lock_bytes = lock.bytes.as_ptr();
                let log = fixture.log.clone();
                let prepared = fixture.run().unwrap();
                assert_eq!(effects(&log), full_effects());
                assert_eq!(prepared.root.source.bytes.as_ptr(), root_bytes);
                assert_eq!(prepared.root.document.nodes().as_ptr(), root_nodes);
                assert_eq!(
                    prepared.captures[0].document.document.nodes().as_ptr(),
                    function_nodes
                );
                assert_eq!(
                    prepared.lock.as_ref().unwrap().source.bytes.as_ptr(),
                    lock_bytes
                );
                assert_eq!(prepared.origins.functions, ["first", "last"]);
                assert_eq!(prepared.origins.function_files, [0, 1]);
                assert_eq!(prepared.origins.codelists, [0, 1]);
                assert_eq!(
                    prepared
                        .captures
                        .iter()
                        .map(|c| &c.origin)
                        .collect::<Vec<_>>(),
                    [
                        &Origin::FunctionFile(0),
                        &Origin::FunctionFile(1),
                        &Origin::Codelist(1)
                    ]
                );
                // One host language runs the whole build (D8).
                let draft = prepared.environment.draft();
                assert_eq!(draft.language, Some(Language::Python));
                assert_eq!(draft.lock.as_ref().unwrap().written, "python/uv.lock");
                assert_eq!(draft.lock.as_ref().unwrap().kind, LockKind::Uv);
                assert_eq!(
                    draft.functions.as_ref().unwrap()[0].tests[0].result,
                    Value::Int(7)
                );
                let cases = draft.functions.as_ref().unwrap()[0].tests.as_ptr();
                let execution = prepared.environment.into_execution();
                assert_eq!(execution.language(), Some(Language::Python));
                assert_eq!(execution.undeclared(), None);
                assert_eq!(execution.functions()[0].definition().tests.as_ptr(), cases);
                assert_eq!(
                    execution
                        .catalogue()
                        .get("SEX")
                        .unwrap()
                        .items
                        .as_ref()
                        .unwrap()[0]
                        .value,
                    Value::Str("F".into())
                );
                assert!(execution.catalogue().get("INLINE").is_some());
            }
        }

        /// 0.5 only (#3448): an environment file is the subject. A 1.0
        /// project's paths all resolve from the project file (D8,
        /// PROJECT-4), as `a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file`
        /// shows.
        #[test]
        fn lock_and_function_paths_resolve_from_the_environment_file_and_codelists_from_the_project(
        ) {
            for (host, linked, held) in [
                (
                    Language::Python,
                    "python/environment.yaml",
                    "/held/python/environment.yaml",
                ),
                (
                    Language::R,
                    "r/environment.yaml",
                    "/held/r/environment.yaml",
                ),
            ] {
                let mut fixture = LegacyFixture::new();
                fixture.host = host;
                fixture.script(&[], None, None, None);
                let log = fixture.log.clone();
                // The shared fixture's uv lock and callables admit only under
                // Python; either outcome resolves each path from the same file.
                assert_eq!(fixture.run().is_ok(), host == Language::Python);
                let requests = log
                    .events()
                    .iter()
                    .filter_map(|event| match event {
                        Event::MetadataCapture {
                            kind,
                            declaring_source,
                            written,
                        } => Some((*kind, declaring_source.clone(), written.clone())),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let project = "/author/project.yaml";
                assert_eq!(
                    requests,
                    [
                        (Kind::Environment, project, linked),
                        (Kind::Lock, held, "lockfile"),
                        (Kind::Function, held, "first.yaml"),
                        (Kind::Function, held, "last.yaml"),
                        (Kind::Codelist, project, "ct.yaml"),
                    ]
                    .map(|(kind, declaring, written)| (
                        kind,
                        declaring.to_string(),
                        written.to_string()
                    ))
                );
            }
        }

        #[test]
        fn root_decoder_error_keeps_original_opaque_payload_before_dependencies() {
            let mut fixture = Fixture::new();
            let error = Opaque("root decoding".into());
            let address = error.0.as_ptr();
            fixture.decoder.steps.push_back(MetadataDecodeStep {
                source: fixture.source.clone(),
                target: DecodeTarget::Project,
                outcome: Err(error),
            });
            let log = fixture.log.clone();
            let Err(Failure::Root(error)) = fixture.run() else {
                panic!()
            };
            assert_eq!(error.0.as_ptr(), address);
            assert_eq!(effects(&log), ["decode Project /author/project.yaml"]);
        }

        #[test]
        fn root_scalar_failures_keep_the_exact_context_arena_and_precede_all_dependencies() {
            let mut fixture = Fixture::new();
            // A 1.0 environments entry holds its host's lock (D8).
            let mut root = project(Tree::text("F"));
            root.set(&["environments", "python", "lock"], Tree::boolean(true));
            root.set(&["environments", "r", "lock"], Tree::boolean(false));
            fixture.root = root.document();
            fixture.script(&[], None, None, None);
            let nodes = fixture.decoder.steps[0]
                .outcome
                .as_ref()
                .unwrap()
                .nodes()
                .as_ptr();
            let bytes = fixture.source.bytes.as_ptr();
            let log = fixture.log.clone();
            let Err(Failure::RootScalar { root, findings }) = fixture.run() else {
                panic!()
            };
            assert_eq!(root.document.nodes().as_ptr(), nodes);
            assert_eq!(root.source.bytes.as_ptr(), bytes);
            // The whole project decodes, although each host reads only its own key.
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| (finding.path.as_str(), finding.kind))
                    .collect::<Vec<_>>(),
                [
                    ("environments.python.lock", ScalarKind::NormalizedShape),
                    ("environments.r.lock", ScalarKind::NormalizedShape)
                ]
            );
            assert_eq!(root.document.nodes()[findings[0].node], N::Boolean(true));
            assert_eq!(root.document.nodes()[findings[1].node], N::Boolean(false));
            assert_eq!(effects(&log), ["decode Project /author/project.yaml"]);
        }

        #[test]
        fn invalid_inline_codelist_scalar_precedes_every_capture() {
            let mut fixture = Fixture::new();
            fixture.root = project(Tree::integer("9223372036854775808")).document();
            fixture.script(&[], None, None, None);
            let log = fixture.log.clone();
            let Err(Failure::RootScalar { root, findings }) = fixture.run() else {
                panic!()
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "codelists[0].codelists[0].items[0].value");
            assert_eq!(findings[0].kind, ScalarKind::IntegerRange);
            assert_eq!(
                root.document.nodes()[findings[0].node],
                N::Integer("9223372036854775808".into())
            );
            assert_eq!(effects(&log), ["decode Project /author/project.yaml"]);
        }

        /// 0.5 only (#3448): an environment file is the subject, and only a
        /// 0.5 environment file defines a function inline. A 1.0 project
        /// lists function files (D8).
        #[test]
        fn invalid_inline_function_scalar_rejects_its_environment_file_before_its_lock_and_functions(
        ) {
            let mut fixture = LegacyFixture::new();
            fixture.reject_environment();
            fixture.script(&[], None, None, None);
            let nodes = fixture.decoder.steps[1]
                .outcome
                .as_ref()
                .unwrap()
                .nodes()
                .as_ptr();
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture python/environment.yaml",
                    "decode Environment /held/python/environment.yaml",
                    "capture ct.yaml",
                    "decode Codelist /held/ct.yaml",
                ]
            );
            assert_eq!(rejected.sources.len(), 1);
            let failure = &rejected.sources[0];
            assert_eq!(failure.origin, Origin::Environment(Language::Python));
            assert_eq!(failure.written, "python/environment.yaml");
            let CaptureFailure::Scalar { document, findings } = &failure.error else {
                panic!()
            };
            assert_eq!(document.source.identity, "/held/python/environment.yaml");
            assert_eq!(document.document.nodes().as_ptr(), nodes);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "functions.inline.tests[0].result");
            assert_eq!(findings[0].kind, ScalarKind::IntegerRange);
            assert_eq!(
                document.document.nodes()[findings[0].node],
                N::Integer("9223372036854775808".into())
            );
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            assert_eq!(
                rejected
                    .captures
                    .iter()
                    .map(|c| &c.origin)
                    .collect::<Vec<_>>(),
                [&Origin::Codelist(1)]
            );
            assert_eq!(rejected.draft.language, Some(Language::Python));
            assert!(rejected.draft.lock.is_none() && rejected.draft.functions.is_none());
            assert!(rejected.origins.functions.is_empty());
            assert!(rejected.admission.is_none());
        }

        #[test]
        fn independent_capture_failures_collect_in_declaration_order_without_relabeling() {
            let mut fixture = Fixture::new();
            let addresses = fixture.script(&[0, 1, 2, 3], None, None, None);
            let root_bytes = fixture.source.bytes.as_ptr();
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(rejected.root.source.bytes.as_ptr(), root_bytes);
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| f.written.as_str())
                    .collect::<Vec<_>>(),
                ["python/uv.lock", "first.yaml", "last.yaml", "ct.yaml"]
            );
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| f.origin.clone())
                    .collect::<Vec<_>>(),
                [
                    Origin::Lock,
                    Origin::FunctionFile(0),
                    Origin::FunctionFile(1),
                    Origin::Codelist(1)
                ]
            );
            for (failure, address) in rejected.sources.iter().zip(addresses) {
                let CaptureFailure::Port(error) = &failure.error else {
                    panic!()
                };
                assert_eq!(error.0.as_ptr() as usize, address);
            }
            assert!(rejected.admission.is_none()); // An authored lock failing capture is not absent.
            assert!(rejected.origins.functions.is_empty());
            assert_eq!(rejected.origins.codelists, [0]);
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture python/uv.lock",
                    "capture first.yaml",
                    "capture last.yaml",
                    "capture ct.yaml"
                ]
            );
        }

        /// 0.5 only (#3448): an environment file is the subject. A 1.0
        /// project links none (D8).
        #[test]
        fn environment_file_capture_failure_keeps_its_origin_and_skips_only_its_own_paths() {
            let mut fixture = LegacyFixture::new();
            let addresses = fixture.script(&[0, 4], None, None, None);
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| (f.origin.clone(), f.written.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (
                        Origin::Environment(Language::Python),
                        "python/environment.yaml"
                    ),
                    (Origin::Codelist(1), "ct.yaml")
                ]
            );
            for (failure, address) in rejected.sources.iter().zip(addresses) {
                let CaptureFailure::Port(error) = &failure.error else {
                    panic!()
                };
                assert_eq!(error.0.as_ptr() as usize, address);
            }
            assert!(rejected.environment_file.is_none());
            assert!(rejected.lock.is_none());
            assert!(rejected.captures.is_empty());
            // The key still states the language although no code was read.
            assert_eq!(rejected.draft.language, Some(Language::Python));
            assert!(rejected.draft.lock.is_none() && rejected.draft.functions.is_none());
            assert_eq!(rejected.draft.codelists.len(), 1);
            assert!(rejected.origins.functions.is_empty());
            assert_eq!(rejected.origins.codelists, [0]);
            assert!(rejected.admission.is_none());
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture python/environment.yaml",
                    "capture ct.yaml"
                ]
            );
        }

        #[test]
        fn every_capture_interrupt_keeps_original_payload_and_aborts_the_later_authority() {
            for (index, expected_origin, written) in [
                (0, Origin::Lock, "python/uv.lock"),
                (1, Origin::FunctionFile(0), "first.yaml"),
                (2, Origin::FunctionFile(1), "last.yaml"),
                (3, Origin::Codelist(1), "ct.yaml"),
            ] {
                let mut fixture = Fixture::new();
                let address = fixture.script(&[], Some(index), None, None)[0];
                let log = fixture.log.clone();
                let Err(Failure::Interrupted {
                    origin,
                    written: actual,
                    error,
                }) = fixture.run()
                else {
                    panic!()
                };
                assert_eq!(origin, expected_origin);
                assert_eq!(actual, written);
                assert_eq!(error.0.as_ptr() as usize, address);
                let expected = full_effects();
                let stop = expected
                    .iter()
                    .position(|e| e == &format!("capture {written}"))
                    .unwrap();
                assert_eq!(effects(&log), expected[..=stop]);
            }
        }

        #[test]
        fn dependency_decoder_errors_preserve_original_payload_and_later_independent_sources() {
            for (index, origin, captures) in [
                (1, Origin::FunctionFile(0), 2),
                (2, Origin::FunctionFile(1), 2),
                (3, Origin::Codelist(1), 2),
            ] {
                let mut fixture = Fixture::new();
                let address = fixture.script(&[], None, Some(index), None)[0];
                let log = fixture.log.clone();
                let Err(Failure::Rejected(rejected)) = fixture.run() else {
                    panic!()
                };
                assert_eq!(effects(&log), full_effects());
                assert_eq!(rejected.sources.len(), 1);
                assert_eq!(rejected.sources[0].origin, origin);
                let CaptureFailure::Structural(error) = &rejected.sources[0].error else {
                    panic!()
                };
                assert_eq!(error.0.as_ptr() as usize, address);
                assert_eq!(rejected.captures.len(), captures);
                assert!(rejected.admission.is_none());
            }
        }

        #[test]
        fn dependency_scalar_failures_retain_exact_document_and_qualify_later_sources() {
            // The decodes of the first function file and the codelist file.
            for (index, origin) in [(1, Origin::FunctionFile(0)), (3, Origin::Codelist(1))] {
                let mut fixture = Fixture::new();
                fixture.script(&[], None, None, None);
                fixture.decoder.steps[index].outcome = Ok(if index == 1 {
                    function_file(
                        "first",
                        &[("python", "project.constant")],
                        "9223372036854775808",
                    )
                } else {
                    let mut file = codelist_file();
                    file.set(
                        &["codelists", "0", "items", "0", "value"],
                        Tree::integer("9223372036854775808"),
                    );
                    file
                }
                .document());
                let nodes = fixture.decoder.steps[index]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .nodes()
                    .as_ptr();
                let log = fixture.log.clone();
                let Err(Failure::Rejected(rejected)) = fixture.run() else {
                    panic!()
                };
                assert_eq!(effects(&log), full_effects());
                assert_eq!(rejected.sources[0].origin, origin);
                let CaptureFailure::Scalar { document, findings } = &rejected.sources[0].error
                else {
                    panic!()
                };
                assert_eq!(document.document.nodes().as_ptr(), nodes);
                assert_eq!(findings.len(), 1);
                assert_eq!(findings[0].kind, ScalarKind::IntegerRange);
                assert_eq!(
                    findings[0].path,
                    if index == 1 {
                        "tests[0].result"
                    } else {
                        "codelists[0].items[0].value"
                    }
                );
                assert_eq!(
                    document.document.nodes()[findings[0].node],
                    N::Integer("9223372036854775808".into())
                );
                assert_eq!(rejected.captures.len(), 2);
                assert!(rejected.admission.is_none());
            }
        }

        #[test]
        fn native_reply_kind_refusals_collect_without_decoding_the_wrong_kind() {
            for index in 0..4 {
                let mut fixture = Fixture::new();
                fixture.script(&[], None, None, Some(index));
                let log = fixture.log.clone();
                let Err(Failure::Rejected(rejected)) = fixture.run() else {
                    panic!()
                };
                assert_eq!(rejected.sources.len(), 1);
                assert!(matches!(
                    rejected.sources[0].error,
                    CaptureFailure::Protocol
                ));
                assert!(rejected.admission.is_none());
                let mut expected = full_effects();
                // A lock reply has no document decode to skip.
                if index > 0 {
                    expected.remove([3, 5, 7][index - 1]);
                }
                assert_eq!(effects(&log), expected);
            }
        }

        #[test]
        fn lock_kind_and_function_findings_are_core_owned_after_capture_without_code_authority() {
            let mut fixture = Fixture::new();
            fixture.host = Language::R;
            fixture.script(&[], None, None, None);
            // Each function file names an R callable that is not one (FUNC-8).
            for (step, name) in [(1, "first"), (2, "last")] {
                fixture.decoder.steps[step].outcome =
                    Ok(function_file(name, &[("r", "project.constant")], "7").document());
            }
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            // Only the host's own lock is read (PROJECT-2).
            let mut expected = full_effects();
            expected[1] = "capture r/renv.lock".into();
            assert_eq!(effects(&log), expected);
            assert_eq!(rejected.host, Some(Language::R));
            assert_eq!(rejected.draft.language, Some(Language::R));
            // The shared fixture's uv lock and both callables are invalid under R.
            assert_eq!(
                rejected.admission,
                Some(vec![
                    Finding::LockKindMismatch {
                        language: Language::R,
                        actual: LockKind::Uv,
                    },
                    Finding::Function {
                        function: 0,
                        finding: yamaa_core::project_function::Finding::InvalidImplementation(
                            Language::R
                        ),
                    },
                    Finding::Function {
                        function: 1,
                        finding: yamaa_core::project_function::Finding::InvalidImplementation(
                            Language::R
                        ),
                    },
                ])
            );
        }

        #[test]
        fn empty_project_has_only_root_decode_and_no_dependency_or_activation_authority() {
            let log = EventLog::default();
            let root = source("empty.yaml", b"held 1.0 project");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(Tree::map(vec![
                    ("yamaa", Tree::text("1.0")),
                    ("type", Tree::text("project")),
                ])
                .document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let prepared = prepare(&mut decoder, root, Some(Language::Python), &mut port).unwrap();
            assert_eq!(effects(&log), ["decode Project empty.yaml"]);
            assert!(prepared.environment_file.is_none());
            assert!(prepared.environment.draft().functions.is_none());
            assert!(prepared.captures.is_empty());
            assert!(prepared.lock.is_none());
        }

        /// 0.5 only (#3448): an environment file is the subject, and a 0.5
        /// project's `environments` key states the host's language. A 1.0
        /// host reads only its own lock (PROJECT-2), as
        /// `a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file`
        /// shows.
        #[test]
        fn a_project_without_an_environment_for_the_host_leaves_other_languages_unread() {
            for (host, other) in [(Language::Python, "r"), (Language::R, "python")] {
                let log = EventLog::default();
                let root = source("project.yaml", b"held project");
                let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
                decoder.steps.push_back(MetadataDecodeStep {
                    source: root.clone(),
                    target: DecodeTarget::Project,
                    outcome: Ok(Tree::map(vec![
                        (
                            "environments",
                            Tree::map(vec![(other, Tree::text("other/environment.yaml"))]),
                        ),
                        (
                            "codelists",
                            Tree::List(vec![codelist("SEX", Tree::text("F"))]),
                        ),
                    ])
                    .document()),
                });
                let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
                let prepared = prepare(&mut decoder, root, Some(host), &mut port).unwrap();
                assert_eq!(effects(&log), ["decode Project project.yaml"]);
                assert!(prepared.environment_file.is_none());
                assert!(prepared.captures.is_empty() && prepared.lock.is_none());
                let draft = prepared.environment.draft();
                assert_eq!(draft.language, None);
                assert!(draft.lock.is_none() && draft.functions.is_none());
                assert_eq!(draft.codelists.len(), 1);
                assert_eq!(prepared.origins.codelists, [0]);
                let execution = prepared.environment.into_execution();
                assert_eq!(execution.host(), Some(host));
                assert_eq!(execution.undeclared(), Some(Undeclared::Language(host)));
                assert!(execution.catalogue().get("SEX").is_some());
            }
        }

        #[test]
        fn explicitly_empty_functions_keep_presence_without_a_lock() {
            let log = EventLog::default();
            let root = source(
                "project.yaml",
                b"yamaa: '1.0'\ntype: project\nfunctions: []",
            );
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(Tree::map(vec![
                    ("yamaa", Tree::text("1.0")),
                    ("type", Tree::text("project")),
                    ("functions", Tree::List(vec![])),
                ])
                .document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let prepared = prepare(&mut decoder, root, Some(Language::Python), &mut port).unwrap();
            assert_eq!(effects(&log), ["decode Project project.yaml"]);
            assert_eq!(prepared.environment.draft().functions, Some(vec![]));
            assert_eq!(
                prepared.environment.draft().language,
                Some(Language::Python)
            );
            assert!(prepared.lock.is_none());
            let execution = prepared.environment.into_execution();
            assert!(execution.functions_present());
            assert_eq!(execution.undeclared(), None);
        }

        #[test]
        fn metadata_files_each_admit_the_fixed_size_edge_without_a_cumulative_allowance() {
            for length in [Limits::SPEC_BYTES - 1, Limits::SPEC_BYTES] {
                let mut fixture = Fixture::new();
                fixture.source.bytes.resize(length, b' ');
                fixture.script(&[], None, None, None);
                let log = fixture.log.clone();
                assert!(fixture.run().is_ok());
                assert_eq!(effects(&log), full_effects());
            }
        }

        #[test]
        fn oversized_metadata_file_refuses_before_decoder_and_dependency_authority() {
            let mut fixture = Fixture::new();
            fixture.source.bytes.resize(Limits::SPEC_BYTES + 1, b' ');
            let log = fixture.log.clone();
            let Err(Failure::RootLimit(limit)) = fixture.run() else {
                panic!("oversized root metadata must refuse before decode")
            };
            assert_eq!(limit.resource, "spec_bytes");
            assert_eq!(limit.limit, Limits::SPEC_BYTES);
            assert_eq!(limit.actual, Some(Limits::SPEC_BYTES + 1));
            assert!(effects(&log).is_empty());
        }

        #[test]
        fn oversized_captured_metadata_refuses_before_retention_and_preserves_later_capture_order()
        {
            let mut fixture = Fixture::new();
            fixture.script(&[], None, None, None);
            fixture.port.steps[0].outcome = Ok(Reply::Lock {
                source: source("/held/python/uv.lock", &vec![b' '; Limits::SPEC_BYTES + 1]),
                kind: LockKind::Uv,
            });
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!("oversized dependency must reject the environment")
            };
            assert_eq!(effects(&log), full_effects());
            assert!(rejected.lock.is_none());
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::Lock);
            let CaptureFailure::Limit(limit) = &rejected.sources[0].error else {
                panic!("the dependency failure must retain the fixed size boundary")
            };
            assert_eq!(limit.resource, "spec_bytes");
            assert_eq!(limit.limit, Limits::SPEC_BYTES);
            assert_eq!(limit.actual, Some(Limits::SPEC_BYTES + 1));
            assert!(rejected.admission.is_none());
        }

        /// A 1.0 project lists its function files and holds each host
        /// language's lock, all relative to the project file (D8).
        fn project_v1() -> Tree {
            Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("project")),
                (
                    "functions",
                    Tree::List(vec![
                        Tree::text("functions/python.yaml"),
                        Tree::text("functions/both.yaml"),
                    ]),
                ),
                (
                    "environments",
                    Tree::map(vec![
                        (
                            "python",
                            Tree::map(vec![("lock", Tree::text("python/uv.lock"))]),
                        ),
                        ("r", Tree::map(vec![("lock", Tree::text("r/renv.lock"))])),
                    ]),
                ),
                ("codelists", Tree::List(vec![Tree::text("ct.yaml")])),
            ])
        }
        /// `function(result)` as a 1.0 function file: its own header and
        /// name, and the callable of each host it implements.
        fn function_file(name: &str, implementations: &[(&str, &str)], result: &str) -> Tree {
            let Tree::Map(fields) = function(result) else {
                unreachable!("a definition is a mapping")
            };
            let mut file = vec![
                ("yamaa".to_string(), Tree::text("1.0")),
                ("type".into(), Tree::text("function")),
                ("name".into(), Tree::text(name)),
            ];
            file.extend(fields.into_iter().filter(|(key, _)| key != "function"));
            file.push((
                "implementations".into(),
                Tree::Map(
                    implementations
                        .iter()
                        .map(|&(host, callable)| (host.to_string(), Tree::text(callable)))
                        .collect(),
                ),
            ));
            Tree::Map(file)
        }
        fn codelist_file() -> Tree {
            let Tree::Map(fields) = codelist("SEX", Tree::text("F")) else {
                unreachable!("a codelist source is a mapping")
            };
            let mut file = vec![
                ("yamaa".to_string(), Tree::text("1.0")),
                ("type".into(), Tree::text("codelists")),
            ];
            file.extend(fields);
            Tree::Map(file)
        }

        #[test]
        fn a_1_0_host_reads_only_its_own_lock_and_every_listed_function_file() {
            let held = |written: &str| format!("/held/{written}");
            for (host, lock, kind) in [
                (Language::Python, "python/uv.lock", LockKind::Uv),
                (Language::R, "r/renv.lock", LockKind::Renv),
            ] {
                let log = EventLog::default();
                let root = source("/author/project.yaml", b"held 1.0 project");
                let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
                decoder.steps.push_back(MetadataDecodeStep {
                    source: root.clone(),
                    target: DecodeTarget::Project,
                    outcome: Ok(project_v1().document()),
                });
                let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
                // Only the host's own lock is read, from the project file.
                port.steps.push_back(MetadataCaptureStep {
                    kind: Kind::Lock,
                    declaring_source: "/author/project.yaml".into(),
                    written: lock.into(),
                    outcome: Ok(Reply::Lock {
                        source: source(&held(lock), b"held lock"),
                        kind,
                    }),
                });
                // Every listed file is read, whichever hosts it implements.
                for (written, file) in [
                    (
                        "functions/python.yaml",
                        function_file("only_python", &[("python", "project.constant")], "7"),
                    ),
                    (
                        "functions/both.yaml",
                        function_file(
                            "both",
                            &[("python", "project.constant"), ("r", "project::constant")],
                            "7",
                        ),
                    ),
                ] {
                    let captured = source(&held(written), written.as_bytes());
                    port.steps.push_back(MetadataCaptureStep {
                        kind: Kind::Function,
                        declaring_source: "/author/project.yaml".into(),
                        written: written.into(),
                        outcome: Ok(Reply::Document(captured.clone())),
                    });
                    decoder.steps.push_back(MetadataDecodeStep {
                        source: captured,
                        target: DecodeTarget::FunctionFile,
                        outcome: Ok(file.document()),
                    });
                }
                let terminology = source(&held("ct.yaml"), b"held codelists");
                port.steps.push_back(MetadataCaptureStep {
                    kind: Kind::Codelist,
                    declaring_source: "/author/project.yaml".into(),
                    written: "ct.yaml".into(),
                    outcome: Ok(Reply::Document(terminology.clone())),
                });
                decoder.steps.push_back(MetadataDecodeStep {
                    source: terminology,
                    target: DecodeTarget::CodelistFile,
                    outcome: Ok(codelist_file().document()),
                });
                let prepared = prepare(&mut decoder, root, Some(host), &mut port).unwrap();
                assert_eq!(
                    effects(&log),
                    [
                        "decode Project /author/project.yaml".to_string(),
                        format!("capture {lock}"),
                        "capture functions/python.yaml".into(),
                        "decode FunctionFile /held/functions/python.yaml".into(),
                        "capture functions/both.yaml".into(),
                        "decode FunctionFile /held/functions/both.yaml".into(),
                        "capture ct.yaml".into(),
                        "decode CodelistFile /held/ct.yaml".into(),
                    ]
                );
                assert!(prepared.environment_file.is_none());
                assert_eq!(prepared.origins.functions, ["only_python", "both"]);
                assert_eq!(prepared.origins.function_files, [0, 1]);
                assert_eq!(prepared.origins.codelists, [0]);
                assert_eq!(
                    prepared
                        .captures
                        .iter()
                        .map(|capture| capture.origin.clone())
                        .collect::<Vec<_>>(),
                    [
                        Origin::FunctionFile(0),
                        Origin::FunctionFile(1),
                        Origin::Codelist(0)
                    ]
                );
                assert_eq!(prepared.lock.as_ref().unwrap().kind, kind);
                let draft = prepared.environment.draft();
                assert_eq!(draft.surface, yamaa_core::ast::Surface::V1 { minor: 0 });
                assert_eq!(draft.language, Some(host));
                assert_eq!(draft.lock.as_ref().unwrap().written, lock);
                // A function file that names no callable for the host is
                // admitted; a call of it fails before any data is read.
                let execution = prepared.environment.into_execution();
                assert_eq!(execution.undeclared(), None);
                assert_eq!(
                    execution
                        .functions()
                        .iter()
                        .map(|function| (function.name(), function.missing_implementation()))
                        .collect::<Vec<_>>(),
                    [
                        ("only_python", (host == Language::R).then_some(Language::R)),
                        ("both", None)
                    ]
                );
                assert!(execution.catalogue().get("SEX").is_some());
            }
        }

        /// An engine that claims no host language, such as the command
        /// line's, reads no `environments` entry, so no lock. A 1.0 project's
        /// function files and codelists are still read and admitted; its
        /// functions name no callable for such an engine and miss none
        /// (#3030 section 2.10).
        #[test]
        fn an_engine_without_a_host_language_reads_no_environments_entry() {
            let held = |written: &str| format!("/held/{written}");
            let log = EventLog::default();
            let root = source("/author/project.yaml", b"held 1.0 project");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(project_v1().document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            for (written, file) in [
                (
                    "functions/python.yaml",
                    function_file("only_python", &[("python", "project.constant")], "7"),
                ),
                (
                    "functions/both.yaml",
                    function_file(
                        "both",
                        &[("python", "project.constant"), ("r", "project::constant")],
                        "7",
                    ),
                ),
            ] {
                let captured = source(&held(written), written.as_bytes());
                port.steps.push_back(MetadataCaptureStep {
                    kind: Kind::Function,
                    declaring_source: "/author/project.yaml".into(),
                    written: written.into(),
                    outcome: Ok(Reply::Document(captured.clone())),
                });
                decoder.steps.push_back(MetadataDecodeStep {
                    source: captured,
                    target: DecodeTarget::FunctionFile,
                    outcome: Ok(file.document()),
                });
            }
            let terminology = source(&held("ct.yaml"), b"held codelists");
            port.steps.push_back(MetadataCaptureStep {
                kind: Kind::Codelist,
                declaring_source: "/author/project.yaml".into(),
                written: "ct.yaml".into(),
                outcome: Ok(Reply::Document(terminology.clone())),
            });
            decoder.steps.push_back(MetadataDecodeStep {
                source: terminology,
                target: DecodeTarget::CodelistFile,
                outcome: Ok(codelist_file().document()),
            });
            let prepared = prepare(&mut decoder, root, None, &mut port).unwrap();
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture functions/python.yaml",
                    "decode FunctionFile /held/functions/python.yaml",
                    "capture functions/both.yaml",
                    "decode FunctionFile /held/functions/both.yaml",
                    "capture ct.yaml",
                    "decode CodelistFile /held/ct.yaml",
                ]
            );
            assert!(prepared.lock.is_none() && prepared.environment_file.is_none());
            assert_eq!(prepared.origins.functions, ["only_python", "both"]);
            let draft = prepared.environment.draft();
            assert_eq!(draft.language, None);
            assert!(draft.lock.is_none());
            let execution = prepared.environment.into_execution();
            assert_eq!(execution.host(), None);
            assert_eq!(execution.undeclared(), None);
            assert!(execution.lock().is_none());
            assert_eq!(
                execution
                    .functions()
                    .iter()
                    .map(|function| (
                        function.name(),
                        function.missing_implementation(),
                        function.call()
                    ))
                    .collect::<Vec<_>>(),
                [("only_python", None, ""), ("both", None, "")]
            );
            assert!(execution.catalogue().get("SEX").is_some());
        }

        /// 0.5 only (#3448): an environment file is the subject. A 1.0
        /// project for such an engine is the test above.
        #[test]
        fn a_0_5_project_s_environment_files_stay_unread_without_a_host_language() {
            // A 0.5 project's environment files stay unread, so it declares no
            // function for such an engine.
            let log = EventLog::default();
            let root = source("project.yaml", b"held project");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(Tree::map(vec![(
                    "environments",
                    Tree::map(vec![
                        ("python", Tree::text("python/environment.yaml")),
                        ("r", Tree::text("r/environment.yaml")),
                    ]),
                )])
                .document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let prepared = prepare(&mut decoder, root, None, &mut port).unwrap();
            assert_eq!(effects(&log), ["decode Project project.yaml"]);
            assert!(prepared.environment_file.is_none());
            assert!(prepared.captures.is_empty() && prepared.lock.is_none());
            let draft = prepared.environment.draft();
            assert_eq!(draft.language, None);
            assert!(draft.functions.is_none());
            let execution = prepared.environment.into_execution();
            assert_eq!(execution.host(), None);
            assert_eq!(execution.undeclared(), Some(Undeclared::NoHostLanguage));
        }

        #[test]
        fn a_listed_function_file_that_fails_keeps_its_listing() {
            let log = EventLog::default();
            let root = source("/author/project.yaml", b"held 1.0 project");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            let mut project = project_v1();
            project.remove(&["environments"]);
            project.remove(&["codelists"]);
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(project.document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            port.steps.push_back(MetadataCaptureStep {
                kind: Kind::Function,
                declaring_source: "/author/project.yaml".into(),
                written: "functions/python.yaml".into(),
                outcome: Err(Opaque("unreadable".into())),
            });
            let wide = source("/held/functions/both.yaml", b"wide");
            port.steps.push_back(MetadataCaptureStep {
                kind: Kind::Function,
                declaring_source: "/author/project.yaml".into(),
                written: "functions/both.yaml".into(),
                outcome: Ok(Reply::Document(wide.clone())),
            });
            decoder.steps.push_back(MetadataDecodeStep {
                source: wide,
                target: DecodeTarget::FunctionFile,
                outcome: Ok(function_file(
                    "both",
                    &[("python", "project.constant")],
                    "9223372036854775808",
                )
                .document()),
            });
            let Err(Failure::Rejected(rejected)) =
                prepare(&mut decoder, root, Some(Language::Python), &mut port)
            else {
                panic!("both listed function files fail")
            };
            // Without a lock entry the host declares no lock, so none is read.
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture functions/python.yaml",
                    "capture functions/both.yaml",
                    "decode FunctionFile /held/functions/both.yaml",
                ]
            );
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|failure| (failure.origin.clone(), failure.written.as_str()))
                    .collect::<Vec<_>>(),
                [
                    (Origin::FunctionFile(0), "functions/python.yaml"),
                    (Origin::FunctionFile(1), "functions/both.yaml"),
                ]
            );
            assert!(matches!(rejected.sources[0].error, CaptureFailure::Port(_)));
            let CaptureFailure::Scalar { findings, .. } = &rejected.sources[1].error else {
                panic!("an unreadable value in the second file")
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "tests[0].result");
            assert_eq!(findings[0].kind, ScalarKind::IntegerRange);
            assert!(rejected.origins.functions.is_empty());
            assert!(rejected.origins.function_files.is_empty());
            assert_eq!(rejected.draft.functions, Some(vec![]));
            assert!(rejected.admission.is_none());
        }
    }
}
