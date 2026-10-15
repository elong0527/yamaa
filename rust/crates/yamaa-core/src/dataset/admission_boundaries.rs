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
        vec![0],
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
        Assignment {
            column: 1,
            ..assignment(Expression::Source(0))
        },
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

#[test]
fn contextual_window_table_boundary_keeps_its_exact_shape_and_authored_owner() {
    use crate::{
        function_invocation::NoFunctions,
        table::{CellError, TableAccess, ValueRef},
    };
    use std::{cell::Cell, convert::Infallible};
    struct Source {
        schema: TableSchema,
        reads: Cell<usize>,
    }
    impl TableAccess for Source {
        type Error = Infallible;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            1
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
            assert_eq!((row, column), (0, 0));
            self.reads.set(self.reads.get() + 1);
            Ok(ValueRef::Int(7))
        }
    }
    for output_columns in [1023, 1024] {
        let source = Source {
            schema: schema("V"),
            reads: Cell::new(0),
        };
        let output = TableSchema::new(
            (0..output_columns)
                .map(|index| Column {
                    name: if index + 1 == output_columns {
                        "W".into()
                    } else {
                        format!("C{index}")
                    },
                    kind: ColumnType::Int,
                })
                .collect(),
        )
        .unwrap();
        let plan = DatasetPlan::new(
            source.schema.clone(),
            output,
            vec![RowTemplate {
                mode: RowMode::Records,
                filter: None,
                assignments: (0..output_columns - 1)
                    .map(|column| Assignment {
                        column,
                        expression: Expression::Literal(Value::Int(1)),
                        path: format!("columns.C{column}"),
                    })
                    .collect(),
            }],
            vec![Assignment {
                column: output_columns - 1,
                path: "columns.W.derivation.row_number".into(),
                expression: Expression::ContextWindow(ContextWindow {
                    window: Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: output_columns,
                            descending: false,
                            nulls_first: true,
                        }],
                        filter: None,
                    },
                    sources: vec![("SRC.V".into(), 0)],
                }),
            }],
            vec![0],
            vec![],
        )
        .unwrap();
        let attempted = plan.execute(&source, &[], &mut NoFunctions::new(), None);
        if output_columns == 1023 {
            let executed = attempted.result.unwrap();
            assert_eq!(executed.dataset.rows()[0].len(), 1023);
            assert_eq!(executed.dataset.rows()[0][1022], Value::Int(1));
            assert_eq!(source.reads.get(), 1);
        } else {
            assert_eq!(
                *attempted.result.unwrap_err(),
                ExecutionError::ScopedLimit {
                    resource: Resource::SourceColumns,
                    limit: 1024,
                    required: Some(1025),
                    spec_paths: vec!["columns.W.derivation.row_number".into()],
                }
            );
            assert_eq!(source.reads.get(), 0);
        }
    }
}
