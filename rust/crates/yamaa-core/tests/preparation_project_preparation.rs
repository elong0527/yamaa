//! Preparation contracts: project preparation, schema capture, source profile
//! inspection, standalone preparation and structural preparation.
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
        /// The caller-supplied project file links one environment file per host
        /// language and declares its own codelists, study and sections.
        fn project(inline_value: Tree) -> Tree {
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
        fn environment(inline_result: &str) -> Tree {
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
        struct Fixture {
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
        impl Fixture {
            fn new() -> Self {
                let log = EventLog::default();
                Self {
                    host: Language::Python,
                    source: source("/author/project.yaml", b"held root bytes"),
                    root: project(Tree::text("F")).document(),
                    environment: environment("7").document(),
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
                self.environment = environment("9223372036854775808").document();
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
        fn full_effects() -> Vec<String> {
            [
                "decode Project /author/project.yaml",
                "capture python/environment.yaml",
                "decode Environment /held/python/environment.yaml",
                "capture lockfile",
                "capture first.yaml",
                "decode Function /held/first.yaml",
                "capture last.yaml",
                "decode Function /held/last.yaml",
                "capture ct.yaml",
                "decode Codelist /held/ct.yaml",
            ]
            .map(String::from)
            .into()
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
                let environment_nodes = fixture.decoder.steps[1]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .nodes()
                    .as_ptr();
                let function_nodes = fixture.decoder.steps[2]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .nodes()
                    .as_ptr();
                let Reply::Document(linked) = fixture.port.steps[0].outcome.as_ref().unwrap()
                else {
                    panic!()
                };
                let environment_bytes = linked.bytes.as_ptr();
                let Reply::Lock { source: lock, .. } =
                    fixture.port.steps[1].outcome.as_ref().unwrap()
                else {
                    panic!()
                };
                let lock_bytes = lock.bytes.as_ptr();
                let log = fixture.log.clone();
                let prepared = fixture.run().unwrap();
                assert_eq!(effects(&log), full_effects());
                assert_eq!(prepared.root.source.bytes.as_ptr(), root_bytes);
                assert_eq!(prepared.root.document.nodes().as_ptr(), root_nodes);
                // The linked environment file is retained beside, not among, the captures.
                let linked = prepared.environment_file.as_ref().unwrap();
                assert_eq!(linked.source.identity, "/held/python/environment.yaml");
                assert_eq!(linked.source.bytes.as_ptr(), environment_bytes);
                assert_eq!(linked.document.nodes().as_ptr(), environment_nodes);
                assert_eq!(
                    prepared.captures[0].document.document.nodes().as_ptr(),
                    function_nodes
                );
                assert_eq!(
                    prepared.lock.as_ref().unwrap().source.bytes.as_ptr(),
                    lock_bytes
                );
                assert_eq!(prepared.origins.functions, ["first", "inline", "last"]);
                assert_eq!(prepared.origins.codelists, [0, 1]);
                assert_eq!(
                    prepared
                        .captures
                        .iter()
                        .map(|c| &c.origin)
                        .collect::<Vec<_>>(),
                    [
                        &Origin::Function("first".into()),
                        &Origin::Function("last".into()),
                        &Origin::Codelist(1)
                    ]
                );
                // The `environments` key states the environment file's language.
                let draft = prepared.environment.draft();
                assert_eq!(draft.language, Some(Language::Python));
                assert_eq!(draft.lock.as_ref().unwrap().written, "lockfile");
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
                let mut fixture = Fixture::new();
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
            let mut root = project(Tree::text("F"));
            root.set(&["environments", "python"], Tree::boolean(true));
            root.set(&["environments", "r"], Tree::boolean(false));
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
                    ("environments.python", ScalarKind::NormalizedShape),
                    ("environments.r", ScalarKind::NormalizedShape)
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

        #[test]
        fn invalid_inline_function_scalar_rejects_its_environment_file_before_its_lock_and_functions(
        ) {
            let mut fixture = Fixture::new();
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
            let addresses = fixture.script(&[1, 2, 3, 4], None, None, None);
            let root_bytes = fixture.source.bytes.as_ptr();
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(rejected.root.source.bytes.as_ptr(), root_bytes);
            assert_eq!(
                rejected.environment_file.as_ref().unwrap().source.identity,
                "/held/python/environment.yaml"
            );
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| f.written.as_str())
                    .collect::<Vec<_>>(),
                ["lockfile", "first.yaml", "last.yaml", "ct.yaml"]
            );
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|f| f.origin.clone())
                    .collect::<Vec<_>>(),
                [
                    Origin::Lock,
                    Origin::Function("first".into()),
                    Origin::Function("last".into()),
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
            assert_eq!(rejected.origins.functions, ["inline"]);
            assert_eq!(rejected.origins.codelists, [0]);
            assert_eq!(
                effects(&log),
                [
                    "decode Project /author/project.yaml",
                    "capture python/environment.yaml",
                    "decode Environment /held/python/environment.yaml",
                    "capture lockfile",
                    "capture first.yaml",
                    "capture last.yaml",
                    "capture ct.yaml"
                ]
            );
        }

        #[test]
        fn environment_file_capture_failure_keeps_its_origin_and_skips_only_its_own_paths() {
            let mut fixture = Fixture::new();
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
                (
                    0,
                    Origin::Environment(Language::Python),
                    "python/environment.yaml",
                ),
                (1, Origin::Lock, "lockfile"),
                (2, Origin::Function("first".into()), "first.yaml"),
                (3, Origin::Function("last".into()), "last.yaml"),
                (4, Origin::Codelist(1), "ct.yaml"),
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
                (0, Origin::Environment(Language::Python), 1),
                (2, Origin::Function("first".into()), 2),
                (3, Origin::Function("last".into()), 2),
                (4, Origin::Codelist(1), 2),
            ] {
                let mut fixture = Fixture::new();
                let address = fixture.script(&[], None, Some(index), None)[0];
                let log = fixture.log.clone();
                let Err(Failure::Rejected(rejected)) = fixture.run() else {
                    panic!()
                };
                let mut expected = full_effects();
                if index == 0 {
                    // A rejected environment file requests none of its own paths.
                    expected.drain(3..8);
                }
                assert_eq!(effects(&log), expected);
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
            for (index, origin) in [
                (2, Origin::Function("first".into())),
                (4, Origin::Codelist(1)),
            ] {
                let mut fixture = Fixture::new();
                fixture.script(&[], None, None, None);
                fixture.decoder.steps[index].outcome = Ok(if index == 2 {
                    function("9223372036854775808")
                } else {
                    codelist("SEX", Tree::integer("9223372036854775808"))
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
                    if index == 2 {
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
            for index in 0..5 {
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
                match index {
                    // Neither the environment file nor its own paths are read.
                    0 => {
                        expected.drain(2..8);
                    }
                    // A lock reply has no document decode to skip.
                    1 => {}
                    _ => {
                        expected.remove([5, 7, 9][index - 2]);
                    }
                }
                assert_eq!(effects(&log), expected);
            }
        }

        #[test]
        fn lock_kind_and_function_findings_are_core_owned_after_capture_without_code_authority() {
            let mut fixture = Fixture::new();
            fixture.host = Language::R;
            fixture.script(&[], None, None, None);
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            // Only the host's own `environments` key is read.
            let mut expected = full_effects();
            expected[1] = "capture r/environment.yaml".into();
            expected[2] = "decode Environment /held/r/environment.yaml".into();
            assert_eq!(effects(&log), expected);
            assert_eq!(rejected.host, Some(Language::R));
            assert_eq!(
                rejected.environment_file.as_ref().unwrap().source.identity,
                "/held/r/environment.yaml"
            );
            assert_eq!(rejected.draft.language, Some(Language::R));
            // The shared Python fixture's uv lock and three callables are invalid under R.
            assert_eq!(
                rejected.admission,
                Some(vec![
                    Finding::LockKindMismatch {
                        language: Language::R,
                        actual: LockKind::Uv,
                    },
                    Finding::Function {
                        function: 0,
                        finding: yamaa_core::project_function::Finding::InvalidCallable,
                    },
                    Finding::Function {
                        function: 1,
                        finding: yamaa_core::project_function::Finding::InvalidCallable,
                    },
                    Finding::Function {
                        function: 2,
                        finding: yamaa_core::project_function::Finding::InvalidCallable,
                    },
                ])
            );
        }

        #[test]
        fn empty_project_has_only_root_decode_and_no_dependency_or_activation_authority() {
            let log = EventLog::default();
            let root = source("empty.yaml", b"{}");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(Tree::map(vec![]).document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let prepared = prepare(&mut decoder, root, Some(Language::Python), &mut port).unwrap();
            assert_eq!(effects(&log), ["decode Project empty.yaml"]);
            assert!(prepared.environment_file.is_none());
            assert_eq!(prepared.environment.draft().language, None);
            assert!(prepared.environment.draft().functions.is_none());
            assert!(prepared.captures.is_empty());
            assert!(prepared.lock.is_none());
            assert_eq!(
                prepared.environment.into_execution().undeclared(),
                Some(Undeclared::Language(Language::Python))
            );
        }

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
            let root = source("project.yaml", b"environments: {python: environment.yaml}");
            let linked = source("/held/environment.yaml", b"functions: {}");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Project,
                outcome: Ok(Tree::map(vec![(
                    "environments",
                    Tree::map(vec![("python", Tree::text("environment.yaml"))]),
                )])
                .document()),
            });
            decoder.steps.push_back(MetadataDecodeStep {
                source: linked.clone(),
                target: DecodeTarget::Environment,
                outcome: Ok(Tree::map(vec![("functions", Tree::map(vec![]))]).document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            port.steps.push_back(MetadataCaptureStep {
                kind: Kind::Environment,
                declaring_source: "project.yaml".into(),
                written: "environment.yaml".into(),
                outcome: Ok(Reply::Document(linked)),
            });
            let prepared = prepare(&mut decoder, root, Some(Language::Python), &mut port).unwrap();
            assert_eq!(
                effects(&log),
                [
                    "decode Project project.yaml",
                    "capture environment.yaml",
                    "decode Environment /held/environment.yaml"
                ]
            );
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
            fixture.port.steps[1].outcome = Ok(Reply::Lock {
                source: source("/held/uv.lock", &vec![b' '; Limits::SPEC_BYTES + 1]),
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
        /// line's, reads no `environments` entry: no 1.0 lock and no 0.5
        /// environment file. A 1.0 project's function files and codelists are
        /// still read and admitted; its functions name no callable for such an
        /// engine and miss none (#3030 section 2.10).
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
    mod schema_capture {
        use std::{
            cell::{Cell, RefCell},
            collections::VecDeque,
            rc::Rc,
        };
        use yamaa_core::schema::{
            BundleError, BundleIssue, BundleIssueKind, Document, DocumentNode as N,
            NormalizationError, SchemaOrigin, SchemaSource,
        };
        use yamaa_core::{
            project_preparation::Source,
            schema_capture::{
                self, CapturedClosure, DecodedModule, Error, RootSelection, SchemaDecodePort,
            },
        };

        use crate::support::document::Tree;

        fn text(s: &str) -> Tree {
            Tree::text(s)
        }
        fn map(items: Vec<(&str, Tree)>) -> Tree {
            Tree::Map(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
        }
        fn document(tree: Tree) -> Document {
            tree.document()
        }
        fn module(declarations: Vec<(&str, Tree)>, includes: &[&str]) -> Document {
            let mut fields = vec![
                ("version", text("1.0")),
                (
                    "includes",
                    Tree::List(includes.iter().map(|s| text(s)).collect()),
                ),
            ];
            fields.extend(declarations);
            document(map(fields))
        }
        fn descriptor(kind: &str) -> Tree {
            map(vec![("type", text(kind))])
        }
        fn fixtures() -> (Vec<Source>, Decoder) {
            let shared = module(
                vec![(
                    "shared",
                    Tree::List(vec![
                        map(vec![(
                            "id",
                            map(vec![
                                ("type", text("str")),
                                ("required", Tree::boolean(true)),
                            ]),
                        )]),
                        map(vec![(
                            "label",
                            map(vec![("type", text("str")), ("default", text("untitled"))]),
                        )]),
                    ]),
                )],
                &[],
            );
            let root = module(
                vec![(
                    "root_class",
                    Tree::List(vec![
                        map(vec![("fields_from", text("shared"))]),
                        map(vec![("yamaa", descriptor("str"))]),
                    ]),
                )],
                &["schema_s.yaml"],
            );
            (
                vec![source("schema_s.yaml", vec![10, 11]), source("r", vec![12])],
                Decoder::new(vec![shared, root]),
            )
        }
        fn source(identity: &str, bytes: Vec<u8>) -> Source {
            Source {
                identity: identity.into(),
                bytes,
            }
        }
        struct Payload {
            allocation: Box<u32>,
            _native: Rc<Cell<usize>>,
        }
        struct Locations {
            positions: Vec<(usize, usize)>,
            _native: Rc<Cell<usize>>,
        }
        type Trace = Rc<RefCell<Vec<(String, Vec<u8>)>>>;
        struct Decoder {
            documents: VecDeque<Document>,
            locations: VecDeque<Locations>,
            trace: Trace,
            fail_at: Option<usize>,
            failure: Option<Payload>,
        }
        impl Decoder {
            fn new(documents: Vec<Document>) -> Self {
                let locations = (0..documents.len())
                    .map(|i| Locations {
                        positions: vec![(i + 1, 7)],
                        _native: Rc::new(Cell::new(i)),
                    })
                    .collect();
                Self {
                    documents: documents.into(),
                    locations,
                    trace: Rc::new(RefCell::new(vec![])),
                    fail_at: None,
                    failure: None,
                }
            }
        }
        impl SchemaDecodePort for Decoder {
            type Error = Payload;
            type Locations = Locations;
            fn decode(
                &mut self,
                identity: &str,
                bytes: &[u8],
            ) -> Result<DecodedModule<Locations>, Payload> {
                self.trace
                    .borrow_mut()
                    .push((identity.into(), bytes.to_vec()));
                if self.fail_at == Some(self.trace.borrow().len()) {
                    return Err(self.failure.take().unwrap());
                }
                Ok(DecodedModule {
                    document: self.documents.pop_front().unwrap(),
                    locations: self.locations.pop_front().unwrap(),
                })
            }
        }
        fn accepted(
            result: Result<CapturedClosure<Locations>, Error<Payload>>,
        ) -> CapturedClosure<Locations> {
            match result {
                Ok(result) => result,
                Err(Error::Bundle(e)) => panic!("unexpected core bundle failure: {e:?}"),
                Err(_) => panic!("unexpected capture refusal"),
            }
        }
        #[test]
        fn complete_source_decode_order_allocations_descriptor_defaults_and_origins_survive() {
            for _ in 0..2 {
                let (sources, mut decoder) = fixtures();
                let source_arena = sources.as_ptr();
                let identities = sources
                    .iter()
                    .map(|s| s.identity.as_ptr())
                    .collect::<Vec<_>>();
                let bytes = sources.iter().map(|s| s.bytes.as_ptr()).collect::<Vec<_>>();
                let arenas = decoder
                    .documents
                    .iter()
                    .map(|d| d.nodes().as_ptr())
                    .collect::<Vec<_>>();
                let position_pointers = decoder
                    .locations
                    .iter()
                    .map(|l| l.positions.as_ptr())
                    .collect::<Vec<_>>();
                let captured = accepted(schema_capture::admit(
                    sources,
                    1,
                    RootSelection::Default,
                    &mut decoder,
                ));
                assert_eq!(
                    *decoder.trace.borrow(),
                    [
                        ("schema_s.yaml".into(), vec![10, 11]),
                        ("r".into(), vec![12])
                    ]
                );
                assert_eq!(captured.sources().as_ptr(), source_arena);
                for i in 0..2 {
                    assert_eq!(captured.sources()[i].identity.as_ptr(), identities[i]);
                    assert_eq!(captured.sources()[i].bytes.as_ptr(), bytes[i]);
                    assert_eq!(
                        captured.structure().modules()[i].document.nodes().as_ptr(),
                        arenas[i]
                    );
                    assert_eq!(
                        captured.locations()[i].positions.as_ptr(),
                        position_pointers[i]
                    );
                    assert_eq!(captured.locations()[i].positions, [(i + 1, 7)]);
                }
                let schema = captured.structure();
                assert_eq!(schema.version(), "1.0");
                assert_eq!(
                    schema
                        .root_class()
                        .fields
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>(),
                    ["id", "label", "yamaa"]
                );
                let label = schema.root_class().fields[1].descriptor;
                let origin = &schema.descriptors()[label];
                assert_eq!(
                    (origin.module, origin.path.as_str()),
                    (0, "root_class.label")
                );
                let default = origin.descriptor.default_node().unwrap();
                assert_eq!(
                    schema.modules()[0].document.nodes()[default],
                    N::Text("untitled".into())
                );
                // A complete 1.x document, normalized once its header is read.
                let input = document(map(vec![("yamaa", text("1.0")), ("id", text("ID01"))]));
                let normalized = schema.normalize_root(&input).unwrap();
                let value = normalized
                    .document
                    .field(normalized.document.root(), "label")
                    .unwrap();
                assert_eq!(
                    normalized.document.nodes()[value],
                    N::Text("untitled".into())
                );
                assert_eq!(
                    normalized.origins[value],
                    SchemaOrigin {
                        source: SchemaSource::Default {
                            module: 0,
                            descriptor: label
                        },
                        node: default,
                        generated: false
                    }
                );
                let (sources, locations, structure) = captured.into_parts();
                assert_eq!(sources.as_ptr(), source_arena);
                assert_eq!(locations[0].positions.as_ptr(), position_pointers[0]);
                assert_eq!(structure.modules()[0].document.nodes().as_ptr(), arenas[0]);
            }
        }

        #[test]
        fn late_original_opaque_codec_failure_precedes_bundle_entry_or_duplicate_defects() {
            for entry in [0, usize::MAX] {
                let mut decoder = Decoder::new(vec![
                    document(Tree::text("invalid schema")),
                    module(vec![], &[]),
                    module(vec![], &[]),
                ]);
                let allocation = Box::new(72);
                let pointer = (&*allocation) as *const u32;
                decoder.fail_at = Some(2);
                decoder.failure = Some(Payload {
                    allocation,
                    _native: Rc::new(Cell::new(0)),
                });
                let sources = vec![
                    source("schema_s.yaml", vec![1]),
                    source("schema_s.yaml", vec![2]),
                    source("late", vec![3]),
                ];
                let result =
                    schema_capture::admit(sources, entry, RootSelection::Default, &mut decoder);
                let Err(Error::Decode { identity, error }) = result else {
                    panic!("original second codec error expected")
                };
                assert_eq!(identity, "schema_s.yaml");
                assert_eq!((&*error.allocation) as *const u32, pointer);
                assert_eq!(
                    *decoder.trace.borrow(),
                    [
                        ("schema_s.yaml".into(), vec![1]),
                        ("schema_s.yaml".into(), vec![2])
                    ]
                );
                assert_eq!(decoder.documents.len(), 2);
            }
        }
        #[test]
        fn all_codecs_complete_before_exact_ordered_bundle_reports_and_entry_errors() {
            let root = module(
                vec![(
                    "root_class",
                    Tree::List(vec![
                        map(vec![("first", descriptor("unknown_one"))]),
                        map(vec![("second", descriptor("unknown_two"))]),
                    ]),
                )],
                &[],
            );
            let mut decoder = Decoder::new(vec![root]);
            let result = schema_capture::admit(
                vec![source("r", vec![1])],
                0,
                RootSelection::Default,
                &mut decoder,
            );
            let Err(Error::Bundle(error)) = result else {
                panic!("full bundle issues expected")
            };
            assert_eq!(
                error,
                BundleError::Invalid(vec![
                    BundleIssue {
                        module: 0,
                        path: "root_class.first".into(),
                        kind: BundleIssueKind::UnknownType("unknown_one".into())
                    },
                    BundleIssue {
                        module: 0,
                        path: "root_class.second".into(),
                        kind: BundleIssueKind::UnknownType("unknown_two".into())
                    },
                ])
            );
            assert_eq!(*decoder.trace.borrow(), [("r".into(), vec![1])]);
            for entry in [0, usize::MAX] {
                let (mut sources, mut decoder) = fixtures();
                sources[1].identity = "schema_s.yaml".into();
                let result =
                    schema_capture::admit(sources, entry, RootSelection::Default, &mut decoder);
                let Err(Error::Bundle(error)) = result else {
                    panic!("bundle defect expected after both codecs")
                };
                let issue = if entry == 0 {
                    BundleIssue {
                        module: 1,
                        path: "$".into(),
                        kind: BundleIssueKind::DuplicateModule("schema_s.yaml".into()),
                    }
                } else {
                    BundleIssue {
                        module: usize::MAX,
                        path: "$".into(),
                        kind: BundleIssueKind::ModuleName,
                    }
                };
                assert_eq!(error, BundleError::Invalid(vec![issue]));
                assert_eq!(decoder.trace.borrow().len(), 2);
            }
        }
        #[test]
        fn invalid_defaults_remain_a_later_normalization_gate() {
            let schema = module(
                vec![(
                    "root_class",
                    Tree::List(vec![map(vec![(
                        "value",
                        map(vec![
                            ("type", text("int")),
                            ("default", text("invalid integer")),
                        ]),
                    )])]),
                )],
                &[],
            );
            let mut decoder = Decoder::new(vec![schema]);
            let captured = accepted(schema_capture::admit(
                vec![source("r", vec![1])],
                0,
                RootSelection::Default,
                &mut decoder,
            ));
            let input = document(map(vec![]));
            assert!(matches!(
                captured.structure().normalize_root(&input),
                Err(NormalizationError::Defaults(_))
            ));
            assert_eq!(decoder.trace.borrow().len(), 1);
        }
        #[test]
        fn large_closed_schema_keeps_every_original_source_and_decode() {
            {
                let names = (0..200)
                    .map(|i| format!("schema_m{i}.yaml"))
                    .collect::<Vec<_>>();
                let mut sources = names
                    .iter()
                    .map(|name| source(name, vec![1]))
                    .collect::<Vec<_>>();
                sources.push(source("root", vec![2]));
                let mut documents = names
                    .iter()
                    .map(|_| module(vec![], &[]))
                    .collect::<Vec<_>>();
                documents.push(module(
                    vec![(
                        "root_class",
                        Tree::List(vec![map(vec![("value", descriptor("str"))])]),
                    )],
                    &names.iter().map(String::as_str).collect::<Vec<_>>(),
                ));
                let mut decoder = Decoder::new(documents);
                let result =
                    schema_capture::admit(sources, 200, RootSelection::Default, &mut decoder);
                let captured = accepted(result);
                assert_eq!(captured.sources().len(), 201);
                assert_eq!(captured.structure().modules().len(), 201);
                assert_eq!(decoder.trace.borrow().len(), 201);
            }
        }
    }
    mod source_profile_inspection {
        use yamaa_core::source_profile_inspection;
        use yamaa_core::{
            resource::ResourceFailure,
            specification::{PreflightFinding, PrepareError, UnsupportedFeature},
        };

        fn unknown() -> PrepareError {
            PrepareError::Invalid(vec![PreflightFinding::UnknownSourceProfile {
                dataset: "SRC".into(),
                path: "input.unknown".into(),
            }])
        }

        #[test]
        fn physical_kind_refines_unknown_profile_without_byte_authority() {
            for (failure, condition) in [
                (
                    ResourceFailure::NotRegularFile,
                    "resource_path_not_regular_file",
                ),
                (ResourceFailure::Symlink, "resource_path_symlink"),
                (ResourceFailure::Missing, "source_profile_unknown"),
            ] {
                let mut inspected = Vec::new();
                let error = source_profile_inspection::refine(unknown(), |dataset, path| {
                    inspected.push((dataset.to_owned(), path.to_owned()));
                    Some((failure, "../input.unknown".into()))
                });
                assert_eq!(inspected, [("SRC".into(), "input.unknown".into())]);
                let PrepareError::Invalid(findings) = error else {
                    panic!("invalid findings")
                };
                let issue = findings[0].diagnostic();
                assert_eq!(issue.definition().condition, condition);
                assert_eq!(issue.spec_paths, ["input.SRC.path"]);
            }
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(unknown(), |_, _| None)
            else {
                panic!("invalid findings")
            };
            assert_eq!(
                findings[0].diagnostic().definition().condition,
                "source_profile_unknown"
            );
        }

        #[test]
        fn unsupported_and_other_static_findings_do_not_enter_inspection() {
            let unsupported = PrepareError::Unsupported(vec![UnsupportedFeature {
                operation: "future_operation".into(),
                path: "columns.X".into(),
            }]);
            assert!(matches!(
                source_profile_inspection::refine(unsupported, |_, _| panic!(
                    "unsupported entered study metadata"
                )),
                PrepareError::Unsupported(_)
            ));
            let original = PreflightFinding::UndeclaredKey {
                position: 0,
                column: "ID".into(),
            };
            let expected = original.diagnostic();
            let PrepareError::Invalid(findings) =
                source_profile_inspection::refine(PrepareError::Invalid(vec![original]), |_, _| {
                    panic!("unrelated finding entered study metadata")
                })
            else {
                panic!("invalid findings")
            };
            assert_eq!(findings[0].diagnostic(), expected);
        }
    }
    mod standalone_preparation {
        //! Independent logical documents exercise the complete standalone application.
        //!
        //! Every test here is 0.5 only (#3447): `standalone_preparation::prepare`
        //! prepares a 0.5 entry without a `parent`, through the 0.5
        //! normalization and the 0.5 model contract. A 1.x specification is
        //! a chain of layers, resolved by `prepare_versioned` (LAYER-1); its
        //! window expansion is `schema_windows`', and its defaults
        //! `inheritance_capture::complete_effects_defaults_and_distinct_orders_are_engine_owned`.
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure, WindowReference,
        };
        use yamaa_core::standalone_preparation::{prepare, Error};

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        #[derive(Debug, PartialEq)]
        enum Logical {
            Null,
            Int(String),
            Text(String),
            List(Vec<Logical>),
            Map(Vec<(String, Logical)>),
        }
        fn logical(document: &Document, node: usize) -> Logical {
            match &document.nodes()[node] {
                N::Null => Logical::Null,
                N::Integer(text) => Logical::Int(text.clone()),
                N::Text(text) => Logical::Text(text.clone()),
                N::Sequence(items) => {
                    Logical::List(items.iter().map(|&id| logical(document, id)).collect())
                }
                N::Mapping(items) => Logical::Map(
                    items
                        .iter()
                        .map(|&(k, v)| {
                            let N::Text(key) = &document.nodes()[k] else {
                                panic!("text fixture keys")
                            };
                            (key.clone(), logical(document, v))
                        })
                        .collect(),
                ),
                _ => panic!("fixture scalar"),
            }
        }
        fn field(name: &'static str, kind: &'static str) -> V {
            V::map(vec![(name, V::map(vec![("type", V::text(kind))]))])
        }
        fn schema(default_domain: bool, loose_domain: bool) -> SchemaStructure {
            let mut domain = vec![(
                "type",
                if loose_domain {
                    V::List(vec![V::text("str"), V::text("int")])
                } else {
                    V::text("str")
                },
            )];
            if default_domain {
                domain.push(("default", V::text("DEFAULT")));
            }
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "root",
                            V::List(vec![
                                field("schema_version", "str"),
                                V::map(vec![("domain", V::map(domain))]),
                                field("input", "dict"),
                                field("keys", "list[str]"),
                                field("output", "dict"),
                                field("columns", "list[column_class]"),
                                field("windows", "dict[str, window_spec]"),
                            ]),
                        ),
                        (
                            "column_class",
                            V::List(vec![
                                field("name", "str"),
                                field("type", "str"),
                                field("derivation", "handled_expression_class"),
                            ]),
                        ),
                        (
                            "handled_expression_class",
                            V::List(vec![field("value", "expression")]),
                        ),
                        (
                            "expression",
                            V::map(vec![("registry", V::text("expressions"))]),
                        ),
                        (
                            "expressions",
                            V::map(vec![(
                                "row_number",
                                V::List(vec![field("window", "window_selection")]),
                            )]),
                        ),
                        (
                            "window_selection",
                            V::map(vec![(
                                "type",
                                V::List(vec![V::text("str"), V::text("window_spec")]),
                            )]),
                        ),
                        (
                            "window_spec",
                            V::List(vec![
                                field("group_by", "list[str]"),
                                field("order_by", "list[order_term]"),
                            ]),
                        ),
                        (
                            "order_term",
                            V::List(vec![
                                field("variable", "str"),
                                field("direction", "str"),
                                field("nulls", "str"),
                            ]),
                        ),
                    ])),
                }],
                0,
                "root",
            )
            .unwrap()
        }
        fn basic_fields() -> Vec<(&'static str, V)> {
            vec![
                ("schema_version", V::text("1.0")),
                ("domain", V::text("X")),
                ("input", V::map(vec![])),
                ("keys", V::List(vec![])),
                (
                    "output",
                    V::map(vec![
                        ("path", V::text("output.csv")),
                        ("columns", V::List(vec![])),
                    ]),
                ),
                ("columns", V::List(vec![])),
            ]
        }

        fn column(name: &'static str, window: V) -> V {
            V::map(vec![
                ("name", V::text(name)),
                ("type", V::text("int")),
                (
                    "derivation",
                    V::map(vec![(
                        "value",
                        V::map(vec![("row_number", V::map(vec![("window", window)]))]),
                    )]),
                ),
            ])
        }
        fn window() -> V {
            V::map(vec![(
                "order_by",
                V::List(vec![V::map(vec![("variable", V::text("SEQ"))])]),
            )])
        }
        fn field_id(document: &Document, path: &[&str]) -> usize {
            path.iter().fold(document.root(), |id, name| {
                document.field(id, name).unwrap()
            })
        }
        fn item(document: &Document, sequence: usize, index: usize) -> usize {
            let N::Sequence(items) = &document.nodes()[sequence] else {
                panic!()
            };
            items[index]
        }
        fn window_id(document: &Document, index: usize) -> usize {
            ["derivation", "value", "row_number", "window"]
                .into_iter()
                .fold(
                    item(document, field_id(document, &["columns"]), index),
                    |id, name| document.field(id, name).unwrap(),
                )
        }
        fn unknown(name: &str, window: &str) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: format!("columns.{name}.derivation.row_number.window"),
                code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownWindow,
                context: vec![("window", C::Text(window.into()))],
            }
        }

        #[test]
        fn success_moves_raw_arena_and_retains_complete_default_and_window_provenance() {
            let mut fields = basic_fields();
            fields.remove(1);
            // Keys name both columns, so pruning (LAYER-11) keeps them live.
            fields[2].1 = V::List(vec![V::text("Z"), V::text("A")]);
            fields.last_mut().unwrap().1 = V::List(vec![
                column("Z", V::text("VISITS")),
                column("A", V::text("VISITS")),
            ]);
            fields.push(("windows", V::map(vec![("VISITS", window())])));
            let raw = document(V::map(fields));
            let raw_pointer = raw.nodes().as_ptr();
            let raw_expected = raw.clone();
            let schema = schema(true, false);
            let prepared = prepare(&schema, raw).unwrap();
            assert_eq!(prepared.raw().nodes().as_ptr(), raw_pointer);
            assert_eq!(prepared.raw(), &raw_expected);
            let mut expected = basic_fields();
            expected[1].1 = V::text("DEFAULT");
            expected[3].1 = V::List(vec![V::text("Z"), V::text("A")]);
            expected.last_mut().unwrap().1 =
                V::List(vec![column("Z", window()), column("A", window())]);
            let model = prepared.model().document();
            let expected = document(V::map(expected));
            assert_eq!(
                logical(model, model.root()),
                logical(&expected, expected.root())
            );
            assert_eq!(
                prepared.windows(),
                &[
                    WindowReference {
                        path: "columns.Z.derivation.value.row_number.window".into(),
                        definition: "VISITS".into()
                    },
                    WindowReference {
                        path: "columns.A.derivation.value.row_number.window".into(),
                        definition: "VISITS".into()
                    },
                ]
            );
            assert_eq!(prepared.origins().len(), model.nodes().len());
            let descriptor = schema
                .class_named("root")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "domain")
                .unwrap()
                .descriptor;
            let located = &schema.descriptors()[descriptor];
            let default = located.descriptor.default_node().unwrap();
            let default_origin = SchemaOrigin {
                source: SchemaSource::Default {
                    module: 0,
                    descriptor,
                },
                node: default,
                generated: false,
            };
            let domain = model.field(model.root(), "domain").unwrap();
            assert_eq!(prepared.origins()[domain], default_origin);
            let N::Mapping(entries) = &model.nodes()[model.root()] else {
                panic!()
            };
            let domain_key = entries.iter().find(|(_, v)| *v == domain).unwrap().0;
            assert_eq!(
                prepared.origins()[domain_key],
                SchemaOrigin {
                    generated: true,
                    ..default_origin
                }
            );
            assert_eq!(
                prepared.origins()[model.root()],
                SchemaOrigin {
                    source: SchemaSource::Input,
                    node: raw_expected.root(),
                    generated: false
                }
            );
            for index in 0..2 {
                let selected = window_id(model, index);
                let original = window_id(&raw_expected, index);
                assert_eq!(
                    prepared.origins()[selected],
                    SchemaOrigin {
                        source: SchemaSource::Input,
                        node: original,
                        generated: false
                    }
                );
                let copied = model.field(selected, "order_by").unwrap();
                let definition = field_id(&raw_expected, &["windows", "VISITS", "order_by"]);
                assert_eq!(
                    prepared.origins()[copied],
                    SchemaOrigin {
                        source: SchemaSource::Input,
                        node: definition,
                        generated: false
                    }
                );
            }
            // Independent complete provenance oracle: copy authored occurrences, replace
            // only the named selection's children from its definition, and materialize
            // the sole default key/value from its descriptor's captured schema arena.
            fn copied(
                model: &Document,
                id: usize,
                raw: &Document,
                input: usize,
                expected: &mut [Option<SchemaOrigin>],
            ) {
                expected[id] = Some(SchemaOrigin {
                    source: SchemaSource::Input,
                    node: input,
                    generated: false,
                });
                match (&model.nodes()[id], &raw.nodes()[input]) {
                    (N::Sequence(actual), N::Sequence(original)) => {
                        assert_eq!(actual.len(), original.len());
                        for (&actual, &original) in actual.iter().zip(original) {
                            copied(model, actual, raw, original, expected);
                        }
                    }
                    (N::Mapping(actual), N::Mapping(original)) => {
                        for &(key, value) in actual {
                            let &(original_key, original_value) = original
                                .iter()
                                .find(|&&(k, _)| raw.nodes()[k] == model.nodes()[key])
                                .unwrap();
                            copied(model, key, raw, original_key, expected);
                            copied(model, value, raw, original_value, expected);
                        }
                    }
                    (N::Mapping(_), N::Text(name)) if name == "VISITS" => {
                        let definition = field_id(raw, &["windows", "VISITS"]);
                        copied(model, id, raw, definition, expected);
                        expected[id] = Some(SchemaOrigin {
                            source: SchemaSource::Input,
                            node: input,
                            generated: false,
                        });
                    }
                    (actual, original) => assert_eq!(actual, original),
                }
            }
            let mut expected_origins = vec![None; model.nodes().len()];
            expected_origins[model.root()] = Some(SchemaOrigin {
                source: SchemaSource::Input,
                node: raw_expected.root(),
                generated: false,
            });
            let N::Mapping(raw_fields) = &raw_expected.nodes()[raw_expected.root()] else {
                panic!()
            };
            for &(key, value) in entries {
                if value == domain {
                    expected_origins[value] = Some(default_origin);
                    expected_origins[key] = Some(SchemaOrigin {
                        generated: true,
                        ..default_origin
                    });
                } else {
                    let &(raw_key, raw_value) = raw_fields
                        .iter()
                        .find(|&&(k, _)| raw_expected.nodes()[k] == model.nodes()[key])
                        .unwrap();
                    copied(model, key, &raw_expected, raw_key, &mut expected_origins);
                    copied(
                        model,
                        value,
                        &raw_expected,
                        raw_value,
                        &mut expected_origins,
                    );
                }
            }
            assert_eq!(
                prepared.origins(),
                expected_origins
                    .into_iter()
                    .map(Option::unwrap)
                    .collect::<Vec<_>>()
            );
            let (raw, model, origins, windows) = prepared.into_parts();
            assert_eq!(raw.nodes().as_ptr(), raw_pointer);
            assert_eq!(origins.len(), model.document().nodes().len());
            assert_eq!(windows.len(), 2);
        }

        #[test]
        fn parent_presence_selects_inheritance_before_standalone_semantic_checks() {
            let schema = schema(false, false);
            for parent in [V::List(vec![]), V::text("parent.yaml")] {
                let raw = document(V::map(vec![
                    ("schema_version", V::text("99.0")),
                    ("parent", parent),
                    ("domain", V::integer("8")),
                ]));

                assert!(matches!(
                    prepare(&schema, raw),
                    Err(Error::InheritanceRequired)
                ));
            }
            // A null parent is absent: the entry takes the standalone route,
            // whose version gate refuses it.
            let raw = document(V::map(vec![
                ("schema_version", V::text("99.0")),
                ("parent", V::Scalar(N::Null)),
                ("domain", V::integer("8")),
            ]));
            let actual_node = field_id(&raw, &["schema_version"]);
            let Err(Error::Normalize(failure)) = prepare(&schema, raw) else {
                panic!("a null parent takes the standalone route")
            };
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![SchemaDiagnostic {
                    path: "schema_version".into(),
                    code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                    context: vec![
                        ("expected", C::Text("1.0".into())),
                        ("actual", C::InputValue(actual_node))
                    ]
                }])
            );
        }

        #[test]
        fn normalization_findings_keep_original_raw_allocation_and_full_version_precedence() {
            let raw = document(V::map(vec![
                (
                    "schema_version",
                    V::integer("92233720368547758081234567890"),
                ),
                ("domain", V::integer("8")),
                ("windows", V::map(vec![("BAD", V::text("broken"))])),
            ]));
            let pointer = raw.nodes().as_ptr();
            let expected = raw.clone();
            let actual_node = field_id(&raw, &["schema_version"]);
            let Err(Error::Normalize(failure)) = prepare(&schema(false, false), raw) else {
                panic!()
            };
            let held = failure.input.unwrap();
            assert_eq!(held.nodes().as_ptr(), pointer);
            assert_eq!(held, expected);
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![SchemaDiagnostic {
                    path: "schema_version".into(),
                    code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                    context: vec![
                        ("expected", C::Text("1.0".into())),
                        ("actual", C::InputValue(actual_node))
                    ]
                }])
            );
        }

        #[test]
        fn strict_windows_preserve_complete_order_and_precede_model_findings() {
            let mut fields = basic_fields();
            fields[4].1 = V::map(vec![]);
            // Keys name both columns, so pruning (LAYER-11) keeps them live
            // and strict window expansion still reports their unknown windows.
            fields[3].1 = V::List(vec![V::text("Z"), V::text("A")]);
            fields.last_mut().unwrap().1 = V::List(vec![
                column("Z", V::text("SECOND")),
                column("A", V::text("FIRST")),
            ]);
            let Err(Error::Normalize(failure)) =
                prepare(&schema(false, false), document(V::map(fields)))
            else {
                panic!()
            };
            assert_eq!(
                failure.error,
                NormalizationError::Invalid(vec![unknown("Z", "SECOND"), unknown("A", "FIRST")])
            );
            assert!(failure.input.is_some());
        }

        #[test]
        fn model_findings_remain_complete_and_have_no_document_value_references() {
            let mut fields = basic_fields();
            fields[1].1 = V::integer("7");
            fields[4].1 = V::map(vec![]);
            let Err(Error::Model(findings)) =
                prepare(&schema(false, true), document(V::map(fields)))
            else {
                panic!()
            };
            let finding = |path: &str, reason: &str| SchemaDiagnostic {
                path: path.into(),
                code: if reason == "Field required" {
                    yamaa_core::diagnostic::ConditionCode::SchemaModelMissingField
                } else {
                    yamaa_core::diagnostic::ConditionCode::SchemaModelInvalidValue
                },
                context: vec![("reason", C::Text(reason.into()))],
            };
            assert_eq!(
                findings,
                vec![
                    finding("domain", "Input should be a valid string"),
                    finding("output.path", "Field required"),
                    finding("output.columns", "Field required")
                ]
            );
        }
    }
    mod structural_preparation {
        //! Independent logical truth and physical codec effects for live metadata preparation.
        //!
        //! Every test here is 0.5 only (#3447): `structural_preparation::prepare`
        //! admits the files the 0.5 closures read (a 0.5 project, environment
        //! file, definition file or codelist source). A 1.x file is admitted
        //! against its type's closure once its header is read (SCHEMA-2).
        use std::rc::Rc;
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            project_preparation::Source,
            structural_preparation::{
                self as application, Decoded, Error, Selection, StructuralDecodePort,
            },
        };

        use crate::support::document::Tree as V;

        fn document(value: V) -> Document {
            value.document()
        }
        #[derive(Debug, PartialEq)]
        enum Logical {
            Text(String),
            Int(String),
            Bool(bool),
            List(Vec<Logical>),
            Map(Vec<(String, Logical)>),
        }
        fn logical(d: &Document, id: usize) -> Logical {
            match &d.nodes()[id] {
                N::Text(v) => Logical::Text(v.clone()),
                N::Integer(v) => Logical::Int(v.clone()),
                N::Boolean(v) => Logical::Bool(*v),
                N::Sequence(items) => {
                    Logical::List(items.iter().map(|&id| logical(d, id)).collect())
                }
                N::Mapping(items) => Logical::Map(
                    items
                        .iter()
                        .map(|&(k, v)| {
                            let N::Text(k) = &d.nodes()[k] else { panic!() };
                            (k.clone(), logical(d, v))
                        })
                        .collect(),
                ),
                _ => panic!(),
            }
        }
        fn field(name: &'static str, kind: &'static str, required: bool) -> V {
            V::map(vec![(
                name,
                V::map(vec![
                    ("type", V::text(kind)),
                    ("required", V::boolean(required)),
                ]),
            )])
        }
        fn schema(bad_default: bool) -> SchemaStructure {
            SchemaStructure::admit(
                vec![SchemaModule {
                    name: "schema.yaml".into(),
                    document: document(V::map(vec![
                        ("version", V::text("1.0")),
                        (
                            "environment_class",
                            V::List(vec![
                                field("schema_version", "str", true),
                                field("lock", "str", false),
                            ]),
                        ),
                        (
                            "function_definition_class",
                            V::List(vec![
                                field("function", "str", true),
                                field("returns", "str", true),
                                V::map(vec![(
                                    "flag",
                                    V::map(vec![
                                        ("type", V::text("bool")),
                                        (
                                            "default",
                                            if bad_default {
                                                V::integer("7")
                                            } else {
                                                V::boolean(false)
                                            },
                                        ),
                                    ]),
                                )]),
                            ]),
                        ),
                        (
                            "versioned_other",
                            V::List(vec![field("schema_version", "str", false)]),
                        ),
                        (
                            "function_alias",
                            V::map(vec![("type", V::text("function_definition_class"))]),
                        ),
                    ])),
                }],
                0,
                "environment_class",
            )
            .unwrap()
        }
        fn source() -> Source {
            Source {
                identity: "held/function.yaml".into(),
                bytes: b"original captured representation".to_vec(),
            }
        }
        fn function() -> Document {
            document(V::map(vec![
                ("function", V::text("program.constant")),
                ("returns", V::text("int")),
            ]))
        }
        // Intentionally no Clone/Debug/Send/Sync: the actual allocation must move through
        // the new application rather than be interpreted, copied or formatted.
        struct Opaque {
            value: Box<str>,
            _thread: Rc<()>,
        }
        struct Codec {
            reply: Option<Result<Decoded<Vec<usize>>, Opaque>>,
            calls: Vec<(usize, usize)>,
        }
        impl Codec {
            fn success(document: Document) -> Self {
                Self {
                    reply: Some(Ok(Decoded {
                        locations: (0..document.nodes().len()).collect(),
                        document,
                    })),
                    calls: vec![],
                }
            }
            fn raised(value: Opaque) -> Self {
                Self {
                    reply: Some(Err(value)),
                    calls: vec![],
                }
            }
        }
        impl StructuralDecodePort for Codec {
            type Error = Opaque;
            type Locations = Vec<usize>;
            fn decode(&mut self, source: &Source) -> Result<Decoded<Vec<usize>>, Opaque> {
                self.calls.push((
                    source.bytes.as_ptr() as usize,
                    source.identity.as_ptr() as usize,
                ));
                self.reply.take().expect("codec must execute once")
            }
        }
        fn unwrap<T>(value: Result<T, Error<Opaque>>) -> T {
            value.unwrap_or_else(|_| panic!("unexpected preparation failure"))
        }
        fn unknown(path: &str, name: &str, class: &str) -> SchemaDiagnostic {
            SchemaDiagnostic {
                path: path.into(),
                code: yamaa_core::diagnostic::ConditionCode::SchemaUnknownField,
                context: vec![
                    ("field", C::Text(name.into())),
                    ("class", C::Text(class.into())),
                ],
            }
        }

        #[test]
        fn named_authority_guards_precede_decode_and_defaults() {
            let schema = schema(true);

            for name in ["missing", "str", "function_alias", ""] {
                let mut codec = Codec::success(function());
                let error = application::prepare(
                    &schema,
                    source(),
                    Selection::NamedClass(name),
                    &mut codec,
                );
                assert!(matches!(error,Err(Error::UnknownClass(actual)) if actual==name));
                assert!(codec.calls.is_empty());
                assert!(codec.reply.is_some());
            }
            for name in ["environment_class", "versioned_other"] {
                let mut codec = Codec::success(function());
                assert!(matches!(
                    application::prepare(
                        &schema,
                        source(),
                        Selection::NamedClass(name),
                        &mut codec
                    ),
                    Err(Error::VersionedRootRequired)
                ));
                assert!(codec.calls.is_empty());
            }
        }

        #[test]
        fn physical_codec_runs_once_and_original_opaque_error_precedes_query_and_defaults() {
            let schema = schema(true);
            for selection in [
                Selection::StructuralRoot,
                Selection::NamedClass("function_definition_class"),
            ] {
                let value = Opaque {
                    value: "original opaque codec failure".into(),
                    _thread: Rc::new(()),
                };
                let pointer = value.value.as_ptr();
                let entry = source();
                let bytes = entry.bytes.as_ptr() as usize;
                let identity = entry.identity.as_ptr() as usize;
                let mut codec = Codec::raised(value);

                let Err(Error::Decode(original)) =
                    application::prepare(&schema, entry, selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(original.value.as_ptr(), pointer);
                assert_eq!(&*original.value, "original opaque codec failure");
                assert_eq!(codec.calls, [(bytes, identity)]);
                assert!(codec.reply.is_none());
            }
        }

        #[test]
        fn success_moves_every_original_allocation_and_complete_default_origin_vector() {
            let schema = schema(false);
            let entry = source();
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let raw = function();
            let expected_raw = raw.clone();
            let raw_pointer = raw.nodes().as_ptr();
            let mut codec = Codec::success(raw);
            let location_pointer = codec
                .reply
                .as_ref()
                .unwrap()
                .as_ref()
                .ok()
                .unwrap()
                .locations
                .as_ptr();
            let result = unwrap(application::prepare(
                &schema,
                entry,
                Selection::NamedClass("function_definition_class"),
                &mut codec,
            ));
            assert_eq!(result.source().bytes.as_ptr(), bytes);
            assert_eq!(result.source().identity.as_ptr(), identity);
            assert_eq!(result.raw().nodes().as_ptr(), raw_pointer);
            assert_eq!(result.locations().as_ptr(), location_pointer);
            assert_eq!(result.raw(), &expected_raw);
            assert_eq!(result.class_name(), "function_definition_class");
            assert_eq!(codec.calls.len(), 1);
            let expected = document(V::map(vec![
                ("function", V::text("program.constant")),
                ("returns", V::text("int")),
                ("flag", V::boolean(false)),
            ]));
            let normalized = &result.normalized().document;
            assert_eq!(
                logical(normalized, normalized.root()),
                logical(&expected, expected.root())
            );
            assert!(normalized
                .field(normalized.root(), "schema_version")
                .is_none());
            let descriptor = schema
                .class_named("function_definition_class")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "flag")
                .unwrap()
                .descriptor;
            let default = schema.descriptors()[descriptor]
                .descriptor
                .default_node()
                .unwrap();
            let origin = SchemaOrigin {
                source: SchemaSource::Default {
                    module: 0,
                    descriptor,
                },
                node: default,
                generated: false,
            };
            // Complete independently declared origin truth, aligned to independently
            // checked output nodes. Authored pair IDs are 0..3 and root ID is 4.
            let input = |node| SchemaOrigin {
                source: SchemaSource::Input,
                node,
                generated: false,
            };
            assert_eq!(
                result.normalized().origins,
                [
                    input(0),
                    input(1),
                    input(2),
                    input(3),
                    origin,
                    SchemaOrigin {
                        generated: true,
                        ..origin
                    },
                    input(4)
                ]
            );
            let (source, raw, locations, class_name, normalized) = result.into_parts();
            assert_eq!(source.bytes.as_ptr(), bytes);
            assert_eq!(source.identity.as_ptr(), identity);
            assert_eq!(raw.nodes().as_ptr(), raw_pointer);
            assert_eq!(locations.as_ptr(), location_pointer);
            assert_eq!(class_name, "function_definition_class");
            assert_eq!(normalized.origins.len(), 7);
        }

        #[test]
        fn semantic_failures_move_raw_arena_and_full_ordered_findings_without_partial_output() {
            let schema = schema(false);
            for (selection, raw, expected) in [
                (
                    Selection::StructuralRoot,
                    document(V::map(vec![
                        (
                            "schema_version",
                            V::integer("92233720368547758081234567890"),
                        ),
                        ("extra", V::text("late")),
                    ])),
                    vec![SchemaDiagnostic {
                        path: "schema_version".into(),
                        code: yamaa_core::diagnostic::ConditionCode::SchemaVersionMismatch,
                        context: vec![
                            ("expected", C::Text("1.0".into())),
                            ("actual", C::InputValue(1)),
                        ],
                    }],
                ),
                (
                    Selection::NamedClass("function_definition_class"),
                    document(V::map(vec![
                        ("function", V::text("program.constant")),
                        ("returns", V::text("int")),
                        ("schema_version", V::text("1.0")),
                        ("second", V::text("extra")),
                    ])),
                    vec![
                        unknown(
                            "<normalization>.schema_version",
                            "schema_version",
                            "function_definition_class",
                        ),
                        unknown(
                            "<normalization>.second",
                            "second",
                            "function_definition_class",
                        ),
                    ],
                ),
            ] {
                let pointer = raw.nodes().as_ptr();
                let raw_expected = raw.clone();
                let entry = source();
                let source_pointer = entry.bytes.as_ptr();
                let identity = entry.identity.as_ptr();
                let mut codec = Codec::success(raw);
                let Err(Error::Findings(held)) =
                    application::prepare(&schema, entry, selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(held.document().nodes().as_ptr(), pointer);
                assert_eq!(held.document(), &raw_expected);
                assert_eq!(held.source().bytes.as_ptr(), source_pointer);
                assert_eq!(held.source().identity.as_ptr(), identity);
                assert_eq!(held.findings(), expected);
                assert_eq!(codec.calls.len(), 1);
                let (source, document, findings) = held.into_parts();
                assert_eq!(source.bytes.as_ptr(), source_pointer);
                assert_eq!(document.nodes().as_ptr(), pointer);
                assert_eq!(findings, expected);
            }
        }

        #[test]
        fn structural_root_has_no_named_name_guard_and_never_admits_a_model_or_window() {
            let schema = schema(false);
            let raw = document(V::map(vec![
                ("schema_version", V::text("1.0")),
                ("lock", V::text("uv.lock")),
            ]));
            let expected = raw.clone();
            let pointer = raw.nodes().as_ptr();
            let mut codec = Codec::success(raw);
            let result = unwrap(application::prepare(
                &schema,
                source(),
                Selection::StructuralRoot,
                &mut codec,
            ));
            assert_eq!(result.raw().nodes().as_ptr(), pointer);
            assert_eq!(result.class_name(), "environment_class");
            let normalized = &result.normalized().document;
            assert_eq!(
                logical(normalized, normalized.root()),
                logical(&expected, expected.root())
            );
            assert!(normalized.field(normalized.root(), "domain").is_none());
            assert_eq!(codec.calls.len(), 1);
            assert_eq!(
                result.normalized().origins,
                (0..expected.nodes().len())
                    .map(|node| SchemaOrigin {
                        source: SchemaSource::Input,
                        node,
                        generated: false
                    })
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn defaults_keep_their_post_decode_post_query_checkpoint_and_original_typed_payload() {
            let schema = schema(true);
            for selection in [
                Selection::StructuralRoot,
                Selection::NamedClass("function_definition_class"),
            ] {
                let raw = document(V::map(vec![("schema_version", V::text("99.0"))]));
                let mut codec = Codec::success(raw);
                let Err(Error::Normalize(NormalizationError::Defaults(defaults))) =
                    application::prepare(&schema, source(), selection, &mut codec)
                else {
                    panic!()
                };
                assert_eq!(defaults.len(), 1);
                assert_eq!(
                    defaults[0].diagnostics,
                    vec![SchemaDiagnostic {
                        path: "function_definition_class.flag.default".into(),
                        code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
                        context: vec![
                            ("expected", C::Text("bool".into())),
                            (
                                "actual",
                                C::ValueKind(yamaa_core::schema::DocumentKind::Integer)
                            )
                        ]
                    }]
                );
                assert_eq!(codec.calls.len(), 1);
            }
        }
    }
}
