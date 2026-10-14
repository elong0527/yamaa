//! Primary declaration/header witnesses from the complete held compilation owner.
//! Original Check/Build findings and execution checkpoints remain separate.
use super::*;
use crate::diagnostic::{ContextValue, Diagnostic};
use alloc::collections::BTreeSet;

impl PreparedSpecification {
    /// A metadata witness has no table values, activation or publication port.
    /// Failed selectors and cycles do not establish downstream field availability.
    pub fn metadata_source_findings(
        &self,
        schemas: &[&TableSchema],
        maximum: usize,
    ) -> Result<Vec<Diagnostic>, BindError> {
        match self.bind_sources(schemas) {
            Ok(plan) => {
                let findings = plan
                    .metadata_type_findings(maximum)
                    .map_err(|_| BindError::PredicateDiagnosticLimit)?;
                if !findings.is_empty() {
                    return Ok(findings);
                }
                let output = self.output_findings();
                if output.len() > maximum {
                    return Err(BindError::PredicateDiagnosticLimit);
                }
                Ok(output
                    .into_iter()
                    .map(|finding| finding.diagnostic())
                    .collect())
            }
            Err(BindError::Invalid(findings)) => {
                if findings.len() > maximum {
                    return Err(BindError::PredicateDiagnosticLimit);
                }
                let mut diagnostics = Vec::new();
                for finding in findings {
                    let projected = match finding {
                        BindFinding::Dependencies { columns, paths, diagnostics } => {
                            let cyclic = diagnostics.iter().filter_map(|finding| {
                                if let column_dependencies::Diagnostic::Cycle { columns } = finding { Some(columns.as_slice()) } else { None }
                            }).flatten().copied().collect::<BTreeSet<_>>();
                            diagnostics.iter().filter(|finding| {
                                !matches!(finding, column_dependencies::Diagnostic::ForwardReference { dependency, .. } if cyclic.contains(dependency))
                            }).map(|finding| finding.specification_diagnostic(&columns,&paths)).collect::<Option<Vec<_>>>()
                        }
                        finding => finding.diagnostics(self.source()),
                    }.ok_or(BindError::Internal)?;
                    if projected.len() > maximum.saturating_sub(diagnostics.len()) {
                        return Err(BindError::PredicateDiagnosticLimit);
                    }
                    diagnostics.extend(projected);
                }
                let failed = self.intermediates.failed_metadata_names(&diagnostics);
                diagnostics.retain(|finding| {
                    if finding.code.definition().condition
                        != crate::diagnostic::ConditionCode::NumericUnknownField
                            .definition()
                            .condition
                    {
                        return true;
                    }
                    let Some(ContextValue::Scalar(crate::value::Value::Str(identifier))) =
                        finding.context.get("identifier")
                    else {
                        return true;
                    };
                    !identifier
                        .split_once('.')
                        .is_some_and(|(namespace, _)| failed.contains(namespace))
                });
                Ok(diagnostics)
            }
            Err(error) => Err(error),
        }
    }
}
