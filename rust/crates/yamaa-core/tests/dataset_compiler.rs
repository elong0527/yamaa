//! The temporary decoded vocabulary compiles in core, without an engine or IO.
use yamaa_core::{
    bound_expression::{Binding, Read},
    dataset::{
        compile_dataset, AssignmentDeclaration as A, DatasetCompileError as Error,
        DatasetCompileLimits, DatasetDeclaration, Expression, ExpressionDeclaration as D,
        FunctionArgument, FunctionInput, PlanError, RowMode, SourceSchemas, TemplateDeclaration,
        Window, WindowKind,
    },
    function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
    numeric_compiler::CompileError,
    table::{Column, TableSchema},
    value::{ColumnType, Value, ValueType},
};

fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
fn assignment(column: usize, expression: D) -> A {
    A {
        column,
        path: format!("columns.C{column}.derivation"),
        expression,
    }
}
fn declaration(expression: D) -> DatasetDeclaration {
    DatasetDeclaration {
        sources: SourceSchemas {
            primary: schema(&["ID"]),
            secondary: vec![],
        },
        intermediates: vec![],
        output: schema(&["ID", "VALUE"]),
        templates: vec![TemplateDeclaration {
            mode: RowMode::Keys,
            assignments: vec![assignment(0, D::Source(0))],
            filter: None,
        }],
        columns: vec![assignment(1, expression)],
        keys: vec![0],
        verifications: vec![],
        conversion_handlers: vec![],
        functions: vec![],
    }
}
fn compute(text: &str, name: &str, read: Read) -> D {
    D::Compute {
        text: text.into(),
        bindings: vec![Binding {
            name: name.into(),
            read,
        }],
    }
}
fn signature() -> InvocationPlan {
    InvocationPlan::new(
        FunctionIdentity {
            name: "identity".into(),
            contract_version: "1".into(),
            implementation_version: "1".into(),
            call: "identity".into(),
        },
        vec![Parameter {
            name: "value".into(),
            host_name: "x".into(),
            kind: ValueType::Int,
            accepts_missing: false,
            presence: Presence::Required,
        }],
        ColumnType::Int,
        false,
    )
    .unwrap()
}
fn call(slot: usize) -> D {
    D::Function {
        slot,
        arguments: vec![FunctionArgument {
            name: "value".into(),
            input: FunctionInput::Read(Read::Column(0)),
        }],
    }
}

#[test]
fn compiling_preserves_deferred_arithmetic_and_authored_path() {
    let compiled = compile_dataset(
        declaration(compute("ID + 1 / 0", "ID", Read::Column(0))),
        Default::default(),
    )
    .unwrap();
    let Expression::Compute(numeric) = &compiled.plan().columns()[0].expression else {
        panic!("compiled numeric expression")
    };
    assert_eq!(
        numeric.bindings(),
        &[Binding {
            name: "ID".into(),
            read: Read::Column(0)
        }]
    );
    assert_eq!(numeric.expression().spec_path(), "columns.C1.derivation");
    // Division by zero remains execution work; compilation grants a valid plan.
    assert_eq!(compiled.plan().columns()[0].column, 1);
    assert!(compiled.functions().is_empty());
}

#[test]
fn grammar_binding_and_phase_errors_are_owned_by_pure_compilation() {
    assert!(matches!(
        compile_dataset(
            declaration(compute("ID +", "ID", Read::Column(0))),
            Default::default()
        ),
        Err(Error::Numeric(CompileError::Parse(_)))
    ));
    assert!(matches!(
        compile_dataset(
            declaration(compute("ID + 1", "OTHER", Read::Column(0))),
            Default::default()
        ),
        Err(Error::Plan(PlanError::InvalidColumns))
    ));
    assert!(matches!(
        compile_dataset(
            declaration(compute("ID + 1", "ID", Read::Column(1))),
            Default::default()
        ),
        Err(Error::Plan(PlanError::UnavailableColumn))
    ));
    assert!(matches!(
        compile_dataset(declaration(call(0)), Default::default()),
        Err(Error::Plan(PlanError::InvalidFunction))
    ));
}

#[test]
fn one_budget_covers_function_copies_across_row_and_column_phases() {
    let mut input = declaration(call(0));
    input.functions = vec![signature()];
    input.templates[0].assignments[0].expression = D::Function {
        slot: 0,
        arguments: vec![FunctionArgument {
            name: "value".into(),
            input: FunctionInput::Literal(Value::Int(1)),
        }],
    };
    input.templates[0].mode = RowMode::Records;
    let policy = DatasetCompileLimits {
        copied_parameters: 1,
        ..Default::default()
    };
    assert!(matches!(
        compile_dataset(input, policy),
        Err(Error::Limit("copied parameters"))
    ));
    let mut input = declaration(call(0));
    input.functions = vec![signature()];
    let policy = DatasetCompileLimits {
        copied_text_bytes: 1,
        ..Default::default()
    };
    assert!(matches!(
        compile_dataset(input, policy),
        Err(Error::Limit("copied text"))
    ));
    let mut input = declaration(call(0));
    input.functions = vec![signature()];
    let compiled = compile_dataset(input, Default::default()).unwrap();
    let Expression::Function(function) = &compiled.plan().columns()[0].expression else {
        panic!("compiled function")
    };
    assert_eq!(function.slot(), 0);
    assert_eq!(function.signature(), &signature());
    assert_eq!(compiled.functions(), &[signature()]);
}

#[test]
fn legacy_number_and_value_window_forms_have_one_core_admission_gate() {
    let window = |kind| Window {
        kind,
        group_by: vec![],
        order_by: vec![],
        filter: None,
    };
    assert!(matches!(
        compile_dataset(
            declaration(D::Number(window(WindowKind::Locf { column: 0 }))),
            Default::default()
        ),
        Err(Error::Plan(PlanError::InvalidWindow))
    ));
    assert!(matches!(
        compile_dataset(
            declaration(D::Window(window(WindowKind::Dense))),
            Default::default()
        ),
        Err(Error::Plan(PlanError::InvalidWindow))
    ));
}

#[test]
fn invalid_recovery_never_exposes_a_partly_admitted_plan() {
    let mut input = declaration(D::Literal(Value::Int(1)));
    input
        .conversion_handlers
        .push(yamaa_core::dataset::ConversionHandler {
            assignment_path: "columns.ABSENT.derivation".into(),
            handler: yamaa_core::conversion::LiteralHandler {
                spec_path: "columns.ABSENT.unconvertible".into(),
                value: Value::Int(0),
            },
        });
    assert!(matches!(
        compile_dataset(input, Default::default()),
        Err(Error::Plan(PlanError::InvalidConversionHandler))
    ));
}

#[test]
fn ignored_draft_faults_and_unused_reservations_cannot_publish_a_complete_prefix() {
    use yamaa_core::dataset::{DatasetCompiler, DatasetMetadata};
    let metadata = || {
        let input = declaration(D::Literal(Value::Int(1)));
        DatasetMetadata {
            sources: input.sources,
            intermediates: input.intermediates,
            output: input.output,
            keys: input.keys,
            verifications: input.verifications,
            conversion_handlers: input.conversion_handlers,
        }
    };
    let complete_prefix = || {
        let mut draft = DatasetCompiler::new(vec![signature()], Default::default()).unwrap();
        draft.begin_template(RowMode::Keys, None).unwrap();
        draft
            .template_assignment(assignment(0, D::Source(0)))
            .unwrap();
        draft
            .column_assignment(assignment(1, D::Literal(Value::Int(1))))
            .unwrap();
        draft
    };
    let mut draft = complete_prefix();
    assert!(draft
        .column_assignment(assignment(1, compute("ID +", "ID", Read::Column(0))))
        .is_err());
    assert!(matches!(
        draft.finish(metadata()),
        Err(Error::Plan(PlanError::InvalidColumns))
    ));
    let mut draft = complete_prefix();
    assert!(draft.reserve_function(999).is_err());
    assert!(draft.finish(metadata()).is_err());
    let mut draft = complete_prefix();
    draft.reserve_function(0).unwrap();
    assert!(matches!(
        draft.finish(metadata()),
        Err(Error::Plan(PlanError::InvalidFunction))
    ));
    let mut draft = complete_prefix();
    draft.reserve_numeric("ID + 1", "authored").unwrap();
    assert!(matches!(
        draft.finish(metadata()),
        Err(Error::Plan(PlanError::InvalidColumns))
    ));
    let mut draft = complete_prefix();
    draft.reserve_numeric("ID + 1", "authored").unwrap();
    assert!(draft
        .column_assignment(assignment(1, compute("ID + 1", "ID", Read::Column(0))))
        .is_err());
    assert!(draft.finish(metadata()).is_err());
}
