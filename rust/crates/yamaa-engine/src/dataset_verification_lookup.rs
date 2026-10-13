//! Completed assertions reuse the existing operand context and selection owner.
use super::*;
use core::convert::Infallible;
use yamaa_core::{
    bound_expression::Read,
    predicate::{self, Resolved, Resolver},
    table::{CellError, ValueRef},
    value::Selection,
};

struct Samples<'a> {
    predicate: &'a BoundPredicate,
    values: Vec<Value>,
}
impl Resolver for Samples<'_> {
    type Error = CellError<Infallible>;
    fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
        self.resolve_value(name).map(|value| match value {
            Resolved::Absent => Selection::Absent,
            Resolved::Borrowed(value) => Selection::Present(dataset::own(value)),
            Resolved::Owned(value) => Selection::Present(value),
        })
    }
    fn resolve_value(&mut self, name: &str) -> Result<Resolved<'_>, Self::Error> {
        Ok(
            match self
                .predicate
                .bindings()
                .binary_search_by(|binding| binding.name.as_str().cmp(name))
            {
                Ok(index) => Resolved::Borrowed(ValueRef::from(&self.values[index])),
                Err(_) => Resolved::Absent,
            },
        )
    }
}
pub(super) fn sample<E>(
    predicate: &BoundPredicate,
    dataset: &CompletedDataset<'_>,
    plan: &DatasetPlan,
    budget: &mut Budget,
    records: &mut Vec<CheckRecord>,
) -> Result<(), Box<ExecutionError<E>>> {
    let values = predicate
        .bindings()
        .iter()
        .map(|binding| {
            let kind = match binding.read {
                Read::Column(column) => {
                    dataset.schema().columns().get(column).map(|item| item.kind)
                }
                Read::Intermediate { index, column } => plan
                    .intermediates()
                    .get(index)
                    .and_then(|item| item.source_schema(plan.secondary()))
                    .and_then(|schema| schema.columns().get(column))
                    .map(|item| item.kind),
                _ => None,
            }
            .ok_or_else(|| Box::new(ExecutionError::SchemaMismatch))?;
            Ok(super::sample(kind))
        })
        .collect::<Result<Vec<_>, Box<ExecutionError<E>>>>()?;
    predicate
        .plan()
        .evaluate_with_budget(&mut Samples { predicate, values }, budget.predicate())
        .map(|_| ())
        .map_err(|error| {
            Box::new(match error.kind {
                ErrorKind::Limit(limit) => dataset::predicate_limit(limit),
                _ => ExecutionError::VerificationPredicate {
                    error,
                    records: core::mem::take(records),
                },
            })
        })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn actual<T: TableAccess + ?Sized>(
    predicate: &BoundPredicate,
    dataset: &CompletedDataset<'_>,
    table: &T,
    candidate: &Candidate,
    plan: &DatasetPlan,
    row: usize,
    state: &mut EvaluationState<'_, T::Error>,
    records: &mut Vec<CheckRecord>,
) -> Result<Truth, Box<ExecutionError<T::Error>>> {
    if !predicate
        .bindings()
        .iter()
        .any(|binding| matches!(binding.read, Read::Intermediate { .. }))
    {
        return crate::dataset_predicate::evaluate(
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
    // Admission allows only completed columns and selected fields here. The
    // shared context uses this carrier's path for correlated operands; selected
    // source errors belong to the intermediate itself. Its column and expression
    // never participate in assertion resolution or diagnostic attribution.
    let assignment = Assignment {
        column: 0,
        path: predicate.plan().spec_path().into(),
        expression: Expression::Literal(Value::Missing),
    };
    super::super::case_predicates::evaluate_context(
        predicate,
        table,
        candidate,
        plan,
        row,
        &assignment,
        state,
    )
    .map_err(|error| {
        let predicate::EvaluationError {
            spec_path,
            expression,
            route,
            kind,
        } = error;
        match kind {
            ErrorKind::Resolution { error, .. } => error,
            ErrorKind::Limit(limit) => Box::new(dataset::predicate_limit(limit)),
            ErrorKind::Condition(condition) => Box::new(ExecutionError::VerificationPredicate {
                error: predicate::EvaluationError {
                    spec_path,
                    expression,
                    route,
                    kind: ErrorKind::Condition(condition),
                },
                records: core::mem::take(records),
            }),
        }
    })
}
