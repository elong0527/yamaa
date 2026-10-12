//! Borrowed ingestion facts, admitted as one ordered report before owned copies.
use super::{
    ConditionCode as C, Context, ContextValue as V, Diagnostic, ProjectionBudget, ProjectionError,
    MAX_PROJECTION_BYTES,
};
use crate::{
    csv_source::{Field, ProfileCause},
    value::{ColumnType, Value},
};
use alloc::{format, string::ToString, vec};

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
fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
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
    fn admit(self, budget: &mut ProjectionBudget) -> Result<(), ProjectionError> {
        budget.charge(128)?;
        let code = self.code().definition();
        budget.text(code.phase)?;
        budget.text(code.condition)?;
        if let Some(requirement) = code.requirement {
            budget.text(requirement)?;
        }
        let dataset = match self {
            Self::Csv { dataset, .. }
            | Self::UnknownField { dataset, .. }
            | Self::FieldParse { dataset, .. }
            | Self::Parquet { dataset, .. } => dataset,
            Self::OrdinalCollision { dataset, .. } => dataset,
        };
        // Charge each component of the constructed specification path before formatting.
        budget.text("input.")?;
        budget.text(dataset)?;
        budget.field("dataset", dataset)?;
        match self {
            Self::OrdinalCollision { field, .. } => {
                budget.text(".ordinal")?;
                budget.field("field", field)?;
            }
            Self::Csv { field, path, .. } => {
                budget.text(".path")?;
                budget.field("path", path)?;
                budget.text("record")?;
                budget.charge(20)?;
                budget.text("field")?;
                match field {
                    Field::Index(_) => budget.charge(20)?,
                    Field::Name(name) => budget.text(name)?,
                }
            }
            Self::UnknownField { field, .. } => {
                budget.text(".types.")?;
                budget.text(field)?;
                budget.field("field", field)?;
            }
            Self::FieldParse {
                field,
                target,
                value,
                ..
            } => {
                budget.text(".types.")?;
                budget.text(field)?;
                budget.field("field", field)?;
                budget.field("type", kind(target))?;
                budget.field("value", value)?;
            }
            Self::Parquet { finding, path, .. } => {
                budget.text(".path")?;
                budget.field("path", path)?;
                match finding {
                    ParquetFinding::Invalid => {}
                    ParquetFinding::EmptyName { .. } => {
                        budget.text("field")?;
                        budget.charge(20)?;
                    }
                    ParquetFinding::DuplicateName { field } => budget.field("field", field)?,
                    ParquetFinding::Unsupported { field, stored_type } => {
                        budget.field("field", field)?;
                        budget.field("stored_type", stored_type)?;
                    }
                    ParquetFinding::Value { field, .. } => {
                        budget.field("field", field)?;
                        budget.text("row")?;
                        budget.charge(20)?;
                        budget.text("value")?;
                        budget.charge(20)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn diagnostic_with_limit(self, maximum: usize) -> Result<Diagnostic, ProjectionError> {
        let findings = [self];
        let report = admit_source_diagnostics(&findings, maximum)?;
        let diagnostic = report.iter().next().expect("single admitted finding");
        Ok(diagnostic)
    }
    pub fn diagnostic(self) -> Result<Diagnostic, ProjectionError> {
        self.diagnostic_with_limit(MAX_PROJECTION_BYTES)
    }
    /// Unbounded owned compatibility for the existing error diagnostic methods.
    /// Public callers should migrate to complete-report admission before removal.
    pub fn owned_compatibility(self) -> Diagnostic {
        let dataset = match self {
            Self::Csv { dataset, .. }
            | Self::UnknownField { dataset, .. }
            | Self::FieldParse { dataset, .. }
            | Self::Parquet { dataset, .. } => dataset,
            Self::OrdinalCollision { dataset, .. } => dataset,
        };
        let mut context = Context::from([("dataset".into(), text(dataset))]);
        let spec_path = match self {
            Self::OrdinalCollision { field, .. } => {
                context.insert("field".into(), text(field));
                format!("input.{dataset}.ordinal")
            }
            Self::Csv {
                record,
                field,
                path,
                ..
            } => {
                context.insert("path".into(), text(path));
                context.insert("record".into(), V::Integer(record.to_string()));
                context.insert(
                    "field".into(),
                    match field {
                        Field::Index(index) => V::Integer(index.to_string()),
                        Field::Name(name) => text(name),
                    },
                );
                format!("input.{dataset}.path")
            }
            Self::UnknownField { field, .. } => {
                context.insert("field".into(), text(field));
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
                        context.insert("field".into(), V::Integer(field.to_string()));
                    }
                    ParquetFinding::DuplicateName { field } => {
                        context.insert("field".into(), text(field));
                    }
                    ParquetFinding::Unsupported { field, stored_type } => {
                        context.insert("field".into(), text(field));
                        context.insert("stored_type".into(), text(stored_type));
                    }
                    ParquetFinding::Value { field, row, value } => {
                        context.insert("field".into(), text(field));
                        context.insert("row".into(), V::Integer(row.to_string()));
                        context.insert("value".into(), V::Scalar(Value::Int(value)));
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

/// Whole-report admission holds borrowed facts; original allocation identity survives.
pub struct SourceDiagnosticReport<'a> {
    findings: &'a [SourceFinding<'a>],
}
impl<'a> SourceDiagnosticReport<'a> {
    pub fn findings(&self) -> &'a [SourceFinding<'a>] {
        self.findings
    }
    /// Owned materialization is available only after every record was admitted.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Diagnostic> + 'a {
        self.findings
            .iter()
            .map(|finding| finding.owned_compatibility())
    }
}
pub fn admit_source_diagnostics<'a>(
    findings: &'a [SourceFinding<'a>],
    maximum: usize,
) -> Result<SourceDiagnosticReport<'a>, ProjectionError> {
    if !findings.is_empty() {
        let mut budget = ProjectionBudget::new(maximum)?;
        for finding in findings {
            finding.admit(&mut budget)?;
        }
    }
    Ok(SourceDiagnosticReport { findings })
}
