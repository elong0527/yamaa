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

#[pymethods]
impl Domain {
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
        let failures = guard
            .project_failures()
            .unwrap_or(&[])
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
        .interrupt()
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
