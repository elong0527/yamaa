use std::sync::Arc;
use yamaa_adapters::{
    project_source::{
        self, CaptureFailure, CapturePort, Failure, Kind, Limits, Origin, Reply, Request,
    },
    specification_source::{CapturedSchema, Error, Limits as SchemaLimits, Source},
};
use yamaa_core::{
    project_environment::{Finding, LockKind},
    project_function::Language,
    project_limits::AdmissionError,
    value::Value,
};
fn source(identity: &str, bytes: &str) -> Source {
    Source {
        identity: identity.into(),
        bytes: bytes.as_bytes().to_vec(),
    }
}
fn schema() -> Arc<CapturedSchema> {
    CapturedSchema::admit_root(
        vec![
            source(
                "schema_environment.yaml",
                include_str!("fixtures/environment-candidate/schema_environment.yaml"),
            ),
            source(
                "schema_shared.yaml",
                include_str!("fixtures/environment-candidate/schema_shared.yaml"),
            ),
        ],
        0,
        "environment_class",
        SchemaLimits::default(),
    )
    .unwrap()
}
#[derive(Debug, PartialEq)]
struct Payload(Box<str>);
struct Port {
    trace: Vec<(Kind, String, String, usize)>,
    fail: bool,
    wide: bool,
    lock_kind: LockKind,
    interrupt_at: Option<String>,
    interrupt_address: usize,
}
impl Port {
    fn new() -> Self {
        Self {
            trace: vec![],
            fail: false,
            wide: false,
            lock_kind: LockKind::Uv,
            interrupt_at: None,
            interrupt_address: 0,
        }
    }
}
impl CapturePort for Port {
    type Error = Payload;
    fn is_interrupt(&self, error: &Payload) -> bool {
        error.0.as_ref() == "host interrupt"
    }
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Payload> {
        self.trace.push((
            request.kind,
            request.declaring_source.into(),
            request.written.into(),
            request.remaining_bytes,
        ));
        if self.interrupt_at.as_deref() == Some(request.written) {
            let payload = Payload("host interrupt".into());
            self.interrupt_address = payload.0.as_ptr() as usize;
            return Err(payload);
        }
        if self.fail {
            return Err(Payload(request.written.into()));
        }
        match request.kind {
            Kind::Lock => Ok(Reply::Lock {
                source: source("held/project.lock", "version = 1\n"),
                kind: self.lock_kind,
            }),
            Kind::Function => Ok(Reply::Document(source(
                "held/function.yaml",
                if self.wide {
                    "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 9223372036854775808}]\n"
                } else {
                    "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]\n"
                },
            ))),
            Kind::Codelist => Ok(Reply::Document(source(
                "held/terminology.yaml",
                "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n",
            ))),
        }
    }
}
fn root() -> Source {
    source("study/environment.yaml","schema_version: '1.0'\nlanguage: python\nlock: project.lock\nfunctions:\n  first: functions/constant.yaml\n  inline: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]}\ncodelists: [ct/sex.yaml]\nstudy: {id: Study, name: Study, description: Study, protocol_name: Protocol}\nadam: {standard: {name: ADaMIG, version: '1.3', status: Final}, specs: [specs/adsl.yaml], define: data/define.xml, documents: [{id: guide, kind: supplemental, link: docs/adrg.pdf, title: Guide}]}\n")
}
#[test]
fn owned_environment_parts_preserve_original_allocations_and_capture_evidence() {
    let mut port = Port::new();
    let prepared = project_source::prepare(
        schema(),
        root(),
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap();
    let root_bytes = prepared.root.source().bytes.as_ptr();
    let lock_bytes = prepared.lock.as_ref().unwrap().source.bytes.as_ptr();
    let cases = prepared.environment.draft().functions.as_ref().unwrap()[0]
        .tests
        .as_ptr();
    let captures = prepared.captures.as_ptr();
    let count = port.trace.len();
    let (environment, provenance) = prepared.into_owned().into_parts();
    assert_eq!(provenance.root().source().bytes.as_ptr(), root_bytes);
    assert_eq!(provenance.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
    assert_eq!(
        environment.functions()[0].definition().tests.as_ptr(),
        cases
    );
    assert_eq!(provenance.captures().as_ptr(), captures);
    assert_eq!(provenance.origins().functions, ["first", "inline"]);
    assert_eq!(environment.lock().unwrap().written, "project.lock");
    assert_eq!(
        environment
            .catalogue()
            .get("SEX")
            .unwrap()
            .items
            .as_ref()
            .unwrap()[0]
            .value,
        Value::Str("F".into())
    );
    assert_eq!(port.trace.len(), count);
}
#[test]
fn held_dependency_capture_is_ordered_typed_and_repeated_without_study_or_code_ports() {
    let schema = schema();
    let mut port = Port::new();
    for _ in 0..2 {
        let prepared = project_source::prepare(
            Arc::clone(&schema),
            root(),
            Language::Python,
            &mut port,
            Limits::default(),
        )
        .unwrap();
        assert!(Arc::ptr_eq(prepared.root.schema(), &schema));
        assert_eq!(prepared.root.source().bytes, root().bytes);
        assert_eq!(
            prepared
                .environment
                .draft()
                .functions
                .as_ref()
                .unwrap()
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "inline"]
        );
        assert_eq!(prepared.captures.len(), 2);
        assert_eq!(
            prepared.captures[0].origin,
            Origin::Function("first".into())
        );
        assert_eq!(
            prepared.captures[0].document.source().identity,
            "held/function.yaml"
        );
        assert_eq!(
            prepared.captures[0].document.class_name(),
            "function_definition_class"
        );
        assert_eq!(prepared.captures[1].origin, Origin::Codelist(0));
        assert_eq!(
            prepared.lock.as_ref().unwrap().source.bytes,
            b"version = 1\n"
        );
        assert!(prepared.environment.draft().has_study);
        let root_bytes = prepared.root.source().bytes.as_ptr();
        let lock_bytes = prepared.lock.as_ref().unwrap().source.bytes.as_ptr();
        let cases = prepared.environment.draft().functions.as_ref().unwrap()[0]
            .tests
            .as_ptr();
        let items = prepared.environment.draft().codelists[0].codelists[0]
            .items
            .as_ref()
            .unwrap()
            .as_ptr();
        let captured = port.trace.len();
        let owned = prepared.into_owned();
        assert_eq!(owned.root().source().bytes.as_ptr(), root_bytes);
        assert_eq!(owned.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
        assert_eq!(
            owned.environment().functions()[0]
                .definition()
                .tests
                .as_ptr(),
            cases
        );
        assert_eq!(
            owned.environment().catalogue().sources()[0].codelists[0]
                .items
                .as_ref()
                .unwrap()
                .as_ptr(),
            items
        );
        assert_eq!(owned.origins().functions, ["first", "inline"]);
        assert_eq!(owned.origins().codelists, [0]);
        assert_eq!(owned.captures().len(), 2);
        assert_eq!(owned.environment().host(), Language::Python);
        assert_eq!(owned.environment().language(), Some(Language::Python));
        assert!(owned.environment().has_study());
        assert_eq!(port.trace.len(), captured);
    }
    let one = [
        (Kind::Lock, "project.lock"),
        (Kind::Function, "functions/constant.yaml"),
        (Kind::Codelist, "ct/sex.yaml"),
    ];
    assert_eq!(
        port.trace
            .iter()
            .map(|(kind, declaring, written, _)| {
                assert_eq!(declaring, "study/environment.yaml");
                (*kind, written.as_str())
            })
            .collect::<Vec<_>>(),
        one.into_iter().chain(one).collect::<Vec<_>>()
    );
    for round in port.trace.chunks_exact(3) {
        assert!(round[0].3 > round[1].3 && round[1].3 > round[2].3);
    }
}
#[test]
fn invalid_root_and_inline_scalars_fail_before_every_dependency_capture() {
    let mut port = Port::new();
    let invalid = source(
        "environment.yaml",
        "schema_version: '1.0'\nruntime: {language: python}\nlock: project.lock\n",
    );
    assert!(matches!(
        project_source::prepare(
            schema(),
            invalid,
            Language::Python,
            &mut port,
            Limits::default()
        ),
        Err(Failure::Root(Error::Findings(_)))
    ));
    let invalid=source("environment.yaml","schema_version: '1.0'\nlock: project.lock\nfunctions: {wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: wide, covers: [normal], args: {}, result: 9223372036854775808}]}}\n");
    let Err(Failure::RootScalar { root, findings }) = project_source::prepare(
        schema(),
        invalid,
        Language::Python,
        &mut port,
        Limits::default(),
    ) else {
        panic!("owned scalar failure")
    };
    assert_eq!(root.source().identity, "environment.yaml");
    assert_eq!(findings[0].path, "functions.wide.tests[0].result");
    assert!(port.trace.is_empty());
}
#[test]
fn every_capture_failure_retains_original_opaque_payload_and_declaration_context() {
    let mut port = Port::new();
    port.fail = true;
    let root=source("study/environment.yaml","schema_version: '1.0'\nlanguage: python\nlock: project.lock\nfunctions: {a: a.yaml, b: b.yaml}\ncodelists: [one.yaml, two.yaml]\n");
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        schema(),
        root,
        Language::Python,
        &mut port,
        Limits::default(),
    ) else {
        panic!("all source failures")
    };
    assert_eq!(port.trace.len(), 5);
    assert_eq!(rejected.sources.len(), 5);
    assert_eq!(
        rejected
            .sources
            .iter()
            .map(|f| f.written.as_str())
            .collect::<Vec<_>>(),
        ["project.lock", "a.yaml", "b.yaml", "one.yaml", "two.yaml"]
    );
    for failure in &rejected.sources {
        let CaptureFailure::Port(Payload(original)) = &failure.error else {
            panic!("opaque error preserved")
        };
        assert_eq!(&**original, failure.written);
    }
    assert_eq!(rejected.root.source().identity, "study/environment.yaml");
    assert!(rejected.captures.is_empty());
    assert_eq!(rejected.host, Language::Python);
    assert_eq!(rejected.draft.language, Some(Language::Python));
    assert!(rejected.draft.lock.is_none());
    assert_eq!(rejected.draft.functions.as_deref(), Some([].as_slice()));
    assert!(rejected.draft.codelists.is_empty());
}
#[test]
fn source_quotas_precede_ports_and_byte_exhaustion_precedes_decoding() {
    let mut port = Port::new();
    let limits = Limits {
        sources: 1,
        ..Limits::default()
    };
    assert!(matches!(
        project_source::prepare(schema(), root(), Language::Python, &mut port, limits),
        Err(Failure::Root(Error::Limit("sources")))
    ));
    assert!(port.trace.is_empty());
    let mut port = Port::new();
    let limits = Limits {
        bytes: root().bytes.len(),
        ..Limits::default()
    };
    let Err(Failure::Rejected(rejected)) =
        project_source::prepare(schema(), root(), Language::Python, &mut port, limits)
    else {
        panic!("total held byte quota")
    };
    assert!(port.trace.iter().all(|request| request.3 == 0));
    assert!(rejected
        .sources
        .iter()
        .all(|f| matches!(f.error, CaptureFailure::Limit("bytes"))));
}
#[test]
fn dependency_scalar_failure_keeps_the_original_owned_document_and_other_sources() {
    let mut port = Port::new();
    port.wide = true;
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        schema(),
        root(),
        Language::Python,
        &mut port,
        Limits::default(),
    ) else {
        panic!("scalar failure")
    };
    assert_eq!(port.trace.len(), 3);
    assert_eq!(rejected.sources.len(), 1);
    assert_eq!(rejected.captures.len(), 1);
    let CaptureFailure::Scalar { document, findings } = &rejected.sources[0].error else {
        panic!("owned context")
    };
    assert_eq!(document.source().identity, "held/function.yaml");
    assert!(String::from_utf8_lossy(&document.source().bytes).contains("9223372036854775808"));
    assert_eq!(findings[0].path, "tests[0].result");
    assert!(findings[0].node < document.normalized().document.nodes().len());
}
#[test]
fn lock_kind_is_captured_syntax_metadata_and_mismatch_does_not_import_project_code() {
    let mut port = Port::new();
    port.lock_kind = LockKind::Renv;
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        schema(),
        root(),
        Language::Python,
        &mut port,
        Limits::default(),
    ) else {
        panic!("lock kind mismatch")
    };
    assert_eq!(port.trace.len(), 3);
    assert_eq!(
        rejected.admission,
        Some(AdmissionError::Findings(vec![Finding::LockKindMismatch {
            language: Language::Python,
            actual: LockKind::Renv
        }]))
    );
    assert!(rejected.sources.is_empty());
    assert_eq!(rejected.host, Language::Python);
    assert_eq!(rejected.draft.language, Some(Language::Python));
    assert_eq!(
        rejected.draft.lock.as_ref().unwrap().written,
        "project.lock"
    );
    assert_eq!(rejected.draft.lock.as_ref().unwrap().kind, LockKind::Renv);
    let functions = rejected.draft.functions.as_ref().unwrap();
    assert_eq!(
        functions
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "inline"]
    );
    assert_eq!(functions[0].tests[0].result, Value::Int(7));
    assert_eq!(functions[1].tests[0].result, Value::Int(7));
    assert_eq!(rejected.draft.codelists[0].codelists[0].id, "SEX");
    assert!(rejected.draft.has_study);
    assert_eq!(
        rejected.draft.submissions,
        [yamaa_core::project_environment::Submission::Adam]
    );
}

#[test]
fn interrupts_at_every_dependency_boundary_preserve_original_payload_and_abort_later_reads() {
    for (written, origin, observed) in [
        ("project.lock", Origin::Lock, 1),
        (
            "functions/constant.yaml",
            Origin::Function("first".into()),
            2,
        ),
        ("ct/sex.yaml", Origin::Codelist(0), 3),
    ] {
        let mut port = Port::new();
        port.interrupt_at = Some(written.into());
        let Err(Failure::Interrupted {
            origin: actual_origin,
            written: actual_written,
            error,
        }) = project_source::prepare(
            schema(),
            root(),
            Language::Python,
            &mut port,
            Limits::default(),
        )
        else {
            panic!("interrupt must bypass collected static findings");
        };
        assert_eq!(actual_origin, origin);
        assert_eq!(actual_written, written);
        assert_eq!(error.0.as_ptr() as usize, port.interrupt_address);
        assert_eq!(error.0.as_ref(), "host interrupt");
        assert_eq!(port.trace.len(), observed);
    }
}
