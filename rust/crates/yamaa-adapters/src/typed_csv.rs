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
    // The header is record 1.
    convert(parsed, types, 1)
}

/// Convert `parsed`, whose first record follows `before` records of the
/// same source (its header, skipped lines, or earlier partitions), so a
/// finding counts its record as a reading finding does (CSV-16, CSV-18).
pub(crate) fn convert(
    parsed: csv_source::CsvSource,
    types: PreparedTypes,
    before: usize,
) -> Result<ColumnTable, Error> {
    let rows = types
        .convert(parsed)
        .map_err(|error| Error::Typing(error.after(before)))?;
    ColumnTable::from_access(&rows).map_err(Error::Table)
}
