#![cfg(feature = "parquet")]
//! Saved Parquet values: a saved file promises what it reads back as, not
//! its bytes (D305 on #4184, PARQUET-4).
//!
//! These tests save authored tables through the artifact encoder
//! (`src/parquet_artifact/encode.rs`) and read them back with parquet
//! 55.2.0, the test-only oracle: every field name, row, null and value comes
//! back, each float bit for bit, whole or saved a partition at a time (D264).
//! The tables cross the definition levels' runs (a group of eight, a
//! bit-packed run of 63 groups), the row group of 131,072 rows, a data page's
//! 1 MiB of values and the dictionary's 1 MiB bound, the layout the encoder
//! documents. Each table lends itself to threads, and is saved on four as on
//! one, byte for byte (#4220).
//!
//! The seam is the encoder's saved file read by an independent reader; the
//! benchmark reader and the host fixtures read Parquet through the engine's
//! own reader.
use bytes::Bytes;
use parquet::{
    basic::{Compression, Encoding, PageType},
    file::{
        metadata::ParquetMetaData, reader::FileReader, serialized_reader::SerializedFileReader,
    },
    record::Field,
};
use std::{convert::Infallible, num::NonZeroUsize, ops::Range};
use yamaa_adapters::{
    column_table::ColumnTable,
    epoch::{date_from_days, datetime_from_seconds},
    parquet_artifact::{render, render_partitions},
};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    temporal::Precision,
    value::{ColumnType, Value},
};

/// One authored cell as a reader sees it: a date in days and a datetime in
/// microseconds from 1970-01-01, and a float as its bits.
#[derive(Clone, Debug, PartialEq)]
enum Cell {
    Missing,
    Str(String),
    Int(i64),
    Float(u64),
    Date(i32),
    DateTime(i64),
}

impl Cell {
    fn float(value: f64) -> Self {
        Self::Float(value.to_bits())
    }

    /// The engine value the cell holds.
    fn value(&self) -> Value {
        match self {
            Cell::Missing => Value::Missing,
            Cell::Str(value) => Value::Str(value.clone()),
            Cell::Int(value) => Value::Int(*value),
            Cell::Float(bits) => Value::float(f64::from_bits(*bits)),
            Cell::Date(days) => {
                Value::Date(date_from_days(i64::from(*days), Precision::Day).unwrap())
            }
            Cell::DateTime(micros) => Value::DateTime(
                datetime_from_seconds(micros / 1_000_000, Precision::Second).unwrap(),
            ),
        }
    }

    /// The cell a read field holds.
    fn read(field: &Field) -> Self {
        match field {
            Field::Null => Cell::Missing,
            Field::Str(value) => Cell::Str(value.clone()),
            Field::Long(value) => Cell::Int(*value),
            Field::Double(value) => Cell::float(*value),
            Field::Date(days) => Cell::Date(*days),
            Field::TimestampMicros(micros) => Cell::DateTime(*micros),
            other => panic!("a field of no saved type: {other:?}"),
        }
    }
}

/// An authored table: its columns and the cells of each row.
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    cells: Vec<Vec<Cell>>,
}

impl Table {
    fn new(columns: &[(&str, ColumnType)], cells: Vec<Vec<Cell>>) -> Self {
        Self {
            schema: crate::support::table_schema(columns),
            rows: cells
                .iter()
                .map(|row| row.iter().map(Cell::value).collect())
                .collect(),
            cells,
        }
    }

    fn projection(&self) -> Vec<usize> {
        (0..self.schema.columns().len()).collect()
    }
}

impl TableAccess for Table {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.rows
            .get(row)
            .and_then(|cells| cells.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
    fn shared(&self) -> Option<&(dyn TableAccess<Error = Infallible> + Sync)> {
        Some(self)
    }
}

/// The threads a table is saved on besides one (#4220).
const FOUR: NonZeroUsize = match NonZeroUsize::new(4) {
    Some(four) => four,
    None => unreachable!(),
};

/// The five column types that PARQUET-1 maps, in its order.
const COLUMNS: [(&str, ColumnType); 5] = [
    ("str", ColumnType::Str),
    ("int", ColumnType::Int),
    ("float", ColumnType::Float),
    ("date", ColumnType::Date),
    ("datetime", ColumnType::DateTime),
];

/// The field names and rows that a saved file reads back as, and its
/// metadata.
fn read_back(saved: Vec<u8>) -> (Vec<String>, Vec<Vec<Cell>>, ParquetMetaData) {
    let reader = SerializedFileReader::new(Bytes::from(saved)).unwrap();
    let names = reader
        .metadata()
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| column.name().to_owned())
        .collect();
    let rows = reader
        .get_row_iter(None)
        .unwrap()
        .map(|row| {
            row.unwrap()
                .get_column_iter()
                .map(|(_, field)| Cell::read(field))
                .collect()
        })
        .collect();
    (names, rows, reader.metadata().clone())
}

/// Consecutive ranges of `size` rows over `rows`, the last holding the rest;
/// one empty range for no rows.
fn partitions(rows: usize, size: usize) -> Vec<Range<usize>> {
    (0..rows.max(1))
        .step_by(size)
        .map(|start| start..rows.min(start + size))
        .collect()
}

/// 1,030 rows, each column missing at its own period, then six rows of edge
/// values: each type's least and greatest, a row missing in every column,
/// the epoch and the second before it, -0.0 and the least subnormal, and
/// text that is empty, a space, multibyte, control characters and long.
fn edges() -> Vec<Vec<Cell>> {
    let mut rows = (0..1_024_i64)
        .map(|i| {
            let n = i - 512;
            let unless = |period: i64, phase: i64, cell: Cell| {
                if i % period == phase {
                    Cell::Missing
                } else {
                    cell
                }
            };
            vec![
                unless(7, 0, Cell::Str(format!("row {i}"))),
                unless(11, 1, Cell::Int(n * 1_000_000_007)),
                unless(13, 2, Cell::float(n as f64 * 0.125)),
                unless(5, 3, Cell::Date((n * 97) as i32)),
                unless(17, 4, Cell::DateTime(n * 3_196_861 * 1_000_000)),
            ]
        })
        .collect::<Vec<_>>();
    rows.extend([
        vec![
            Cell::Str(String::new()),
            Cell::Int(i64::MIN),
            Cell::float(-0.0),
            Cell::Date(-719_162),
            Cell::DateTime(-62_135_596_800 * 1_000_000),
        ],
        vec![
            Cell::Str("a\u{e9}\u{65e5}\u{1f600}".into()),
            Cell::Int(i64::MAX),
            Cell::float(f64::MAX),
            Cell::Date(2_932_896),
            Cell::DateTime(253_402_300_799 * 1_000_000),
        ],
        vec![Cell::Missing; 5],
        vec![
            Cell::Str("\0\t\n\"\\".into()),
            Cell::Int(0),
            Cell::Float(1),
            Cell::Date(0),
            Cell::DateTime(0),
        ],
        vec![
            Cell::Str(" ".into()),
            Cell::Int(-1),
            Cell::float(f64::MIN),
            Cell::Date(-1),
            Cell::DateTime(-1_000_000),
        ],
        vec![
            Cell::Str("x".repeat(300)),
            Cell::Int(9_007_199_254_740_993),
            Cell::float(0.1),
            Cell::Date(11_016),
            Cell::DateTime(951_782_400 * 1_000_000),
        ],
    ]);
    rows
}

/// Whether row `row` is missing under `pattern`: never, always, every
/// `pattern`th row, in runs of one to seventeen between runs of one to
/// twelve, in alternate runs of eight or nine, in the first 600 rows, or at
/// random with `state`.
fn missing(pattern: usize, row: usize, state: &mut u64) -> bool {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    match pattern {
        0 => false,
        1 => true,
        2..=21 => row % pattern == 1,
        22 => {
            let (mut at, mut length) = (0, 1);
            loop {
                if row < at + length {
                    return true;
                }
                at += length;
                let present = length % 12 + 1;
                if row < at + present {
                    return false;
                }
                at += present;
                length = length % 17 + 1;
            }
        }
        23 => row % 16 < 8,
        24 => row % 18 < 9,
        25 => row < 600,
        26 => *state % 10 == 0,
        27 => *state % 2 == 0,
        _ => *state % 10 != 0,
    }
}

/// The present cell of column `kind` at `row`: text of a few repeated
/// values, and numbers and temporals from integer arithmetic.
fn present(kind: ColumnType, row: usize) -> Cell {
    let n = row as i64 - 700;
    match kind {
        ColumnType::Str => Cell::Str("x".repeat(row % 5)),
        ColumnType::Int => Cell::Int(n * 1_000_003),
        ColumnType::Float => Cell::float(n as f64 * 0.25),
        ColumnType::Date => Cell::Date((n * 31) as i32),
        ColumnType::DateTime => Cell::DateTime(n * 4_093 * 1_000_000),
    }
}

/// D305 and D264: every table below reads back as authored, saved whole and
/// in partitions of one, seven and a thousand rows. The edge table holds
/// each type's extremes; the others cross the null patterns at row counts on
/// each side of a group of eight, a bit-packed run of 63 groups and 1,024
/// rows; one has no rows.
#[test]
fn every_table_reads_back_as_authored_whole_and_a_partition_at_a_time() {
    let mut tables = vec![("edges".to_owned(), Table::new(&COLUMNS, edges()))];
    for rows in [
        0, 1, 7, 8, 9, 15, 17, 503, 504, 505, 512, 1_023, 1_024, 1_025, 2_051,
    ] {
        for patterns in (0..29).collect::<Vec<_>>().chunks(5) {
            let mut state = 0x9e37_79b9_7f4a_7c15 ^ rows as u64;
            let columns = &COLUMNS[..patterns.len()];
            let cells = (0..rows)
                .map(|row| {
                    patterns
                        .iter()
                        .zip(columns)
                        .map(|(&pattern, &(_, kind))| {
                            if missing(pattern, row, &mut state) {
                                Cell::Missing
                            } else {
                                present(kind, row)
                            }
                        })
                        .collect()
                })
                .collect();
            tables.push((
                format!("{rows} rows, patterns {patterns:?}"),
                Table::new(columns, cells),
            ));
        }
    }
    for (label, table) in &tables {
        let names: Vec<String> = table
            .schema
            .columns()
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let expected = (names, table.cells.clone());
        let whole = render(table, &table.projection()).unwrap();
        // The same values through a port that lends its columns whole
        // (#4201): the encoder reads them as it reads cells, so the file is
        // the one the cells give.
        let lent = ColumnTable::from_access(table).unwrap();
        assert!(
            render(&lent, &table.projection()).unwrap() == whole,
            "{label}, read through column views"
        );
        let (names, rows, _) = read_back(whole);
        assert_eq!((names, rows), expected, "{label}, whole");
        for size in [1, 7, 1_000] {
            let partitions = partitions(table.row_count(), size);
            let (saved, ends) =
                render_partitions(table, &table.projection(), &partitions, NonZeroUsize::MIN)
                    .unwrap();
            assert_eq!(
                ends.last(),
                Some(&saved.len()),
                "{label}, partitions of {size}"
            );
            assert!(
                render_partitions(table, &table.projection(), &partitions, FOUR).unwrap()
                    == (saved.clone(), ends),
                "{label}, partitions of {size}, on four threads"
            );
            let (names, rows, _) = read_back(saved);
            assert_eq!((names, rows), expected, "{label}, partitions of {size}");
        }
    }
}

/// D305: a table of one full row group of 131,072 rows and five more reads
/// back in order from the two row groups the encoder documents, saved whole
/// and in partitions of 100,000 rows, which divide no row group.
#[test]
fn a_table_past_one_row_group_reads_back_in_order_from_two_row_groups() {
    const ROWS: usize = 131_077;
    let cells: Vec<Vec<Cell>> = (0..ROWS)
        .map(|row| {
            let n = row as i64;
            vec![
                Cell::Str(format!("ARM {}", row % 3)),
                Cell::Int(n * 7 - 1_000),
                if row % 9 == 4 {
                    Cell::Missing
                } else {
                    Cell::float(n as f64 / 8.0)
                },
                Cell::Date((n % 400) as i32),
                Cell::DateTime(n * 60 * 1_000_000),
            ]
        })
        .collect();
    let table = Table::new(&COLUMNS, cells);
    let names: Vec<String> = COLUMNS.iter().map(|(name, _)| (*name).to_owned()).collect();
    let lent = ColumnTable::from_access(&table).unwrap();
    for (label, partitions) in [
        ("whole", partitions(ROWS, ROWS)),
        ("partitions of 100,000", partitions(ROWS, 100_000)),
    ] {
        let (saved, ends) =
            render_partitions(&table, &table.projection(), &partitions, NonZeroUsize::MIN).unwrap();
        // A port that lends its columns whole saves the same file, its
        // partitions ending at the same bytes (#4201), and so does a save on
        // four threads, each row group encoded on its own (#4220).
        let saves = [
            (
                render_partitions(&lent, &table.projection(), &partitions, NonZeroUsize::MIN),
                "read through column views",
            ),
            (
                render_partitions(&table, &table.projection(), &partitions, FOUR),
                "on four threads",
            ),
            (
                render_partitions(&lent, &table.projection(), &partitions, FOUR),
                "read through column views on four threads",
            ),
        ];
        for (save, case) in saves {
            assert!(
                save.unwrap() == (saved.clone(), ends.clone()),
                "{label}, {case}"
            );
        }
        let (read, rows, metadata) = read_back(saved);
        let groups: Vec<i64> = metadata
            .row_groups()
            .iter()
            .map(|group| group.num_rows())
            .collect();
        assert!(
            (read, groups) == (names.clone(), vec![131_072, 5]) && rows == table.cells,
            "{label}"
        );
    }
}

/// D305: of 20,000 rows, the column of distinct 60-byte strings passes the
/// dictionary's 1 MiB as PLAIN values (20,000 x 64 bytes with their
/// lengths), so it is saved in PLAIN data pages, which close at 1 MiB: two,
/// of 16,384 rows and 3,616. The column of three values is a dictionary page
/// and one data page of indices. Every page is Zstandard, and both columns
/// read back as authored.
#[test]
fn a_chunk_whose_distinct_values_pass_1_mib_is_saved_in_plain_pages_and_reads_back() {
    const ROWS: usize = 20_000;
    let cells: Vec<Vec<Cell>> = (0..ROWS)
        .map(|row| {
            vec![
                Cell::Str(format!("{row:060}")),
                Cell::Str(["a", "b", "c"][row % 3].to_owned()),
            ]
        })
        .collect();
    let table = Table::new(
        &[("UNIQUE", ColumnType::Str), ("FEW", ColumnType::Str)],
        cells,
    );
    let (_, rows, metadata) = read_back(render(&table, &[0, 1]).unwrap());
    let chunks: Vec<_> = metadata.row_groups()[0]
        .columns()
        .iter()
        .map(|chunk| {
            let pages: Vec<(PageType, Encoding, i32)> = chunk
                .page_encoding_stats()
                .unwrap()
                .iter()
                .map(|stat| (stat.page_type, stat.encoding, stat.count))
                .collect();
            (matches!(chunk.compression(), Compression::ZSTD(_)), pages)
        })
        .collect();
    assert_eq!(
        chunks,
        [
            (true, vec![(PageType::DATA_PAGE, Encoding::PLAIN, 2)]),
            (
                true,
                vec![
                    (PageType::DICTIONARY_PAGE, Encoding::PLAIN, 1),
                    (PageType::DATA_PAGE, Encoding::RLE_DICTIONARY, 1),
                ]
            ),
        ],
        "the pages of each column chunk"
    );
    assert!(rows == table.cells, "the values read back");
}
