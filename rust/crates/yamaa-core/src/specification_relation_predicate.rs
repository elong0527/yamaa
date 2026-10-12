//! Shared donor/current filter binding and authored read-context admission.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{CurrentField, CurrentRead, Intermediate, RelationPredicate},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    plan: &crate::predicate::Plan,
    donor_name: &str,
    donor: &TableSchema,
    self_rows: bool,
    admits: &impl Fn(usize) -> bool,
    intermediate: &str,
    sources: &[SourceDeclaration],
    schemas: &[&TableSchema],
    driver: usize,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationPredicate>, BindError> {
    let before = findings.len();
    let mut bindings = Vec::new();
    let mut current = Vec::new();
    let mut seen = alloc::collections::BTreeSet::new();
    for name in plan.identifiers() {
        if !seen.insert(name) {
            continue;
        }
        let own = name
            .strip_prefix(&format!("{donor_name}."))
            .or_else(|| (self_rows && !name.contains('.')).then_some(name));
        let donor_column = own
            .and_then(|field| {
                donor
                    .columns()
                    .iter()
                    .position(|column| column.name == field)
            })
            .filter(|&column| admits(column));
        let read = if let Some(column) = donor_column {
            Some(Read::Source(column))
        } else if own.is_none() {
            name.split_once('.')
                .filter(|(dataset, _)| *dataset != donor_name)
                .and_then(|(dataset, field)| {
                    sources
                        .iter()
                        .position(|source| source.name == dataset)
                        .and_then(|index| {
                            schemas[index]
                                .columns()
                                .iter()
                                .position(|column| column.name == field)
                                .map(|column| (index, column))
                        })
                })
                .map(|(index, column)| {
                    let slot = current.len();
                    current.push(CurrentField {
                        identifier: name.into(),
                        read: CurrentRead::Record {
                            source: if index == driver {
                                0
                            } else if index < driver {
                                index + 1
                            } else {
                                index
                            },
                            column,
                        },
                    });
                    Read::Column(slot)
                })
        } else {
            None
        };
        if let Some(read) = read {
            bindings.push(Binding {
                name: name.into(),
                read,
            });
        } else {
            let suggestion = (!name.contains('.')
                && donor.columns().iter().any(|column| column.name == name))
            .then(|| format!("{donor_name}.{name}"));
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                plan.spec_path(),
                name,
                Some(intermediate),
                ReferenceCause::Selection,
                suggestion,
            )));
        }
    }
    if findings.len() != before {
        return Ok(None);
    }
    Ok(Some(
        RelationPredicate::new(
            BoundPredicate::new(plan.clone(), bindings)
                .map_err(BindError::InvalidPredicateBinding)?,
            current,
        )
        .map_err(BindError::InvalidPredicateBinding)?,
    ))
}

/// A globally declared filter is checked against each context that actually reads it.
pub(super) fn admit_reads<'a>(
    expressions: impl IntoIterator<Item = &'a Expression>,
    items: &[Option<Intermediate>],
    source: usize,
    mode: &RowMode,
    findings: &mut Vec<BindFinding>,
) {
    let mut seen = alloc::collections::BTreeSet::new();
    for expression in expressions {
        crate::dataset::relation_predicates::visit(expression, &mut |index| {
            if !seen.insert(index) {
                return;
            }
            let Some(item) = items.get(index).and_then(Option::as_ref) else {
                return;
            };
            let Some(filter) = &item.filter else {
                return;
            };
            for field in filter.current() {
                if let CurrentRead::Record {
                    source: driver,
                    column,
                } = field.read
                {
                    if driver != source
                        || matches!(mode.local_mode(), RowMode::Groups(keys) if !keys.contains(&column))
                    {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            filter.plan().spec_path(),
                            &field.identifier,
                            Some(&item.identifier),
                            ReferenceCause::Selection,
                            None,
                        )));
                    }
                }
            }
        });
    }
}
