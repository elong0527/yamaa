#![allow(unused_imports)]
pub(crate) use crate::support::project_call_compiler as support;
pub(crate) use std::{collections::BTreeSet, rc::Rc};
pub(crate) use yamaa_core::{
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
