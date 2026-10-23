//! Native half of the host API: specification operations, their results and
//! one error type. The Python facade keeps paths in lazy objects and calls
//! these functions, which reach the engine through the boundary's client;
//! behavior, validation and messages stay in Rust.
use pyo3::{exceptions::PyException, exceptions::PyRuntimeError, prelude::*, types::PyDict};
use yamaa_boundary::client::{Call, Engine, Info, Language, OdmOptions, Owned, Refused};

use crate::arrow_bridge::Table;
use crate::interface::gate;
use crate::project_activation::Port;

pyo3::create_exception!(yamaa, _Error, PyException);

/// Raise `_Error` carrying the issues that explain it as an `issues` table,
/// and the files a refused save wrote before it stopped as `paths`; a
/// boundary refusal is a `RuntimeError`, as a busy handle's is.
fn raise(py: Python<'_>, refusal: Refused) -> PyErr {
    match refusal {
        Refused::Engine {
            message,
            issues,
            paths,
        } => {
            let error = _Error::new_err(message);
            let value = error.value(py);
            let issues = issues.map(Table::new);
            match value
                .setattr("issues", issues)
                .and_then(|()| value.setattr("paths", paths))
            {
                Ok(()) => error,
                Err(setattr) => setattr,
            }
        }
        Refused::Boundary(message) => PyRuntimeError::new_err(message),
        // The interrupt the call kept is raised in preference to this.
        Refused::Cancelled => PyRuntimeError::new_err("the call was cancelled"),
    }
}

/// The running interpreter's full version, as the engine's runtime reports
/// it.
// `Python::version` builds on the PyO3 0.27.2 floor; 0.29 renamed it `version_str`.
#[allow(deprecated)]
fn runtime(py: Python<'_>) -> String {
    py.version().to_owned()
}

/// Run a call of the engine with the interpreter's interrupts and project
/// functions behind its callbacks.
fn called<T>(
    py: Python<'_>,
    functions: bool,
    run: impl FnOnce(Engine, &Call<'_>) -> Result<T, Refused>,
) -> PyResult<T> {
    let engine = gate()?;
    let version = runtime(py);
    crate::interrupt::run(py, |state| {
        let host = Port::new(state);
        let call = Call::new(Language::Python, "python", &version).with(&host, functions);
        run(engine, &call).map_err(|refusal| raise(py, refusal))
    })
}

/// Run a call of the engine that reads files and calls no project function.
fn plain<T>(
    py: Python<'_>,
    run: impl FnOnce(Engine, &Call<'_>) -> Result<T, Refused>,
) -> PyResult<T> {
    let engine = gate()?;
    let version = runtime(py);
    let call = Call::new(Language::Python, "python", &version);
    run(engine, &call).map_err(|refusal| raise(py, refusal))
}

/// A result's accessor, which reaches only the result itself.
fn read<T>(
    py: Python<'_>,
    result: &Owned,
    read: impl FnOnce(Engine) -> Result<T, Refused>,
) -> PyResult<T> {
    read(result.engine()).map_err(|refusal| raise(py, refusal))
}

/// Save a result, with the interpreter's interrupts behind the call's probe.
fn save(py: Python<'_>, result: &Owned) -> PyResult<Vec<String>> {
    let version = runtime(py);
    crate::interrupt::run(py, |state| {
        let host = Port::new(state);
        let call = Call::new(Language::Python, "python", &version).with(&host, false);
        let saved = result.engine().save(&call, result.handle());
        saved.map_err(|refusal| raise(py, refusal))
    })
}

#[pyclass(frozen, module = "yamaa._native", name = "_Run")]
pub struct Run {
    run: Owned,
}

#[pymethods]
impl Run {
    fn status(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.run, |engine| {
            engine.status(self.run.handle(), None)
        })
    }
    fn output(&self, py: Python<'_>) -> PyResult<Option<Table>> {
        let output = read(py, &self.run, |engine| {
            engine.output(self.run.handle(), None)
        })?;
        Ok(output.map(Table::new))
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        read(py, &self.run, |engine| {
            engine.issues(self.run.handle(), None)
        })
        .map(Table::new)
    }
    fn report(&self, py: Python<'_>) -> PyResult<Option<Table>> {
        let report = read(py, &self.run, |engine| {
            engine.report(self.run.handle(), None)
        })?;
        Ok(report.map(Table::new))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, &self.run)
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        let lines = read(py, &self.run, |engine| {
            engine.summary(self.run.handle(), None)
        })?;
        Ok(lines.join("\n"))
    }
    /// Private installed-run evidence for qualification suites.
    #[pyo3(signature=(include_measurements=false))]
    fn observations(&self, py: Python<'_>, include_measurements: bool) -> PyResult<Option<String>> {
        read(py, &self.run, |engine| {
            engine.observations(self.run.handle(), include_measurements)
        })
    }
}

/// The static check of a specification or a project (D36, D37): the engine's
/// result, whose status, issues and printed text the public class returns as
/// they are.
#[pyclass(frozen, module = "yamaa._native", name = "_Check")]
pub struct Check {
    check: Owned,
}

#[pymethods]
impl Check {
    fn status(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.check, |engine| {
            engine.status(self.check.handle(), None)
        })
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        read(py, &self.check, |engine| {
            engine.issues(self.check.handle(), None)
        })
        .map(Table::new)
    }
    /// The printed text (D37), which `repr()` returns.
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        let lines = read(py, &self.check, |engine| {
            engine.summary(self.check.handle(), None)
        })?;
        Ok(lines.join("\n"))
    }
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_check(py: Python<'_>, specification: &str, project: Option<&str>) -> PyResult<Check> {
    let check = called(py, true, |engine, call| {
        engine.spec_check(call, specification, project)
    })?;
    Ok(Check { check })
}

/// The printed text of a lazy specification object, which has read nothing
/// (D37): `yamaa spec <path>`, with the path as given.
#[pyfunction]
pub fn _spec_summary(py: Python<'_>, specification: &str) -> PyResult<String> {
    let lines = plain(py, |engine, _| engine.spec_summary(specification))?;
    Ok(lines.join("\n"))
}

/// The printed text of a lazy project object: `yamaa project <path>`.
#[pyfunction]
pub fn _project_summary(py: Python<'_>, project: &str) -> PyResult<String> {
    let lines = plain(py, |engine, _| engine.project_summary(project))?;
    Ok(lines.join("\n"))
}

/// The compiled plans one lazy specification or project object keeps between
/// its runs (#2821), an engine plans holder.
#[pyclass(frozen, module = "yamaa._native", name = "_Plans")]
pub struct Plans {
    plans: Owned,
}

/// An empty holder for one lazy object. Creating it reads nothing.
#[pyfunction]
pub fn _plans(py: Python<'_>) -> PyResult<Plans> {
    let plans = plain(py, |engine, _| engine.plans())?;
    Ok(Plans { plans })
}

#[pyfunction]
#[pyo3(signature=(specification, project=None, plans=None))]
pub fn _spec_run(
    py: Python<'_>,
    specification: &str,
    project: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
) -> PyResult<Run> {
    let plans = plans.as_deref().map_or(0, |plans| plans.plans.handle());
    let run = called(py, true, |engine, call| {
        engine.spec_run(call, specification, project, plans)
    })?;
    Ok(Run { run })
}

#[pyfunction]
#[pyo3(signature=(specification, project=None))]
pub fn _spec_resolve<'py>(
    py: Python<'py>,
    specification: &str,
    project: Option<&str>,
) -> PyResult<Bound<'py, PyAny>> {
    let value = plain(py, |engine, call| {
        engine.spec_resolve(call, specification, project)
    })?;
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
    let limit = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
    let options = OdmOptions {
        archive_member: archive_member.as_deref(),
        max_archive_members: limit(max_archive_members),
        max_expanded_bytes: limit(max_expanded_bytes),
        max_xml_depth: limit(max_xml_depth),
    };
    plain(py, |engine, call| engine.read_odm(call, source, &options)).map(Table::new)
}

/// The lint table of every file `paths` names, fixing them first when `fix`
/// is true; a file that cannot be read or admitted raises `_Error`.
#[pyfunction]
#[pyo3(signature=(paths, fix=false))]
pub fn _lint(py: Python<'_>, paths: Vec<String>, fix: bool) -> PyResult<Table> {
    plain(py, |engine, _| engine.lint(&paths, fix)).map(Table::new)
}

/// The JSON Schema of a file type as canonical JSON text (#3030 D149); a
/// type the facade does not know raises `_Error`.
#[pyfunction]
pub fn _schema(py: Python<'_>, file_type: &str) -> PyResult<String> {
    plain(py, |engine, _| engine.schema(file_type))
}

/// The installation self-test (#3030 D28, D119): the engine's result, whose
/// status, cases, engine information and printed report the public class
/// returns as they are.
#[pyclass(frozen, module = "yamaa._native", name = "_SelfTest")]
pub struct SelfTest {
    selftest: Owned,
}

#[pymethods]
impl SelfTest {
    fn status(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.selftest, |engine| {
            engine.status(self.selftest.handle(), None)
        })
    }
    fn cases(&self, py: Python<'_>) -> PyResult<Table> {
        read(py, &self.selftest, |engine| {
            engine.cases(self.selftest.handle())
        })
        .map(Table::new)
    }
    /// The engine information that ran the cases, as `info()` gives it.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let info = read(py, &self.selftest, |engine| {
            engine.selftest_info(self.selftest.handle())
        })?;
        info_dict(py, &info)
    }
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        let lines = read(py, &self.selftest, |engine| {
            engine.summary(self.selftest.handle(), None)
        })?;
        Ok(lines.join("\n"))
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
fn info_dict<'py>(py: Python<'py>, info: &Info) -> PyResult<Bound<'py, PyDict>> {
    let fields = PyDict::new(py);
    fields.set_item("version", &info.version)?;
    fields.set_item("language_versions", &info.language_versions)?;
    fields.set_item("features", &info.features)?;
    fields.set_item("host", &info.host)?;
    fields.set_item("host_version", info.host_version.as_deref())?;
    fields.set_item("platform", &info.platform)?;
    Ok(fields)
}

/// What this engine is and where it runs, as the Python host on the running
/// interpreter (D41, P-INFO-1).
#[pyfunction]
pub fn _info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let engine = gate()?;
    let version = python_version(py);
    let info = engine.info(&Call::new(Language::Python, "python", &version));
    info_dict(py, &info.map_err(|refusal| raise(py, refusal))?)
}

/// Run every selftest case through this engine as the Python host, with the
/// running interpreter's version.
#[pyfunction]
pub fn _selftest(py: Python<'_>) -> PyResult<SelfTest> {
    let engine = gate()?;
    let version = python_version(py);
    let selftest = engine.selftest(&Call::new(Language::Python, "python", &version));
    let selftest = selftest.map_err(|refusal| raise(py, refusal))?;
    Ok(SelfTest { selftest })
}

#[pyclass(frozen, module = "yamaa._native", name = "_ProjectRun")]
pub struct ProjectRun {
    runs: Owned,
}

#[pymethods]
impl ProjectRun {
    fn status(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.runs, |engine| {
            engine.status(self.runs.handle(), None)
        })
    }
    fn names(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        read(py, &self.runs, |engine| engine.names(self.runs.handle()))
    }
    fn member_status(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        read(py, &self.runs, |engine| {
            engine.status(self.runs.handle(), Some(name))
        })
    }
    fn output(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        let output = read(py, &self.runs, |engine| {
            engine.output(self.runs.handle(), Some(name))
        })?;
        Ok(output.map(Table::new))
    }
    fn member_issues(&self, py: Python<'_>, name: &str) -> PyResult<Table> {
        read(py, &self.runs, |engine| {
            engine.issues(self.runs.handle(), Some(name))
        })
        .map(Table::new)
    }
    fn member_report(&self, py: Python<'_>, name: &str) -> PyResult<Option<Table>> {
        let report = read(py, &self.runs, |engine| {
            engine.report(self.runs.handle(), Some(name))
        })?;
        Ok(report.map(Table::new))
    }
    /// One member's printed text, its run's under its name (D37).
    fn member_summary(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        let lines = read(py, &self.runs, |engine| {
            engine.summary(self.runs.handle(), Some(name))
        })?;
        Ok(lines.join("\n"))
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        read(py, &self.runs, |engine| {
            engine.issues(self.runs.handle(), None)
        })
        .map(Table::new)
    }
    fn report(&self, py: Python<'_>) -> PyResult<Table> {
        let report = read(py, &self.runs, |engine| {
            engine.report(self.runs.handle(), None)
        })?;
        report
            .map(Table::new)
            .ok_or_else(|| PyRuntimeError::new_err("a project run has a report"))
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, &self.runs)
    }
    /// The printed text with one line per member (D37).
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        let lines = read(py, &self.runs, |engine| {
            engine.summary(self.runs.handle(), None)
        })?;
        Ok(lines.join("\n"))
    }
}

#[pyclass(frozen, module = "yamaa._native", name = "_Submission")]
pub struct Submission {
    submission: Owned,
}

#[pymethods]
impl Submission {
    fn status(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.submission, |engine| {
            engine.status(self.submission.handle(), None)
        })
    }
    /// The creation time the documents carry (D69, D129).
    fn created(&self, py: Python<'_>) -> PyResult<String> {
        read(py, &self.submission, |engine| {
            engine.created(self.submission.handle())
        })
    }
    fn issues(&self, py: Python<'_>) -> PyResult<Table> {
        read(py, &self.submission, |engine| {
            engine.issues(self.submission.handle(), None)
        })
        .map(Table::new)
    }
    fn save(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        save(py, &self.submission)
    }
    /// The printed text (D37).
    fn summary(&self, py: Python<'_>) -> PyResult<String> {
        let lines = read(py, &self.submission, |engine| {
            engine.summary(self.submission.handle(), None)
        })?;
        Ok(lines.join("\n"))
    }
}

#[pyfunction]
pub fn _project_spec(py: Python<'_>, project: &str, name: &str) -> PyResult<String> {
    plain(py, |engine, call| engine.project_spec(call, project, name))
}

/// The listed specifications' names in run order, read without data (D45).
#[pyfunction]
pub fn _project_names(py: Python<'_>, project: &str) -> PyResult<Vec<String>> {
    plain(py, |engine, call| engine.project_names(call, project))
}

#[pyfunction]
pub fn _project_check(py: Python<'_>, project: &str) -> PyResult<Check> {
    let check = called(py, true, |engine, call| engine.project_check(call, project))?;
    Ok(Check { check })
}

#[pyfunction]
#[pyo3(signature=(project, section=None, plans=None))]
pub fn _project_run(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    plans: Option<PyRef<'_, Plans>>,
) -> PyResult<ProjectRun> {
    let plans = plans.as_deref().map_or(0, |plans| plans.plans.handle());
    let runs = called(py, true, |engine, call| {
        engine.project_run(call, project, section, plans)
    })?;
    Ok(ProjectRun { runs })
}

#[pyfunction]
#[pyo3(signature=(project, section=None, created=None))]
pub fn _project_submission(
    py: Python<'_>,
    project: &str,
    section: Option<&str>,
    created: Option<&str>,
) -> PyResult<Submission> {
    let submission = called(py, true, |engine, call| {
        engine.project_submission(call, project, section, created)
    })?;
    Ok(Submission { submission })
}
