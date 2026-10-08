//! Own original-document preflight vocabulary and authored geometry at its cause.
use super::PreflightFinding;
use crate::{
    diagnostic::{ConditionCode as C, Context, ContextValue as V, Diagnostic},
    value::{ColumnType, Value},
};
use alloc::{format, string::String, vec, vec::Vec};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}
fn optional_text(value: &Option<String>) -> V {
    value.as_deref().map_or(V::Scalar(Value::Missing), text)
}
fn texts(values: &[String]) -> V {
    V::Sequence(values.iter().map(|value| text(value)).collect())
}
fn context<const N: usize>(fields: [(&str, V); N]) -> Context {
    fields
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

impl PreflightFinding {
    /// Preserve the compiler's ordered findings and authored paths. This does not
    /// perform validation again, consult a source, or choose a host representation.
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, paths, context): (C, Vec<String>, Context) = match self {
            Self::ProjectFunction(finding) => return finding.diagnostic(),
            Self::UndeclaredRowColumn { index, column } => (
                C::PreflightUndeclaredRowColumn,
                vec![format!("rows[{index}].derivations.{column}")],
                context([("column", text(column))]),
            ),
            Self::DuplicateRowDefault { column, rows } => (
                C::PreflightDuplicateRowDefault,
                vec![format!("columns.{column}.derivation")],
                context([("column", text(column)), ("rows", texts(rows))]),
            ),
            Self::MissingRowDerivation { column, rows } => (
                C::PreflightMissingRowDerivation,
                vec![format!("columns.{column}.derivation")],
                context([("column", text(column)), ("rows", texts(rows))]),
            ),
            Self::ConflictingRowConstruction => (
                C::PreflightConflictingRowConstruction,
                vec!["filter".into(), "rows".into()],
                Context::new(),
            ),
            Self::InvalidGroup { index, row, groups } => (
                C::PreflightInvalidGroup,
                vec![format!("rows[{index}].group_by")],
                context([("row", text(row)), ("group_by", texts(groups))]),
            ),
            Self::GroupReference {
                index,
                row,
                name,
                dataset,
            } => (
                C::PreflightGroupReference,
                vec![format!("rows[{index}].group_by")],
                context([
                    ("row", text(row)),
                    ("identifier", text(name)),
                    ("dataset", text(dataset)),
                ]),
            ),
            Self::RowDriverUnavailable {
                index,
                row,
                dataset,
            } => (
                C::PreflightRowDriverUnavailable,
                vec![format!("rows[{index}].dataset")],
                context([("row", text(row)), ("dataset", optional_text(dataset))]),
            ),
            Self::MissingDerivation { column } => (
                C::PreflightMissingDerivation,
                vec![format!("columns.{column}.derivation")],
                context([("column", text(column))]),
            ),
            Self::UndeclaredKey { position, column } => (
                C::PreflightUndeclaredKey,
                vec![format!("keys[{position}]")],
                context([("column", text(column))]),
            ),
            Self::DriverUnavailable { dataset } => (
                C::PreflightDriverUnavailable,
                vec!["base".into()],
                match dataset {
                    Some(dataset) => context([("dataset", text(dataset))]),
                    None => context([("row", V::Scalar(Value::Missing))]),
                },
            ),
            Self::DomainInputCollision { domain } => (
                C::PreflightDomainInputCollision,
                vec![format!("input.{domain}"), "domain".into()],
                context([("identifier", text(domain))]),
            ),
            Self::RedundantSourceType {
                dataset,
                field,
                kind,
            } => (
                C::PreflightRedundantSourceType,
                vec![format!("input.{dataset}.types.{field}")],
                context([
                    ("dataset", text(dataset)),
                    ("field", text(field)),
                    (
                        "type",
                        text(match kind {
                            ColumnType::Str => "str",
                            ColumnType::Int => "int",
                            ColumnType::Float => "float",
                            ColumnType::Date => "date",
                            ColumnType::DateTime => "datetime",
                        }),
                    ),
                ]),
            ),
        };
        Diagnostic {
            code,
            spec_paths: paths,
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
