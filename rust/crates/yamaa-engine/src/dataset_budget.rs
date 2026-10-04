//! Run-local cumulative accounting for installed dataset boundary policies.
use crate::dataset::{ExecutionError, Limits, Resource};
use alloc::boxed::Box;
use yamaa_core::{predicate, value::Value};

/// These counters bound retained data and logical work, not allocator overhead or CPU time.
pub(crate) struct Budget {
    limits: Limits,
    output_text: usize,
    identity_cells: usize,
    identity_text: usize,
    runtime: predicate::Budget,
}
impl Budget {
    /// Start a fresh accounting scope; reuse of a plan never reuses counters.
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            output_text: 0,
            identity_cells: 0,
            identity_text: 0,
            runtime: predicate::Budget::new(predicate::Usage {
                work: limits.work_cells,
                resolutions: limits.work_cells,
                text_bytes: limits.scalar_text_bytes,
                like_work: limits.work_cells,
            }),
        }
    }

    /// Charge logical table visits before reads, folds or verification scans begin.
    pub(crate) fn work<E>(
        &mut self,
        rows: usize,
        columns: usize,
    ) -> Result<(), Box<ExecutionError<E>>> {
        let amount = rows.checked_mul(columns);
        let required = amount.and_then(|amount| self.runtime.used().work.checked_add(amount));
        if let Some(amount) = amount {
            if self.runtime.work(amount).is_ok() {
                return Ok(());
            }
        }
        Err(Box::new(ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: self.limits.work_cells,
            required,
        }))
    }

    /// Bound repeated string cloning/parsing even when conversion yields tiny values.
    pub(crate) fn scalar_text<E>(&mut self, bytes: usize) -> Result<(), Box<ExecutionError<E>>> {
        let required = self.runtime.used().text_bytes.checked_add(bytes);
        self.runtime.text(bytes).map_err(|_| {
            Box::new(ExecutionError::Limit {
                resource: Resource::ScalarTextBytes,
                limit: self.limits.scalar_text_bytes,
                required,
            })
        })
    }

    /// Predicates share cumulative work/text with ordinary dataset evaluation.
    pub(crate) fn predicate(&mut self) -> &mut predicate::Budget {
        &mut self.runtime
    }

    /// A filtered candidate releases retained text, but never refunds evaluated work.
    pub(crate) fn discard_candidate(&mut self, values: &[Value]) {
        let bytes: usize = values.iter().map(text_bytes).sum();
        self.output_text -= bytes;
    }

    /// Each admitted assignment fills a new slot, so retained text grows monotonically.
    pub(crate) fn value<E>(&mut self, value: &Value) -> Result<(), Box<ExecutionError<E>>> {
        charge(
            &mut self.output_text,
            Some(text_bytes(value)),
            self.limits.output_text_bytes,
            Resource::OutputTextBytes,
        )
    }

    /// Admit a complete identity before cloning any of its cells or text.
    pub(crate) fn identity<'a, E>(
        &mut self,
        values: impl Iterator<Item = &'a Value>,
    ) -> Result<(), Box<ExecutionError<E>>> {
        let mut cells = Some(0_usize);
        let mut text = Some(0_usize);
        for value in values {
            cells = cells.and_then(|cells| cells.checked_add(1));
            text = text.and_then(|text| text.checked_add(text_bytes(value)));
        }
        charge(
            &mut self.identity_cells,
            cells,
            self.limits.identity_cells,
            Resource::IdentityCells,
        )?;
        charge(
            &mut self.identity_text,
            text,
            self.limits.identity_text_bytes,
            Resource::IdentityTextBytes,
        )
    }
}

/// Runtime strings are UTF-8; other fixed-size value variants contribute no text payload.
fn text_bytes(value: &Value) -> usize {
    match value {
        Value::Str(text) => text.len(),
        _ => 0,
    }
}

/// Checked counters fail without wrapping, saturation or accepting partial data.
fn charge<E>(
    used: &mut usize,
    amount: Option<usize>,
    limit: usize,
    resource: Resource,
) -> Result<(), Box<ExecutionError<E>>> {
    let required = amount.and_then(|amount| used.checked_add(amount));
    match required {
        Some(required) if required <= limit => {
            *used = required;
            Ok(())
        }
        _ => Err(Box::new(ExecutionError::Limit {
            resource,
            limit,
            required,
        })),
    }
}
