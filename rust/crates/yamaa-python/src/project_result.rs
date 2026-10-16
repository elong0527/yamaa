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
    file_application::BuildRefusal,
    project_report::{self, Classified},
    specification_report::BuildResult,
};

pub struct Result {
    carrier: Attempt,
    formatted: std::result::Result<BuildResult, BuildRefusal>,
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
        project_report::build_result(
            &attempt.run,
            &state,
            report_fields::identity(&fields),
            &facts,
        )
        .map_err(|error| error.refusal())
    };
    Ok(Result {
        carrier: Attempt {
            inner: Arc::clone(&attempt.inner),
            run: Arc::clone(&attempt.run),
        },
        formatted,
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
            Ok(formatted) => {
                yamaa_adapters::file_application::Domain::from_project(formatted, project)
            }
            Err(refusal) => Err(refusal.issues()),
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
