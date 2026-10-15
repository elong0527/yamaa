//! Typed ODM causes preserve fixed-schema metadata and complete row scope.
use super::{ConditionCode as C, Diagnostic};
use crate::json::Value as V;
use crate::{
    dataset::AggregatePhase,
    value::{ColumnType, Value},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OdmLocation {
    Column,
    Row,
    Intermediate,
    Derive,
}
impl OdmLocation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Row => "row",
            Self::Intermediate => "intermediate",
            Self::Derive => "derive",
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
        location: OdmLocation,
    },
    NotUnique {
        path: &'a str,
        item: &'a str,
        row: Option<&'a str>,
        phase: AggregatePhase,
        scope: &'a [(String, Value)],
        differ: &'a [(String, std::vec::Vec<Value>)],
        records: usize,
    },
}
fn text(value: &str) -> V {
    V::String(value.into())
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
    pub fn diagnostic(self) -> Diagnostic {
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
                        V::Array(fields.iter().map(|field| text(field)).collect()),
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
                        V::Array(stored.iter().map(|name| text(name)).collect()),
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
                    ("record".into(), V::from(record)),
                    ("records".into(), V::from(records)),
                ]
                .into(),
            ),
            Self::InvalidContext {
                path,
                dataset,
                location,
            } => (
                path.into(),
                [
                    ("dataset".into(), text(dataset)),
                    ("location".into(), text(location.name())),
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
                    ("row".into(), row.map_or(V::Null, text)),
                    (
                        "scope".into(),
                        V::Object(
                            scope
                                .iter()
                                .map(|(name, value)| (name.clone(), V::from(value.clone())))
                                .collect(),
                        ),
                    ),
                    ("records".into(), V::from(records)),
                    (
                        "differ".into(),
                        V::Object(
                            differ
                                .iter()
                                .map(|(name, values)| {
                                    (
                                        name.clone(),
                                        V::Array(values.iter().cloned().map(V::from).collect()),
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    ("repeated".into(), V::Bool(differ.is_empty())),
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
