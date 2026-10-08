//! Declared source types over admitted CSV, independent of physical table storage.
use crate::{
    conversion::convert,
    csv_source::CsvSource,
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use alloc::{collections::BTreeSet, format, string::String, vec, vec::Vec};
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
    /// Structural defects have no fabricated language diagnostic.
    pub fn diagnostic(&self, dataset: &str) -> Option<Diagnostic> {
        let (code, field, extra) = match self {
            Self::UnknownField { field } => (C::SourceTypeUnknownField, field, vec![]),
            Self::FieldParse {
                field,
                target,
                value,
            } => {
                let kind = match target {
                    ColumnType::Str => "str",
                    ColumnType::Int => "int",
                    ColumnType::Float => "float",
                    ColumnType::Date => "date",
                    ColumnType::DateTime => "datetime",
                };
                (
                    C::SourceTypeFieldParse,
                    field,
                    vec![("type", kind), ("value", value.as_str())],
                )
            }
            Self::InvalidDeclarations | Self::InvalidSource => return None,
        };
        Some(Diagnostic {
            code,
            spec_paths: vec![format!("input.{dataset}.types.{field}")],
            context: [("dataset", dataset), ("field", field.as_str())]
                .into_iter()
                .chain(extra)
                .map(|(key, value)| (key.into(), V::Scalar(Value::Str(value.into()))))
                .collect(),
            source_span: None,
            operand_route: None,
        })
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
