use std::{cell::Cell, convert::Infallible};
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, DatasetExecution, DatasetPlan, ExecutionBudget, ExecutionError, Expression,
    RowMode, RowTemplate, SecondarySource,
};

struct Source {
    schema: TableSchema,
    rows: usize,
    reads: Cell<usize>,
}

impl Source {
    fn new(rows: usize, columns: usize) -> Self {
        Self {
            schema: TableSchema::new(
                (0..columns)
                    .map(|column| Column {
                        name: format!("C{column}"),
                        kind: ColumnType::Int,
                    })
                    .collect(),
            )
            .unwrap(),
            rows,
            reads: Cell::new(0),
        }
    }
}

impl TableAccess for Source {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.set(self.reads.get() + 1);
        if row >= self.rows || column >= self.schema.columns().len() {
            return Err(CellError::OutOfBounds { row, column });
        }
        Ok(ValueRef::Int(i64::try_from(row).unwrap()))
    }
}

fn plan(source: &Source, secondary: usize) -> DatasetPlan {
    DatasetPlan::new_with_sources(
        source.schema.clone(),
        (0..secondary)
            .map(|index| SecondarySource {
                name: format!("S{index}"),
                schema: source.schema.clone(),
            })
            .collect(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: (0..source.schema.columns().len())
                .map(|column| Assignment {
                    column,
                    expression: Expression::Source(column),
                    path: format!("columns.C{column}"),
                })
                .collect(),
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}

#[test]
fn source_shape_refusals_precede_reads_and_do_not_spend_future_budgets() {
    let source = Source::new(2, 1);
    let secondary = Source::new(2, 1);
    let plan = plan(&source, 1);
    let budget = ExecutionBudget::default();
    for tiny in [
        ExecutionBudget {
            max_sources: 1,
            ..budget
        },
        ExecutionBudget {
            max_decoded_cells: 3,
            ..budget
        },
        ExecutionBudget {
            max_columns: 0,
            ..budget
        },
        ExecutionBudget {
            execution: yamaa_engine::dataset::Limits {
                source_rows: 3,
                ..budget.execution
            },
            ..budget
        },
    ] {
        let result = plan
            .execute_with_budget(&source, &[&secondary], tiny)
            .result;
        assert!(matches!(*result.unwrap_err(), ExecutionError::Capacity));
        assert_eq!(source.reads.get(), 0);
        assert_eq!(secondary.reads.get(), 0);
    }
    for _ in 0..2 {
        assert_eq!(
            plan.execute_with_budget(&source, &[&secondary], budget)
                .result
                .unwrap()
                .dataset
                .rows(),
            &[vec![Value::Int(0)], vec![Value::Int(1)]]
        );
    }
}

#[test]
fn checked_multiplication_and_combined_addition_refuse_overflow_before_reads() {
    let mut budget = ExecutionBudget::longitudinal();
    budget.execution.source_rows = usize::MAX;
    budget.max_decoded_cells = usize::MAX;
    let source = Source::new(usize::MAX, 2);
    let result = plan(&source, 0)
        .execute_with_budget(&source, &[], budget)
        .result;
    assert!(matches!(*result.unwrap_err(), ExecutionError::Capacity));
    assert_eq!(source.reads.get(), 0);
    let source = Source::new(usize::MAX, 1);
    let secondary = Source::new(1, 1);
    let result = plan(&source, 1)
        .execute_with_budget(&source, &[&secondary], budget)
        .result;
    assert!(matches!(*result.unwrap_err(), ExecutionError::Capacity));
    assert_eq!(source.reads.get(), 0);
    assert_eq!(secondary.reads.get(), 0);
}

#[test]
fn longitudinal_row_quota_still_refuses_the_next_record() {
    let source = Source::new(10_000_001, 1);
    let result = plan(&source, 0)
        .execute_with_budget(&source, &[], ExecutionBudget::longitudinal())
        .result;
    assert!(matches!(*result.unwrap_err(), ExecutionError::Capacity));
    assert_eq!(source.reads.get(), 0);
}
