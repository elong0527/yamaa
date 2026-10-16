//! Retained public Arrow arrays; temporal precision stays in the execution table.
use crate::{
    arrow_table::ArrowTable,
    ipc_table::{self, TableError as Error},
};
use arrow_array::{
    Array, ArrayRef, Date32Array, RecordBatch, RecordBatchOptions, StructArray,
    TimestampSecondArray,
};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use std::sync::Arc;
use yamaa_core::{table::TableAccess, value::ColumnType};

#[derive(Debug, Clone)]
pub struct PublicTable {
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    rows: usize,
}
impl PublicTable {
    pub fn columns(&self) -> &arrow_schema::Fields {
        self.schema.fields()
    }
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }
    pub fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }
    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn decode(input: &[u8]) -> Result<Self, Error> {
        Self::from_snapshot(&ipc_table::decode_snapshot(input)?)
    }

    /// Primitive buffers are shared with the snapshot. Temporal values project
    /// the existing child buffer and the parent validity bitmap, retaining the
    /// established public epoch representation without leaking precision fields.
    pub fn from_snapshot(table: &ArrowTable) -> Result<Self, Error> {
        let fields = table
            .schema()
            .columns()
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let kind = match column.kind {
                    ColumnType::Str => table.batches().first().map_or(DataType::Utf8, |batch| {
                        batch.schema().field(index).data_type().clone()
                    }),
                    ColumnType::Int => DataType::Int64,
                    ColumnType::Float => DataType::Float64,
                    ColumnType::Date => DataType::Date32,
                    ColumnType::DateTime => {
                        DataType::Timestamp(arrow_schema::TimeUnit::Second, None)
                    }
                };
                Field::new(&column.name, kind, true)
            })
            .collect::<Vec<_>>();
        let schema = Arc::new(Schema::new(fields));
        let mut batches = Vec::with_capacity(table.batches().len());
        for batch in table.batches() {
            let mut arrays = Vec::<ArrayRef>::with_capacity(batch.num_columns());
            for (column, array) in table.schema().columns().iter().zip(batch.columns()) {
                let public: ArrayRef = match column.kind {
                    ColumnType::Date | ColumnType::DateTime => {
                        let temporal = array
                            .as_any()
                            .downcast_ref::<StructArray>()
                            .ok_or(Error::InvalidTable)?;
                        match column.kind {
                            ColumnType::Date => {
                                let value = temporal
                                    .column(0)
                                    .as_any()
                                    .downcast_ref::<Date32Array>()
                                    .ok_or(Error::InvalidTable)?;
                                Arc::new(Date32Array::new(
                                    value.values().clone(),
                                    temporal.nulls().cloned(),
                                ))
                            }
                            _ => {
                                let value = temporal
                                    .column(0)
                                    .as_any()
                                    .downcast_ref::<TimestampSecondArray>()
                                    .ok_or(Error::InvalidTable)?;
                                Arc::new(TimestampSecondArray::new(
                                    value.values().clone(),
                                    temporal.nulls().cloned(),
                                ))
                            }
                        }
                    }
                    _ => array.clone(),
                };
                arrays.push(public);
            }
            let options = RecordBatchOptions::new().with_row_count(Some(batch.num_rows()));
            batches.push(
                RecordBatch::try_new_with_options(schema.clone(), arrays, &options)
                    .map_err(|_| Error::InvalidTable)?,
            );
        }
        Ok(Self {
            schema,
            batches,
            rows: table.row_count(),
        })
    }

    /// Each export owns an independent cursor and shared immutable buffer refs.
    pub fn stream(&self) -> arrow_array::ffi_stream::FFI_ArrowArrayStream {
        crate::arrow_bridge::export_stream(self.schema.clone(), self.batches.clone())
    }

    /// Storage/qualification encoding only; host views use the C Stream bridge.
    pub fn ipc(&self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        let mut writer =
            StreamWriter::try_new(&mut bytes, &self.schema).map_err(|_| Error::Internal)?;
        for batch in &self.batches {
            writer.write(batch).map_err(|_| Error::Internal)?;
        }
        writer.finish().map_err(|_| Error::Internal)?;
        drop(writer);
        Ok(bytes)
    }
}
