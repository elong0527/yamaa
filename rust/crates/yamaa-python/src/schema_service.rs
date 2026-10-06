//! Owned shared schema snapshots; no Python schema interpreter is involved.
use pyo3::prelude::*;
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
