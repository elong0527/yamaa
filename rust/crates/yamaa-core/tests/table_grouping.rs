//! Stable, exact partitioning of a table snapshot by its bound columns.
//!
//! The seam is `table_grouping::partition` over table doubles that record
//! each cell read or claim impossible geometry: the groups it returns, the
//! reads it makes and its typed `GroupingError`, which no run reports.
use crate::support;
use support::dataset::{schema, table};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    table_grouping::{partition, GroupingError},
    temporal::{Date, Precision},
    value::{ColumnType, Value},
};

/// First occurrence determines group order; source order determines member order.
#[test]
fn grouping_uses_exact_logical_equality_without_sorting() {
    let date = |precision| Value::Date(Date::new(2024, 2, 29, precision).unwrap());
    let source = table(
        &[
            ("text", ColumnType::Str),
            ("number", ColumnType::Float),
            ("day", ColumnType::Date),
        ],
        vec![
            vec![
                Value::Str("z\0\u{e9}".into()),
                Value::float(-0.0),
                date(Precision::Year),
            ],
            vec![Value::Missing, Value::float(2.0), date(Precision::Day)],
            vec![
                Value::Str("a".into()),
                Value::float(0.0),
                date(Precision::Day),
            ],
            vec![
                Value::Str("z\0\u{e9}".into()),
                Value::float(0.0),
                date(Precision::Month),
            ],
            vec![Value::Missing, Value::float(2.0), date(Precision::Month)],
            vec![
                Value::Str("z\0e\u{301}".into()),
                Value::float(0.0),
                date(Precision::Year),
            ],
        ],
    );
    assert_eq!(
        partition(&source, &[0, 1, 2]).unwrap(),
        vec![vec![0, 3], vec![1, 4], vec![2], vec![5]]
    );
    assert_eq!(
        *source.reads.borrow(),
        (0..6)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .collect::<Vec<_>>()
    );
}

/// Invalid grouping declarations precede every source cell read.
#[test]
fn grouping_preflight_and_port_failures_are_distinct() {
    let mut source = table(
        &[("x", ColumnType::Int)],
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
    );
    assert_eq!(partition(&source, &[]), Err(GroupingError::EmptyColumns));
    assert_eq!(
        partition(&source, &[0, 1]),
        Err(GroupingError::InvalidColumn {
            position: 1,
            column: 1
        })
    );
    assert_eq!(
        partition(&source, &[0, 0]),
        Err(GroupingError::DuplicateColumn {
            position: 1,
            column: 0
        })
    );
    assert!(source.reads.borrow().is_empty());
    source.fail = Some((1, 0));
    assert_eq!(
        partition(&source, &[0]),
        Err(GroupingError::Cell {
            row: 1,
            column: 0,
            error: CellError::Access("source failure")
        })
    );
}

/// Full-width integer keys are never converted to floating point for grouping;
/// an empty snapshot has no groups and reads nothing.
#[test]
fn grouping_distinguishes_adjacent_large_integers_and_empty_snapshots() {
    let source = table(
        &[("id", ColumnType::Int)],
        vec![
            vec![Value::Int(9_007_199_254_740_993)],
            vec![Value::Int(9_007_199_254_740_992)],
            vec![Value::Int(i64::MIN)],
            vec![Value::Int(9_007_199_254_740_993)],
            vec![Value::Int(i64::MAX)],
        ],
    );
    assert_eq!(
        partition(&source, &[0]).unwrap(),
        vec![vec![0, 3], vec![1], vec![2], vec![4]]
    );
    let empty = table(&[("id", ColumnType::Int)], vec![]);
    assert!(partition(&empty, &[0]).unwrap().is_empty());
    assert!(empty.reads.borrow().is_empty());
}

/// Impossible table geometry is rejected before a port can be read.
#[test]
fn grouping_refuses_impossible_row_geometry_before_reads() {
    struct Huge(TableSchema);
    impl TableAccess for Huge {
        type Error = ();
        /// Describe a virtual source without allocating its records.
        fn schema(&self) -> &TableSchema {
            &self.0
        }
        /// Exercise usize multiplication overflow at preflight.
        fn row_count(&self) -> usize {
            usize::MAX
        }
        /// Any read is an admission bug for this impossible table geometry.
        fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<()>> {
            panic!("must not read")
        }
    }
    let source = Huge(schema(&[("a", ColumnType::Int), ("b", ColumnType::Int)]));
    assert_eq!(
        partition(&source, &[0, 1]),
        Err(GroupingError::Limit(yamaa_core::limits::LimitExceeded {
            resource: "table_rows",
            limit: 10_000_000,
            actual: Some(usize::MAX)
        }))
    );
}
