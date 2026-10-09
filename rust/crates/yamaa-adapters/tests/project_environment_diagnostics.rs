use serde_json::Value;
use std::{rc::Rc, sync::Arc};
use yamaa_adapters::{
    project_environment_diagnostics::{admission, admission_with_limit, Error as ProjectionError},
    project_source::{self, CaptureFailure, CapturePort, Failure, Kind, Limits, Reply, Request},
    specification_source::{CapturedSchema, Limits as SchemaLimits, Source},
};
use yamaa_core::{project_environment::LockKind, project_function::Language};
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
struct Port {
    payload: Rc<()>,
    same_identity: bool,
    reads: usize,
    wide: bool,
    shape: bool,
}
impl CapturePort for Port {
    type Error = Rc<()>;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Rc<()>> {
        self.reads += 1;
        if request.written == "bad.yaml" {
            return Err(Rc::clone(&self.payload));
        }
        Ok(match request.kind {
            Kind::Lock => Reply::Lock { source:source("held/uv.lock","version = 1\n"),kind:LockKind::Uv },
            Kind::Function if self.shape => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: []\ncontract_version: '1'\nbinding: {call: project.constant}\n")),
            Kind::Codelist if self.shape => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\nlegacy: true\n")),
            Kind::Function if self.wide => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 123456789012345678901234567890}]\n")),
            Kind::Codelist if self.wide => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n")),
            Kind::Function => Reply::Document(source(if self.same_identity { "study/environment.yaml" } else { "held/function.yaml" }, "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n")),
            Kind::Codelist => Reply::Document(source("held/ct.yaml","codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n")),
        })
    }
}
fn rejected(same_identity: bool) -> (Box<project_source::RejectedEnvironment<Rc<()>>>, Rc<()>) {
    let payload = Rc::new(());
    let mut port = Port {
        payload: Rc::clone(&payload),
        same_identity,
        reads: 0,
        wide: false,
        shape: false,
    };
    let root=source("study/environment.yaml","schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n");
    let Failure::Rejected(rejected) = project_source::prepare(
        schema(),
        root,
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap_err() else {
        panic!("retained rejection")
    };
    assert_eq!(port.reads, 4);
    (rejected, payload)
}
fn context(issue: &yamaa_adapters::issue_rows::Issue) -> Value {
    serde_json::from_str(&issue.context).unwrap()
}
#[test]
fn static_admission_restores_external_and_inline_origins_after_an_earlier_capture_failure() {
    let (rejected, payload) = rejected(false);
    assert_eq!(rejected.origins.codelists, [1, 2]);
    let issues = admission(&rejected).unwrap();
    let function = issues
        .iter()
        .find(|i| context(i)["reason"] == "invalid_case_result")
        .unwrap();
    assert_eq!(function.spec_paths, ["tests[0].result"]);
    assert_eq!(context(function)["source"], "held/function.yaml");
    assert_eq!(context(function)["entry"], "study/environment.yaml");
    assert_eq!(context(function)["environment_path"], "functions.first");
    assert_eq!(context(function)["value"], true);
    let lists = issues
        .iter()
        .filter(|i| i.requirement.as_deref() == Some("REQ-0950"))
        .collect::<Vec<_>>();
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[0].spec_paths, ["codelists[0].items[0].value"]);
    assert_eq!(context(lists[0])["source"], "held/ct.yaml");
    assert_eq!(context(lists[0])["environment_path"], "codelists[1]");
    assert_eq!(
        lists[1].spec_paths,
        ["codelists[2].codelists[0].items[0].value"]
    );
    assert_eq!(context(lists[1])["source"], "study/environment.yaml");
    assert_eq!(context(lists[1])["environment_path"], "codelists[2]");
    let CaptureFailure::Port(original) = &rejected.sources[0].error else {
        panic!("held host payload")
    };
    assert!(Rc::ptr_eq(original, &payload));
    let again = admission(&rejected).unwrap();
    assert_eq!(again, issues);
}
#[test]
fn captured_external_document_ownership_does_not_depend_on_identity_text_equality() {
    let (rejected, _) = rejected(true);
    let issues = admission(&rejected).unwrap();
    let function = issues
        .iter()
        .find(|i| context(i)["reason"] == "invalid_case_result")
        .unwrap();
    assert_eq!(function.spec_paths, ["tests[0].result"]);
    assert_eq!(context(function)["source"], "study/environment.yaml");
    assert_eq!(context(function)["environment_path"], "functions.first");
}
#[test]
fn origin_and_payload_limits_refuse_whole_projection_while_retaining_original_rejection() {
    let (mut rejected, payload) = rejected(false);
    assert_eq!(
        admission_with_limit(&rejected, 1),
        Err(ProjectionError::Limit)
    );
    rejected.origins.functions.clear();
    assert_eq!(admission(&rejected), Err(ProjectionError::InvalidOrigin));
    let CaptureFailure::Port(original) = &rejected.sources[0].error else {
        panic!("held host payload")
    };
    assert!(Rc::ptr_eq(original, &payload));
}

#[test]
fn scalar_decode_diagnostics_obey_the_environment_quota_before_prefix_copies() {
    let name = "f".repeat(100);
    let cases = (0..40)
        .map(|n| {
            format!("{{id: case-{n}, covers: [normal], args: {{}}, result: 9223372036854775808}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let body = format!("schema_version: '1.0'\nfunctions:\n  ? {name}\n  : {{function: project.constant, description: Constant, params: [], returns: int, tests: [{cases}]}}\n");
    let mut port = Port {
        payload: Rc::new(()),
        same_identity: false,
        reads: 0,
        wide: false,
        shape: false,
    };
    let limits = Limits {
        semantic: yamaa_core::project_limits::Limits {
            text_bytes: 512,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        project_source::prepare(
            schema(),
            source("study/environment.yaml", &body),
            Language::Python,
            &mut port,
            limits
        ),
        Err(Failure::Root(
            yamaa_adapters::specification_source::Error::Limit("environment_diagnostic_text")
        ))
    ));
    assert_eq!(port.reads, 0);
}

#[test]
fn root_scalar_issues_keep_arbitrary_integers_and_invalid_temporal_text_without_dependency_reads() {
    use yamaa_adapters::project_environment_diagnostics::scalar_root;
    let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n";
    let mut port = Port {
        payload: Rc::new(()),
        same_identity: false,
        reads: 0,
        wide: false,
        shape: false,
    };
    let Failure::RootScalar { root, findings } = project_source::prepare(
        schema(),
        source("held/environment.yaml", body),
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap_err() else {
        panic!("complete root scalars")
    };
    let issues = scalar_root(&root, &findings).unwrap();
    assert_eq!(issues.len(), 3);
    assert_eq!(issues[0].spec_paths, ["functions.wide.tests[0].result"]);
    assert_eq!(
        context(&issues[0])["value"].to_string(),
        "123456789012345678901234567890"
    );
    assert_eq!(context(&issues[1])["value"], "2024-02-30");
    assert_eq!(context(&issues[2])["value"], "2024-01-01T25:00:00");
    for issue in &issues {
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            (
                "validation",
                "project_environment_invalid",
                Some("REQ-0695")
            )
        );
        assert_eq!(context(issue)["source"], "held/environment.yaml");
        assert_eq!(context(issue)["entry"], "held/environment.yaml");
        assert_eq!(context(issue)["environment_path"], "$");
    }
    assert_eq!(scalar_root(&root, &findings).unwrap(), issues);
    assert_eq!(port.reads, 0);
}

#[test]
fn failed_external_scalar_issues_keep_each_document_origin_and_original_other_failures() {
    use yamaa_adapters::project_environment_diagnostics::scalar_sources;
    let payload = Rc::new(());
    let mut port = Port {
        payload: Rc::clone(&payload),
        same_identity: false,
        reads: 0,
        wide: true,
        shape: false,
    };
    let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
    let Failure::Rejected(rejected) = project_source::prepare(
        schema(),
        source("study/environment.yaml", body),
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap_err() else {
        panic!("independent source scalars")
    };
    let issues = scalar_sources(&rejected).unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].spec_paths, ["tests[0].result"]);
    assert_eq!(context(&issues[0])["source"], "held/function.yaml");
    assert_eq!(context(&issues[0])["environment_path"], "functions.first");
    assert_eq!(issues[1].spec_paths, ["codelists[0].items[0].value"]);
    assert_eq!(context(&issues[1])["source"], "held/ct.yaml");
    assert_eq!(context(&issues[1])["environment_path"], "codelists[1]");
    for issue in &issues {
        assert_eq!(
            context(issue)["value"].to_string(),
            "123456789012345678901234567890"
        );
        assert_eq!(context(issue)["entry"], "study/environment.yaml");
    }
    let CaptureFailure::Port(original) = &rejected.sources[1].error else {
        panic!("held original failure")
    };
    assert!(Rc::ptr_eq(original, &payload));
    assert_eq!(port.reads, 4);
    assert_eq!(scalar_sources(&rejected).unwrap(), issues);
    assert_eq!(port.reads, 4);
}

#[test]
fn root_schema_issues_keep_actual_closed_fields_and_source_without_dependency_reads() {
    use yamaa_adapters::{
        project_environment_diagnostics::schema_root, specification_source::Error as SourceError,
    };
    let mut port = Port {
        payload: Rc::new(()),
        same_identity: false,
        reads: 0,
        wide: false,
        shape: false,
    };
    let body = "schema_version: '1.0'\nparents: []\nruntime: {}\nversion: '1'\n";
    let Failure::Root(SourceError::Findings(captured)) = project_source::prepare(
        schema(),
        source("held/environment.yaml", body),
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap_err() else {
        panic!("retained root schema findings")
    };
    let issues = schema_root(&captured).unwrap();
    assert_eq!(issues.len(), 3);
    for (issue, field) in issues.iter().zip(["parents", "runtime", "version"]) {
        assert_eq!(issue.spec_paths, [field]);
        assert_eq!(issue.condition, "unknown_field");
        assert_eq!(issue.requirement, None);
        assert_eq!(context(issue)["field"], field);
        assert_eq!(context(issue)["class"], "environment_class");
        assert_eq!(context(issue)["source"], "held/environment.yaml");
        assert_eq!(context(issue)["entry"], "held/environment.yaml");
        assert_eq!(context(issue)["environment_path"], "$");
    }
    assert_eq!(schema_root(&captured).unwrap(), issues);
    assert_eq!(port.reads, 0);
}

#[test]
fn failed_external_schema_issues_use_held_arenas_and_keep_original_other_failures() {
    use yamaa_adapters::project_environment_diagnostics::schema_sources;
    let payload = Rc::new(());
    let mut port = Port {
        payload: Rc::clone(&payload),
        same_identity: false,
        reads: 0,
        wide: false,
        shape: true,
    };
    let body = "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
    let Failure::Rejected(rejected) = project_source::prepare(
        schema(),
        source("study/environment.yaml", body),
        Language::Python,
        &mut port,
        Limits::default(),
    )
    .unwrap_err() else {
        panic!("independent captured schema failures")
    };
    let issues = schema_sources(&rejected).unwrap();
    assert_eq!(issues.len(), 3);
    for (issue, field, class, source, declaration) in [
        (
            &issues[0],
            "contract_version",
            "function_definition_class",
            "held/function.yaml",
            "functions.first",
        ),
        (
            &issues[1],
            "binding",
            "function_definition_class",
            "held/function.yaml",
            "functions.first",
        ),
        (
            &issues[2],
            "legacy",
            "codelist_source_class",
            "held/ct.yaml",
            "codelists[1]",
        ),
    ] {
        assert_eq!(issue.spec_paths, [format!("<normalization>.{field}")]);
        assert_eq!(issue.condition, "unknown_field");
        assert_eq!(issue.requirement, None);
        assert_eq!(context(issue)["field"], field);
        assert_eq!(context(issue)["class"], class);
        assert_eq!(context(issue)["source"], source);
        assert_eq!(context(issue)["entry"], "study/environment.yaml");
        assert_eq!(context(issue)["environment_path"], declaration);
    }
    let CaptureFailure::Port(original) = &rejected.sources[1].error else {
        panic!("held original host failure")
    };
    assert!(Rc::ptr_eq(original, &payload));
    assert_eq!(schema_sources(&rejected).unwrap(), issues);
    assert_eq!(port.reads, 4);
}

#[test]
fn invalid_inline_scalars_do_not_mask_independent_root_header_findings() {
    use yamaa_adapters::project_environment_diagnostics::scalar_root_with_host;
    for language in ["", "language: r\n"] {
        let body = format!("schema_version: '1.0'\n{language}functions: {{bad: {{function: project.constant, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal], args: {{}}, result: 123456789012345678901234567890}}]}}}}\nsdtm: {{standard: {{name: SDTMIG, version: '3.4', status: Final}}, specs: [], define: define.xml}}\n");
        let mut port = Port {
            payload: Rc::new(()),
            same_identity: false,
            reads: 0,
            wide: false,
            shape: false,
        };
        let Failure::RootScalar { root, findings } = project_source::prepare(
            schema(),
            source("held/environment.yaml", &body),
            Language::Python,
            &mut port,
            Limits::default(),
        )
        .unwrap_err() else {
            panic!("held root scalars")
        };
        let issues = scalar_root_with_host(&root, &findings, Language::Python).unwrap();
        assert_eq!(issues.len(), 4);
        let expected = if language.is_empty() {
            ["language", "lock", "study", "functions.bad.tests[0].result"]
        } else {
            ["lock", "language", "study", "functions.bad.tests[0].result"]
        };
        assert_eq!(
            issues
                .iter()
                .map(|i| i.spec_paths[0].as_str())
                .collect::<Vec<_>>(),
            expected
        );
        let mismatch = if language.is_empty() {
            None
        } else {
            Some(&issues[1])
        };
        if let Some(issue) = mismatch {
            assert_eq!(issue.requirement.as_deref(), Some("REQ-0696"));
            assert_eq!(context(issue)["declared"], "r");
            assert_eq!(context(issue)["runner"], "python");
        }
        assert_eq!(context(&issues[2])["reason"], "missing_study");
        for issue in &issues {
            assert_eq!(context(issue)["source"], "held/environment.yaml");
            assert_eq!(context(issue)["environment_path"], "$");
        }
        assert_eq!(
            scalar_root_with_host(&root, &findings, Language::Python).unwrap(),
            issues
        );
        assert_eq!(port.reads, 0);
    }
}
