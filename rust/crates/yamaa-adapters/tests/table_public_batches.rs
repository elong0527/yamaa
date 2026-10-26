//! A run's accepted rows read as a stream (#3697): `PublicBatches` builds
//! one public batch per pull, holding the values of the table
//! `PublicTable::from_table` would build whole, and refuses exactly the
//! tables it refuses.
//!
//! The expectations derive from the documented geometry: the whole table is
//! built in batches of 65,536 records, a streamed batch holds at most the
//! records asked for and never crosses one of those batches, and building a
//! batch reads each of its cells once.
//!
//! The seam is `PublicBatches` over a table that counts its cell reads, and a
//! consumer that stops pulling, which neither the benchmark reader nor the
//! host fixtures reach.
#![cfg(feature = "arrow")]

use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{Array, Int64Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::ArrowError;
use yamaa_adapters::public_table::TableError;
use yamaa_adapters::public_table::{PublicBatches, PublicTable};
use yamaa_core::table::{CellError, Column, TableAccess, TableSchema, ValueRef};
use yamaa_core::value::{ColumnType, Value};

/// A table of `rows` records: an `ID` equal to the record's position and,
/// with `text`, a `TEXT` of those bytes in every record. It counts the reads
/// of each record's ID.
struct Counted {
    schema: TableSchema,
    ids: Vec<Value>,
    text: Option<String>,
    reads: Vec<AtomicUsize>,
}

impl Counted {
    fn new(rows: usize, text: Option<String>) -> Self {
        let mut columns = vec![Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        }];
        if text.is_some() {
            columns.push(Column {
                name: "TEXT".into(),
                kind: ColumnType::Str,
            });
        }
        Self {
            schema: TableSchema::new(columns).unwrap(),
            ids: (0..rows).map(|row| Value::Int(row as i64)).collect(),
            text,
            reads: (0..rows).map(|_| AtomicUsize::new(0)).collect(),
        }
    }

    /// How often each record's ID was read.
    fn reads(&self) -> Vec<usize> {
        self.reads
            .iter()
            .map(|reads| reads.load(Ordering::SeqCst))
            .collect()
    }
}

impl TableAccess for Counted {
    type Error = Infallible;

    fn schema(&self) -> &TableSchema {
        &self.schema
    }

    fn row_count(&self) -> usize {
        self.ids.len()
    }

    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        match (column, &self.text) {
            (0, _) if row < self.ids.len() => {
                self.reads[row].fetch_add(1, Ordering::SeqCst);
                Ok(ValueRef::from(&self.ids[row]))
            }
            (1, Some(text)) if row < self.ids.len() => Ok(ValueRef::Str(text)),
            _ => Err(CellError::OutOfBounds { row, column }),
        }
    }
}

impl TableAccess for &Counted {
    type Error = Infallible;

    fn schema(&self) -> &TableSchema {
        (*self).schema()
    }

    fn row_count(&self) -> usize {
        (*self).row_count()
    }

    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        (*self).cell(row, column)
    }
}

/// A counted table the batches own while the test reads its counts.
struct Shared(Arc<Counted>);

impl TableAccess for Shared {
    type Error = Infallible;

    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }

    fn row_count(&self) -> usize {
        self.0.row_count()
    }

    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.0.cell(row, column)
    }
}

fn rows(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

/// The IDs of a batch.
fn ids(batch: &RecordBatch) -> Vec<i64> {
    let ids = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    (0..ids.len()).map(|row| ids.value(row)).collect()
}

/// A consumer that stops pulling stops production: read through an Arrow C
/// stream, two pulls of two-record batches read the IDs of the first four
/// records, once each, and nothing is read before the first pull or after
/// the consumer releases the stream.
#[test]
fn a_consumer_that_stops_pulling_stops_production() {
    let source = Arc::new(Counted::new(10, None));
    let batches = PublicBatches::new(Shared(Arc::clone(&source)), vec![0], rows(2), None).unwrap();
    assert_eq!(source.reads(), [0; 10], "before the first pull");
    let schema = batches.schema().clone();
    let stream = FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
        batches.map(|batch| batch.map_err(|error| ArrowError::ExternalError(Box::new(error)))),
        schema,
    )));
    let mut reader = ArrowArrayStreamReader::try_new(stream).unwrap();
    let pulled = (0..2)
        .map(|_| ids(&reader.next().unwrap().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(pulled, [vec![0, 1], vec![2, 3]], "the batches pulled");
    assert_eq!(
        source.reads(),
        [1, 1, 1, 1, 0, 0, 0, 0, 0, 0],
        "after two pulls"
    );
    drop(reader);
    assert_eq!(
        source.reads(),
        [1, 1, 1, 1, 0, 0, 0, 0, 0, 0],
        "after the release"
    );
}

/// Streamed batches hold the whole table's values in its order, under its
/// schema: three-record batches over 65,541 records are 21,845 of three,
/// then the one record that ends the whole table's first batch, then three
/// and two.
#[test]
fn streamed_batches_hold_the_whole_table_and_never_cross_its_batches() {
    let source = Counted::new(65_541, Some("t".into()));
    let projection = [1, 0];
    let whole = PublicTable::from_table(&&source, &projection).unwrap();
    let batches = PublicBatches::new(&source, projection.to_vec(), rows(3), None)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut expected = vec![3; 21_845];
    expected.extend([1, 3, 2]);
    assert_eq!(
        batches
            .iter()
            .map(RecordBatch::num_rows)
            .collect::<Vec<_>>(),
        expected,
        "the batch sizes"
    );
    let mut start = 0;
    for batch in &batches {
        assert_eq!(batch.schema(), *whole.schema(), "the schema at {start}");
        let (index, offset) = (start / 65_536, start % 65_536);
        let rows = whole.batches()[index].slice(offset, batch.num_rows());
        assert_eq!(batch, &rows, "the batch at {start}");
        start += batch.num_rows();
    }
    let text = batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(text.value(0), "t", "the projected text column comes first");
}

/// A table with no record is one empty batch, as the whole table is.
#[test]
fn a_table_with_no_record_is_one_empty_batch() {
    let source = Counted::new(0, Some("t".into()));
    let whole = PublicTable::from_table(&&source, &[0, 1]).unwrap();
    let batches = PublicBatches::new(&source, vec![0, 1], rows(2), None)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(batches, whole.batches(), "one empty batch");
    assert_eq!(batches[0].num_rows(), 0);
}

/// The tables the whole view refuses are refused before any batch: a
/// projection outside the table, and a batch of the whole table whose text
/// in one column needs more than Arrow `Utf8`'s 32-bit offsets, 65,536
/// records of 32,768 bytes, 2^31 bytes in all.
#[test]
fn a_table_the_whole_view_refuses_is_refused_before_any_batch() {
    let long = Counted::new(65_536, Some("t".repeat(32_768)));
    let short = Counted::new(3, Some("t".into()));
    let utf8 = TableError::Representation {
        representation: "Arrow Utf8 signed 32-bit offsets",
    };
    for (label, source, projection, expected) in [
        ("text beyond the offsets", &long, vec![0], utf8),
        (
            "a projection outside",
            &short,
            vec![2],
            TableError::InvalidTable,
        ),
    ] {
        let streamed = PublicBatches::new(source, projection.clone(), rows(2), None).map(|_| ());
        assert_eq!(
            source.reads().iter().sum::<usize>(),
            0,
            "{label}: no ID read"
        );
        let whole = PublicTable::from_table(&source, &projection).map(|_| ());
        assert_eq!((whole, streamed), (Err(expected), Err(expected)), "{label}");
    }
}
