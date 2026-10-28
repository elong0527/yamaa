//! Shared donor/current filter binding and authored read-context admission.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{CurrentField, CurrentRead, Intermediate, RelationPredicate},
};

/// Bind a lookup's filter over the relation it reads: a field of the
/// relation, or a field of another input's current record; with the
/// predicate's operand types when `static_filter_types`.
pub(super) fn bind(
    plan: &crate::expression::CompiledExpression,
    relation: &intermediates::Donor<'_>,
    self_rows: bool,
    inputs: binder::Inputs<'_>,
    static_filter_types: bool,
    contextual: bool,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<RelationPredicate>, BindError> {
    let intermediates::Donor {
        lookup: intermediate,
        dataset: donor_name,
        schema: donor,
        kind: donor_kind,
        admits,
    } = *relation;
    let binder::Inputs {
        sources,
        schemas,
        driver,
    } = inputs;
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
                            schemas[index].position(field).map(|column| (index, column))
                        })
                })
                .map(|(index, column)| CurrentRead::Record {
                    source: if index == driver {
                        0
                    } else if index < driver {
                        index + 1
                    } else {
                        index
                    },
                    column,
                })
                .or_else(|| contextual.then_some(CurrentRead::Contextual))
                .map(|read| {
                    let slot = current.len();
                    current.push(CurrentField {
                        identifier: name.into(),
                        read,
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
            // The suggestion names a field of the donor, so it is withheld
            // unless the donor's schema decides whether it stores it, or the
            // lookup derives it.
            let derived = suggestion.is_some()
                && inputs
                    .schema(donor_name)
                    .is_some_and(|schema| schema.position(bare).is_none());
            let finding = BindFinding::Lookup(LookupFinding::reference(
                plan.spec_path(),
                name,
                Some(intermediate),
                ReferenceCause::Selection,
                suggestion,
            ));
            findings.push(if derived || inputs.decides(donor_name, bare) {
                finding
            } else {
                BindFinding::Withheld(std::boxed::Box::new(finding))
            });
        }
    }
    // A type refusal still retains the donor's bound fields for later checks.
    // Only unresolved references prevent constructing this partial plan.
    let references_resolved = findings.len() == before;
    let kind = |name: &str| -> Option<crate::value::ValueType> {
        let binding = bindings.iter().find(|binding| binding.name == name)?;
        match binding.read {
            Read::Source(column) => donor_kind(column).map(Into::into),
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
                schemas.get(index)?.kind(column).map(Into::into)
            }
            _ => None,
        }
    };
    let type_findings = types::predicate_kinds(plan, kind);
    if static_filter_types {
        findings.extend(type_findings.iter().cloned().map(BindFinding::Predicate));
    }
    if !references_resolved {
        return Ok(None);
    }
    let typed = plan.typed(|name| crate::result_type::ResultType::known(kind(name)));
    Ok(Some(
        RelationPredicate::new(
            BoundPredicate::new(typed, bindings).map_err(BindError::InvalidPredicateBinding)?,
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

/// The named intermediates that predicates read, as reads [`admit_reads`] and
/// [`lookup_range::admit`] visit. A row filter reads them for its own row
/// template, and a dataset verification for every completed row, as a column
/// derivation does.
pub(super) fn predicate_reads<'a>(
    predicates: impl IntoIterator<Item = &'a BoundPredicate>,
) -> Vec<Expression> {
    predicates
        .into_iter()
        .flat_map(BoundPredicate::bindings)
        .filter_map(|binding| match binding.read {
            Read::Intermediate { index, column } => {
                Some(Expression::Intermediate { index, column })
            }
            _ => None,
        })
        .collect()
}

/// The predicates a bound dataset verification evaluates over completed rows.
pub(super) fn verification_predicates(
    verifications: &[crate::dataset::Verification],
) -> impl Iterator<Item = &BoundPredicate> {
    use crate::dataset::Check;
    verifications
        .iter()
        .flat_map(|verification| match &verification.check {
            Check::Assert { when, require } => vec![when.as_ref(), Some(require)],
            Check::ScopedRowCount(check) => vec![check.filter.as_ref(), check.when.as_ref()],
            Check::PredicateDeclaration(predicate) => vec![Some(predicate)],
            _ => Vec::new(),
        })
        .flatten()
}

/// Two read-context findings are one when they report the same diagnostic.
pub(super) fn same_finding(left: &BindFinding, right: &BindFinding) -> bool {
    matches!((left, right), (BindFinding::Lookup(left), BindFinding::Lookup(right))
        if left.diagnostic() == right.diagnostic())
}

/// The row that reads a lookup: the reading row template's id and driver.
/// A column derivation or dataset verification without row templates has none.
pub(super) type ReadingRow<'a> = Option<(&'a str, &'a str)>;

/// A globally declared lookup is checked against each context that actually
/// reads it: its key match values, its correlated filter fields and its
/// derivations' current-record fields read the reading row's own driver
/// record, and in grouped construction only its group keys. A chained
/// `between` value runs the selector it names for the same row; the range
/// values themselves are admitted with every other lookup's range
/// ([`lookup_range::admit`]), and only their group keys are checked here.
///
/// `source` is the reading row's driver port: 0 for the specification's
/// driver, or one past a secondary input's position.
pub(super) fn admit_reads<'a>(
    expressions: impl IntoIterator<Item = &'a Expression>,
    items: &[Option<Intermediate>],
    named: &intermediates::Declarations,
    source: usize,
    mode: &RowMode,
    row: ReadingRow<'_>,
    findings: &mut Vec<BindFinding>,
) {
    // In grouped construction a driver field is one value of the row only
    // when it is a group key; any other field varies within the group and
    // names no single value (LOOKUP-2, LOOKUP-8, ROW-5).
    let ungrouped = |column: usize| matches!(mode.local_mode(), RowMode::Groups(keys) if !keys.contains(&column));
    let grouped = |path: &str, identifier: &str, intermediate: &str| {
        let mut finding =
            LookupFinding::new(path, crate::diagnostic::ConditionCode::GroupedRowReference);
        finding.text("identifier", identifier);
        // The reading row template and its driver, null without one.
        match row {
            Some((row, dataset)) => {
                finding.text("row", row);
                finding.text("dataset", dataset);
            }
            None => {
                finding.null("row");
                finding.null("dataset");
            }
        }
        finding.text("intermediate", intermediate);
        BindFinding::Lookup(finding)
    };
    let mut seen = std::collections::BTreeSet::new();
    for expression in expressions {
        crate::dataset::relation_predicates::visit(expression, &mut |mut index| loop {
            if !seen.insert(index) {
                return;
            }
            let Some(item) = items.get(index).and_then(Option::as_ref) else {
                return;
            };
            // The reference of a nearest selection is read as a key's match
            // value is (LOOKUP-26).
            let reference = item.nearest.iter().map(|nearest| &nearest.reference);
            for key in item.keys.iter().chain(reference) {
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
                    // A record match value reads the reading row's own driver
                    // record (LOOKUP-5).
                    let (own, column) = match read {
                        crate::dataset::MatchRead::SecondaryRecord {
                            source: owner,
                            column,
                            ..
                        } => (owner + 1 == source, *column),
                        crate::dataset::MatchRead::Record { column, .. } => (source == 0, *column),
                        crate::dataset::MatchRead::Output { .. } => continue,
                    };
                    if !own {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            &key.path,
                            read.identifier(),
                            Some(&item.identifier),
                            ReferenceCause::OutputKey,
                            None,
                        )));
                    } else if ungrouped(column) {
                        findings.push(grouped(&key.path, read.identifier(), &item.identifier));
                    }
                }
            }
            if let Some(filter) = &item.filter {
                for field in filter.current() {
                    if field.read == CurrentRead::Contextual {
                        findings.push(BindFinding::Lookup(LookupFinding::reference(
                            filter.plan().spec_path(),
                            &field.identifier,
                            Some(&item.identifier),
                            ReferenceCause::Selection,
                            None,
                        )));
                    }
                    if let CurrentRead::Record {
                        source: driver,
                        column,
                    } = field.read
                    {
                        if driver != source || ungrouped(column) {
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
            }
            // A derivation reads the current record in the scope of a
            // correlated filter (LOOKUP-15).
            for field in item.derivations.iter().flat_map(|derived| &derived.current) {
                if let CurrentRead::Record {
                    source: driver,
                    column,
                } = field.read
                {
                    let path = named
                        .derivation_path(index, &field.identifier)
                        .unwrap_or(&item.path);
                    if driver != source {
                        let mut finding = LookupFinding::new(
                            path,
                            crate::diagnostic::ConditionCode::IntermediateDerivationReference,
                        );
                        finding.text("intermediate", &item.identifier);
                        finding.text("identifier", &field.identifier);
                        findings.push(BindFinding::Lookup(finding));
                    } else if ungrouped(column) {
                        findings.push(grouped(path, &field.identifier, &item.identifier));
                    }
                }
            }
            let Some(range) = &item.range else {
                return;
            };
            if let CurrentRead::Record {
                source: driver,
                column,
            } = range.current().read
            {
                if driver == source && ungrouped(column) {
                    findings.push(grouped(
                        &format!("{}.value", range.path()),
                        &range.current().identifier,
                        &item.identifier,
                    ));
                }
            }
            let CurrentRead::Intermediate { index: chained, .. } = range.current().read else {
                return;
            };
            index = chained;
        });
    }
}
