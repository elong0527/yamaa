//! The progress callback of one call (#3762), on the calling R thread.
//!
//! The R layer wraps the user's function, so it replies with a closed
//! envelope, `list(status, payload)`, and no R error longjmps across Rust:
//! status 0 when the function returned, 1 when it signalled an error, whose
//! condition and raw UTF-8 class and message form the payload, and 3 for
//! an interrupt, which cancels the call
//! as a project function's interrupt does. Each event reaches the function
//! as one named list in the order of `records.progress_event`: `name`,
//! `phase`, `kind`, `outcome`, `number` and `rows`, the last three `NULL`
//! where the event has none. An event on another thread, such as a batch a
//! library pulls there, is not reported.
use extendr_api::prelude::*;
use std::sync::Arc;
use yamaa::host::functions::{FunctionError, HostPayload};
use yamaa::ProgressEvent;

use crate::thread_bound::ThreadBound;

#[derive(Clone)]
pub(crate) struct Progress {
    function: Arc<ThreadBound<Function>>,
}

impl Progress {
    /// The wrapped callback `value`, or none for `NULL`.
    pub(crate) fn of(value: &Robj) -> std::result::Result<Option<Self>, &'static str> {
        if value.is_null() {
            return Ok(None);
        }
        let function = value
            .as_function()
            .ok_or("progress must be a function or NULL")?;
        Ok(Some(Self {
            function: Arc::new(ThreadBound::new(function)),
        }))
    }

    /// Report `event` to the function.
    pub(crate) fn call(&self, event: &ProgressEvent) -> std::result::Result<(), FunctionError> {
        let Some(function) = self.function.get() else {
            return Ok(());
        };
        let count = |value: Option<usize>| value.map_or_else(|| NULL.into_robj(), |n| r!(n as f64));
        let outcome = event
            .outcome
            .map_or_else(|| NULL.into_robj(), |outcome| r!(outcome.as_str()));
        let fields = list!(
            name = event.name.as_str(),
            phase = event.phase.as_str(),
            kind = event.kind.as_str(),
            outcome = outcome,
            number = count(event.number),
            rows = count(event.rows)
        );
        let reply =
            function
                .call(pairlist!(event = fields))
                .map_err(|_| FunctionError::Internal {
                    code: "progress_boundary",
                })?;
        let reply = reply
            .as_list()
            .filter(|reply| reply.len() == 2)
            .ok_or(FunctionError::Protocol)?;
        let status = reply
            .elt(0)
            .ok()
            .and_then(|status| status.as_integer())
            .ok_or(FunctionError::Protocol)?;
        match status {
            0 => Ok(()),
            1 => {
                let payload = reply
                    .elt(1)
                    .ok()
                    .and_then(|payload| payload.as_list())
                    .filter(|payload| payload.len() == 3)
                    .ok_or(FunctionError::Protocol)?;
                let condition = payload.elt(0).map_err(|_| FunctionError::Protocol)?;
                let class = crate::function_callback::detail(
                    &payload.elt(1).map_err(|_| FunctionError::Protocol)?,
                    "R condition",
                )
                .map_err(|_| FunctionError::Protocol)?;
                let message = crate::function_callback::detail(
                    &payload.elt(2).map_err(|_| FunctionError::Protocol)?,
                    "unavailable R condition message",
                )
                .map_err(|_| FunctionError::Protocol)?;
                Err(FunctionError::Exception {
                    class,
                    message,
                    payload: Some(HostPayload::new(ThreadBound::new(condition))),
                })
            }
            3 => {
                if let Ok(condition) = reply.elt(1) {
                    crate::interrupt::remember(condition);
                }
                Err(FunctionError::Interrupted { payload: None })
            }
            _ => Err(FunctionError::Protocol),
        }
    }
}
