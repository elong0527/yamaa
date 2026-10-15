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
    callback::details,
    project_report::{self, Classified},
};

pub struct Result {
    carrier: Attempt,
    formatted: Option<yamaa_adapters::specification_report::BuildResult>,
    refusal: &'static str,
    refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
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
        ordinary.push(projection);
    }
    let formatted = {
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
                        yamaa_adapters::specification_report::Error::OutputLimit
                        | yamaa_adapters::specification_report::Error::ResourceLimit(_),
                    ) => "output_limit",
                    project_report::Error::Report(_) => "report_refusal",
                };
                None
            }
        }
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
