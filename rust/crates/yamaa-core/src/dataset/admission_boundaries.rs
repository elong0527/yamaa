//! Structural admission keeps broad expressions and full authored identities.
use super::*;
use crate::{table::Column, value::ColumnType};

fn schema(name: &str) -> TableSchema {
    TableSchema::new(vec![Column {
        name: name.into(),
        kind: ColumnType::Int,
    }])
    .unwrap()
}

fn assignment(expression: Expression) -> Assignment {
    Assignment {
        column: 0,
        expression,
        path: "columns.V.derivation".into(),
    }
}

fn plan(expression: Expression) -> Result<DatasetPlan, PlanError> {
    DatasetPlan::new(
        schema("V"),
        schema("V"),
        vec![RowTemplate {
            mode: RowMode::Records,
            filter: None,
            assignments: vec![assignment(expression)],
        }],
        vec![],
        vec![],
        vec![],
    )
}

#[test]
fn broad_scalar_expressions_have_no_pooled_node_allowance() {
    let operands = (0..65_537)
        .map(|_| assignment(Expression::Literal(Value::Int(1))))
        .collect();
    assert!(plan(Expression::Extreme {
        operands,
        largest: true,
    })
    .is_ok());
}

#[test]
fn expression_nesting_uses_the_fixed_semantic_boundary() {
    for depth in [128, 129] {
        let mut expression = Expression::Literal(Value::Int(1));
        for _ in 0..depth {
            expression = Expression::Extreme {
                operands: vec![assignment(expression)],
                largest: true,
            };
        }
        match plan(expression) {
            Ok(_) => assert_eq!(depth, 128),
            Err(error) => {
                assert_eq!(depth, 129);
                assert_eq!(error, PlanError::InvalidColumns);
            }
        }
    }
}

#[test]
fn match_identifiers_keep_the_complete_authored_name() {
    let name = "x".repeat(65_537);
    let schema = schema(&name);
    let expression = MatchExpression::new(
        assignment(Expression::Source(0)),
        vec![MatchRead::Record {
            column: 0,
            identifier: name.clone(),
        }],
        schema.clone(),
        Some(ColumnType::Int.into()),
    )
    .unwrap();
    expression
        .validate(&schema, &[], &schema, &[false])
        .unwrap();
    assert_eq!(expression.reads[0].identifier(), name);
}
