//! Native half of the host API: specification operations, their results and
//! one error type. The Python facade keeps paths in lazy objects and calls
//! these functions, which call the `yamaa` facade; behavior, validation and
//! messages stay in Rust.
//!
//! The facade engine of every call is built in one place, [`engine`], from
//! the Python host, the running interpreter's version and, for a call that
//! may run project functions, the installed functions' port.
//!
//! Each call that runs the engine, or reads a result that may run it, runs
//! detached from the interpreter, so that other threads run meanwhile
//! (#4072): its progress callback, project functions and interrupt probe
//! attach again, and its refusal is raised once it has attached.
use pyo3::{prelude::*, types::PyDict};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};
use yamaa::{
    CancellationToken, EngineOptions, Failure, FailureKind, HostLanguage, Partitions,
    ProjectRequest, SpecRequest, SubmissionOptions,
};
use yamaa_boundary::{HandleKind, Live};

use crate::arrow_bridge::Table;
use crate::interface::{gate, CENSUS};
use crate::progress::{Causes, Progress};
use crate::project_activation::Port;

/// Why a call answered no value.
pub(crate) enum Refused {
    /// The engine refused the operation: D39's text, the issues that explain
    /// it and the files a refused save wrote.
    Engine(Failure),
    /// The binding refused the call, for this reason.
    State(String),
    /// The call was cancelled: the interrupt it kept is raised instead.
    Cancelled,
}

/// A facade failure: a cancelled one answers the cancellation, and any other
/// the refusal, with its message, issues and paths.
impl From<Failure> for Refused {
    fn from(failure: Failure) -> Self {
        match failure.kind {
            FailureKind::Cancelled => Refused::Cancelled,
            _ => Refused::Engine(failure),
        }
    }
}

/// A streamed run's refusal of a reader or of its result (#3697), whose text
/// is the failure's, `cannot read <subject>: ...`: a binding refusal, as a
/// busy handle's is.
pub(crate) fn streamed(failure: Failure) -> Refused {
    Refused::Engine(failure)
}

/// Raise `Error` carrying the issues that explain it as an `issues` table,
/// and the files a refused save wrote before it stopped as `paths`; a
/// binding refusal is a StateError, carrying an empty issues table.
pub(crate) fn raise(py: Python<'_>, refusal: Refused) -> PyErr {
    match refusal {
        Refused::Engine(failure) => crate::errors::failure(py, failure),
        Refused::State(message) => crate::errors::message(py, true, message),
        Refused::Cancelled => crate::errors::message(py, true, "the call was cancelled"),
    }
}

/// The running interpreter's full version, as the engine's runtime reports
/// it.
// `Python::version` builds on the PyO3 0.27.2 floor; 0.29 renamed it `version_str`.
#[allow(deprecated)]
fn runtime(py: Python<'_>) -> String {
    py.version().to_owned()
}

/// The partitions every build runs over (#3794, D264): key-bounded over the
/// specification's two leading declared keys, of at most 624,640 driver
/// records each, with the whole-table fallback. The binding depends on the
/// facade alone, so it spells core's `Limits::PARTITION_KEYS` and
/// `Limits::PARTITION_ROWS`, which the facade's `client_partitions.rs` checks.
const PARTITIONS: [usize; 2] = [2, 624_640];

/// [`PARTITIONS`], up to `threads` of one run deriving at once when it is
/// given and not None (#4193); a `threads` that is no positive integer
/// raises `Error`.
fn partitions(threads: Option<&Bound<'_, PyAny>>) -> PyResult<Partitions> {
    let [keys, rows] = PARTITIONS.map(|count| NonZeroUsize::new(count).expect("nonzero"));
    let partitions = Partitions::by_keys(keys, rows).with_whole_table_fallback();
    let Some(threads) = threads else {
        return Ok(partitions);
    };
    partitions
        .with_host_threads(&host_number(threads)?)
        .ok_or_else(|| {
            crate::errors::message(
                threads.py(),
                false,
                "threads must be a positive integer or None",
            )
        })
}

/// The facade engine of one call: the Python host on the interpreter of
/// `version`, over `partitions`, with the installed project functions when
/// `functions`, and reporting to `progress` when one is given (#3762). Every
/// call builds its engine here.
fn engine(
    version: &str,
    functions: bool,
    progress: Option<&Progress>,
    partitions: Partitions,
) -> yamaa::Engine {
    let options = EngineOptions::new(HostLanguage::Python, "python".into(), version.into())
        .with_partitions(partitions);
    let options = match functions {
        true => options.with_functions(Arc::new(Port)),
        false => options,
    };
    yamaa::Engine::new(match progress {
        Some(progress) => {
            let progress = progress.clone();
            options.with_progress(move |event| progress.call(event))
        }
        None => options,
    })
}

/// Run `run` detached from the interpreter, with the interpreter's
/// interrupts behind its token. Once it returns the token is polled once
/// more, so an interrupt that arrived as it returned still cancels it
/// (D186).
fn cancellable<T: Send>(
    py: Python<'_>,
    run: impl Send + FnOnce(&CancellationToken) -> Result<T, Refused>,
) -> PyResult<T> {
    crate::interrupt::run(py, |state| {
        let token = crate::interrupt::token(state);
        py.detach(|| {
            let answer = run(&token);
            if token.is_cancelled() {
                Err(Refused::Cancelled)
            } else {
                answer
            }
        })
        .map_err(|refusal| raise(py, refusal))
    })
}

/// What configures a call's engine rather than its request (#4214), the
/// one `settings` tuple the generated layer passes: its progress callback
/// (#3762) and its `threads` (#4193). None is neither.
type Settings<'py> = Option<(Option<Bound<'py, PyAny>>, Option<Bound<'py, PyAny>>)>;

/// The interpreter's version, the partitions and the progress callback of a
/// call that runs the engine, from its `settings`; a `threads` or a progress
/// callback the call cannot take raises `Error`.
fn configured(
    py: Python<'_>,
    settings: Settings<'_>,
) -> PyResult<(String, Partitions, Option<Progress>, Causes)> {
    let (progress, threads) = settings.unwrap_or((None, None));
    let partitions = partitions(threads.as_ref())?;
    let (progress, causes) = Progress::of(progress)?;
    Ok((runtime(py), partitions, progress, causes))
}

/// Run a cancellable call of the engine, which may call project functions,
/// as its `settings` configure it.
fn called<T: Send>(
    py: Python<'_>,
    settings: Settings<'_>,
    run: impl Send + FnOnce(&yamaa::Engine, &CancellationToken) -> Result<T, Refused>,
) -> PyResult<(T, Causes)> {
    gate()?;
    let (version, partitions, progress, causes) = configured(py, settings)?;
    let value = cancellable(py, |token| {
        run(
            &engine(&version, true, progress.as_ref(), partitions),
            token,
        )
    })?;
    causes.finished(py)?;
    Ok((value, causes))
}

/// Run a call of the engine that reads files and calls no project function,
/// detached from the interpreter.
fn plain<T: Send>(
    py: Python<'_>,
    run: impl Send + FnOnce(&yamaa::Engine) -> Result<T, Refused>,
) -> PyResult<T> {
    gate()?;
    let version = runtime(py);
    let partitions = partitions(None)?;
    py.detach(|| run(&engine(&version, false, None, partitions)))
        .map_err(|refusal| raise(py, refusal))
}

/// A result's accessor, which reaches only the result itself, detached from
/// the interpreter: a streamed run nobody started to read runs whole here.
fn read<T: Send>(py: Python<'_>, read: impl Send + FnOnce() -> Result<T, Refused>) -> PyResult<T> {
    crate::interrupt::run(py, |_| {
        py.detach(read).map_err(|refusal| raise(py, refusal))
    })
}

/// A result, unless another call holds it (D258): `noun` names its kind in
/// words.
fn locked<'a, T>(value: &'a Mutex<T>, noun: &str) -> Result<MutexGuard<'a, T>, Refused> {
    value
        .try_lock()
        .map_err(|_| Refused::State(format!("{noun} handle is in use or poisoned")))
}

/// Save a result with `save`, a cancellable call.
fn save(
    py: Python<'_>,
    save: impl Send + FnOnce(&CancellationToken) -> Result<Vec<String>, Refused>,
) -> PyResult<Vec<String>> {
    cancellable(py, save)
}

/// The request of `specification`, read with `project` when one is given.
fn spec_request<'a>(
    engine: &'a yamaa::Engine,
    specification: &'a str,
    project: Option<&'a str>,
) -> SpecRequest<'a> {
    let request = engine.spec(specification);
    match project {
        Some(project) => request.project(project),
        None => request,
    }
}

/// The request of `project`, its run and submission scoped to `section`
/// when one is given.
fn project_request<'a>(
    engine: &'a yamaa::Engine,
    project: &'a str,
    section: Option<&'a str>,
) -> ProjectRequest<'a> {
    let request = engine.project(project);
    match section {
        Some(section) => request.section(section),
        None => request,
    }
}

/// A run built whole, or a streamed run (#3697), whose result is known once
/// it ran whole.
enum Held {
    Built(Box<Mutex<yamaa::Run>>),
    Streamed(Arc<yamaa::Streamed>),
}

#[pyclass(frozen, module = "yamaa._native", name = "_Run")]
pub struct Run {
    held: Held,
    causes: Causes,
    _live: Live,
}

impl Run {
    fn new(held: Held, causes: Causes) -> Self {
        Self {
            held,
            causes,
            _live: CENSUS.hold(HandleKind::Run),
        }
    }

    /// Read the run's result with `read`. A streamed run nobody started to
    /// read runs whole first; one whose stream has not ended, or that was
    /// cancelled, refuses.
    fn result<T: Send>(
        &self,
        py: Python<'_>,
        read: impl Send + FnOnce(&yamaa::Run) -> T,
    ) -> PyResult<T> {
        self::read(py, || match &self.held {
            Held::Built(run) => Ok(read(&*locked(run, "run")?)),
            Held::Streamed(run) => Ok(read(&*run.run().map_err(streamed)?)),
        })
    }
}

#[pymethods]
impl Run {
    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        self.causes.traverse(visit)
    }

    fn status(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.result(py, |run| run.status().as_str())
    }
    /// The output: a table, none for a failed run, or for a streamed run a
    /// table over its stream until it ran whole.
    fn output(&self, py: Python<'_>) -> PyResult<Option<Table>> {
        let Held::Streamed(run) = &self.held else {
            return self.result(py, |run| run.output().map(Table::from));
        };
        let causes = self.causes.clone_ref(py);
        read(py, || {
            if let yamaa::StreamProgress::Cancelled | yamaa::StreamProgress::Ended = run.progress()
            {
                // A cancelled run's result refuses as cancelled.
                let built = run.run().map_err(streamed)?;
                if built.output().is_some() || built.status() == yamaa::Status::Failed {
                    return Ok(built.output().map(Table::from));
                }
            }
            Ok(Some(Table::stream(Arc::clone(run), causes)))
        })
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |run| Table::from(&run.issues()))
    }
    fn report(&self, py: Python<'_>) -> PyResult<Option<Table>> {
        self.result(py, |run| run.report().map(Table::from))
    }
    /// The time each phase of the run took (#3762).
    fn timings(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |run| Table::from(&run.timings().table()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, |token| match &self.held {
            Held::Built(run) => Ok(locked(run, "run")?.save(token)?),
            Held::Streamed(run) => Ok(run.run().map_err(streamed)?.save(token)?),
        })
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |run| run.summary().to_string())
    }
    /// Private installed-run evidence for qualification suites.
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, py: Python<'_>, include_measurements: bool) -> PyResult<Option<String>> {
        self.result(py, |run| match include_measurements {
            true => Some(run.measured_observations()),
            false => run.observations(),
        })
    }
}

/// The static check of a specification or a project (D36, D37): the facade's
/// result, whose status, issues and printed text the public class returns as
/// they are.
#[pyclass(frozen, module = "yamaa._native", name = "_Check")]
pub struct Check {
    check: Mutex<yamaa::Check>,
    causes: Causes,
    _live: Live,
}

impl Check {
    fn new(check: yamaa::Check, causes: Causes) -> Self {
        Self {
            check: Mutex::new(check),
            causes,
            _live: CENSUS.hold(HandleKind::Check),
        }
    }

    fn result<T: Send>(
        &self,
        py: Python<'_>,
        read: impl Send + FnOnce(&yamaa::Check) -> T,
    ) -> PyResult<T> {
        self::read(py, || Ok(read(&*locked(&self.check, "check")?)))
    }
}

#[pymethods]
impl Check {
    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        self.causes.traverse(visit)
    }

    fn status(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.result(py, |check| check.status().as_str())
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |check| Table::from(&check.issues()))
    }
    fn timings(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |check| Table::from(&check.timings().table()))
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |check| check.summary().to_string())
    }
}

/// The check of a specification, or, with `parent`, of a fragment composed on
/// the specification `parent` names as its parent (#3694).
#[pyfunction]
#[pyo3(signature=(specification, project=None, parent=None, settings=None))]
pub fn _spec_check(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
    parent: Option<&str>,
    settings: Settings<'_>,
) -> PyResult<Check> {
    let (check, causes) = called(py, settings, |engine, token| {
        let request = spec_request(engine, specification, project).cancel_with(token);
        Ok(match parent {
            Some(parent) => request.check_on(parent)?,
            None => request.check()?,
        })
    })?;
    Ok(Check::new(check, causes))
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
/// its runs (#2821); see `yamaa::Plans`. A holder is shared without a lock,
/// so a run started from a project function can use it too.
#[pyclass(frozen, module = "yamaa._native", name = "_Plans")]
pub struct Plans {
    plans: yamaa::Plans,
    _live: Live,
}

/// An empty holder for one lazy object. Creating it reads nothing.
#[pyfunction]
pub fn _plans() -> PyResult<Plans> {
    gate()?;
    Ok(Plans {
        plans: yamaa::Plans::new(),
        _live: CENSUS.hold(HandleKind::Plans),
    })
}

/// A run of one specification: built now, or with `stream` built as its
/// output is read (#3697), polling a token of its own from then on.
#[pyfunction]
#[pyo3(signature=(specification, project=None, plans=None, stream=false, inputs=None, settings=None))]
pub fn _spec_run(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
    stream: bool,
    inputs: Option<Bound<'_, PyAny>>,
    settings: Settings<'_>,
) -> PyResult<Run> {
    gate()?;
    let inputs = crate::input_frames::import(specification, inputs)?;
    let plans = plans.as_deref().map(|plans| &plans.plans);
    if stream {
        gate()?;
        let (version, partitions, progress, causes) = configured(py, settings)?;
        let token = crate::interrupt::stream_token();
        let run = crate::interrupt::run(py, |_| {
            py.detach(|| {
                let engine = engine(&version, true, progress.as_ref(), partitions);
                let request = spec_request(&engine, specification, project)
                    .cancel_with(&token)
                    .inputs(inputs);
                let run = match plans {
                    Some(plans) => request.reusing(plans),
                    None => request,
                }
                .stream();
                // As a built run's call, the start polls its token once more.
                if token.is_cancelled() {
                    return Err(Refused::Cancelled);
                }
                run.map_err(Refused::from)
            })
            .map_err(|refusal| raise(py, refusal))
        })?;
        return Ok(Run::new(Held::Streamed(Arc::new(run)), causes));
    }
    let (run, causes) = called(py, settings, |engine, token| {
        let request = spec_request(engine, specification, project)
            .cancel_with(token)
            .inputs(inputs);
        Ok(match plans {
            Some(plans) => request.reusing(plans),
            None => request,
        }
        .run()?)
    })?;
    Ok(Run::new(Held::Built(Box::new(Mutex::new(run))), causes))
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_resolve<'py>(
    py: Python<'py>,
    specification: &str,
    project: Option<&str>,
) -> PyResult<Bound<'py, PyAny>> {
    let value = plain(py, |engine| {
        Ok(spec_request(engine, specification, project).resolve()?)
    })?;
    crate::context_values::context_value(py, &value)
}

#[pyfunction]
#[pyo3(signature=(source, archive_member, max_archive_members, max_expanded_bytes, max_xml_depth))]
pub fn _read_odm(
    py: Python<'_>,
    source: &str,
    archive_member: Option<String>,
    max_archive_members: &Bound<'_, PyAny>,
    max_expanded_bytes: &Bound<'_, PyAny>,
    max_xml_depth: &Bound<'_, PyAny>,
) -> PyResult<Table> {
    gate()?;
    let members = host_number(max_archive_members)?;
    let bytes = host_number(max_expanded_bytes)?;
    let depth = host_number(max_xml_depth)?;
    let items = plain(py, |engine| {
        let mut options = yamaa::OdmOptions::from_host_limits(source, members, bytes, depth)?;
        if let Some(member) = archive_member {
            options = options.with_archive_member(member);
        }
        Ok(engine.read_odm(source, &options)?)
    })?;
    Ok(Table::from(&items))
}

/// A number a host argument gives, an ODM limit or `threads`: its kind and
/// exact integer spelling preserved until the facade admits it.
fn host_number(value: &Bound<'_, PyAny>) -> PyResult<yamaa::ContextValue> {
    use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
    use yamaa::ContextValue as V;
    Ok(if value.is_none() {
        V::Null
    } else if value.is_instance_of::<PyBool>() {
        V::Bool(value.extract()?)
    } else if value.is_instance_of::<PyInt>() {
        value.extract::<i64>().map_or_else(
            |_| {
                value
                    .extract::<num_bigint::BigInt>()
                    .map(|integer| V::Wide(integer.to_string()))
            },
            |value| Ok(V::Int(value)),
        )?
    } else if value.is_instance_of::<PyFloat>() {
        V::Float(value.extract()?)
    } else if value.is_instance_of::<PyString>() {
        V::Text(value.extract()?)
    } else if let Ok(integer) = value.extract::<i64>() {
        V::Int(integer)
    } else {
        V::Text("non-numeric value".into())
    })
}

/// The lint result of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted raises `Error`.
#[pyfunction]
#[pyo3(signature=(paths, fix=false))]
pub fn _lint(py: Python<'_>, paths: Vec<String>, fix: bool) -> PyResult<Lint> {
    let result = plain(py, |_| Ok(yamaa::lint(&paths, fix)?))?;
    Ok(Lint {
        result,
        _live: CENSUS.hold(HandleKind::Lint),
    })
}

/// Immutable lint findings and rewritten paths, held after the files change.
#[pyclass(frozen, module = "yamaa._native", name = "_Lint")]
pub struct Lint {
    result: yamaa::Lint,
    _live: Live,
}

#[pymethods]
impl Lint {
    fn status(&self) -> &'static str {
        self.result.status().as_str()
    }
    fn findings(&self) -> Table {
        Table::from(&self.result.table())
    }
    fn paths(&self) -> Vec<String> {
        self.result.rewritten().to_vec()
    }
    fn summary(&self) -> String {
        self.result.summary()
    }
}

/// The JSON Schema of a file type as canonical JSON text (#3030 D149); a
/// type the facade does not know raises `Error`.
#[pyfunction]
pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {
    plain(py, |_| Ok(yamaa::schema(file_type)?))
}

/// The condition catalogue (#3700): every condition the engine may report,
/// and every condition a release retired, as a table.
#[pyfunction]
pub fn _conditions() -> PyResult<Table> {
    gate()?;
    Ok(Table::from(&yamaa::conditions().table()))
}

/// The installation self-test (#3030 D28, D119): the facade's result, whose
/// status, cases, engine information and printed report the public class
/// returns as they are.
#[pyclass(frozen, module = "yamaa._native", name = "_SelfTest")]
pub struct SelfTest {
    selftest: yamaa::SelfTest,
    _live: Live,
}

#[pymethods]
impl SelfTest {
    fn status(&self) -> &'static str {
        self.selftest.status().as_str()
    }
    fn cases(&self) -> Table {
        Table::from(&self.selftest.table())
    }
    /// The engine information that ran the cases, as `info()` gives it.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        info_dict(py, self.selftest.info())
    }
    fn summary(&self) -> String {
        self.selftest.summary().to_string()
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
    let version = python_version(py);
    info_dict(py, &yamaa::info(yamaa::Host::Python, Some(&version)))
}

/// Run every selftest case through this engine as the Python host, with the
/// running interpreter's version.
#[pyfunction]
pub fn _selftest(py: Python<'_>) -> PyResult<SelfTest> {
    gate()?;
    let version = python_version(py);
    let selftest = py.detach(|| yamaa::selftest(yamaa::Host::Python, Some(&version)));
    Ok(SelfTest {
        selftest,
        _live: CENSUS.hold(HandleKind::SelfTest),
    })
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProjectRun")]
pub struct ProjectRun {
    runs: Mutex<yamaa::ProjectRun>,
    causes: Causes,
    _live: Live,
}

impl ProjectRun {
    fn result<T: Send>(
        &self,
        py: Python<'_>,
        read: impl Send + FnOnce(&yamaa::ProjectRun) -> Result<T, Refused>,
    ) -> PyResult<T> {
        self::read(py, || read(&*locked(&self.runs, "project run")?))
    }

    /// Read the member `name` of the project run with `read`.
    fn member<T: Send>(
        &self,
        py: Python<'_>,
        name: &str,
        read: impl Send + FnOnce(&yamaa::Run) -> T,
    ) -> PyResult<T> {
        self.result(py, |runs| Ok(read(runs.member(name)?)))
    }
}

#[pymethods]
impl ProjectRun {
    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        self.causes.traverse(visit)
    }

    fn status(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.result(py, |runs| Ok(runs.status().as_str()))
    }
    fn names(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        self.result(py, |runs| Ok(runs.names()))
    }
    fn member_status(&self, py: Python<'_>, name: &str) -> PyResult<&'static str> {
        self.member(py, name, |run| run.status().as_str())
    }
    fn output(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        self.member(py, name, |run| run.output().map(Table::from))
    }
    fn member_issues(&self, py: Python<'_>, name: &str) -> PyResult<Table> {
        self.member(py, name, |run| Table::from(&run.issues()))
    }
    fn member_report(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        self.member(py, name, |run| run.report().map(Table::from))
    }
    fn member_timings(&self, py: Python<'_>, name: &str) -> PyResult<Table> {
        self.member(py, name, |run| Table::from(&run.timings().table()))
    }
    /// One member's printed text, its run's under its name (D37).
    fn member_summary(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.member(py, name, |run| run.summary().to_string())
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |runs| Ok(Table::from(&runs.issues())))
    }
    fn report(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |runs| Ok(Table::from(&runs.report())))
    }
    fn timings(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |runs| Ok(Table::from(&runs.timings().table())))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, |token| {
            Ok(locked(&self.runs, "project run")?.save(token)?)
        })
    }
    /// The printed text with one line per member (D37).
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |runs| Ok(runs.summary().to_string()))
    }
}

#[pyclass(frozen, module = "yamaa._native", name = "_Submission")]
pub struct Submission {
    submission: Mutex<yamaa::Submission>,
    causes: Causes,
    _live: Live,
}

impl Submission {
    fn result<T: Send>(
        &self,
        py: Python<'_>,
        read: impl Send + FnOnce(&yamaa::Submission) -> T,
    ) -> PyResult<T> {
        self::read(py, || Ok(read(&*locked(&self.submission, "submission")?)))
    }
}

#[pymethods]
impl Submission {
    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        self.causes.traverse(visit)
    }

    fn status(&self, py: Python<'_>) -> PyResult<&'static str> {
        self.result(py, |submission| submission.status().as_str())
    }
    /// The creation time the documents carry (D69, D129).
    fn created(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |submission| submission.created().to_owned())
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |submission| Table::from(&submission.issues()))
    }
    fn timings(&self, py: Python<'_>) -> PyResult<Table> {
        self.result(py, |submission| Table::from(&submission.timings().table()))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, |token| {
            Ok(locked(&self.submission, "submission")?.save(token)?)
        })
    }
    /// The printed text (D37).
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        self.result(py, |submission| submission.summary().to_string())
    }
}

#[pyfunction]
pub fn _project_spec(py: Python<'_>, project: &str, name: &str) -> PyResult<String> {
    plain(py, |engine| {
        Ok(engine.project(project).spec(name)?.path().to_owned())
    })
}

/// The listed specifications' names in run order, read without data (D45).
#[pyfunction]
pub fn _project_names(py: Python<'_>, project: &str) -> PyResult<Vec<String>> {
    plain(py, |engine| Ok(engine.project(project).names()?))
}

#[pyfunction]
#[pyo3(signature=(project, settings=None))]
pub fn _project_check(py: Python<'_>, project: &str, settings: Settings<'_>) -> PyResult<Check> {
    let (check, causes) = called(py, settings, |engine, token| {
        Ok(engine.project(project).cancel_with(token).check()?)
    })?;
    Ok(Check::new(check, causes))
}

#[pyfunction]
#[pyo3(signature=(project, section=None, plans=None, settings=None))]
pub fn _project_run(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
    settings: Settings<'_>,
) -> PyResult<ProjectRun> {
    let plans = plans.as_deref().map(|plans| &plans.plans);
    let (runs, causes) = called(py, settings, |engine, token| {
        let request = project_request(engine, project, section).cancel_with(token);
        // Only an unknown section refuses the call with issues.
        Ok(match plans {
            Some(plans) => request.reusing(plans),
            None => request,
        }
        .run()?)
    })?;
    Ok(ProjectRun {
        runs: Mutex::new(runs),
        causes,
        _live: CENSUS.hold(HandleKind::ProjectRun),
    })
}

#[pyfunction]
#[pyo3(signature=(project, section=None, created=None, settings=None))]
pub fn _project_submission(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    created: Option<&str>,
    settings: Settings<'_>,
) -> PyResult<Submission> {
    let mut options = SubmissionOptions::new(yamaa::VERSION);
    options.created = created.map(str::to_owned);
    let (submission, causes) = called(py, settings, |engine, token| {
        Ok(project_request(engine, project, section)
            .cancel_with(token)
            .submission(&options)?)
    })?;
    Ok(Submission {
        submission: Mutex::new(submission),
        causes,
        _live: CENSUS.hold(HandleKind::Submission),
    })
}
