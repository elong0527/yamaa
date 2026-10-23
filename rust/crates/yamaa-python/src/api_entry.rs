//! Native half of the host API: specification operations, their results and
//! one error type. The Python facade keeps paths in lazy objects and calls
//! these functions; behavior, validation and messages stay in Rust.
use pyo3::{exceptions::PyException, prelude::*, types::PyDict};
use std::sync::{Arc, Mutex};
use yamaa::{Action, Engine, EngineOptions, Error, Failure, HostLanguage};
use yamaa_boundary::{HandleKind, Live};

use crate::arrow_bridge::Table;
use crate::interface::{gate, CENSUS};

pyo3::create_exception!(yamaa, _Error, PyException);

/// Raise `_Error` carrying the issues that explain it as an `issues` table,
/// and the files a refused save wrote before it stopped as `paths`.
fn raise(py: Python<'_>, failure: Failure) -> PyErr {
    let error = _Error::new_err(failure.message);
    let value = error.value(py);
    match value
        .setattr("issues", Table::from(&failure.issues))
        .and_then(|()| value.setattr("paths", failure.paths))
    {
        Ok(()) => error,
        Err(setattr) => setattr,
    }
}

/// Raise the refusal of `action` for `error` (D39), or the cancellation.
fn raise_error(py: Python<'_>, action: Action<'_>, error: Error) -> PyErr {
    raise(py, Failure::from_error(action, &error))
}

/// The engine of one call, behind the interface gate.
// `Python::version` builds on the PyO3 0.27.2 floor; 0.29 renamed it `version_str`.
#[allow(deprecated)]
fn engine(py: Python<'_>) -> PyResult<Engine> {
    gate()?;
    Ok(Engine::new(
        EngineOptions::new(HostLanguage::Python, "python".into(), py.version().into())
            .with_functions(Arc::new(crate::project_activation::Port)),
    ))
}

fn poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("run handle is in use or poisoned")
}

/// A streamed run's refusal of a reader or of its result (#3697): a boundary
/// refusal, raised as `RuntimeError` as a busy handle's is, with D39's text
/// of reading the run, such as `cannot read DM: operation cancelled`.
pub(crate) fn refusal(streamed: &yamaa::Streamed, error: &Error) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        Failure::from_error(Action::Read(streamed.subject()), error).message,
    )
}

/// A run built whole, or a streamed run (#3697), whose result is known once
/// its output's stream ends.
enum Held {
    Built(Arc<Mutex<yamaa::Run>>),
    Streamed(Arc<yamaa::Streamed>),
}

#[pyclass(frozen, module = "yamaa._native", name = "_Run")]
pub struct Run {
    held: Held,
    _live: Live,
}

impl Run {
    pub(crate) fn new(run: yamaa::Run) -> Self {
        Self::held(Held::Built(Arc::new(Mutex::new(run))))
    }

    fn streamed(streamed: yamaa::Streamed) -> Self {
        Self::held(Held::Streamed(Arc::new(streamed)))
    }

    fn held(held: Held) -> Self {
        Self {
            held,
            _live: CENSUS.hold(HandleKind::Run),
        }
    }

    /// Read the run's result with `read`. A streamed run nobody started to
    /// read runs whole first, inside a native call that raises its
    /// interrupt again (D46); one whose stream has not ended, or that was
    /// cancelled, refuses.
    fn result<T>(
        &self,
        py: Python<'_>,
        read: impl FnOnce(&mut yamaa::Run) -> PyResult<T>,
    ) -> PyResult<T> {
        match &self.held {
            Held::Built(run) => read(&mut *run.try_lock().map_err(|_| poisoned())?),
            Held::Streamed(streamed) => crate::interrupt::run(py, |_| {
                let mut run = streamed.run().map_err(|error| refusal(streamed, &error))?;
                read(&mut run)
            }),
        }
    }
}

/// A streamed run's output: its stream, read through a table, or, once the
/// run ran whole, the table it keeps; none for a failed run.
fn streamed_output(streamed: &Arc<yamaa::Streamed>) -> PyResult<Option<Table>> {
    let progress = streamed
        .progress()
        .map_err(|error| refusal(streamed, &error))?;
    match progress {
        yamaa::StreamProgress::Cancelled => Err(refusal(streamed, &Error::Cancelled)),
        yamaa::StreamProgress::Ended => {
            let run = streamed.run().map_err(|error| refusal(streamed, &error))?;
            Ok(match run.output() {
                Some(table) => Some(Table::from(table)),
                None if run.status() == yamaa::Status::Failed => None,
                None => Some(Table::stream(Arc::clone(streamed))),
            })
        }
        _ => Ok(Some(Table::stream(Arc::clone(streamed)))),
    }
}

#[pymethods]
impl Run {
    fn status(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.result(py, |run| Ok(run.status().as_str()))
    }
    fn output(&self) -> PyResult<Option<Table>> {
        match &self.held {
            Held::Built(run) => Ok(run
                .try_lock()
                .map_err(|_| poisoned())?
                .output()
                .map(Table::from)),
            Held::Streamed(streamed) => streamed_output(streamed),
        }
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |run| Ok(Table::from(&run.issues())))
    }
    fn report(&self, py: Python<'_>) -> PyResult<Option<Table>> {
        self.result(py, |run| Ok(run.report().map(Table::from)))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        crate::interrupt::run(py, |token| {
            let saved = match &self.held {
                Held::Built(run) => run.try_lock().map_err(|_| poisoned())?.save(token),
                Held::Streamed(streamed) => streamed
                    .run()
                    .map_err(|error| refusal(streamed, &error))?
                    .save(token),
            };
            saved.map_err(|failure| raise(py, failure))
        })
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |run| Ok(run.summary().to_string()))
    }
    /// Private installed-run evidence for qualification suites.
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, py: Python<'_>, include_measurements: bool) -> PyResult<Option<String>> {
        self.result(py, |run| Ok(run.observations(include_measurements)))
    }
}

fn check_poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("check handle is in use or poisoned")
}

/// The static check of a specification or a project (D36, D37): the facade's
/// result, whose status, issues and printed text the public class returns as
/// they are.
#[pyclass(frozen, module = "yamaa._native", name = "_Check")]
pub struct Check {
    inner: Mutex<yamaa::Check>,
    _live: Live,
}

impl Check {
    fn new(check: yamaa::Check) -> Self {
        Self {
            inner: Mutex::new(check),
            _live: CENSUS.hold(HandleKind::Check),
        }
    }
}

#[pymethods]
impl Check {
    fn status(&self) -> PyResult<&'static str> {
        let guard = self.inner.try_lock().map_err(|_| check_poisoned())?;
        Ok(guard.status().as_str())
    }
    fn issues(&self) -> PyResult<Table> {
        let guard = self.inner.try_lock().map_err(|_| check_poisoned())?;
        Ok(Table::from(&guard.issues()))
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self) -> PyResult<String> {
        let guard = self.inner.try_lock().map_err(|_| check_poisoned())?;
        Ok(guard.summary().to_string())
    }
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_check(py: Python<'_>, specification: &str, project: Option<&str>) -> PyResult<Check> {
    crate::interrupt::run(py, |token| {
        engine(py)?
            .spec_check(specification, project, token)
            .map(Check::new)
            .map_err(|error| raise_error(py, Action::Read(specification), error))
    })
}

/// The printed text of a lazy specification object, which has read nothing
/// (D37): `yamaa spec <path>`, with the path as given.
#[pyfunction]
pub fn _spec_summary(specification: &str) -> PyResult<String> {
    gate()?;
    Ok(yamaa::Summary::spec(specification).to_string())
}

/// The printed text of a lazy project object: `yamaa project <path>`.
#[pyfunction]
pub fn _project_summary(project: &str) -> PyResult<String> {
    gate()?;
    Ok(yamaa::Summary::project(project).to_string())
}

/// The compiled plans one lazy specification or project object keeps between
/// its runs (#2821); see `yamaa::Plans`.
#[pyclass(frozen, module = "yamaa._native", name = "_Plans")]
pub struct Plans {
    inner: yamaa::Plans,
    _live: Live,
}

/// An empty holder for one lazy object. Creating it reads nothing.
#[pyfunction]
pub fn _plans() -> PyResult<Plans> {
    gate()?;
    Ok(Plans {
        inner: yamaa::Plans::new(),
        _live: CENSUS.hold(HandleKind::Plans),
    })
}

/// A run of one specification: built now, or with `stream` built as its
/// output is read (#3697), polling a token of its own from then on.
#[pyfunction]
#[pyo3(signature=(specification, project=None, plans=None, stream=false))]
pub fn _spec_run(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
    stream: bool,
) -> PyResult<Run> {
    crate::interrupt::run(py, |token| {
        let engine = engine(py)?;
        let refused = |error| raise_error(py, Action::Read(specification), error);
        if stream {
            let once = yamaa::Plans::new();
            let plans = plans.as_deref().map_or(&once, |plans| &plans.inner);
            let token = crate::interrupt::stream_token();
            return engine
                .spec_stream(specification, project, plans, &token)
                .map(Run::streamed)
                .map_err(refused);
        }
        let run = match plans.as_deref() {
            Some(plans) => engine.spec_run_reusing(specification, project, &plans.inner, token),
            None => engine.spec_run(specification, project, token),
        }
        .map_err(refused)?;
        Ok(Run::new(run))
    })
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_resolve<'py>(
    py: Python<'py>,
    specification: &str,
    project: Option<&str>,
) -> PyResult<Bound<'py, PyAny>> {
    let value = engine(py)?
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
) -> PyResult<Table> {
    // The shared option validation refuses limits below its minimums, so
    // every host reports the same refusal.
    let mut options = yamaa::OdmOptions::new()
        .with_max_archive_members(max_archive_members)
        .with_max_expanded_bytes(u64::try_from(max_expanded_bytes).unwrap_or(u64::MAX))
        .with_max_xml_depth(max_xml_depth);
    if let Some(member) = archive_member {
        options = options.with_archive_member(member);
    }
    engine(py)?
        .read_odm(source, &options)
        .map(|table| Table::from(&table))
        .map_err(|error| raise_error(py, Action::Read(source), error))
}

/// The lint table of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted raises `_Error`.
#[pyfunction]
#[pyo3(signature=(paths, fix=false))]
pub fn _lint(py: Python<'_>, paths: Vec<String>, fix: bool) -> PyResult<Table> {
    gate()?;
    yamaa::lint(&paths, fix)
        .map(|lint| Table::from(&lint.table()))
        .map_err(|failure| raise(py, failure))
}

/// The JSON Schema of a file type as canonical JSON text (#3030 D149); a
/// type the facade does not know raises `_Error`.
#[pyfunction]
pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {
    gate()?;
    yamaa::schema(file_type).map_err(|failure| raise(py, failure))
}

/// The installation self-test (#3030 D28, D119): the facade's result, whose
/// status, cases, engine information and printed report the public class
/// returns as they are.
#[pyclass(frozen, module = "yamaa._native", name = "_SelfTest")]
pub struct SelfTest {
    inner: yamaa::SelfTest,
    _live: Live,
}

#[pymethods]
impl SelfTest {
    fn status(&self) -> &'static str {
        self.inner.status().as_str()
    }
    fn cases(&self) -> Table {
        Table::from(&self.inner.table())
    }
    /// The engine information that ran the cases, as `info()` gives it.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        info_dict(py, self.inner.info())
    }
    fn summary(&self) -> String {
        self.inner.summary().to_string()
    }
}

/// The running interpreter's version, such as `3.12.4`, spelled as
/// `platform.python_version()` spells it.
fn python_version(py: Python<'_>) -> String {
    let version = py.version_info();
    format!(
        "{}.{}.{}{}",
        version.major,
        version.minor,
        version.patch,
        version.suffix.unwrap_or_default()
    )
}

/// The engine information as a dict keyed in the facade's field order: the
/// one construction that `info()` and a selftest's `info` share (D41).
fn info_dict<'py>(py: Python<'py>, info: &yamaa::Info) -> PyResult<Bound<'py, PyDict>> {
    let fields = PyDict::new(py);
    fields.set_item("version", info.version)?;
    fields.set_item("language_versions", &info.language_versions)?;
    fields.set_item("features", &info.features)?;
    fields.set_item("host", info.host.as_str())?;
    fields.set_item("host_version", info.host_version.as_deref())?;
    fields.set_item("platform", &info.platform)?;
    Ok(fields)
}

/// What this engine is and where it runs, as the Python host on the running
/// interpreter (D41, P-INFO-1).
#[pyfunction]
pub fn _info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    gate()?;
    info_dict(
        py,
        &yamaa::info(yamaa::Host::Python, Some(&python_version(py))),
    )
}

/// Run every selftest case through this engine as the Python host, with the
/// running interpreter's version.
#[pyfunction]
pub fn _selftest(py: Python<'_>) -> PyResult<SelfTest> {
    gate()?;
    Ok(SelfTest {
        inner: yamaa::selftest(yamaa::Host::Python, Some(&python_version(py))),
        _live: CENSUS.hold(HandleKind::SelfTest),
    })
}

fn project_poisoned() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err("project handle is in use or poisoned")
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProjectRun")]
pub struct ProjectRun {
    inner: Arc<Mutex<yamaa::ProjectRun>>,
    _live: Live,
}

#[pymethods]
impl ProjectRun {
    fn status(&self) -> PyResult<&'static str> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(guard.status().as_str())
    }
    fn names(&self) -> PyResult<Vec<String>> {
        Ok(self
            .inner
            .try_lock()
            .map_err(|_| project_poisoned())?
            .names())
    }
    fn member_status(&self, py: Python<'_>, name: &str) -> PyResult<&'static str> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.status().as_str())
    }
    fn output(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.output().map(Table::from))
    }
    fn member_issues(&self, py: Python<'_>, name: &str) -> PyResult<Table> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(Table::from(&run.issues()))
    }
    fn member_report(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.report().map(Table::from))
    }
    /// One member's printed text, its run's under its name (D37).
    fn member_summary(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        let run = guard.member(name).map_err(|failure| raise(py, failure))?;
        Ok(run.summary().to_string())
    }
    fn issues(&self) -> PyResult<Table> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(Table::from(&guard.issues()))
    }
    fn report(&self) -> PyResult<Table> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(Table::from(&guard.report()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        crate::interrupt::run(py, |token| {
            let saved = self
                .inner
                .try_lock()
                .map_err(|_| project_poisoned())?
                .save(token);
            saved.map_err(|failure| raise(py, failure))
        })
    }
    /// The printed text with one line per member (D37).
    fn summary(&self) -> PyResult<String> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(guard.summary().to_string())
    }
}

#[pyclass(frozen, module = "yamaa._native", name = "_Submission")]
pub struct Submission {
    inner: Arc<Mutex<yamaa::Submission>>,
    _live: Live,
}

#[pymethods]
impl Submission {
    fn status(&self) -> PyResult<&'static str> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(guard.status().as_str())
    }
    /// The creation time the documents carry (D69, D129).
    fn created(&self) -> PyResult<String> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(guard.created().to_owned())
    }
    fn issues(&self) -> PyResult<Table> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(Table::from(&guard.issues()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        crate::interrupt::run(py, |token| {
            let saved = self
                .inner
                .try_lock()
                .map_err(|_| project_poisoned())?
                .save(token);
            saved.map_err(|failure| raise(py, failure))
        })
    }
    /// The printed text (D37).
    fn summary(&self) -> PyResult<String> {
        let guard = self.inner.try_lock().map_err(|_| project_poisoned())?;
        Ok(guard.summary().to_string())
    }
}

#[pyfunction]
pub fn _project_spec(py: Python<'_>, project: &str, name: &str) -> PyResult<String> {
    engine(py)?
        .project_spec(project, name)
        .map_err(|failure| raise(py, failure))
}

/// The listed specifications' names in run order, read without data (D45).
#[pyfunction]
pub fn _project_names(py: Python<'_>, project: &str) -> PyResult<Vec<String>> {
    engine(py)?
        .project_names(project)
        .map_err(|failure| raise(py, failure))
}

#[pyfunction]
pub fn _project_check(py: Python<'_>, project: &str) -> PyResult<Check> {
    crate::interrupt::run(py, |token| {
        engine(py)?
            .project_check(project, token)
            .map(Check::new)
            .map_err(|error| raise_error(py, Action::Read(project), error))
    })
}

#[pyfunction]
#[pyo3(signature=(project, section=None, plans=None))]
pub fn _project_run(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
) -> PyResult<ProjectRun> {
    crate::interrupt::run(py, |token| {
        let engine = engine(py)?;
        let runs = match plans.as_deref() {
            Some(plans) => engine.project_run_reusing(project, section, &plans.inner, token),
            None => engine.project_run(project, section, token),
        }
        // Only an unknown section refuses the call with issues.
        .map_err(|error| raise_error(py, Action::Select(section.unwrap_or_default()), error))?;
        Ok(ProjectRun {
            inner: Arc::new(Mutex::new(runs)),
            _live: CENSUS.hold(HandleKind::ProjectRun),
        })
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
    crate::interrupt::run(py, |token| {
        let submission = engine(py)?
            .project_submission(project, section, created, yamaa::VERSION, token)
            // Only an unknown section refuses the call with issues.
            .map_err(|error| raise_error(py, Action::Select(section.unwrap_or_default()), error))?;
        Ok(Submission {
            inner: Arc::new(Mutex::new(submission)),
            _live: CENSUS.hold(HandleKind::Submission),
        })
    })
}
