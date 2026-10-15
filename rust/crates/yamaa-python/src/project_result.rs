//! Retain the original whole attempt beside its complete public-result candidate.
//! Formatting refusal cannot discard original exceptions, interrupts or sources.
use crate::{file_project::Attempt, project_activation::Error as HostError, report_fields};
use pyo3::{
    exceptions::{PyException, PyRuntimeError},
    prelude::*,
    types::PyTuple,
};
use std::sync::Arc;
use yamaa_adapters::{
    callback::CallbackError,
    project_function_diagnostics::HostDetails,
    project_report::{self, Classified},
};

pub struct Result {
    carrier: Attempt,
    formatted: Option<yamaa_adapters::specification_report::BuildResult>,
    refusal: &'static str,
    refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
}
fn details(error: &CallbackError) -> Option<HostDetails<'_>> {
    match error {
        CallbackError::Exception {
            class,
            message,
            truncated,
        } => Some(HostDetails::Exception {
            class,
            message,
            truncated: *truncated,
        }),
        CallbackError::Rejected { reason, returned } => Some(HostDetails::Rejected {
            reason,
            returned: returned.as_deref(),
        }),
        CallbackError::Boundary(_) => None,
    }
}
pub(super) fn prepare(
    py: Python<'_>,
    attempt: &Attempt,
    metadata: &Bound<'_, PyTuple>,
) -> PyResult<Result> {
    let fields = report_fields::fields(metadata)?;
    let state = attempt.inner.try_lock().map_err(|_| {
        PyRuntimeError::new_err("native project attempt is already borrowed or poisoned")
    })?;
    let mut failures = Vec::new();
    yamaa_adapters::project_attempt::visit_host_failures(&state, |_, error| failures.push(error));
    let mut budget = yamaa_adapters::specification_check::MAX_ISSUE_BYTES;
    let mut ordinary = Vec::new();
    let mut refusal = "";
    let mut refusal_details = None;
    for error in &failures {
        let projection = match error {
            HostError::Host(error) if error.is_instance_of::<PyException>(py) => {
                Some(crate::function_callback::exception(py, error.clone_ref(py)))
            }
            _ => None,
        };
        let value = projection.as_ref().or(match error {
            HostError::Result(value) => Some(value),
            _ => None,
        });
        if let Some(facts) = value.and_then(details) {
            let texts = match facts {
                HostDetails::Exception { class, message, .. } => [Some(class), Some(message)],
                HostDetails::Rejected { reason, returned } => [Some(reason), returned],
            };
            for text in texts.into_iter().flatten() {
                let remaining = text
                    .len()
                    .checked_mul(12)
                    .and_then(|size| size.checked_add(128))
                    .and_then(|charge| budget.checked_sub(charge));
                let Some(remaining) = remaining else {
                    refusal = "report_limit";
                    refusal_details = Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                        "diagnostic_bytes",
                        yamaa_adapters::specification_check::MAX_ISSUE_BYTES,
                        text.len()
                            .checked_mul(12)
                            .and_then(|n| n.checked_add(128))
                            .and_then(|charge| {
                                (yamaa_adapters::specification_check::MAX_ISSUE_BYTES - budget)
                                    .checked_add(charge)
                            }),
                        Vec::new(),
                    ));
                    break;
                };
                budget = remaining;
            }
        }
        ordinary.push(projection);
        if !refusal.is_empty() {
            break;
        }
    }
    let formatted = if refusal.is_empty() {
        let facts = failures
            .iter()
            .zip(&ordinary)
            .filter_map(|(&error, ordinary)| match error {
                HostError::Lock(findings) => Some(Classified::Lock {
                    failure: error,
                    findings,
                }),
                HostError::Result(value) => details(value).map(|details| Classified::Host {
                    failure: error,
                    details,
                }),
                _ => ordinary
                    .as_ref()
                    .and_then(details)
                    .map(|details| Classified::Host {
                        failure: error,
                        details,
                    }),
            })
            .collect::<Vec<_>>();
        match project_report::build_result(
            &attempt.run,
            &state,
            report_fields::identity(&fields),
            &facts,
        ) {
            Ok(inner) => Some(inner),
            Err(error) => {
                if let project_report::Error::Report(
                    yamaa_adapters::specification_report::Error::ResourceLimit(details),
                ) = &error
                {
                    refusal_details = Some(details.clone());
                }
                refusal = match error {
                    project_report::Error::Opaque(_) => "opaque_host",
                    project_report::Error::Boundary(_) => "activation_boundary",
                    project_report::Error::Source(_) => "source_boundary",
                    project_report::Error::Execution(_) => "execution_boundary",
                    project_report::Error::Report(
                        yamaa_adapters::specification_report::Error::OutputLimit,
                    ) => "report_limit",
                    project_report::Error::Report(
                        yamaa_adapters::specification_report::Error::ResourceLimit(_),
                    ) => "report_limit",
                    project_report::Error::Report(_) => "report_refusal",
                };
                None
            }
        }
    } else {
        None
    };
    Ok(Result {
        carrier: Attempt {
            inner: Arc::clone(&attempt.inner),
            run: Arc::clone(&attempt.run),
        },
        formatted,
        refusal,
        refusal_details,
    })
}
impl Result {
    pub(super) fn into_application(
        self,
        project: &yamaa_adapters::file_project::FileProject,
    ) -> (
        Attempt,
        std::result::Result<
            yamaa_adapters::file_application::Domain,
            Vec<yamaa_adapters::issue_rows::Issue>,
        >,
    ) {
        let result = match self.formatted {
            Some(formatted) => {
                yamaa_adapters::file_application::Domain::from_project(formatted, project)
            }
            None => Err(self.refusal_details.map_or_else(
                || yamaa_adapters::file_application::rejected("build", self.refusal),
                |details| details.issues("build", self.refusal),
            )),
        };
        (self.carrier, result)
    }
    /// Preserve the original control-flow payload even after a refused formatter.
    pub(super) fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        let state = self.carrier.inner.try_lock().map_err(|_| {
            PyRuntimeError::new_err("native project attempt is already borrowed or poisoned")
        })?;
        let mut interrupt = None;
        yamaa_adapters::project_attempt::visit_host_failures(&state, |_, error| {
            if let HostError::Host(error) = error {
                if !error.is_instance_of::<PyException>(py) && interrupt.is_none() {
                    interrupt = Some(error.clone_ref(py));
                }
            }
        });
        match interrupt {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
