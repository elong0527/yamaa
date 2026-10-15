//! Independent private-constructor controls for augmented source field timing.
use super::*;
use crate::{
    bound_expression::Binding,
    function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
    table::Column,
    value::{ColumnType, ValueType},
};
use alloc::format;
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
fn assignment(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("intermediates[0].derivations.C{column}"),
    }
}
fn model(expression: Expression) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new_donor(
        SourceSchemas {
            primary: schema(&["STORED", "FIRST", "LATER", "CURRENT"]),
            secondary: vec![],
        },
        vec![],
        schema(&["STORED", "FIRST", "LATER"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            filter: None,
            assignments: vec![assignment(0, Expression::Source(0))],
        }],
        vec![
            assignment(1, expression),
            assignment(2, Expression::Literal(Value::Int(5))),
        ],
    )
}
fn predicate() -> crate::predicate::Plan {
    crate::predicate_compiler::compile("LATER > 0", "donor.when", Default::default()).unwrap()
}
fn numeric(column: usize) -> Expression {
    Expression::Compute(
        BoundNumeric::new(
            crate::numeric_compiler::compile_numeric(
                "LATER + 1",
                "donor.compute.expr",
                Default::default(),
            )
            .unwrap(),
            vec![Binding {
                name: "LATER".into(),
                read: Read::Source(column),
            }],
        )
        .unwrap(),
    )
}
fn function(column: usize) -> Expression {
    Expression::Function(
        BoundFunction::new(
            0,
            InvocationPlan::new(
                FunctionIdentity {
                    name: "identity".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "identity".into(),
                },
                vec![Parameter {
                    name: "value".into(),
                    host_name: "value".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                }],
                ColumnType::Int,
                false,
            )
            .unwrap(),
            vec![FunctionArgument {
                name: "value".into(),
                input: FunctionInput::Read(Read::Source(column)),
            }],
        )
        .unwrap(),
    )
}
#[test]
fn later_augmented_source_fields_fail_before_a_donor_plan_can_execute() {
    let flag = Expression::Flag(
        Flag::new(
            predicate(),
            vec![FlagBinding {
                name: "LATER".into(),
                read: FlagRead::Read(Read::Source(2)),
            }],
            Value::Int(1),
            None,
            None,
        )
        .unwrap(),
    );
    let case = Expression::Case(vec![CaseBranch {
        when: Some(
            BoundPredicate::new(
                predicate(),
                vec![Binding {
                    name: "LATER".into(),
                    read: Read::Source(2),
                }],
            )
            .unwrap(),
        ),
        assignment: alloc::boxed::Box::new(assignment(1, Expression::Literal(Value::Int(1)))),
    }]);
    let nested = Expression::StrCase {
        source: alloc::boxed::Box::new(assignment(1, numeric(2))),
        operation: StringCase::new(
            "LATER".into(),
            "donor.case".into(),
            crate::text_case::Case::Upper,
            None,
        )
        .unwrap(),
    };
    for expression in [
        Expression::Source(2),
        numeric(2),
        function(2),
        flag,
        case,
        nested,
    ] {
        assert_eq!(model(expression), Err(PlanError::UnavailableColumn));
    }
}
#[test]
fn stored_initialization_and_appended_current_operands_remain_admitted() {
    for expression in [
        Expression::Source(0),
        numeric(0),
        function(0),
        Expression::Source(3),
        numeric(3),
        function(3),
    ] {
        assert!(model(expression).is_ok());
    }
}
