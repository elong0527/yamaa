//! One mapping/cut declaration shared by root, row and case dispatch.
use super::*;
use crate::mapping_cut::{AdmissionError, Cut, Mapping};

fn admission<T>(result: Result<T, AdmissionError>) -> Result<T, Refusal> {
    result.map_err(|error| match error {
        AdmissionError::Shape => Refusal::Internal,
        AdmissionError::Diagnostic(finding) => Refusal::Findings(vec![finding]),
    })
}
/// The executor's mapping or cut at `e`. Its source is a source binding,
/// whose declaration preparation admits first; it binds as the scope
/// decides, and its type is a lookup read's donor type or the catalog's
/// type of the name. A dictionary file is refused before binding.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let path = path(spec, e);
    let source = operand(operands, Slot::Source).ok_or(Refusal::Internal)?;
    let variable = match &spec.expr(source).kind {
        ExprKind::Ref(name) => name.as_str(),
        ExprKind::Source(read) => name(spec, read.variable).ok_or(Refusal::Internal)?,
        _ => return Err(Refusal::Internal),
    };
    let missing = handler(spec, handlers.missing.as_ref())?;
    let operation = match op {
        ScalarOp::Mapping {
            dict,
            case_sensitive,
        } => {
            let entries = match dict {
                ast::Dictionary::Inline(entries) => entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.text.clone(),
                            literal(&value.value, spec.cite(value.meta))?,
                        ))
                    })
                    .collect::<Result<Vec<_>, Refusal>>()?,
                ast::Dictionary::File(_) => Vec::new(),
            };
            let unmapped = handler(spec, handlers.unmapped.as_ref())?;
            let operation = MappingCut::Mapping(admission(Mapping::new(
                variable.into(),
                path.clone(),
                entries,
                *case_sensitive,
                missing,
                unmapped,
            ))?);
            if let ast::Dictionary::File(file) = dict {
                return Err(Refusal::Unsupported {
                    operation: "mapping_dictionary_resource",
                    path: spec.cite(file.meta),
                });
            }
            operation
        }
        ScalarOp::Cut {
            breaks,
            labels,
            closed,
        } => {
            let findings = Cut::declaration_findings(&path, breaks, labels);
            if !findings.is_empty() {
                return Err(Refusal::Findings(findings));
            }
            MappingCut::Cut(admission(Cut::new(
                variable.into(),
                path.clone(),
                breaks.clone(),
                labels.clone(),
                *closed == ast::Closed::Right,
                missing,
            ))?)
        }
        _ => return Err(Refusal::Internal),
    };
    Ok(Operation {
        operands: vec![Operand {
            read: Read::Source(source),
            path: format!("{path}.source"),
        }],
        kind: Kind::MappingCut(operation),
    })
}
