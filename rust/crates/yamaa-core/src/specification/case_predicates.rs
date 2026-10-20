//! Bind ordered case predicates to the same scalar grain as their derivation.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{Intermediate, SecondarySource},
};

fn unknown(path: &str, name: &str) -> BindFinding {
    let mut finding = LookupFinding::new(
        path,
        crate::diagnostic::ConditionCode::PredicateUnknownField,
    );
    finding.text("identifier", name);
    BindFinding::Predicate(finding.diagnostic())
}

/// Operand binding is shared with expressions; only predicate-name failures
/// cite REQ-0189 here. Phase, type and relation failures keep their own owner.
pub(super) fn retain_reference_owner(
    findings: &mut [BindFinding],
    catalog: &Catalog,
) -> Result<(), BindError> {
    for finding in findings {
        let replacement = match finding {
            BindFinding::UnknownReference { path, name }
            | BindFinding::OutputReference {
                path,
                name,
                finding: reference_binding::Diagnostic::UnknownField,
                ..
            }
            | BindFinding::QualifiedReference {
                path,
                name,
                finding: crate::reference_scope::Finding::UnknownField,
                ..
            } => Some(unknown(path, name)),
            BindFinding::OutputReference {
                path,
                name,
                finding: reference_binding::Diagnostic::UnresolvableName { dataset },
                ..
            } => {
                let owner = catalog.dataset_name(*dataset).ok_or(BindError::Internal)?;
                let mut diagnostic = LookupFinding::new(
                    path,
                    crate::diagnostic::ConditionCode::PredicateUnresolvableName,
                );
                diagnostic.text("identifier", name);
                diagnostic.text("suggestion", &format!("{owner}.{name}"));
                Some(BindFinding::Predicate(diagnostic.diagnostic()))
            }
            _ => None,
        };
        if let Some(replacement) = replacement {
            *finding = replacement;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    spec: &PreparedSpecification,
    plan: &CompiledExpression,
    column: usize,
    catalog: &Catalog,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<BoundPredicate>, BindError> {
    let start = findings.len();
    let mut bindings = Vec::new();
    let mut collected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for name in plan.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let reference_start = findings.len();
        let read = if let Some((index, field)) = spec.intermediates.reference(name) {
            if let Some(item) = &intermediates[index] {
                edges.extend(item.output_dependencies());
                if let Some(column) = item.source_column(secondary, field) {
                    Some(Read::Intermediate { index, column })
                } else {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        plan.spec_path(),
                        name,
                        None,
                        ReferenceCause::Value,
                        None,
                    )));
                    continue;
                }
            } else {
                None
            }
        } else if name
            .split_once('.')
            .is_some_and(|(dataset, _)| catalog.joined(dataset).is_some())
        {
            source_expressions::bind_catalog_join(
                name,
                plan.spec_path(),
                catalog,
                None,
                None,
                edges,
                findings,
            )?
            .and_then(|expression| match expression {
                Expression::Joined(read) => Some(Read::Joined(read)),
                _ => None,
            })
        } else {
            match catalog.bind(name) {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    edges.push(column);
                    Some(Read::Column(column))
                }
                Some(reference_binding::Binding::Dataset { field, .. }) => {
                    if !spec.keys.contains(&column) {
                        collected.push(name.into());
                    }
                    Some(Read::Source(field))
                }
                None => None,
            }
        };
        retain_reference_owner(&mut findings[reference_start..], catalog)?;
        if let Some(read) = read {
            bindings.push(Binding {
                name: name.into(),
                read,
            });
        } else {
            if !name.contains('.') {
                if let Some(owner) = catalog.unique_field_owner(name) {
                    let mut finding = LookupFinding::new(
                        plan.spec_path(),
                        crate::diagnostic::ConditionCode::PredicateUnresolvableName,
                    );
                    finding.text("identifier", name);
                    finding.text("suggestion", &format!("{owner}.{name}"));
                    findings.push(BindFinding::Predicate(finding.diagnostic()));
                    continue;
                }
            }
            findings.push(unknown(plan.spec_path(), name));
        }
    }
    let captured =
        types::CapturedTypes::new(&spec.intermediates, catalog, secondary, intermediates);
    captured.predicate(plan, findings);
    if findings.len() != start {
        return Ok(None);
    }
    BoundPredicate::new_contextual(captured.typed(plan), bindings, collected)
        .map(Some)
        .map_err(BindError::InvalidPredicateBinding)
}
