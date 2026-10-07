//! Declared CSV source typing; shared conversion runs in stored row/column order.
use crate::{
    arrow_table::{ArrowTable, TableError, TableLimits},
    csv_source,
};
use std::{collections::BTreeSet, convert::Infallible};
use yamaa_core::{
    conversion::convert,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
#[derive(Debug)]
pub enum Error {
    Csv(csv_source::Error),
    Table(TableError),
    InvalidDeclarations,
    UnknownField {
        field: String,
    },
    FieldParse {
        field: String,
        target: ColumnType,
        value: String,
    },
}
struct Rows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Rows {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
/// Undeclared fields stay text. Source conversion never invokes a result handler.
/// Unknown declarations are reported before any cell conversion, in declaration order.
pub fn parse(
    content: &[u8],
    declarations: &[(String, ColumnType)],
    csv_limits: csv_source::Limits,
    table_limits: TableLimits,
) -> Result<ArrowTable, Error> {
    if declarations.len() > table_limits.max_columns {
        return Err(Error::InvalidDeclarations);
    }
    let mut names = BTreeSet::new();
    for (name, _) in declarations {
        if name.is_empty() || !names.insert(name.as_str()) {
            return Err(Error::InvalidDeclarations);
        }
    }
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
    for (name, _) in declarations {
        if !parsed.names.contains(name) {
            return Err(Error::UnknownField {
                field: name.clone(),
            });
        }
    }
    let schema = TableSchema::new(
        parsed
            .names
            .into_iter()
            .map(|name| {
                let kind = declarations
                    .iter()
                    .find(|(field, _)| field == &name)
                    .map_or(ColumnType::Str, |(_, kind)| *kind);
                Column { name, kind }
            })
            .collect(),
    )
    .expect("CSV profile admitted unique nonempty header");
    let mut rows = Vec::with_capacity(row_count);
    for record in parsed.records {
        let mut converted = Vec::with_capacity(column_count);
        for (column, field) in record.into_iter().enumerate() {
            converted.push(match field {
                None => Value::Missing,
                Some(text) => {
                    let field = &schema.columns()[column];
                    let value = Value::Str(text);
                    match convert(&value, field.kind) {
                        Ok(value) => value,
                        Err(_) => {
                            let Value::Str(text) = value else {
                                unreachable!()
                            };
                            return Err(Error::FieldParse {
                                field: field.name.clone(),
                                target: field.kind,
                                value: text,
                            });
                        }
                    }
                }
            });
        }
        rows.push(converted);
    }
    let rows = Rows { schema, rows };
    let batch = crate::table_transport::sanitized_batch(&rows, 0, row_count)
        .map_err(|error| Error::Table(TableError::Arrow(error)))?;
    ArrowTable::try_new(rows.schema, vec![batch], table_limits).map_err(Error::Table)
}
