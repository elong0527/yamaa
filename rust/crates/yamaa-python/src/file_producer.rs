//! Whole-graph carriers retain actual build evidence and ordered Save authority.
use crate::project_activation::Error as HostError;
use pyo3::{
    exceptions::{PyException, PyRuntimeError},
    prelude::*,
    types::PyBytes,
};
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa_adapters::{
    callback::details, file_application::BuildRefusal, file_producer_build::FileBuild,
    producer_result::RootResult, project_report::Classified,
};

type NativeAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostError>;
fn borrow_error() -> PyErr {
    PyRuntimeError::new_err("native producer handle is already borrowed or poisoned")
}
/// Transfer the selected whole build directly; preparation is never repeated.
pub(crate) fn build_selected(
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
        let projection = yamaa_adapters::producer_result::project_owned(
            &state,
            crate::report_fields::identity(&self.fields),
            &facts,
        );
        Ok(Report {
            carrier: self.retained(),
            observations: projection.observations,
            domain: projection.result.map(Mutex::new),
            output_error: projection.output_error,
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
    domain: Result<Mutex<RootResult>, BuildRefusal>,
    output_error: Option<yamaa_adapters::ipc_table::TableError>,
}
impl Report {
    pub(crate) fn domain_attempt(&self) -> Attempt {
        self.carrier.retained()
    }
    pub(crate) fn domain_output(&self) -> PyResult<Option<crate::arrow_bridge::ArrowTable>> {
        self.domain
            .as_ref()
            .ok()
            .map(|result| {
                Ok(result
                    .try_lock()
                    .map_err(|_| borrow_error())?
                    .output_table()
                    .map(crate::arrow_bridge::ArrowTable::from))
            })
            .transpose()
            .map(Option::flatten)
    }
    pub(crate) fn domain_projection_failure(&self) -> Option<String> {
        self.output_error.map(|error| error.to_string())
    }
    pub(crate) fn domain_log(
        &self,
        warning: bool,
    ) -> PyResult<Option<crate::arrow_bridge::ArrowTable>> {
        self.domain
            .as_ref()
            .ok()
            .map(|result| {
                Ok(result
                    .try_lock()
                    .map_err(|_| borrow_error())?
                    .log_table(warning)
                    .map(crate::arrow_bridge::ArrowTable::from))
            })
            .transpose()
            .map(Option::flatten)
    }
    pub(crate) fn domain_issues(&self) -> PyResult<Vec<yamaa_adapters::issue_rows::Issue>> {
        match &self.domain {
            Ok(result) => Ok(result
                .try_lock()
                .map_err(|_| borrow_error())?
                .issues()
                .to_vec()),
            Err(refusal) => Ok(refusal.issues()),
        }
    }
    pub(crate) fn domain_accepted(&self) -> bool {
        self.domain.as_ref().is_ok_and(|result| {
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
            .ok()
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
        let result = self
            .domain
            .as_ref()
            .map_err(|_| crate::domain_entry::DomainError::new_err("cannot save a failed build"))?;
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
