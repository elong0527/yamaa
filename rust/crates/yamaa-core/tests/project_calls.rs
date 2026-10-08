use yamaa_core::{
    project_call_document::{Argument, Call, Input},
    project_calls::{Error, Finding, Kind, Limits, LocatedCall, ProjectCalls},
    project_function::{CallFinding, Case, Definition, Function, Language, Parameter},
    value::{ColumnType, Value, ValueType},
};
fn function(name: &str) -> Function {
    Function::admit(
        Language::Python,
        Definition {
            name: name.into(),
            function: format!("project.{name}"),
            description: "Integer identity.".into(),
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
            tests: vec![
                Case {
                    id: "normal".into(),
                    covers: vec!["normal".into()],
                    args: vec![("x".into(), Value::Int(7))],
                    result: Value::Int(7),
                },
                Case {
                    id: "boundary".into(),
                    covers: vec!["boundary".into()],
                    args: vec![("x".into(), Value::Int(i64::MIN))],
                    result: Value::Int(i64::MIN),
                },
                Case {
                    id: "missing".into(),
                    covers: vec!["short-circuit-missing:x".into()],
                    args: vec![("x".into(), Value::Missing)],
                    result: Value::Missing,
                },
            ],
        },
    )
    .unwrap()
}
fn call(name: &str, args: Vec<(&str, Input)>) -> LocatedCall {
    LocatedCall {
        path: format!("columns.{name}.function"),
        call: Call {
            name: name.into(),
            name_node: 5,
            node: 9,
            arguments: args
                .into_iter()
                .enumerate()
                .map(|(node, (name, input))| Argument {
                    name: name.into(),
                    node,
                    input,
                })
                .collect(),
        },
    }
}
fn ordinary(name: &str) -> LocatedCall {
    call(name, vec![("x", Input::Literal(Value::Int(7)))])
}

#[test]
fn slots_follow_environment_order_and_occurrences_keep_their_authored_order() {
    let functions = vec![function("alpha"), function("beta"), function("unused")];
    let calls = vec![ordinary("beta"), ordinary("alpha"), ordinary("beta")];
    let expected = calls.clone();
    let compiled = ProjectCalls::admit(&functions, calls, Limits::default()).unwrap();
    assert_eq!(compiled.selected(), [0, 1]);
    assert_eq!(
        compiled
            .plans()
            .iter()
            .map(|p| p.identity().name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    assert_eq!(
        compiled
            .calls()
            .iter()
            .map(|c| c.slot())
            .collect::<Vec<_>>(),
        [1, 0, 1]
    );
    assert_eq!(
        compiled
            .calls()
            .iter()
            .map(|c| c.located.clone())
            .collect::<Vec<_>>(),
        expected
    );
    for c in compiled.calls() {
        assert_eq!(
            compiled.plans()[c.slot()].identity().name,
            c.located.call.name
        );
    }
}

#[test]
fn all_unknown_name_and_independent_argument_faults_are_retained() {
    let functions = vec![function("id")];
    let calls = vec![
        ordinary("unknown"),
        call("id", vec![("x", Input::Literal(Value::float(7.0)))]),
        call("id", vec![("other", Input::Reference("SRC.X".into()))]),
    ];
    let Err(Error::Findings(findings)) = ProjectCalls::admit(&functions, calls, Limits::default())
    else {
        panic!("complete static findings")
    };
    assert_eq!(
        findings,
        vec![
            Finding {
                call: 0,
                kind: Kind::UnknownFunction
            },
            Finding {
                call: 1,
                kind: Kind::Argument(CallFinding::ArgumentType {
                    argument: 0,
                    expected: ValueType::Int,
                    actual: ValueType::Float
                })
            },
            Finding {
                call: 2,
                kind: Kind::Argument(CallFinding::UnknownArgument { argument: 0 })
            },
            Finding {
                call: 2,
                kind: Kind::Argument(CallFinding::MissingRequiredArgument { parameter: 0 })
            },
        ]
    );
}

#[test]
fn missing_literals_and_unbound_reference_types_do_not_invent_type_faults() {
    let functions = vec![function("id")];
    let missing = call("id", vec![("x", Input::Literal(Value::Missing))]);
    let reference = call("id", vec![("x", Input::Reference("SRC.X".into()))]);
    let compiled =
        ProjectCalls::admit(&functions, vec![missing, reference], Limits::default()).unwrap();
    assert_eq!(compiled.selected(), [0]);
    assert!(matches!(
        compiled.calls()[0].located.call.arguments[0].input,
        Input::Literal(Value::Missing)
    ));
    assert!(matches!(
        compiled.calls()[1].located.call.arguments[0].input,
        Input::Reference(_)
    ));
}

#[test]
fn repeated_calls_charge_future_bound_plan_parameters_and_cumulative_work() {
    let functions = vec![function("id")];
    let limits = Limits {
        parameters: 3,
        work: 452,
        ..Limits::default()
    };
    assert!(ProjectCalls::admit(&functions, vec![ordinary("id"), ordinary("id")], limits).is_ok());
    for (limits, resource) in [
        (
            Limits {
                parameters: 2,
                ..limits
            },
            "parameters",
        ),
        (
            Limits {
                work: 451,
                ..limits
            },
            "work",
        ),
        (
            Limits {
                arguments: 1,
                ..limits
            },
            "arguments",
        ),
        (Limits { calls: 1, ..limits }, "calls"),
    ] {
        assert!(
            matches!(ProjectCalls::admit(&functions, vec![ordinary("id"), ordinary("id")], limits), Err(Error::Limit(r)) if r == resource)
        );
    }
}

#[test]
fn empty_calls_select_no_functions_and_duplicate_definitions_never_ambiguously_bind() {
    let f = function("id");
    let compiled = ProjectCalls::admit(
        std::slice::from_ref(&f),
        vec![],
        Limits {
            parameters: 0,
            work: 64,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(
        compiled.selected().is_empty()
            && compiled.plans().is_empty()
            && compiled.calls().is_empty()
    );
    assert!(
        matches!(ProjectCalls::admit(&[f.clone(), f.clone(), f], vec![ordinary("id")], Limits::default()), Err(Error::DuplicateDefinitions(indices)) if indices == [1, 2])
    );
}

#[test]
fn future_diagnostics_cannot_duplicate_long_logical_names_without_a_text_budget() {
    let name = "f".repeat(16_384);
    let functions = vec![function(&name)];
    let names = (0..1024).map(|i| format!("unknown{i}")).collect::<Vec<_>>();
    let args = names
        .iter()
        .map(|name| (name.as_str(), Input::Literal(Value::Int(7))))
        .collect();
    assert!(matches!(
        ProjectCalls::admit(&functions, vec![call(&name, args)], Limits::default()),
        Err(Error::Limit("text"))
    ));
}
