//! Append an engine-admitted generated field without copying existing columns.
use crate::{
    column_table::{Column, ColumnTable},
    csv_source::TextTableError,
    source_decoding::Error,
};
use yamaa_core::table::TableAccess;
use yamaa_core::{source_ordinal, specification::SourceDeclaration};

pub(crate) fn append(source: &SourceDeclaration, table: ColumnTable) -> Result<ColumnTable, Error> {
    append_at(source, table, 0)
}

/// Append the generated field to `table`, whose records follow `start`
/// records of the source: one partition of a source read by partition
/// (#3792). Each value is the record's position in the whole source, and
/// the field is admitted with the partition's own records.
pub(crate) fn append_at(
    source: &SourceDeclaration,
    table: ColumnTable,
    start: usize,
) -> Result<ColumnTable, Error> {
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
    let ordinal = ordinal.starting_at(start);
    let generated = Column::Int(
        (0..table.row_count())
            .map(|position| Some(ordinal.value(position).expect("admitted source position")))
            .collect(),
    );
    table
        .appended(ordinal.schema().clone(), generated)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
