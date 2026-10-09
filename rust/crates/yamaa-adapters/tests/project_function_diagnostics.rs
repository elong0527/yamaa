use std::rc::Rc;
use yamaa_adapters::project_function_diagnostics::{invocation, Error, HostDetails};
use yamaa_core::{
    function_signature::ProjectFunctionIdentity,
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::{
    dataset::RowIdentity,
    function_invocation::{FailureKind, InvocationFailure},
};

fn make_failure(kind: FailureKind<Rc<str>>) -> InvocationFailure<Rc<str>, ProjectFunctionIdentity> {
    InvocationFailure {
        identity: ProjectFunctionIdentity {
            name: "bmi".into(),
            call: "projectbmi.bmi".into(),
        },
        kind,
    }
}
#[test]
fn every_runtime_failure_keeps_versionless_identity_and_authored_provenance() {
    let cases = [
        (
            FailureKind::UnknownArguments(vec!["extra".into()]),
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::MissingRequired {
                parameter: "weight".into(),
            },
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::ArgumentType {
                parameter: "weight".into(),
                expected: ValueType::Float,
                actual: ValueType::Int,
            },
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::BooleanResult,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            FailureKind::UndeclaredMissing,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            FailureKind::ResultType {
                expected: ColumnType::Float,
                actual: ValueType::Str,
            },
            "invalid_function_result",
            "REQ-0702",
        ),
    ];
    let row = RowIdentity {
        position: 3,
        values: vec![
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            Value::Str("é\0🙂".into()),
        ],
    };
    for (kind, condition, requirement) in cases {
        let failure = make_failure(kind);
        let issue = invocation(
            "rows[2].derivation.BMI.function",
            &failure,
            Some(&row),
            &["MIN", "MAX", "TEXT"],
            None,
        )
        .unwrap();
        assert_eq!(issue.phase, "derivation");
        assert_eq!(issue.condition, condition);
        assert_eq!(issue.requirement.as_deref(), Some(requirement));
        assert_eq!(issue.spec_paths, ["rows[2].derivation.BMI.function"]);
        let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
        assert_eq!(context["function"], "bmi");
        assert_eq!(context["call"], "projectbmi.bmi");
        assert_eq!(
            context["keys"],
            serde_json::json!([{"MIN":i64::MIN,"MAX":i64::MAX,"TEXT":"é\0🙂"}])
        );
        assert!(context.get("contract_version").is_none());
        assert!(context.get("implementation_version").is_none());
    }
}
#[test]
fn opaque_payload_is_borrowed_unchanged_and_explicit_ordinary_facts_are_bounded() {
    let payload: Rc<str> = Rc::from("original interrupt or boundary");
    for kind in [
        FailureKind::CallFailed(payload.clone()),
        FailureKind::InvalidHostResult(payload.clone()),
    ] {
        let failure = make_failure(kind);
        let Err(Error::Opaque(held)) =
            invocation("columns.BMI.derivation.function", &failure, None, &[], None)
        else {
            panic!("opaque payload flattened")
        };
        assert!(Rc::ptr_eq(held, &payload));
        let issue = invocation(
            "columns.BMI.derivation.function",
            &failure,
            None,
            &[],
            Some(HostDetails::Exception {
                class: "ValueError",
                message: "é\0🙂",
                truncated: true,
            }),
        )
        .unwrap();
        let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
        assert_eq!(context["host_error"], "ValueError");
        assert_eq!(context["host_message"], "é\0🙂");
        assert_eq!(context["host_details_truncated"], true);
        assert!(Rc::strong_count(&payload) >= 2);
        let oversized = "x".repeat(8193);
        assert!(matches!(
            invocation(
                "columns.BMI.derivation.function",
                &failure,
                None,
                &[],
                Some(HostDetails::Rejected {
                    reason: &oversized,
                    returned: None
                })
            ),
            Err(Error::Limit)
        ));
    }
    let failure = make_failure(FailureKind::InvalidHostResult(payload));
    let issue = invocation(
        "columns.BMI.derivation.function",
        &failure,
        None,
        &[],
        Some(HostDetails::Rejected {
            reason: "not a scalar",
            returned: Some("list"),
        }),
    )
    .unwrap();
    assert_eq!(issue.condition, "invalid_function_result");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&issue.context).unwrap()["returned"],
        "list"
    );
}
#[test]
fn malformed_identity_keys_and_escaped_amplification_return_no_partial_issue() {
    let failure = make_failure(FailureKind::BooleanResult);
    let row = RowIdentity {
        position: 0,
        values: vec![Value::Int(1), Value::Int(2)],
    };
    for names in [&["ID"][..], &["ID", "ID"][..], &["", "ID"][..]] {
        assert!(matches!(
            invocation(
                "columns.BMI.derivation.function",
                &failure,
                Some(&row),
                names,
                None
            ),
            Err(Error::InvalidContext)
        ));
    }
    assert!(matches!(
        invocation("", &failure, None, &[], None),
        Err(Error::InvalidContext)
    ));
    assert!(matches!(
        invocation(
            "columns.BMI.derivation.function",
            &failure,
            None,
            &[],
            Some(HostDetails::Rejected {
                reason: "invented",
                returned: None
            })
        ),
        Err(Error::InvalidContext)
    ));
    let oversized = "\0".repeat(1_400_000);
    let failure = make_failure(FailureKind::UnknownArguments(vec![oversized]));
    assert!(matches!(
        invocation("columns.BMI.derivation.function", &failure, None, &[], None),
        Err(Error::Limit)
    ));
}

#[test]
fn actual_engine_failures_project_without_a_second_callback_or_payload_consumption() {
    use yamaa_core::function_signature::ProjectInvocationPlan;
    use yamaa_engine::function_invocation::{invoke_project, Argument, FunctionPort, HostError};
    struct Port {
        calls: usize,
        result: Option<Result<Value, HostError<Rc<str>>>>,
    }
    impl FunctionPort for Port {
        type Error = Rc<str>;
        fn call(&mut self, arguments: &[Argument<'_>]) -> Result<Value, HostError<Self::Error>> {
            assert!(arguments.is_empty());
            self.calls += 1;
            self.result.take().expect("callback was retried")
        }
    }
    let plan = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "identity".into(),
            call: "project.identity".into(),
        },
        vec![],
        ColumnType::Int,
        false,
    )
    .unwrap();
    let payload: Rc<str> = Rc::from("original host condition");
    for (result, condition, requirement, host_failure) in [
        (
            Ok(Value::Bool(true)),
            "invalid_function_result",
            "REQ-0702",
            false,
        ),
        (
            Ok(Value::Missing),
            "invalid_function_result",
            "REQ-0702",
            false,
        ),
        (
            Ok(Value::float(7.0)),
            "invalid_function_result",
            "REQ-0702",
            false,
        ),
        (
            Err(HostError::Raised(payload.clone())),
            "function_call_failed",
            "REQ-0701",
            true,
        ),
        (
            Err(HostError::InvalidResult(payload.clone())),
            "invalid_function_result",
            "REQ-0702",
            true,
        ),
    ] {
        let mut port = Port {
            calls: 0,
            result: Some(result),
        };
        let failure =
            invoke_project(&plan, &std::collections::BTreeMap::new(), &mut port).unwrap_err();
        let issue = if host_failure {
            let Err(Error::Opaque(held)) = invocation(
                "columns.VALUE.derivation.function",
                &failure,
                None,
                &[],
                None,
            ) else {
                panic!("opaque payload consumed")
            };
            assert!(Rc::ptr_eq(held, &payload));
            invocation(
                "columns.VALUE.derivation.function",
                &failure,
                None,
                &[],
                Some(HostDetails::Exception {
                    class: "OriginalCondition",
                    message: "ordinary classified failure",
                    truncated: false,
                }),
            )
            .unwrap()
        } else {
            invocation(
                "columns.VALUE.derivation.function",
                &failure,
                None,
                &[],
                None,
            )
            .unwrap()
        };
        assert_eq!(issue.condition, condition);
        assert_eq!(issue.requirement.as_deref(), Some(requirement));
        assert_eq!(port.calls, 1);
    }
}
