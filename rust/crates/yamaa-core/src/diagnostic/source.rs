//! Borrowed ingestion facts with complete diagnostic contexts.
use super::{ConditionCode as C, Context, Diagnostic};
use crate::json::Value as V;
use crate::{
    csv_source::{Field, ProfileCause},
    value::ColumnType,
};

/// Exact codec facts. Codec parsing and physical schema conversion remain external.
#[derive(Clone, Copy, Debug)]
pub enum ParquetFinding<'a> {
    Invalid,
    EmptyName {
        field: usize,
    },
    DuplicateName {
        field: &'a str,
    },
    Unsupported {
        field: &'a str,
        stored_type: &'a str,
    },
    Value {
        field: &'a str,
        row: usize,
        value: i64,
    },
}

/// A classified failure borrows the original error, declaration and codec spelling.
#[derive(Clone, Copy, Debug)]
pub enum SourceFinding<'a> {
    Sas {
        dataset: &'a str,
        path: &'a str,
        cause: &'a str,
    },
    Odm {
        dataset: &'a str,
        finding: &'a crate::odm_source::SchemaError,
    },
    OdmValues {
        dataset: &'a str,
        finding: crate::odm_source::MissingValues,
    },
    Csv {
        cause: ProfileCause,
        record: usize,
        field: &'a Field,
        dataset: &'a str,
        path: &'a str,
    },
    UnknownField {
        dataset: &'a str,
        field: &'a str,
    },
    OrdinalCollision {
        dataset: &'a str,
        field: &'a str,
    },
    FieldParse {
        dataset: &'a str,
        field: &'a str,
        target: ColumnType,
        value: &'a str,
    },
    Parquet {
        finding: ParquetFinding<'a>,
        dataset: &'a str,
        path: &'a str,
    },
}
/// SAS keeps records deleted in place until the dataset is rewritten.
const SAS_DELETED_RECORDS_HINT: &str = "rewrite the SAS dataset first, for example with \
    PROC COPY or a DATA step, to remove the records deleted in place";
fn text(value: &str) -> V {
    V::String(value.into())
}
fn kind(value: ColumnType) -> &'static str {
    match value {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}
impl SourceFinding<'_> {
    fn code(self) -> C {
        match self {
            Self::Sas { .. } => C::SasInvalid,
            Self::Odm { dataset, finding } => finding.finding(dataset).code(),
            Self::OdmValues { dataset, finding } => finding.finding(dataset).code(),
            Self::Csv { cause, .. } => cause.code(),
            Self::UnknownField { .. } => C::SourceTypeUnknownField,
            Self::OrdinalCollision { .. } => C::SourceOrdinalCollision,
            Self::FieldParse { .. } => C::SourceTypeFieldParse,
            Self::Parquet { finding, .. } => match finding {
                ParquetFinding::Invalid => C::ParquetInvalid,
                ParquetFinding::EmptyName { .. } => C::ParquetFieldNameEmpty,
                ParquetFinding::DuplicateName { .. } => C::ParquetFieldNameDuplicate,
                ParquetFinding::Unsupported { .. } => C::ParquetFieldTypeUnsupported,
                ParquetFinding::Value { .. } => C::ParquetFieldValueInvalid,
            },
        }
    }
    /// Materialize the complete retained fact without reading a source again.
    pub fn diagnostic(self) -> Diagnostic {
        if let Self::Odm { dataset, finding } = self {
            return finding.finding(dataset).diagnostic();
        }
        if let Self::OdmValues { dataset, finding } = self {
            return finding.finding(dataset).diagnostic();
        }
        let dataset = match self {
            Self::Odm { dataset, .. }
            | Self::OdmValues { dataset, .. }
            | Self::Csv { dataset, .. }
            | Self::UnknownField { dataset, .. }
            | Self::FieldParse { dataset, .. }
            | Self::Parquet { dataset, .. } => dataset,
            Self::Sas { dataset, .. } => dataset,
            Self::OrdinalCollision { dataset, .. } => dataset,
        };
        let mut context = Context::from([("dataset".into(), text(dataset))]);
        let spec_path = match self {
            Self::Sas { path, cause, .. } => {
                context.insert("path".into(), text(path));
                context.insert("cause".into(), text(cause));
                if cause == "deleted_records" {
                    context.insert("hint".into(), text(SAS_DELETED_RECORDS_HINT));
                }
                format!("input.{dataset}.path")
            }
            Self::OrdinalCollision { field, .. } => {
                context.insert("field".into(), text(field));
                format!("input.{dataset}.ordinal")
            }
            Self::Odm { .. } | Self::OdmValues { .. } => {
                unreachable!("ODM has its own complete projection")
            }
            Self::Csv {
                cause,
                record,
                field,
                path,
                ..
            } => {
                context.insert("path".into(), text(path));
                context.insert("record".into(), V::from(record));
                context.insert(
                    "field".into(),
                    match field {
                        Field::Index(index) => V::from(*index),
                        Field::Name(name) => text(name),
                    },
                );
                // Whether the input declares the mark tells a mark it does
                // not declare from a declared one that is absent (CSV-10).
                if let ProfileCause::ByteOrderMark { declared } = cause {
                    context.insert("declared".into(), V::Bool(declared));
                }
                format!("input.{dataset}.path")
            }
            // The undeclared field is the identifier the input's types write.
            Self::UnknownField { field, .. } => {
                context.insert("field".into(), text(field));
                context.insert("identifier".into(), text(field));
                format!("input.{dataset}.types.{field}")
            }
            Self::FieldParse {
                field,
                target,
                value,
                ..
            } => {
                context.insert("field".into(), text(field));
                context.insert("type".into(), text(kind(target)));
                context.insert("value".into(), text(value));
                format!("input.{dataset}.types.{field}")
            }
            Self::Parquet { finding, path, .. } => {
                context.insert("path".into(), text(path));
                match finding {
                    ParquetFinding::Invalid => {}
                    ParquetFinding::EmptyName { field } => {
                        context.insert("field".into(), V::from(field));
                    }
                    ParquetFinding::DuplicateName { field } => {
                        context.insert("field".into(), text(field));
                    }
                    ParquetFinding::Unsupported { field, stored_type } => {
                        context.insert("field".into(), text(field));
                        context.insert("stored_type".into(), text(stored_type));
                    }
                    // The position of an input record is its `record`, as
                    // a CSV or ODM failure names it (PARQUET-9, #3030 D200).
                    ParquetFinding::Value { field, row, value } => {
                        context.insert("field".into(), text(field));
                        context.insert("record".into(), V::from(row));
                        context.insert("value".into(), V::from(value));
                    }
                }
                format!("input.{dataset}.path")
            }
        };
        Diagnostic {
            code: self.code(),
            spec_paths: vec![spec_path],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}

/// Project the retained report in order without replaying source effects.
pub fn source_diagnostics<'a>(
    findings: &'a [SourceFinding<'a>],
) -> impl ExactSizeIterator<Item = Diagnostic> + 'a {
    findings.iter().map(|finding| finding.diagnostic())
}
