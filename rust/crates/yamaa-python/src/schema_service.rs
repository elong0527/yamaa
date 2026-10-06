//! Owned shared schema snapshots; no Python schema interpreter is involved.
use pyo3::{prelude::*, types::PyString};
use yamaa_adapters::inheritance_transport;
use yamaa_adapters::schema_transport::{self, CompiledSchema, TransportError};

#[pyclass(frozen, module = "yamaa_native", name = "_Schema")]
pub struct Schema {
    inner: CompiledSchema,
}

fn error(error: TransportError) -> PyErr {
    if error == TransportError::Internal {
        pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
    } else {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}
#[pymethods]
impl Schema {
    /// Interpret a bounded batch against this captured schema with fresh request policies.
    fn analyze(&self, request: &str) -> PyResult<String> {
        self.inner.analyze(request).map_err(error)
    }

    /// Traverse with explicit synchronous source authority; the schema snapshot stays captured.
    fn traverse_inheritance(&self, request: &str, callback: &Bound<'_, PyAny>) -> PyResult<String> {
        if !callback.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "source callback must be callable",
            ));
        }
        inheritance_transport::traverse(&self.inner, request, |message, maximum| {
            call_source(callback, message, maximum)
        })
        .map_err(inheritance_error)
    }
}

/// Admit decoded modules and their defaults without retaining Python buffers or reading files.
#[pyfunction]
pub fn _compile_schema(request: &str) -> PyResult<(Option<Schema>, String)> {
    schema_transport::compile_schema(request)
        .map(|(schema, outcome)| (schema.map(|inner| Schema { inner }), outcome))
        .map_err(error)
}

/// Experimental decoded schema/1 batch service; this does not enable engine execution.
#[pyfunction]
pub fn interpret_schema(request: &str) -> PyResult<String> {
    schema_transport::interpret_schema(request).map_err(error)
}

fn inheritance_error(error: inheritance_transport::Failure<PyErr>) -> PyErr {
    match error {
        inheritance_transport::Failure::Host(error) => error,
        inheritance_transport::Failure::Transport(
            inheritance_transport::TransportError::Internal,
        ) => pyo3::exceptions::PyRuntimeError::new_err("internal inheritance failure"),
        inheritance_transport::Failure::Transport(error) => {
            pyo3::exceptions::PyValueError::new_err(error.to_string())
        }
    }
}

fn call_source(callback: &Bound<'_, PyAny>, request: &str, maximum: usize) -> PyResult<String> {
    let value = callback.call1((request, maximum))?;
    let value = value.cast::<PyString>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("source callback must return JSON text")
    })?;
    let text = value.to_cow()?;
    if text.len() > maximum {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "inheritance source reply exceeds byte limit",
        ));
    }
    Ok(text.into_owned())
}

/// Experimental source-port transport; this does not enable workflow execution.
#[pyfunction]
pub fn traverse_inheritance(
    schema_request: &str,
    request: &str,
    callback: &Bound<'_, PyAny>,
) -> PyResult<String> {
    if !callback.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "source callback must be callable",
        ));
    }
    inheritance_transport::interpret(schema_request, request, |message, maximum| {
        call_source(callback, message, maximum)
    })
    .map_err(inheritance_error)
}
