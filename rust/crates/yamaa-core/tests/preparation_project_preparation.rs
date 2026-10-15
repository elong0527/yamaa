//! Preparation contracts.
#[allow(dead_code)]
mod support;
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
        fn environment(language: Tree, inline_result: &str) -> Tree {
            Tree::map(vec![
                ("language", language),
                ("lock", Tree::text("lockfile")),
                (
                    "functions",
                    Tree::map(vec![
                        ("first", Tree::text("first.yaml")),
                        ("inline", function(inline_result)),
                        ("last", Tree::text("last.yaml")),
                    ]),
                ),
                (
                    "codelists",
                    Tree::List(vec![
                        codelist("INLINE", Tree::text("F")),
                        Tree::text("ct.yaml"),
                    ]),
                ),
                ("study", Tree::Scalar(N::Null)),
                ("adam", Tree::Scalar(N::Null)),
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
                    source: source("/author/env.yaml", b"held root bytes"),
                    root: environment(Tree::text("python"), "7").document(),
                    port: ScriptedMetadata::new(log.clone()),
                    decoder: ScriptedMetadataDecoder::new(log.clone()),
                    log,
                }
            }
            fn script(
                &mut self,
                failing: &[usize],
                interrupt: Option<usize>,
                decoding: Option<usize>,
                malformed: Option<usize>,
            ) -> Vec<usize> {
                self.decoder.steps.push_back(MetadataDecodeStep {
                    source: self.source.clone(),
                    target: DecodeTarget::Environment,
                    outcome: Ok(self.root.clone()),
                });
                self.port.interrupt_if = |error| error.0.as_ref() == "interrupt";
                let mut payloads = vec![];
                for (index, (kind, written, identity, raw)) in [
                    (Kind::Lock, "lockfile", "/held/uv.lock", b"uv".as_slice()),
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
                        declaring_source: self.source.identity.clone(),
                        written: written.into(),
                        outcome,
                    });
                    if kind != Kind::Lock && !failed && malformed != Some(index) {
                        let target = if kind == Kind::Function {
                            DecodeTarget::Function
                        } else {
                            DecodeTarget::Codelist
                        };
                        let outcome = if decoding == Some(index) {
                            let error = Opaque(written.into());
                            payloads.push(error.0.as_ptr() as usize);
                            Err(error)
                        } else {
                            Ok(if kind == Kind::Function {
                                function("7")
                            } else {
                                codelist("SEX", Tree::text("F"))
                            }
                            .document())
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
                    source,
                    mut decoder,
                    mut port,
                    ..
                } = self;
                prepare(&mut decoder, source, Language::Python, &mut port)
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
                "decode Environment /author/env.yaml",
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
                assert_eq!(
                    prepared.environment.draft().functions.as_ref().unwrap()[0].tests[0].result,
                    Value::Int(7)
                );
                let cases = prepared.environment.draft().functions.as_ref().unwrap()[0]
                    .tests
                    .as_ptr();
                let execution = prepared.environment.into_execution();
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
            }
        }

        #[test]
        fn root_decoder_error_keeps_original_opaque_payload_before_dependencies() {
            let mut fixture = Fixture::new();
            let error = Opaque("root decoding".into());
            let address = error.0.as_ptr();
            fixture.decoder.steps.push_back(MetadataDecodeStep {
                source: fixture.source.clone(),
                target: DecodeTarget::Environment,
                outcome: Err(error),
            });
            let log = fixture.log.clone();
            let Err(Failure::Root(error)) = fixture.run() else {
                panic!()
            };
            assert_eq!(error.0.as_ptr(), address);
            assert_eq!(effects(&log), ["decode Environment /author/env.yaml"]);
        }

        #[test]
        fn root_scalar_failures_keep_the_exact_context_arena_and_precede_all_dependencies() {
            let mut fixture = Fixture::new();
            fixture.root = environment(Tree::boolean(true), "7").document();
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
            assert_eq!(findings.len(), 2);
            assert!(findings
                .iter()
                .all(|finding| finding.path == "language"
                    && finding.kind == ScalarKind::NormalizedShape));
            assert_eq!(findings[0].path, "language");
            assert_eq!(findings[0].kind, ScalarKind::NormalizedShape);
            assert_eq!(root.document.nodes()[findings[0].node], N::Boolean(true));
            assert_eq!(effects(&log), ["decode Environment /author/env.yaml"]);
        }

        #[test]
        fn invalid_inline_function_scalar_precedes_every_capture() {
            let mut fixture = Fixture::new();
            fixture.root = environment(Tree::text("python"), "9223372036854775808").document();
            fixture.script(&[], None, None, None);
            let log = fixture.log.clone();
            let Err(Failure::RootScalar { root, findings }) = fixture.run() else {
                panic!()
            };
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "functions.inline.tests[0].result");
            assert_eq!(findings[0].kind, ScalarKind::IntegerRange);
            assert_eq!(
                root.document.nodes()[findings[0].node],
                N::Integer("9223372036854775808".into())
            );
            assert_eq!(effects(&log), ["decode Environment /author/env.yaml"]);
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
                    "decode Environment /author/env.yaml",
                    "capture lockfile",
                    "capture first.yaml",
                    "capture last.yaml",
                    "capture ct.yaml"
                ]
            );
        }

        #[test]
        fn every_capture_interrupt_keeps_original_payload_and_aborts_the_later_authority() {
            for (index, expected_origin, written) in [
                (0, Origin::Lock, "lockfile"),
                (1, Origin::Function("first".into()), "first.yaml"),
                (2, Origin::Function("last".into()), "last.yaml"),
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
            for index in 1..4 {
                let mut fixture = Fixture::new();
                let address = fixture.script(&[], None, Some(index), None)[0];
                let log = fixture.log.clone();
                let Err(Failure::Rejected(rejected)) = fixture.run() else {
                    panic!()
                };
                assert_eq!(effects(&log), full_effects());
                assert_eq!(rejected.sources.len(), 1);
                let CaptureFailure::Structural(error) = &rejected.sources[0].error else {
                    panic!()
                };
                assert_eq!(error.0.as_ptr() as usize, address);
                assert_eq!(rejected.captures.len(), 2);
                assert!(rejected.admission.is_none());
            }
        }

        #[test]
        fn dependency_scalar_failures_retain_exact_document_and_qualify_later_sources() {
            for index in [1, 3] {
                let mut fixture = Fixture::new();
                fixture.script(&[], None, None, None);
                fixture.decoder.steps[index].outcome = Ok(if index == 1 {
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
                if index > 0 {
                    expected.remove([0, 3, 5, 7][index]);
                }
                assert_eq!(effects(&log), expected);
            }
        }

        #[test]
        fn language_and_lock_kind_findings_are_core_owned_after_capture_without_code_authority() {
            let mut fixture = Fixture::new();
            fixture.root = environment(Tree::text("r"), "7").document();
            fixture.script(&[], None, None, None);
            let log = fixture.log.clone();
            let Err(Failure::Rejected(rejected)) = fixture.run() else {
                panic!()
            };
            assert_eq!(effects(&log), full_effects());
            // The shared Python fixture's three callables are also invalid under R.
            assert_eq!(
                rejected.admission,
                Some(vec![
                    Finding::LanguageMismatch {
                        declared: Language::R,
                        host: Language::Python,
                    },
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
        fn empty_environment_has_only_root_decode_and_no_dependency_or_activation_authority() {
            let log = EventLog::default();
            let root = source("empty.yaml", b"{}");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Environment,
                outcome: Ok(Tree::map(vec![]).document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let prepared = prepare(&mut decoder, root, Language::Python, &mut port).unwrap();
            assert_eq!(effects(&log), ["decode Environment empty.yaml"]);
            assert!(prepared.environment.draft().functions.is_none());
            assert!(prepared.captures.is_empty());
            assert!(prepared.lock.is_none());
        }

        #[test]
        fn explicitly_empty_functions_keep_presence_and_original_independent_root_findings() {
            let log = EventLog::default();
            let root = source("empty.yaml", b"functions: {}");
            let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
            decoder.steps.push_back(MetadataDecodeStep {
                source: root.clone(),
                target: DecodeTarget::Environment,
                outcome: Ok(Tree::map(vec![("functions", Tree::map(vec![]))]).document()),
            });
            let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
            let Err(Failure::Rejected(rejected)) =
                prepare(&mut decoder, root, Language::Python, &mut port)
            else {
                panic!()
            };
            assert_eq!(effects(&log), ["decode Environment empty.yaml"]);
            assert_eq!(rejected.draft.functions, Some(vec![]));
            assert_eq!(rejected.admission, Some(vec![Finding::MissingLanguage]));
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
            let CaptureFailure::Limit(limit) = &rejected.sources[0].error else {
                panic!("the dependency failure must retain the fixed size boundary")
            };
            assert_eq!(limit.resource, "spec_bytes");
            assert_eq!(limit.limit, Limits::SPEC_BYTES);
            assert_eq!(limit.actual, Some(Limits::SPEC_BYTES + 1));
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
                self, CapturedClosure, DecodePort, DecodedModule, Error, RootSelection,
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
                        map(vec![("schema_version", descriptor("str"))]),
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
        impl DecodePort for Decoder {
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
                    ["id", "label", "schema_version"]
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
                let input = document(map(vec![
                    ("schema_version", text("1.0")),
                    ("id", text("ID01")),
                ]));
                let normalized = schema.normalize_document(&input).unwrap();
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
                captured.structure().normalize_document(&input),
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
        fn parents_presence_selects_inheritance_before_standalone_semantic_checks() {
            let schema = schema(false, false);
            for parents in [
                V::Scalar(N::Null),
                V::List(vec![]),
                V::List(vec![V::text("parent.yaml")]),
            ] {
                let raw = document(V::map(vec![
                    ("schema_version", V::text("99.0")),
                    ("parents", parents),
                    ("domain", V::integer("8")),
                ]));

                assert!(matches!(
                    prepare(&schema, raw),
                    Err(Error::InheritanceRequired)
                ));
            }
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
        use std::rc::Rc;
        use yamaa_core::schema::{
            Document, DocumentNode as N, NormalizationError, SchemaContext as C, SchemaDiagnostic,
            SchemaModule, SchemaOrigin, SchemaSource, SchemaStructure,
        };
        use yamaa_core::{
            project_preparation::Source,
            structural_preparation::{self as application, DecodePort, Decoded, Error, Selection},
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
                                field("language", "str", false),
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
        impl DecodePort for Codec {
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
                ("language", V::text("python")),
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
