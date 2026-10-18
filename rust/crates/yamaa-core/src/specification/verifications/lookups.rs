//! Declared completed-row lookup names acquire coordinates only from captured schemas.
use super::*;
use crate::dataset::{Intermediate, SecondarySource};

pub(super) fn declared(d: &Document, name: &str) -> Option<usize> {
    let (relation, _) = name.split_once('.')?;
    let N::Sequence(entries) = &d.nodes()[d.field(d.root(), "intermediates")?] else {
        return None;
    };
    entries.iter().position(|&entry| matches!(d.field(entry, "id").map(|id| &d.nodes()[id]), Some(N::Text(id)) if id == relation))
}
fn unknown(predicate: &BoundPredicate, name: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::VerificationUnknownReference,
        spec_paths: vec![predicate.plan().spec_path().into()],
        context: [(
            "identifier".into(),
            JsonValue::from(Value::Str(name.into())),
        )]
        .into(),
        source_span: None,
        operand_route: None,
    }
}
fn bind_predicate(
    predicate: &BoundPredicate,
    spec: &PreparedSpecification,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Result<BoundPredicate, Diagnostic>, BindError> {
    let mut bindings = Vec::new();
    let mut kinds = Vec::new();
    for binding in predicate.bindings() {
        let read = if matches!(binding.read, Read::Intermediate { .. }) {
            let (index, field) = spec
                .intermediates
                .reference(&binding.name)
                .ok_or(BindError::Internal)?;
            let Some((item, column)) = intermediates[index].as_ref().and_then(|item| {
                item.source_column(secondary, field)
                    .map(|column| (item, column))
            }) else {
                return Ok(Err(unknown(predicate, &binding.name)));
            };
            kinds.push((binding.name.as_str(), item.source_kind(secondary, column)));
            Read::Intermediate { index, column }
        } else {
            binding.read.clone()
        };
        bindings.push(Binding {
            name: binding.name.clone(),
            read,
        });
    }
    let typed = predicate.plan().typed(|name| {
        kinds.iter().find(|(bound, _)| *bound == name).map_or_else(
            || spec.declared_input(name),
            |&(_, kind)| crate::result_type::ResultType::declared(kind),
        )
    });
    BoundPredicate::new_contextual(typed, bindings, Vec::new())
        .map(Ok)
        .map_err(BindError::InvalidPredicateBinding)
}
pub(super) fn bind(
    checks: &[Verification],
    spec: &PreparedSpecification,
    secondary: &[SecondarySource],
    intermediates: &[Option<Intermediate>],
) -> Result<Vec<Verification>, BindError> {
    let mut result = Vec::new();
    for verification in checks {
        let mut bound = verification.clone();
        let mut failure = None;
        match &verification.check {
            Check::Assert { when, require } => {
                let when = match when
                    .as_ref()
                    .map(|p| bind_predicate(p, spec, secondary, intermediates))
                    .transpose()?
                {
                    Some(Ok(p)) => Some(p),
                    Some(Err(d)) => {
                        failure = Some(d);
                        None
                    }
                    None => None,
                };
                if failure.is_none() {
                    match bind_predicate(require, spec, secondary, intermediates)? {
                        Ok(require) => bound.check = Check::Assert { when, require },
                        Err(d) => {
                            if let Some(when) = when {
                                result.push(Verification {
                                    path: when.plan().spec_path().into(),
                                    check: Check::PredicateDeclaration(when),
                                });
                            }
                            failure = Some(d);
                        }
                    }
                }
            }
            Check::ScopedRowCount(check) => {
                let mut check = check.clone();
                let mut filter_bound = false;
                for (position, predicate) in
                    [&mut check.filter, &mut check.when].into_iter().enumerate()
                {
                    if let Some(original) = predicate {
                        match bind_predicate(original, spec, secondary, intermediates)? {
                            Ok(bound) => {
                                *original = bound;
                                filter_bound |= position == 0;
                            }
                            Err(d) => {
                                failure = Some(d);
                                break;
                            }
                        }
                    }
                }
                if failure.is_some() {
                    if let Some(filter) = check.filter.filter(|_| filter_bound) {
                        result.push(Verification {
                            path: filter.plan().spec_path().into(),
                            check: Check::PredicateDeclaration(filter),
                        });
                    }
                } else {
                    bound.check = Check::ScopedRowCount(check);
                }
            }
            Check::PredicateDeclaration(p) => {
                match bind_predicate(p, spec, secondary, intermediates)? {
                    Ok(p) => bound.check = Check::PredicateDeclaration(p),
                    Err(d) => failure = Some(d),
                }
            }
            _ => {}
        }
        if let Some(diagnostic) = failure {
            result.push(Verification {
                path: verification.path.clone(),
                check: Check::InvalidDiagnostic(diagnostic),
            });
            continue;
        }
        result.push(bound);
    }
    Ok(result)
}
