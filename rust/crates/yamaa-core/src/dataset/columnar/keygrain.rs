//! The key grain of a specification without rows: the source filter over
//! every record, every key derived before any other column, then one
//! candidate per distinct key with its complete feeding records.
use super::frame::{Frame, Identity, NO_TEMPLATE};
use super::{reserved, Fail, Footprint, Run, Table};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{BindingId, RegionId, Step};
use crate::table::{TableAccess, ValueRef};
use crate::table_grouping::GroupingError;
use crate::value::Value;

/// Build the candidates of the key-grain template into `frame`.
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
    // A partitioned run admits each partition's records on its own (#3792).
    for records in run.driver_admissions() {
        crate::limits::Limits::table(records, plan.keys().len())
            .map_err(|error| Box::new(ExecutionError::Grouping(GroupingError::Limit(error))))?;
    }
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
    // A partitioned run divides the filter and the key bindings when none
    // of them needs the whole table, then merges the partitions' key groups
    // (#3697).
    let divided = run.divides(|step| match step {
        Step::Filter(owner) => owner == template,
        Step::Row(binding) => bindings.contains(&binding),
        _ => false,
    });
    // Finish the entire source filter before any key conversion can fail,
    // a divided filter one partition after another.
    let records = run.partition_runs(divided && filter.is_some(), rows, Some);
    let mut retained = Vec::new();
    reserved(retained.try_reserve_exact(rows))?;
    for (number, range) in records.into_iter().enumerate() {
        run.next_partition(number)?;
        for row in range {
            if let Some(filter) = filter {
                let crate::dataset::resolved::Op::Predicate(predicate) = &plan.expr(filter.expr).op
                else {
                    unreachable!("a filter holds its predicate")
                };
                let truth =
                    super::predicates::row_truth(predicate, table, row, |_| ValueRef::Missing)
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
    }
    // One probe row per source record, so a probe's position is its record.
    let mut probes = Frame::new(frame.width());
    reserved(probes.try_reserve(rows, rows))?;
    for row in 0..rows {
        probes.push(NO_TEMPLATE, Identity::Probe(row), &[row]);
    }
    let callbacks = bindings
        .iter()
        .any(|&binding| run.step_callbacks(region, Step::Row(binding)));
    let key_columns: Vec<usize> = bindings
        .iter()
        .map(|&binding| run.local_column(region, plan.binding(binding).column))
        .collect();
    // The retained records of one partition after another, each a unit of
    // its own; undivided, all of them are one unit.
    let units = run.partition_runs(divided, retained.len(), |position| Some(retained[position]));
    for (number, range) in units.iter().enumerate() {
        run.next_partition(number)?;
        let unit_rows = &retained[range.clone()];
        let footprint = Footprint::Cells {
            columns: &key_columns,
            rows: unit_rows,
        };
        run.run_unit(
            &mut probes,
            footprint,
            callbacks,
            |run, probes, row_mode| {
                if row_mode {
                    for &row in unit_rows {
                        for &binding in &bindings {
                            complete(run, probes, binding, table, secondary, &[row])?;
                        }
                    }
                } else {
                    for &binding in &bindings {
                        complete(run, probes, binding, table, secondary, unit_rows)?;
                    }
                }
                Ok(())
            },
        )?;
    }
    run.poll(Step::KeyGrain)?;
    let keys: Vec<usize> = plan
        .keys()
        .iter()
        .map(|&key| run.local_column(region, key))
        .collect();
    let value = |position: usize, key: usize| probes.get(retained[position], keys[key]);
    // Partition the retained records by their keys, in first-occurrence
    // order. A divided run groups each partition's records, then merges the
    // partitions' key groups (#3697).
    let partitions = if divided {
        let mut parts: Vec<Vec<Vec<usize>>> = Vec::new();
        for range in &units {
            let local = super::group::partition(range.len(), keys.len(), |item, key| {
                value(range.start + item, key)
            })
            .map_err(|_| Box::new(ExecutionError::Grouping(GroupingError::Allocation)))?;
            parts.push(
                local
                    .iter()
                    .map(|members| members.iter().map(|&item| range.start + item).collect())
                    .collect(),
            );
        }
        super::group::merge(&parts, keys.len(), value)
    } else {
        super::group::partition(retained.len(), keys.len(), value)
    }
    .map_err(|_| Box::new(ExecutionError::Grouping(GroupingError::Allocation)))?;
    let mut groups: Vec<Vec<usize>> = Vec::new();
    reserved(groups.try_reserve_exact(partitions.len()))?;
    for members in partitions.iter() {
        let first = retained[members[0]];
        if keys
            .iter()
            .any(|&key| matches!(probes.get(first, key), ValueRef::Missing))
        {
            // Each missing-key record survives independently for the output gate.
            groups.extend(members.iter().map(|&member| vec![member]));
        } else {
            groups.push(members.to_vec());
        }
    }
    groups.sort_unstable_by_key(|members| members[0]);
    run.check_capacity(region, 0, groups.len())?;
    reserved(frame.try_reserve(groups.len(), retained.len()))?;
    for members in groups {
        let records: Vec<usize> = members.iter().map(|&member| retained[member]).collect();
        let first = records[0];
        let row = frame.push(NO_TEMPLATE, Identity::Candidate(frame.rows()), &records);
        for &key in &keys {
            let value: Value = probes.value(first, key);
            frame.complete_value(row, key, value);
        }
    }
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
