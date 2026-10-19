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
pub(super) fn construct<'a, E>(
    run: &mut Run<'_, 'a, E>,
    frame: &mut Frame,
    secondary: &[Table<'a, E>],
) -> Result<(), Fail<E>> {
    let plan = run.plan;
    let region = RegionId::DATASET;
    let template = plan
        .region_templates(region)
        .next()
        .expect("the key grain has one template");
    let table = Table::Source(run.primary);
    let rows = table.row_count();
    crate::limits::Limits::table(rows, plan.keys().len())
        .map_err(|error| Box::new(ExecutionError::Grouping(GroupingError::Limit(error))))?;
    // Finish the entire source filter before any key conversion can fail.
    let mut retained = Vec::new();
    reserved(retained.try_reserve_exact(rows))?;
    let filter = plan.template(template).filter;
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
    // One probe row per source record, so a probe's position is its record.
    let mut probes = Frame::new(frame.width());
    reserved(probes.try_reserve(rows, rows))?;
    for row in 0..rows {
        probes.push(NO_TEMPLATE, Identity::Probe(row), &[row]);
    }
    let bindings: Vec<BindingId> = plan.row_bindings(template).collect();
    let callbacks = bindings
        .iter()
        .any(|&binding| run.step_callbacks(region, Step::Row(binding)));
    let key_columns: Vec<usize> = bindings
        .iter()
        .map(|&binding| run.local_column(region, plan.binding(binding).column))
        .collect();
    let footprint = Footprint::Cells {
        columns: &key_columns,
        rows: &retained,
    };
    run.run_unit(
        &mut probes,
        footprint,
        callbacks,
        |run, probes, row_mode| {
            let unit_rows: Vec<usize> = retained.clone();
            if row_mode {
                for &row in &unit_rows {
                    for &binding in &bindings {
                        complete(run, probes, binding, table, secondary, &[row])?;
                    }
                }
            } else {
                for &binding in &bindings {
                    complete(run, probes, binding, table, secondary, &unit_rows)?;
                }
            }
            Ok(())
        },
    )?;
    let keys: Vec<usize> = plan
        .keys()
        .iter()
        .map(|&key| run.local_column(region, key))
        .collect();
    // Partition the retained records by their keys, in first-occurrence order.
    let partitions = super::group::partition(retained.len(), keys.len(), |position, key| {
        probes.get(retained[position], keys[key])
    })
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
fn complete<'a, E>(
    run: &mut Run<'_, 'a, E>,
    probes: &mut Frame,
    binding: BindingId,
    table: Table<'a, E>,
    secondary: &[Table<'a, E>],
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
