//! Complete retained-attempt witnesses; expected reports are authored independently.
use serde_json::{json, Value};
use std::{cell::Cell, rc::Rc, sync::Arc};
use yamaa_adapters::{
    issue_rows::Issue,
    project_environment_diagnostics::{self as diagnostic, Error as ProjectionError},
    project_source::{self, CaptureFailure, CapturePort, Failure, Kind, Origin, Reply, Request},
    specification_source::{CapturedSchema, Source},
};
use yamaa_core::{
    project_environment::LockKind,
    project_function::Language,
    project_limits::{AdmissionError, Limit, Resource},
};
// Deliberately neither Clone, Debug nor Send; the exact held error remains owned.
struct Opaque {
    allocation: Rc<Cell<usize>>,
    text: String,
}
struct Port {
    error: Option<Opaque>,
    trace: Vec<(Kind, String)>,
    wide: bool,
    same_identity: bool,
}
fn source(identity: &str, text: &str) -> Source {
    Source {
        identity: identity.into(),
        bytes: text.as_bytes().to_vec(),
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
        Default::default(),
    )
    .unwrap()
}
impl CapturePort for Port {
    type Error = Opaque;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        self.trace.push((request.kind, request.written.into()));
        if request.written == "missing.yaml" {
            return Err(self.error.take().unwrap());
        }
        Ok(match request.kind {
            Kind::Lock => Reply::Lock {
                source: source("held/uv.lock", "version = 1\n"),
                kind: LockKind::Uv,
            },
            Kind::Function => Reply::Document(source(
                if self.same_identity {
                    "held/environment.yaml"
                } else {
                    "held/function.yaml"
                },
                if self.wide {
                    "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 123456789012345678901234567890}]\n"
                } else {
                    "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n"
                },
            )),
            Kind::Codelist => Reply::Document(source(
                "held/ct.yaml",
                if self.wide {
                    "codelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n"
                } else {
                    "codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n"
                },
            )),
        })
    }
}
fn rejected(
    wide: bool,
    same_identity: bool,
) -> (
    Box<project_source::RejectedEnvironment<Opaque>>,
    Port,
    Rc<Cell<usize>>,
    *const u8,
) {
    let allocation = Rc::new(Cell::new(19));
    let error = Opaque {
        allocation: Rc::clone(&allocation),
        text: "original capture allocation".into(),
    };
    let pointer = error.text.as_ptr();
    let mut port = Port {
        error: Some(error),
        trace: vec![],
        wide,
        same_identity,
    };
    let body = if wide {
        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [missing.yaml, ct.yaml]\n"
    } else {
        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [missing.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n"
    };
    let rejected = match project_source::prepare(
        schema(),
        source("held/environment.yaml", body),
        Language::Python,
        &mut port,
        Default::default(),
    ) {
        Err(Failure::Rejected(rejected)) => rejected,
        _ => panic!("complete retained rejection"),
    };
    assert_eq!(
        port.trace,
        [
            (Kind::Lock, "uv.lock".into()),
            (Kind::Function, "function.yaml".into()),
            (Kind::Codelist, "missing.yaml".into()),
            (Kind::Codelist, "ct.yaml".into())
        ]
    );
    (rejected, port, allocation, pointer)
}
fn expected(condition: &str, requirement: &str, path: &str, context: Value) -> Issue {
    Issue {
        phase: "validation".into(),
        condition: condition.into(),
        requirement: Some(requirement.into()),
        spec_paths: vec![path.into()],
        context: context.to_string(),
    }
}
fn opaque(
    rejected: &project_source::RejectedEnvironment<Opaque>,
    allocation: &Rc<Cell<usize>>,
    pointer: *const u8,
) {
    let original = rejected
        .sources
        .iter()
        .find_map(|source| match &source.error {
            CaptureFailure::Port(error) => Some(error),
            _ => None,
        })
        .unwrap();
    assert!(Rc::ptr_eq(&original.allocation, allocation));
    assert_eq!(original.text.as_ptr(), pointer);
    assert_eq!(original.text, "original capture allocation");
    assert_eq!(original.allocation.get(), 19);
}
#[test]
fn complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes() {
    for same_identity in [false, true] {
        let (rejected, port, allocation, pointer) = rejected(false, same_identity);
        assert_eq!(rejected.origins.codelists, [1, 2]);
        let function_source = if same_identity {
            "held/environment.yaml"
        } else {
            "held/function.yaml"
        };
        let mut want = vec![expected(
            "project_environment_invalid",
            "REQ-0695",
            "tests[0].result",
            json!({"function":"first","case":"ordinary","value":true,"reason":"invalid_case_result","source":function_source,"entry":"held/environment.yaml","environment_path":"functions.first"}),
        )];
        for obligation in ["boundary", "normal"] {
            want.push(expected("project_environment_invalid", "REQ-0695", "tests", json!({"function":"first","obligation":obligation,"reason":"missing_coverage_obligation","source":function_source,"entry":"held/environment.yaml","environment_path":"functions.first"})));
        }
        want.push(expected("codelist_shape_invalid", "REQ-0950", "codelists[0].items[0].value", json!({"codelist":"SEX","value":true,"source":"held/ct.yaml","entry":"held/environment.yaml","environment_path":"codelists[1]"})));
        want.push(expected("codelist_shape_invalid", "REQ-0950", "codelists[2].codelists[0].items[0].value", json!({"codelist":"OTHER","value":true,"source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"codelists[2]"})));
        let result = diagnostic::admission(&rejected).unwrap();
        assert_eq!(result, want);
        assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
        assert_eq!(port.trace.len(), 4);
        opaque(&rejected, &allocation, pointer);
    }
}
#[test]
fn complete_external_scalar_reports_preserve_unbounded_integers_and_non_debug_originals() {
    let (rejected, port, allocation, pointer) = rejected(true, false);
    assert_eq!(diagnostic::admission(&rejected).unwrap(), []);
    let wide: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
    let want = [
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "tests[0].result",
            json!({"reason":"integer_out_of_range","value":wide,"source":"held/function.yaml","entry":"held/environment.yaml","environment_path":"functions.first"}),
        ),
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "codelists[0].items[0].value",
            json!({"reason":"integer_out_of_range","value":wide,"source":"held/ct.yaml","entry":"held/environment.yaml","environment_path":"codelists[1]"}),
        ),
    ];
    let result = diagnostic::scalar_sources(&rejected).unwrap();
    assert_eq!(result, want);
    assert_eq!(diagnostic::scalar_sources(&rejected).unwrap(), result);
    assert_eq!(port.trace.len(), 4);
    opaque(&rejected, &allocation, pointer);
}
#[test]
fn whole_static_report_refusal_is_late_and_each_individual_finding_remains_admissible() {
    let (mut rejected, port, allocation, pointer) = rejected(false, false);
    let Some(AdmissionError::Findings(findings)) = rejected.admission.as_mut() else {
        panic!("static report")
    };
    let all = std::mem::take(findings);
    for finding in &all {
        rejected.admission = Some(AdmissionError::Findings(vec![finding.clone()]));
        assert!(diagnostic::admission_with_limit(&rejected, 3_000).is_ok());
    }
    rejected.admission = Some(AdmissionError::Findings(all));
    assert_eq!(
        diagnostic::admission_with_limit(&rejected, 3_000),
        Err(ProjectionError::Limit)
    );
    assert_eq!(diagnostic::admission(&rejected).unwrap().len(), 5);
    assert_eq!(port.trace.len(), 4);
    opaque(&rejected, &allocation, pointer);
}
#[test]
fn wrong_origin_shape_duplicate_origins_and_equal_text_wrong_schema_refuse_without_replacement_errors(
) {
    let (mut rejected, port, allocation, pointer) = rejected(false, false);
    rejected.origins.codelists[1] = 1;
    assert_eq!(
        diagnostic::admission(&rejected),
        Err(ProjectionError::InvalidOrigin)
    );
    rejected.origins.codelists[1] = 2;
    rejected.captures[0].origin = Origin::Codelist(2);
    assert_eq!(
        diagnostic::admission(&rejected),
        Err(ProjectionError::InvalidOrigin)
    );
    rejected.captures[0].origin = Origin::Function("first".into());
    let replacement_schema = schema();
    rejected.captures[0].document = replacement_schema.prepare_named_class(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n"), "function_definition_class").unwrap();
    assert_eq!(
        diagnostic::admission(&rejected),
        Err(ProjectionError::InvalidOrigin)
    );
    assert_eq!(port.trace.len(), 4);
    opaque(&rejected, &allocation, pointer);
}
#[test]
fn original_semantic_resource_error_precedes_bad_origins_and_projection_quota() {
    let (mut rejected, port, allocation, pointer) = rejected(false, false);
    let limit = Limit {
        resource: Resource::TextBytes,
    };
    rejected.admission = Some(AdmissionError::Limit(limit));
    rejected.origins.functions.clear();
    assert_eq!(
        diagnostic::admission_with_limit(&rejected, 0),
        Err(ProjectionError::AdmissionLimit(limit))
    );
    assert_eq!(port.trace.len(), 4);
    opaque(&rejected, &allocation, pointer);
}

#[test]
fn original_root_temporal_and_wide_scalar_report_keeps_independent_headers_without_capture_effects()
{
    let allocation = Rc::new(Cell::new(19));
    let mut port = Port {
        error: Some(Opaque {
            allocation,
            text: "never consumed".into(),
        }),
        trace: vec![],
        wide: false,
        same_identity: false,
    };
    let body = "schema_version: '1.0'\nlanguage: r\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n";
    let (root, findings) = match project_source::prepare(
        schema(),
        source("held/environment.yaml", body),
        Language::Python,
        &mut port,
        Default::default(),
    ) {
        Err(Failure::RootScalar { root, findings }) => (root, findings),
        _ => panic!("retained original root scalars"),
    };
    let wide: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
    let want = [
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "lock",
            json!({"reason":"missing_lock","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
        ),
        expected(
            "runner_language_mismatch",
            "REQ-0696",
            "language",
            json!({"declared":"r","runner":"python","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
        ),
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "functions.wide.tests[0].result",
            json!({"reason":"integer_out_of_range","value":wide,"source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
        ),
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "functions.bad_date.tests[0].result",
            json!({"reason":"invalid_date","value":"2024-02-30","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
        ),
        expected(
            "project_environment_invalid",
            "REQ-0695",
            "functions.bad_time.tests[0].result",
            json!({"reason":"invalid_datetime","value":"2024-01-01T25:00:00","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
        ),
    ];
    let result = diagnostic::scalar_root_with_host(&root, &findings, Language::Python).unwrap();
    assert_eq!(result, want);
    assert_eq!(
        diagnostic::scalar_root_with_host(&root, &findings, Language::Python).unwrap(),
        result
    );
    assert_eq!(port.trace, []);
    assert!(port.error.is_some());
}

#[test]
fn actual_captured_two_hundred_inline_functions_keep_the_full_six_hundred_record_report() {
    let allocation = Rc::new(Cell::new(19));
    let mut port = Port {
        error: Some(Opaque {
            allocation,
            text: "never consumed".into(),
        }),
        trace: vec![],
        wide: false,
        same_identity: false,
    };
    let mut body =
        String::from("schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n");
    for index in 0..200 {
        body.push_str(&format!("  function_{index}: {{function: project.constant, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: true}}]}}\n"));
    }
    let rejected = match project_source::prepare(
        schema(),
        source("held/environment.yaml", &body),
        Language::Python,
        &mut port,
        Default::default(),
    ) {
        Err(Failure::Rejected(rejected)) => rejected,
        _ => panic!("actual captured static rejection"),
    };
    let Some(AdmissionError::Findings(findings)) = &rejected.admission else {
        panic!("complete independent findings")
    };
    assert_eq!(findings.len(), 600);
    let result = diagnostic::admission(&rejected).unwrap();
    let mut want = vec![];
    for index in 0..200 {
        let name = format!("function_{index}");
        let declaration = format!("functions.{name}");
        want.push(expected("project_environment_invalid", "REQ-0695", &format!("{declaration}.tests[0].result"), json!({"function":name,"case":"ordinary","value":true,"reason":"invalid_case_result","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":declaration})));
        for obligation in ["boundary", "normal"] {
            want.push(expected("project_environment_invalid", "REQ-0695", &format!("{declaration}.tests"), json!({"function":name,"obligation":obligation,"reason":"missing_coverage_obligation","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":declaration})));
        }
    }
    assert_eq!(result, want);
    assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
    assert_eq!(port.trace, [(Kind::Lock, "uv.lock".into())]);
    assert!(port.error.is_some());
}
