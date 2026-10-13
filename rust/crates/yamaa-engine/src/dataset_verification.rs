//! Predicate declarations and row checks at the completed-dataset verification phase.
use super::{
    intermediates, row_drivers, windows, Assignment, Candidate, Check, CheckRecord,
    CompletedDataset, DatasetPlan, EvaluationState, ExecutionError, Expression, FunctionPort,
    Verification,
};
use crate::dataset;
use crate::{
    dataset_budget::Budget, dataset_predicate::BoundPredicate, numeric_lifecycle::HandlerCounter,
};
use alloc::{boxed::Box, vec::Vec};
#[path = "dataset_verification_lookup.rs"]
mod lookup;
use yamaa_core::{
    predicate::{ErrorKind, Truth},
    table::TableAccess,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{ColumnType, Value},
};

/// Nonmissing representatives expose invalid declarations even for empty output.
pub(super) fn sample(kind: ColumnType) -> Value {
    match kind {
        ColumnType::Str => Value::Str("sample".into()),
        ColumnType::Int => Value::Int(0),
        ColumnType::Float => Value::float(0.0),
        ColumnType::Date => Value::Date(Date::new(2000, 1, 1, DatePrecision::Day).unwrap()),
        ColumnType::DateTime => Value::DateTime(
            DateTime::new(
                Date::new(2000, 1, 1, DatePrecision::Day).unwrap(),
                0,
                0,
                0,
                DateTimePrecision::Second,
            )
            .unwrap(),
        ),
    }
}

/// Validate declarations in order, then evaluate every row without a `when` short circuit.
/// Semantic failures retain previously completed check records and never an output artifact.
#[allow(clippy::too_many_arguments)]
pub(super) fn predicate_check<T: TableAccess + ?Sized>(
    verification: &Verification,
    dataset: &CompletedDataset<'_>,
    keys: &[usize],
    table: &T,
    plan: &DatasetPlan,
    functions: &mut dyn FunctionPort<Error = T::Error>,
    secondary: &[&dyn TableAccess<Error = T::Error>],
    intermediate_run: &mut intermediates::Run,
    handlers: &mut HandlerCounter,
    budget: &mut Budget,
    records: &mut Vec<CheckRecord>,
) -> Result<Option<CheckRecord>, Box<ExecutionError<T::Error>>> {
    let (when, require, metadata) = match &verification.check {
        Check::Assert { when, require } => (
            when.as_ref(),
            require,
            Some(yamaa_core::diagnostic::ConditionCode::VerificationAssertFailed),
        ),
        Check::PredicateDeclaration(predicate) => (None, predicate, None),
        _ => unreachable!("only predicate checks use this service"),
    };
    // Schema representatives have no selector, table-cell or function authority.
    if let Some(when) = when {
        lookup::sample(when, dataset, plan, budget, records)?;
    }
    lookup::sample(require, dataset, plan, budget, records)?;
    let Some(code) = metadata else {
        return Ok(None);
    };
    let mut offending = Vec::new();
    for (position, candidate) in dataset.rows.iter().enumerate() {
        let mut windows = windows::Cache::default();
        let primary = row_drivers::Driver::Primary(table);
        let mut state = EvaluationState {
            primary: &primary,
            functions,
            secondary,
            self_rows: dataset.rows,
            budget,
            handlers,
            intermediates: intermediate_run,
            output_rows: dataset.rows,
            output_offset: 0,
            windows: &mut windows,
            aggregate_row: None,
        };
        let binds = match when {
            Some(when) => lookup::actual(
                when, dataset, table, candidate, plan, position, &mut state, records,
            )?,
            None => Truth::True,
        };
        // REQ-0383 evaluates require even when when exempts this row.
        let holds = lookup::actual(
            require, dataset, table, candidate, plan, position, &mut state, records,
        )?;
        if binds == Truth::True && holds != Truth::True {
            offending.push(position);
        }
    }
    Ok(Some(CheckRecord {
        path: verification.path.clone(),
        code,
        evaluated_count: dataset.row_count(),
        failed_count: offending.len(),
        output_rows: dataset.row_count(),
        codelist: None,
        inventory: None,
        row_count: None,
        offending_rows: dataset::identities(dataset, keys, offending.into_iter(), budget)?,
    }))
}

/// Partition all artifact rows before applying predicates; a filtered-empty group remains visible.
pub(super) fn row_count_check<E>(
    verification: &Verification,
    check: &yamaa_core::dataset::RowCount,
    dataset: &CompletedDataset<'_>,
    limits: dataset::Limits,
    budget: &mut Budget,
    records: &mut Vec<CheckRecord>,
) -> Result<CheckRecord, Box<ExecutionError<E>>> {
    let samples: Vec<_> = dataset
        .schema()
        .columns()
        .iter()
        .map(|column| sample(column.kind))
        .collect();
    budget.work(dataset.row_count(), 2)?;
    let mut evaluate = |predicate: &BoundPredicate, values: &[Value]| {
        crate::dataset_predicate::evaluate(predicate, dataset, 0, values, budget.predicate())
            .map_err(|error| {
                Box::new(match error.kind {
                    ErrorKind::Limit(limit) => dataset::predicate_limit(limit),
                    _ => ExecutionError::VerificationPredicate {
                        error,
                        records: core::mem::take(records),
                    },
                })
            })
    };
    // Invalid comparison types are diagnosed even for an empty artifact.
    for predicate in check.filter.iter().chain(&check.when) {
        evaluate(predicate, &samples)?;
    }
    let mut admitted = Vec::with_capacity(dataset.row_count());
    let mut bound = Vec::with_capacity(dataset.row_count());
    for values in dataset.rows() {
        admitted.push(match &check.filter {
            Some(filter) => evaluate(filter, values)? == Truth::True,
            None => true,
        });
    }
    for values in dataset.rows() {
        bound.push(match &check.when {
            Some(when) => evaluate(when, values)? == Truth::True,
            None => true,
        });
    }
    let groups = if let Some(columns) = &check.group_by {
        budget.work(dataset.row_count(), columns.len())?;
        crate::table_grouping::partition(dataset, columns, limits.output_rows, limits.key_cells)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?
    } else {
        alloc::vec![(0..dataset.row_count()).collect()]
    };
    budget.work(groups.len(), check.bound_work())?;
    let mut offenders = Vec::new();
    let mut counts = Vec::new();
    let mut denominators = Vec::new();
    for members in &groups {
        if check.when.is_some() && !members.iter().any(|&row| bound[row]) {
            continue;
        }
        let count = members.iter().filter(|&&row| admitted[row]).count();
        if !check.accepts(count, members.len()) {
            let borrowed = check
                .group_by
                .iter()
                .flatten()
                .map(|&column| &dataset.rows[members[0]].values[column]);
            budget.identity(borrowed.clone())?;
            let values = borrowed.cloned().collect();
            offenders.push(dataset::RowIdentity {
                position: members.first().copied().unwrap_or(0),
                values,
            });
            counts.push(count);
            denominators.push(members.len());
        }
    }
    Ok(CheckRecord {
        path: verification.path.clone(),
        code: yamaa_core::diagnostic::ConditionCode::VerificationRowCountFailed,
        evaluated_count: groups.len(),
        failed_count: offenders.len(),
        output_rows: dataset.row_count(),
        offending_rows: offenders,
        codelist: None,
        inventory: None,
        row_count: Some(dataset::RowCountObservation {
            counts,
            denominators: (check.min_fraction.is_some() || check.max_fraction.is_some())
                .then_some(denominators),
        }),
    })
}
