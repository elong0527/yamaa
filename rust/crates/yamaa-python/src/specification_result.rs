//! Host marshalling for owned build results and explicit save authority.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyString, PyTuple},
};
use std::sync::Arc;
use yamaa_adapters::{
    specification_report::{self, Identity},
    specification_run::{CapturedAttempt, PortError, PreparedRun, SourcePort},
};

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
    type Error = PyErr;
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(
        &mut self,
        source: &yamaa_engine::specification::SourceDeclaration,
        maximum: usize,
    ) -> PyResult<Arc<[u8]>> {
        let result = self.capture.call1((&source.name, &source.path, maximum))?;
        let result = result.cast::<PyTuple>()?;
        if result.len() != 2 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid capture response",
            ));
        }
        let content = result.get_item(0)?;
        let content = content.cast::<PyBytes>()?;
        let created = result.get_item(1)?.extract::<bool>()?;
        if content.as_bytes().len() > maximum {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "capture byte limit",
            ));
        }
        self.reads += usize::from(created);
        Ok(Arc::from(content.as_bytes()))
    }
}
pub(super) fn capture_attempt(
    run: &PreparedRun,
    capture: &Bound<'_, PyAny>,
) -> PyResult<CapturedAttempt<PyErr>> {
    if !capture.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "capture must be callable",
        ));
    }
    let attempt = run.execute_with_port(&mut Port { capture, reads: 0 });
    if let Err(PortError::Capture(error)) = &attempt.result {
        return Err(error.clone_ref(capture.py()));
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
