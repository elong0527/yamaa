#[path = "../../yamaa-core/tests/support/project_call_compiler.rs"]
#[allow(dead_code)]
mod support;
use std::rc::Rc;
use yamaa_adapters::project_activation_diagnostics::{
    binding_issues, test_issues, test_issues_with_limit, DefinitionSource, Error, HostDetails,
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::{LockKind, LockReference},
    project_function::{Case, Definition, Function, Language},
    value::{ColumnType, FiniteFloat, Value},
};
use yamaa_engine::{
    function_invocation::{Argument, FailureKind, HostError, InvocationFailure},
    project_activation::{self, ActivationPort, BindingFailure, Failure, TestFailure},
};

struct Port {
    payload: Rc<str>,
    bindings_fail: bool,
    calls_fail: bool,
    calls: usize,
    bindings: usize,
}
impl ActivationPort for Port {
    type Handle = ();
    type Error = Rc<str>;
    fn verify_lock(
        &mut self,
        _: Language,
        _: &LockReference,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), Rc<str>> {
        Ok(())
    }
    fn bind(&mut self, _: &ProjectFunctionIdentity, _: &LogicalSignature) -> Result<(), Rc<str>> {
        self.bindings += 1;
        if self.bindings_fail {
            Err(self.payload.clone())
        } else {
            Ok(())
        }
    }
    fn invoke(&mut self, _: &(), args: &[Argument<'_>]) -> Result<Value, HostError<Rc<str>>> {
        self.calls += 1;
        if self.calls_fail {
            return Err(HostError::Raised(self.payload.clone()));
        }
        let [Argument {
            value: yamaa_core::table::ValueRef::Int(value),
            ..
        }] = args
        else {
            panic!("exact argument")
        };
        Ok(Value::Int(if *value == 7 { 8 } else { *value }))
    }
}
fn port() -> Port {
    Port {
        payload: Rc::from("é\0🙂 original"),
        bindings_fail: false,
        calls_fail: false,
        calls: 0,
        bindings: 0,
    }
}
fn lock() -> LockReference {
    LockReference {
        written: "uv.lock".into(),
        kind: LockKind::Uv,
    }
}
fn sources(functions: &[Function; 2]) -> [DefinitionSource<'_>; 2] {
    [
        DefinitionSource {
            function: &functions[0],
            path: "functions.first",
        },
        DefinitionSource {
            function: &functions[1],
            path: "functions.second",
        },
    ]
}
#[test]
fn real_activation_mismatches_keep_every_case_in_selected_order_without_retry() {
    let functions = [support::function("first"), support::function("second")];
    let mut port = port();
    let Failure::Tests(failures) =
        project_activation::activate(Language::Python, &lock(), &functions, &mut port).unwrap_err()
    else {
        panic!("actual case mismatch")
    };
    assert_eq!((port.bindings, port.calls), (2, 4));
    assert_eq!(failures.len(), 2);
    let issues = test_issues(&sources(&functions), &failures, &[None, None]).unwrap();
    assert_eq!(issues.len(), 2);
    for (index, issue) in issues.iter().enumerate() {
        assert_eq!(
            (
                issue.phase.as_str(),
                issue.condition.as_str(),
                issue.requirement.as_deref()
            ),
            (
                "validation",
                "function_conformance_failed",
                Some("REQ-0703")
            )
        );
        assert_eq!(
            issue.spec_paths,
            [format!("{}.tests[0]", sources(&functions)[index].path)]
        );
        let context: serde_json::Value = serde_json::from_str(&issue.context).unwrap();
        assert_eq!(context["function"], functions[index].definition().name);
        assert_eq!(context["case"], "normal");
        assert_eq!(context["args"]["x"], 7);
        assert_eq!(context["expected"], 7);
        assert_eq!(context["actual"], 8);
        assert!(context.get("contract_version").is_none());
        assert!(context.get("implementation_version").is_none());
    }
    assert_eq!(
        test_issues(&sources(&functions), &failures, &[None, None]).unwrap(),
        issues
    );
    assert_eq!((port.bindings, port.calls), (2, 4));
}
#[test]
fn actual_binding_and_invocation_errors_remain_original_non_send_payloads() {
    let functions = [support::function("first"), support::function("second")];
    let mut port = port();
    port.bindings_fail = true;
    let Failure::Bindings(failures) =
        project_activation::activate(Language::Python, &lock(), &functions, &mut port).unwrap_err()
    else {
        panic!("binding failures")
    };
    assert_eq!((port.bindings, port.calls), (2, 0));
    let Err(Error::Opaque(held)) = binding_issues(&sources(&functions), &failures, &[None, None])
    else {
        panic!("opaque binding flattened")
    };
    assert!(Rc::ptr_eq(held, &port.payload));
    let host = Some(HostDetails::Exception {
        class: "ValueError",
        message: "é\0🙂",
        truncated: false,
    });
    let issues = binding_issues(&sources(&functions), &failures, &[host, host]).unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].spec_paths, ["functions.first.function"]);
    assert_eq!(issues[0].requirement.as_deref(), Some("REQ-0695"));
    port.bindings_fail = false;
    port.calls_fail = true;
    let Failure::Tests(failures) =
        project_activation::activate(Language::Python, &lock(), &functions, &mut port).unwrap_err()
    else {
        panic!("call failures")
    };
    assert_eq!(failures.len(), 4);
    let details = [None, None, None, None];
    let Err(Error::Opaque(held)) = test_issues(&sources(&functions), &failures, &details) else {
        panic!("opaque test flattened")
    };
    assert!(Rc::ptr_eq(held, &port.payload));
    let issues = test_issues(&sources(&functions), &failures, &[host, host, host, host]).unwrap();
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(context["failure"]["condition"], "function_call_failed");
    assert_eq!(context["failure"]["requirement"], "REQ-0701");
    assert_eq!(context["failure"]["context"]["host_message"], "é\0🙂");
    assert_eq!(port.calls, 4);
}
#[test]
fn contradictory_indices_identity_result_and_duplicate_observations_return_no_issues() {
    let functions = [support::function("first"), support::function("second")];
    let selected = sources(&functions);
    for (function, case, actual, expected) in
        [(2, 0, 8, 7), (0, 9, 8, 7), (0, 0, 8, 9), (0, 0, 7, 7)]
    {
        let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
            function,
            case,
            actual: Value::Int(actual),
            expected: Value::Int(expected),
        }];
        assert!(matches!(
            test_issues(&selected, &failures, &[None]),
            Err(Error::InvalidContext)
        ));
    }
    let failure: TestFailure<()> = TestFailure::Invocation {
        function: 0,
        case: 0,
        error: InvocationFailure {
            identity: ProjectFunctionIdentity {
                name: "second".into(),
                call: "project.second".into(),
            },
            kind: FailureKind::BooleanResult,
        },
    };
    assert!(matches!(
        test_issues(&selected, &[failure], &[None]),
        Err(Error::InvalidContext)
    ));
    let failures = [
        BindingFailure {
            function: 0,
            error: (),
        },
        BindingFailure {
            function: 0,
            error: (),
        },
    ];
    let host = Some(HostDetails::Rejected {
        reason: "binding rejected",
        returned: None,
    });
    assert!(matches!(
        binding_issues(&selected, &failures, &[host, host]),
        Err(Error::InvalidContext)
    ));
}
#[test]
fn complete_projection_counts_host_details_and_escaped_paths_are_bounded() {
    let functions = [support::function("first"), support::function("second")];
    let selected = sources(&functions);
    let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
        function: 0,
        case: 0,
        actual: Value::Int(8),
        expected: Value::Int(7),
    }];
    assert!(matches!(
        test_issues_with_limit(&selected, &failures, &[None], 1),
        Err(Error::Limit)
    ));
    let escaped = "\0".repeat(1_400_000);
    let huge = [DefinitionSource {
        function: &functions[0],
        path: &escaped,
    }];
    assert!(matches!(
        test_issues(&huge, &failures, &[None]),
        Err(Error::Limit)
    ));
    let too_many: Vec<TestFailure<()>> = (0..65_537)
        .map(|_| TestFailure::Result {
            function: 0,
            case: 0,
            actual: Value::Int(8),
            expected: Value::Int(7),
        })
        .collect();
    assert!(matches!(
        test_issues(&selected, &too_many, &vec![None; 65_537]),
        Err(Error::Limit)
    ));
    let text = "x".repeat(8193);
    let host = Some(HostDetails::Exception {
        class: "ValueError",
        message: &text,
        truncated: false,
    });
    assert!(matches!(
        binding_issues(
            &selected,
            &[BindingFailure {
                function: 0,
                error: ()
            }],
            &[host]
        ),
        Err(Error::Limit)
    ));
}
#[test]
fn authored_float_bits_and_complete_scalar_values_remain_exact() {
    let float = |value| Value::Float(FiniteFloat::new(value).unwrap());
    let function = Function::admit(
        Language::Python,
        Definition {
            name: "zero".into(),
            function: "project.zero".into(),
            description: "Zero".into(),
            params: vec![],
            returns: ColumnType::Float,
            may_return_missing: false,
            comparison_decimals: 4,
            tests: vec![Case {
                id: "zero".into(),
                covers: vec![
                    "normal".into(),
                    "boundary".into(),
                    "numeric-comparison".into(),
                ],
                args: vec![],
                result: float(0.0),
            }],
        },
    )
    .unwrap();
    let selected = [DefinitionSource {
        function: &function,
        path: "functions.zero",
    }];
    let wrong: Vec<TestFailure<()>> = vec![TestFailure::Result {
        function: 0,
        case: 0,
        actual: float(1.0),
        expected: float(-0.0),
    }];
    assert!(matches!(
        test_issues(&selected, &wrong, &[None]),
        Err(Error::InvalidContext)
    ));
    let correct: Vec<TestFailure<()>> = vec![TestFailure::Result {
        function: 0,
        case: 0,
        actual: float(1.0),
        expected: float(0.0),
    }];
    let issues = test_issues(&selected, &correct, &[None]).unwrap();
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(
        context["expected"].as_f64().unwrap().to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(function.definition().tests[0].result, float(0.0));

    let integer = support::function("integer");
    let selected = [DefinitionSource {
        function: &integer,
        path: "functions.integer",
    }];
    let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
        function: 0,
        case: 1,
        actual: Value::Int(i64::MAX),
        expected: Value::Int(i64::MIN),
    }];
    let issues = test_issues(&selected, &failures, &[None]).unwrap();
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(context["expected"].as_i64(), Some(i64::MIN));
    assert_eq!(context["args"]["x"].as_i64(), Some(i64::MIN));
    assert_eq!(context["actual"].as_i64(), Some(i64::MAX));

    let text = Function::admit(
        Language::Python,
        Definition {
            name: "text".into(),
            function: "project.text".into(),
            description: "Text".into(),
            params: vec![],
            returns: ColumnType::Str,
            may_return_missing: false,
            comparison_decimals: 4,
            tests: vec![Case {
                id: "text".into(),
                covers: vec!["normal".into(), "boundary".into()],
                args: vec![],
                result: Value::Str("authored é\0🙂".into()),
            }],
        },
    )
    .unwrap();
    let selected = [DefinitionSource {
        function: &text,
        path: "functions.text",
    }];
    let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
        function: 0,
        case: 0,
        actual: Value::Str("actual é\0🙂".into()),
        expected: Value::Str("authored é\0🙂".into()),
    }];
    let issues = test_issues(&selected, &failures, &[None]).unwrap();
    let context: serde_json::Value = serde_json::from_str(&issues[0].context).unwrap();
    assert_eq!(context["expected"], "authored é\0🙂");
    assert_eq!(context["actual"], "actual é\0🙂");
}
