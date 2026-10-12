//! Primary donors borrow the original table even while a secondary drives construction.
use std::cell::RefCell;
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource,
        MatchRead, MatchValue, RowMode, RowTemplate, SecondarySource, SourceSchemas,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, Limits};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    failure: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.failure == Some((row, column)) {
            return Err(CellError::Access("held primary failure"));
        }
        Ok((&self.rows[row][column]).into())
    }
}
fn schema(names: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("read.C{column}"),
    }
}

fn fixture(failure: Option<(usize, usize)>) -> (Table, Table, DatasetPlan) {
    let primary = Table {
        schema: schema(&[("ID", T::Int), ("V", T::Str)]),
        rows: vec![
            vec![Value::Int(1), Value::Str("x".into())],
            vec![Value::Int(2), Value::Str("y".into())],
        ],
        reads: RefCell::default(),
        failure,
    };
    let other = Table {
        schema: schema(&[("ID", T::Int)]),
        rows: vec![vec![Value::Int(2)], vec![Value::Int(1)]],
        reads: RefCell::default(),
        failure: None,
    };
    let item = Intermediate {
        identifier: "P".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Primary {
            schema: primary.schema.clone(),
            name: "SRC".into(),
        },
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: None,
        selection: None,
        no_match: None,

        range: None,
        columns: None,
    };
    let read = || Expression::Intermediate {
        index: 0,
        column: 1,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: primary.schema.clone(),
            secondary: vec![SecondarySource {
                name: "ALT".into(),
                schema: other.schema.clone(),
            }],
        },
        vec![item],
        schema(&[("ID", T::Int), ("A", T::Str), ("B", T::Str)]),
        vec![RowTemplate {
            mode: RowMode::Driver {
                source: 1,
                mode: Box::new(RowMode::Records),
            },
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, read()),
                assign(2, read()),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    (primary, other, plan)
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 100_000,
        scalar_text_bytes: 100_000,
        output_text_bytes: 100_000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
#[test]
fn one_selection_per_current_row_reads_the_original_relation_without_copies() {
    let (primary, other, plan) = fixture(None);
    let result = plan.execute_observed_sources(&primary, &[&other], limits());
    assert!(result.handler_counts.is_empty());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [
            vec![
                Value::Int(2),
                Value::Str("y".into()),
                Value::Str("y".into())
            ],
            vec![
                Value::Int(1),
                Value::Str("x".into()),
                Value::Str("x".into())
            ]
        ]
    );
    assert_eq!(
        *primary.reads.borrow(),
        [
            (0, 0),
            (1, 0),
            (1, 1),
            (1, 1),
            (0, 0),
            (1, 0),
            (0, 1),
            (0, 1)
        ]
    );
    assert_eq!(*other.reads.borrow(), [(0, 0), (1, 0)]);
}
#[test]
fn a_primary_donor_failure_keeps_its_original_owner_and_exact_read_prefix() {
    let (primary, other, plan) = fixture(Some((1, 0)));
    let result = plan.execute_observed_sources(&primary, &[&other], limits());
    let error = result.result.unwrap_err();
    assert!(
        matches!(*error, yamaa_engine::dataset::ExecutionError::Cell { ref path, source_row: 1, error: CellError::Access("held primary failure") } if path == "intermediates[0]")
    );
    assert_eq!(*primary.reads.borrow(), [(0, 0), (1, 0)]);
    assert_eq!(*other.reads.borrow(), [(0, 0)]);
    assert!(result.handler_counts.is_empty());
}
