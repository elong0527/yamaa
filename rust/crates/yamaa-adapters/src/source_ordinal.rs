//! Encode an engine-admitted generated field without copying existing payload buffers.
use crate::{
    arrow_table::{self, ArrowTable},
    csv_source::TextTableError,
    specification_run::Error,
};
use arrow_array::{ArrayRef, Int64Array, RecordBatch};
use std::sync::Arc;
use yamaa_core::table::TableAccess;
use yamaa_core::{source_ordinal, specification::SourceDeclaration};

pub(crate) fn append(source: &SourceDeclaration, table: ArrowTable) -> Result<ArrowTable, Error> {
    let Some(ordinal) = source_ordinal::prepare(source, table.schema(), table.row_count())
        .map_err(|error| match error {
            source_ordinal::Error::ResourceLimit {
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
    ArrowTable::try_new_with_schema(ordinal.schema().clone(), batches, physical)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
