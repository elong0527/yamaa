use super::project_support as support;
use std::{collections::BTreeSet, rc::Rc};
use yamaa_core::{
    diagnostic::ConditionCode,
    function_signature::ProjectFunctionIdentity,
    project_diagnostics::{
        self as projection, DefinitionSource, Error, Fact, HostDetails, Invocation, InvocationKind,
        TestFailure,
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
        truncated: false,
    });
    let mut reached = BTreeSet::new();
    for (kind, details, condition, requirement) in [
        (
            InvocationKind::UnknownArguments(&names),
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            InvocationKind::MissingRequired { parameter: "x" },
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            InvocationKind::ArgumentType {
                parameter: "x",
                expected: ValueType::Int,
                actual: ValueType::Str,
            },
            None,
            "invalid_function_argument",
            "REQ-0700",
        ),
        (
            InvocationKind::CallFailed(&payload),
            host,
            "function_call_failed",
            "REQ-0701",
        ),
        (
            InvocationKind::InvalidHostResult(&payload),
            host,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            InvocationKind::BooleanResult,
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            InvocationKind::UndeclaredMissing,
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
        (
            InvocationKind::ResultType {
                expected: ColumnType::Int,
                actual: ValueType::Str,
            },
            None,
            "invalid_function_result",
            "REQ-0702",
        ),
    ] {
        let diagnostic = projection::invocation(
            "columns.X.derivation",
            &Invocation {
                identity: &identity,
                kind,
            },
            None,
            &[],
            details,
        )
        .unwrap();
        let mapping = diagnostic.code.definition();
        assert_eq!(
            (mapping.phase, mapping.condition, mapping.requirement),
            ("derivation", condition, Some(requirement))
        );
        assert_eq!(diagnostic.spec_path, "columns.X.derivation");
        let Fact::Text(name) = diagnostic.context["function"] else {
            panic!("borrowed function")
        };
        assert!(std::ptr::eq(name.as_ptr(), identity.name.as_ptr()));
        reached.insert(diagnostic.code);
    }
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let binding = projection::binding_diagnostics(
        &definitions,
        &[projection::BindingFailure {
            function: 0,
            error: &payload,
        }],
        &[host],
        projection::MAX_DIAGNOSTIC_BYTES,
    )
    .unwrap();
    assert_eq!(binding[0].code, ConditionCode::ProjectEnvironmentInvalid);
    assert_eq!(binding[0].spec_path, "functions.id.function");
    reached.insert(binding[0].code);
    let actual = Value::Int(8);
    let failures: [TestFailure<'_, Rc<&str>>; 1] = [TestFailure::Result {
        function: 0,
        case: 0,
        actual: &actual,
        expected: &function.definition().tests[0].result,
    }];
    let diagnostic = projection::test_diagnostics(
        &definitions,
        &failures,
        &[None],
        projection::MAX_DIAGNOSTIC_BYTES,
    )
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
    assert!(std::ptr::eq(held, &actual));
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
    let failure = Invocation {
        identity: &identity,
        kind: InvocationKind::CallFailed(&payload),
    };
    let Err(Error::Opaque(held)) = projection::invocation("", &failure, None, &[], None) else {
        panic!("opaque failure precedence")
    };
    assert!(std::ptr::eq(held, &payload));
    assert_eq!(Rc::strong_count(&held.0), 1);
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let failure = [projection::BindingFailure {
        function: 0,
        error: &payload,
    }];
    let Err(Error::Opaque(held)) = projection::binding_diagnostics(
        &definitions,
        &failure,
        &[None],
        projection::MAX_DIAGNOSTIC_BYTES,
    ) else {
        panic!("opaque binding")
    };
    assert!(std::ptr::eq(held, &payload));
}

#[test]
fn late_contradictions_and_budget_exhaustion_return_no_partial_report() {
    let function = support::function("id");
    let definitions = [DefinitionSource {
        function: &function,
        path: "functions.id",
    }];
    let actual = Value::Int(8);
    let expected = &function.definition().tests[0].result;
    let failures: [TestFailure<'_, ()>; 2] = [
        TestFailure::Result {
            function: 0,
            case: 0,
            actual: &actual,
            expected,
        },
        TestFailure::Result {
            function: 0,
            case: 99,
            actual: &actual,
            expected,
        },
    ];
    assert!(matches!(
        projection::test_diagnostics(
            &definitions,
            &failures,
            &[None, None],
            projection::MAX_DIAGNOSTIC_BYTES
        ),
        Err(Error::InvalidContext)
    ));
    assert!(matches!(
        projection::test_diagnostics(&definitions, &failures[..1], &[None], 1),
        Err(Error::Limit)
    ));
    let wrong = Value::Int(9);
    let failures: [TestFailure<'_, ()>; 1] = [TestFailure::Result {
        function: 0,
        case: 0,
        actual: &actual,
        expected: &wrong,
    }];
    assert!(matches!(
        projection::test_diagnostics(
            &definitions,
            &failures,
            &[None],
            projection::MAX_DIAGNOSTIC_BYTES
        ),
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
    let failures: [TestFailure<'_, ()>; 1] = [TestFailure::Invocation {
        function: 0,
        case: 0,
        error: Invocation {
            identity: &identity,
            kind: InvocationKind::MissingRequired { parameter: "x" },
        },
    }];
    let diagnostics = projection::test_diagnostics(
        &definitions,
        &failures,
        &[None],
        projection::MAX_DIAGNOSTIC_BYTES,
    )
    .unwrap();
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
    let diagnostic = projection::invocation(
        "columns.X.derivation",
        &Invocation::<()> {
            identity: &identity,
            kind: InvocationKind::BooleanResult,
        },
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
    for (name, value) in ["MIN", "MAX", "ZERO", "TEXT"].into_iter().zip(&row) {
        let Fact::Scalar(held) = keys[name] else {
            panic!("scalar key")
        };
        assert!(std::ptr::eq(held, value));
    }
}
