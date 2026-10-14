use crate::dataset::{DatasetExecution, Limits};
use alloc::{format, vec, vec::Vec};
use core::cell::RefCell;
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, DatasetPlan, Expression, OrderTerm, RowMode, RowTemplate, Window, WindowKind,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl TableAccess for Table {
    type Error = core::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        Ok((&self.rows[row][column]).into())
    }
}
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
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("row.C{column}"),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10_000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
fn plan(expression: &str, name: &str, column: usize) -> DatasetPlan {
    DatasetPlan::new(
        schema(&["ID"]),
        schema(&["ID", "SEQ", "ALIAS"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 0,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                ),
                assign(2, Expression::Column(1)),
            ],
            filter: Some(
                BoundPredicate::new(
                    yamaa_core::predicate_compiler::compile(
                        expression,
                        "row.filter",
                        Default::default(),
                    )
                    .unwrap(),
                    vec![Binding {
                        name: name.into(),
                        read: Read::Column(column),
                    }],
                )
                .unwrap(),
            ),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}

#[test]
fn grouped_window_filter_runs_once_after_complete_windows_without_renumbering() {
    for (expression, name, column, expected) in [
        (
            "SEQ > 1",
            "SEQ",
            1,
            vec![
                vec![Value::Int(2), Value::Int(2), Value::Int(2)],
                vec![Value::Int(3), Value::Int(3), Value::Int(3)],
            ],
        ),
        (
            "ALIAS > 1",
            "ALIAS",
            2,
            vec![
                vec![Value::Int(2), Value::Int(2), Value::Int(2)],
                vec![Value::Int(3), Value::Int(3), Value::Int(3)],
            ],
        ),
        (
            "ID > 1",
            "ID",
            0,
            vec![
                vec![Value::Int(2), Value::Int(1), Value::Int(1)],
                vec![Value::Int(3), Value::Int(2), Value::Int(2)],
            ],
        ),
    ] {
        let table = Table {
            schema: schema(&["ID"]),
            rows: [1, 1, 2, 3, 3]
                .into_iter()
                .map(|id| vec![Value::Int(id)])
                .collect(),
            reads: RefCell::default(),
        };
        assert_eq!(
            plan(expression, name, column)
                .execute(&table, limits())
                .unwrap()
                .dataset
                .rows(),
            expected
        );
        // Partitioning reads each source key, then each group borrows its first
        // ID exactly once. Both filter phases use completed output cells only.
        assert_eq!(
            *table.reads.borrow(),
            vec![
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (0, 0),
                (2, 0),
                (3, 0)
            ]
        );
    }
}
