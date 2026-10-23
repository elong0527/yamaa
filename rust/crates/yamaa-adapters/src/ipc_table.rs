//! Sanitized Arrow batches, and IPC streams of them, rebuilt from checked
//! tables. No files, callbacks or fallback.
use crate::arrow_table::{date_array, datetime_array, physical_schema};
use arrow_array::{ArrayRef, Float64Array, Int64Array, RecordBatch, RecordBatchOptions};
use arrow_ipc::writer::StreamWriter;
use serde::Serialize;
use std::{fmt, io::Write, sync::Arc};
use yamaa_core::{dataset::Dataset, limits::Limits};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    Limit {
        resource: &'static str,
        limit: usize,
        required: Option<usize>,
    },
    Representation {
        representation: &'static str,
    },
    InvalidTable,
    OutputIo,
    Internal,
}
impl fmt::Display for TableError {
    /// Stable host errors never disclose raw input or panic payloads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Limit { .. } => "table geometry exceeds fixed limit",
            Self::Representation { representation } => representation,
            Self::InvalidTable => "invalid table schema or value",
            Self::OutputIo => "table IPC output write failed",
            Self::Internal => "internal table codec failure",
        })
    }
}
impl std::error::Error for TableError {}
type Error = TableError;

pub(crate) fn shape_error(error: yamaa_core::limits::LimitExceeded) -> Error {
    Error::Limit {
        resource: error.resource,
        limit: error.limit,
        required: error.actual,
    }
}

/// Export only a checked owned dataset; this is not a generic untrusted table API.
/// The caller's execution policy has already bounded retained cell/text payloads.
pub(crate) fn encode_dataset(table: &yamaa_core::dataset::Dataset) -> Result<Vec<u8>, Error> {
    encode_dataset_projection(table, None)
}

fn encode_dataset_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
) -> Result<Vec<u8>, Error> {
    encode_table_projection(table, projection)
}

fn encode_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    write_table_projection(table, projection, &mut output)?;
    Ok(output)
}

/// The records of each batch a table is rebuilt in: the batches of every
/// engine table a host reads whole.
pub(crate) const BATCH_ROWS: usize = 65_536;

/// How many batches of `batch_rows` records hold `rows` records: one, empty,
/// for a table with no record.
pub(crate) fn batch_count(rows: usize, batch_rows: usize) -> usize {
    if rows == 0 {
        1
    } else {
        (rows - 1) / batch_rows + 1
    }
}

/// Lazily rebuild accepted values into sanitized Arrow batches.
/// No batch is exposed until the complete dataset has passed engine checks.
pub(crate) struct DatasetBatches<'a, T = Dataset> {
    table: &'a T,
    projection: Option<&'a [usize]>,
    start: usize,
    batch_rows: usize,
    remaining: usize,
}

pub(crate) fn table_batches<'a, T: TableAccess<Error = std::convert::Infallible>>(
    table: &'a T,
    projection: Option<&'a [usize]>,
) -> Result<DatasetBatches<'a, T>, Error> {
    Limits::table(table.row_count(), table.schema().columns().len()).map_err(shape_error)?;
    if projection.is_some_and(|columns| {
        columns
            .iter()
            .any(|&column| column >= table.schema().columns().len())
    }) {
        return Err(Error::InvalidTable);
    }
    let batch_rows = BATCH_ROWS;
    let batches = batch_count(table.row_count(), batch_rows);
    Ok(DatasetBatches {
        table,
        projection,
        start: 0,
        batch_rows,
        remaining: batches,
    })
}

impl<T: TableAccess<Error = std::convert::Infallible>> Iterator for DatasetBatches<'_, T> {
    type Item = Result<RecordBatch, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let rows = self.batch_rows.min(self.table.row_count() - self.start);
        let result = sanitized_batch(self.table, self.start, rows)
            .map_err(batch_error)
            .and_then(|batch| match self.projection {
                Some(projection) => batch.project(projection).map_err(|_| Error::InvalidTable),
                None => Ok(batch),
            });
        self.start += rows;
        self.remaining = if result.is_err() {
            0
        } else {
            self.remaining - 1
        };
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::from(self.remaining != 0), Some(self.remaining))
    }
}

impl<T: TableAccess<Error = std::convert::Infallible>> std::iter::FusedIterator
    for DatasetBatches<'_, T>
{
}

fn write_table_projection<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    projection: Option<&[usize]>,
    output: &mut impl Write,
) -> Result<(), Error> {
    let batches = table_batches(table, projection)?;
    let schema = physical_schema(table.schema());
    let schema = match projection {
        Some(projection) => Arc::new(
            schema
                .project(projection)
                .map_err(|_| Error::InvalidTable)?,
        ),
        None => schema,
    };
    let mut writer = StreamWriter::try_new(output, &schema).map_err(|_| Error::OutputIo)?;
    for batch in batches {
        writer.write(&batch?).map_err(|_| Error::OutputIo)?;
    }
    writer.finish().map_err(|_| Error::OutputIo)
}

/// Serialize the actual observation without a second transport byte policy.
pub fn serialize_json(value: &impl Serialize) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|_| Error::Internal)
}

pub(crate) fn batch_error(error: arrow_schema::ArrowError) -> Error {
    match error {
        arrow_schema::ArrowError::CastError(message)
            if message == crate::arrow_table::UTF8_OFFSETS =>
        {
            Error::Representation {
                representation: crate::arrow_table::UTF8_OFFSETS,
            }
        }
        _ => Error::Internal,
    }
}

/// Rebuild visible values into fresh arrays, removing all masked physical payloads
/// and out-of-slice buffers before the public IPC writer sees them.
pub(crate) fn sanitized_batch<T: TableAccess<Error = std::convert::Infallible>>(
    table: &T,
    start: usize,
    rows: usize,
) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for (column, field) in table.schema().columns().iter().enumerate() {
        let values =
            (start..start + rows).map(|row| table.cell(row, column).expect("validated coordinate"));
        arrays.push(match field.kind {
            ColumnType::Str => {
                Arc::new(crate::arrow_table::string_array(values.map(|v| match v {
                    ValueRef::Str(v) => Some(v),
                    _ => None,
                }))?)
            }
            ColumnType::Int => Arc::new(Int64Array::from_iter(values.map(|v| match v {
                ValueRef::Int(v) => Some(v),
                _ => None,
            }))),
            ColumnType::Float => Arc::new(Float64Array::from_iter(values.map(|v| match v {
                ValueRef::Float(v) => Some(v.get()),
                _ => None,
            }))),
            ColumnType::Date => Arc::new(date_array(
                &values
                    .map(|v| match v {
                        ValueRef::Date(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )?),
            ColumnType::DateTime => Arc::new(datetime_array(
                &values
                    .map(|v| match v {
                        ValueRef::DateTime(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )?),
        });
    }
    RecordBatch::try_new_with_options(
        physical_schema(table.schema()),
        arrays,
        &RecordBatchOptions::new().with_row_count(Some(rows)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_table::ArrowTable;
    use arrow_array::{Array, Date32Array, StringArray, StructArray, UInt8Array};
    use arrow_buffer::{Buffer, NullBuffer, OffsetBuffer, ScalarBuffer};
    use arrow_schema::DataType;
    use yamaa_core::table::{Column, TableSchema};

    /// A null cell keeps no hidden bytes in the exported stream: not the
    /// scalar or the text under its validity bit, nor a temporal child.
    #[test]
    fn exported_batches_erase_payloads_hidden_under_nulls() {
        let schema = TableSchema::new(vec![
            Column {
                name: "I".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "S".into(),
                kind: ColumnType::Str,
            },
            Column {
                name: "D".into(),
                kind: ColumnType::Date,
            },
        ])
        .unwrap();
        let nulls = || Some(NullBuffer::from(vec![false, true]));
        let hidden = Int64Array::new(ScalarBuffer::from(vec![424242, 7]), nulls());
        let strings = StringArray::new(
            OffsetBuffer::new(ScalarBuffer::from(vec![0, 6, 8])),
            Buffer::from(b"SECRETok".as_slice()),
            nulls(),
        );
        let physical = physical_schema(&schema);
        let DataType::Struct(fields) = physical.field(2).data_type() else {
            panic!("a date is a struct column");
        };
        let dates = StructArray::try_new(
            fields.clone(),
            vec![
                Arc::new(Date32Array::from(vec![i32::MIN, 0])),
                Arc::new(UInt8Array::from(vec![255, 2])),
            ],
            nulls(),
        )
        .unwrap();
        let batch = RecordBatch::try_new(
            physical,
            vec![Arc::new(hidden), Arc::new(strings), Arc::new(dates)],
        )
        .unwrap();
        let table = ArrowTable::try_new(schema, vec![batch]).unwrap();
        let output = encode_table_projection(&table, None).unwrap();
        assert!(!output.windows(6).any(|bytes| bytes == b"SECRET"));
        let batch = arrow_ipc::reader::StreamReader::try_new(output.as_slice(), None)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let integers = batch.column(0).as_any().downcast_ref::<Int64Array>();
        assert_eq!(integers.unwrap().values().as_ref(), [0, 7]);
        let date = batch.column(2).as_any().downcast_ref::<StructArray>();
        let date = date.unwrap();
        assert!(date.is_null(0));
        let days = date.column(0).as_any().downcast_ref::<Date32Array>();
        assert_eq!(days.unwrap().value(0), 0);
        let precision = date.column(1).as_any().downcast_ref::<UInt8Array>();
        assert_eq!(precision.unwrap().value(0), 0);
    }
}
