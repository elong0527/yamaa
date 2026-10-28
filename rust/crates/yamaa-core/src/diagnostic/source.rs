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

/// A Dataset-JSON input's closed cause (DSJSON-5) with the facts that
/// locate it. A field is named by its `columns` entry, except an empty name,
/// which only its one-based position names.
#[derive(Clone, Copy, Debug)]
pub enum DatasetJsonCause<'a> {
    /// The bytes are not the representation's JSON text.
    Malformed,
    /// `datasetJSONVersion` is not a 1.1 release.
    Version,
    /// A member the profile reads is absent, repeated or of another JSON
    /// kind; `field` is the position of the `columns` entry that holds it.
    Member {
        member: &'a str,
        field: Option<usize>,
    },
    /// An empty or repeated column name.
    FieldName { field: &'a Field },
    /// A `dataType` that names no column type.
    DataType { field: &'a str, data_type: &'a str },
    /// A `types` entry the field's `dataType` does not admit.
    FieldType {
        field: &'a str,
        data_type: &'a str,
        declared: ColumnType,
    },
    /// The rows are not as many as `records` counts.
    Records,
    /// A row that is not an array of one value per column.
    RowWidth { record: usize },
    /// A value of another JSON kind than its column's `dataType` spells.
    Cell { record: usize, field: &'a str },
}

impl DatasetJsonCause<'_> {
    /// The cause's closed spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::Version => "version",
            Self::Member { .. } => "member",
            Self::FieldName { .. } => "field_name",
            Self::DataType { .. } => "data_type",
            Self::FieldType { .. } => "field_type",
            Self::Records => "records",
            Self::RowWidth { .. } => "row_width",
            Self::Cell { .. } => "cell",
        }
    }
}

/// A classified failure borrows the original error, declaration and codec spelling.
#[derive(Clone, Copy, Debug)]
pub enum SourceFinding<'a> {
    /// A SAS or Transport input the reader cannot decode (SAS-7), with the
    /// field that locates it: its name, or the position of a field whose
    /// name is empty or does not decode; and the `record` of a value that
    /// does not decode.
    Sas {
        dataset: &'a str,
        path: &'a str,
        cause: &'a str,
        field: Option<&'a Field>,
        record: Option<usize>,
    },
    /// A Dataset-JSON input the profile cannot read (DSJSON-5).
    DatasetJson {
        dataset: &'a str,
        path: &'a str,
        cause: DatasetJsonCause<'a>,
    },
    /// A Dataset-JSON value that does not parse as its field's type, which
    /// its `dataType` supplies or a `types` entry `declared` (DSJSON-3).
    DatasetJsonValue {
        dataset: &'a str,
        field: &'a str,
        target: ColumnType,
        value: &'a str,
        record: usize,
        declared: bool,
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
/// A field named by its name, or by its position when it has none.
fn field(value: &Field) -> V {
    match value {
        Field::Index(index) => V::from(*index),
        Field::Name(name) => text(name),
    }
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
            Self::DatasetJson { .. } => C::DatasetJsonInvalid,
            Self::DatasetJsonValue { .. } => C::DatasetJsonFieldParse,
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
            Self::Sas { dataset, .. }
            | Self::DatasetJson { dataset, .. }
            | Self::DatasetJsonValue { dataset, .. } => dataset,
            Self::OrdinalCollision { dataset, .. } => dataset,
        };
        let mut context = Context::from([("dataset".into(), text(dataset))]);
        let spec_path = match self {
            Self::Sas {
                path,
                cause,
                field: located,
                record,
                ..
            } => {
                context.insert("path".into(), text(path));
                context.insert("cause".into(), text(cause));
                if let Some(located) = located {
                    context.insert("field".into(), field(located));
                }
                if let Some(record) = record {
                    context.insert("record".into(), V::from(record));
                }
                if cause == "deleted_records" {
                    context.insert("hint".into(), text(SAS_DELETED_RECORDS_HINT));
                }
                // A `types` entry the field's storage does not admit fails
                // at the entry, as an unknown field does (SAS-3).
                match (cause, located) {
                    ("field_type", Some(Field::Name(name))) => {
                        format!("input.{dataset}.types.{name}")
                    }
                    _ => format!("input.{dataset}.path"),
                }
            }
            Self::DatasetJson { path, cause, .. } => {
                context.insert("path".into(), text(path));
                context.insert("cause".into(), text(cause.name()));
                match cause {
                    DatasetJsonCause::Malformed
                    | DatasetJsonCause::Version
                    | DatasetJsonCause::Records => {}
                    DatasetJsonCause::Member { member, field } => {
                        context.insert("member".into(), text(member));
                        if let Some(field) = field {
                            context.insert("field".into(), V::from(field));
                        }
                    }
                    DatasetJsonCause::FieldName { field: named } => {
                        context.insert("field".into(), field(named));
                    }
                    DatasetJsonCause::DataType { field, data_type } => {
                        context.insert("field".into(), text(field));
                        context.insert("data_type".into(), text(data_type));
                    }
                    // The `types` entry the field's `dataType` does not admit.
                    DatasetJsonCause::FieldType {
                        field,
                        data_type,
                        declared,
                    } => {
                        context.insert("field".into(), text(field));
                        context.insert("data_type".into(), text(data_type));
                        context.insert("type".into(), text(kind(declared)));
                        return Diagnostic {
                            code: self.code(),
                            spec_paths: vec![format!("input.{dataset}.types.{field}")],
                            context,
                            source_span: None,
                            operand_route: None,
                        };
                    }
                    DatasetJsonCause::RowWidth { record } => {
                        context.insert("record".into(), V::from(record));
                    }
                    DatasetJsonCause::Cell { record, field } => {
                        context.insert("record".into(), V::from(record));
                        context.insert("field".into(), text(field));
                    }
                }
                format!("input.{dataset}.path")
            }
            // A field its `dataType` types fails at the input's path, and one
            // a `types` entry narrows at that entry.
            Self::DatasetJsonValue {
                field,
                target,
                value,
                record,
                declared,
                ..
            } => {
                context.insert("field".into(), text(field));
                context.insert("type".into(), text(kind(target)));
                context.insert("value".into(), text(value));
                context.insert("record".into(), V::from(record));
                if declared {
                    format!("input.{dataset}.types.{field}")
                } else {
                    format!("input.{dataset}.path")
                }
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
                context.insert("field".into(), self::field(field));
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
