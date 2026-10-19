//! A match expression keeps its complete authored identifiers.
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

#[test]
fn match_identifiers_keep_the_complete_authored_name() {
    let name = "x".repeat(65_537);
    let schema = schema(&name);
    let expression = MatchExpression::new(
        Assignment {
            column: 1,
            ..assignment(Expression::Source(0))
        },
        vec![MatchRead::Record {
            column: 0,
            identifier: name.clone(),
        }],
        schema.clone(),
        crate::result_type::ResultType::INT,
    )
    .unwrap();
    assert_eq!(expression.reads[0].identifier(), name);
}
