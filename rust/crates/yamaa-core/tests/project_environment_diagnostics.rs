#[path = "diagnostic/project_environment.rs"]
mod support;
use yamaa_core::{
    diagnostic::ContextValue,
    project_environment::{self, Draft, Finding, LockKind, LockReference},
    project_environment_diagnostics::{diagnostics, diagnostics_with_text_limit, Error},
    project_function::{Case, Definition, Finding as F, Language, Parameter},
    value::{ColumnType, Value, ValueType},
};
#[test]
fn every_root_and_terminology_cause_has_independent_normative_paths_and_mapping() {
    assert_eq!(support::reached().len(), 8);
    let draft = support::draft();
    let projected = diagnostics(
        &draft,
        &project_environment::validate(Language::Python, &draft),
    )
    .unwrap();
    assert_eq!(
        projected[3].context["value"],
        ContextValue::Scalar(Value::Bool(true))
    );
    assert_eq!(projected[1].context.len(), 2);
    assert_eq!(
        projected[1].context["runner"],
        ContextValue::Scalar(Value::Str("python".into()))
    );
    assert_eq!(
        projected[7].context["field"],
        ContextValue::Scalar(Value::Str("decode".into()))
    );
    assert_eq!(
        projected[8].context["field"],
        ContextValue::Scalar(Value::Str("rank".into()))
    );
}
fn draft() -> Draft {
    Draft {
        language: Some(Language::Python),
        lock: Some(LockReference {
            written: "uv.lock".into(),
            kind: LockKind::Uv,
        }),
        functions: Some(vec![Definition {
            name: "id".into(),
            function: "project.id".into(),
            description: "Identity".into(),
            params: vec![Parameter {
                name: "x".into(),
                kind: ValueType::Int,
                required: true,
                default: None,
                accepts_missing: false,
            }],
            returns: ColumnType::Int,
            may_return_missing: false,
            comparison_decimals: 4,
            tests: vec![Case {
                id: "ordinary".into(),
                covers: vec!["normal".into()],
                args: vec![("x".into(), Value::Int(i64::MIN))],
                result: Value::Int(i64::MAX),
            }],
        }]),
        codelists: vec![],
        has_study: false,
        submissions: vec![],
    }
}
#[test]
fn every_function_field_route_preserves_authored_indices_and_exact_scalar_kinds() {
    let cases = [
        (F::InvalidName, "functions.id", "invalid_function_name"),
        (
            F::InvalidCallable,
            "functions.id.function",
            "invalid_callable",
        ),
        (
            F::EmptyDescription,
            "functions.id.description",
            "empty_description",
        ),
        (
            F::NegativeComparisonDecimals,
            "functions.id.comparison_decimals",
            "negative_comparison_decimals",
        ),
        (
            F::InvalidParameterName { parameter: 0 },
            "functions.id.params[0].name",
            "invalid_parameter_name",
        ),
        (
            F::DuplicateParameter { parameter: 0 },
            "functions.id.params[0].name",
            "duplicate_parameter",
        ),
        (
            F::RequiredDefault { parameter: 0 },
            "functions.id.params[0].default",
            "required_parameter_default",
        ),
        (
            F::MissingDefault { parameter: 0 },
            "functions.id.params[0].default",
            "missing_parameter_default",
        ),
        (
            F::InvalidDefault { parameter: 0 },
            "functions.id.params[0].default",
            "invalid_parameter_default",
        ),
        (
            F::InvalidCaseId { case: 0 },
            "functions.id.tests[0].id",
            "invalid_case_id",
        ),
        (
            F::DuplicateCaseId { case: 0 },
            "functions.id.tests[0].id",
            "duplicate_case_id",
        ),
        (
            F::EmptyCoverage { case: 0 },
            "functions.id.tests[0].covers",
            "empty_coverage",
        ),
        (
            F::DuplicateCoverage { case: 0, tag: 0 },
            "functions.id.tests[0].covers[0]",
            "duplicate_coverage",
        ),
        (
            F::InvalidCoverage { case: 0, tag: 0 },
            "functions.id.tests[0].covers[0]",
            "invalid_coverage",
        ),
        (
            F::UnsupportedEvidence { case: 0, tag: 0 },
            "functions.id.tests[0].covers[0]",
            "unsupported_coverage_evidence",
        ),
        (
            F::DuplicateArgument {
                case: 0,
                argument: 0,
            },
            "functions.id.tests[0].args.x",
            "duplicate_case_argument",
        ),
        (
            F::UnknownArgument {
                case: 0,
                argument: 0,
            },
            "functions.id.tests[0].args.x",
            "unknown_case_argument",
        ),
        (
            F::MissingRequiredArgument {
                case: 0,
                parameter: 0,
            },
            "functions.id.tests[0].args.x",
            "missing_required_case_argument",
        ),
        (
            F::ArgumentType {
                case: 0,
                argument: 0,
            },
            "functions.id.tests[0].args.x",
            "invalid_case_argument_type",
        ),
        (
            F::InvalidResult { case: 0 },
            "functions.id.tests[0].result",
            "invalid_case_result",
        ),
        (
            F::MissingObligation("boundary".into()),
            "functions.id.tests",
            "missing_coverage_obligation",
        ),
    ];
    let draft = draft();
    for (finding, path, reason) in cases {
        let result = diagnostics(
            &draft,
            &[Finding::Function {
                function: 0,
                finding,
            }],
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        let d = &result[0];
        assert_eq!(
            (d.definition().condition, d.definition().requirement),
            ("project_environment_invalid", Some("REQ-0695"))
        );
        assert_eq!(d.spec_paths, [path]);
        assert_eq!(
            d.context["reason"],
            ContextValue::Scalar(Value::Str(reason.into()))
        );
        if path.ends_with(".args.x") && reason != "missing_required_case_argument" {
            assert_eq!(
                d.context["value"],
                ContextValue::Scalar(Value::Int(i64::MIN))
            );
        }
        if path.ends_with(".result") {
            assert_eq!(
                d.context["value"],
                ContextValue::Scalar(Value::Int(i64::MAX))
            );
        }
    }
}
#[test]
fn cumulative_projection_limits_count_paths_context_and_repeated_findings_before_copying() {
    let draft = draft();
    let finding = Finding::Function {
        function: 0,
        finding: F::InvalidCallable,
    };
    // 128 fixed bytes + 10 path prefix + 2 name + 9 suffix + 8 key + 2 name
    // + 6 reason key + 16 reason bytes = 181 bytes.
    let exact = 181;
    assert_eq!(
        diagnostics_with_text_limit(&draft, std::slice::from_ref(&finding), exact - 1),
        Err(Error::Limit)
    );
    assert!(diagnostics_with_text_limit(&draft, std::slice::from_ref(&finding), exact).is_ok());
    assert_eq!(
        diagnostics_with_text_limit(&draft, &[finding.clone(), finding], exact * 2 - 1),
        Err(Error::Limit)
    );
    assert_eq!(
        diagnostics_with_text_limit(&draft, &vec![Finding::MissingLanguage; 65_537], usize::MAX),
        Err(Error::Limit)
    );
}
#[test]
fn invalid_held_indices_refuse_projection_without_inventing_a_semantic_diagnostic() {
    let draft = draft();
    for finding in [
        Finding::DuplicateFunction { function: 1 },
        Finding::Function {
            function: 0,
            finding: F::ArgumentType {
                case: 0,
                argument: 1,
            },
        },
        Finding::Function {
            function: 0,
            finding: F::MissingRequiredArgument {
                case: 0,
                parameter: 1,
            },
        },
    ] {
        assert_eq!(diagnostics(&draft, &[finding]), Err(Error::InvalidIndex));
    }
}

#[test]
fn retained_admission_reports_all_independent_root_errors_and_captured_lock_syntax() {
    use yamaa_core::{
        project_environment::{Environment, Submission},
        project_limits::{AdmissionError, Limits},
    };
    let mut d = draft();
    d.language = None;
    d.lock = None;
    d.functions = Some(vec![]);
    d.submissions = vec![Submission::Adam];
    let rejection = Environment::admit_retained_with_limits(Language::Python, d, Limits::default())
        .unwrap_err();
    let AdmissionError::Findings(findings) = &rejection.error else {
        panic!("semantic admission")
    };
    let projected = diagnostics(&rejection.draft, findings).unwrap();
    assert_eq!(projected.len(), 3);
    for (finding, (path, reason)) in projected.iter().zip([
        ("language", "missing_language"),
        ("lock", "missing_lock"),
        ("study", "missing_study"),
    ]) {
        assert_eq!(finding.spec_paths, [path]);
        assert_eq!(
            finding.context["reason"],
            ContextValue::Scalar(Value::Str(reason.into()))
        );
        assert_eq!(finding.context.len(), 1);
    }
    let mut d = draft();
    d.functions = None;
    d.language = Some(Language::R);
    let findings = project_environment::validate(Language::Python, &d);
    let projected = diagnostics(&d, &findings).unwrap();
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[1].spec_paths, ["lock"]);
    assert_eq!(
        projected[1].definition().condition,
        "project_environment_invalid"
    );
    assert_eq!(
        projected[1].context["actual_lock_kind"],
        ContextValue::Scalar(Value::Str("uv".into()))
    );
    assert_eq!(
        projected[1].context["language"],
        ContextValue::Scalar(Value::Str("r".into()))
    );
    assert_eq!(
        projected[1].context["reason"],
        ContextValue::Scalar(Value::Str("lock_kind_mismatch".into()))
    );
}
