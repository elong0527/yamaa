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
    let Some(ordinal) = source_ingestion::ordinal(
        source,
        table.schema(),
        table.row_count(),
        source_ingestion::Limits {
            columns: limits.max_columns,
            cells: limits.max_cells,
        },
    )
    .map_err(Error::Ordinal)?
    else {
        return Ok(table);
    };
    let physical = arrow_table::physical_schema(ordinal.schema());
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
    ArrowTable::try_new(ordinal.schema().clone(), batches, limits)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
