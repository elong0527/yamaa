//! Private whole-graph qualification carriers. Retained evidence has no Save port.
use crate::project_activation::Error as HostError;
use pyo3::{
    exceptions::{PyException, PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyTuple},
};
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa_adapters::{
    file_producer_build::{FileBuild, Limits},
    file_workflow,
    function_transport::CallbackError,
    producer_report::{self, ProjectionError},
    project_function_diagnostics::HostDetails,
    project_report::Classified,
};

type NativeAttempt = producer_report::NativeAttempt<HostError>;
type Prepared = Result<FileBuild, file_workflow::Error>;
fn borrow_error() -> PyErr {
    PyRuntimeError::new_err("native producer handle is already borrowed or poisoned")
}
#[pyclass(frozen, module = "yamaa._native", name = "_ProducerSpecification")]
pub struct Specification {
    inner: Mutex<Prepared>,
}
impl Specification {
    fn get(&self) -> PyResult<MutexGuard<'_, Prepared>> {
        self.inner.try_lock().map_err(|_| borrow_error())
    }
}
#[pymethods]
impl Specification {
    fn preparation_status(&self) -> PyResult<&'static str> {
        Ok(match &*self.get()? {
            Ok(_) => "ready",
            Err(file_workflow::Error::Configuration(_)) => "configuration_failure",
            Err(file_workflow::Error::Schema(_)) => "schema_failure",
            Err(file_workflow::Error::Environment(_)) => "environment_failure",
            Err(file_workflow::Error::Graph(_)) => "graph_failure",
        })
    }
    #[pyo3(signature = (maximum=16_777_216))]
    fn rejection_issues(&self, maximum: usize) -> PyResult<String> {
        let state = self.get()?;
        let error = state
            .as_ref()
            .err()
            .ok_or_else(|| PyValueError::new_err("producer preparation has no rejection"))?;
        let issues = yamaa_adapters::producer_check::rejected_with_limit(
            error,
            yamaa_core::project_function::Language::Python,
            maximum,
        )
        .map_err(|_| {
            PyValueError::new_err(
                "rejection projection refused; original rejection remains retained",
            )
        })?;
        Ok(yamaa_adapters::issue_rows::encode(&issues))
    }
    #[pyo3(signature = (metadata, report_bytes=16_777_216))]
    fn build(
        &self,
        py: Python<'_>,
        metadata: &Bound<'_, PyTuple>,
        report_bytes: usize,
    ) -> PyResult<Attempt> {
        let fields = Arc::new(crate::specification_result::fields(metadata)?);
        let mut state = self.get()?;
        let build = state.as_mut().map_err(|_| {
            PyValueError::new_err(
                "producer preparation failed; original rejection remains retained",
            )
        })?;
        let provenance = Arc::clone(build.provenance());
        let mut port = crate::project_activation::Port::new(
            py,
            provenance
                .environment()
                .lock()
                .map_or(&[], |lock| &lock.source.bytes),
        );
        let mut limits = Limits::default();
        limits.engine.report_bytes = report_bytes;
        let inner = build.build_reported(
            &mut port,
            crate::specification_result::identity(&fields),
            limits,
        );
        Ok(Attempt {
            inner: Arc::new(Mutex::new(inner)),
            fields,
        })
    }
    /// Early rejections retain all original bytes even if no build can be entered.
    fn rejected_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        let state = self.get()?;
        let sources = match &*state {
            Err(file_workflow::Error::Environment(error)) => {
                error.retained_sources(65_536, 16_777_216).map_err(|_| {
                    PyValueError::new_err(
                        "source evidence limit; original rejection remains retained",
                    )
                })?
            }
            Err(file_workflow::Error::Graph(error)) => error
                .retained_sources(65_536, 16_777_216)
                .map_err(|_| {
                PyValueError::new_err("source evidence limit; original rejection remains retained")
            })?,
            _ => Vec::new(),
        };
        source_budget(
            sources
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_ref())),
        )?;
        Ok(sources
            .into_iter()
            .map(|(name, bytes)| (name, PyBytes::new(py, &bytes)))
            .collect())
    }
}
#[pyfunction]
pub fn _prepare_producer_file(specification: &str, environment: &str) -> Specification {
    Specification {
        inner: Mutex::new(
            file_workflow::prepare(
                specification,
                environment,
                yamaa_core::project_function::Language::Python,
            )
            .map(|graph| graph.into_build()),
        ),
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
    fn retained(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            fields: Arc::clone(&self.fields),
        }
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
    #[pyo3(signature = (maximum=16_777_216))]
    fn result(&self, py: Python<'_>, maximum: usize) -> PyResult<Report> {
        let state = self.get()?;
        let mut count = 0usize;
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, _| {
            count = count.saturating_add(1)
        });
        let mut remaining = maximum.min(16_777_216);
        let admitted = count <= 65_536
            && count
                .checked_mul(128)
                .and_then(|size| remaining.checked_sub(size))
                .is_some();
        let mut refusal = if admitted { "" } else { "report_limit" };
        let mut failures = Vec::new();
        let mut ordinary = Vec::new();
        if admitted {
            remaining -= count * 128;
            failures.reserve(count);
            ordinary.reserve(count);
            yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, error| {
                failures.push(error)
            });
            for error in &failures {
                let mut limited = false;
                let projection = match error {
                    HostError::Host(error) if error.is_instance_of::<PyException>(py) => {
                        ordinary_exception(py, error, &mut remaining, &mut limited)
                    }
                    _ => None,
                };
                if limited {
                    refusal = "report_limit";
                    break;
                }
                if let Some(value) = projection.as_ref().or(match error {
                    HostError::Result(value) => Some(value),
                    _ => None,
                }) {
                    if let Some(details) = details(value) {
                        let fields = match details {
                            HostDetails::Exception { class, message, .. } => {
                                [Some(class), Some(message)]
                            }
                            HostDetails::Rejected { reason, returned } => [Some(reason), returned],
                        };
                        for text in fields.into_iter().flatten() {
                            let charge = text
                                .len()
                                .checked_mul(12)
                                .and_then(|size| size.checked_add(128));
                            if let Some(left) = charge.and_then(|size| remaining.checked_sub(size))
                            {
                                remaining = left;
                            } else {
                                refusal = "report_limit";
                            }
                        }
                    }
                }
                ordinary.push(projection);
                if !refusal.is_empty() {
                    break;
                }
            }
        }
        let mut observations = None;
        if refusal.is_empty() {
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
            match producer_report::attempt_report_classified(
                &state,
                crate::specification_result::identity(&self.fields),
                maximum,
                &facts,
            ) {
                Ok(value) => observations = Some(value.to_string()),
                Err(ProjectionError::Original {
                    observations: value,
                    ..
                }) => {
                    observations = Some(value.to_string());
                    refusal = "original_boundary";
                }
                Err(ProjectionError::Report(
                    yamaa_adapters::specification_report::Error::OutputLimit,
                )) => refusal = "report_limit",
                Err(_) => refusal = "report_refusal",
            }
        }
        Ok(Report {
            carrier: self.retained(),
            observations,
            refusal,
        })
    }
    fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.get()?;
        let mut count = 0usize;
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, error| {
            if matches!(error, HostError::Host(_)) {
                count = count.saturating_add(1);
            }
        });
        if count > 65536 {
            return Err(PyValueError::new_err(
                "host failure evidence limit; original attempt remains retained",
            ));
        }
        let mut failures = Vec::with_capacity(count);
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |stage, error| {
            if let HostError::Host(error) = error {
                failures.push((stage, error.value(py).clone().into_any().unbind()));
            }
        });
        Ok(failures)
    }
    fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
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
        let sources = match &state.resources {
            Ok(sources) => sources,
            Err(error) => &error.retained,
        };
        source_budget(
            sources
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_ref())),
        )?;
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

#[pyclass(frozen, module = "yamaa._native", name = "_ProducerReport")]
pub struct Report {
    carrier: Attempt,
    observations: Option<String>,
    refusal: &'static str,
}
#[pymethods]
impl Report {
    fn report_status(&self) -> &'static str {
        if self.refusal.is_empty() {
            "complete"
        } else {
            self.refusal
        }
    }
    fn observations(&self) -> PyResult<String> {
        self.observations.as_ref().cloned().ok_or_else(|| {
            PyValueError::new_err("report refused; original attempt remains retained")
        })
    }
    fn attempt(&self) -> Attempt {
        self.carrier.retained()
    }
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
fn ordinary_exception(
    py: Python<'_>,
    error: &PyErr,
    remaining: &mut usize,
    limited: &mut bool,
) -> Option<CallbackError> {
    let class = error.get_type(py).name().ok()?;
    let class = class.to_str().ok()?;
    let message = error.value(py).str().ok()?;
    let message = message.to_str().ok()?;
    if [class, message]
        .iter()
        .any(|text| text.len() > yamaa_adapters::function_transport::MAX_ERROR_BYTES)
    {
        *limited = true;
        return None;
    }
    // Admit both borrowed host texts before constructing native owned copies.
    for text in [class, message] {
        let next = text
            .len()
            .checked_mul(12)
            .and_then(|size| size.checked_add(128))
            .and_then(|size| remaining.checked_sub(size));
        let Some(next) = next else {
            *limited = true;
            return None;
        };
        *remaining = next;
    }
    Some(CallbackError::Exception {
        class: class.into(),
        message: message.into(),
        truncated: false,
    })
}
fn source_budget<'a>(sources: impl Iterator<Item = (&'a str, &'a [u8])>) -> PyResult<()> {
    let mut text = 16_777_216usize;
    let mut bytes = 67_108_864usize;
    let mut count = 65_536usize;
    for (name, source) in sources {
        count = count
            .checked_sub(1)
            .ok_or_else(|| PyValueError::new_err("source evidence limit"))?;
        text = text
            .checked_sub(name.len())
            .ok_or_else(|| PyValueError::new_err("source evidence limit"))?;
        bytes = bytes
            .checked_sub(source.len())
            .ok_or_else(|| PyValueError::new_err("source evidence limit"))?;
    }
    Ok(())
}
