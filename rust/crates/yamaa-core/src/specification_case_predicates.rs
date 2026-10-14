//! Bind ordered case predicates to the same scalar grain as their derivation.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{Intermediate, SecondarySource},
    predicate::Plan,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    spec: &PreparedSpecification,
    plan: &Plan,
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
    let mut seen = alloc::collections::BTreeSet::new();
    for name in plan.identifiers() {
        if !seen.insert(name) {
            continue;
        }
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
            match catalog.bind(name).map_err(BindError::Catalog)? {
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
        if let Some(read) = read {
            bindings.push(Binding {
                name: name.into(),
                read,
            });
        } else {
            findings.push(BindFinding::UnknownReference {
                path: plan.spec_path().into(),
                name: name.into(),
            });
        }
    }
    if findings.len() != start {
        return Ok(None);
    }
    BoundPredicate::new_contextual(plan.clone(), bindings, collected)
        .map(Some)
        .map_err(BindError::InvalidPredicateBinding)
}
