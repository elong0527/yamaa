#![cfg(feature = "parquet")]
//! Saved Parquet bytes: a regression control that keeps every byte the 1.0
//! writer saves, the footer's `created_by` included (D160 on #3030, #3475).
//!
//! The stability contract in `docs/release.md` keeps the bytes that each
//! profile saves through every 1.x release. PARQUET-4 asks only that a
//! Parquet file read back the same, and the corpus compares Parquet that way,
//! so a change to the encoder would pass it unnoticed. These tests save three
//! fixed tables through the artifact encoder (`src/parquet_artifact/encode.rs`):
//! one with a column of each type that PARQUET-1 maps, a missing value in
//! each, and 1,030 rows in two row groups; one with the same columns and row
//! groups, each missing in a pattern that the definition levels encode
//! differently; and one with no rows.
//!
//! A run divided into partitions writes its output one partition at a time
//! (D264); the writer holds rows until a row group is full, so every
//! partition size, one that does not divide the row-group size included,
//! saves the same bytes.
//!
//! `fixtures/parquet_bytes/` holds the bytes that the 1.0 writer saved from
//! them as base64 text, since binary files live only under `benchmarks/`.
//! They are a regression control, not independently authored truth: a 1.x
//! release replaces them only for a defect fix that names the rule it
//! restores. A failed comparison prints the saved bytes as base64 between
//! `-----BEGIN <fixture>-----` and `-----END <fixture>-----`, and that text is
//! the replacement, as `fixtures/README.md` describes. No test writes a
//! fixture. The writer that `created_by` names is an authored literal here,
//! so a replaced fixture cannot change it.
use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use parquet::file::{reader::FileReader, serialized_reader::SerializedFileReader};
use std::convert::Infallible;
use yamaa_adapters::{
    epoch::{date_from_days, datetime_from_seconds},
    parquet_artifact::{render, render_partitions},
};
use yamaa_core::{
    table::{CellError, TableAccess, TableSchema, ValueRef},
    temporal::Precision,
    value::{ColumnType, Value},
};

/// The writer that the footer of every 1.x Parquet artifact names (D160).
const CREATED_BY: &str = "parquet-rs version 55.2.0";

/// Every column, in schema order.
const PROJECTION: [usize; 5] = [0, 1, 2, 3, 4];

/// The width of a printed base64 line, as MIME wraps it.
const LINE: usize = 76;

struct Table {
    schema: TableSchema,
    rows: Vec<[Value; 5]>,
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
}

/// A table with one column of each type that PARQUET-1 maps, in its order.
fn table(rows: Vec<[Value; 5]>) -> Table {
    Table {
        schema: crate::support::table_schema(&[
            ("str", ColumnType::Str),
            ("int", ColumnType::Int),
            ("float", ColumnType::Float),
            ("date", ColumnType::Date),
            ("datetime", ColumnType::DateTime),
        ]),
        rows,
    }
}

/// Days from 1970-01-01.
fn date(days: i64) -> Value {
    Value::Date(date_from_days(days, Precision::Day).unwrap())
}

/// Seconds from 1970-01-01T00:00:00 on the same wall clock, with no zone.
fn datetime(seconds: i64) -> Value {
    Value::DateTime(datetime_from_seconds(seconds, Precision::Second).unwrap())
}

/// 1,030 rows that every platform builds alike: each value is a constant or
/// comes from integer arithmetic, and each computed float is an exact eighth.
/// The first 1,024 rows fill the first row group, each column missing at its
/// own period. The six rows of the second hold the edge values: each type's
/// least and greatest, a row missing in every column, the epoch and the second
/// before it, and text that is empty, a space, multibyte, control characters
/// and long.
fn rows() -> Vec<[Value; 5]> {
    let mut rows = (0..1_024_i64)
        .map(|i| {
            let n = i - 512;
            let present = |period: i64, phase: i64, value: Value| {
                if i % period == phase {
                    Value::Missing
                } else {
                    value
                }
            };
            [
                present(7, 0, Value::Str(format!("row {i}"))),
                present(11, 1, Value::Int(n * 1_000_000_007)),
                present(13, 2, Value::float(n as f64 * 0.125)),
                present(5, 3, date(n * 97)),
                present(17, 4, datetime(n * 3_196_861)),
            ]
        })
        .collect::<Vec<_>>();
    rows.extend([
        [
            Value::Str(String::new()),
            Value::Int(i64::MIN),
            Value::float(-0.0),
            date(-719_162),
            datetime(-62_135_596_800),
        ],
        [
            Value::Str("a\u{e9}\u{65e5}\u{1f600}".into()),
            Value::Int(i64::MAX),
            Value::float(f64::MAX),
            date(2_932_896),
            datetime(253_402_300_799),
        ],
        [
            Value::Missing,
            Value::Missing,
            Value::Missing,
            Value::Missing,
            Value::Missing,
        ],
        [
            Value::Str("\0\t\n\"\\".into()),
            Value::Int(0),
            Value::float(f64::from_bits(1)),
            date(0),
            datetime(0),
        ],
        [
            Value::Str(" ".into()),
            Value::Int(-1),
            Value::float(f64::MIN),
            date(-1),
            datetime(-1),
        ],
        [
            Value::Str("x".repeat(300)),
            Value::Int(9_007_199_254_740_993),
            Value::float(0.1),
            date(11_016),
            datetime(951_782_400),
        ],
    ]);
    rows
}

/// Whether row `row` falls in a run of missing rows, where the runs grow
/// from one row to seventeen and back to one, with runs of one to twelve
/// present rows between them.
fn in_missing_run(row: usize) -> bool {
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

/// 1,030 rows whose columns are missing in the patterns that decide how the
/// definition levels are encoded: a value repeated eight times or more is a
/// run of its own, other values are packed eight to a byte in runs of at most
/// 504, and the last group of a row group is padded. `str` is missing in runs
/// of one to seventeen rows, `int` in every row, `float` in the first 600,
/// `date` in alternate runs of eight and `datetime` in every other row from
/// row 504. Present values come from integer arithmetic, and each float is
/// an exact half.
fn nulls() -> Vec<[Value; 5]> {
    (0..1_030_usize)
        .map(|i| {
            let n = i as i64;
            let unless = |missing: bool, value: Value| {
                if missing {
                    Value::Missing
                } else {
                    value
                }
            };
            [
                unless(in_missing_run(i), Value::Str("s".repeat(i % 3))),
                Value::Missing,
                unless(i < 600, Value::float(n as f64 * 0.5)),
                unless(i % 16 < 8, date(n - 515)),
                unless(i >= 504 && i % 2 == 1, datetime(n * 86_399)),
            ]
        })
        .collect()
}

/// Each case: its fixture's name and text, the table it was saved from, and
/// the rows of each row group that its footer lists.
fn cases() -> [(&'static str, &'static str, Table, Vec<i64>); 3] {
    [
        (
            "table.parquet.b64",
            include_str!("fixtures/parquet_bytes/table.parquet.b64"),
            table(rows()),
            vec![1_024, 6],
        ),
        (
            "nulls.parquet.b64",
            include_str!("fixtures/parquet_bytes/nulls.parquet.b64"),
            table(nulls()),
            vec![1_024, 6],
        ),
        (
            "empty.parquet.b64",
            include_str!("fixtures/parquet_bytes/empty.parquet.b64"),
            table(Vec::new()),
            vec![],
        ),
    ]
}

/// How the saved bytes differ from the fixture's, with all of them as the
/// base64 lines that would replace it; `None` when they are the same.
fn difference(name: &str, fixture: &str, saved: &[u8]) -> Option<String> {
    // A checkout may end the lines with CRLF; base64 itself has no whitespace.
    let text = fixture
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    let fixed = BASE64_STANDARD.decode(text).unwrap();
    if fixed == saved {
        return None;
    }
    let first = fixed
        .iter()
        .zip(saved)
        .position(|(fixed, saved)| fixed != saved)
        .unwrap_or(fixed.len().min(saved.len()));
    let encoded = BASE64_STANDARD.encode(saved);
    let lines = encoded
        .as_bytes()
        .chunks(LINE)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "{name}: the saved bytes differ from the fixture's from byte {first} \
         ({} fixed, {} saved)\n-----BEGIN {name}-----\n{lines}\n-----END {name}-----",
        fixed.len(),
        saved.len()
    ))
}

/// D160 on #3030.
#[test]
fn every_saved_byte_is_the_1_0_writers() {
    let differences = cases()
        .into_iter()
        .filter_map(|(name, fixture, table, _)| {
            difference(name, fixture, &render(&table, &PROJECTION).unwrap())
        })
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "{}\nEvery 1.x release keeps these bytes (D160 on #3030). Only a defect \
         fix that names its rule replaces a fixture, with the saved bytes above.",
        differences.join("\n")
    );
}

/// D160 on #3030 and D264: the fixed tables saved a partition at a time, in
/// partitions of each of these sizes, the last holding the rest, which
/// include sizes that divide 1,024, the writer's row-group size, sizes that
/// do not, and sizes beyond the table.
#[test]
fn every_partition_size_saves_the_1_0_writers_bytes() {
    let mut differences = Vec::new();
    for (name, fixture, table, _) in cases() {
        for size in [1, 3, 7, 1_000, 1_023, 1_024, 1_025, 1_030, 2_048] {
            let rows = table.row_count();
            let partitions = (0..rows.max(1))
                .step_by(size)
                .map(|start| start..rows.min(start + size))
                .collect::<Vec<_>>();
            let (saved, _) = render_partitions(&table, &PROJECTION, &partitions).unwrap();
            differences.extend(
                difference(name, fixture, &saved)
                    .map(|difference| format!("partitions of {size}: {difference}")),
            );
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// D160 on #3030. The row groups show that the fixed table spans two.
#[test]
fn the_footer_names_parquet_rs_55_2_0_as_the_writer() {
    for (name, _, table, groups) in cases() {
        let saved = render(&table, &PROJECTION).unwrap();
        let reader = SerializedFileReader::new(Bytes::from(saved)).unwrap();
        let metadata = reader.metadata();
        let footer = (
            metadata.file_metadata().created_by(),
            metadata
                .row_groups()
                .iter()
                .map(|group| group.num_rows())
                .collect::<Vec<_>>(),
        );
        assert_eq!(footer, (Some(CREATED_BY), groups), "{name}");
    }
}
