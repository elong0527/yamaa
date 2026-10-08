//! Owned public columns; temporal precision stays in the internal execution table.
use crate::table_transport::{self, TableTransportError as Error};
use arrow_array::{
    ArrayRef, Date32Array, Float64Array, Int64Array, RecordBatch, StringArray, TimestampSecondArray,
};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use std::{
    io::{self, Write},
    sync::Arc,
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

#[derive(Debug)]
pub enum Values {
    Str(Vec<Option<String>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    Date(Vec<Option<i32>>),
    DateTime(Vec<Option<i64>>),
}
#[derive(Debug)]
pub struct PublicTable {
    columns: Vec<(String, Values)>,
    rows: usize,
}
impl PublicTable {
    pub fn columns(&self) -> &[(String, Values)] {
        &self.columns
    }
    pub fn rows(&self) -> usize {
        self.rows
    }
    pub fn decode(input: &[u8]) -> Result<Self, Error> {
        let table = table_transport::decode_snapshot(input)?;
        let mut columns = Vec::new();
        for (index, column) in table.schema().columns().iter().enumerate() {
            macro_rules! collect {
                ($pattern:pat => $value:expr) => {
                    (0..table.row_count())
                        .map(
                            |row| match table.cell(row, index).map_err(|_| Error::Internal)? {
                                ValueRef::Missing => Ok(None),
                                $pattern => Ok(Some($value)),
                                _ => Err(Error::InvalidTable),
                            },
                        )
                        .collect::<Result<Vec<_>, Error>>()?
                };
            }
            let values = match column.kind {
                ColumnType::Str => Values::Str(collect!(ValueRef::Str(v) => v.to_owned())),
                ColumnType::Int => Values::Int(collect!(ValueRef::Int(v) => v)),
                ColumnType::Float => Values::Float(collect!(ValueRef::Float(v) => v.get())),
                ColumnType::Date => {
                    Values::Date(collect!(ValueRef::Date(v) => crate::arrow_temporal::date_days(v)))
                }
                ColumnType::DateTime => Values::DateTime(
                    collect!(ValueRef::DateTime(v) => crate::arrow_temporal::datetime_seconds(v)),
                ),
            };
            columns.push((column.name.clone(), values));
        }
        Ok(Self {
            columns,
            rows: table.row_count(),
        })
    }
    pub fn ipc(&self) -> Result<Vec<u8>, Error> {
        let mut fields = Vec::new();
        let mut arrays = Vec::<ArrayRef>::new();
        for (name, values) in &self.columns {
            let (kind, array): (DataType, ArrayRef) = match values {
                Values::Str(v) => (
                    DataType::Utf8,
                    Arc::new(StringArray::from_iter(v.iter().map(Option::as_deref))),
                ),
                Values::Int(v) => (DataType::Int64, Arc::new(Int64Array::from(v.clone()))),
                Values::Float(v) => (DataType::Float64, Arc::new(Float64Array::from(v.clone()))),
                Values::Date(v) => (DataType::Date32, Arc::new(Date32Array::from(v.clone()))),
                Values::DateTime(v) => (
                    DataType::Timestamp(TimeUnit::Second, None),
                    Arc::new(TimestampSecondArray::from(v.clone())),
                ),
            };
            fields.push(Field::new(name, kind, true));
            arrays.push(array);
        }
        let schema = Arc::new(Schema::new(fields));
        let options = arrow_array::RecordBatchOptions::new().with_row_count(Some(self.rows));
        let batch = RecordBatch::try_new_with_options(schema.clone(), arrays, &options)
            .map_err(|_| Error::InvalidTable)?;
        struct Bytes(Vec<u8>);
        impl Write for Bytes {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > table_transport::MAX_INPUT_BYTES {
                    return Err(io::Error::other("public table byte limit"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut bytes = Bytes(Vec::new());
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).map_err(|_| Error::Internal)?;
        writer.write(&batch).map_err(|_| Error::Internal)?;
        writer.finish().map_err(|_| Error::Internal)?;
        drop(writer);
        Ok(bytes.0)
    }
}
