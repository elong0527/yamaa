//! Host marshalling for owned build results and explicit save authority.
use pyo3::{
    exceptions::{PyBaseException, PyKeyboardInterrupt, PySystemExit},
    prelude::*,
    types::{PyBytes, PyString, PyTuple},
};
use std::sync::Arc;
use yamaa_adapters::{
    specification_report::{self, Identity},
    specification_run::{CapturedAttempt, PortError, PreparedRun, SourcePort},
};
use yamaa_core::resource::ResourceFailure;

pub(super) struct CaptureError {
    error: PyErr,
    failure: Option<ResourceFailure>,
}
impl From<PyErr> for CaptureError {
    fn from(error: PyErr) -> Self {
        Self {
            error,
            failure: None,
        }
    }
}

fn failure_reply(result: &Bound<'_, PyAny>) -> PyResult<CaptureError> {
    let result = result.cast::<PyTuple>()?;
    if result.len() != 2 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "invalid resource failure reply",
        ));
    }
    let kind = result.get_item(0)?;
    let kind = kind.cast::<PyString>()?;
    let failure = match kind.to_str()? {
        "missing" => ResourceFailure::Missing,
        "not_regular_file" => ResourceFailure::NotRegularFile,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid capture failure kind",
            ))
        }
    };
    let payload = result.get_item(1)?;
    let payload = payload.cast::<PyBaseException>()?;
    let interruption =
        payload.is_instance_of::<PyKeyboardInterrupt>() || payload.is_instance_of::<PySystemExit>();
    Ok(CaptureError {
        error: PyErr::from_value(payload.clone().into_any()),
        failure: (!interruption).then_some(failure),
    })
}

pub(super) fn fields(metadata: &Bound<'_, PyTuple>) -> PyResult<Vec<String>> {
    if metadata.len() != 5 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "invalid report metadata",
        ));
    }
    metadata
        .iter()
        .map(|item| {
            let text = item.cast::<PyString>()?.to_str()?;
            if text.len() > 4096 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "report metadata limit",
                ));
            }
            Ok(text.to_owned())
        })
        .collect()
}
pub(super) fn identity(fields: &[String]) -> Identity<'_> {
    Identity {
        runtime: "python",
        runtime_version: &fields[0],
        engine_version: &fields[1],
        example: &fields[2],
        specification: &fields[3],
        base_directory: &fields[4],
    }
}
struct Port<'a, 'py> {
    capture: &'a Bound<'py, PyAny>,
    inspect: Option<&'a Bound<'py, PyAny>>,
    reads: usize,
}
impl SourcePort for Port<'_, '_> {
    type Error = CaptureError;
    fn resource_failure(&self, error: &CaptureError) -> Option<ResourceFailure> {
        error.failure
    }
    fn inspect(
        &mut self,
        source: &yamaa_engine::specification::SourceDeclaration,
    ) -> Result<(), CaptureError> {
        if let Some(inspect) = self.inspect {
            let result = inspect.call1((&source.name, &source.path))?;
            if !result.is_none() {
                return Err(failure_reply(&result)?);
            }
        }
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(
        &mut self,
        source: &yamaa_engine::specification::SourceDeclaration,
        maximum: usize,
    ) -> Result<Arc<[u8]>, CaptureError> {
        let result = self.capture.call1((&source.name, &source.path, maximum))?;
        let result = result.cast::<PyTuple>().map_err(PyErr::from)?;
        if result.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err("invalid capture response").into());
        }
        let content = result.get_item(0)?;
        if content.cast::<PyString>().is_ok() {
            return Err(failure_reply(result.as_any())?);
        }
        let content = content.cast::<PyBytes>().map_err(PyErr::from)?;
        let created = result.get_item(1)?.extract::<bool>()?;
        if content.as_bytes().len() > maximum {
            return Err(pyo3::exceptions::PyValueError::new_err("capture byte limit").into());
        }
        self.reads += usize::from(created);
        Ok(Arc::from(content.as_bytes()))
    }
}
pub(super) fn capture_attempt(
    run: &PreparedRun,
    capture: &Bound<'_, PyAny>,
    inspect: Option<&Bound<'_, PyAny>>,
) -> PyResult<CapturedAttempt<CaptureError>> {
    if !capture.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "capture must be callable",
        ));
    }
    if inspect.is_some_and(|value| !value.is_callable()) {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "inspect must be callable",
        ));
    }
    let attempt = run.execute_with_port(&mut Port {
        capture,
        inspect,
        reads: 0,
    });
    if let Err(PortError::Capture(error)) = &attempt.result {
        if error.failure.is_none() {
            return Err(error.error.clone_ref(capture.py()));
        }
    }
    if let Err(PortError::Inspect(errors)) = &attempt.result {
        if let Some(error) = errors.iter().find(|error| error.failure.is_none()) {
            return Err(error.error.error.clone_ref(capture.py()));
        }
    }
    Ok(attempt)
}
pub(super) struct Publisher<'a, 'py>(pub(super) &'a Bound<'py, PyAny>);
impl specification_report::ArtifactPort for Publisher<'_, '_> {
    type Error = PyErr;
    fn publish(&mut self, path: &str, content: &[u8]) -> PyResult<()> {
        let result = self.0.call1((path, PyBytes::new(self.0.py(), content)))?;
        if !result.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "publisher must return None",
            ));
        }
        Ok(())
    }
}
#[pyclass(frozen, module = "yamaa._native", name = "_BuildResult")]
pub struct BuildResult {
    pub(super) inner: specification_report::BuildResult,
}
pub(super) type IssueRow = (String, String, Option<String>, Vec<String>, String);
pub(super) fn issue_rows(issues: &[yamaa_adapters::issue_rows::Issue]) -> Vec<IssueRow> {
    issues
        .iter()
        .map(|issue| {
            (
                issue.phase.clone(),
                issue.condition.clone(),
                issue.requirement.clone(),
                issue.spec_paths.clone(),
                issue.context.clone(),
            )
        })
        .collect()
}
#[cfg(unix)]
struct FileTarget<'a> {
    declared: &'a str,
    target: &'a str,
}
#[cfg(unix)]
impl specification_report::ArtifactPort for FileTarget<'_> {
    type Error = yamaa_adapters::file_publication::Error;
    fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        yamaa_adapters::file_publication::Publisher::new(self.declared, self.target)?
            .publish(path, content)
    }
}
#[pymethods]
impl BuildResult {
    fn issues(&self) -> Vec<IssueRow> {
        issue_rows(self.inner.issues())
    }
    #[cfg(unix)]
    fn save_file(&self, declared: &str, target: &str) -> PyResult<String> {
        let mut publisher = FileTarget { declared, target };
        self.inner
            .save(&mut publisher)
            .map(|report| report.to_string())
            .map_err(|error| match error {
                yamaa_engine::specification_output::SaveError::FailedBuild => {
                    pyo3::exceptions::PyValueError::new_err("cannot save a failed build")
                }
                yamaa_engine::specification_output::SaveError::Publish(error) => {
                    pyo3::exceptions::PyValueError::new_err(error.message())
                }
            })
    }
    fn output<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.inner.output().map(|bytes| PyBytes::new(py, bytes))
    }
    fn observations(&self) -> String {
        self.inner.observations().to_string()
    }
    fn save(&self, publish: &Bound<'_, PyAny>) -> PyResult<String> {
        if !publish.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "publish must be callable",
            ));
        }
        self.inner
            .save(&mut Publisher(publish))
            .map(|report| report.to_string())
            .map_err(|error| match error {
                yamaa_engine::specification_output::SaveError::FailedBuild => {
                    pyo3::exceptions::PyValueError::new_err("cannot save a failed build")
                }
                yamaa_engine::specification_output::SaveError::Publish(error) => error,
            })
    }
}
pub(super) fn build(
    run: &PreparedRun,
    capture: &Bound<'_, PyAny>,
    metadata: &Bound<'_, PyTuple>,
    inspect: Option<&Bound<'_, PyAny>>,
) -> PyResult<BuildResult> {
    let fields = fields(metadata)?;
    let attempt = capture_attempt(run, capture, inspect)?;
    let inner =
        specification_report::build_result(run, &attempt, identity(&fields)).map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("unsupported or invalid build observation")
        })?;
    Ok(BuildResult { inner })
}
