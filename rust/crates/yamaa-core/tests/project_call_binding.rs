use yamaa_core::{
    bound_expression::Read,
    dataset::FunctionInput,
    project_call_binding::{bind, Context, Error, Kind, SourceMode},
    project_call_document::{Argument, Call, Input},
    project_calls::{Limits, LocatedCall, ProjectCalls},
    project_function::{Case, Definition, Function, Language, Parameter},
    reference_binding::{Catalog, Dataset, Diagnostic, Field},
    reference_scope::{Finding as ScopeFinding, Phase, Reach, Scope},
    value::{ColumnType, Value, ValueType},
};
fn prepared(input: Input) -> ProjectCalls {
    let function = Function::admit(
        Language::Python,
        Definition {
            name: "id".into(),
            function: "project.identity".into(),
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
    .unwrap();
    ProjectCalls::admit(
        &[function],
        vec![LocatedCall {
            path: "columns.OUT.function".into(),
            call: Call {
                name: "id".into(),
                name_node: 1,
                node: 2,
                arguments: vec![Argument {
                    name: "x".into(),
                    node: 3,
                    input,
                }],
            },
        }],
        Limits::default(),
    )
    .unwrap()
}
fn catalog(kind: ColumnType) -> Catalog {
    Catalog::compile(
        &[Field {
            name: "OUT",
            column_type: ColumnType::Int,
        }],
        &[Dataset {
            name: "SRC",
            fields: &[
                Field {
                    name: "ID",
                    column_type: ColumnType::Int,
                },
                Field {
                    name: "X",
                    column_type: kind,
                },
            ],
        }],
        Default::default(),
    )
    .unwrap()
}
fn column_context(mode: SourceMode) -> Context<'static> {
    Context {
        source_dataset: 0,
        source_mode: mode,
        available_outputs: None,
        scope: Scope {
            drivers: &["SRC"],
            current_driver: true,
            reach: Reach::Relation,
            joined: false,
            phase: Phase::Column { groups: &[] },
        },
    }
}

#[test]
fn stored_arguments_collect_for_key_grain_values_and_read_for_records() {
    let calls = prepared(Input::Reference("SRC.X".into()));
    let catalog = catalog(ColumnType::Int);
    let collected = bind(&calls, 0, &catalog, column_context(SourceMode::Collect)).unwrap();
    assert!(collected.dependencies.is_empty());
    assert_eq!(collected.function.slot(), 0);
    assert_eq!(
        collected.function.signature().identity().call,
        "project.identity"
    );
    assert_eq!(
        collected.function.arguments()[0].input,
        FunctionInput::Collect {
            column: 1,
            identifier: "SRC.X".into()
        }
    );
    let record = bind(&calls, 0, &catalog, column_context(SourceMode::Record)).unwrap();
    assert_eq!(
        record.function.arguments()[0].input,
        FunctionInput::Read(Read::Source(1))
    );
}

#[test]
fn bare_output_arguments_create_dependencies_and_respect_row_phase_availability() {
    let calls = prepared(Input::Reference("OUT".into()));
    let catalog = catalog(ColumnType::Int);
    let bound = bind(&calls, 0, &catalog, column_context(SourceMode::Collect)).unwrap();
    assert_eq!(bound.dependencies, [0]);
    assert_eq!(
        bound.function.arguments()[0].input,
        FunctionInput::Read(Read::Column(0))
    );
    let context = Context {
        available_outputs: Some(&[]),
        ..column_context(SourceMode::Record)
    };
    let Err(Error::Findings(findings)) = bind(&calls, 0, &catalog, context) else {
        panic!("phase finding")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].kind,
        Kind::Output(Diagnostic::PhaseBoundary { column: 0 })
    );
}

#[test]
fn grouping_fault_keeps_independent_exact_type_fault_for_the_same_argument() {
    let calls = prepared(Input::Reference("SRC.X".into()));
    let catalog = catalog(ColumnType::Float);
    let context = Context {
        source_dataset: 0,
        source_mode: SourceMode::Record,
        available_outputs: Some(&[]),
        scope: Scope {
            drivers: &["SRC"],
            current_driver: true,
            reach: Reach::Scalar,
            joined: false,
            phase: Phase::Row {
                group_by: Some(&["SRC.ID"]),
            },
        },
    };
    let Err(Error::Findings(findings)) = bind(&calls, 0, &catalog, context) else {
        panic!("complete independent findings")
    };
    assert_eq!(
        findings.iter().map(|f| f.argument).collect::<Vec<_>>(),
        [0, 0]
    );
    assert_eq!(
        findings.iter().map(|f| f.kind.clone()).collect::<Vec<_>>(),
        [
            Kind::Scope(ScopeFinding::RowGroup),
            Kind::ArgumentType {
                expected: ValueType::Int,
                actual: ValueType::Float
            },
        ]
    );
}

#[test]
fn unresolved_bare_and_wrong_driver_references_keep_existing_diagnostic_priority() {
    let catalog = catalog(ColumnType::Float);
    let calls = prepared(Input::Reference("X".into()));
    let Err(Error::Findings(findings)) =
        bind(&calls, 0, &catalog, column_context(SourceMode::Collect))
    else {
        panic!("bare reference finding")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].kind,
        Kind::Output(Diagnostic::UnresolvableName { dataset: 0 })
    );
    let calls = prepared(Input::Reference("OTHER.X".into()));
    let Err(Error::Findings(findings)) =
        bind(&calls, 0, &catalog, column_context(SourceMode::Collect))
    else {
        panic!("driver finding")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, Kind::Scope(ScopeFinding::DriverMismatch));
}

#[test]
fn missing_and_full_width_literals_need_no_reference_bindings() {
    let empty = Catalog::compile(&[], &[], Default::default()).unwrap();
    for value in [Value::Missing, Value::Int(i64::MIN), Value::Int(i64::MAX)] {
        let calls = prepared(Input::Literal(value.clone()));
        let bound = bind(&calls, 0, &empty, column_context(SourceMode::Record)).unwrap();
        assert_eq!(
            bound.function.arguments()[0].input,
            FunctionInput::Literal(value)
        );
        assert!(bound.dependencies.is_empty());
    }
    assert!(matches!(
        bind(
            &prepared(Input::Literal(Value::Int(1))),
            1,
            &empty,
            column_context(SourceMode::Record)
        ),
        Err(Error::InvalidCallIndex)
    ));
}
