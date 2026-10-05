//! Optional Python installation probe; this is not an execution backend.
use pyo3::prelude::*;
mod function_callback;
mod temporal_result;
use pyo3::types::PyDict;

/// Analyze an already-bound graph through shared Rust without accessing host data.
#[pyfunction]
fn analyze_dependencies(request: &str) -> PyResult<String> {
    use yamaa_adapters::dependency_transport::TransportError;
    yamaa_adapters::dependency_transport::analyze_dependencies(request).map_err(|error| {
        if error == TransportError::Internal {
            pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
        } else {
            pyo3::exceptions::PyValueError::new_err(error.to_string())
        }
    })
}

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

/// Copy a bounded Arrow IPC stream through validated, sanitized table storage.
#[pyfunction]
fn table_round_trip<'py>(
    py: Python<'py>,
    request: &[u8],
) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
    yamaa_adapters::table_transport::table_round_trip(request)
        .map(|bytes| pyo3::types::PyBytes::new(py, &bytes))
        .map_err(table_error)
}

/// Inspect exact table values without narrowing integers into host number types.
#[pyfunction]
fn table_snapshot(request: &[u8]) -> PyResult<String> {
    yamaa_adapters::table_transport::table_snapshot(request).map_err(table_error)
}

/// Execute an explicitly supplied dataset/1 plan over copied canonical IPC.
/// Success returns owned IPC and observations; failure never returns a table.
#[pyfunction]
fn execute_dataset<'py>(
    py: Python<'py>,
    request: &str,
    source: &[u8],
) -> PyResult<(Option<Bound<'py, pyo3::types::PyBytes>>, String)> {
    dataset_result(
        py,
        yamaa_adapters::dataset_transport::execute_dataset(request, source),
    )
}

/// Execute a bounded list of independent secondary snapshots without host joins.
#[pyfunction]
fn execute_dataset_sources<'py>(
    py: Python<'py>,
    request: &str,
    source: &[u8],
    secondary: &Bound<'py, pyo3::types::PyList>,
) -> PyResult<(Option<Bound<'py, pyo3::types::PyBytes>>, String)> {
    if secondary.len() >= yamaa_adapters::dataset_transport::MAX_SOURCES {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "too many secondary dataset sources",
        ));
    }
    let buffers = secondary
        .iter()
        .map(|value| value.cast_into::<pyo3::types::PyBytes>())
        .collect::<Result<Vec<_>, _>>()?;
    let slices: Vec<&[u8]> = buffers.iter().map(|bytes| bytes.as_bytes()).collect();
    dataset_result(
        py,
        yamaa_adapters::dataset_transport::execute_dataset_sources(request, source, &slices),
    )
}

/// Return fresh result bytes and preserve the transport's error classification.
fn dataset_result<'py>(
    py: Python<'py>,
    result: Result<
        yamaa_adapters::dataset_transport::DatasetResponse,
        yamaa_adapters::dataset_transport::DatasetTransportError,
    >,
) -> PyResult<(Option<Bound<'py, pyo3::types::PyBytes>>, String)> {
    result
        .map(|result| dataset_output(py, result))
        .map_err(dataset_error)
}

/// Materialize result ownership identically for ordinary and instrumented executions.
fn dataset_output<'py>(
    py: Python<'py>,
    result: yamaa_adapters::dataset_transport::DatasetResponse,
) -> (Option<Bound<'py, pyo3::types::PyBytes>>, String) {
    (
        result
            .table
            .map(|bytes| pyo3::types::PyBytes::new(py, &bytes)),
        result.outcome,
    )
}

/// Preserve the existing distinction between internal failures and rejected transport input.
fn dataset_error(error: yamaa_adapters::dataset_transport::DatasetTransportError) -> PyErr {
    if error.is_internal() {
        pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
    } else {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

/// Keep internal failures separate from rejected input or resource policy.
fn table_error(error: yamaa_adapters::table_transport::TableTransportError) -> PyErr {
    if error == yamaa_adapters::table_transport::TableTransportError::Internal {
        pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
    } else {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

/// Report typed dataset features without reading sources or claiming backend readiness.
#[pyfunction]
fn dataset_capabilities() -> &'static str {
    yamaa_adapters::dataset_transport::capabilities()
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
    module.add_function(wrap_pyfunction!(temporal_result::_temporal_result, module)?)?;
    module.add_function(wrap_pyfunction!(engine_info, module)?)?;
    module.add_function(wrap_pyfunction!(analyze_dependencies, module)?)?;
    module.add_function(wrap_pyfunction!(
        function_callback::invoke_function,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        function_callback::execute_dataset_functions,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        function_callback::_profile_dataset_functions,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(scalar_round_trip, module)?)?;
    module.add_function(wrap_pyfunction!(evaluate_numeric, module)?)?;
    module.add_function(wrap_pyfunction!(table_round_trip, module)?)?;
    module.add_function(wrap_pyfunction!(table_snapshot, module)?)?;
    module.add_function(wrap_pyfunction!(execute_dataset, module)?)?;
    module.add_function(wrap_pyfunction!(execute_dataset_sources, module)?)?;
    module.add_function(wrap_pyfunction!(dataset_capabilities, module)?)?;
    Ok(())
}
