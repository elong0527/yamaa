//! Complete inherited-column dependency findings, admitted before any projection.
use super::{ConditionCode as C, ContextValue, Diagnostic, ProjectionBudget, ProjectionError};
use crate::{schema::InheritanceDependencyIssue as Issue, value::Value};
use alloc::{format, vec, vec::Vec};

/// Preserve all dependency findings in the resolver's order. Cycle membership is
/// the unresolved authored column list, rather than a closed traversal loop.
pub fn inheritance_dependency_diagnostics(
    issues: &[Issue],
    maximum: usize,
) -> Result<Vec<Diagnostic>, ProjectionError> {
    let mut budget = ProjectionBudget::new(maximum)?;
    for issue in issues {
        // The report envelope, diagnostic metadata and ordered context containers.
        budget.charge(256)?;
        match issue {
            Issue::Unknown { column, dependency } => {
                budget.text("columns.")?;
                budget.text(column)?;
                budget.text(".derivation")?;
                budget.field("column", column)?;
                budget.field("dependency", dependency)?;
            }
            Issue::Cycle { columns } => {
                budget.text("columns")?;
                budget.text("cycle")?;
                for column in columns {
                    // Each retained sequence member also owns a typed container
                    // and contributes separators, even for an empty spelling.
                    budget.charge(16)?;
                    budget.text(column)?;
                }
            }
        }
    }
    Ok(issues
        .iter()
        .map(|issue| {
            let mut context = super::Context::new();
            let (code, path) = match issue {
                Issue::Unknown { column, dependency } => {
                    context.insert(
                        "column".into(),
                        ContextValue::Scalar(Value::Str(column.clone())),
                    );
                    context.insert(
                        "dependency".into(),
                        ContextValue::Scalar(Value::Str(dependency.clone())),
                    );
                    (
                        C::InheritanceUnknownDependency,
                        format!("columns.{column}.derivation"),
                    )
                }
                Issue::Cycle { columns } => {
                    context.insert(
                        "cycle".into(),
                        ContextValue::Sequence(
                            columns
                                .iter()
                                .map(|column| ContextValue::Scalar(Value::Str(column.clone())))
                                .collect(),
                        ),
                    );
                    (C::ColumnDependencyCycle, "columns".into())
                }
            };
            Diagnostic {
                code,
                spec_paths: vec![path],
                context,
                source_span: None,
                operand_route: None,
            }
        })
        .collect())
}
