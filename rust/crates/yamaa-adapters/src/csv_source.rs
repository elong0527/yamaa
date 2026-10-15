//! CSV physical-table boundary; pure profile admission is owned by core.
pub use yamaa_core::csv_source::{parse, CsvSource, Error, Field, ProfileCause};

/// Profile failures and Arrow/resource failures remain distinct to the compiler.
#[derive(Debug)]
pub enum TextTableError {
    Csv(Error),
    Table(crate::arrow_table::TableError),
}

/// Decode the undeclared-type CSV case into an owned canonical Arrow snapshot.
/// Declared types, producer schemas and ordinals require their separate admission;
/// callers must not silently route those declarations through this text-only helper.
pub fn parse_text_table(content: &[u8]) -> Result<crate::arrow_table::ArrowTable, TextTableError> {
    use crate::arrow_table::{physical_schema, ArrowTable, TableError};
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use std::sync::Arc;
    use yamaa_core::{
        table::{Column, TableSchema},
        value::ColumnType,
    };

    let parsed = parse(content).map_err(TextTableError::Csv)?;
    let rows = parsed.records.len();
    let columns = parsed.names.len();
    let schema = TableSchema::new(
        parsed
            .names
            .into_iter()
            .map(|name| Column {
                name,
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .expect("CSV admission established nonempty unique header names");
    let arrays: Vec<ArrayRef> = (0..columns)
        .map(|column| {
            Arc::new(StringArray::from_iter(
                parsed.records.iter().map(|row| row[column].as_deref()),
            )) as ArrayRef
        })
        .collect();
    let batch = RecordBatch::try_new(physical_schema(&schema), arrays)
        .map_err(|error| TextTableError::Table(TableError::Arrow(error)))?;
    ArrowTable::try_new(schema, vec![batch]).map_err(TextTableError::Table)
}
