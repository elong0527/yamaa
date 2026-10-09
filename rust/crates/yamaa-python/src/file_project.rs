//! Private installed project lifecycle over captured candidate schemas. Handles
//! retain whole typed attempts; phase evidence grants no publication authority.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString},
};
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa_adapters::{
    file_project::{self, FileProject},
    file_resources::Resources,
};
use yamaa_core::project_function::Language;

type NativeAttempt = yamaa_adapters::project_run::Attempt<
    yamaa_adapters::file_resources::Error,
    crate::project_activation::Error,
>;
#[pyclass(frozen, module = "yamaa._native", name = "_ProjectSpecification")]
pub struct Specification {
    inner: Mutex<Result<FileProject, file_project::Error>>,
}
impl Specification {
    fn get(&self) -> PyResult<MutexGuard<'_, Result<FileProject, file_project::Error>>> {
        self.inner.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "native project handle is already borrowed or poisoned",
            )
        })
    }
}
#[pymethods]
impl Specification {
    fn preparation_status(&self) -> PyResult<&'static str> {
        Ok(match &*self.get()? {
            Ok(_) => "ready",
            Err(file_project::Error::Resource(_)) => "resource_failure",
            Err(file_project::Error::Environment(_)) => "environment_failure",
            Err(file_project::Error::Document(_)) => "document_failure",
            Err(file_project::Error::Compile(_)) => "compile_failure",
        })
    }
    fn capture_reads(&self) -> PyResult<usize> {
        self.get()?
            .as_ref()
            .map(FileProject::capture_reads)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))
    }
    fn build(&self, py: Python<'_>) -> PyResult<Attempt> {
        let mut state = self.get()?;
        let project = state
            .as_mut()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))?;
        let inner = project.build_with(|run| {
            crate::project_activation::Port::new(
                py,
                run.captured_environment()
                    .lock()
                    .map_or(&[], |lock| &lock.source.bytes),
            )
        });
        Ok(Attempt {
            inner: Mutex::new(inner),
            run: project.retained_run(),
        })
    }
}
#[pyclass(frozen, module = "yamaa._native", name = "_ProjectAttempt")]
pub struct Attempt {
    inner: Mutex<NativeAttempt>,
    run: Arc<yamaa_adapters::project_run::PreparedRun>,
}
impl Attempt {
    fn get(&self) -> PyResult<MutexGuard<'_, NativeAttempt>> {
        self.inner.try_lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "native project attempt is already borrowed or poisoned",
            )
        })
    }
}
#[pymethods]
impl Attempt {
    /// This is engine execution status, before output admission or publication.
    fn engine_succeeded(&self) -> PyResult<bool> {
        let state = self.get()?;
        Ok(yamaa_adapters::project_attempt::execution(&state).is_some())
    }
    fn activation(&self) -> PyResult<String> {
        let state = self.get()?;
        yamaa_adapters::project_activation_observations::activation(&self.run, &state.activation)
            .map(|value| value.to_string())
            .map_err(|_| {
                pyo3::exceptions::PyValueError::new_err(
                    "invalid or over-limit activation observation",
                )
            })
    }
    /// Held authored entry snapshots remain available after the file handle dies.
    fn prepared_sources<'py>(&self, py: Python<'py>) -> Vec<(String, Bound<'py, PyBytes>)> {
        [
            self.run.document().source(),
            self.run.captured_environment().root().source(),
        ]
        .into_iter()
        .map(|source| (source.identity.clone(), PyBytes::new(py, &source.bytes)))
        .collect()
    }
    fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.get()?;
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&state, |stage, error| {
            if let crate::project_activation::Error::Host(error) = error {
                failures.push((stage, error.value(py).clone().into_any().unbind()));
            }
        });
        Ok(failures)
    }
    fn failure_facts<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<(&'static str, Bound<'py, pyo3::types::PyDict>)>> {
        let state = self.get()?;
        let mut facts = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&state, |stage, error| {
            facts.push(error.facts(py).map(|facts| (stage, facts)));
        });
        facts.into_iter().collect()
    }
    fn study_snapshot<'py>(
        &self,
        py: Python<'py>,
        index: usize,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let state = self.get()?;
        let source =
            state.dataset.sources.get(index).ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err("unknown attempted source")
            })?;
        Ok(source
            .snapshot
            .as_deref()
            .map(|bytes| PyBytes::new(py, bytes)))
    }
}

/// Candidate closures are explicit; installed public formats are unchanged.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn _prepare_file_project(
    project_root: &str,
    base_directory: &str,
    entry: &str,
    environment: &str,
    data_roots: &Bound<'_, PyList>,
    specification_modules: &Bound<'_, PyList>,
    specification_entry: usize,
    environment_modules: &Bound<'_, PyList>,
    environment_entry: usize,
) -> PyResult<Specification> {
    if data_roots.len() >= 64
        || [project_root, base_directory, entry, environment]
            .iter()
            .any(|text| text.len() > 65536)
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "project path or root limit",
        ));
    }
    let roots = data_roots
        .iter()
        .map(|value| {
            let text = value.cast::<PyString>()?.to_str()?;
            if text.len() > 65536 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "project root byte limit",
                ));
            }
            Ok(text.to_owned())
        })
        .collect::<PyResult<Vec<_>>>()?;
    let specification_schema = crate::specification_service::captured_modules(
        specification_modules,
        specification_entry,
        "root_class",
    )?;
    let environment_schema = crate::specification_service::captured_modules(
        environment_modules,
        environment_entry,
        "environment_class",
    )?;
    let resources = Resources::new(project_root, base_directory, &roots)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?;
    let inner = FileProject::prepare(
        resources,
        entry,
        environment,
        Language::Python,
        specification_schema,
        environment_schema,
    );
    Ok(Specification {
        inner: Mutex::new(inner),
    })
}
