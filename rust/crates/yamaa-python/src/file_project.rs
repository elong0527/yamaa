//! Private installed project lifecycle over captured candidate schemas. Handles
//! retain whole typed attempts; phase evidence grants no publication authority.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString},
};
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa_adapters::file_workflow::{self, Selected, SelectionError};
use yamaa_engine::host_values::Language;

type NativeAttempt = yamaa_adapters::project_run::Attempt<
    yamaa_adapters::file_resources::Error,
    crate::project_activation::Error,
>;
#[pyclass(frozen, module = "yamaa._native", name = "_ProjectSpecification")]
pub struct Specification {
    inner: Mutex<Result<Selected, SelectionError>>,
}
impl Specification {
    fn get(&self) -> PyResult<MutexGuard<'_, Result<Selected, SelectionError>>> {
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
        Ok(if self.get()?.is_ok() {
            "ready"
        } else {
            "preparation_failure"
        })
    }
    fn check_issues(&self) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        let state = self.get()?;
        let issues = match &*state {
            Ok(owner) => yamaa_adapters::source_metadata::selected_issues(owner)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?,
            Err(error) => {
                yamaa_adapters::project_application::selection_failure(error, Language::Python)
            }
        };
        Ok(crate::specification_result::issue_rows(&issues))
    }
    fn input_metadata(&self) -> PyResult<Vec<crate::source_metadata::InputRow>> {
        let state = self.get()?;
        let owner = state
            .as_ref()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))?;
        yamaa_adapters::source_metadata::selected_inputs(owner)
            .map(crate::source_metadata::input_rows)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))
    }
    fn diagnostic_origins(&self) -> PyResult<Vec<(String, String)>> {
        let state = self.get()?;
        match &*state {
            Ok(owner) => yamaa_adapters::source_metadata::selected_origins(owner)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message())),
            Err(error) => yamaa_adapters::source_metadata::rejected_origins(error)
                .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message())),
        }
    }
    fn capture_input_metadata<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        maximum: usize,
    ) -> PyResult<(
        Option<Bound<'py, PyBytes>>,
        Vec<crate::specification_result::IssueRow>,
    )> {
        let mut state = self.get()?;
        let owner = state
            .as_mut()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))?;
        Ok(
            match yamaa_adapters::source_metadata::selected_capture(owner, index, maximum) {
                Ok(bytes) => (Some(PyBytes::new(py, &bytes)), Vec::new()),
                Err(issues) => (None, crate::specification_result::issue_rows(&issues)),
            },
        )
    }
    fn check_source_metadata(
        &self,
        sources: &Bound<'_, PyList>,
    ) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        let sources = crate::source_metadata::convert(sources)?;
        let state = self.get()?;
        let owner = state
            .as_ref()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))?;
        let issues = yamaa_adapters::source_metadata::selected_check(owner, &sources)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?;
        Ok(crate::specification_result::issue_rows(&issues))
    }
    fn capture_reads(&self) -> PyResult<usize> {
        self.get()?
            .as_ref()
            .map(yamaa_adapters::source_metadata::selected_reads)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))
    }
    fn build(&self, py: Python<'_>) -> PyResult<Attempt> {
        let mut state = self.get()?;
        let project = state
            .as_mut()
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("project preparation failed"))?;
        let yamaa_engine::build::Prepared::Ordinary(project) = project else {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "this metadata handle is not an ordinary project build capability",
            ));
        };
        let inner = project.build_with(|run| {
            crate::project_activation::Port::new(
                py,
                run.captured_environment()
                    .lock()
                    .map_or(&[], |lock| &lock.source.bytes),
            )
        });
        Ok(Attempt {
            inner: Arc::new(Mutex::new(inner)),
            run: project.retained_run(),
        })
    }
}
#[pyclass(frozen, module = "yamaa._native", name = "_ProjectAttempt")]
pub struct Attempt {
    pub(super) inner: Arc<Mutex<NativeAttempt>>,
    pub(super) run: Arc<yamaa_adapters::project_run::PreparedRun>,
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
    fn result(
        &self,
        py: Python<'_>,
        metadata: &Bound<'_, pyo3::types::PyTuple>,
    ) -> PyResult<crate::project_result::Result> {
        crate::project_result::prepare(py, self, metadata)
    }
    /// This is engine execution status, before output admission or publication.
    fn engine_succeeded(&self) -> PyResult<bool> {
        let state = self.get()?;
        Ok(yamaa_adapters::project_attempt::execution(&state).is_some())
    }
    pub(super) fn activation(&self) -> PyResult<String> {
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
    pub(super) fn prepared_sources<'py>(
        &self,
        py: Python<'py>,
    ) -> Vec<(String, Bound<'py, PyBytes>)> {
        [
            self.run.document().source(),
            self.run.captured_environment().root().source(),
        ]
        .into_iter()
        .map(|source| (source.identity.clone(), PyBytes::new(py, &source.bytes)))
        .collect()
    }
    pub(super) fn host_failures(&self, py: Python<'_>) -> PyResult<Vec<(&'static str, Py<PyAny>)>> {
        let state = self.get()?;
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&state, |stage, error| {
            if let crate::project_activation::Error::Host(error) = error {
                failures.push((stage, error.value(py).clone().into_any().unbind()));
            }
        });
        Ok(failures)
    }
    pub(super) fn failure_facts<'py>(
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
    pub(super) fn study_snapshot<'py>(
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
#[pyo3(signature = (project_root, base_directory, entry, environment, data_roots, specification_modules, specification_entry, environment_modules, environment_entry))]
pub fn _prepare_file_project(
    project_root: &str,
    base_directory: &str,
    entry: &str,
    environment: Option<&str>,
    data_roots: &Bound<'_, PyList>,
    specification_modules: &Bound<'_, PyList>,
    specification_entry: usize,
    environment_modules: &Bound<'_, PyList>,
    environment_entry: usize,
) -> PyResult<Specification> {
    if data_roots.len() >= 64
        || [project_root, base_directory, entry]
            .into_iter()
            .chain(environment)
            .any(|text| text.len() > yamaa_adapters::host_limits::IDENTITY_BYTES)
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "project path or root limit",
        ));
    }
    let roots = data_roots
        .iter()
        .map(|value| {
            let text = value.cast::<PyString>()?.to_str()?;
            if text.len() > yamaa_adapters::host_limits::IDENTITY_BYTES {
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
    let resources = file_workflow::repository_resources(project_root, base_directory, &roots)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?;
    let inner = file_workflow::select_with_optional_resources(
        resources,
        entry,
        environment,
        Language::Python,
        specification_schema,
        environment_schema,
        Default::default(),
    );
    Ok(Specification {
        inner: Mutex::new(inner),
    })
}
