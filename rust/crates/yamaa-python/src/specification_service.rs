//! Prototype original-YAML API: immutable Rust objects, no Python semantic model.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString, PyTuple},
};
use yamaa_adapters::{
    specification_diagnostics::{capture_failure, failure},
    specification_run::PreparedRun,
    specification_source::{CapturedSchema, Limits, Source},
};

#[pyclass(frozen, module = "yamaa_native", name = "_Specification")]
pub struct Specification {
    inner: PreparedRun,
}
#[pymethods]
impl Specification {
    /// Source IO remains explicit host authority after shared admission.
    fn source(&self) -> (String, String) {
        let source = self.inner.source();
        (source.name.clone(), source.path.clone())
    }
    /// Experimental complete failure observations using an explicit resource callback.
    /// Metadata: runtime version, engine version, example, relative spec, relative base.
    #[pyo3(signature = (capture, metadata, inspect=None))]
    fn failure_report(
        &self,
        py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        metadata: &Bound<'_, PyTuple>,
        inspect: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        self.observed_report(py, capture, None, metadata, inspect)
    }
    /// Execute once and publish through an explicit host callback after shared checks.
    #[pyo3(signature = (capture, publish, metadata, inspect=None))]
    fn report(
        &self,
        py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        publish: &Bound<'_, PyAny>,
        metadata: &Bound<'_, PyTuple>,
        inspect: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        self.observed_report(py, capture, Some(publish), metadata, inspect)
    }
    /// Execute and retain the admitted output without publication.
    #[pyo3(signature = (capture, metadata, inspect=None))]
    fn build(
        &self,
        capture: &Bound<'_, PyAny>,
        metadata: &Bound<'_, PyTuple>,
        inspect: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<crate::specification_result::BuildResult> {
        crate::specification_result::build(&self.inner, capture, metadata, inspect)
    }
    /// Decode and bind owned captured CSV bytes; no reference interpreter is imported.
    fn execute_csv<'py>(
        &self,
        py: Python<'py>,
        bytes: &Bound<'py, PyBytes>,
    ) -> PyResult<(Option<Bound<'py, PyBytes>>, String)> {
        let result = self.inner.execute_csv(bytes.as_bytes()).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(failure(&e, Some(self.inner.source())))
        })?;
        Ok((
            result.table.as_deref().map(|bytes| PyBytes::new(py, bytes)),
            result.outcome,
        ))
    }
}
/// Capture host-provided raw modules and entry YAML. Limits precede owned copies.
/// This prototype's error rendering is not yet a qualified frontend contract.
#[pyfunction]
pub fn _prepare_specification(
    modules: &Bound<'_, PyList>,
    entry: usize,
    identity: &str,
    source: &Bound<'_, PyBytes>,
) -> PyResult<Specification> {
    let schema = captured_schema(modules, entry, identity, source)?;
    let document = schema
        .prepare_standalone(Source {
            identity: identity.into(),
            bytes: source.as_bytes().to_vec(),
        })
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(capture_failure(&e)))?;
    let inner = PreparedRun::prepare(document)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(failure(&e, None)))?;
    Ok(Specification { inner })
}

pub(super) fn captured_schema(
    modules: &Bound<'_, PyList>,
    entry: usize,
    identity: &str,
    source: &Bound<'_, PyBytes>,
) -> PyResult<std::sync::Arc<CapturedSchema>> {
    let limits = Limits::default();
    let invalid = || {
        pyo3::exceptions::PyValueError::new_err(
            "invalid or over-limit captured specification sources",
        )
    };
    if modules.len() > limits.bundle.modules
        || identity.len() > limits.identity_bytes
        || source.as_bytes().len() > limits.captured_bytes
    {
        return Err(invalid());
    }
    let mut total = 0usize;
    let mut names = 0usize;
    for item in modules.iter() {
        let item = item.cast::<PyTuple>().map_err(|_| invalid())?;
        if item.len() != 2 {
            return Err(invalid());
        }
        let name = item.get_item(0)?;
        let name = name.cast::<PyString>().map_err(|_| invalid())?.to_str()?;
        let bytes = item.get_item(1)?;
        let bytes = bytes.cast::<PyBytes>().map_err(|_| invalid())?;
        total = total
            .checked_add(bytes.as_bytes().len())
            .filter(|&n| n <= limits.captured_bytes)
            .ok_or_else(invalid)?;
        names = names
            .checked_add(name.len())
            .filter(|&n| n <= limits.identity_bytes)
            .ok_or_else(invalid)?;
    }
    let sources = modules
        .iter()
        .map(|item| {
            let item = item.cast::<PyTuple>().map_err(|_| invalid())?;
            let name = item.get_item(0)?.extract::<String>()?;
            let bytes = item.get_item(1)?;
            let bytes = bytes
                .cast::<PyBytes>()
                .map_err(|_| invalid())?
                .as_bytes()
                .to_vec();
            Ok(Source {
                identity: name,
                bytes,
            })
        })
        .collect::<PyResult<Vec<_>>>()?;
    CapturedSchema::admit(sources, entry, limits)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(capture_failure(&e)))
}

pub(super) fn from_document(
    document: yamaa_adapters::specification_source::PreparedDocument,
) -> PyResult<Specification> {
    let inner = PreparedRun::prepare(document)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(failure(&e, None)))?;
    Ok(Specification { inner })
}

impl Specification {
    fn observed_report(
        &self,
        _py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        publisher: Option<&Bound<'_, PyAny>>,
        metadata: &Bound<'_, PyTuple>,
        inspect: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        use crate::specification_result::{capture_attempt, fields, identity, Publisher};
        use yamaa_adapters::specification_report;
        if publisher.is_some_and(|callback| !callback.is_callable()) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "publish must be callable",
            ));
        }
        let fields = fields(metadata)?;
        let attempt = capture_attempt(&self.inner, capture, inspect)?;
        if let Some(callback) = publisher {
            specification_report::complete(
                &self.inner,
                &attempt,
                identity(&fields),
                &mut Publisher(callback),
            )
            .map(|value| value.to_string())
            .map_err(|error| match error {
                specification_report::CompleteError::Publish(error) => error,
                specification_report::CompleteError::Report(_) => {
                    pyo3::exceptions::PyValueError::new_err(
                        "unsupported or invalid report observation",
                    )
                }
            })
        } else {
            specification_report::failure(&self.inner, &attempt, identity(&fields))
                .map(|value| value.to_string())
                .map_err(|_| {
                    pyo3::exceptions::PyValueError::new_err(
                        "unsupported or invalid failure-report observation",
                    )
                })
        }
    }
}
