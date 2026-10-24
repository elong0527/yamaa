//! Implicit joins (LOOKUP-1): a read through a dataset-qualified source
//! matches, narrows and selects on its declared port and answers a row
//! without a record (LOOKUP-24).
use super::buffer::Values;
use super::eval::owned;
use super::index::exact;
use super::relations::{candidates, select, Probe, Refusal, Site};
use super::{Cx, Fail, Run, Table};
use crate::bound_expression::BoundPredicate;
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Op};
use crate::dataset::{AggregateSource, JoinAbsence, JoinedRead, RelationRange, SourceSelection};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::{TableAccess, ValueRef};
use crate::value::{Value, ValueType};

/// Where a read through an implicit join is made, and so what answers a row
/// the join finds no record for.
#[derive(Clone, Copy)]
pub(super) enum JoinSite<'s> {
    /// A source binding or a bound operand: its own answer, counted at its
    /// path unless it fails.
    Read(&'s JoinAbsence),
    /// A formula operand read at this path: missing, counted under `absent`.
    Operand(&'s str),
    /// A read made while a predicate is evaluated, or a plan-level lookup:
    /// missing, uncounted; the path locates an `unmatched_key` failure.
    Uncounted(&'s str),
}

impl<'s> JoinSite<'s> {
    fn path(self) -> &'s str {
        match self {
            Self::Read(absence) => &absence.path,
            Self::Operand(path) | Self::Uncounted(path) => path,
        }
    }
}

/// A source binding's options on its implicit join.
#[derive(Clone, Copy, Default)]
pub(super) struct Options<'o> {
    pub range: Option<&'o RelationRange>,
    pub filter: Option<&'o BoundPredicate>,
    pub selection: Option<&'o SourceSelection>,
}

/// An implicit join node over rows.
pub(super) fn join<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let node = plan.expr(expr);
    let owner = Site::node(plan, expr);
    let mut values = Values::missing(0);
    for &row in rows {
        let value = match &node.op {
            Op::Joined(read) => joined_value(
                run,
                cx,
                owner,
                read,
                None,
                JoinSite::Operand(&node.path),
                row,
            )?,
            Op::JoinedSource {
                read,
                range,
                filter,
                selection,
                absence,
            } => {
                let site = absence
                    .as_ref()
                    .map_or(JoinSite::Uncounted(&node.path), JoinSite::Read);
                let options = Options {
                    range: range.as_ref(),
                    filter: filter.as_ref(),
                    selection: selection.as_ref(),
                };
                joined_value(run, cx, owner, read, Some(options), site, row)?
            }
            Op::SourceLookup {
                lookup,
                range,
                filter,
                selection,
                absence,
            } => {
                let range = match range {
                    Some(range) => Some((
                        range,
                        super::relations::current_value(
                            run,
                            cx,
                            owner,
                            range.current(),
                            range.path(),
                            Some(row),
                        )?,
                    )),
                    None => None,
                };
                let keys: Vec<(usize, Value)> = lookup
                    .keys
                    .iter()
                    .map(|key| (key.source_column, cx.frame.value(row, key.output_column)))
                    .collect();
                let site = absence
                    .as_ref()
                    .map_or(JoinSite::Uncounted(&node.path), JoinSite::Read);
                let table = cx.secondary[lookup.source];
                matching(
                    run,
                    cx,
                    owner,
                    Donor {
                        table,
                        source: lookup.source,
                        name: None,
                    },
                    lookup.column,
                    &keys,
                    row,
                    range
                        .as_ref()
                        .map(|(range, value)| (*range, ValueRef::from(value))),
                    filter.as_ref(),
                    selection.as_ref(),
                    site,
                    true,
                )?
            }
            _ => unreachable!("a join node holds its read"),
        };
        values.push_value(value);
    }
    Ok(values)
}

/// One row's read through a qualified implicit join: row templates match
/// raw driver fields, column joins match completed output keys. `owner` is
/// the node whose path names the read.
pub(super) fn joined_value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    joined: &JoinedRead,
    options: Option<Options<'_>>,
    site: JoinSite<'_>,
    row: usize,
) -> Result<Value, Fail<E>> {
    let options = options.unwrap_or_default();
    let range = match options.range {
        Some(range) => Some((
            range,
            super::relations::current_value(
                run,
                cx,
                owner,
                range.current(),
                range.path(),
                Some(row),
            )?,
        )),
        None => None,
    };
    let (source, table) = match joined.source {
        AggregateSource::Primary => (usize::MAX, cx.primary),
        AggregateSource::Secondary(index) => (index, cx.secondary[index]),
    };
    let path = owner.path;
    let mut keys = Vec::with_capacity(joined.keys.len());
    for (index, key) in joined.keys.iter().enumerate() {
        let value = if let Some(records) = &joined.record_keys {
            let source_row = cx.frame.members(row)[0];
            owned(cx.table.get(source_row, records[index]).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: path.to_string(),
                    source_row,
                    error,
                })
            })?)
        } else {
            cx.frame.value(row, key.output_column)
        };
        keys.push((key.source_column, value));
    }
    matching(
        run,
        cx,
        owner,
        Donor {
            table,
            source,
            name: Some(&joined.name),
        },
        joined.column,
        &keys,
        row,
        range
            .as_ref()
            .map(|(range, value)| (*range, ValueRef::from(value))),
        options.filter,
        options.selection,
        site,
        true,
    )
}

/// The donor of one implicit join: its port, its position (`usize::MAX`
/// for the primary) and its name when the read declares one.
struct Donor<'t, 'n, E> {
    table: Table<'t, E>,
    source: usize,
    name: Option<&'n str>,
}

/// Retain the donor and its row coordinates on opaque port failures.
fn cell<'t, E>(
    donor: &Donor<'t, '_, E>,
    path: &str,
    source_row: usize,
    column: usize,
) -> Result<ValueRef<'t>, Fail<E>> {
    donor.table.get(source_row, column).map_err(|error| {
        Box::new(if donor.source == usize::MAX {
            ExecutionError::Cell {
                path: path.into(),
                source_row,
                error,
            }
        } else {
            ExecutionError::SecondaryCell {
                path: path.into(),
                source: donor.source,
                source_row,
                error,
            }
        })
    })
}

/// Match, narrow and select on the implicit join (LOOKUP-25), preserving raw
/// match-key evidence. `counted` keeps the handler counts of the read.
#[allow(clippy::too_many_arguments)]
fn matching<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    donor: Donor<'_, '_, E>,
    column: usize,
    keys: &[(usize, Value)],
    row: usize,
    range: Option<(&RelationRange, ValueRef<'_>)>,
    filter: Option<&BoundPredicate>,
    selection: Option<&SourceSelection>,
    site: JoinSite<'_>,
    counted: bool,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let path = owner.path.to_string();
    let table = donor.table;
    let probe_keys: Vec<(usize, ValueRef<'_>)> = keys
        .iter()
        .map(|(column, value)| (*column, ValueRef::from(value)))
        .collect();
    let kinds: Vec<Option<ValueType>> = keys
        .iter()
        .map(|&(column, _)| {
            table
                .schema()
                .columns()
                .get(column)
                .map(|field| field.kind.into())
        })
        .collect();
    let probe = Probe {
        keys: &probe_keys,
        kinds: &kinds,
        range,
    };
    if probe.incomplete() {
        return absent(run, cx, owner, &donor, keys, row, site, counted);
    }
    // The join operator (#2842): the records whose keys equal the match
    // values come from the port's key index when every pair can be answered
    // exactly; the range and the filter then run on them in stored order.
    let bucket = bucket(run, table, &probe_keys, &kinds, |source_row, field| {
        cell(&donor, &path, source_row, field)
    })?;
    let indexed = Probe {
        keys: &[],
        kinds: &[],
        range: probe.range,
    };
    let (rows, probe) = match &bucket {
        Some(rows) => (Rows::Bucket(rows.iter()), &indexed),
        None => (Rows::Scan(0..table.row_count()), &probe),
    };
    let expr = owner.expr;
    let found = candidates(
        rows,
        probe,
        |source_row, field| cell(&donor, &path, source_row, field),
        |source_row, run: &mut Run<'_, '_, E>| match filter {
            None => Ok(true),
            Some(filter) => {
                let evaluate = || {
                    super::predicates::row_truth(filter, table, source_row, |_| ValueRef::Missing)
                        .map(|truth| truth == crate::expression::Truth::True)
                        .map_err(|error| {
                            Box::new(ExecutionError::Predicate {
                                source_row,
                                error: *error,
                            })
                        })
                };
                match expr {
                    // The filter reads only the record: one answer each.
                    Some(expr) => run.ports.kept(expr, table, source_row, evaluate),
                    None => evaluate(),
                }
            }
        },
        run,
    )
    .map_err(|refusal| match refusal {
        Refusal::Failed(error) => error,
        Refusal::Key {
            index,
            current,
            donor,
        } => Box::new(ExecutionError::ScalarDiagnostic(
            crate::diagnostic::lookup::key_type(
                site.path(),
                crate::diagnostic::lookup::Owner::Unnamed,
                &table.schema().columns()[keys[index].0].name,
                crate::result_type::ResultType::known(current),
                crate::result_type::ResultType::known(donor),
            ),
        )),
        Refusal::Range(types) => Box::new(ExecutionError::ScalarDiagnostic(
            types.diagnostic_for(range.map_or("", |(range, _)| range.path()), None),
        )),
    })?;
    let count = found.rows.len();
    if count == 0 {
        return absent(run, cx, owner, &donor, keys, row, site, counted);
    }
    let mut chosen = found.rows[0];
    let name = donor_name(plan, cx, &donor);
    if count > 1 {
        let Some(selection) = selection else {
            return Err(Box::new(ExecutionError::MultipleMatches {
                path,
                dataset: name.clone(),
                intermediate: format!("intermediate({name})"),
                match_count: count,
                matched_key: keys
                    .iter()
                    .map(|(column, value)| {
                        (
                            table.schema().columns()[*column].name.clone(),
                            value.clone(),
                        )
                    })
                    .collect(),
                identity: run.failure_identity(cx, row),
            }));
        };
        chosen = select(&found.rows, selection, |source_row, field| {
            cell(&donor, &path, source_row, field)
        })?;
        if counted {
            run.fire_at(
                owner.expr,
                &format!("{path}.multiple_matches"),
                HandlerKind::MultipleMatches,
            )?;
        }
    }
    Ok(owned(cell(&donor, &path, chosen, column)?))
}

/// The records a match reads, in stored order: an index bucket, or every
/// record for the scan.
pub(super) enum Rows<'b> {
    Bucket(std::slice::Iter<'b, usize>),
    Scan(std::ops::Range<usize>),
}

impl Iterator for Rows<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        match self {
            Self::Bucket(rows) => rows.next().copied(),
            Self::Scan(rows) => rows.next(),
        }
    }
}

/// The records of a port whose keys equal the match values, from the port's
/// key index when every pair can be answered exactly, or none for the scan.
pub(super) fn bucket<'t, E>(
    run: &mut Run<'_, '_, E>,
    table: Table<'t, E>,
    keys: &[(usize, ValueRef<'_>)],
    kinds: &[Option<ValueType>],
    read: impl FnMut(usize, usize) -> Result<ValueRef<'t>, Fail<E>>,
) -> Result<Option<Vec<usize>>, Fail<E>> {
    if keys.is_empty() || !exact(keys, kinds) {
        return Ok(None);
    }
    let columns: Vec<usize> = keys.iter().map(|(column, _)| *column).collect();
    Ok(run
        .ports
        .keys(table, &columns, read)?
        .map(|index| index.matching_pairs(keys).to_vec()))
}

/// The name a join's donor is known by in diagnostics.
fn donor_name<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    cx: &Cx<'_, E>,
    donor: &Donor<'_, '_, E>,
) -> String {
    donor
        .name
        .map(String::from)
        .unwrap_or_else(|| plan.region(cx.region).secondary[donor.source].name.clone())
}

/// Answer a row whose match finds no record or reads a missing match value
/// (LOOKUP-24): the declared `absent` value, or missing by default, counted
/// at the read's path except during a predicate; or `unmatched_key` when the
/// read, or its input, requires a match and the read declares no `absent`.
#[allow(clippy::too_many_arguments)]
fn absent<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    donor: &Donor<'_, '_, E>,
    keys: &[(usize, Value)],
    row: usize,
    site: JoinSite<'_>,
    counted: bool,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let dataset = donor_name(plan, cx, donor);
    let input = plan.region(cx.region).required_matches.contains(&dataset);
    let (path, answer, required, counts) = match site {
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
            intermediate: format!("intermediate({dataset})"),
            dataset,
            matched_key: keys
                .iter()
                .map(|(column, value)| {
                    (
                        donor.table.schema().columns()[*column].name.clone(),
                        value.clone(),
                    )
                })
                .collect(),
            identity: run.failure_identity(cx, row),
        }));
    }
    if counts && counted {
        run.fire_at(owner.expr, &format!("{path}.absent"), HandlerKind::Absent)?;
    }
    Ok(answer.cloned().unwrap_or(Value::Missing))
}
