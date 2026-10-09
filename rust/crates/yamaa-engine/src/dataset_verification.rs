//! Predicate declarations and row checks at the completed-dataset verification phase.
use crate::{
    dataset::{self, Check, CheckRecord, Dataset, ExecutionError, Verification},
    dataset_budget::Budget,
    dataset_predicate::BoundPredicate,
};
use alloc::{boxed::Box, vec::Vec};
use yamaa_core::{
    predicate::{ErrorKind, Truth},
    table::TableAccess,
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{ColumnType, Value},
};

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

/// Validate declarations in order, then evaluate every row without a `when` short circuit.
/// Semantic failures retain previously completed check records and never an output artifact.
pub(crate) fn predicate_check<E>(
    verification: &Verification,
    dataset: &Dataset,
    keys: &[usize],
    budget: &mut Budget,
    records: &mut Vec<CheckRecord>,
) -> Result<Option<CheckRecord>, Box<ExecutionError<E>>> {
    let (when, require, metadata) = match &verification.check {
        Check::Assert { when, require } => {
            (when.as_ref(), require, Some(("assert_failed", "REQ-0384")))
        }
        Check::PredicateDeclaration(predicate) => (None, predicate, None),
        _ => unreachable!("only predicate checks use this service"),
    };
    let samples: Vec<_> = dataset
        .schema()
        .columns()
        .iter()
        .map(|column| sample(column.kind))
        .collect();
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
    // Both declarations are validated before the first actual-row predicate read.
    if let Some(when) = when {
        evaluate(when, &samples)?;
    }
    evaluate(require, &samples)?;
    let Some((condition, requirement)) = metadata else {
        return Ok(None);
    };
    let mut offending = Vec::new();
    for (position, values) in dataset.rows().iter().enumerate() {
        let binds = match when {
            Some(when) => evaluate(when, values)?,
            None => Truth::True,
        };
        let holds = evaluate(require, values)?;
        if binds == Truth::True && holds != Truth::True {
            offending.push(position);
        }
    }
    Ok(Some(CheckRecord {
        path: verification.path.clone(),
        condition,
        requirement,
        evaluated_count: dataset.rows().len(),
        failed_count: offending.len(),
        output_rows: dataset.rows().len(),
        codelist: None,
        offending_rows: dataset::identities(dataset, keys, offending.into_iter(), budget)?,
    }))
}
