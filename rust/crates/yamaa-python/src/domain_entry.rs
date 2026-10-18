//! Public-request argument/result conversion; the shared engine owns the lifecycle.
use pyo3::{exceptions::PyException, prelude::*, types::PyBytes};
use std::sync::{Arc, Mutex};
use yamaa::{
    Build, CancellationToken, Engine, EngineOptions, Error, HostLanguage, Request, SaveOutcome,
};

pyo3::create_exception!(yamaa, DomainError, PyException);

fn poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("domain result handle poisoned")
}

#[pyclass(frozen, module = "yamaa._native", name = "_DomainResult")]
pub struct Domain {
    pub(crate) inner: Arc<Mutex<Build>>,
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProducerAttempt")]
pub struct Attempt {
    inner: Arc<Mutex<Build>>,
}

impl Attempt {
    fn get(&self) -> PyResult<std::sync::MutexGuard<'_, Build>> {
        self.inner.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "native producer handle is already borrowed or poisoned",
            )
        })
    }
}

#[pymethods]
impl Domain {
    /// The public facade propagates control flow from this retained owner.
    fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        let interrupt = self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .producer_interrupt()
            .and_then(|error| crate::function_callback::payload(py, error));
        match interrupt {
            Some(error) => Err(crate::function_callback::raise(py, error)),
            None => Ok(()),
        }
    }
    /// Private qualification borrows the actual retained whole attempt.
    fn producer_attempt(&self, py: Python<'_>) -> PyResult<Option<Attempt>> {
        let producer = self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .producer_nodes()
            .is_some();
        let _ = py;
        Ok(producer.then(|| Attempt {
            inner: Arc::clone(&self.inner),
        }))
    }
    fn producer_projection_failure(&self) -> PyResult<Option<String>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .projection_failure()
            .map(str::to_owned))
    }
    fn preparation_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        let sources = self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .preparation_sources()
            .iter()
            .map(|(name, bytes)| (name.clone(), PyBytes::new(py, bytes)))
            .collect();
        Ok(sources)
    }
    /// Qualification can inspect original host failures even if report projection refuses.
    fn project_failures(&self, py: Python<'_>) -> PyResult<Vec<(String, Py<PyAny>)>> {
        let _ = py;
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        let project = guard.project_failures().unwrap_or(&[]);
        let producer = guard.producer_failures().unwrap_or(&[]);
        let failures = if project.is_empty() {
            producer
        } else {
            project
        };
        let failures = failures
            .iter()
            .filter_map(|failure| {
                crate::function_callback::payload(py, &failure.error)
                    .map(|error| (failure.stage.to_owned(), error))
            })
            .collect();
        Ok(failures)
    }
    /// Private installed-run evidence; unsupported preparation has no build report.
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, include_measurements: bool) -> PyResult<Option<String>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .observations(include_measurements))
    }
    fn output(&self) -> PyResult<Option<crate::arrow_bridge::ArrowTable>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .output()
            .map(crate::arrow_bridge::ArrowTable::from))
    }
    /// One native row per diagnostic; the facade projects the issues frame
    /// and the named records from these same rows.
    fn issues<'py>(&self, py: Python<'py>) -> PyResult<Vec<crate::report_fields::IssueRow<'py>>> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        crate::report_fields::issue_rows(py, &guard.report().diagnostics)
    }
    fn log(&self, warning: bool) -> PyResult<Option<crate::arrow_bridge::ArrowTable>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| poisoned())?
            .log(warning)
            .map(crate::arrow_bridge::ArrowTable::from))
    }
    fn save(&self) -> PyResult<bool> {
        match self.inner.try_lock().map_err(|_| poisoned())?.save() {
            Ok(SaveOutcome::Saved) => Ok(true),
            Ok(_) => Ok(false),
            Err(_) => Err(DomainError::new_err("cannot save a failed build")),
        }
    }
}

#[pymethods]
impl Attempt {
    fn accepted(&self) -> PyResult<bool> {
        Ok(self.get()?.producer_accepted().unwrap_or(false))
    }
    fn entered_nodes(&self) -> PyResult<Vec<usize>> {
        Ok(self.get()?.producer_nodes().unwrap_or(&[]).to_vec())
    }
    pub(crate) fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(String, Py<PyAny>)>> {
        let _ = py;
        let failures = self
            .get()?
            .producer_failures()
            .unwrap_or(&[])
            .iter()
            .filter_map(|failure| {
                crate::function_callback::payload(py, &failure.error)
                    .map(|error| (failure.stage.to_owned(), error))
            })
            .collect();
        Ok(failures)
    }
    pub(crate) fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        let interrupt = self
            .get()?
            .producer_interrupt()
            .and_then(|error| crate::function_callback::payload(py, error));
        match interrupt {
            Some(error) => Err(crate::function_callback::raise(py, error)),
            None => Ok(()),
        }
    }
    fn retained_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(String, Bound<'py, PyBytes>)>> {
        let sources = self
            .get()?
            .producer_sources()
            .into_iter()
            .flatten()
            .map(|(name, bytes)| (name.clone(), PyBytes::new(py, bytes)))
            .collect();
        Ok(sources)
    }
    /// Artifact bytes are evidence, including earlier accepted producer evidence
    /// after a later failure. This class exposes no publisher or Save method.
    fn artifact<'py>(&self, py: Python<'py>, node: usize) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let bytes = self
            .get()?
            .producer_artifact(node)
            .map(|bytes| PyBytes::new(py, bytes));
        Ok(bytes)
    }
}

// `Python::version` builds on the PyO3 0.27.2 floor; 0.29 renamed it `version_str`.
#[allow(deprecated)]
fn engine(py: Python<'_>, resolver: Option<Py<PyAny>>) -> Engine {
    let mut options =
        EngineOptions::new(HostLanguage::Python, "python".into(), py.version().into())
            .with_functions(Arc::new(crate::project_activation::Port));
    if let Some(resolver) = resolver {
        options = options.with_resolver(crate::resource_resolver::Port::from_callback(resolver));
    }
    Engine::new(options)
}

#[pyfunction]
#[pyo3(signature=(specification,environment=None,resolver=None))]
pub fn _domain_file(
    py: Python<'_>,
    specification: &str,
    environment: Option<&str>,
    resolver: Option<Py<PyAny>>,
) -> PyResult<Domain> {
    let mut request = Request::new(specification);
    if let Some(environment) = environment {
        request = request.with_environment(environment);
    }
    let token = CancellationToken::new();
    let build = engine(py, resolver)
        .build(&request, &token)
        .map_err(|error| match error {
            Error::Cancelled => pyo3::exceptions::PyRuntimeError::new_err("build cancelled"),
            error => DomainError::new_err(format!("{error}")),
        })?;
    let domain = Domain {
        inner: Arc::new(Mutex::new(build)),
    };
    // Preserve the original control-flow payload: a project interrupt fails
    // the call with its own object, as the runtime port did.
    let interrupt = domain
        .inner
        .try_lock()
        .map_err(|_| poisoned())?
        .project_interrupt()
        .and_then(|error| crate::function_callback::payload(py, error));
    if let Some(error) = interrupt {
        return Err(crate::function_callback::raise(py, error));
    }
    Ok(domain)
}
#[pyfunction]
#[pyo3(signature=(specification,environment=None,resolver=None))]
pub fn _check_file<'py>(
    py: Python<'py>,
    specification: &str,
    environment: Option<&str>,
    resolver: Option<Py<PyAny>>,
) -> PyResult<Vec<crate::report_fields::IssueRow<'py>>> {
    let mut request = Request::new(specification);
    if let Some(environment) = environment {
        request = request.with_environment(environment);
    }
    let token = CancellationToken::new();
    let report = engine(py, resolver).check(&request, &token);
    crate::report_fields::issue_rows(py, &report.diagnostics)
}
