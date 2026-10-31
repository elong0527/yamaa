//! Executor behaviour no specification can reach.
use crate::table::ValueRef;
use std::convert::Infallible;

/// A buffer the executor cannot reserve fails the run as an allocation
/// failure instead of aborting the process: a frame, a filled vector, and the
/// hash partition.
#[test]
fn a_refused_reservation_is_an_allocation_failure() {
    use crate::dataset::execution::ExecutionError;
    let mut frame = super::frame::Frame::new(3);
    let refused = super::reserved::<(), Infallible>(frame.try_reserve(usize::MAX, 0));
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    let refused = super::filled::<u64, Infallible>(usize::MAX, 0);
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    assert!(super::group::partition(usize::MAX, 1, |_, _| ValueRef::Missing).is_err());
    // A reservation that fits changes nothing a reader sees.
    super::reserved::<(), Infallible>(frame.try_reserve(4, 4)).unwrap();
    assert_eq!(frame.rows(), 0);
}

/// The accepted dataset lends each column of one type whole, and its view
/// reads every row as the cell does (#4201); a column with no type of its
/// own, every row missing or values of two types, lends none and is read a
/// cell at a time.
#[test]
fn the_dataset_lends_each_typed_column_as_its_cells_read() {
    use crate::table::{Column, TableAccess, TableSchema};
    use crate::temporal::{Date, DateTime, Precision};
    use crate::value::{ColumnType, FiniteFloat, Value};
    let day = Date::new(2024, 2, 29, Precision::Day).unwrap();
    let moment = DateTime::new(day, 1, 2, 3, Precision::Second).unwrap();
    let float = Value::Float(FiniteFloat::new(-0.0).unwrap());
    let kinds = [
        ColumnType::Str,
        ColumnType::Int,
        ColumnType::Float,
        ColumnType::Date,
        ColumnType::DateTime,
        ColumnType::Str,
        ColumnType::Int,
    ];
    let schema = TableSchema::new(
        kinds
            .iter()
            .enumerate()
            .map(|(index, &kind)| Column {
                name: format!("C{index}"),
                kind,
            })
            .collect(),
    )
    .unwrap();
    let rows = vec![
        vec![
            Value::Str("ab".into()),
            Value::Int(1),
            float.clone(),
            Value::Missing,
            Value::DateTime(moment),
            Value::Missing,
            Value::Int(1),
        ],
        vec![
            Value::Missing,
            Value::Missing,
            Value::Missing,
            Value::Date(day),
            Value::Missing,
            Value::Missing,
            Value::Str("x".into()),
        ],
        vec![
            Value::Str(String::new()),
            Value::Int(-3),
            float,
            Value::Date(day),
            Value::DateTime(moment),
            Value::Missing,
            Value::Int(2),
        ],
    ];
    let dataset = crate::dataset::Dataset::from_rows(schema, rows);
    let lent: Vec<bool> = (0..kinds.len())
        .map(|column| dataset.column(column).is_some())
        .collect();
    assert_eq!(
        lent,
        [true, true, true, true, true, false, false],
        "the typed columns lend views; an all-missing and a mixed column do not"
    );
    for column in 0..kinds.len() {
        let Some(view) = dataset.column(column) else {
            continue;
        };
        let read: Vec<_> = (0..3).map(|row| view.get(row)).collect();
        let cells: Vec<_> = (0..3)
            .map(|row| dataset.cell(row, column).unwrap())
            .collect();
        assert_eq!((view.len(), read), (3, cells), "column C{column}");
    }
}
