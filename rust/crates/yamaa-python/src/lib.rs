//! Optional Python installation probe; this is not an execution backend.
use pyo3::prelude::*;
use pyo3::types::PyDict;

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
    Ok(())
}
