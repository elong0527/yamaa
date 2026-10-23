//! Declared CSV source typing; shared conversion runs in stored row/column order.
use crate::{
    column_table::{ColumnTable, TableError},
    csv_source,
};
use yamaa_core::{typed_csv::PreparedTypes, value::ColumnType};
#[derive(Debug)]
pub enum Error {
    Csv(csv_source::Error),
    Table(TableError),
    Typing(yamaa_core::typed_csv::Error),
}
/// Undeclared fields stay text. Source conversion never invokes a result handler.
/// Unknown declarations are reported before any cell conversion, in declaration order.
pub fn parse(content: &[u8], declarations: &[(String, ColumnType)]) -> Result<ColumnTable, Error> {
    let types = PreparedTypes::new(declarations).map_err(Error::Typing)?;
    let parsed = csv_source::parse(content).map_err(Error::Csv)?;
    convert(parsed, types)
}

pub(crate) fn convert(
    parsed: csv_source::CsvSource,
    types: PreparedTypes,
) -> Result<ColumnTable, Error> {
    let row_count = parsed.records.len();
    let column_count = parsed.names.len();
    yamaa_core::limits::Limits::table(row_count, column_count).map_err(|error| {
        Error::Table(TableError::Limit {
            resource: error.resource,
            limit: error.limit,
            required: error.actual,
        })
    })?;
    let rows = types.convert(parsed).map_err(Error::Typing)?;
    ColumnTable::from_access(&rows).map_err(Error::Table)
}
