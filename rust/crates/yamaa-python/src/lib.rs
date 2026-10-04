//! Optional Python installation probe; this is not an execution backend.
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Round-trip a versioned scalar envelope through normalized core values.
#[pyfunction]
fn scalar_round_trip(request: &str) -> PyResult<String> {
    use yamaa_adapters::scalar_transport::TransportError;
    yamaa_adapters::scalar_transport::scalar_round_trip(request).map_err(|error| {
        if error == TransportError::Internal {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        } else {
            pyo3::exceptions::PyValueError::new_err(error.to_string())
        }
    })
}

/// Evaluate one normalized numeric request with structured outcomes and no fallback.
#[pyfunction]
fn evaluate_numeric(request: &str) -> PyResult<String> {
    use yamaa_adapters::numeric_transport::NumericTransportError;
    yamaa_adapters::numeric_transport::evaluate_numeric(request).map_err(|error| {
        if error == NumericTransportError::Internal {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        } else {
            pyo3::exceptions::PyValueError::new_err(error.to_string())
        }
    })
}

#[pyfunction]
fn engine_info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let info = yamaa_engine::engine_info();
    let result = PyDict::new(py);
    result.set_item("core_version", info.core_version)?;
    result.set_item("protocol_version", info.protocol_version)?;
    result.set_item("execution_supported", info.execution_supported)?;
    result.set_item(
        "installation_resource",
        yamaa_adapters::installation_resource(),
    )?;
    Ok(result)
}

#[pymodule]
fn yamaa_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(engine_info, module)?)?;
    module.add_function(wrap_pyfunction!(scalar_round_trip, module)?)?;
    module.add_function(wrap_pyfunction!(evaluate_numeric, module)?)?;
    Ok(())
}
