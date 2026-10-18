//! Bound many-to-one secondary-source reads over immutable normalized snapshots.
use super::*;

/// Count all matching records before reading a value or applying output conversion.
pub(super) fn read<E>(
    table: &dyn TableAccess<Error = E>,
    lookup: &Lookup,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
) -> Result<Value, Box<ExecutionError<E>>> {
    let keys = lookup
        .keys
        .iter()
        .map(|key| {
            (
                key.source_column,
                ValueRef::from(&candidate.values[key.output_column]),
            )
        })
        .collect::<Vec<_>>();
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        &Context {
            assignment,
            candidate,
            plan,
            row,
        },
        Options::uncounted(&assignment.path),
        None,
    )
}

/// Where a read through an implicit join is made, and so what answers a row
/// the join finds no record for (LOOKUP-24).
#[derive(Clone, Copy)]
pub(super) enum JoinSite<'a> {
    /// A source binding or a bound operand: its own answer, counted at its
    /// path unless it fails.
    Read(&'a crate::dataset::JoinAbsence),
    /// A formula operand read at this path: missing, counted under `absent`.
    Operand(&'a str),
    /// A read made while a predicate is evaluated, or a plan-level lookup:
    /// missing, uncounted; the path locates an `unmatched_key` failure.
    Uncounted(&'a str),
}

/// An implicit join's range and the current row's value for it (LOOKUP-9).
pub(super) type BoundRange<'a> = (&'a RelationRange, Value);

/// A source binding's options on its implicit join; an operand that is not a
/// source binding has only its site.
struct Options<'a> {
    range: Option<BoundRange<'a>>,
    filter: Option<&'a crate::bound_expression::BoundPredicate>,
    selection: Option<&'a SourceSelection>,
    site: JoinSite<'a>,
    donor_name: Option<&'a str>,
}
impl<'a> Options<'a> {
    fn uncounted(path: &'a str) -> Self {
        Self {
            range: None,
            filter: None,
            selection: None,
            site: JoinSite::Uncounted(path),
            donor_name: None,
        }
    }
}

/// The current row's value for an implicit join's range, read before any
/// record is matched (LOOKUP-9).
#[allow(clippy::too_many_arguments)]
pub(super) fn range_value<'a, T: TableAccess + ?Sized>(
    range: Option<&'a RelationRange>,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    assignment: &Assignment,
    state: &mut EvaluationState<'_, T::Error>,
) -> Result<Option<BoundRange<'a>>, Box<ExecutionError<T::Error>>> {
    range
        .map(|range| {
            relation_predicates::resolve_current(
                range.current(),
                range.path(),
                table,
                candidate,
                plan,
                row,
                assignment,
                relation_predicates::CurrentObservation::Operand,
                state,
            )
            .map(|value| (range, value))
        })
        .transpose()
}

/// Keep assignment provenance and optional completed output identity independent of match keys.
pub(super) struct Context<'a> {
    pub assignment: &'a Assignment,
    pub candidate: &'a Candidate,
    pub plan: &'a DatasetPlan,
    pub row: usize,
}

/// Resolve every raw driver match field before the bounded secondary scan.
pub(super) fn read_row<T: TableAccess + ?Sized>(
    driver: &T,
    table: &dyn TableAccess<Error = T::Error>,
    lookup: &RowLookup,
    context: &Context<'_>,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let source_row = context.candidate.members[0];
    let mut keys = Vec::with_capacity(lookup.keys.len());
    for key in &lookup.keys {
        let value = driver
            .cell(source_row, key.driver_column)
            .map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: context.assignment.path.clone(),
                    source_row,
                    error,
                })
            })?;
        keys.push((key.source_column, value));
    }
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        context,
        Options::uncounted(&context.assignment.path),
        None,
    )
}

/// Answer a row whose match finds no record or reads a missing match value
/// (LOOKUP-24): a source binding's declared `absent` value, or missing by
/// default, counted at the read's path except during a predicate; or
/// `unmatched_key` when the read, or its input, requires a match and the read
/// declares no `absent`.
fn absent_record<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    keys: &[(usize, ValueRef<'_>)],
    context: &Context<'_>,
    options: &Options<'_>,
    handlers: Option<&mut HandlerCounter>,
) -> Result<Value, Box<ExecutionError<E>>> {
    let dataset = options
        .donor_name
        .unwrap_or_else(|| context.plan.secondary()[source].name.as_str());
    let input = context.plan.requires_match(dataset);
    let (path, answer, required, counted) = match options.site {
        JoinSite::Read(absence) => (
            absence.path.as_str(),
            absence.absent.as_ref(),
            absence.requires_match(input),
            true,
        ),
        JoinSite::Operand(path) => (path, None, input, true),
        JoinSite::Uncounted(path) => (path, None, input, false),
    };
    if required {
        return Err(Box::new(ExecutionError::UnmatchedKey {
            path: path.into(),
            intermediate: std::format!("intermediate({dataset})"),
            dataset: dataset.into(),
            matched_key: keys
                .iter()
                .map(|(column, value)| {
                    (table.schema().columns()[*column].name.clone(), own(*value))
                })
                .collect(),
            identity: failure_identity(context.candidate, context.plan.keys(), context.row)?,
        }));
    }
    if let (true, Some(counter)) = (counted, handlers) {
        counter
            .record(&std::format!("{path}.absent"), HandlerKind::Absent)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
    }
    Ok(answer.cloned().unwrap_or(Value::Missing))
}

/// The read's path, owning its key-type and `unmatched_key` failures.
fn site_path<'a>(site: &JoinSite<'a>) -> &'a str {
    match *site {
        JoinSite::Read(absence) => &absence.path,
        JoinSite::Operand(path) | JoinSite::Uncounted(path) => path,
    }
}

/// Match, narrow and select on the implicit join (LOOKUP-25), preserving raw
/// match-key evidence.
fn read_matching<E>(
    table: &dyn TableAccess<Error = E>,
    source: usize,
    column: usize,
    keys: &[(usize, ValueRef<'_>)],
    context: &Context<'_>,
    options: Options<'_>,
    mut handlers: Option<&mut HandlerCounter>,
) -> Result<Value, Box<ExecutionError<E>>> {
    let Options {
        ref range,
        filter,
        selection,
        donor_name,
        ..
    } = options;
    let kinds = keys
        .iter()
        .map(|&(column, _)| {
            table
                .schema()
                .columns()
                .get(column)
                .map(|field| field.kind.into())
        })
        .collect::<Vec<_>>();
    let probe = matching::Probe {
        keys,
        kinds: &kinds,
        range: range
            .as_ref()
            .map(|(range, value)| (*range, ValueRef::from(value))),
    };
    if probe.incomplete() {
        return absent_record(table, source, keys, context, &options, handlers);
    }
    let found = matching::candidates(
        0..table.row_count(),
        0,
        &probe,
        |row, field| cell(table, source, context.assignment, row, field),
        |row| match filter {
            None => Ok(true),
            Some(filter) => {
                crate::dataset::execution::predicates::evaluate(filter, table, row, &[])
                    .map(|truth| truth == crate::expression::Truth::True)
                    .map_err(|error| {
                        Box::new(ExecutionError::Predicate {
                            source_row: row,
                            error: *error,
                        })
                    })
            }
        },
    )
    .map_err(|refusal| match refusal {
        matching::Refusal::Failed(error) => error,
        matching::Refusal::Key {
            index,
            current,
            donor,
        } => Box::new(ExecutionError::ScalarDiagnostic(
            crate::diagnostic::lookup::key_type(
                site_path(&options.site),
                crate::diagnostic::lookup::Owner::Unnamed,
                &table.schema().columns()[keys[index].0].name,
                crate::result_type::ResultType::known(current),
                crate::result_type::ResultType::known(donor),
            ),
        )),
        matching::Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(
            types.diagnostic_for(range.as_ref().map_or("", |(range, _)| range.path()), None),
        )),
    })?;
    let count = found.rows.len();
    let mut chosen = found.rows.first().copied().unwrap_or_default();
    if count == 0 {
        return absent_record(table, source, keys, context, &options, handlers);
    }
    if count > 1 {
        let Some(selection) = selection else {
            // Admit diagnostic copies before owning any raw driver text.

            return Err(Box::new(ExecutionError::MultipleMatches {
                path: context.assignment.path.clone(),
                dataset: donor_name
                    .map(String::from)
                    .unwrap_or_else(|| context.plan.secondary()[source].name.clone()),
                intermediate: std::format!(
                    "intermediate({})",
                    donor_name.unwrap_or_else(|| context.plan.secondary()[source].name.as_str())
                ),
                match_count: count,
                matched_key: keys
                    .iter()
                    .map(|(column, value)| {
                        (table.schema().columns()[*column].name.clone(), own(*value))
                    })
                    .collect(),
                identity: failure_identity(context.candidate, context.plan.keys(), context.row)?,
            }));
        };
        chosen = matching::select(&found.rows, selection, |row, field| {
            cell(table, source, context.assignment, row, field)
        })?;
        if let Some(counter) = handlers.as_mut() {
            counter
                .record(
                    &std::format!("{}.multiple_matches", context.assignment.path),
                    HandlerKind::MultipleMatches,
                )
                .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))?;
        }
    }

    let value = cell(table, source, context.assignment, chosen, column)?;
    Ok(own(value))
}

/// Retain the secondary source and original row coordinates on opaque port failures.
fn cell<'a, E>(
    table: &'a dyn TableAccess<Error = E>,
    source: usize,
    assignment: &Assignment,
    source_row: usize,
    column: usize,
) -> Result<ValueRef<'a>, Box<ExecutionError<E>>> {
    table.cell(source_row, column).map_err(|error| {
        Box::new(if source == usize::MAX {
            ExecutionError::Cell {
                path: assignment.path.clone(),
                source_row,
                error,
            }
        } else {
            ExecutionError::SecondaryCell {
                path: assignment.path.clone(),
                source,
                source_row,
                error,
            }
        })
    })
}

/// A source binding's implicit join: match, narrow, then select (LOOKUP-25).
#[allow(clippy::too_many_arguments)]
pub(super) fn read_source<E>(
    table: &dyn TableAccess<Error = E>,
    lookup: &Lookup,
    range: Option<BoundRange<'_>>,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    absence: Option<&crate::dataset::JoinAbsence>,
    context: &Context<'_>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<E>>> {
    let keys = lookup
        .keys
        .iter()
        .map(|key| {
            (
                key.source_column,
                ValueRef::from(&context.candidate.values[key.output_column]),
            )
        })
        .collect::<Vec<_>>();
    read_matching(
        table,
        lookup.source,
        lookup.column,
        &keys,
        context,
        Options {
            range,
            filter,
            selection,
            site: absence.map_or(
                JoinSite::Uncounted(&context.assignment.path),
                JoinSite::Read,
            ),
            donor_name: None,
        },
        Some(handlers),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_joined<T: TableAccess + ?Sized>(
    current: &T,
    table: &dyn TableAccess<Error = T::Error>,
    source: usize,
    joined: &crate::dataset::JoinedRead,
    assignment: &Assignment,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    range: Option<BoundRange<'_>>,
    filter: Option<&crate::bound_expression::BoundPredicate>,
    selection: Option<&SourceSelection>,
    site: JoinSite<'_>,
    handlers: &mut HandlerCounter,
) -> Result<Value, Box<ExecutionError<T::Error>>> {
    let mut keys = Vec::new();
    for (index, key) in joined.keys.iter().enumerate() {
        let value = if let Some(records) = &joined.record_keys {
            let source_row = candidate.members[0];
            current.cell(source_row, records[index]).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: assignment.path.clone(),
                    source_row,
                    error,
                })
            })?
        } else {
            ValueRef::from(&candidate.values[key.output_column])
        };
        keys.push((key.source_column, value));
    }
    read_matching(
        table,
        source,
        joined.column,
        &keys,
        &Context {
            assignment,
            candidate,
            plan,
            row,
        },
        Options {
            range,
            filter,
            selection,
            site,
            donor_name: Some(&joined.name),
        },
        Some(handlers),
    )
}
