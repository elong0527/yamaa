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
    static_filter_types: bool,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationPredicate>, BindError> {
    let before = findings.len();
    let mut bindings = Vec::new();
    let mut current = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
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
            let bare = name.split_once('.').map_or(name, |(_, field)| field);
            let suggestion = donor
                .columns()
                .iter()
                .enumerate()
                .any(|(column, metadata)| metadata.name == bare && admits(column))
                .then(|| format!("{donor_name}.{bare}"));
            findings.push(BindFinding::Lookup(LookupFinding::reference(
                plan.spec_path(),
                name,
                Some(intermediate),
                ReferenceCause::Selection,
                suggestion,
            )));
        }
    }
    // A type refusal still retains the donor's bound fields for later checks.
    // Only unresolved references prevent constructing this partial plan.
    let references_resolved = findings.len() == before;
    let type_findings = static_types::predicate_kinds(plan, |name| {
        let binding = bindings.iter().find(|binding| binding.name == name)?;
        let (schema, column) = match binding.read {
            Read::Source(column) => (donor, column),
            Read::Column(slot) => {
                let CurrentRead::Record { source, column } = current.get(slot)?.read else {
                    return None;
                };
                let index = if source == 0 {
                    driver
                } else if source <= driver {
                    source - 1
                } else {
                    source
                };
                (*schemas.get(index)?, column)
            }
            _ => return None,
        };
        schema
            .columns()
            .get(column)
            .map(|column| column.kind.into())
    })?;
    if static_filter_types {
        findings.extend(type_findings.iter().cloned().map(BindFinding::Predicate));
    }
    if !references_resolved {
        return Ok(None);
    }
    Ok(Some(
        RelationPredicate::new(
            BoundPredicate::new(plan.clone(), bindings)
                .map_err(BindError::InvalidPredicateBinding)?,
            current,
        )
        .map_err(BindError::InvalidPredicateBinding)?
        .with_deferred_type_findings(if static_filter_types {
            Vec::new()
        } else {
            type_findings
        }),
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
    let mut seen = std::collections::BTreeSet::new();
    for expression in expressions {
        crate::dataset::relation_predicates::visit(expression, &mut |index| {
            if !seen.insert(index) {
                return;
            }
            let Some(item) = items.get(index).and_then(Option::as_ref) else {
                return;
            };
            for key in &item.keys {
                if let crate::dataset::MatchValue::Contextual(declaration) = &key.value {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        &key.path,
                        declaration.identifier(),
                        Some(&item.identifier),
                        ReferenceCause::OutputKey,
                        None,
                    )));
                }
                for read in key.value.reads() {
                    if let crate::dataset::MatchRead::SecondaryRecord {
                        source: owner,
                        identifier,
                        ..
                    } = read
                    {
                        if owner + 1 != source {
                            findings.push(BindFinding::Lookup(LookupFinding::reference(
                                &key.path,
                                identifier,
                                Some(&item.identifier),
                                ReferenceCause::OutputKey,
                                None,
                            )));
                        }
                    }
                }
            }
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
