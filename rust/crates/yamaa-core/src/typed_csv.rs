//! Declared source types over admitted CSV, independent of physical table storage.
use crate::{
    conversion::convert,
    csv_source::CsvSource,
    diagnostic::{Diagnostic, SourceFinding},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use core::convert::Infallible;

#[derive(Debug)]
pub enum Error {
    InvalidDeclarations,
    InvalidSource,
    UnknownField {
        field: String,
    },
    FieldParse {
        field: String,
        target: ColumnType,
        value: String,
    },
}
impl Error {
    /// Borrow the existing cause and complete source value; no conversion is retried.
    pub fn finding<'a>(&'a self, dataset: &'a str) -> Option<SourceFinding<'a>> {
        match self {
            Self::UnknownField { field } => Some(SourceFinding::UnknownField { dataset, field }),
            Self::FieldParse {
                field,
                target,
                value,
            } => Some(SourceFinding::FieldParse {
                dataset,
                field,
                target: *target,
                value,
            }),
            Self::InvalidDeclarations | Self::InvalidSource => None,
        }
    }
    /// Unbounded owned compatibility; structural defects retain the existing None.
    /// New callers should use complete borrowed report admission before removal.
    pub fn diagnostic(&self, dataset: &str) -> Option<Diagnostic> {
        self.finding(dataset)
            .map(SourceFinding::owned_compatibility)
    }
}

/// Admitted declarations borrow immutable compiler metadata for one conversion.
pub struct PreparedTypes<'a>(&'a [(String, ColumnType)]);
impl<'a> PreparedTypes<'a> {
    /// Declaration policy/shape is checked before callers decode source bytes.
    pub fn new(declarations: &'a [(String, ColumnType)], maximum: usize) -> Result<Self, Error> {
        if declarations.len() > maximum {
            return Err(Error::InvalidDeclarations);
        }
        let mut names = BTreeSet::new();
        for (name, _) in declarations {
            if name.is_empty() || !names.insert(name.as_str()) {
                return Err(Error::InvalidDeclarations);
            }
        }
        Ok(Self(declarations))
    }

    /// Unknown fields precede conversion in written declaration order. Cells
    /// convert in stored row/field order; source conversion never uses handlers.
    pub fn convert(self, source: CsvSource) -> Result<TypedRows, Error> {
        if source.names.is_empty() {
            return Err(Error::InvalidSource);
        }
        let schema = TableSchema::new(
            source
                .names
                .into_iter()
                .map(|name| {
                    let kind = self
                        .0
                        .iter()
                        .find(|(field, _)| field == &name)
                        .map_or(ColumnType::Str, |(_, kind)| *kind);
                    Column { name, kind }
                })
                .collect(),
        )
        .map_err(|_| Error::InvalidSource)?;
        if source
            .records
            .iter()
            .any(|record| record.len() != schema.columns().len())
        {
            return Err(Error::InvalidSource);
        }
        for (name, _) in self.0 {
            if !schema.columns().iter().any(|field| &field.name == name) {
                return Err(Error::UnknownField {
                    field: name.clone(),
                });
            }
        }
        let mut rows = Vec::with_capacity(source.records.len());
        for record in source.records {
            let mut converted = Vec::with_capacity(record.len());
            for (column, text) in record.into_iter().enumerate() {
                converted.push(match text {
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
        Ok(TypedRows { schema, rows })
    }
}

/// Owned normalized values remain valid after source and declarations are gone.
#[derive(Debug)]
pub struct TypedRows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for TypedRows {
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
