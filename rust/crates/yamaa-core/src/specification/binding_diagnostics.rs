//! Project retained source-dependent binding findings without rebinding or IO.
use super::{BindFinding, SourceDeclaration};
use crate::json::Value as V;
use crate::{
    column_dependencies,
    diagnostic::{ConditionCode as C, Context, Diagnostic},
    expression::{CompileError, Context as Field},
    reference_binding, reference_scope,
    schema::quoted_diagnostic_text,
};
use std::vec;

fn text(value: &str) -> V {
    V::String(value.into())
}
fn context<const N: usize>(fields: [(&str, V); N]) -> Context {
    fields.into_iter().map(|(k, v)| (k.into(), v)).collect()
}
fn finding(code: C, paths: Vec<String>, context: Context) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: paths,
        context,
        source_span: None,
        operand_route: None,
    }
}

impl column_dependencies::Diagnostic {
    /// One canonical cause mapping for the dependency service and original compiler.
    pub fn diagnostic_code(&self) -> C {
        match self {
            Self::Cycle { .. } => C::ColumnDependencyCycle,
            Self::ForwardReference { .. } => C::ColumnForwardReference,
            Self::MissingKeyDerivation { .. } => C::ColumnMissingKeyDerivation,
            Self::KeyDependency { .. } => C::ColumnKeyDependency,
        }
    }
    /// Names and authored operation paths belong to the compiler's bound phase.
    /// Inconsistent caller-supplied metadata has no fabricated semantic projection.
    pub fn specification_diagnostic(
        &self,
        columns: &[String],
        paths: &[String],
    ) -> Option<Diagnostic> {
        let (site, values) = match self {
            Self::Cycle { columns: members } => {
                let (last, first) = members.split_last()?;
                if first.is_empty() || first.first()? != last {
                    return None;
                }
                let site = first
                    .iter()
                    .map(|&id| paths.get(id).cloned())
                    .collect::<Option<_>>()?;
                let cycle = members
                    .iter()
                    .map(|&id| columns.get(id).map(|v| text(v)))
                    .collect::<Option<_>>()?;
                (site, context([("cycle", V::Array(cycle))]))
            }
            Self::ForwardReference { column, dependency } => (
                vec![paths.get(*column)?.clone()],
                context([
                    ("column", text(columns.get(*column)?)),
                    ("dependency", text(columns.get(*dependency)?)),
                ]),
            ),
            Self::MissingKeyDerivation { column } => (
                vec![format!("columns.{}.derivation", columns.get(*column)?)],
                context([("column", text(columns.get(*column)?))]),
            ),
            Self::KeyDependency { column, dependency } => (
                vec![paths.get(*column)?.rsplit_once('.')?.0.into()],
                context([
                    ("column", text(columns.get(*column)?)),
                    ("dependency", text(columns.get(*dependency)?)),
                ]),
            ),
        };
        Some(finding(self.diagnostic_code(), site, values))
    }
}

impl BindFinding {
    /// The retained source identifies the already-selected driver. No source schema
    /// is queried here, and resource/parser policy failures remain outside this type.
    /// Every retained original-document binding family uses the common projection.
    pub fn diagnostics(&self, source: &SourceDeclaration) -> Option<Vec<Diagnostic>> {
        let (code, path, values) = match self {
            Self::Padding(diagnostic)
            | Self::Odm(diagnostic)
            | Self::AggregateDiagnostic(diagnostic)
            | Self::TextTemplate(diagnostic)
            | Self::Scalar(diagnostic)
            | Self::TextExtract(diagnostic)
            | Self::Temporal(diagnostic)
            | Self::Predicate(diagnostic) => return Some(vec![diagnostic.clone()]),
            Self::ProjectFunction(finding) => return Some(vec![finding.diagnostic()]),
            Self::Source(finding) => return Some(vec![finding.diagnostic()]),
            Self::Window(finding) => return Some(vec![finding.diagnostic()]),
            Self::Lookup(finding) => return Some(vec![finding.diagnostic()]),
            Self::RowFilterPhase {
                path,
                identifier,
                row,
                grouped,
                qualified,
            } => (
                C::RowFilterPhaseBoundary,
                path,
                context([
                    ("identifier", text(identifier)),
                    ("row", text(row)),
                    (
                        "available_phase",
                        text(if *qualified {
                            "row_construction"
                        } else {
                            "column_derivation"
                        }),
                    ),
                    (
                        "required_phase",
                        text(if *grouped {
                            "grouped_row_filter"
                        } else {
                            "row_filter"
                        }),
                    ),
                ]),
            ),
            Self::QualifiedReference {
                path,
                name,
                row,
                finding,
            } => {
                // Scope raises a grouping finding only for a read qualified
                // by the driver whose groups lack the variable, so the
                // qualifier names that driver: the grouped row template's
                // own `dataset`. A column-level window reads only a driver
                // every row template shares (WINDOW-11), so its grouped
                // templates are over that driver too. The specification's
                // driver is the input behind the first row template, which a
                // later template need not share (ROW-5, NAME-4).
                let driver = || name.split_once('.').map(|(driver, _)| text(driver));
                let (code, values) = match finding {
                    reference_scope::Finding::UnknownField => (
                        C::SourceUnknownReference,
                        context([("identifier", text(name))]),
                    ),
                    reference_scope::Finding::RowGroup => (
                        C::GroupedRowReference,
                        context([
                            ("identifier", text(name)),
                            ("row", row.as_deref().map_or(V::Null, text)),
                            ("dataset", driver()?),
                        ]),
                    ),
                    reference_scope::Finding::ColumnGroup => (
                        C::GroupedColumnReference,
                        context([("identifier", text(name)), ("dataset", driver()?)]),
                    ),
                    _ => return None,
                };
                (code, path, values)
            }
            Self::Aggregate {
                path,
                expression,
                error,
            } => {
                return Some(vec![error.diagnostic(
                    Field::Aggregate,
                    path,
                    expression,
                    0,
                )])
            }
            Self::Numeric {
                path,
                expression,
                error: CompileError::Invalid { failure, .. },
            } => {
                return Some(vec![failure.diagnostic(
                    Field::Compute,
                    path,
                    expression,
                    0,
                )])
            }
            Self::AggregateScope {
                path,
                expression,
                relation,
                driver,
            } => {
                let reason = match relation {
                    None => "a grouped row aggregate reads its row driver".into(),
                    Some(relation) => {
                        let driver = quoted_diagnostic_text(driver);
                        let relation = quoted_diagnostic_text(relation);
                        format!("a grouped row aggregate reads {driver}, not {relation}")
                    }
                };
                (
                    C::AggregateDriverScope,
                    path,
                    context([("expr", text(expression)), ("reason", V::String(reason))]),
                )
            }
            Self::AggregateGroupScope {
                path,
                expression,
                relation,
            } => {
                let relation = quoted_diagnostic_text(relation);
                (
                    C::AggregateDriverScope,
                    path,
                    context([
                        ("expr", text(expression)),
                        (
                            "reason",
                            V::String(format!(
                                "a qualified reduction groups on columns of {relation}"
                            )),
                        ),
                    ]),
                )
            }
            Self::QualifiedNumericReference {
                path,
                expression,
                identifier,
            } => (
                C::QualifiedNumericReference,
                path,
                context([("expr", text(expression)), ("identifier", text(identifier))]),
            ),
            Self::UnknownReference { path, name } => (
                C::SourceUnknownReference,
                path,
                context([("identifier", text(name))]),
            ),
            Self::OutputReference {
                path,
                name,
                row,
                finding,
            } => {
                let (code, values) = match finding {
                    reference_binding::Diagnostic::UnknownField => (
                        C::OutputUnknownReference,
                        context([("identifier", text(name))]),
                    ),
                    reference_binding::Diagnostic::UnresolvableName { dataset: 0 } => (
                        C::OutputUnresolvableReference,
                        context([
                            ("identifier", text(name)),
                            ("suggestion", text(&format!("{}.{name}", source.name))),
                        ]),
                    ),
                    // A row derivation reads a column this template leaves to the
                    // column phase (COLUMN-4), as a bare row read reports it.
                    reference_binding::Diagnostic::PhaseBoundary { .. } => (
                        C::RowDependencyPhaseBoundary,
                        context([
                            ("identifier", text(name)),
                            ("row", text(row.as_deref()?)),
                            ("available_phase", text("column_derivation")),
                            ("required_phase", text("row_construction")),
                        ]),
                    ),
                    _ => return None,
                };
                (code, path, values)
            }
            Self::Dependencies {
                columns,
                paths,
                diagnostics,
            } => {
                return diagnostics
                    .iter()
                    .map(|d| d.specification_diagnostic(columns, paths))
                    .collect()
            }
            Self::Numeric { .. } => return None,
        };
        Some(vec![finding(code, vec![path.clone()], values)])
    }
}
