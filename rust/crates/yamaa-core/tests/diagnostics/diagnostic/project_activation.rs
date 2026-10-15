use super::project_support as support;
use std::{collections::BTreeSet, rc::Rc};
use yamaa_core::{
    dataset::RowIdentity,
    diagnostic::ConditionCode,
    function_invocation::{FailureKind, InvocationFailure},
    function_signature::ProjectFunctionIdentity,
    project_activation::{BindingFailure, TestFailure},
    project_diagnostics::{
        self as projection, DefinitionSource, Diagnostic, Error, Fact, HostDetails,
    },
    value::{ColumnType, FiniteFloat, Value, ValueType},
};

/// Actual core projection reaches every runtime mapping without owned host data.
pub fn reached() -> BTreeSet<ConditionCode> {
    let identity = ProjectFunctionIdentity {
        name: "id".into(),
        call: "pkg.id".into(),
    };
    let names = vec!["unknown".into()];
    let payload = Rc::new("retained non-Send error");
    let host = Some(HostDetails::Exception {
        class: "ValueError",
        message: "unchanged",
    });
    let mut reached = BTreeSet::new();
    for (kind, details, condition, requirement) in [
        (
            FailureKind::UnknownArguments(names.clone()),
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::MissingRequired {
                parameter: "x".into(),
            },
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::ArgumentType {
                parameter: "x".into(),
                expected: ValueType::Int,
                actual: ValueType::Str,
            },
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            FailureKind::CallFailed(&payload),
            host,
            "function_call_failed",
            "REQ-0701",
        ),
        (
            FailureKind::InvalidHostResult(&payload),
            host,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            FailureKind::BooleanResult,
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            FailureKind::UndeclaredMissing,
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            FailureKind::ResultType {
                expected: ColumnType::Int,
                actual: ValueType::Str,
            },
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
    ] {
        let failure = InvocationFailure {
            identity: identity.clone(),
            kind,
        };
        let diagnostic =
            projection::invocation("columns.X.derivation", &failure, None, &[], details).unwrap();
        let mapping = diagnostic.code.definition();
        assert_eq!(
            (mapping.phase, mapping.condition, mapping.requirement),
            ("derivation", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_path, "columns.X.derivation");
        let Fact::Text(name) = diagnostic.context["function"] else {
            panic!("borrowed function")
        };
        assert!(std::ptr::eq(name.as_ptr(), failure.identity.name.as_ptr()));
        reached.insert(diagnostic.code);
    }
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let binding = projection::binding_diagnostics(
        &definitions,
        &[BindingFailure {
            function: 0,
            error: &payload,
        }],
        &[host],
    )
    .unwrap();
    assert_eq!(binding[0].code, ConditionCode::ProjectEnvironmentInvalid);
    assert_eq!(binding[0].spec_path, "functions.id.function");
    reached.insert(binding[0].code);
    let actual = Value::Int(8);
    let failures: [TestFailure<Rc<&str>>; 1] = [TestFailure::Result {
        function: 0,
        case: 0,
        actual: actual.clone(),
        expected: function.definition().tests[0].result.clone(),
    }];
    let diagnostic = projection::test_diagnostics(&definitions, &failures, &[None])
        .unwrap()
        .remove(0);
    assert_eq!(diagnostic.spec_path, "functions.id.tests[0]");
    assert_eq!(
        diagnostic.context["reason"],
        Fact::Text("the case produced a different result")
    );
    let Fact::Scalar(held) = diagnostic.context["actual"] else {
        panic!("borrowed actual")
    };
    assert!(
        matches!(&failures[0], TestFailure::Result { actual, .. } if std::ptr::eq(held, actual))
    );
    reached.insert(diagnostic.code);
    reached
}

#[test]
fn opaque_payloads_remain_borrowed_with_original_precedence() {
    struct Payload(Rc<()>); // no Clone, Display, Send or Sync requirement
    let payload = Payload(Rc::new(()));
    let identity = ProjectFunctionIdentity {
        name: String::new(),
        call: String::new(),
    };
    let failure = InvocationFailure {
        identity: identity.clone(),
        kind: FailureKind::CallFailed(&payload),
    };
    let Err(Error::Opaque(held)) = projection::invocation("", &failure, None, &[], None) else {
        panic!("opaque failure precedence")
    };
    assert!(std::ptr::eq(*held, &payload));
    assert_eq!(Rc::strong_count(&held.0), 1);
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let failure = [BindingFailure {
        function: 0,
        error: &payload,
    }];
    let Err(Error::Opaque(held)) = projection::binding_diagnostics(&definitions, &failure, &[None])
    else {
        panic!("opaque binding")
    };
    assert!(std::ptr::eq(*held, &payload));
}

#[test]
fn complete_host_details_are_borrowed_in_runtime_binding_and_nested_case_diagnostics() {
    let function = support::function("id");
    let definition = function.definition();
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let class = "Exception".repeat(2048);
    let message = "exact\0\u{e9}\u{1f642}".repeat(2048);
    let reason = "invalid_native_result".repeat(1024);
    let returned = "returned\0\u{e9}".repeat(2048);
    for (host, fields) in [
        (
            HostDetails::Exception {
                class: &class,
                message: &message,
            },
            [
                ("host_error", class.as_str()),
                ("host_message", message.as_str()),
            ],
        ),
        (
            HostDetails::Rejected {
                reason: &reason,
                returned: Some(&returned),
            },
            [("reason", reason.as_str()), ("returned", returned.as_str())],
        ),
    ] {
        let failure = InvocationFailure {
            identity: ProjectFunctionIdentity {
                name: "id".into(),
                call: "project.id".into(),
            },
            kind: FailureKind::CallFailed(()),
        };
        let failures = [TestFailure::Invocation {
            function: 0,
            case: 0,
            error: failure,
        }];
        let TestFailure::Invocation { error: failure, .. } = &failures[0] else {
            unreachable!()
        };
        let context = [
            ("function", Fact::Text("id")),
            ("call", Fact::Text("project.id")),
            (fields[0].0, Fact::Text(fields[0].1)),
            (fields[1].0, Fact::Text(fields[1].1)),
        ]
        .into();
        let diagnostic =
            projection::invocation("columns.X.derivation", failure, None, &[], Some(host)).unwrap();
        assert_eq!(
            diagnostic,
            Diagnostic {
                code: ConditionCode::RuntimeFunctionCallFailed,
                spec_path: "columns.X.derivation".into(),
                context,
            }
        );
        for (key, text) in fields {
            let Fact::Text(held) = diagnostic.context[key] else {
                panic!("borrowed host detail")
            };
            assert!(std::ptr::eq(held.as_ptr(), text.as_ptr()));
        }
        let binding = [BindingFailure {
            function: 0,
            error: (),
        }];
        assert_eq!(
            projection::binding_diagnostics(&definitions, &binding, &[Some(host)]).unwrap(),
            [Diagnostic {
                code: ConditionCode::ProjectEnvironmentInvalid,
                spec_path: "functions.id.function".into(),
                context: diagnostic.context.clone(),
            }]
        );
        assert_eq!(
            projection::test_diagnostics(&definitions, &failures, &[Some(host)]).unwrap(),
            [Diagnostic {
                code: ConditionCode::FunctionConformanceFailed,
                spec_path: "functions.id.tests[0]".into(),
                context: [
                    ("function", Fact::Text("id")),
                    ("call", Fact::Text("project.id")),
                    ("case", Fact::Text("normal")),
                    (
                        "args",
                        Fact::Object([("x", Fact::Scalar(&definition.tests[0].args[0].1))].into())
                    ),
                    ("expected", Fact::Scalar(&definition.tests[0].result)),
                    ("comparison_decimals", Fact::Integer(4)),
                    (
                        "reason",
                        Fact::Text("the case failed before it produced a result")
                    ),
                    (
                        "failure",
                        Fact::Object(
                            [
                                ("phase", Fact::Text("derivation")),
                                ("condition", Fact::Text("function_call_failed")),
                                ("requirement", Fact::Text("REQ-0701")),
                                ("context", Fact::Object(diagnostic.context)),
                            ]
                            .into()
                        )
                    ),
                ]
                .into(),
            }]
        );
    }
}

#[test]
fn late_contradictions_return_no_partial_report() {
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let actual = Value::Int(8);
    let expected = &function.definition().tests[0].result;
    let failures: [TestFailure<()>; 2] = [
        TestFailure::Result {
            function: 0,
            case: 0,
            actual: actual.clone(),
            expected: expected.clone(),
        },
        TestFailure::Result {
            function: 0,
            case: 99,
            actual: actual.clone(),
            expected: expected.clone(),
        },
    ];
    assert!(matches!(
        projection::test_diagnostics(&definitions, &failures, &[None, None],),
        Err(Error::InvalidContext)
    ));
    let wrong = Value::Int(9);
    let failures: [TestFailure<()>; 1] = [TestFailure::Result {
        function: 0,
        case: 0,
        actual: actual.clone(),
        expected: wrong.clone(),
    }];
    assert!(matches!(
        projection::test_diagnostics(&definitions, &failures, &[None],),
        Err(Error::InvalidContext)
    ));
}

#[test]
fn nested_invocation_and_row_keys_preserve_borrowed_scalar_geometry() {
    let function = support::function("id");
    let identity = ProjectFunctionIdentity {
        name: "id".into(),
        call: function.definition().function.clone(),
    };
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let failures: [TestFailure<()>; 1] = [TestFailure::Invocation {
        function: 0,
        case: 0,
        error: InvocationFailure {
            identity: identity.clone(),
            kind: FailureKind::MissingRequired {
                parameter: "x".into(),
            },
        },
    }];
    let diagnostics = projection::test_diagnostics(&definitions, &failures, &[None]).unwrap();
    let Fact::Object(held) = &diagnostics[0].context["failure"] else {
        panic!("nested borrowed diagnostic")
    };
    assert_eq!(held["requirement"], Fact::Text("REQ-0700"));
    let Fact::Object(context) = &held["context"] else {
        panic!("nested invocation")
    };
    assert_eq!(context["parameter"], Fact::Text("x"));
    let row = [
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::Float(FiniteFloat::new(-0.0).unwrap()),
        Value::Str("\u{e9}\0\u{1f642}".into()),
    ];
    let row = RowIdentity {
        position: 0,
        values: row.to_vec(),
    };
    let failure = InvocationFailure::<(), ProjectFunctionIdentity> {
        identity,
        kind: FailureKind::BooleanResult,
    };
    let diagnostic = projection::invocation(
        "columns.X.derivation",
        &failure,
        Some(&row),
        &["MIN", "MAX", "ZERO", "TEXT"],
        None,
    )
    .unwrap();
    let Fact::Sequence(keys) = &diagnostic.context["keys"] else {
        panic!("row keys")
    };
    let [Fact::Object(keys)] = keys.as_slice() else {
        panic!("one exact key observation")
    };
    for (name, value) in ["MIN", "MAX", "ZERO", "TEXT"].into_iter().zip(&row.values) {
        let Fact::Scalar(held) = keys[name] else {
            panic!("scalar key")
        };
        assert!(std::ptr::eq(held, value));
    }
}
