//! Authored versionless dataset plans and observable table/callback boundaries.
use std::cell::RefCell;
use yamaa_core::{
    bound_expression::Read,
    function_signature::{
        InvocationPlan, Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value, ValueType},
};
use yamaa_engine::{
    dataset::{
        Assignment, BoundProjectFunction, DatasetExecution, DatasetPlan, ExecutionError,
        Expression, FunctionArgument, FunctionBindings, FunctionInput, Limits, PlanError, RowMode,
        RowTemplate,
    },
    function_invocation::{Argument, FailureKind, HostError},
    project_activation::{ActivatedFunction, ActivationPort, Bindings},
};
fn schema() -> TableSchema {
    TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "X".into(),
            kind: ColumnType::Int,
        },
    ])
    .unwrap()
}
struct Table {
    rows: Vec<Vec<Value>>,
    schema: TableSchema,
    observations: RefCell<Vec<String>>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        self.observations.borrow_mut().push("schema".into());
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.observations.borrow_mut().push("rows".into());
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.observations
            .borrow_mut()
            .push(format!("cell:{row}:{column}"));
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn table(values: Vec<Value>) -> Table {
    Table {
        rows: values
            .into_iter()
            .enumerate()
            .map(|(i, x)| vec![Value::Int(i as i64 + 1), x])
            .collect(),
        schema: schema(),
        observations: RefCell::default(),
    }
}
fn signature() -> ProjectInvocationPlan {
    ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "identity".into(),
            call: "installed.identity".into(),
        },
        vec![
            Parameter {
                name: "x".into(),
                host_name: "x".into(),
                kind: ValueType::Int,
                presence: Presence::Required,
                accepts_missing: false,
            },
            Parameter {
                name: "scale".into(),
                host_name: "scale".into(),
                kind: ValueType::Int,
                presence: Presence::Optional(Value::Int(100)),
                accepts_missing: false,
            },
        ],
        ColumnType::Int,
        false,
    )
    .unwrap()
}
fn argument(input: FunctionInput) -> FunctionArgument {
    FunctionArgument {
        name: "x".into(),
        input,
    }
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation.function"),
    }
}
fn plan(
    signature: ProjectInvocationPlan,
    args: Vec<FunctionArgument>,
    mode: RowMode,
) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        schema(),
        schema(),
        vec![RowTemplate {
            mode,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::ProjectFunction(BoundProjectFunction::new_project(
                        0, signature, args,
                    )?),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 1000,
        key_cells: 1000,
        work_cells: 10000,
        scalar_text_bytes: 10000,
        output_text_bytes: 10000,
        identity_cells: 10000,
        identity_text_bytes: 10000,
    }
}
struct Callbacks {
    signature: Option<ProjectInvocationPlan>,
    calls: Vec<Vec<(String, Value)>>,
    result: Result<Value, &'static str>,
}
impl FunctionBindings for Callbacks {
    type Error = &'static str;
    fn signature(&self, _: usize) -> Option<&InvocationPlan> {
        None
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        if slot == 0 {
            self.signature.as_ref()
        } else {
            None
        }
    }
    fn call(
        &mut self,
        slot: usize,
        arguments: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        assert_eq!(slot, 0);
        self.calls.push(
            arguments
                .iter()
                .map(|a| {
                    (
                        a.name.into(),
                        match a.value {
                            ValueRef::Int(x) => Value::Int(x),
                            ValueRef::Float(x) => Value::float(x.get()),
                            ValueRef::Missing => Value::Missing,
                            _ => panic!("exact integer args"),
                        },
                    )
                })
                .collect(),
        );
        self.result.clone().map_err(HostError::Raised)
    }
}
fn callbacks() -> Callbacks {
    Callbacks {
        signature: Some(signature()),
        calls: vec![],
        result: Ok(Value::Int(i64::MAX)),
    }
}

#[test]
fn closed_versionless_slots_defaults_full_i64_and_repeated_invocations() {
    let source = table(vec![Value::Int(i64::MIN), Value::Int(7)]);
    let plan = plan(
        signature(),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    let mut callbacks = callbacks();
    for _ in 0..2 {
        let result = plan
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
            .result
            .unwrap();
        assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Int(i64::MAX));
        assert_eq!(result.dataset.cell(1, 1).unwrap(), ValueRef::Int(i64::MAX));
    }
    assert_eq!(
        callbacks.calls,
        vec![
            vec![
                ("x".into(), Value::Int(i64::MIN)),
                ("scale".into(), Value::Int(100))
            ],
            vec![
                ("x".into(), Value::Int(7)),
                ("scale".into(), Value::Int(100))
            ],
            vec![
                ("x".into(), Value::Int(i64::MIN)),
                ("scale".into(), Value::Int(100))
            ],
            vec![
                ("x".into(), Value::Int(7)),
                ("scale".into(), Value::Int(100))
            ],
        ]
    );
}

#[test]
fn every_signature_mismatch_precedes_even_schema_cardinality_and_empty_table_observations() {
    let plan = plan(
        signature(),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    for values in [vec![], vec![Value::Int(7)]] {
        for incompatible in [
            None,
            Some(
                ProjectInvocationPlan::new(
                    ProjectFunctionIdentity {
                        name: "other".into(),
                        call: "installed.identity".into(),
                    },
                    signature().signature().parameters().to_vec(),
                    ColumnType::Int,
                    false,
                )
                .unwrap(),
            ),
            Some(
                ProjectInvocationPlan::new(
                    signature().identity().clone(),
                    signature().signature().parameters().to_vec(),
                    ColumnType::Float,
                    false,
                )
                .unwrap(),
            ),
        ] {
            let source = table(values.clone());
            let mut callbacks = callbacks();
            callbacks.signature = incompatible;
            let error = plan
                .execute_observed_functions(&source, &[], &mut callbacks, limits())
                .result
                .unwrap_err();
            assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
            assert!(source.observations.borrow().is_empty());
            assert!(callbacks.calls.is_empty());
        }
    }
}

#[test]
fn present_missing_short_circuits_without_host_call_or_default_substitution() {
    let source = table(vec![Value::Missing]);
    let plan = plan(
        signature(),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    let mut callbacks = callbacks();
    let result = plan
        .execute_observed_functions(&source, &[], &mut callbacks, limits())
        .result
        .unwrap();
    assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Missing);
    assert!(callbacks.calls.is_empty());
}

#[test]
fn original_host_failure_keeps_versionless_identity_and_row_without_retry_or_output() {
    let source = table(vec![Value::Int(7), Value::Int(8)]);
    let plan = plan(
        signature(),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    let mut callbacks = callbacks();
    callbacks.result = Err("original owned host payload");
    let error = plan
        .execute_observed_functions(&source, &[], &mut callbacks, limits())
        .result
        .unwrap_err();
    let ExecutionError::ProjectFunction {
        path,
        error,
        identity,
    } = *error
    else {
        panic!("project invocation facts")
    };
    assert_eq!(path, "columns.C1.derivation.function");
    assert_eq!(error.identity, signature().identity().clone());
    assert_eq!(
        error.kind,
        FailureKind::CallFailed("original owned host payload")
    );
    assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
    assert_eq!(callbacks.calls.len(), 1);
    assert!(!source
        .observations
        .borrow()
        .iter()
        .any(|s| s.starts_with("cell:1:")));
}

#[test]
fn versionless_call_admission_preserves_closed_names_and_group_completed_output_scope() {
    for args in [
        vec![],
        vec![
            argument(FunctionInput::Literal(Value::Int(7))),
            argument(FunctionInput::Literal(Value::Int(8))),
        ],
        vec![FunctionArgument {
            name: "unknown".into(),
            input: FunctionInput::Literal(Value::Int(7)),
        }],
    ] {
        assert_eq!(
            plan(signature(), args, RowMode::Records),
            Err(PlanError::InvalidFunction)
        );
    }
    assert_eq!(
        plan(
            signature(),
            vec![argument(FunctionInput::Read(Read::Column(1)))],
            RowMode::Records
        ),
        Err(PlanError::UnavailableColumn)
    );
    assert_eq!(
        plan(
            signature(),
            vec![argument(FunctionInput::Read(Read::Source(1)))],
            RowMode::Groups(vec![0])
        ),
        Err(PlanError::NonGroupSource)
    );
}

#[test]
fn opposite_zero_default_is_a_binding_mismatch_before_any_table_or_callback_effect() {
    let make = |zero| {
        let mut parameters = signature().signature().parameters().to_vec();
        parameters[1].kind = ValueType::Float;
        parameters[1].presence = Presence::Optional(Value::float(zero));
        ProjectInvocationPlan::new(
            signature().identity().clone(),
            parameters,
            ColumnType::Int,
            false,
        )
        .unwrap()
    };
    for (compiled, activated) in [(-0.0, 0.0), (0.0, -0.0)] {
        let plan = plan(
            make(compiled),
            vec![argument(FunctionInput::Read(Read::Source(1)))],
            RowMode::Records,
        )
        .unwrap();
        let source = table(vec![Value::Int(7)]);
        let mut callbacks = callbacks();
        callbacks.signature = Some(make(activated));
        let error = plan
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
            .result
            .unwrap_err();
        assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
        assert!(source.observations.borrow().is_empty());
        assert!(callbacks.calls.is_empty());
    }
}

fn union_signature(name: &str) -> ProjectInvocationPlan {
    ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: name.into(),
            call: format!("installed.{name}"),
        },
        signature().signature().parameters().to_vec(),
        ColumnType::Int,
        false,
    )
    .unwrap()
}
fn union() -> [ActivatedFunction<&'static str>; 2] {
    ["producer", "consumer"].map(|name| ActivatedFunction {
        plan: union_signature(name),
        handle: name,
        cases: vec![],
    })
}
#[derive(Default)]
struct UnionPort {
    calls: Vec<&'static str>,
    failure: bool,
}
impl ActivationPort for UnionPort {
    type Handle = &'static str;
    type Error = &'static str;
    fn verify_lock(
        &mut self,
        _: yamaa_core::project_function::Language,
        _: &yamaa_core::project_environment::LockReference,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), Self::Error> {
        panic!("projected handles must not reactivate")
    }
    fn bind(
        &mut self,
        _: &ProjectFunctionIdentity,
        _: &yamaa_core::function_signature::LogicalSignature,
    ) -> Result<Self::Handle, Self::Error> {
        panic!("projected handles must not rebind")
    }
    fn invoke(
        &mut self,
        handle: &Self::Handle,
        _: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        self.calls.push(*handle);
        if self.failure {
            Err(HostError::Raised("original union host failure"))
        } else {
            Ok(Value::Int(match *handle {
                "producer" => 71,
                "consumer" => 19,
                _ => panic!("independently declared handle"),
            }))
        }
    }
}

#[test]
fn different_nodes_local_zero_projects_to_their_exact_union_handle() {
    let activated = union();
    let mut port = UnionPort::default();
    // Consumer is local slot zero but graph slot one; producer is graph slot
    // zero. Distinct host results make accidental identity projection observable.
    for (name, global, expected) in [("consumer", 1, 19), ("producer", 0, 71)] {
        let node_plan = plan(
            union_signature(name),
            vec![argument(FunctionInput::Read(Read::Source(1)))],
            RowMode::Records,
        )
        .unwrap();
        let source = table(vec![Value::Int(7)]);
        let projection = [global];
        let plans = [union_signature(name)];
        let mut bindings = Bindings::projected(&activated, &projection, &plans, &mut port).unwrap();
        assert!(bindings.project_signature(1).is_none());
        let result = node_plan
            .execute_observed_functions(&source, &[], &mut bindings, limits())
            .result
            .unwrap();
        assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Int(expected));
    }
    assert_eq!(port.calls, ["consumer", "producer"]);
}

#[test]
fn projection_admission_rejects_shape_slot_identity_and_complete_signature_without_host_effects() {
    let activated = union();
    let consumer = [union_signature("consumer")];
    for projection in [vec![], vec![0], vec![2], vec![1, 0]] {
        let mut port = UnionPort::default();
        assert!(Bindings::projected(&activated, &projection, &consumer, &mut port).is_err());
        assert!(port.calls.is_empty());
    }
    let changed = ProjectInvocationPlan::new(
        consumer[0].identity().clone(),
        consumer[0].signature().parameters().to_vec(),
        ColumnType::Float,
        false,
    )
    .unwrap();
    let mut port = UnionPort::default();
    assert!(matches!(
        Bindings::projected(&activated, &[1], &[changed], &mut port),
        Err(0)
    ));
    assert!(port.calls.is_empty());
}

#[test]
fn projected_handles_still_preflight_the_actual_node_before_any_table_observation() {
    let activated = union();
    let expected = [union_signature("producer")];
    let mut port = UnionPort::default();
    let mut bindings = Bindings::projected(&activated, &[0], &expected, &mut port).unwrap();
    let consumer = plan(
        union_signature("consumer"),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    let source = table(vec![Value::Int(7)]);
    let error = consumer
        .execute_observed_functions(&source, &[], &mut bindings, limits())
        .result
        .unwrap_err();
    assert_eq!(*error, ExecutionError::FunctionBinding { slot: 0 });
    assert!(source.observations.borrow().is_empty());
    assert!(port.calls.is_empty());
}

#[test]
fn projected_invocations_preserve_missing_short_circuit_and_original_failure_without_replay() {
    let activated = union();
    let expected = [union_signature("consumer")];
    let node_plan = plan(
        union_signature("consumer"),
        vec![argument(FunctionInput::Read(Read::Source(1)))],
        RowMode::Records,
    )
    .unwrap();
    let mut port = UnionPort::default();
    let source = table(vec![Value::Missing]);
    let mut bindings = Bindings::projected(&activated, &[1], &expected, &mut port).unwrap();
    let result = node_plan
        .execute_observed_functions(&source, &[], &mut bindings, limits())
        .result
        .unwrap();
    assert_eq!(result.dataset.cell(0, 1).unwrap(), ValueRef::Missing);
    assert!(port.calls.is_empty());

    port.failure = true;
    let mut bindings = Bindings::projected(&activated, &[1], &expected, &mut port).unwrap();
    let source = table(vec![Value::Int(7), Value::Int(8)]);
    let error = node_plan
        .execute_observed_functions(&source, &[], &mut bindings, limits())
        .result
        .unwrap_err();
    assert!(
        matches!(*error, ExecutionError::ProjectFunction { error, .. }
        if error.kind == FailureKind::CallFailed("original union host failure"))
    );
    assert_eq!(port.calls, ["consumer"]);
}
