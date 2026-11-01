//! The key grain of a specification without rows: the source filter over
//! every record, every key derived before any other column, then one
//! candidate per distinct key with its complete feeding records.
use super::buffer::Values;
use super::frame::{Frame, Identity, NO_TEMPLATE};
use super::partitions::Coordinates;
use super::{reserved, Fail, Footprint, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{BindingId, RegionId, Step};
use crate::table::{ColumnRef, TableAccess, ValueRef};
use crate::table_grouping::GroupingError;

/// Build the candidates of the key-grain template into `frame`. A
/// partition's run (#3830) marks the filter, the key bindings and the
/// grouping apart: the whole-table run finishes each over every record
/// before the next.
pub(super) fn construct<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    frame: &mut Frame,
    secondary: &[Table<'s, E>],
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let region = RegionId::DATASET;
    let template = plan
        .region_templates(region)
        .next()
        .expect("the key grain has one template");
    let table = Table::Source(run.primary);
    let rows = table.row_count();
    run.mark(Coordinates::Records)?;
    // A partition's run admits its partition's records on its own (#3792).
    crate::limits::Limits::table(rows, plan.keys().len())
        .map_err(|error| Box::new(ExecutionError::Grouping(GroupingError::Limit(error))))?;
    // The template's first step, its filter or else its first key binding,
    // is its checkpoint (#2822).
    let filter = plan.template(template).filter;
    let bindings: Vec<BindingId> = plan.row_bindings(template).collect();
    if filter.is_some() {
        run.poll(Step::Filter(template))?;
    }
    if let Some(&first) = bindings.first() {
        run.poll(Step::Row(first))?;
    }
    // Finish the entire source filter before any key conversion can fail.
    let mut retained = Vec::new();
    reserved(retained.try_reserve_exact(rows))?;
    for row in 0..rows {
        if let Some(filter) = filter {
            let crate::dataset::resolved::Op::Predicate(predicate) = &plan.expr(filter.expr).op
            else {
                unreachable!("a filter holds its predicate")
            };
            let truth = super::predicates::row_truth(predicate, table, row, |_| ValueRef::Missing)
                .map_err(|error| {
                    Box::new(ExecutionError::Predicate {
                        source_row: row,
                        error: *error,
                    })
                })?;
            if truth != crate::expression::Truth::True {
                continue;
            }
        }
        retained.push(row);
    }
    let keys: Vec<usize> = plan
        .keys()
        .iter()
        .map(|&key| run.local_column(region, key))
        .collect();
    // When every key copies a driver field unchanged and the port lends each
    // field's storage (#4202), the keys read that storage: their bindings
    // would convert nothing and count no handler, so the run neither
    // completes nor gathers them again. Otherwise every key binding
    // completes over the probes, where a binding may read another key.
    let stored: Option<Vec<ColumnRef<'s>>> = plan
        .keys()
        .iter()
        .map(|&key| {
            plan.copied_field(template, key)
                .and_then(|field| table.storage(field))
        })
        .collect();
    let bindings = if stored.is_some() {
        Vec::new()
    } else {
        bindings
    };
    // One probe row per source record, so a probe's position is its record,
    // for the keys their bindings complete.
    let mut probes = Frame::new(frame.width());
    if !bindings.is_empty() {
        reserved(probes.try_reserve(rows, rows))?;
        probes.extend(NO_TEMPLATE, rows, |row, records| {
            records.push(row);
            Identity::Probe(row)
        });
    }
    let callbacks = bindings
        .iter()
        .any(|&binding| run.step_callbacks(region, Step::Row(binding)));
    let key_columns: Vec<usize> = bindings
        .iter()
        .map(|&binding| run.local_column(region, plan.binding(binding).column))
        .collect();
    run.mark(Coordinates::Records)?;
    let footprint = Footprint::Cells {
        columns: &key_columns,
        rows: &retained,
    };
    if !bindings.is_empty() {
        run.run_unit(
            &mut probes,
            footprint,
            callbacks,
            |run, probes, row_mode| {
                if row_mode {
                    for &row in &retained {
                        for &binding in &bindings {
                            complete(run, probes, binding, table, secondary, &[row])?;
                        }
                    }
                } else {
                    for &binding in &bindings {
                        complete(run, probes, binding, table, secondary, &retained)?;
                    }
                }
                Ok(())
            },
        )?;
    }
    run.mark(Coordinates::Frame)?;
    run.poll(Step::KeyGrain)?;
    // Key `key` of record `record`.
    let at = |record: usize, key: usize| match &stored {
        Some(columns) => columns[key].get(record),
        None => probes.get(record, keys[key]),
    };
    let value = |position: usize, key: usize| at(retained[position], key);
    // Partition the retained records by their keys, in first-occurrence
    // order: without hashing while they arrive in key order, a column at a
    // time when the keys read storage and every record is retained.
    let allocation = |_| Box::new(ExecutionError::Grouping(GroupingError::Allocation));
    let ordered = match &stored {
        Some(columns) if retained.len() == rows => {
            crate::table_grouping::ascending_columns(columns, 0..rows).map_err(allocation)?
        }
        _ => None,
    };
    let partitions = match ordered {
        Some((group_of, groups)) => {
            super::group::Groups::from_assignment(&group_of, groups).map_err(allocation)?
        }
        None => super::group::partition(retained.len(), keys.len(), value).map_err(allocation)?,
    };
    // Each candidate is a whole group, at its first member, or, for a group
    // with a missing key, one member: each missing-key record survives
    // independently for the output gate. Candidates follow their first
    // records.
    let mut split = Vec::new();
    reserved(split.try_reserve_exact(partitions.len()))?;
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    reserved(candidates.try_reserve(partitions.len()))?;
    for (group, members) in partitions.iter().enumerate() {
        let first = retained[members[0]];
        let missing = (0..keys.len()).any(|key| matches!(at(first, key), ValueRef::Missing));
        split.push(missing);
        if missing {
            reserved(candidates.try_reserve(members.len() - 1))?;
            candidates.extend(members.iter().map(|&member| (member, group)));
        } else {
            candidates.push((members[0], group));
        }
    }
    if split.contains(&true) {
        candidates.sort_unstable_by_key(|&(first, _)| first);
    }
    run.check_capacity(region, 0, candidates.len())?;
    reserved(frame.try_reserve(candidates.len(), retained.len()))?;
    // The key group of each candidate, by frame row, for the output key
    // check over the same keys and rows.
    let mut key_groups = Vec::new();
    reserved(key_groups.try_reserve_exact(candidates.len()))?;
    let start = frame.rows();
    // The record each candidate takes its keys from.
    let mut firsts = Vec::new();
    reserved(firsts.try_reserve_exact(candidates.len()))?;
    frame.extend(NO_TEMPLATE, candidates.len(), |index, records| {
        let (position, group) = candidates[index];
        let first = records.len();
        if split[group] {
            records.push(retained[position]);
        } else {
            records.extend(
                partitions
                    .group(group)
                    .iter()
                    .map(|&member| retained[member]),
            );
        }
        firsts.push(records[first]);
        key_groups.push(group);
        Identity::Candidate(start + index)
    });
    // Each key column is gathered in one pass (#4190), from the field's
    // storage or from the probes.
    let rows: Vec<usize> = (start..frame.rows()).collect();
    for (index, &key) in keys.iter().enumerate() {
        let values = match &stored {
            Some(columns) => Values::from_column(columns[index], &firsts)
                .expect("every record is a row of its input"),
            None => probes.column(key).gather(&firsts),
        };
        frame.complete_rows(&rows, key, reserved(values)?);
    }
    run.key_groups = (start == 0).then_some(key_groups);
    Ok(())
}

/// One key binding over the probe rows of its records.
fn complete<'a: 's, 's, E>(
    run: &mut Run<'_, 'a, E>,
    probes: &mut Frame,
    binding: BindingId,
    table: Table<'s, E>,
    secondary: &[Table<'s, E>],
    rows: &[usize],
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let region = RegionId::DATASET;
    let values = {
        let cx = run.template_cx(probes, region, table, secondary, 0);
        run.evaluate_binding(&cx, binding, rows)?
    };
    let column = run.local_column(region, plan.binding(binding).column);
    probes.complete_rows(rows, column, values);
    Ok(())
}
