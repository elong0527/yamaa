//! Independent reports/effects through the live environment metadata route.
#[path = "support/schema_requirement_expectations.rs"]
mod schema_requirement_expectations;

use serde_json::{json, Value};
use std::{rc::Rc, sync::Arc};
use yamaa_adapters::{
    issue_rows::Issue,
    project_environment_diagnostics as diagnostics,
    project_source::{
        self, CaptureFailure, CapturePort, Failure, Kind, Limits as ProjectLimits, Reply, Request,
    },
    specification_source::{CapturedSchema, Error, Limits, Source},
    yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits},
};
use yamaa_core::{
    project_environment::LockKind,
    project_function::Language,
    schema::{Document, DocumentNode as N, NormalizationError, SchemaSource, ValidationError},
};

const FUNCTION:&str="function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]\n";
const CODELIST: &str = "codelists: [{id: SEX, name: Sex, items: [{value: F}, {value: M}]}]\n";
fn source(name: &str, text: &str) -> Source {
    Source {
        identity: name.into(),
        bytes: text.as_bytes().to_vec(),
    }
}
fn captured_schema(limits: Limits) -> Arc<CapturedSchema> {
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
        limits,
    )
    .unwrap()
}
#[derive(Debug, PartialEq)]
enum Logical {
    Null,
    Bool(bool),
    Int(String),
    Float(u64),
    Text(String),
    Sequence(Vec<Logical>),
    Mapping(Vec<(Logical, Logical)>),
}
fn tree(d: &Document, id: usize) -> Logical {
    match &d.nodes()[id] {
        N::Null => Logical::Null,
        N::Boolean(v) => Logical::Bool(*v),
        N::Integer(v) => Logical::Int(v.clone()),
        N::Float(v) => Logical::Float(v.to_bits()),
        N::Text(v) => Logical::Text(v.clone()),
        N::Sequence(v) => Logical::Sequence(v.iter().map(|&id| tree(d, id)).collect()),
        N::Mapping(v) => {
            Logical::Mapping(v.iter().map(|&(k, v)| (tree(d, k), tree(d, v))).collect())
        }
    }
}
fn issues(rows: Vec<Issue>) -> Value {
    Value::Array(rows.into_iter().map(|row|json!({"phase":row.phase,"condition":row.condition,"requirement":row.requirement,"spec_paths":row.spec_paths,"context":serde_json::from_str::<Value>(&row.context).unwrap()})).collect())
}
fn original_locations(actual: &yamaa_adapters::specification_source::CapturedDocument, text: &str) {
    let decoded = decode_yaml(text.as_bytes(), DecodeLimits::default()).unwrap();
    assert_eq!(actual.raw().document, decoded.document);
    assert_eq!(actual.raw().locations, decoded.locations);
}
// This is not a language error. No Clone/Debug/Send/Sync requirement can be added
// by structural preparation or the surrounding live project codec integration.
struct Opaque {
    value: Box<str>,
    _thread: Rc<()>,
}
struct Metadata {
    pending: Vec<(Kind, String, Result<Reply, Opaque>)>,
    trace: Vec<(Kind, String, usize, usize)>,
}
impl Metadata {
    fn new(pending: Vec<(Kind, &str, Result<Reply, Opaque>)>) -> Self {
        Self {
            pending: pending
                .into_iter()
                .map(|(kind, path, reply)| (kind, path.into(), reply))
                .collect(),
            trace: vec![],
        }
    }
}
impl CapturePort for Metadata {
    type Error = Opaque;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Opaque> {
        self.trace.push((
            request.kind,
            request.written.into(),
            request.remaining_bytes,
            request.remaining_identity_bytes,
        ));
        let (kind, path, reply) = self.pending.remove(0);
        assert_eq!(kind, request.kind);
        assert_eq!(path, request.written);
        assert_eq!(request.declaring_source, "study/environment.yaml");
        reply
    }
}
fn lock() -> Reply {
    Reply::Lock {
        source: source("held/uv.lock", "version = 1\n"),
        kind: LockKind::Uv,
    }
}
fn document(name: &str, text: &str) -> Reply {
    Reply::Document(source(name, text))
}

#[test]
fn live_root_full_reports_preserve_original_source_and_stop_before_every_dependency() {
    let schema = captured_schema(Limits::default());
    for (text,expected) in [
        ("schema_version: '99.0'\nparents: [parent.yaml]\nfunctions: {bad: bad.yaml}\n",schema_requirement_expectations::reconciled(&json!([{ "phase":"validation","condition":"schema_version_mismatch","requirement":null,"spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":"study/environment.yaml","entry":"study/environment.yaml","environment_path":"$"}}]).to_string(), false)),
        ("schema_version: '1.0'\nparents: [base.yaml]\nruntime: {language: python}\nversion: old\n",schema_requirement_expectations::reconciled(&json!([
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["parents"],"context":{"field":"parents","class":"environment_class","source":"study/environment.yaml","entry":"study/environment.yaml","environment_path":"$"}},
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["runtime"],"context":{"field":"runtime","class":"environment_class","source":"study/environment.yaml","entry":"study/environment.yaml","environment_path":"$"}},
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["version"],"context":{"field":"version","class":"environment_class","source":"study/environment.yaml","entry":"study/environment.yaml","environment_path":"$"}}
        ]).to_string(), false)),
    ] {
        let entry=source("study/environment.yaml",text);let bytes=entry.bytes.as_ptr();let identity=entry.identity.as_ptr();let mut port=Metadata::new(vec![]);
        let Err(Failure::Root(Error::Findings(captured)))=project_source::prepare(Arc::clone(&schema),entry,Language::Python,&mut port,ProjectLimits::default()) else{panic!("root structural gate")};
        assert!(port.trace.is_empty());assert!(std::ptr::eq(captured.schema(),schema.as_ref()));assert_eq!(captured.source().bytes.as_ptr(),bytes);assert_eq!(captured.source().identity.as_ptr(),identity);
        assert_eq!(captured.document(),Some(&decode_yaml(text.as_bytes(),Default::default()).unwrap().document));assert_eq!(issues(diagnostics::schema_root(&captured).unwrap()),expected);
    }
}

#[test]
fn live_success_preserves_order_allowances_allocations_and_independent_default_trees() {
    let root="schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {external: functions/constant.yaml, inline: {function: project.constant, description: Constant., params: [], returns: int, tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]}}\ncodelists: [ct/sex.yaml]\n";
    let schema = captured_schema(Limits::default());
    let entry = source("study/environment.yaml", root);
    let root_bytes = entry.bytes.as_ptr();
    let root_identity = entry.identity.as_ptr();
    let function = source("held/function.yaml", FUNCTION);
    let function_pointer = function.bytes.as_ptr();
    let function_identity = function.identity.as_ptr();
    let codelist = source("held/ct.yaml", CODELIST);
    let ct_pointer = codelist.bytes.as_ptr();
    let ct_identity = codelist.identity.as_ptr();
    let mut port = Metadata::new(vec![
        (Kind::Lock, "uv.lock", Ok(lock())),
        (
            Kind::Function,
            "functions/constant.yaml",
            Ok(Reply::Document(function)),
        ),
        (Kind::Codelist, "ct/sex.yaml", Ok(Reply::Document(codelist))),
    ]);
    let prepared = project_source::prepare(
        Arc::clone(&schema),
        entry,
        Language::Python,
        &mut port,
        ProjectLimits::default(),
    )
    .unwrap_or_else(|_| panic!("complete live metadata"));
    assert_eq!(prepared.root.source().bytes.as_ptr(), root_bytes);
    assert_eq!(prepared.root.source().identity.as_ptr(), root_identity);
    original_locations(&prepared.root, root);
    assert!(Arc::ptr_eq(prepared.root.schema(), &schema));
    assert_eq!(prepared.captures.len(), 2);
    let function = &prepared.captures[0].document;
    let codelist = &prepared.captures[1].document;
    assert_eq!(function.source().bytes.as_ptr(), function_pointer);
    assert_eq!(function.source().identity.as_ptr(), function_identity);
    assert_eq!(codelist.source().bytes.as_ptr(), ct_pointer);
    assert_eq!(codelist.source().identity.as_ptr(), ct_identity);
    original_locations(function, FUNCTION);
    original_locations(codelist, CODELIST);
    assert_eq!(function.class_name(), "function_definition_class");
    assert_eq!(codelist.class_name(), "codelist_source_class");
    let expected_function="function: project.constant\ndescription: Constant.\nparams: []\nreturns: int\nmay_return_missing: false\ncomparison_decimals: 4\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]\n";
    let expected_ct="codelists: [{id: SEX, name: Sex, data_type: text, extensible: false, items: [{value: F, extended: false}, {value: M, extended: false}]}]\n";
    for (actual, expected) in [(function, expected_function), (codelist, expected_ct)] {
        let expected = decode_yaml(expected.as_bytes(), Default::default())
            .unwrap()
            .document;
        let d = &actual.normalized().document;
        assert_eq!(tree(d, d.root()), tree(&expected, expected.root()));
        assert!(d.field(d.root(), "schema_version").is_none());
        for origin in &actual.normalized().origins {
            match origin.source {
                SchemaSource::Input => assert!(origin.node < actual.raw().document.nodes().len()),
                SchemaSource::Default { module, descriptor } => {
                    assert_eq!(
                        actual.schema().structure().descriptors()[descriptor].module,
                        module
                    );
                    assert!(
                        origin.node
                            < actual.schema().structure().modules()[module]
                                .document
                                .nodes()
                                .len()
                    );
                }
            }
        }
    }
    let limits = ProjectLimits::default();
    let root_ids = "study/environment.yaml".len();
    assert_eq!(
        port.trace,
        vec![
            (
                Kind::Lock,
                "uv.lock".into(),
                limits.bytes - root.len(),
                limits.identity_bytes - root_ids
            ),
            (
                Kind::Function,
                "functions/constant.yaml".into(),
                limits.bytes - root.len() - "version = 1\n".len(),
                limits.identity_bytes - root_ids - "held/uv.lock".len()
            ),
            (
                Kind::Codelist,
                "ct/sex.yaml".into(),
                limits.bytes - root.len() - "version = 1\n".len() - FUNCTION.len(),
                limits.identity_bytes
                    - root_ids
                    - "held/uv.lock".len()
                    - "held/function.yaml".len()
            ),
        ]
    );
    assert!(port.pending.is_empty());
    assert_eq!(prepared.origins.functions, ["external", "inline"]);
    assert_eq!(prepared.origins.codelists, [0]);
    let inline = prepared
        .root
        .normalized()
        .document
        .field(
            prepared
                .root
                .normalized()
                .document
                .field(prepared.root.normalized().document.root(), "functions")
                .unwrap(),
            "inline",
        )
        .unwrap();
    assert_eq!(
        prepared.root.normalized().document.nodes()[prepared
            .root
            .normalized()
            .document
            .field(inline, "comparison_decimals")
            .unwrap()],
        N::Integer("4".into())
    );
}

#[test]
fn external_findings_keep_full_original_provenance_and_collect_later_live_sources() {
    let root="schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {bad: bad.yaml, good: good.yaml}\ncodelists: [ct.yaml]\n";
    let bad = FUNCTION.to_owned() + "contract_version: '1'\nbinding: {call: project.constant}\n";
    let schema = captured_schema(Limits::default());
    let failed = source("held/bad.yaml", &bad);
    let bytes = failed.bytes.as_ptr();
    let identity = failed.identity.as_ptr();
    let mut port = Metadata::new(vec![
        (Kind::Lock, "uv.lock", Ok(lock())),
        (Kind::Function, "bad.yaml", Ok(Reply::Document(failed))),
        (
            Kind::Function,
            "good.yaml",
            Ok(document("held/good.yaml", FUNCTION)),
        ),
        (
            Kind::Codelist,
            "ct.yaml",
            Ok(document("held/ct.yaml", CODELIST)),
        ),
    ]);
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        Arc::clone(&schema),
        source("study/environment.yaml", root),
        Language::Python,
        &mut port,
        ProjectLimits::default(),
    ) else {
        panic!("external structural rejection")
    };
    assert_eq!(
        port.trace
            .iter()
            .map(|(_, path, _, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["uv.lock", "bad.yaml", "good.yaml", "ct.yaml"]
    );
    assert_eq!(rejected.captures.len(), 2);
    assert_eq!(rejected.sources.len(), 1);
    let CaptureFailure::Structural(Error::Findings(captured)) = &rejected.sources[0].error else {
        panic!()
    };
    assert_eq!(captured.source().bytes.as_ptr(), bytes);
    assert_eq!(captured.source().identity.as_ptr(), identity);
    assert!(std::ptr::eq(captured.schema(), schema.as_ref()));
    assert_eq!(
        captured.document(),
        Some(
            &decode_yaml(bad.as_bytes(), Default::default())
                .unwrap()
                .document
        )
    );
    assert_eq!(
        issues(diagnostics::schema_sources(&rejected).unwrap()),
        schema_requirement_expectations::reconciled(&json!([
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.contract_version"],"context":{"field":"contract_version","class":"function_definition_class","source":"held/bad.yaml","entry":"study/environment.yaml","environment_path":"functions.bad"}},
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.binding"],"context":{"field":"binding","class":"function_definition_class","source":"held/bad.yaml","entry":"study/environment.yaml","environment_path":"functions.bad"}}
        ]).to_string(), false)
    );
}

#[test]
fn live_opaque_capture_identity_and_collection_stay_outside_structural_codec() {
    let root="schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {missing: missing.yaml, good: good.yaml}\ncodelists: [ct.yaml]\n";
    let payload = Opaque {
        value: "actual capture error".into(),
        _thread: Rc::new(()),
    };
    let pointer = payload.value.as_ptr();
    let mut port = Metadata::new(vec![
        (Kind::Lock, "uv.lock", Ok(lock())),
        (Kind::Function, "missing.yaml", Err(payload)),
        (
            Kind::Function,
            "good.yaml",
            Ok(document("held/good.yaml", FUNCTION)),
        ),
        (
            Kind::Codelist,
            "ct.yaml",
            Ok(document("held/ct.yaml", CODELIST)),
        ),
    ]);
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        captured_schema(Limits::default()),
        source("study/environment.yaml", root),
        Language::Python,
        &mut port,
        ProjectLimits::default(),
    ) else {
        panic!()
    };
    let CaptureFailure::Port(original) = &rejected.sources[0].error else {
        panic!()
    };
    assert_eq!(original.value.as_ptr(), pointer);
    assert_eq!(&*original.value, "actual capture error");
    assert_eq!(rejected.captures.len(), 2);
    assert_eq!(port.trace.len(), 4);
    assert!(port.pending.is_empty());
}

#[test]
fn named_predecode_guards_and_physical_query_failure_categories_remain_exact() {
    let schema = captured_schema(Limits {
        identity_bytes: 64,
        ..Default::default()
    });
    let malformed = || source("held/invalid.yaml", "[unclosed");
    assert!(matches!(
        schema.prepare_named_class(malformed(), &"x".repeat(65)),
        Err(Error::Limit("class_name"))
    ));
    for name in ["", "str", "function_definition", "unknown"] {
        assert!(
            matches!(schema.prepare_named_class(malformed(),name),Err(Error::UnknownClass(actual)) if actual==name)
        );
    }
    assert!(matches!(
        schema.prepare_named_class(malformed(), "environment_class"),
        Err(Error::VersionedRootRequired)
    ));
    assert!(matches!(
        schema.prepare_named_class(malformed(), "function_definition_class"),
        Err(Error::Decode {
            error: DecodeFailure::InvalidYaml { .. },
            ..
        })
    ));
    let mut limits = Limits::default();
    limits.normalization.validation.work = 0;
    let schema = captured_schema(limits);
    assert!(matches!(
        schema.prepare_named_class(
            source("held/function.yaml", FUNCTION),
            "function_definition_class"
        ),
        Err(Error::Validation(ValidationError::Constraint(_)))
    ));
    assert!(matches!(
        schema.prepare_structural(source("study/environment.yaml", "schema_version: '1.0'\n")),
        Err(Error::Normalize(NormalizationError::Validation(
            ValidationError::Constraint(_)
        )))
    ));
    assert!(matches!(
        schema.prepare_named_class(malformed(), "function_definition_class"),
        Err(Error::Decode { .. })
    ));
}

#[test]
fn root_and_named_complete_reports_keep_unbounded_integer_context_and_compatibility_shapes() {
    use yamaa_adapters::specification_diagnostics::capture_failure;
    let schema = captured_schema(Limits::default());
    let root = "schema_version: 92233720368547758081234567890\n";
    let entry = source("study/environment.yaml", root);
    let bytes = entry.bytes.as_ptr();
    let error = schema.prepare_structural(entry).unwrap_err();
    let Error::Findings(captured) = &error else {
        panic!()
    };
    assert_eq!(captured.source().bytes.as_ptr(), bytes);
    assert!(captured.document().is_some());
    let expected:Value=serde_json::from_str("{\"protocol\":\"specification/prototype\",\"outcome\":{\"status\":\"invalid\",\"diagnostics\":[{\"phase\":\"validation\",\"condition\":\"schema_version_mismatch\",\"requirement\":null,\"spec_paths\":[\"schema_version\"],\"context\":{\"expected\":\"1.0\",\"actual\":92233720368547758081234567890}}]}}").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&capture_failure(&error)).unwrap(),
        schema_requirement_expectations::reconciled(&expected.to_string(), false)
    );
    let error = schema
        .prepare_named_class(
            source(
                "held/function.yaml",
                &(FUNCTION.to_owned()
                    + "schema_version: '1.0'\ncontract_version: '1'\nbinding: {}\n"),
            ),
            "function_definition_class",
        )
        .unwrap_err();
    assert_eq!(
        serde_json::from_str::<Value>(&capture_failure(&error)).unwrap(),
        schema_requirement_expectations::reconciled(&json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.schema_version"],"context":{"field":"schema_version","class":"function_definition_class"}},
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.contract_version"],"context":{"field":"contract_version","class":"function_definition_class"}},
            {"phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.binding"],"context":{"field":"binding","class":"function_definition_class"}}
        ]}}).to_string(), false)
    );
}

#[test]
fn live_codelist_class_failure_keeps_full_raw_source_report_and_later_capture() {
    let root = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\ncodelists: [bad.yaml, good.yaml]\n";
    let bad = CODELIST.to_owned() + "retired: value\n";
    let failed = source("held/bad-ct.yaml", &bad);
    let bytes = failed.bytes.as_ptr();
    let identity = failed.identity.as_ptr();
    let schema = captured_schema(Limits::default());
    let mut port = Metadata::new(vec![
        (Kind::Lock, "uv.lock", Ok(lock())),
        (Kind::Codelist, "bad.yaml", Ok(Reply::Document(failed))),
        (
            Kind::Codelist,
            "good.yaml",
            Ok(document("held/good-ct.yaml", CODELIST)),
        ),
    ]);
    let Err(Failure::Rejected(rejected)) = project_source::prepare(
        Arc::clone(&schema),
        source("study/environment.yaml", root),
        Language::Python,
        &mut port,
        ProjectLimits::default(),
    ) else {
        panic!("external codelist structural rejection")
    };
    assert_eq!(
        port.trace
            .iter()
            .map(|(kind, path, _, _)| (*kind, path.as_str()))
            .collect::<Vec<_>>(),
        [
            (Kind::Lock, "uv.lock"),
            (Kind::Codelist, "bad.yaml"),
            (Kind::Codelist, "good.yaml")
        ]
    );
    assert!(port.pending.is_empty());
    assert_eq!(rejected.sources.len(), 1);
    assert_eq!(rejected.captures.len(), 1);
    let CaptureFailure::Structural(Error::Findings(captured)) = &rejected.sources[0].error else {
        panic!()
    };
    assert_eq!(captured.source().bytes.as_ptr(), bytes);
    assert_eq!(captured.source().identity.as_ptr(), identity);
    assert!(std::ptr::eq(captured.schema(), schema.as_ref()));
    assert_eq!(
        captured.document(),
        Some(
            &decode_yaml(bad.as_bytes(), Default::default())
                .unwrap()
                .document
        )
    );
    assert_eq!(
        issues(diagnostics::schema_sources(&rejected).unwrap()),
        schema_requirement_expectations::reconciled(&json!([{ "phase":"validation","condition":"unknown_field","requirement":null,"spec_paths":["<normalization>.retired"],"context":{"field":"retired","class":"codelist_source_class","source":"held/bad-ct.yaml","entry":"study/environment.yaml","environment_path":"codelists[0]"}}]).to_string(), false)
    );
}
