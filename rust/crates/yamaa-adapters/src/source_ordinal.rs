//! Encode an engine-admitted generated field without copying existing payload buffers.
use crate::{
    arrow_table::{self, ArrowTable, TableLimits},
    csv_source::TextTableError,
    specification_run::Error,
};
use arrow_array::{ArrayRef, Int64Array, RecordBatch};
use std::sync::Arc;
use yamaa_core::table::TableAccess;
use yamaa_engine::{source_ingestion, specification::SourceDeclaration};

pub(crate) fn append(
    source: &SourceDeclaration,
    table: ArrowTable,
    limits: TableLimits,
) -> Result<ArrowTable, Error> {
    append_inner(source, table, limits, None)
}

pub(crate) fn append_with_usage(
    source: &SourceDeclaration,
    table: ArrowTable,
    limits: TableLimits,
    storage_bytes: usize,
    usage: &mut crate::source_decode_usage::Usage,
) -> Result<ArrowTable, Error> {
    append_inner(source, table, limits, Some((storage_bytes, usage)))
}

fn append_inner(
    source: &SourceDeclaration,
    table: ArrowTable,
    limits: TableLimits,
    usage: Option<(usize, &mut crate::source_decode_usage::Usage)>,
) -> Result<ArrowTable, Error> {
    let Some(ordinal) = source_ingestion::ordinal(
        source,
        table.schema(),
        table.row_count(),
        source_ingestion::Limits {
            columns: limits.max_columns,
            cells: limits.max_cells,
        },
    )
    .map_err(|error| match error {
        source_ingestion::Error::ResourceLimit {
            resource,
            limit,
            actual,
        } => Error::ResourceLimit(crate::resource_diagnostics::Refusal::new(
            resource,
            limit,
            actual,
            vec![format!("input.{}", source.name)],
        )),
        error => Error::Ordinal(error),
    })?
    else {
        return Ok(table);
    };
    if let Some((storage_bytes, usage)) = usage {
        let required = table.row_count().checked_mul(32);
        let mut used = usage.storage_bytes;
        let amount = required.ok_or_else(|| {
            Error::Source(TextTableError::Table(arrow_table::TableError::Limit {
                resource: "storage_bytes",
                limit: storage_bytes,
                required: None,
            }))
        })?;
        crate::source_decode_usage::charge(&mut used, amount, storage_bytes, "storage_bytes")
            .map_err(|error| Error::Source(TextTableError::Table(error)))?;
        usage.storage(used);
    }
    // Imported large/view strings retain their physical types and shared buffers.
    let physical = if let Some(batch) = table.batches().first() {
        let mut fields = batch.schema().fields().to_vec();
        fields.push(std::sync::Arc::new(arrow_schema::Field::new(
            &ordinal
                .schema()
                .columns()
                .last()
                .expect("generated ordinal")
                .name,
            arrow_schema::DataType::Int64,
            true,
        )));
        Arc::new(arrow_schema::Schema::new(fields))
    } else {
        arrow_table::physical_schema(ordinal.schema())
    };
    let mut start = 0usize;
    let mut batches = Vec::with_capacity(table.batches().len());
    for batch in table.batches() {
        let end = start + batch.num_rows();
        let generated = Int64Array::from_iter_values(
            (start..end).map(|position| ordinal.value(position).expect("admitted source position")),
        );
        let mut arrays = batch.columns().to_vec();
        arrays.push(Arc::new(generated) as ArrayRef);
        batches.push(
            RecordBatch::try_new(physical.clone(), arrays).map_err(|error| {
                Error::Source(TextTableError::Table(arrow_table::TableError::Arrow(error)))
            })?,
        );
        start = end;
    }
    ArrowTable::try_new_with_schema(ordinal.schema().clone(), batches, limits, physical)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
