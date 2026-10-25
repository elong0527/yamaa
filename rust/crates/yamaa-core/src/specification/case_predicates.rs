//! Bind ordered case predicates to the same scalar grain as their derivation.
use super::*;
use crate::bound_expression::BoundPredicate;

fn unknown(path: &str, name: &str) -> BindFinding {
    let mut finding = LookupFinding::new(
        path,
        crate::diagnostic::ConditionCode::PredicateUnknownField,
    );
    finding.text("identifier", name);
    BindFinding::Predicate(finding.diagnostic())
}

/// Operand binding is shared with expressions; only predicate-name failures
/// cite FORMULA-9 here. Phase, type and relation failures keep their own owner.
pub(super) fn retain_reference_owner(
    findings: &mut [BindFinding],
    catalog: &Catalog,
) -> Result<(), BindError> {
    for finding in findings {
        // A withheld finding keeps its cause, so it stays withheld.
        let finding = match finding {
            BindFinding::Withheld(inner) => &mut **inner,
            finding => finding,
        };
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

/// A case branch's predicate, each name bound once in this scope. `case`
/// is where the enclosing `case` started recording findings: a row
/// template's predicate is built only while the whole `case` has none.
pub(super) fn bind(
    cx: &mut BindContext<'_, '_>,
    plan: &CompiledExpression,
    case: usize,
) -> Result<Option<BoundPredicate>, BindError> {
    if let Some(implicit) = cx.implicit() {
        return implicit_predicate(cx, implicit, plan);
    }
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for identifier in plan.identifiers() {
        if !seen.insert(identifier) {
            continue;
        }
        if let crate::reference_scope::Phase::Row { group_by } = cx.scope.phase {
            if identifier
                .split_once('.')
                .is_some_and(|(dataset, _)| cx.catalog.joined(dataset).is_some())
            {
                let mut finding = LookupFinding::new(
                    plan.spec_path(),
                    if group_by.is_some() {
                        crate::diagnostic::ConditionCode::GroupedRowNumericSourceScope
                    } else {
                        crate::diagnostic::ConditionCode::RowNumericSourceScope
                    },
                );
                finding.text("identifier", identifier);
                finding.texts("drivers", &[cx.scope.driver]);
                cx.findings.push(BindFinding::Lookup(finding));
                continue;
            }
        }
        let reference_start = cx.findings.len();
        let expression = cx.operand(identifier, plan.spec_path())?;
        retain_reference_owner(&mut cx.findings[reference_start..], cx.catalog)?;
        if let Some(expression) = expression {
            let read = match expression {
                Expression::Source(column) => Read::Source(column),
                Expression::Column(column) => Read::Column(column),
                Expression::Intermediate { index, column } => Read::Intermediate { index, column },
                Expression::Joined(read) => Read::Joined(read),
                _ => return Err(BindError::Internal),
            };
            bindings.push(Binding {
                name: identifier.into(),
                read,
            });
        }
    }
    if cx.findings.len() == case {
        Ok(Some(
            BoundPredicate::new_contextual(cx.captured().typed(plan), bindings, Vec::new())
                .map_err(BindError::InvalidPredicateBinding)?,
        ))
    } else {
        Ok(None)
    }
}

/// The implicit template's predicate: a lookup read takes its selector's
/// record; a driver field of a non-key column is collected over the row's
/// records; an unknown bare name a single input stores suggests that
/// input's field; and every static type fault is reported (FORMULA-5,
/// FORMULA-12).
fn implicit_predicate(
    cx: &mut BindContext<'_, '_>,
    implicit: binder::Implicit,
    plan: &CompiledExpression,
) -> Result<Option<BoundPredicate>, BindError> {
    let catalog = cx.catalog;
    let binder::Donors {
        named,
        secondary,
        items,
        ..
    } = cx.donors;
    let start = cx.findings.len();
    let mut bindings = Vec::new();
    let mut collected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for name in plan.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let reference_start = cx.findings.len();
        let read = if let Some((index, field)) = named.reference(name) {
            if let Some(item) = &items[index] {
                cx.edges.extend(item.output_dependencies());
                if let Some(column) = item.source_column(secondary, field) {
                    Some(Read::Intermediate { index, column })
                } else {
                    cx.findings
                        .push(BindFinding::Lookup(LookupFinding::reference(
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
                cx.edges,
                cx.findings,
            )?
            .and_then(|expression| match expression {
                Expression::Joined(read) => Some(Read::Joined(read)),
                _ => None,
            })
        } else {
            match catalog.bind(name) {
                Some(reference_binding::Binding::Output { column, .. }) => {
                    cx.edges.push(column);
                    Some(Read::Column(column))
                }
                Some(reference_binding::Binding::Dataset { field, .. }) => {
                    if !implicit.key {
                        collected.push(name.into());
                    }
                    Some(Read::Source(field))
                }
                None => None,
            }
        };
        retain_reference_owner(&mut cx.findings[reference_start..], catalog)?;
        if let Some(read) = read {
            bindings.push(Binding {
                name: name.into(),
                read,
            });
        } else {
            // Which input stores a bare name decides its cause, so the cause is
            // withheld unless every relation's schema decides it.
            let decided = name.contains('.')
                || catalog
                    .relation_names()
                    .all(|relation| cx.donors.decides(relation, name));
            let finding = match catalog
                .unique_field_owner(name)
                .filter(|_| !name.contains('.'))
            {
                Some(owner) => {
                    let mut finding = LookupFinding::new(
                        plan.spec_path(),
                        crate::diagnostic::ConditionCode::PredicateUnresolvableName,
                    );
                    finding.text("identifier", name);
                    finding.text("suggestion", &format!("{owner}.{name}"));
                    BindFinding::Predicate(finding.diagnostic())
                }
                None => unknown(plan.spec_path(), name),
            };
            cx.findings.push(if decided {
                finding
            } else {
                BindFinding::Withheld(std::boxed::Box::new(finding))
            });
        }
    }
    let captured = cx.captured();
    captured.predicate(plan, cx.findings);
    if cx.findings.len() != start {
        return Ok(None);
    }
    BoundPredicate::new_contextual(captured.typed(plan), bindings, collected)
        .map(Some)
        .map_err(BindError::InvalidPredicateBinding)
}
