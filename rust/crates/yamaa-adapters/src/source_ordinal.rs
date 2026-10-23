//! Append an engine-admitted generated field without copying existing columns.
use crate::{
    column_table::{Column, ColumnTable},
    csv_source::TextTableError,
    specification_run::Error,
};
use yamaa_core::table::TableAccess;
use yamaa_core::{source_ordinal, specification::SourceDeclaration};

pub(crate) fn append(source: &SourceDeclaration, table: ColumnTable) -> Result<ColumnTable, Error> {
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
    let generated = Column::Int(
        (0..table.row_count())
            .map(|position| Some(ordinal.value(position).expect("admitted source position")))
            .collect(),
    );
    table
        .appended(ordinal.schema().clone(), generated)
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}
