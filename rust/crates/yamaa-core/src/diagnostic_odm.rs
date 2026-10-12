//! Typed ODM causes preserve fixed-schema metadata and complete row scope.
use super::{
    ConditionCode as C, ContextValue as V, Diagnostic, ProjectionBudget, ProjectionError,
    MAX_PROJECTION_BYTES,
};
use crate::{
    dataset::AggregatePhase,
    value::{ColumnType, Value},
};
use alloc::{
    format,
    string::{String, ToString},
    vec,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OdmDeclared {
    Types,
    Schema,
    Ordinal,
}
impl OdmDeclared {
    pub fn name(self) -> &'static str {
        match self {
            Self::Types => "types",
            Self::Schema => "schema",
            Self::Ordinal => "ordinal",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum OdmFinding<'a> {
    Declared {
        dataset: &'a str,
        declared: OdmDeclared,
    },
    MissingFields {
        dataset: &'a str,
        fields: &'a [&'a str],
    },
    AmbiguousField {
        dataset: &'a str,
        field: &'a str,
        stored: &'a [String],
    },
    FieldType {
        dataset: &'a str,
        field: &'a str,
        stored_field: &'a str,
        kind: Option<ColumnType>,
    },
    MissingValues {
        dataset: &'a str,
        field: &'a str,
        record: usize,
        records: usize,
    },
    InvalidContext {
        path: &'a str,
        dataset: &'a str,
        row: Option<&'a str>,
    },
    NotUnique {
        path: &'a str,
        item: &'a str,
        row: Option<&'a str>,
        phase: AggregatePhase,
        scope: &'a [(String, Value)],
        differ: &'a [(String, alloc::vec::Vec<Value>)],
        records: usize,
    },
}
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn type_name(kind: Option<ColumnType>) -> &'static str {
    match kind {
        Some(ColumnType::Str) => "str",
        Some(ColumnType::Int) => "int",
        Some(ColumnType::Float) => "float",
        Some(ColumnType::Date) => "date",
        Some(ColumnType::DateTime) => "datetime",
        None => "unsupported",
    }
}
fn value_budget(value: &Value, budget: &mut ProjectionBudget) -> Result<(), ProjectionError> {
    match value {
        Value::Str(value) => budget.text(value),
        _ => budget.charge(64),
    }
}
impl OdmFinding<'_> {
    pub(crate) fn code(self) -> C {
        match self {
            Self::Declared {
                declared: OdmDeclared::Ordinal,
                ..
            } => C::OdmDeclaredOrdinal,
            Self::Declared { .. } | Self::FieldType { .. } => C::OdmFieldType,
            Self::MissingFields { .. } => C::OdmFieldMissing,
            Self::AmbiguousField { .. } => C::OdmFieldAmbiguous,
            Self::MissingValues { .. } => C::OdmValueMissing,
            Self::InvalidContext { .. } => C::OdmInvalidContext,
            Self::NotUnique {
                phase: AggregatePhase::RowConstruction,
                ..
            } => C::OdmRowNotUnique,
            Self::NotUnique { .. } => C::OdmNotUnique,
        }
    }
    pub(crate) fn admit(self, budget: &mut ProjectionBudget) -> Result<(), ProjectionError> {
        budget.charge(256)?;
        match self {
            Self::Declared { dataset, declared } => {
                budget.text("input.")?;
                budget.text(dataset)?;
                budget.text(declared.name())?;
                budget.field("dataset", dataset)?;
                budget.field("declared", declared.name())?;
            }
            Self::MissingFields { dataset, fields } => {
                budget.text("input.")?;
                budget.text(dataset)?;
                budget.text(".path")?;
                budget.field("dataset", dataset)?;
                budget.text("fields")?;
                for field in fields {
                    budget.text(field)?;
                    budget.charge(2)?;
                }
            }
            Self::AmbiguousField {
                dataset,
                field,
                stored,
            } => {
                budget.text("input.")?;
                budget.text(dataset)?;
                budget.text(".path")?;
                budget.field("dataset", dataset)?;
                budget.field("field", field)?;
                budget.text("stored")?;
                for name in stored {
                    budget.text(name)?;
                    budget.charge(2)?;
                }
            }
            Self::FieldType {
                dataset,
                field,
                stored_field,
                kind,
            } => {
                budget.text("input.")?;
                budget.text(dataset)?;
                budget.text(".path")?;
                budget.field("dataset", dataset)?;
                budget.field("field", field)?;
                budget.field("stored_field", stored_field)?;
                budget.field("stored_type", type_name(kind))?;
            }
            Self::MissingValues { dataset, field, .. } => {
                budget.text("input.")?;
                budget.text(dataset)?;
                budget.text(".path")?;
                budget.field("dataset", dataset)?;
                budget.field("field", field)?;
                budget.text("record")?;
                budget.text("records")?;
                budget.charge(40)?;
            }
            Self::InvalidContext { path, dataset, row } => {
                budget.text(path)?;
                budget.field("dataset", dataset)?;
                budget.text("row")?;
                if let Some(row) = row {
                    budget.text(row)?;
                }
            }
            Self::NotUnique {
                path,
                item,
                row,
                scope,
                differ,
                ..
            } => {
                budget.text(path)?;
                budget.field("item", item)?;
                budget.text("row")?;
                if let Some(row) = row {
                    budget.text(row)?;
                }
                budget.text("scope")?;
                budget.text("records")?;
                budget.charge(20)?;
                for (name, value) in scope {
                    budget.text(name)?;
                    value_budget(value, budget)?;
                    budget.charge(2)?;
                }
                budget.text("differ")?;
                for (name, values) in differ {
                    budget.text(name)?;
                    for value in values {
                        value_budget(value, budget)?;
                        budget.charge(2)?;
                    }
                }
                budget.text("repeated")?;
                budget.charge(5)?;
            }
        }
        Ok(())
    }
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        self.diagnostic_with_limit(MAX_PROJECTION_BYTES)
    }
    pub fn diagnostic_with_limit(self, maximum: usize) -> Result<Diagnostic, ProjectionError> {
        let mut budget = ProjectionBudget::new(maximum)?;
        self.admit(&mut budget)?;
        Ok(self.owned())
    }
    pub(crate) fn owned(self) -> Diagnostic {
        let (path, context) = match self {
            Self::Declared { dataset, declared } => (
                format!("input.{dataset}.{}", declared.name()),
                [
                    ("dataset".into(), text(dataset)),
                    ("declared".into(), text(declared.name())),
                ]
                .into(),
            ),
            Self::MissingFields { dataset, fields } => (
                format!("input.{dataset}.path"),
                [
                    ("dataset".into(), text(dataset)),
                    (
                        "fields".into(),
                        V::Sequence(fields.iter().map(|field| text(field)).collect()),
                    ),
                ]
                .into(),
            ),
            Self::AmbiguousField {
                dataset,
                field,
                stored,
            } => (
                format!("input.{dataset}.path"),
                [
                    ("dataset".into(), text(dataset)),
                    ("field".into(), text(field)),
                    (
                        "stored".into(),
                        V::Sequence(stored.iter().map(|name| text(name)).collect()),
                    ),
                ]
                .into(),
            ),
            Self::FieldType {
                dataset,
                field,
                stored_field,
                kind,
            } => (
                format!("input.{dataset}.path"),
                [
                    ("dataset".into(), text(dataset)),
                    ("field".into(), text(field)),
                    ("stored_field".into(), text(stored_field)),
                    ("stored_type".into(), text(type_name(kind))),
                ]
                .into(),
            ),
            Self::MissingValues {
                dataset,
                field,
                record,
                records,
            } => (
                format!("input.{dataset}.path"),
                [
                    ("dataset".into(), text(dataset)),
                    ("field".into(), text(field)),
                    ("record".into(), V::Integer(record.to_string())),
                    ("records".into(), V::Integer(records.to_string())),
                ]
                .into(),
            ),
            Self::InvalidContext { path, dataset, row } => (
                path.into(),
                [
                    ("dataset".into(), text(dataset)),
                    ("row".into(), row.map_or(V::Scalar(Value::Missing), text)),
                ]
                .into(),
            ),
            Self::NotUnique {
                path,
                item,
                row,
                scope,
                differ,
                records,
                ..
            } => (
                path.into(),
                [
                    ("item".into(), text(item)),
                    ("row".into(), row.map_or(V::Scalar(Value::Missing), text)),
                    (
                        "scope".into(),
                        V::Object(
                            scope
                                .iter()
                                .map(|(name, value)| (name.clone(), V::Scalar(value.clone())))
                                .collect(),
                        ),
                    ),
                    ("records".into(), V::Integer(records.to_string())),
                    (
                        "differ".into(),
                        V::Object(
                            differ
                                .iter()
                                .map(|(name, values)| {
                                    (
                                        name.clone(),
                                        V::Sequence(
                                            values.iter().cloned().map(V::Scalar).collect(),
                                        ),
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    ("repeated".into(), V::Scalar(Value::Bool(differ.is_empty()))),
                ]
                .into(),
            ),
        };
        Diagnostic {
            code: self.code(),
            spec_paths: vec![path],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
