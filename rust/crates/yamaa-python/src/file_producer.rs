//! Whole-graph carriers retain actual build evidence and ordered Save authority.
use crate::project_activation::Error as HostError;
use pyo3::{
    exceptions::{PyException, PyRuntimeError},
    prelude::*,
    types::PyBytes,
};
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa_adapters::{
    callback::details, file_producer_build::FileBuild, producer_report::ProjectionError,
    project_report::Classified,
};

type NativeAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostError>;
fn borrow_error() -> PyErr {
    PyRuntimeError::new_err("native producer handle is already borrowed or poisoned")
}
/// Transfer the selected whole build directly; preparation is never repeated.
pub(crate) fn build_selected_with_observer(
    py: Python<'_>,
    build: &mut FileBuild,
    fields: Arc<Vec<String>>,
    scope: yamaa_core::application_observer::Scope<'_>,
) -> Attempt {
    let provenance = Arc::clone(build.provenance());
    let mut port = crate::project_activation::Port::new(
        py,
        provenance
            .environment()
            .lock()
            .map_or(&[], |lock| &lock.source.bytes),
    );
    let inner =
        build.build_owned_reported(&mut port, crate::report_fields::identity(&fields), scope);
    Attempt {
        inner: Arc::new(Mutex::new(inner)),
        fields,
    }
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProducerAttempt")]
pub struct Attempt {
    inner: Arc<Mutex<NativeAttempt>>,
    fields: Arc<Vec<String>>,
}
impl Attempt {
    fn get(&self) -> PyResult<MutexGuard<'_, NativeAttempt>> {
        self.inner.try_lock().map_err(|_| borrow_error())
    }
    pub(crate) fn retained(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            fields: Arc::clone(&self.fields),
        }
    }
    pub(crate) fn formatted(&self, py: Python<'_>) -> PyResult<Report> {
        let state = self.get()?;
        let mut failures = Vec::new();
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, error| {
            failures.push(error)
        });
        let ordinary = failures
            .iter()
            .map(|error| match error {
                HostError::Host(error) if error.is_instance_of::<PyException>(py) => {
                    Some(crate::function_callback::exception(py, error.clone_ref(py)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut refusal = "";
        let mut refusal_details = None;
        let mut observations = None;
        let mut domain = None;
        let mut output_error = None;
        {
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
            let projected = match yamaa_adapters::producer_result::prepare_owned_classified(
                &state,
                crate::report_fields::identity(&self.fields),
                &facts,
            ) {
                Ok(value) => {
                    domain = Some(Mutex::new(value));
                    Ok(())
                }
                Err(yamaa_adapters::producer_result::Error::Report(error)) => Err(error),
                Err(yamaa_adapters::producer_result::Error::Output {
                    observations: value,
                    error,
                    ..
                }) => {
                    observations = Some(value.to_string());
                    if let yamaa_adapters::ipc_table::TableError::Limit {
                        resource,
                        limit,
                        required,
                    } = error
                    {
                        refusal_details = Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                            resource,
                            limit,
                            required,
                            vec!["output.path".into()],
                        ));
                    }
                    output_error = Some(error);
                    refusal = "output_projection";
                    Ok(())
                }
            };
            match projected {
                Ok(()) => {}
                Err(ProjectionError::Original {
                    observations: value,
                    ..
                }) => {
                    observations = Some(value.to_string());
                    refusal = "original_boundary";
                }
                Err(ProjectionError::Report(
                    yamaa_adapters::specification_report::Error::OutputLimit,
                )) => refusal = "output_limit",
                Err(ProjectionError::Report(
                    yamaa_adapters::specification_report::Error::ResourceLimit(details),
                )) => {
                    refusal = "output_limit";
                    refusal_details = Some(details);
                }
                Err(_) => refusal = "report_refusal",
            }
        }
        Ok(Report {
            carrier: self.retained(),
            observations,
            refusal,
            refusal_details,
            domain,
            output_error,
        })
    }
}
#[pymethods]
impl Attempt {
    fn accepted(&self) -> PyResult<bool> {
        Ok(self.get()?.accepted())
    }
    fn entered_nodes(&self) -> PyResult<Vec<usize>> {
        let state = self.get()?;
        Ok(state.graph.nodes.iter().map(|node| node.node).collect())
    }
    pub(crate) fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.get()?;
        let mut failures = Vec::new();
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |stage, error| {
            if let HostError::Host(error) = error {
                failures.push((stage, error.value(py).clone().into_any().unbind()));
            }
        });
        Ok(failures)
    }
    pub(crate) fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        let state = self.get()?;
        let mut interrupt = None;
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, error| {
            if let HostError::Host(error) = error {
                if !error.is_instance_of::<PyException>(py) && interrupt.is_none() {
                    interrupt = Some(error.clone_ref(py));
                }
            }
        });
        interrupt.map_or(Ok(()), Err)
    }
    fn retained_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        let state = self.get()?;
        let sources = &state.resources;
        Ok(sources
            .iter()
            .map(|(name, bytes)| (name.clone(), PyBytes::new(py, bytes)))
            .collect())
    }
    /// Artifact bytes are evidence, including earlier accepted producer evidence
    /// after a later failure. This class exposes no publisher or Save method.
    fn artifact<'py>(&self, py: Python<'py>, node: usize) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let state = self.get()?;
        let output = state
            .graph
            .nodes
            .iter()
            .find(|entered| entered.node == node)
            .and_then(|entered| entered.output.as_ref().ok())
            .and_then(Option::as_ref);
        Ok(output
            .and_then(|output| output.artifact())
            .map(|artifact| PyBytes::new(py, artifact.bytes())))
    }
}

pub struct Report {
    carrier: Attempt,
    observations: Option<String>,
    refusal: &'static str,
    refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
    domain: Option<Mutex<yamaa_adapters::producer_result::RootResult>>,
    output_error: Option<yamaa_adapters::ipc_table::TableError>,
}
impl Report {
    pub(crate) fn domain_attempt(&self) -> Attempt {
        self.carrier.retained()
    }
    pub(crate) fn domain_output<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.domain
            .as_ref()
            .map(|result| {
                Ok(result
                    .try_lock()
                    .map_err(|_| borrow_error())?
                    .output()
                    .map(|bytes| PyBytes::new(py, bytes)))
            })
            .transpose()
            .map(Option::flatten)
    }
    pub(crate) fn domain_projection_failure(&self) -> Option<String> {
        self.output_error.map(|error| error.to_string())
    }
    pub(crate) fn domain_log<'py>(
        &self,
        py: Python<'py>,
        warning: bool,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        self.domain
            .as_ref()
            .map(|result| {
                let result = result.try_lock().map_err(|_| borrow_error())?;
                Ok((if warning {
                    result.warning_log()
                } else {
                    result.verification_log()
                })
                .map(|bytes| PyBytes::new(py, bytes)))
            })
            .transpose()
            .map(Option::flatten)
    }
    pub(crate) fn domain_issues(&self) -> PyResult<Vec<yamaa_adapters::issue_rows::Issue>> {
        self.domain
            .as_ref()
            .map(|result| {
                Ok(result
                    .try_lock()
                    .map_err(|_| borrow_error())?
                    .issues()
                    .to_vec())
            })
            .unwrap_or_else(|| {
                Ok(self.refusal_details.as_ref().map_or_else(
                    || yamaa_adapters::file_application::rejected("build", self.refusal),
                    |details| details.issues("build", self.refusal),
                ))
            })
    }
    pub(crate) fn domain_accepted(&self) -> bool {
        self.domain.as_ref().is_some_and(|result| {
            result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .output()
                .is_some()
        })
    }
    pub(crate) fn domain_observations(&self) -> PyResult<Option<String>> {
        self.domain
            .as_ref()
            .map(|result| {
                Ok(Some(
                    result
                        .try_lock()
                        .map_err(|_| borrow_error())?
                        .observations_json(),
                ))
            })
            .unwrap_or_else(|| Ok(self.observations.clone()))
    }
    pub(crate) fn domain_save(&self) -> PyResult<bool> {
        let _owner = self.carrier.get()?;
        let result = self.domain.as_ref().ok_or_else(|| {
            crate::domain_entry::DomainError::new_err("cannot save a failed build")
        })?;
        result
            .try_lock()
            .map_err(|_| borrow_error())?
            .save()
            .map_err(|_| crate::domain_entry::DomainError::new_err("cannot save a failed build"))
    }
    pub(crate) fn domain_failures(
        &self,
        py: Python<'_>,
    ) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        self.carrier.host_failures(py)
    }
    pub(crate) fn propagate_domain_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        self.carrier.propagate_interrupt(py)
    }
}
