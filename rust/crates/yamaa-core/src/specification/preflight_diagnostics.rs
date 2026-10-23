//! Own original-document preflight vocabulary and authored geometry at its cause.
use super::PreflightFinding;
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Context, Diagnostic},
    value::ColumnType,
};
use std::vec;

fn text(value: &str) -> V {
    V::String(value.into())
}
fn optional_text(value: &Option<String>) -> V {
    value.as_deref().map_or(V::Null, text)
}
fn texts(values: &[String]) -> V {
    V::Array(values.iter().map(|value| text(value)).collect())
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
            Self::IntegerLiteralOverflow { path, value } => (
                C::LiteralIntegerOverflow,
                vec![path.clone()],
                context([
                    (
                        "actual",
                        V::Number((value.clone()).parse().expect("canonical integer")),
                    ),
                    ("minimum", V::from(i64::MIN)),
                    ("maximum", V::from(i64::MAX)),
                ]),
            ),
            Self::Report(finding) => return finding.diagnostic(),
            Self::Odm(diagnostic) | Self::Scalar(diagnostic) => return diagnostic.clone(),
            Self::ProjectFunction(finding) => return finding.diagnostic(),
            Self::ProjectCodelist(finding) => return finding.diagnostic(),
            Self::RowPhase {
                path,
                identifier,
                row,
            } => (
                C::RowDependencyPhaseBoundary,
                vec![path.clone()],
                context([
                    ("identifier", text(identifier)),
                    ("row", text(row)),
                    ("available_phase", text("column_derivation")),
                    ("required_phase", text("row_construction")),
                ]),
            ),
            Self::UndeclaredRowColumn { row_path, column } => (
                C::PreflightUndeclaredRowColumn,
                vec![format!("{row_path}.derivations.{column}")],
                context([("column", text(column))]),
            ),
            Self::DuplicateRowDefault {
                column,
                rows,
                not_row_local,
            } => (
                C::PreflightDuplicateRowDefault,
                vec![format!("columns.{column}.derivation")],
                context([
                    ("column", text(column)),
                    ("rows", texts(rows)),
                    ("not_row_local", text(not_row_local)),
                ]),
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
            Self::InvalidGroup {
                row_path,
                row,
                groups,
            } => (
                C::PreflightInvalidGroup,
                vec![format!("{row_path}.group_by")],
                context([("row", text(row)), ("group_by", texts(groups))]),
            ),
            Self::GroupReference {
                row_path,
                row,
                name,
                dataset,
            } => (
                C::PreflightGroupReference,
                vec![format!("{row_path}.group_by")],
                context([
                    ("row", text(row)),
                    ("identifier", text(name)),
                    ("dataset", text(dataset)),
                ]),
            ),
            Self::RowDriverUnavailable {
                row_path,
                row,
                dataset,
            } => (
                C::PreflightRowDriverUnavailable,
                vec![format!("{row_path}.dataset")],
                context([("row", text(row)), ("dataset", optional_text(dataset))]),
            ),
            Self::MissingDerivation { column } => (
                C::PreflightMissingDerivation,
                vec![format!("columns.{column}.derivation")],
                context([("column", text(column))]),
            ),
            Self::EmptyKeys => (
                C::PreflightEmptyKeys,
                vec!["keys".into()],
                context([
                    ("expected", text("at least one key column")),
                    ("actual", text("empty list")),
                ]),
            ),
            Self::DuplicateKey {
                position,
                identifier,
            } => (
                C::PreflightDuplicateKey,
                vec![format!("keys[{position}]")],
                context([("identifier", text(identifier))]),
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
                    None => context([("row", V::Null)]),
                },
            ),
            Self::DomainInputCollision { domain } => (
                C::PreflightDomainInputCollision,
                vec![format!("input.{domain}"), "domain".into()],
                context([("identifier", text(domain))]),
            ),
            Self::UnknownSourceProfile { dataset, path } => (
                C::SourceUnknownProfile,
                vec![format!("input.{dataset}.path")],
                context([("dataset", text(dataset)), ("path", text(path))]),
            ),
            Self::EmptyStringPresentUnsupported { dataset } => (
                C::SourceEmptyStringPresentUnsupported,
                vec![format!("input.{dataset}.empty_string")],
                context([("dataset", text(dataset))]),
            ),
            Self::OdmSourceOrdinal { dataset } => (
                C::OdmDeclaredOrdinal,
                vec![format!("input.{dataset}.ordinal")],
                context([("dataset", text(dataset)), ("declared", text("ordinal"))]),
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
