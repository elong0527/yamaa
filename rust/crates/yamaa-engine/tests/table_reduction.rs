use std::cell::RefCell;
use yamaa_core::{
    numeric::{ArithmeticErrorKind, Number},
    reduction::{NumericReducer, ReductionError},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::table_reduction::{reduce_column, TableReductionError};

// Deliberately not Clone: storage errors must remain opaque and movable.
#[derive(Debug, PartialEq, Eq)]
struct PortError(&'static str);
struct Table {
    schema: TableSchema,
    values: Vec<Value>,
    fail_at: Option<usize>,
    reads: RefCell<Vec<usize>>,
}
impl Table {
    /// Construct a traced fake snapshot with one declared column.
    fn new(values: Vec<Value>) -> Self {
        Self {
            schema: TableSchema::new(vec![Column {
                name: "A".into(),
                kind: ColumnType::Int,
            }])
            .unwrap(),
            values,
            fail_at: None,
            reads: RefCell::new(vec![]),
        }
    }
}
impl TableAccess for Table {
    type Error = PortError;
    /// Return the fake schema without reading any cell.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Return fake snapshot size without touching the read trace.
    fn row_count(&self) -> usize {
        self.values.len()
    }
    /// Record valid access and inject an opaque storage failure when requested.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        if row >= self.row_count() || column >= self.schema.columns().len() {
            return Err(CellError::OutOfBounds { row, column });
        }
        self.reads.borrow_mut().push(row);
        if self.fail_at == Some(row) {
            return Err(CellError::Access(PortError("read failed")));
        }
        Ok((&self.values[row]).into())
    }
}

#[test]
/// Later argument access failures win over potential earlier fold failures.
fn read_all_arguments_before_folding_and_preserve_opaque_error() {
    for first in [Value::Int(i64::MAX), Value::Str("bad".into())] {
        let mut table = Table::new(vec![first, Value::Int(1), Value::Missing, Value::Int(0)]);
        table.fail_at = Some(2);
        assert_eq!(
            reduce_column(&table, 0, &[0, 1, 2, 3], 4, NumericReducer::Sum, "SUM(A)"),
            Err(TableReductionError::Cell {
                position: 2,
                error: CellError::Access(PortError("read failed"))
            })
        );
        assert_eq!(*table.reads.borrow(), [0, 1, 2]);
    }
}

#[test]
/// Once collection succeeds, fold failures retain their own written order.
fn arithmetic_failure_precedes_later_type_failure_after_all_reads() {
    // Mixed values intentionally exercise the consumer's defensive type checks;
    // an actual typed storage adapter validates its schema at ingestion.
    let table = Table::new(vec![
        Value::Int(i64::MAX),
        Value::Int(1),
        Value::Str("bad".into()),
    ]);
    let result = reduce_column(&table, 0, &[0, 1, 2], 3, NumericReducer::Sum, "SUM(A)");
    let Err(TableReductionError::Reduction(ReductionError::Arithmetic { argument, error })) =
        result
    else {
        panic!("expected overflow")
    };
    assert_eq!(argument, 1);
    assert_eq!(
        error.kind,
        ArithmeticErrorKind::IntegerOverflow {
            value: i64::MAX as i128 + 1
        }
    );
    assert_eq!(*table.reads.borrow(), [0, 1, 2]);
}

#[test]
/// Bad selections and resource budgets fail before any storage observation.
fn preflight_limits_and_coordinates_do_not_read_cells() {
    let table = Table::new(vec![Value::Int(1), Value::Int(2)]);
    let reduce = |column, rows: &[usize], limit| {
        reduce_column(&table, column, rows, limit, NumericReducer::Sum, "SUM(A)")
    };
    assert_eq!(
        reduce(0, &[0, 1], 1),
        Err(TableReductionError::RowLimit {
            limit: 1,
            required: 2
        })
    );
    assert_eq!(
        reduce(1, &[], 0),
        Err(TableReductionError::InvalidColumn { column: 1 })
    );
    assert_eq!(
        reduce(0, &[0, 2], 2),
        Err(TableReductionError::InvalidRow {
            position: 1,
            row: 2
        })
    );
    for rows in [[1, 0], [0, 0]] {
        assert_eq!(
            reduce(0, &rows, 2),
            Err(TableReductionError::UnorderedSelection { position: 1 })
        );
    }
    assert!(table.reads.borrow().is_empty());
    assert_eq!(
        table.cell(0, 1),
        Err(CellError::OutOfBounds { row: 0, column: 1 })
    );
}

#[test]
/// Empty and filtered relations retain schema and never read excluded rows.
fn empty_and_filtered_relations_preserve_schema_missing_and_order() {
    let empty = Table::new(vec![]);
    assert_eq!(empty.schema().columns()[0].name, "A");
    assert_eq!(empty.schema().columns()[0].kind, ColumnType::Int);
    assert_eq!(
        reduce_column(&empty, 0, &[], 0, NumericReducer::Mean, "MEAN(A)"),
        Ok(Number::Missing)
    );
    assert_eq!(
        empty.cell(0, 0),
        Err(CellError::OutOfBounds { row: 0, column: 0 })
    );
    let table = Table::new(vec![
        Value::Int(2),
        Value::Str("excluded".into()),
        Value::Missing,
        Value::Int(4),
    ]);
    assert_eq!(
        reduce_column(&table, 0, &[0, 2, 3], 3, NumericReducer::Mean, "MEAN(A)"),
        Ok(Number::float(3.0))
    );
    assert_eq!(*table.reads.borrow(), [0, 2, 3]);
    assert_eq!(table.cell(2, 0), Ok(ValueRef::Missing));
}
