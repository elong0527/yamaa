//! Preparation contracts: project preparation.
//!
//! The seam is `project_preparation::prepare` over the scripted metadata
//! capture port and decoder of `support::ports`: the order of its captures
//! and decodes, the original opaque payloads and allocations it keeps, and
//! the prepared origins, which no replay case or benchmark reports.
mod preparation {
    mod project_preparation {
        //! Independent application truth over native captures and normalized document fixtures.
        use crate::support::ports::*;
        use yamaa_core::project_preparation::*;
        use yamaa_core::{
            limits::Limits,
            project_environment::{Finding, LockKind},
            project_function::Language,
            project_function_document::Kind as ScalarKind,
            schema::{Document, DocumentNode as N},
            value::Value,
        };

        use crate::support::document::Tree;

        /// A function definition returning `result`, without the name and
        /// callables its function file writes ([`function_file`]).
        fn function(result: &str) -> Tree {
            Tree::map(vec![
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
            /// is a uv lock, unless `malformed` names it, and each function
            /// file implements only Python.
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
                        // The scripted decoder reads held bytes `uv` as a uv
                        // lock; any other held lock is in no lock syntax.
                        assert!(
                            malformed.is_none_or(|at| at == 0),
                            "only a lock is malformed"
                        );
                        let raw = if malformed == Some(index) {
                            b"no lock".as_slice()
                        } else {
                            raw
                        };
                        Ok(source(identity, raw))
                    };
                    self.port.steps.push_back(MetadataCaptureStep {
                        kind,
                        declaring_source: project.clone(),
                        written: written.into(),
                        outcome,
                    });
                    if kind != Kind::Lock && !failed {
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
                let lock_bytes = fixture.port.steps[0]
                    .outcome
                    .as_ref()
                    .unwrap()
                    .bytes
                    .clone();
                let log = fixture.log.clone();
                let prepared = fixture.run().unwrap();
                assert_eq!(effects(&log), full_effects());
                assert_eq!(prepared.root.source.bytes.as_ptr(), root_bytes);
                assert_eq!(prepared.root.document.nodes().as_ptr(), root_nodes);
                assert_eq!(
                    prepared.captures[0].document.document.nodes().as_ptr(),
                    function_nodes
                );
                // The lock's bytes are copied out of the resolver's snapshot.
                assert_eq!(prepared.lock.as_ref().unwrap().source.bytes, lock_bytes);
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
        fn a_lock_in_no_lock_syntax_collects_without_admitting_a_lock() {
            let mut fixture = Fixture::new();
            fixture.script(&[], None, None, Some(0));
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(rejected.sources.len(), 1);
            assert_eq!(rejected.sources[0].origin, Origin::Lock);
            assert!(matches!(rejected.sources[0].error, CaptureFailure::Lock));
            assert!(rejected.lock.is_none());
            assert!(rejected.admission.is_none());
            // The lock decodes no document, so every later file is read.
            assert_eq!(effects(&log), full_effects());
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
            assert!(prepared.environment.draft().functions.is_none());
            assert!(prepared.captures.is_empty());
            assert!(prepared.lock.is_none());
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
            fixture.port.steps[0].outcome = Ok(source(
                "/held/python/uv.lock",
                &vec![b' '; Limits::SPEC_BYTES + 1],
            ));
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
        /// `function(result)` as a function file: its own header and name,
        /// and the callable of each host it implements.
        fn function_file(name: &str, implementations: &[(&str, &str)], result: &str) -> Tree {
            let Tree::Map(fields) = function(result) else {
                unreachable!("a definition is a mapping")
            };
            let mut file = vec![
                ("yamaa".to_string(), Tree::text("1.0")),
                ("type".into(), Tree::text("function")),
                ("name".into(), Tree::text(name)),
            ];
            file.extend(fields);
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
                    // The scripted decoder reads held bytes `uv` and `renv` as
                    // locks of those tools.
                    outcome: Ok(source(
                        &held(lock),
                        match kind {
                            LockKind::Uv => b"uv",
                            LockKind::Renv => b"renv",
                        },
                    )),
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
                        outcome: Ok(captured.clone()),
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
                    outcome: Ok(terminology.clone()),
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
                assert_eq!(draft.language, Some(host));
                assert_eq!(draft.lock.as_ref().unwrap().written, lock);
                // A function file that names no callable for the host is
                // admitted; a call of it fails before any data is read.
                let execution = prepared.environment.into_execution();
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
                    outcome: Ok(captured.clone()),
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
                outcome: Ok(terminology.clone()),
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
            assert!(prepared.lock.is_none());
            assert_eq!(prepared.origins.functions, ["only_python", "both"]);
            let draft = prepared.environment.draft();
            assert_eq!(draft.language, None);
            assert!(draft.lock.is_none());
            let execution = prepared.environment.into_execution();
            assert_eq!(execution.host(), None);
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
                outcome: Ok(wide.clone()),
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
