//! Declared CSV source typing; shared conversion runs in stored row/column order.
use crate::{
    arrow_table::{ArrowTable, TableError, TableLimits},
    csv_source,
};
use yamaa_core::{table::TableAccess, typed_csv::PreparedTypes, value::ColumnType};
#[derive(Debug)]
pub enum Error {
    Csv(csv_source::Error),
    Table(TableError),
    Typing(yamaa_core::typed_csv::Error),
}
/// Undeclared fields stay text. Source conversion never invokes a result handler.
/// Unknown declarations are reported before any cell conversion, in declaration order.
pub fn parse(
    content: &[u8],
    declarations: &[(String, ColumnType)],
    csv_limits: csv_source::Limits,
    table_limits: TableLimits,
) -> Result<ArrowTable, Error> {
    let types =
        PreparedTypes::new(declarations, table_limits.max_columns).map_err(Error::Typing)?;
    let parsed = csv_source::parse(content, csv_limits).map_err(Error::Csv)?;
    let row_count = parsed.records.len();
    let column_count = parsed.names.len();
    for (resource, required, limit) in [
        ("columns", Some(column_count), table_limits.max_columns),
        ("batches", Some(1), table_limits.max_batches),
        ("rows", Some(row_count), table_limits.max_rows),
        (
            "cells",
            row_count.checked_mul(column_count),
            table_limits.max_cells,
        ),
    ] {
        if required.is_none_or(|n| n > limit) {
            return Err(Error::Table(TableError::Limit {
                resource,
                limit,
                required,
            }));
        }
    }
    let rows = types.convert(parsed).map_err(Error::Typing)?;
    let batch = crate::table_transport::sanitized_batch(&rows, 0, row_count)
        .map_err(|error| Error::Table(TableError::Arrow(error)))?;
    ArrowTable::try_new(rows.schema().clone(), vec![batch], table_limits).map_err(Error::Table)
}
