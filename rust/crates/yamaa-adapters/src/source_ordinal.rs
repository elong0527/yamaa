//! Append an engine-admitted generated field without copying existing columns.
use crate::{
    column_table::{Column, ColumnTable},
    columns::Values,
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
        .map_err(Error::Ordinal)?
    else {
        return Ok(table);
    };
    let ordinal = ordinal.starting_at(start);
    // The field's values are reserved before any is built: a frame without
    // columns claims its records without holding them, so the field would
    // be the first memory they take (#4782).
    let mut generated =
        Values::try_with_capacity(table.row_count()).ok_or_else(|| Error::Allocation {
            spec_paths: vec![format!("input.{}.ordinal", source.name)],
        })?;
    for position in 0..table.row_count() {
        generated.push(Some(
            ordinal.value(position).expect("admitted source position"),
        ));
    }
    table
        .appended(ordinal.schema().clone(), Column::Int(generated))
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
