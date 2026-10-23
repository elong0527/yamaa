//! Retained public Arrow arrays; temporal precision stays in the execution table.
//!
//! [`PublicBatches`] hands out the same batches one pull at a time, for a run
//! whose output is read as a stream (#3697).
use crate::{
    arrow_table::ArrowTable,
    ipc_table::{self, TableError as Error, BATCH_ROWS},
};
use arrow_array::{
    Array, ArrayRef, Date32Array, RecordBatch, RecordBatchOptions, StructArray,
    TimestampSecondArray,
};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::sync::Arc;
use yamaa_core::{
    table::{TableAccess, TableSchema, ValueRef},
    value::ColumnType,
};

#[derive(Debug, Clone)]
pub struct PublicTable {
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
    rows: usize,
}
impl PublicTable {
    /// Materialize the checked output/report projection once into sanitized Arrow
    /// arrays. No IPC byte encoding or decoding is part of this host handoff.
    pub fn from_table<T: TableAccess<Error = std::convert::Infallible>>(
        table: &T,
        projection: &[usize],
    ) -> Result<Self, Error> {
        let logical = yamaa_core::table::TableSchema::new(
            projection
                .iter()
                .map(|&index| {
                    table
                        .schema()
                        .columns()
                        .get(index)
                        .cloned()
                        .ok_or(Error::InvalidTable)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|_| Error::InvalidTable)?;
        let batches =
            ipc_table::table_batches(table, Some(projection))?.collect::<Result<Vec<_>, _>>()?;
        let snapshot = ArrowTable::try_new(logical, batches).map_err(|_| Error::InvalidTable)?;
        Self::from_snapshot(&snapshot)
    }
    /// Construct an engine-built auxiliary table, such as the ODM clinical-item
    /// table or an issues table, from batches the engine assembled itself.
    /// Specification outputs keep `from_table`/`from_snapshot`, which admit only
    /// the five column types of the language profile; this constructor accepts
    /// any Arrow column type, including lists. Every batch must carry exactly
    /// `schema`, and the table geometry stays within the fixed engine limits.
    pub fn from_batches(schema: SchemaRef, batches: Vec<RecordBatch>) -> Result<Self, Error> {
        let mut rows = 0_usize;
        for batch in &batches {
            if batch.schema().as_ref() != schema.as_ref() {
                return Err(Error::InvalidTable);
            }
            rows = rows.checked_add(batch.num_rows()).ok_or(Error::Limit {
                resource: "table_rows",
                limit: yamaa_core::limits::Limits::TABLE_ROWS,
                required: None,
            })?;
        }
        yamaa_core::limits::Limits::table(rows, schema.fields().len()).map_err(|error| {
            Error::Limit {
                resource: error.resource,
                limit: error.limit,
                required: error.actual,
            }
        })?;
        Ok(Self {
            schema,
            batches,
            rows,
        })
    }
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

/// The public Arrow type of a column of the language's type `kind` in a
/// table the engine built: text is `Utf8`, as every batch it builds holds it.
fn public_type(kind: ColumnType) -> DataType {
    match kind {
        ColumnType::Str => DataType::Utf8,
        ColumnType::Int => DataType::Int64,
        ColumnType::Float => DataType::Float64,
        ColumnType::Date => DataType::Date32,
        ColumnType::DateTime => DataType::Timestamp(arrow_schema::TimeUnit::Second, None),
    }
}

/// The public schema of a table the engine built with the logical
/// `columns`: what [`PublicTable::from_table`] gives it.
fn public_schema(columns: &[yamaa_core::table::Column]) -> SchemaRef {
    Arc::new(Schema::new(
        columns
            .iter()
            .map(|column| Field::new(&column.name, public_type(column.kind), true))
            .collect::<Vec<_>>(),
    ))
}

/// The public schema of the output `compiled` builds, known before its run:
/// the schema of the table its run's host view holds, and of every batch a
/// stream of it hands out (#3697). None when a projected column is not one
/// the specification declares.
pub fn output_schema(
    compiled: &yamaa_core::specification::PreparedSpecification,
) -> Option<SchemaRef> {
    let columns = compiled
        .projection()
        .iter()
        .map(|name| {
            compiled
                .output()
                .columns()
                .iter()
                .find(|column| &column.name == name)
                .cloned()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(public_schema(&columns))
}

/// The columns `projection` names of `table`, as the host view's logical
/// schema; a position outside the table, or a repeated name, is refused.
fn projected<T: TableAccess>(table: &T, projection: &[usize]) -> Result<TableSchema, Error> {
    TableSchema::new(
        projection
            .iter()
            .map(|&index| {
                table
                    .schema()
                    .columns()
                    .get(index)
                    .cloned()
                    .ok_or(Error::InvalidTable)
            })
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(|_| Error::InvalidTable)
}

/// The rows of an accepted output handed out as public batches, one built per
/// pull (#3697): a run whose output is read as a stream holds its accepted
/// rows here instead of a [`PublicTable`]. Nothing is built ahead of a pull,
/// and nothing built is kept.
///
/// The batches hold the values, in the order and with the types,
/// [`PublicTable::from_table`] gives the same table and projection, under the
/// schema [`PublicBatches::schema`] gives before any batch is built. Each
/// holds at most `rows` records, and none crosses the boundary between two
/// of the whole table's batches, so a stream of batches can always be built
/// when the whole table can.
pub struct PublicBatches<T = yamaa_core::dataset::Dataset> {
    table: T,
    projection: Vec<usize>,
    logical: TableSchema,
    schema: SchemaRef,
    rows: usize,
    start: usize,
    done: bool,
}

impl<T: TableAccess<Error = Infallible>> PublicBatches<T> {
    /// The batches of `table`'s columns `projection`, at most `rows` records
    /// each. `table` is refused here, before any batch is built, exactly when
    /// [`PublicTable::from_table`] refuses it (see [`PublicBatches::check`]).
    pub fn new(table: T, projection: Vec<usize>, rows: NonZeroUsize) -> Result<Self, Error> {
        let logical = Self::check(&table, &projection)?;
        Ok(Self {
            schema: public_schema(logical.columns()),
            logical,
            projection,
            table,
            rows: rows.get().min(BATCH_ROWS),
            start: 0,
            done: false,
        })
    }

    /// Whether [`PublicTable::from_table`] can build the host view of
    /// `table`'s columns `projection`, checked without building it: the
    /// projection and the table's geometry, then the one refusal that depends
    /// on the values, a batch whose text in one column exceeds Arrow `Utf8`'s
    /// 32-bit offsets. The whole table is rebuilt in batches of 65,536
    /// records, every column of each, so each such batch of each text column
    /// is measured. Returns the projection's logical schema.
    pub fn check(table: &T, projection: &[usize]) -> Result<TableSchema, Error> {
        let logical = projected(table, projection)?;
        let columns = table.schema().columns();
        yamaa_core::limits::Limits::table(table.row_count(), columns.len())
            .map_err(ipc_table::shape_error)?;
        for (column, field) in columns.iter().enumerate() {
            if field.kind != ColumnType::Str {
                continue;
            }
            for start in (0..table.row_count()).step_by(BATCH_ROWS) {
                let end = table.row_count().min(start + BATCH_ROWS);
                let lengths = (start..end).filter_map(|row| match table.cell(row, column) {
                    Ok(ValueRef::Str(text)) => Some(text.len()),
                    _ => None,
                });
                if !crate::arrow_table::fits_utf8_offsets(lengths) {
                    return Err(Error::Representation {
                        representation: crate::arrow_table::UTF8_OFFSETS,
                    });
                }
            }
        }
        Ok(logical)
    }

    /// The public schema of every batch, known before any is built.
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// The records all the batches hold.
    pub fn rows(&self) -> usize {
        self.table.row_count()
    }

    /// Whether every batch has been built, so the next pull ends.
    pub fn ended(&self) -> bool {
        self.done
    }

    /// Build the public batch of rows `start..start + rows`.
    fn build(&self, start: usize, rows: usize) -> Result<RecordBatch, Error> {
        let batch = ipc_table::sanitized_batch(&self.table, start, rows)
            .map_err(ipc_table::batch_error)?
            .project(&self.projection)
            .map_err(|_| Error::InvalidTable)?;
        let snapshot = ArrowTable::try_new(self.logical.clone(), vec![batch])
            .map_err(|_| Error::InvalidTable)?;
        PublicTable::from_snapshot(&snapshot)?
            .batches
            .pop()
            .ok_or(Error::Internal)
    }
}

impl<T: TableAccess<Error = Infallible>> Iterator for PublicBatches<T> {
    type Item = Result<RecordBatch, Error>;

    /// Build the next batch, or end. A table with no record has one empty
    /// batch, as the whole table does; a batch that cannot be built ends the
    /// batches.
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let total = self.table.row_count();
        let boundary = (self.start / BATCH_ROWS + 1) * BATCH_ROWS;
        let end = total.min(self.start + self.rows).min(boundary);
        let batch = self.build(self.start, end - self.start);
        self.start = end;
        self.done = batch.is_err() || end >= total;
        Some(batch)
    }
}

impl<T: TableAccess<Error = Infallible>> std::iter::FusedIterator for PublicBatches<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::builder::{Int64Builder, ListBuilder, StringBuilder};

    fn issues_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new(
                "spec_paths",
                DataType::List(Arc::new(Field::new_list_field(DataType::Utf8, true))),
                true,
            ),
            Field::new(
                "operand_route",
                DataType::List(Arc::new(Field::new_list_field(DataType::Int64, true))),
                true,
            ),
        ]))
    }

    fn issues_batch(schema: &SchemaRef, rows: usize) -> RecordBatch {
        let mut paths = ListBuilder::new(StringBuilder::new());
        let mut route = ListBuilder::new(Int64Builder::new());
        for row in 0..rows {
            paths.values().append_value(format!("columns[{row}]"));
            paths.append(true);
            route.values().append_value(row as i64);
            route.values().append_value(-1);
            route.append(true);
        }
        RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(paths.finish()), Arc::new(route.finish())],
        )
        .unwrap()
    }

    #[test]
    fn engine_built_tables_admit_list_columns_and_count_rows_across_batches() {
        let schema = issues_schema();
        let table = PublicTable::from_batches(
            schema.clone(),
            vec![
                issues_batch(&schema, 2),
                issues_batch(&schema, 0),
                issues_batch(&schema, 3),
            ],
        )
        .unwrap();
        assert_eq!(table.rows(), 5);
        assert_eq!(table.batches().len(), 3);
        assert_eq!(table.schema(), &schema);
        assert!(matches!(
            table.columns()[0].data_type(),
            DataType::List(field) if field.data_type() == &DataType::Utf8
        ));
        let decoded =
            arrow_ipc::reader::StreamReader::try_new(table.ipc().unwrap().as_slice(), None)
                .unwrap()
                .map(|batch| batch.unwrap().num_rows())
                .sum::<usize>();
        assert_eq!(decoded, 5);
        let empty = PublicTable::from_batches(schema, Vec::new()).unwrap();
        assert_eq!(empty.rows(), 0);
    }

    #[test]
    fn engine_built_tables_refuse_a_batch_with_another_schema() {
        let schema = issues_schema();
        let other = Arc::new(Schema::new(vec![Field::new("ID", DataType::Utf8, true)]));
        let batch = RecordBatch::try_new(
            other,
            vec![Arc::new(arrow_array::StringArray::from(vec![Some("x")]))],
        )
        .unwrap();
        assert!(matches!(
            PublicTable::from_batches(schema.clone(), vec![issues_batch(&schema, 1), batch]),
            Err(Error::InvalidTable)
        ));
    }
}
