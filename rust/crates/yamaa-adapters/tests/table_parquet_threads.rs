#![cfg(feature = "parquet")]
//! Saved Parquet files under the run's threads (#4220): the writer encodes
//! up to `threads` row groups at once when the table lends itself to threads
//! (`TableAccess::shared`) and appends them in row-group order, so a file
//! saved on several threads is byte for byte the file one thread saves, its
//! partitions end at the same bytes, and its first refusal is the one-thread
//! write's.
//!
//! The seam is the writer's thread count, which no fixture can name: the
//! benchmarks and the host fixtures save through the run, whose threads the
//! machine sets. The expected refusals follow the write the encoder documents
//! (`src/parquet_artifact/encode.rs`): the projection first; then the row
//! groups in order, each a column at a time in projection order, each column
//! over its rows in order; each partition admitted when the write reaches
//! it, after the row groups full before it; and rows held until their row
//! group is full, so a refused partition ends the write before the row group
//! it would have filled.
use std::{num::NonZeroUsize, ops::Range, rc::Rc};
use yamaa_adapters::{
    column_table::{Column, ColumnTable},
    epoch::{date_from_days, datetime_from_seconds},
    parquet_artifact::{render_partitions, Error},
};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    temporal::Precision,
    value::{ColumnType, FiniteFloat},
};

/// The rows of a full row group, as the encoder documents.
const GROUP: usize = 131_072;

/// The five column types that PARQUET-1 maps, in its order.
const COLUMNS: [(&str, ColumnType); 5] = [
    ("str", ColumnType::Str),
    ("int", ColumnType::Int),
    ("float", ColumnType::Float),
    ("date", ColumnType::Date),
    ("datetime", ColumnType::DateTime),
];

fn threads(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

/// The cell of column `kind` at `row`: missing at the column's own period,
/// else text of three values, or a number or temporal from the row.
fn value(kind: ColumnType, row: usize) -> ValueRef<'static> {
    let n = row as i64;
    let (period, phase) = match kind {
        ColumnType::Str => (7, 3),
        ColumnType::Int => (11, 5),
        ColumnType::Float => (13, 2),
        ColumnType::Date => (17, 0),
        ColumnType::DateTime => (19, 9),
    };
    if row % period == phase {
        return ValueRef::Missing;
    }
    match kind {
        ColumnType::Str => ValueRef::Str(["ARM A", "ARM B", "PLACEBO"][row % 3]),
        ColumnType::Int => ValueRef::Int(n * 1_000_003 - 7),
        ColumnType::Float => ValueRef::Float(FiniteFloat::new(n as f64 * 0.125).unwrap()),
        ColumnType::Date => ValueRef::Date(date_from_days(n % 40_000, Precision::Day).unwrap()),
        ColumnType::DateTime => ValueRef::DateTime(
            datetime_from_seconds(n * 61 - 1_000_000, Precision::Second).unwrap(),
        ),
    }
}

/// A table of `rows` rows of [`COLUMNS`] whose cells are computed from their
/// rows, so that it can pass several row groups, or the row limit, without
/// holding them. Each cell `refused` names is refused with its coordinates,
/// as an error that cannot leave the thread that raised it (`Rc` is not
/// `Send`). It lends itself to threads.
struct Computed {
    schema: TableSchema,
    rows: usize,
    refused: Vec<(usize, usize)>,
}

impl Computed {
    fn new(rows: usize, refused: &[(usize, usize)]) -> Self {
        Self {
            schema: crate::support::table_schema(&COLUMNS),
            rows,
            refused: refused.to_vec(),
        }
    }

    /// The same values held in columns, which a port lends whole.
    fn lent(&self) -> ColumnTable {
        let columns = COLUMNS
            .iter()
            .map(|&(_, kind)| {
                let mut column = Column::empty(kind);
                for row in 0..self.rows {
                    column.push(value(kind, row));
                }
                column
            })
            .collect();
        ColumnTable::new(self.schema.clone(), columns, self.rows).unwrap()
    }
}

impl TableAccess for Computed {
    type Error = Rc<(usize, usize)>;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        let Some(&(_, kind)) = COLUMNS.get(column).filter(|_| row < self.rows) else {
            return Err(CellError::OutOfBounds { row, column });
        };
        if self.refused.contains(&(row, column)) {
            return Err(CellError::Access(Rc::new((row, column))));
        }
        Ok(value(kind, row))
    }
    fn shared(&self) -> Option<&(dyn TableAccess<Error = Self::Error> + Sync)> {
        Some(self)
    }
}

/// Consecutive ranges of `size` rows over `rows`, the last holding the rest.
fn partitions(rows: usize, size: usize) -> Vec<Range<usize>> {
    (0..rows)
        .step_by(size)
        .map(|start| start..rows.min(start + size))
        .collect()
}

/// A table of five full row groups and a sixth of 1,000 rows, more than
/// four threads take at once, is saved on four threads as on one: the same
/// bytes, its partitions ending at the same bytes, read through its cells or
/// through the column views a port lends, saved whole and in partitions that
/// divide row groups. (The readback tables cross every column type.)
#[test]
fn a_file_saved_on_four_threads_is_the_file_one_thread_saves() {
    let rows = 5 * GROUP + 1_000;
    let table = Computed::new(rows, &[]);
    let lent = table.lent();
    let projection = [0, 1];
    for (label, partitions) in [
        ("whole", partitions(rows, rows)),
        ("partitions of 100,000", partitions(rows, 100_000)),
    ] {
        let one = render_partitions(&table, &projection, &partitions, threads(1)).unwrap();
        let cells = render_partitions(&table, &projection, &partitions, threads(4));
        let views = render_partitions(&lent, &projection, &partitions, threads(4));
        assert!(
            one.1.len() == partitions.len() && cells.as_ref() == Ok(&one) && views == Ok(one),
            "{label}, through cells and through column views"
        );
    }
}

/// One write's refusal: the table's rows and refused cells, the projection
/// and partitions saved, and the refusal the write meets first.
struct Refusal {
    label: &'static str,
    rows: usize,
    refused: &'static [(usize, usize)],
    projection: &'static [usize],
    partitions: Vec<Range<usize>>,
    expected: Result<(), Error<Rc<(usize, usize)>>>,
}

/// Each refusal comes as the one-thread write orders it (the module states
/// the order), on one thread and on four.
#[test]
fn a_write_on_four_threads_is_refused_as_the_write_on_one_is() {
    let cell = |row, column| Err(Error::Cell(CellError::Access(Rc::new((row, column)))));
    let whole = |rows| partitions(rows, rows);
    // A partition of 300,000 rows, two full row groups and 37,856 rows held
    // for a third, then one past the row limit.
    const PAST_LIMIT: usize = 10_300_001;
    let past_limit = || vec![0..300_000, 300_000..PAST_LIMIT];
    let table_rows = || {
        Err(Error::Limit {
            resource: "table_rows",
            limit: 10_000_000,
            actual: Some(10_000_001),
        })
    };
    let cases = [
        Refusal {
            label: "no column",
            rows: 10,
            refused: &[],
            projection: &[],
            partitions: whole(10),
            expected: Err(Error::Projection),
        },
        Refusal {
            label: "a column projected twice",
            rows: 10,
            refused: &[],
            projection: &[0, 1, 0],
            partitions: whole(10),
            expected: Err(Error::Projection),
        },
        Refusal {
            label: "a column past the schema",
            rows: 10,
            refused: &[],
            projection: &[0, 5],
            partitions: whole(10),
            expected: Err(Error::Projection),
        },
        Refusal {
            label: "a refused cell in the third of five row groups",
            rows: 4 * GROUP + 1_000,
            refused: &[(2 * GROUP + 10, 1)],
            projection: &[0, 1],
            partitions: whole(4 * GROUP + 1_000),
            expected: cell(2 * GROUP + 10, 1),
        },
        Refusal {
            label: "a refused cell in the last row group, of 1,000 rows",
            rows: 2 * GROUP + 1_000,
            refused: &[(2 * GROUP + 999, 0)],
            projection: &[0, 1],
            partitions: whole(2 * GROUP + 1_000),
            expected: cell(2 * GROUP + 999, 0),
        },
        Refusal {
            label: "an earlier row group's refusal before a later one's, whatever its column",
            rows: 4 * GROUP + 1_000,
            refused: &[(3 * GROUP + 1, 0), (GROUP + 50, 1)],
            projection: &[0, 1],
            partitions: whole(4 * GROUP + 1_000),
            expected: cell(GROUP + 50, 1),
        },
        Refusal {
            label: "within a row group, the column projected first, whatever its row",
            rows: 2 * GROUP,
            refused: &[(GROUP + 900, 1), (GROUP + 5, 0), (GROUP + 9_000, 2)],
            projection: &[2, 1, 0],
            partitions: partitions(2 * GROUP, 100_000),
            expected: cell(GROUP + 9_000, 2),
        },
        Refusal {
            label: "a refused partition after the row groups full before it",
            rows: PAST_LIMIT,
            refused: &[],
            projection: &[0, 1],
            partitions: past_limit(),
            expected: table_rows(),
        },
        Refusal {
            label: "a refused cell in a row group full before a refused partition",
            rows: PAST_LIMIT,
            refused: &[(200_000, 1)],
            projection: &[0, 1],
            partitions: past_limit(),
            expected: cell(200_000, 1),
        },
        Refusal {
            label: "a refused partition before a refused cell held for the row group it would fill",
            rows: PAST_LIMIT,
            refused: &[(280_000, 0)],
            projection: &[0, 1],
            partitions: past_limit(),
            expected: table_rows(),
        },
        Refusal {
            label: "a refused cell in the first row group, in the column projected last",
            rows: PAST_LIMIT,
            refused: &[(GROUP - 1, 0)],
            projection: &[1, 0],
            partitions: past_limit(),
            expected: cell(GROUP - 1, 0),
        },
    ];
    for case in cases {
        let table = Computed::new(case.rows, case.refused);
        for count in [1, 4] {
            let saved =
                render_partitions(&table, case.projection, &case.partitions, threads(count));
            assert_eq!(
                saved.map(|_| ()),
                case.expected,
                "{}, on {count} threads",
                case.label
            );
        }
    }
}
