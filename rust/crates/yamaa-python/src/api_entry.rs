//! Native half of the host API: specification operations, their results and
//! one error type. The Python facade keeps paths in lazy objects and calls
//! these functions; behavior, validation and messages stay in Rust.
use pyo3::{exceptions::PyException, prelude::*, types::PyDict};
use std::sync::{Arc, Mutex};
use yamaa::{Action, CancellationToken, Engine, EngineOptions, Error, Failure, HostLanguage};

use crate::arrow_bridge::ArrowTable;

pyo3::create_exception!(yamaa, _Error, PyException);

/// Raise `_Error` carrying the issues that explain it as an `issues` table,
/// and the files a refused save wrote before it stopped as `paths`.
fn raise(py: Python<'_>, failure: Failure) -> PyErr {
    let error = _Error::new_err(failure.message);
    let value = error.value(py);
    match value
        .setattr("issues", ArrowTable::from(&failure.issues))
        .and_then(|()| value.setattr("paths", failure.paths))
    {
        Ok(()) => error,
        Err(setattr) => setattr,
    }
}

/// Raise the refusal of `action` for `error` (D39), or the cancellation.
fn raise_error(py: Python<'_>, action: Action<'_>, error: Error) -> PyErr {
    match error {
        Error::Cancelled => pyo3::exceptions::PyRuntimeError::new_err("operation cancelled"),
        error => raise(py, Failure::from_error(action, &error)),
    }
}

// `Python::version` builds on the PyO3 0.27.2 floor; 0.29 renamed it `version_str`.
#[allow(deprecated)]
fn engine(py: Python<'_>) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "python".into(), py.version().into())
            .with_functions(Arc::new(crate::project_activation::Port)),
    )
}

fn poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("run handle is in use or poisoned")
}

#[pyclass(frozen, module = "yamaa._native", name = "_Run")]
pub struct Run {
    pub(crate) inner: Arc<Mutex<yamaa::Run>>,
}

impl Run {
    pub(crate) fn new(run: yamaa::Run) -> Self {
        Self {
            inner: Arc::new(Mutex::new(run)),
        }
    }

    /// A project interrupt fails the call with its own object.
    pub(crate) fn propagate_interrupt(&self, py: Python<'_>) -> PyResult<()> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        let build = guard.build();
        let interrupt = build
            .interrupt()
            .and_then(|error| crate::function_callback::payload(py, error));
        match interrupt {
            Some(error) => Err(crate::function_callback::raise(py, error)),
            None => Ok(()),
        }
    }
}

#[pymethods]
impl Run {
    fn output(&self) -> PyResult<Option<ArrowTable>> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        Ok(guard.output().map(ArrowTable::from))
    }
    fn issues(&self) -> PyResult<ArrowTable> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        Ok(ArrowTable::from(&guard.issues()))
    }
    fn report(&self) -> PyResult<Option<ArrowTable>> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        Ok(guard.report().map(ArrowTable::from))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let saved = self.inner.try_lock().map_err(|_| poisoned())?.save();
        saved.map_err(|failure| raise(py, failure))
    }
    /// Private installed-run evidence for qualification suites.
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, include_measurements: bool) -> PyResult<Option<String>> {
        let guard = self.inner.try_lock().map_err(|_| poisoned())?;
        Ok(guard.build().observations(include_measurements))
    }
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_check(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
) -> PyResult<ArrowTable> {
    let token = CancellationToken::new();
    engine(py)
        .spec_check(specification, project, &token)
        .map(|check| ArrowTable::from(&check.issues()))
        .map_err(|error| raise_error(py, Action::Read(specification), error))
}

/// The compiled plans one lazy specification or project object keeps between
/// its runs (#2821); see `yamaa::Plans`.
#[pyclass(frozen, module = "yamaa._native", name = "_Plans")]
pub struct Plans {
    inner: yamaa::Plans,
}

/// An empty holder for one lazy object. Creating it reads nothing.
#[pyfunction]
pub fn _plans() -> Plans {
    Plans {
        inner: yamaa::Plans::new(),
    }
}

#[pyfunction]
#[pyo3(signature=(specification, project=None, plans=None))]
pub fn _spec_run(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
) -> PyResult<Run> {
    let token = CancellationToken::new();
    let engine = engine(py);
    let run = match plans.as_deref() {
        Some(plans) => engine.spec_run_reusing(specification, project, &plans.inner, &token),
        None => engine.spec_run(specification, project, &token),
    }
    .map_err(|error| raise_error(py, Action::Read(specification), error))?;
    let run = Run::new(run);
    run.propagate_interrupt(py)?;
    Ok(run)
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_resolve<'py>(
    py: Python<'py>,
    specification: &str,
    project: Option<&str>,
) -> PyResult<Bound<'py, PyAny>> {
    let value = engine(py)
        .spec_resolve(specification, project)
        .map_err(|failure| raise(py, failure))?;
    crate::context_values::context_value(py, &value)
}

#[pyfunction]
#[pyo3(signature=(source, archive_member=None, max_archive_members=10_000, max_expanded_bytes=268_435_456, max_xml_depth=64))]
pub fn _read_odm(
    py: Python<'_>,
    source: &str,
    archive_member: Option<String>,
    max_archive_members: usize,
    max_expanded_bytes: usize,
    max_xml_depth: usize,
) -> PyResult<ArrowTable> {
    // The shared option validation refuses limits below its minimums, so
    // every host reports the same refusal.
    let mut options = yamaa::OdmOptions::new()
        .with_max_archive_members(max_archive_members)
        .with_max_expanded_bytes(u64::try_from(max_expanded_bytes).unwrap_or(u64::MAX))
        .with_max_xml_depth(max_xml_depth);
    if let Some(member) = archive_member {
        options = options.with_archive_member(member);
    }
    yamaa::read_odm(std::path::Path::new(source), &options)
        .map(|table| ArrowTable::from(&table))
        .map_err(|error| raise_error(py, Action::Read(source), error))
}

/// The lint table of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted raises `_Error`.
#[pyfunction]
#[pyo3(signature=(paths, fix=false))]
pub fn _lint(py: Python<'_>, paths: Vec<String>, fix: bool) -> PyResult<ArrowTable> {
    yamaa::lint(&paths, fix)
        .map(|lint| ArrowTable::from(&lint.table()))
        .map_err(|failure| raise(py, failure))
}

/// The JSON Schema of a file type as canonical JSON text (#3030 D149); a
/// type the facade does not know raises `_Error`.
#[pyfunction]
pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {
    yamaa::schema(file_type).map_err(|failure| raise(py, failure))
}

/// The installation self-test (#3030 D28, D119): the facade's result, whose
/// status, cases, engine information and printed report the public class
/// returns as they are.
#[pyclass(frozen, module = "yamaa._native", name = "_SelfTest")]
pub struct SelfTest {
    inner: yamaa::SelfTest,
}

#[pymethods]
impl SelfTest {
    fn status(&self) -> &'static str {
        self.inner.status().as_str()
    }
    fn cases(&self) -> ArrowTable {
        ArrowTable::from(&self.inner.table())
    }
    /// The engine information, keyed in the facade's field order.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let info = self.inner.info();
        let fields = PyDict::new(py);
        fields.set_item("version", info.version)?;
        fields.set_item("language_versions", &info.language_versions)?;
        fields.set_item("features", &info.features)?;
        fields.set_item("host", info.host.as_str())?;
        fields.set_item("host_version", info.host_version.as_deref())?;
        fields.set_item("platform", &info.platform)?;
        Ok(fields)
    }
    fn summary(&self) -> String {
        self.inner.summary().to_string()
    }
}

/// Run every selftest case through this engine as the Python host, with the
/// running interpreter's version, such as `3.12.4`.
#[pyfunction]
pub fn _selftest(py: Python<'_>) -> SelfTest {
    let version = py.version_info();
    let version = format!(
        "{}.{}.{}{}",
        version.major,
        version.minor,
        version.patch,
        version.suffix.unwrap_or_default()
    );
    SelfTest {
        inner: yamaa::selftest(yamaa::Host::Python, Some(&version)),
    }
}

fn project_poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("project handle is in use or poisoned")
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProjectRun")]
pub struct ProjectRun {
    inner: Arc<Mutex<yamaa::ProjectRun>>,
}

#[pymethods]
impl ProjectRun {
    fn names(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| project_poisoned())?
            .names())
    }
    fn output(&self, py: Python<'_>, name: &str) -> PyResult<Option<ArrowTable>> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.output().map(ArrowTable::from))
    }
    fn member_issues(&self, py: Python<'_>, name: &str) -> PyResult<ArrowTable> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(ArrowTable::from(&run.issues()))
    }
    fn member_report(&self, py: Python<'_>, name: &str) -> PyResult<Option<ArrowTable>> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.report().map(ArrowTable::from))
    }
    fn issues(&self) -> PyResult<ArrowTable> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(ArrowTable::from(&guard.issues()))
    }
    fn report(&self) -> PyResult<ArrowTable> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(ArrowTable::from(&guard.report()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let saved = self
            .inner
            .try_lock()
            .map_err(|_| project_poisoned())?
            .save();
        saved.map_err(|failure| raise(py, failure))
    }
}

#[pyclass(frozen, module = "yamaa._native", name = "_Submission")]
pub struct Submission {
    inner: Arc<Mutex<yamaa::Submission>>,
}

#[pymethods]
impl Submission {
    fn issues(&self) -> PyResult<ArrowTable> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(ArrowTable::from(&guard.issues()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let saved = self
            .inner
            .try_lock()
            .map_err(|_| project_poisoned())?
            .save();
        saved.map_err(|failure| raise(py, failure))
    }
}

#[pyfunction]
pub fn _project_spec(py: Python<'_>, project: &str, name: &str) -> PyResult<String> {
    engine(py)
        .project_spec(project, name)
        .map_err(|failure| raise(py, failure))
}

#[pyfunction]
pub fn _project_check(py: Python<'_>, project: &str) -> PyResult<ArrowTable> {
    engine(py)
        .project_check(project, &CancellationToken::new())
        .map(|check| ArrowTable::from(&check.issues()))
        .map_err(|error| raise_error(py, Action::Read(project), error))
}

#[pyfunction]
#[pyo3(signature=(project, section=None, plans=None))]
pub fn _project_run(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
) -> PyResult<ProjectRun> {
    let token = CancellationToken::new();
    let engine = engine(py);
    let runs = match plans.as_deref() {
        Some(plans) => engine.project_run_reusing(project, section, &plans.inner, &token),
        None => engine.project_run(project, section, &token),
    }
    // Only an unknown section refuses the call with issues.
    .map_err(|error| raise_error(py, Action::Select(section.unwrap_or_default()), error))?;
    // A project interrupt in any specification fails the call with its own
    // object, as a single run does.
    for name in runs.names() {
        if let Some(run) = runs.get(&name) {
            let build = run.build();
            let interrupt = build
                .interrupt()
                .and_then(|error| crate::function_callback::payload(py, error));
            if let Some(error) = interrupt {
                return Err(crate::function_callback::raise(py, error));
            }
        }
    }
    Ok(ProjectRun {
        inner: Arc::new(Mutex::new(runs)),
    })
}

#[pyfunction]
#[pyo3(signature=(project, section=None, created=None))]
pub fn _project_submission(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    created: Option<&str>,
) -> PyResult<Submission> {
    let submission = engine(py)
        .project_submission(
            project,
            section,
            created,
            yamaa::VERSION,
            &CancellationToken::new(),
        )
        // Only an unknown section refuses the call with issues.
        .map_err(|error| raise_error(py, Action::Select(section.unwrap_or_default()), error))?;
    Ok(Submission {
        inner: Arc::new(Mutex::new(submission)),
    })
}
