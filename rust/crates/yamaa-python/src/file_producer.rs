//! Whole-graph carriers retain actual build evidence and ordered Save authority.
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

type NativeAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostError>;
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
            Err(file_workflow::Error::Admission(_)) => "admission_failure",
            Err(file_workflow::Error::Environment(_)) => "environment_failure",
            Err(file_workflow::Error::Graph(_)) => "graph_failure",
        })
    }
    #[pyo3(signature = (maximum=yamaa_adapters::host_limits::REPLY_BYTES))]
    fn rejection_issues(&self, maximum: usize) -> PyResult<String> {
        let state = self.get()?;
        let error = state
            .as_ref()
            .err()
            .ok_or_else(|| PyValueError::new_err("producer preparation has no rejection"))?;
        let issues = yamaa_adapters::producer_check::rejected_with_limit(
            error,
            yamaa_engine::host_values::Language::Python,
            maximum,
        )
        .map_err(|_| {
            PyValueError::new_err(
                "rejection projection refused; original rejection remains retained",
            )
        })?;
        Ok(yamaa_adapters::issue_rows::encode(&issues))
    }
    #[pyo3(signature = (metadata, report_bytes=yamaa_adapters::host_limits::REPLY_BYTES))]
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
        Ok(build_selected(py, build, fields, report_bytes))
    }

    /// Early rejections retain all original bytes even if no build can be entered.
    fn rejected_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        let state = self.get()?;
        let sources = match &*state {
            Err(file_workflow::Error::Admission(error)) => error
                .retained_sources(
                    yamaa_adapters::host_limits::RECORDS,
                    yamaa_adapters::host_limits::REPLY_BYTES,
                )
                .map_err(|_| {
                    PyValueError::new_err(
                        "source evidence limit; original rejection remains retained",
                    )
                })?,
            Err(file_workflow::Error::Environment(error)) => error
                .retained_sources(
                    yamaa_adapters::host_limits::RECORDS,
                    yamaa_adapters::host_limits::REPLY_BYTES,
                )
                .map_err(|_| {
                    PyValueError::new_err(
                        "source evidence limit; original rejection remains retained",
                    )
                })?,
            Err(file_workflow::Error::Graph(error)) => error
                .retained_sources(
                    yamaa_adapters::host_limits::RECORDS,
                    yamaa_adapters::host_limits::REPLY_BYTES,
                )
                .map_err(|_| {
                    PyValueError::new_err(
                        "source evidence limit; original rejection remains retained",
                    )
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
/// Transfer the selected whole build directly; preparation is never repeated.
pub(crate) fn build_selected(
    py: Python<'_>,
    build: &mut FileBuild,
    fields: Arc<Vec<String>>,
    report_bytes: usize,
) -> Attempt {
    build_selected_with_observer(
        py,
        build,
        fields,
        report_bytes,
        yamaa_engine::application_observer::Scope::new(&yamaa_engine::application_observer::Noop),
    )
}
pub(crate) fn build_selected_with_observer(
    py: Python<'_>,
    build: &mut FileBuild,
    fields: Arc<Vec<String>>,
    report_bytes: usize,
    scope: yamaa_engine::application_observer::Scope<'_>,
) -> Attempt {
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
    let inner = build.build_owned_reported_with_observer(
        &mut port,
        crate::specification_result::identity(&fields),
        limits,
        scope,
    );
    Attempt {
        inner: Arc::new(Mutex::new(inner)),
        fields,
    }
}

#[pyfunction]
pub fn _prepare_producer_file(specification: &str, environment: &str) -> Specification {
    Specification {
        inner: Mutex::new(
            file_workflow::prepare(
                specification,
                environment,
                yamaa_engine::host_values::Language::Python,
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
    pub(crate) fn retained(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            fields: Arc::clone(&self.fields),
        }
    }
    pub(crate) fn formatted(
        &self,
        py: Python<'_>,
        maximum: usize,
        public: bool,
    ) -> PyResult<Report> {
        let state = self.get()?;
        let mut count = 0usize;
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, _| {
            count = count.saturating_add(1)
        });
        let mut remaining = maximum.min(yamaa_adapters::host_limits::REPLY_BYTES);
        let admitted = count <= yamaa_adapters::host_limits::RECORDS
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
        let mut domain = None;
        let mut output_error = None;
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
            let projected = if public {
                match yamaa_adapters::producer_result::prepare_owned_classified(
                    &state,
                    crate::specification_result::identity(&self.fields),
                    maximum,
                    &facts,
                ) {
                    Ok(value) => {
                        domain = Some(Mutex::new(value));
                        Ok(None)
                    }
                    Err(yamaa_adapters::producer_result::Error::Report(error)) => Err(error),
                    Err(yamaa_adapters::producer_result::Error::Output {
                        observations: value,
                        error,
                        ..
                    }) => {
                        observations = Some(value.to_string());
                        output_error = Some(error);
                        refusal = "output_projection";
                        Ok(None)
                    }
                }
            } else {
                producer_report::attempt_report_classified(
                    &state,
                    crate::specification_result::identity(&self.fields),
                    maximum,
                    &facts,
                )
                .map(Some)
            };
            match projected {
                Ok(Some(value)) => observations = Some(value.to_string()),
                Ok(None) => {}
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
    #[pyo3(signature = (maximum=yamaa_adapters::host_limits::REPLY_BYTES))]
    fn result(&self, py: Python<'_>, maximum: usize) -> PyResult<Report> {
        self.formatted(py, maximum, false)
    }
    pub(crate) fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.get()?;
        let mut count = 0usize;
        yamaa_adapters::producer_attempt::visit_host_failures(&state, |_, error| {
            if matches!(error, HostError::Host(_)) {
                count = count.saturating_add(1);
            }
        });
        if count > yamaa_adapters::host_limits::RECORDS {
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
    domain: Option<Mutex<yamaa_adapters::producer_result::RootResult>>,
    output_error: Option<yamaa_adapters::table_transport::TableTransportError>,
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
                Ok(yamaa_adapters::file_application::rejected(
                    "build",
                    self.refusal,
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
pub(crate) fn source_budget<'a>(
    sources: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> PyResult<()> {
    yamaa_adapters::host_limits::source_evidence(sources).map_err(PyValueError::new_err)
}
