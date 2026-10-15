use std::{cell::Cell, convert::Infallible};
use yamaa_core::{function_invocation::NoFunctions, Limits};
use yamaa_core::dataset::{
    Assignment, DatasetPlan, ExecutionError, Expression, Resource, RowMode,
    RowTemplate, SecondarySource,
};
use yamaa_core::{
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
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
        assert!(self.rows <= Limits::TABLE_ROWS);
        assert!(self.rows.checked_mul(self.schema.columns().len()).is_some_and(|cells| cells <= Limits::TABLE_CELLS));
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
fn source_count_is_admitted_before_reads_and_repeated_attempts_are_independent() {
    for secondary_count in [63, 64] {
        let primary = Source::new(2, 1);
        let secondary = Source::new(2, 1);
        let plan = plan(&primary, secondary_count);
        let inputs = vec![&secondary as &dyn TableAccess<Error = Infallible>; secondary_count];
        for _ in 0..2 {
            let attempted = plan.execute(&primary, &inputs, &mut NoFunctions::new(), None);
            if secondary_count == 63 {
                assert_eq!(attempted.result.unwrap().dataset.rows(), &[vec![Value::Int(0)], vec![Value::Int(1)]]);
            } else {
                assert_eq!(*attempted.result.unwrap_err(), ExecutionError::Limit {
                    resource: Resource::SourceCount, limit: 64, required: Some(65),
                });
                assert_eq!(primary.reads.get(), 0);
            }
            assert_eq!(secondary.reads.get(), 0);
        }
    }
}

#[test]
fn row_and_cell_geometry_refusals_precede_all_cell_reads() {
    for (rows, columns, resource, limit, required) in [
        (10_000_001, 1, Resource::SourceRows, 10_000_000, 10_000_001),
        (3_575_419, 179, Resource::SourceCells, 640_000_000, 640_000_001),
        (usize::MAX, 2, Resource::SourceRows, 10_000_000, usize::MAX),
    ] {
        let primary = Source::new(rows, columns);
        let secondary = Source::new(1, columns);
        let attempted = plan(&primary, 1).execute(&primary, &[&secondary], &mut NoFunctions::new(), None);
        assert_eq!(*attempted.result.unwrap_err(), ExecutionError::SourceLimit {
            resource, limit, required: Some(required), source_index: 0,
        });
        assert_eq!(primary.reads.get(), 0);
        assert_eq!(secondary.reads.get(), 0);
    }
}

#[test]
fn secondary_width_refusal_retains_only_the_refused_source_coordinate() {
    for width in [1_024, 1_025] {
        let primary = Source::new(1, 1);
        let narrow = Source::new(1, 1);
        let wide = Source::new(1, width);
        let plan = DatasetPlan::new_with_sources(
            primary.schema.clone(),
            [&narrow, &wide].into_iter().enumerate().map(|(index, source)| SecondarySource {
                name: format!("S{index}"), schema: source.schema.clone(),
            }).collect(),
            primary.schema.clone(),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![Assignment {column: 0, expression: Expression::Source(0), path: "columns.C0".into()}],
                filter: None,
            }],
            vec![], vec![0], vec![],
        ).unwrap();
        let attempted = plan.execute(&primary, &[&narrow, &wide], &mut NoFunctions::new(), None);
        if width == 1_024 {
            assert_eq!(attempted.result.unwrap().dataset.rows(), &[vec![Value::Int(0)]]);
            assert_eq!(primary.reads.get(), 1);
        } else {
            assert_eq!(*attempted.result.unwrap_err(), ExecutionError::SourceLimit {
                resource: Resource::SourceColumns, limit: 1_024, required: Some(1_025), source_index: 2,
            });
            assert_eq!(primary.reads.get(), 0);
        }
        assert_eq!(narrow.reads.get(), 0);
        assert_eq!(wide.reads.get(), 0);
    }
}

#[test]
fn column_width_admits_1024_and_refuses_1025_before_cell_reads() {
    for width in [1_024, 1_025] {
        let source = Source::new(1, width);
        let attempted = plan(&source, 0).execute(&source, &[], &mut NoFunctions::new(), None);
        if width == 1_024 {
            assert_eq!(attempted.result.unwrap().dataset.rows(), &[vec![Value::Int(0); 1_024]]);
            assert_eq!(source.reads.get(), 1_024);
        } else {
            assert_eq!(*attempted.result.unwrap_err(), ExecutionError::SourceLimit {
                resource: Resource::SourceColumns, limit: 1_024, required: Some(1_025), source_index: 0,
            });
            assert_eq!(source.reads.get(), 0);
        }
    }
}

#[test]
fn wider_output_refusal_names_output_columns_without_attributing_it_to_input() {
    let source = Source::new(1, 1);
    let output = Source::new(1, 1_025);
    let plan = DatasetPlan::new(
        source.schema.clone(), output.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: (0..1_025).map(|column| Assignment {
                column,
                expression: if column == 0 { Expression::Source(0) } else { Expression::Literal(Value::Int(1)) },
                path: format!("columns.C{column}"),
            }).collect(),
            filter: None,
        }], vec![], vec![0], vec![],
    ).unwrap();
    let attempted = plan.execute(&source, &[], &mut NoFunctions::new(), None);
    assert!(matches!(*attempted.result.unwrap_err(), ExecutionError::ScopedLimit {
        resource: Resource::OutputColumns, limit: 1_024, required: Some(1_025), spec_paths
    } if spec_paths == ["output.columns"]));
    assert_eq!(source.reads.get(), 0);
}
