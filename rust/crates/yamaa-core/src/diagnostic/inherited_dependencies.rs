//! Complete inherited-column dependency findings in resolver order.
use super::{ConditionCode as C, Diagnostic};
use crate::json::Value as JsonValue;
use crate::resolution::Issue;
use std::vec;

/// Preserve all dependency findings in the resolver's order. A cycle names
/// its closed loop, as the single-file compiler does (#2844, decision 1).
pub fn inheritance_dependency_diagnostics(issues: &[Issue]) -> Vec<Diagnostic> {
    issues
        .iter()
        .map(|issue| {
            let mut context = super::Context::new();
            let (code, spec_paths) = match issue {
                Issue::Unknown { column, dependency } => {
                    context.insert("column".into(), JsonValue::String(column.clone()));
                    context.insert("dependency".into(), JsonValue::String(dependency.clone()));
                    (
                        C::InheritanceUnknownDependency,
                        vec![format!("columns.{column}.derivation")],
                    )
                }
                Issue::Cycle { cycle, paths } => {
                    context.insert(
                        "cycle".into(),
                        JsonValue::Array(
                            cycle
                                .iter()
                                .map(|column| JsonValue::String(column.clone()))
                                .collect(),
                        ),
                    );
                    // The paths name the cycle's own derivations, as the flat
                    // compiler reports them; each path also resolves through
                    // composed provenance to its contributing file (REQ-0651).
                    (C::ColumnDependencyCycle, paths.clone())
                }
            };
            Diagnostic {
                code,
                spec_paths,
                context,
                source_span: None,
                operand_route: None,
            }
        })
        .collect()
}
