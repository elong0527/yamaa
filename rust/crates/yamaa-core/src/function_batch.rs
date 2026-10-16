//! Batch invocation of an already-bound project function over logical rows.
//!
//! The batch contract is defined over the shared logical value contract, not
//! over any host array type: adapters convert Arrow columns to owned logical
//! values before calling here and convert accepted results back only after
//! this service has validated the whole batch. Hosts receive named columns
//! in declaration order; every column has exactly the batch length.
//!
//! Semantics, each mirroring the scalar lifecycle in `function_invocation`:
//! - Unknown argument names are rejected before any parameter check, and any
//!   argument failure in any row aborts the batch before the callback runs.
//! - An absent optional parameter broadcasts its declared default to every
//!   row of the batch; an absent required parameter is an error.
//! - A row whose value for a parameter that rejects missing is missing is
//!   short-circuited exactly as scalar invocation returns `Value::Missing`:
//!   later parameters of that row are not checked and the row never enters
//!   the callback. Rows whose parameters accept missing receive explicit
//!   nulls (`Value::Missing`) in their column.
//! - Results are validated as one unit before any output is accepted: the
//!   host must return exactly one result per row it was given, and every
//!   result must satisfy the scalar result rules (no booleans, missing only
//!   when declared, exact declared result type). A wrong-length or
//!   wrong-type batch is rejected whole; no partial output is produced.
//! - Accepted results are scattered back in original row order, interleaved
//!   with the missing results of short-circuited rows.
//! - An empty batch returns an empty result without invoking the callback,
//!   and so does a batch in which every row is short-circuited.
//! - Callback exceptions abort the batch with the original host payload;
//!   host-classified interrupts are reported distinctly by the caller via
//!   the port's interrupt classification, never retried or rolled back.
//! - Chunked invocation checks cancellation before each chunk and between
//!   chunks; a cancelled run discards every previously accepted chunk and
//!   reports `BatchFailureKind::Cancelled` instead of partial output.
use crate::{
    function_invocation::{FailureKind, HostError},
    function_signature::{
        LogicalSignature, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
    },
    table::ValueRef,
    value::{ColumnType, Value, ValueType},
};
use std::collections::BTreeMap;

/// One named batch argument: declaration-order name plus one logical value
/// per row the callback is actually given, in original row order.
#[derive(Clone, Debug, PartialEq)]
pub struct BatchArgument {
    pub name: String,
    pub values: Vec<Value>,
}

/// A host capability that invokes one bound callable once per batch.
///
/// Implementations own all host array conversion. They must return one
/// owned logical value per input row, in input order, or fail the whole
/// call; the core re-validates length and exact result types regardless.
/// Implementations must not short-circuit missing rows themselves: the
/// core has already removed rows the signature short-circuits, and every
/// row supplied here must reach the callable (accepting-missing parameters
/// receive explicit missing values).
pub trait BatchFunctionPort {
    type Handle;
    type Error;
    /// Hosts classify user interrupts; their payload is propagated immediately.
    fn is_interrupt(&self, _error: &Self::Error) -> bool {
        false
    }
    fn call_batch(
        &mut self,
        handle: &Self::Handle,
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<Self::Error>>;
}

/// Adapt any scalar callable to the batch port by invoking it once per row.
///
/// This is the reference batch implementation: its results are scalar
/// results by construction, and hosts with a native vectorized callable
/// must prove equivalence against it (or against scalar invocation).
pub struct ScalarBatchAdapter<F> {
    call: F,
}
impl<F> ScalarBatchAdapter<F> {
    pub fn new(call: F) -> Self {
        Self { call }
    }
}
impl<E, F> BatchFunctionPort for ScalarBatchAdapter<F>
where
    F: FnMut(&[crate::function_invocation::Argument<'_>]) -> Result<Value, HostError<E>>,
{
    type Handle = ();
    type Error = E;
    fn call_batch(
        &mut self,
        _handle: &(),
        arguments: &[BatchArgument],
    ) -> Result<Vec<Value>, HostError<E>> {
        let rows = arguments
            .first()
            .map_or(0, |argument| argument.values.len());
        let mut results = Vec::with_capacity(rows);
        for row in 0..rows {
            let borrowed: Vec<crate::function_invocation::Argument<'_>> = arguments
                .iter()
                .map(|argument| crate::function_invocation::Argument {
                    name: &argument.name,
                    value: ValueRef::from(&argument.values[row]),
                })
                .collect();
            results.push((self.call)(&borrowed)?);
        }
        Ok(results)
    }
}

/// A batch failure retains the function identity and, when the failure is
/// tied to one row, that row's original (pre-short-circuit) index.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("batch invocation: identity={identity:?}, row={row:?}, kind={kind:?}")]
pub struct BatchFailure<E> {
    pub identity: ProjectFunctionIdentity,
    pub row: Option<usize>,
    pub kind: BatchFailureKind<E>,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum BatchFailureKind<E> {
    /// A scalar-semantics failure: argument validation for `row`, or the
    /// host call/result for the batch position mapped back to `row`.
    #[error("scalar failure: {0:?}")]
    Scalar(FailureKind<E>),
    /// The host returned a result vector of the wrong length; the whole
    /// batch is rejected and no result is accepted.
    #[error("batch result length: expected={expected:?}, actual={actual:?}")]
    WrongLength { expected: usize, actual: usize },
    /// Cancellation was observed before a chunk started; all previously
    /// accepted chunk results are discarded with this failure.
    #[error("batch invocation cancelled")]
    Cancelled,
    /// The chunk size supplied by the caller was zero.
    #[error("batch chunk size is zero")]
    InvalidChunkSize,
}

fn scalar_failure<E>(
    identity: &ProjectFunctionIdentity,
    row: Option<usize>,
    kind: FailureKind<E>,
) -> BatchFailure<E> {
    BatchFailure {
        identity: identity.clone(),
        row,
        kind: BatchFailureKind::Scalar(kind),
    }
}

/// Validate one result against the scalar result contract.
fn check_result<E>(signature: &LogicalSignature, returned: &Value) -> Result<(), FailureKind<E>> {
    let actual = match returned.value_type() {
        None if signature.may_return_missing() => return Ok(()),
        None => return Err(FailureKind::UndeclaredMissing),
        Some(ValueType::Bool) => return Err(FailureKind::BooleanResult),
        Some(actual) => actual,
    };
    let expected = match signature.returns() {
        ColumnType::Str => ValueType::Str,
        ColumnType::Int => ValueType::Int,
        ColumnType::Float => ValueType::Float,
        ColumnType::Date => ValueType::Date,
        ColumnType::DateTime => ValueType::DateTime,
    };
    if actual != expected {
        return Err(FailureKind::ResultType {
            expected: signature.returns(),
            actual,
        });
    }
    Ok(())
}

/// Callback columns plus the original indices of rows actually given to
/// the callback and the short-circuit flag of every original row.
struct PreparedBatch {
    arguments: Vec<BatchArgument>,
    active: Vec<usize>,
    short_circuited: Vec<bool>,
}

/// Prepare callback columns for one batch of rows, mirroring scalar
/// argument checking row by row in original order.
fn prepare<E>(
    plan: &ProjectInvocationPlan,
    rows: &[BTreeMap<String, Value>],
) -> Result<PreparedBatch, BatchFailure<E>> {
    let signature = plan.signature();
    let mut columns: Vec<Vec<Value>> = signature
        .parameters()
        .iter()
        .map(|_| Vec::with_capacity(rows.len()))
        .collect();
    let mut active: Vec<usize> = Vec::with_capacity(rows.len());
    let mut short_circuited = vec![false; rows.len()];
    for (row, supplied) in rows.iter().enumerate() {
        let unknown: Vec<_> = supplied
            .keys()
            .filter(|name| !signature.parameters().iter().any(|p| &p.name == *name))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(scalar_failure(
                plan.identity(),
                Some(row),
                FailureKind::UnknownArguments(unknown),
            ));
        }
        let mut values: Vec<Value> = Vec::with_capacity(signature.parameters().len());
        let mut skipped = false;
        for parameter in signature.parameters() {
            let value = match supplied.get(&parameter.name) {
                Some(value) => value.clone(),
                None => match &parameter.presence {
                    Presence::Required => {
                        return Err(scalar_failure(
                            plan.identity(),
                            Some(row),
                            FailureKind::MissingRequired {
                                parameter: parameter.name.clone(),
                            },
                        ));
                    }
                    Presence::Optional(default) => default.clone(),
                },
            };
            if let Some(actual) = value.value_type() {
                if actual != parameter.kind {
                    return Err(scalar_failure(
                        plan.identity(),
                        Some(row),
                        FailureKind::ArgumentType {
                            parameter: parameter.name.clone(),
                            expected: parameter.kind,
                            actual,
                        },
                    ));
                }
            } else if !parameter.accepts_missing {
                skipped = true;
                break;
            }
            values.push(value);
        }
        if skipped {
            short_circuited[row] = true;
        } else {
            active.push(row);
            for (column, value) in columns.iter_mut().zip(values) {
                column.push(value);
            }
        }
    }
    let arguments = signature
        .parameters()
        .iter()
        .zip(columns)
        .map(|(parameter, values)| BatchArgument {
            name: parameter.host_name.clone(),
            values,
        })
        .collect();
    Ok(PreparedBatch {
        arguments,
        active,
        short_circuited,
    })
}

/// Invoke one batch. See the module contract for the exact semantics.
pub fn invoke_batch<P: BatchFunctionPort>(
    plan: &ProjectInvocationPlan,
    handle: &P::Handle,
    rows: &[BTreeMap<String, Value>],
    port: &mut P,
) -> Result<Vec<Value>, BatchFailure<P::Error>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let PreparedBatch {
        arguments,
        active,
        short_circuited,
    } = prepare(plan, rows)?;
    let mut results = vec![Value::Missing; rows.len()];
    if active.is_empty() {
        return Ok(results);
    }
    let returned = port.call_batch(handle, &arguments).map_err(|error| {
        scalar_failure(
            plan.identity(),
            None,
            match error {
                HostError::Raised(payload) => FailureKind::CallFailed(payload),
                HostError::InvalidResult(payload) => FailureKind::InvalidHostResult(payload),
            },
        )
    })?;
    if returned.len() != active.len() {
        return Err(BatchFailure {
            identity: plan.identity().clone(),
            row: None,
            kind: BatchFailureKind::WrongLength {
                expected: active.len(),
                actual: returned.len(),
            },
        });
    }
    for (position, value) in returned.into_iter().enumerate() {
        let row = active[position];
        check_result(plan.signature(), &value)
            .map_err(|kind| scalar_failure(plan.identity(), Some(row), kind))?;
        results[row] = value;
    }
    debug_assert!(results
        .iter()
        .zip(&short_circuited)
        .all(|(value, skipped)| !skipped || *value == Value::Missing));
    Ok(results)
}

/// Invoke rows in consecutive chunks of `chunk_size`, checking
/// `is_cancelled` before every chunk (including the first). Cancellation
/// discards all previously accepted chunks: the caller receives either the
/// complete scattered result or one failure, never partial output. An
/// interrupt or exception inside a chunk likewise aborts the whole run.
pub fn invoke_batch_chunked<P: BatchFunctionPort>(
    plan: &ProjectInvocationPlan,
    handle: &P::Handle,
    rows: &[BTreeMap<String, Value>],
    chunk_size: usize,
    port: &mut P,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Value>, BatchFailure<P::Error>> {
    if chunk_size == 0 {
        return Err(BatchFailure {
            identity: plan.identity().clone(),
            row: None,
            kind: BatchFailureKind::InvalidChunkSize,
        });
    }
    let mut results = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(chunk_size) {
        if is_cancelled() {
            return Err(BatchFailure {
                identity: plan.identity().clone(),
                row: None,
                kind: BatchFailureKind::Cancelled,
            });
        }
        let offset = results.len();
        let mut part = invoke_batch(plan, handle, chunk, port).map_err(|mut failure| {
            if let Some(row) = failure.row {
                failure.row = Some(row + offset);
            }
            failure
        })?;
        results.append(&mut part);
    }
    Ok(results)
}
