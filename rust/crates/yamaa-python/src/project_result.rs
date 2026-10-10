//! Retain the original whole attempt beside its complete public-result candidate.
//! Formatting refusal cannot discard original exceptions, interrupts or sources.
use crate::{file_project::Attempt, project_activation::Error as HostError, specification_result};
use pyo3::{
    exceptions::{PyException, PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};
use std::sync::Arc;
use yamaa_adapters::{
    function_transport::CallbackError,
    project_function_diagnostics::HostDetails,
    project_report::{self, Classified},
};

#[pyclass(frozen, module = "yamaa._native", name = "_ProjectResult")]
pub struct Result {
    carrier: Attempt,
    formatted: Option<specification_result::BuildResult>,
    refusal: &'static str,
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
    let fields = specification_result::fields(metadata)?;
    let state = attempt.inner.try_lock().map_err(|_| {
        PyRuntimeError::new_err("native project attempt is already borrowed or poisoned")
    })?;
    let mut failures = Vec::new();
    yamaa_adapters::project_attempt::visit_host_failures(&state, |_, error| failures.push(error));
    let mut budget = yamaa_adapters::specification_check::MAX_ISSUE_BYTES;
    let mut ordinary = Vec::new();
    let mut refusal = "";
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
            specification_result::identity(&fields),
            &facts,
        ) {
            Ok(inner) => Some(specification_result::BuildResult { inner }),
            Err(error) => {
                refusal = match error {
                    project_report::Error::Opaque(_) => "opaque_host",
                    project_report::Error::Boundary(_) => "activation_boundary",
                    project_report::Error::Source(_) => "source_boundary",
                    project_report::Error::Execution(_) => "execution_boundary",
                    project_report::Error::Report(
                        yamaa_adapters::specification_report::Error::OutputLimit,
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
                yamaa_adapters::file_application::Domain::from_project(formatted.inner, project)
            }
            None => Err(yamaa_adapters::file_application::rejected(
                "build",
                self.refusal,
            )),
        };
        (self.carrier, result)
    }
    fn report(&self) -> PyResult<&specification_result::BuildResult> {
        self.formatted.as_ref().ok_or_else(|| {
            PyValueError::new_err(
                "complete project report is unavailable; original attempt remains retained",
            )
        })
    }
}
#[pymethods]
impl Result {
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
    fn report_status(&self) -> &'static str {
        if self.formatted.is_some() {
            "complete"
        } else {
            self.refusal
        }
    }
    fn issues(&self) -> PyResult<Vec<specification_result::IssueRow>> {
        Ok(self.report()?.issues())
    }
    fn output<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        Ok(self.report()?.output(py))
    }
    fn observations(&self) -> PyResult<String> {
        Ok(self.report()?.observations())
    }
    fn save(&self, publish: &Bound<'_, PyAny>) -> PyResult<String> {
        self.report()?.save(publish)
    }
    fn save_file(&self, declared: &str, target: &str) -> PyResult<String> {
        self.report()?.save_file(declared, target)
    }
    fn activation(&self) -> PyResult<String> {
        self.carrier.activation()
    }
    fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        self.carrier.host_failures(py)
    }
    fn failure_facts<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(&'static str, Bound<'py, PyDict>)>> {
        self.carrier.failure_facts(py)
    }
    fn prepared_sources<'py>(&self, py: Python<'py>) -> Vec<(String, Bound<'py, PyBytes>)> {
        self.carrier.prepared_sources(py)
    }
    fn study_snapshot<'py>(
        &self,
        py: Python<'py>,
        index: usize,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.carrier.study_snapshot(py, index)
    }
}
