//! Independent application truth over native captures and normalized document fixtures.
#![cfg(feature = "test-support")]
use yamaa_core::{
    project_environment::{Finding, LockKind},
    project_function::Language,
    project_function_document::Kind as ScalarKind,
    project_limits::{AdmissionError, Resource},
    schema::{Document, DocumentLimits, DocumentNode as N},
    value::Value,
};
use yamaa_engine::{project_preparation::*, test_support::*};

enum Tree {
    Scalar(N),
    Map(Vec<(&'static str, Tree)>),
    Seq(Vec<Tree>),
}
impl Tree {
    fn text(value: &str) -> Self {
        Self::Scalar(N::Text(value.into()))
    }
    fn integer(value: &str) -> Self {
        Self::Scalar(N::Integer(value.into()))
    }
    fn boolean(value: bool) -> Self {
        Self::Scalar(N::Boolean(value))
    }
    fn append(self, nodes: &mut Vec<N>) -> usize {
        let node = match self {
            Self::Scalar(node) => node,
            Self::Seq(items) => N::Sequence(items.into_iter().map(|v| v.append(nodes)).collect()),
            Self::Map(items) => N::Mapping(
                items
                    .into_iter()
                    .map(|(k, v)| (Self::text(k).append(nodes), v.append(nodes)))
                    .collect(),
            ),
        };
        let id = nodes.len();
        nodes.push(node);
        id
    }
    fn document(self) -> Document {
        let mut nodes = vec![];
        let root = self.append(&mut nodes);
        Document::new(nodes, root, DocumentLimits::default()).unwrap()
    }
}
fn function(result: &str) -> Tree {
    Tree::Map(vec![
        ("function", Tree::text("project.constant")),
        ("description", Tree::text("A constant")),
        ("params", Tree::Seq(vec![])),
        ("returns", Tree::text("int")),
        ("may_return_missing", Tree::boolean(false)),
        ("comparison_decimals", Tree::integer("4")),
        (
            "tests",
            Tree::Seq(vec![Tree::Map(vec![
                ("id", Tree::text("ordinary")),
                (
                    "covers",
                    Tree::Seq(vec![Tree::text("normal"), Tree::text("boundary")]),
                ),
                ("args", Tree::Map(vec![])),
                ("result", Tree::integer(result)),
            ])]),
        ),
    ])
}
fn codelist(id: &str, value: Tree) -> Tree {
    Tree::Map(vec![(
        "codelists",
        Tree::Seq(vec![Tree::Map(vec![
            ("id", Tree::text(id)),
            ("name", Tree::text(id)),
            ("data_type", Tree::text("text")),
            ("extensible", Tree::boolean(false)),
            (
                "items",
                Tree::Seq(vec![Tree::Map(vec![
                    ("value", value),
                    ("extended", Tree::boolean(false)),
                ])]),
            ),
        ])]),
    )])
}
fn environment(language: Tree, inline_result: &str) -> Tree {
    Tree::Map(vec![
        ("language", language),
        ("lock", Tree::text("lockfile")),
        (
            "functions",
            Tree::Map(vec![
                ("first", Tree::text("first.yaml")),
                ("inline", function(inline_result)),
                ("last", Tree::text("last.yaml")),
            ]),
        ),
        (
            "codelists",
            Tree::Seq(vec![
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
type Outcome =
    Result<PreparedEnvironment<MetadataDocument>, Failure<Opaque, MetadataDocument, Opaque>>;
struct Fixture {
    source: Source,
    root: Document,
    log: EventLog,
    port: ScriptedMetadata<Opaque>,
    decoder: ScriptedMetadataDecoder<Opaque>,
    limits: Limits,
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
            limits: Limits {
                sources: 8,
                bytes: 256,
                identity_bytes: 256,
                nodes: 1024,
                ..Limits::default()
            },
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
        let mut bytes = self.source.bytes.len();
        let mut identities = self.source.identity.len();
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
                bytes += raw.len();
                identities += identity.len();
                Ok(if (kind == Kind::Lock) != (malformed == Some(index)) {
                    Reply::Lock {
                        source: captured,
                        kind: LockKind::Uv,
                    }
                } else {
                    Reply::Document(captured)
                })
            };
            // The scripted request is the allowance before this capture.
            let reply_bytes = if failed { 0 } else { raw.len() };
            let reply_identity = if failed { 0 } else { identity.len() };
            self.port.steps.push_back(MetadataCaptureStep {
                kind,
                declaring_source: self.source.identity.clone(),
                written: written.into(),
                remaining_bytes: self.limits.bytes - bytes + reply_bytes,
                remaining_identity_bytes: self.limits.identity_bytes - identities + reply_identity,
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
            limits,
            ..
        } = self;
        prepare(&mut decoder, source, Language::Python, &mut port, limits)
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
        let Reply::Lock { source: lock, .. } = fixture.port.steps[0].outcome.as_ref().unwrap()
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
fn root_capture_quotas_refuse_before_decoder_or_dependency_authority() {
    for resource in ["sources", "bytes", "identity_bytes"] {
        let mut fixture = Fixture::new();
        match resource {
            "sources" => fixture.limits.sources = 0,
            "bytes" => fixture.limits.bytes = fixture.source.bytes.len() - 1,
            _ => fixture.limits.identity_bytes = fixture.source.identity.len() - 1,
        }
        let log = fixture.log.clone();
        assert!(matches!(fixture.run(), Err(Failure::RootLimit(code)) if code == resource));
        assert!(effects(&log).is_empty());
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
fn root_node_quota_is_after_structural_decode_and_before_scalar_lowering() {
    let mut fixture = Fixture::new();
    fixture.root = environment(Tree::boolean(true), "7").document();
    fixture.limits.nodes = fixture.root.nodes().len() - 1;
    fixture.script(&[], None, None, None);
    let log = fixture.log.clone();
    assert!(matches!(fixture.run(), Err(Failure::RootLimit("nodes"))));
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
        .all(|finding| finding.path == "language" && finding.kind == ScalarKind::NormalizedShape));
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
fn root_diagnostic_storage_refusal_has_no_partial_findings_or_dependency_effects() {
    for (resource, expected) in [
        ("count", "environment_diagnostic_count"),
        ("text", "environment_diagnostic_text"),
    ] {
        let mut fixture = Fixture::new();
        fixture.root = environment(Tree::boolean(true), "7").document();
        if resource == "count" {
            fixture.limits.semantic.findings = 0;
        } else {
            fixture.limits.semantic.text_bytes = 0;
        }
        fixture.script(&[], None, None, None);
        let log = fixture.log.clone();
        assert!(matches!(fixture.run(), Err(Failure::RootLimit(code)) if code == expected));
        assert_eq!(effects(&log), ["decode Environment /author/env.yaml"]);
    }
}

#[test]
fn total_declared_capture_attempts_are_bounded_before_even_failed_first_capture() {
    let mut fixture = Fixture::new();
    fixture.limits.sources = 4; // root + lock + two external functions + external codelist requires five.
    fixture.script(&[0, 1, 2, 3], None, None, None);
    let log = fixture.log.clone();
    assert!(matches!(fixture.run(), Err(Failure::RootLimit("sources"))));
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
        let CaptureFailure::Scalar { document, findings } = &rejected.sources[0].error else {
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
fn oversized_capture_bytes_and_identity_are_refused_before_decoder_and_retention() {
    for resource in ["bytes", "identity_bytes"] {
        let mut fixture = Fixture::new();
        fixture.script(&[], None, None, None);
        let Reply::Document(source) = fixture.port.steps[3].outcome.as_mut().unwrap() else {
            panic!()
        };
        if resource == "bytes" {
            source.bytes = vec![b'x'; 256];
        } else {
            source.identity = "x".repeat(256);
        }
        let log = fixture.log.clone();
        let Err(Failure::Rejected(rejected)) = fixture.run() else {
            panic!()
        };
        assert!(
            matches!(rejected.sources[0].error, CaptureFailure::Limit(code) if code == resource)
        );
        assert_eq!(rejected.captures.len(), 2);
        assert_eq!(rejected.origins.codelists, [0]);
        assert_eq!(effects(&log), full_effects()[..7]);
        assert!(rejected.admission.is_none());
    }
}

#[test]
fn cumulative_document_node_limit_never_retains_partial_definitions() {
    let mut fixture = Fixture::new();
    fixture.limits.nodes = fixture.root.nodes().len() + function("7").document().nodes().len() - 1;
    fixture.script(&[], None, None, None);
    let log = fixture.log.clone();
    let Err(Failure::Rejected(rejected)) = fixture.run() else {
        panic!()
    };
    assert_eq!(effects(&log), full_effects());
    assert_eq!(rejected.sources.len(), 2);
    assert!(rejected
        .sources
        .iter()
        .all(|f| matches!(f.error, CaptureFailure::Limit("nodes"))));
    assert_eq!(rejected.origins.functions, ["inline"]);
    assert_eq!(rejected.origins.codelists, [0, 1]);
    assert_eq!(rejected.captures.len(), 1);
    assert!(rejected.admission.is_none());
}

#[test]
fn semantic_quotas_preserve_the_existing_complete_metadata_capture_checkpoint() {
    for resource in [
        Resource::Nodes,
        Resource::TextBytes,
        Resource::Work,
        Resource::Findings,
    ] {
        let mut fixture = Fixture::new();
        match resource {
            Resource::Nodes => fixture.limits.semantic.nodes = 0,
            Resource::TextBytes => fixture.limits.semantic.text_bytes = 0,
            Resource::Work => fixture.limits.semantic.work = 0,
            Resource::Findings => fixture.limits.semantic.findings = 0,
        }
        fixture.script(&[], None, None, None);
        let log = fixture.log.clone();
        let Err(Failure::Rejected(rejected)) = fixture.run() else {
            panic!()
        };
        assert_eq!(effects(&log), full_effects());
        assert!(rejected.sources.is_empty());
        assert!(
            matches!(rejected.admission, Some(AdmissionError::Limit(limit)) if limit.resource == resource)
        );
        assert_eq!(rejected.captures.len(), 3);
        assert_eq!(rejected.draft.functions.as_ref().unwrap().len(), 3);
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
    let Some(AdmissionError::Findings(findings)) = rejected.admission else {
        panic!()
    };
    assert!(matches!(
        findings[0],
        Finding::LanguageMismatch {
            declared: Language::R,
            host: Language::Python
        }
    ));
    assert!(matches!(
        findings[1],
        Finding::LockKindMismatch {
            language: Language::R,
            actual: LockKind::Uv
        }
    ));
}

#[test]
fn empty_environment_has_only_root_decode_and_no_dependency_or_activation_authority() {
    let log = EventLog::default();
    let root = source("empty.yaml", b"{}");
    let mut decoder = ScriptedMetadataDecoder::<Opaque>::new(log.clone());
    decoder.steps.push_back(MetadataDecodeStep {
        source: root.clone(),
        target: DecodeTarget::Environment,
        outcome: Ok(Tree::Map(vec![]).document()),
    });
    let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
    let prepared = prepare(
        &mut decoder,
        root,
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap();
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
        outcome: Ok(Tree::Map(vec![("functions", Tree::Map(vec![]))]).document()),
    });
    let mut port = ScriptedMetadata::<Opaque>::new(log.clone());
    let Err(Failure::Rejected(rejected)) = prepare(
        &mut decoder,
        root,
        Language::Python,
        &mut port,
        Limits::default(),
    ) else {
        panic!()
    };
    assert_eq!(effects(&log), ["decode Environment empty.yaml"]);
    assert_eq!(rejected.draft.functions, Some(vec![]));
    assert_eq!(
        rejected.admission,
        Some(AdmissionError::Findings(vec![
            Finding::MissingLanguage,
            Finding::MissingLock
        ]))
    );
}
