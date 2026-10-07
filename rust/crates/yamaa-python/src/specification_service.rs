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
    fn failure_report(
        &self,
        py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        metadata: &Bound<'_, PyTuple>,
    ) -> PyResult<String> {
        self.observed_report(py, capture, None, metadata)
    }
    /// Execute once and publish through an explicit host callback after shared checks.
    fn report(
        &self,
        py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        publish: &Bound<'_, PyAny>,
        metadata: &Bound<'_, PyTuple>,
    ) -> PyResult<String> {
        self.observed_report(py, capture, Some(publish), metadata)
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
    let schema = CapturedSchema::admit(sources, entry, limits)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(capture_failure(&e)))?;
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

impl Specification {
    fn observed_report(
        &self,
        py: Python<'_>,
        capture: &Bound<'_, PyAny>,
        publisher: Option<&Bound<'_, PyAny>>,
        metadata: &Bound<'_, PyTuple>,
    ) -> PyResult<String> {
        use std::sync::Arc;
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::{PortError, SourcePort},
        };
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
        if !capture.is_callable() || publisher.is_some_and(|callback| !callback.is_callable()) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "capture and publish must be callable",
            ));
        }
        if metadata.len() != 5 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid report metadata",
            ));
        }
        let fields = metadata
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
            .collect::<PyResult<Vec<_>>>()?;
        let attempt = self
            .inner
            .execute_with_port(&mut Port { capture, reads: 0 });
        if let Err(PortError::Capture(error)) = &attempt.result {
            return Err(error.clone_ref(py));
        }
        let identity = Identity {
            runtime: "python",
            runtime_version: &fields[0],
            engine_version: &fields[1],
            example: &fields[2],
            specification: &fields[3],
            base_directory: &fields[4],
        };
        if let Some(callback) = publisher {
            struct Publisher<'a, 'py>(&'a Bound<'py, PyAny>);
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
            specification_report::complete(
                &self.inner,
                &attempt,
                identity,
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
            specification_report::failure(&self.inner, &attempt, identity)
                .map(|value| value.to_string())
                .map_err(|_| {
                    pyo3::exceptions::PyValueError::new_err(
                        "unsupported or invalid failure-report observation",
                    )
                })
        }
    }
}
