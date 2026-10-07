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
    reads: usize,
}
impl SourcePort for Port<'_, '_> {
    type Error = CaptureError;
    fn resource_failure(&self, error: &CaptureError) -> Option<ResourceFailure> {
        error.failure
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
        if let Ok(kind) = content.cast::<PyString>() {
            let failure = match kind.to_str()? {
                "missing" => ResourceFailure::Missing,
                "not_regular_file" => ResourceFailure::NotRegularFile,
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "invalid capture failure kind",
                    )
                    .into())
                }
            };
            let payload = result.get_item(1)?;
            let payload = payload.cast::<PyBaseException>().map_err(PyErr::from)?;
            let interruption = payload.is_instance_of::<PyKeyboardInterrupt>()
                || payload.is_instance_of::<PySystemExit>();
            return Err(CaptureError {
                error: PyErr::from_value(payload.clone().into_any()),
                failure: (!interruption).then_some(failure),
            });
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
) -> PyResult<CapturedAttempt<CaptureError>> {
    if !capture.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "capture must be callable",
        ));
    }
    let attempt = run.execute_with_port(&mut Port { capture, reads: 0 });
    if let Err(PortError::Capture(error)) = &attempt.result {
        if error.failure.is_none() {
            return Err(error.error.clone_ref(capture.py()));
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
#[pyclass(frozen, module = "yamaa_native", name = "_BuildResult")]
pub struct BuildResult {
    inner: specification_report::BuildResult,
}
#[pymethods]
impl BuildResult {
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
) -> PyResult<BuildResult> {
    let fields = fields(metadata)?;
    let attempt = capture_attempt(run, capture)?;
    let inner =
        specification_report::build_result(run, &attempt, identity(&fields)).map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("unsupported or invalid build observation")
        })?;
    Ok(BuildResult { inner })
}
