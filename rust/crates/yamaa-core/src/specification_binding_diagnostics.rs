//! Project retained source-dependent binding findings without rebinding or IO.
use super::{BindFinding, SourceDeclaration};
use crate::{
    column_dependencies,
    diagnostic::{ConditionCode as C, Context, ContextValue as V, Diagnostic},
    numeric_compiler::CompileError,
    numeric_parser::ParseError,
    reference_binding, reference_scope,
    schema::{quoted_diagnostic_text, ValidationBudget},
    value::Value,
};
use alloc::{format, string::String, vec, vec::Vec};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
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
                (site, context([("cycle", V::Sequence(cycle))]))
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
            Self::Predicate(diagnostic) => return Some(vec![diagnostic.clone()]),
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
                let (code, values) = match finding {
                    reference_scope::Finding::RowGroup => (
                        C::GroupedRowReference,
                        context([
                            ("identifier", text(name)),
                            (
                                "row",
                                row.as_deref().map_or(V::Scalar(Value::Missing), text),
                            ),
                            ("dataset", text(&source.name)),
                        ]),
                    ),
                    reference_scope::Finding::ColumnGroup => (
                        C::GroupedColumnReference,
                        context([("identifier", text(name)), ("dataset", text(&source.name))]),
                    ),
                    _ => return None,
                };
                (code, path, values)
            }
            Self::Aggregate {
                path,
                expression,
                error,
            } => return Some(vec![error.diagnostic(path, expression)?]),
            Self::Numeric {
                path,
                expression,
                error: CompileError::Parse(ParseError::Grammar { failure, .. }),
            } => return Some(vec![failure.diagnostic(path, expression)?]),
            Self::AggregateScope {
                path,
                expression,
                relation,
            } => {
                let reason = match relation {
                    None => "a grouped row aggregate reads its row driver".into(),
                    Some(relation) => {
                        let mut budget = ValidationBudget::new(Default::default());
                        let driver = quoted_diagnostic_text(&source.name, &mut budget).ok()?;
                        let relation = quoted_diagnostic_text(relation, &mut budget).ok()?;
                        format!("a grouped row aggregate reads {driver}, not {relation}")
                    }
                };
                (
                    C::AggregateDriverScope,
                    path,
                    context([
                        ("expr", text(expression)),
                        ("reason", V::Scalar(Value::Str(reason))),
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
