use std::{cell::Cell, convert::Infallible};
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::{
    Assignment, DatasetExecution, DatasetPlan, ExecutionBudget, ExecutionError, Expression,
    Resource, RowMode, RowTemplate, SecondarySource,
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
    for (tiny, resource, limit, required) in [
        (
            ExecutionBudget {
                max_sources: 1,
                ..budget
            },
            Resource::SourceCount,
            1,
            Some(2),
        ),
        (
            ExecutionBudget {
                max_decoded_cells: 3,
                ..budget
            },
            Resource::SourceCells,
            3,
            Some(4),
        ),
        (
            ExecutionBudget {
                max_columns: 0,
                ..budget
            },
            Resource::SourceColumns,
            0,
            Some(1),
        ),
        (
            ExecutionBudget {
                execution: yamaa_engine::dataset::Limits {
                    source_rows: 3,
                    ..budget.execution
                },
                ..budget
            },
            Resource::SourceRows,
            3,
            Some(4),
        ),
    ] {
        let result = plan
            .execute_with_budget(&source, &[&secondary], tiny)
            .result;
        let expected = if resource == Resource::SourceColumns {
            ExecutionError::SourceLimit {
                resource,
                limit,
                required,
                source_index: 0,
            }
        } else {
            ExecutionError::Limit {
                resource,
                limit,
                required,
            }
        };
        assert_eq!(*result.unwrap_err(), expected);
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
fn secondary_width_refusal_retains_only_the_refused_source_coordinate() {
    let primary = Source::new(1, 1);
    let narrow = Source::new(1, 1);
    let wide = Source::new(1, 2);
    let plan = DatasetPlan::new_with_sources(
        primary.schema.clone(),
        [&narrow, &wide]
            .into_iter()
            .enumerate()
            .map(|(index, source)| SecondarySource {
                name: format!("S{index}"),
                schema: source.schema.clone(),
            })
            .collect(),
        primary.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![Assignment {
                column: 0,
                expression: Expression::Source(0),
                path: "columns.C0".into(),
            }],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let result = plan.execute_with_budget(
        &primary,
        &[&narrow, &wide],
        ExecutionBudget {
            max_columns: 1,
            ..ExecutionBudget::default()
        },
    );
    assert_eq!(
        *result.result.unwrap_err(),
        ExecutionError::SourceLimit {
            resource: Resource::SourceColumns,
            limit: 1,
            required: Some(2),
            source_index: 2,
        }
    );
    assert_eq!(primary.reads.get(), 0);
    assert_eq!(narrow.reads.get(), 0);
    assert_eq!(wide.reads.get(), 0);
    assert!(plan
        .execute_with_budget(&primary, &[&narrow, &wide], ExecutionBudget::default())
        .result
        .is_ok());
}

#[test]
fn bounded_column_width_admits_1024_and_refuses_1025_before_cell_reads() {
    for width in [1_024, 1_025] {
        let source = Source::new(1, width);
        let result = plan(&source, 0)
            .execute_with_budget(&source, &[], ExecutionBudget::bounded())
            .result;
        if width == 1_024 {
            assert_eq!(
                result.unwrap().dataset.rows(),
                &[vec![Value::Int(0); 1_024]]
            );
            assert_eq!(source.reads.get(), 1_024);
        } else {
            assert_eq!(
                *result.unwrap_err(),
                ExecutionError::SourceLimit {
                    resource: Resource::SourceColumns,
                    limit: 1_024,
                    required: Some(1_025),
                    source_index: 0,
                }
            );
            assert_eq!(source.reads.get(), 0);
        }
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
    assert!(matches!(*result.unwrap_err(), ExecutionError::Limit { .. }));
    assert_eq!(source.reads.get(), 0);
    let source = Source::new(usize::MAX, 1);
    let secondary = Source::new(1, 1);
    let result = plan(&source, 1)
        .execute_with_budget(&source, &[&secondary], budget)
        .result;
    assert!(matches!(*result.unwrap_err(), ExecutionError::Limit { .. }));
    assert_eq!(source.reads.get(), 0);
    assert_eq!(secondary.reads.get(), 0);
}

#[test]
fn explicitly_selected_bounded_profile_still_refuses_its_original_cell_frontier() {
    let source = Source::new(2_049, 1_024);
    let result = plan(&source, 0)
        .execute_with_budget(&source, &[], ExecutionBudget::bounded())
        .result;
    assert_eq!(
        *result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::SourceCells,
            limit: 2_097_152,
            required: Some(2_098_176),
        }
    );
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn longitudinal_row_quota_still_refuses_the_next_record() {
    let source = Source::new(10_000_001, 1);
    let result = plan(&source, 0)
        .execute_with_budget(&source, &[], ExecutionBudget::longitudinal())
        .result;
    assert!(matches!(*result.unwrap_err(), ExecutionError::Limit { .. }));
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn wider_output_refusal_names_output_columns_without_attributing_it_to_input() {
    let source = Source::new(1, 1);
    let output = Source::new(1, 2);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        output.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    expression: Expression::Source(0),
                    path: "columns.C0".into(),
                },
                Assignment {
                    column: 1,
                    expression: Expression::Literal(Value::Int(1)),
                    path: "columns.C1".into(),
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let result = plan
        .execute_with_budget(
            &source,
            &[],
            ExecutionBudget {
                max_columns: 1,
                ..ExecutionBudget::bounded()
            },
        )
        .result;
    assert!(
        matches!(*result.unwrap_err(), ExecutionError::ScopedLimit { resource: Resource::OutputColumns, limit: 1, required: Some(2), spec_paths } if spec_paths == ["output.columns"])
    );
    assert_eq!(source.reads.get(), 0);
}
