//! Predicate declarations and row checks at the completed-dataset verification phase.
use super::{Check, CheckRecord, ExecutionError, Verification};
use crate::bound_expression::BoundPredicate;
use crate::dataset::execution as dataset;
use crate::{
    bound_expression::{Binding, Read},
    predicate::{ErrorKind, Truth},
    table::TableAccess,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{ColumnType, Value},
};
use alloc::{boxed::Box, vec::Vec};

/// Nonmissing representatives expose invalid declarations even for empty output.
fn sample(kind: ColumnType) -> Value {
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

#[allow(clippy::too_many_arguments)]
fn validate_predicate<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    dataset: &CompletedRows<'_>,
    plan: &dataset::DatasetPlan,
    state: &mut dataset::EvaluationState<'_, T::Error>,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Box<ExecutionError<T::Error>>> {
    let mut samples = Vec::new();
    let mut bindings = Vec::new();
    for binding in predicate.bindings() {
        let kind = match binding.read {
            Read::Column(column) => plan.output().columns()[column].kind,
            Read::Intermediate { index, column } => {
                plan.intermediates()[index]
                    .source_schema(plan.secondary())
                    .expect("admitted donor schema")
                    .columns()[column]
                    .kind
            }
            _ => unreachable!(
                "verification admission permits completed columns and declared lookups"
            ),
        };
        bindings.push(Binding {
            name: binding.name.clone(),
            read: Read::Column(samples.len()),
        });
        samples.push(sample(kind));
    }
    let sample_predicate = BoundPredicate::new(predicate.plan().clone(), bindings)
        .expect("admitted verification names");
    crate::dataset::execution::predicates::evaluate(
        &sample_predicate,
        dataset,
        0,
        &samples,
        state.budget.predicate(),
    )
    .map_err(|error| {
        Box::new(match error.kind {
            ErrorKind::Limit(limit) => dataset::predicate_limit(limit),
            _ => ExecutionError::VerificationPredicate {
                error,
                records: core::mem::take(records),
            },
        })
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_predicate<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    dataset: &CompletedRows<'_>,
    plan: &dataset::DatasetPlan,
    primary: &T,
    candidate: &dataset::Candidate,
    position: usize,
    assignment: &dataset::Assignment,
    state: &mut dataset::EvaluationState<'_, T::Error>,
    records: &mut Vec<CheckRecord>,
) -> Result<Truth, Box<ExecutionError<T::Error>>> {
    if !predicate
        .bindings()
        .iter()
        .any(|binding| matches!(binding.read, Read::Intermediate { .. }))
    {
        return crate::dataset::execution::predicates::evaluate(
            predicate,
            dataset,
            0,
            &candidate.values,
            state.budget.predicate(),
        )
        .map_err(|error| {
            Box::new(match error.kind {
                ErrorKind::Limit(limit) => dataset::predicate_limit(limit),
                _ => ExecutionError::VerificationPredicate {
                    error,
                    records: core::mem::take(records),
                },
            })
        });
    }
    super::case_predicates::evaluate(
        predicate, primary, candidate, plan, position, assignment, state,
    )
    .map_err(|error| match *error {
        ExecutionError::Predicate { source_row, error } => match error.kind {
            ErrorKind::Condition(condition) => Box::new(ExecutionError::VerificationPredicate {
                error: crate::predicate::EvaluationError {
                    spec_path: error.spec_path,
                    expression: error.expression,
                    route: error.route,
                    kind: ErrorKind::Condition(condition),
                },
                records: core::mem::take(records),
            }),
            _ => Box::new(ExecutionError::Predicate { source_row, error }),
        },
        other => Box::new(other),
    })
}

/// Validate declarations in order, then evaluate every row without a `when` short circuit.
/// Semantic failures retain previously completed check records and never an output artifact.
#[allow(clippy::too_many_arguments)]
pub(crate) fn predicate_check<T: TableAccess + ?Sized>(
    verification: &Verification,
    dataset: &CompletedRows<'_>,
    plan: &dataset::DatasetPlan,
    primary: &T,
    candidates: &[dataset::Candidate],
    state: &mut dataset::EvaluationState<'_, T::Error>,
    records: &mut Vec<CheckRecord>,
) -> Result<Option<CheckRecord>, Box<ExecutionError<T::Error>>> {
    let (when, require, metadata) = match &verification.check {
        Check::Assert { when, require } => (
            when.as_ref(),
            require,
            Some(crate::diagnostic::ConditionCode::VerificationAssertFailed),
        ),
        Check::PredicateDeclaration(predicate) => (None, predicate, None),
        _ => unreachable!("only predicate checks use this service"),
    };
    // Validate types from immutable schemas, including an empty output. These
    // synthetic operands never select donors, run handlers or read source cells.
    for predicate in when.into_iter().chain([require]) {
        validate_predicate::<T>(predicate, dataset, plan, state, records)?;
    }
    let Some(code) = metadata else {
        return Ok(None);
    };
    let assignment = dataset::Assignment {
        column: 0,
        path: verification.path.clone(),
        expression: dataset::Expression::Literal(Value::Missing),
    };
    let mut offending = Vec::new();
    for (position, candidate) in candidates.iter().enumerate() {
        let mut evaluate = |predicate: &BoundPredicate| {
            evaluate_predicate(
                predicate,
                dataset,
                plan,
                primary,
                candidate,
                position,
                &assignment,
                state,
                records,
            )
        };
        let binds = match when {
            Some(when) => evaluate(when)?,
            None => Truth::True,
        };
        let holds = evaluate(require)?;
        if binds == Truth::True && holds != Truth::True {
            offending.push(position);
        }
    }
    Ok(Some(CheckRecord {
        path: verification.path.clone(),
        code,
        evaluated_count: candidates.len(),
        failed_count: offending.len(),
        output_rows: candidates.len(),
        codelist: None,
        inventory: None,
        row_count: None,
        offending_rows: dataset::identities(dataset, plan.keys(), offending, state.budget)?,
    }))
}

/// Partition all artifact rows before applying predicates; a filtered-empty group remains visible.
#[allow(clippy::too_many_arguments)]
pub(crate) fn row_count_check<T: TableAccess + ?Sized>(
    verification: &Verification,
    check: &crate::dataset::RowCount,
    dataset: &CompletedRows<'_>,
    plan: &dataset::DatasetPlan,
    primary: &T,
    candidates: &[dataset::Candidate],
    limits: dataset::Limits,
    state: &mut dataset::EvaluationState<'_, T::Error>,
    records: &mut Vec<CheckRecord>,
) -> Result<CheckRecord, Box<ExecutionError<T::Error>>> {
    state.budget.work(dataset.rows().len(), 2)?;
    // Type admission must not select a donor, even for empty or exempt groups.
    for predicate in check.filter.iter().chain(&check.when) {
        validate_predicate::<T>(predicate, dataset, plan, state, records)?;
    }
    let assignment = dataset::Assignment {
        column: 0,
        path: verification.path.clone(),
        expression: dataset::Expression::Literal(Value::Missing),
    };
    let mut admitted = Vec::with_capacity(dataset.rows().len());
    let mut bound = Vec::with_capacity(dataset.rows().len());
    for (position, candidate) in candidates.iter().enumerate() {
        admitted.push(match &check.filter {
            Some(filter) => {
                evaluate_predicate(
                    filter,
                    dataset,
                    plan,
                    primary,
                    candidate,
                    position,
                    &assignment,
                    state,
                    records,
                )? == Truth::True
            }
            None => true,
        });
    }
    for (position, candidate) in candidates.iter().enumerate() {
        bound.push(match &check.when {
            Some(when) => {
                evaluate_predicate(
                    when,
                    dataset,
                    plan,
                    primary,
                    candidate,
                    position,
                    &assignment,
                    state,
                    records,
                )? == Truth::True
            }
            None => true,
        });
    }
    let groups = if let Some(columns) = &check.group_by {
        state.budget.work(dataset.rows().len(), columns.len())?;
        crate::table_grouping::partition(dataset, columns, limits.output_rows, limits.key_cells)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?
    } else {
        alloc::vec![(0..dataset.rows().len()).collect()]
    };
    state.budget.work(groups.len(), check.bound_work())?;
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
                .map(|&column| &dataset.rows()[members[0]][column]);
            state.budget.identity(borrowed.clone())?;
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
        code: crate::diagnostic::ConditionCode::VerificationRowCountFailed,
        evaluated_count: groups.len(),
        failed_count: offenders.len(),
        output_rows: dataset.rows().len(),
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

use crate::table::{CellError, TableSchema, ValueRef};
use core::convert::Infallible;

/// Borrow completed cells while retaining the attempt's candidate selection identities.
pub(crate) struct CompletedRows<'a> {
    pub(crate) schema: &'a TableSchema,
    pub(crate) rows: Vec<&'a Vec<Value>>,
}
impl CompletedRows<'_> {
    pub(crate) fn rows(&self) -> &[&Vec<Value>] {
        &self.rows
    }
}
impl TableAccess for CompletedRows<'_> {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
