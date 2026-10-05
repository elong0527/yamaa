//! Opaque owned compiler metadata; no constructor, mutation or data access is exposed.
use pyo3::prelude::*;
use yamaa_adapters::reference_transport::{self, CompiledCatalog, TransportError};

/// Report metadata-query support without preparing a catalog or accessing host data.
#[pyfunction]
pub fn reference_capabilities() -> &'static str {
    reference_transport::capabilities()
}

/// An immutable catalog owned by Python and independent of the request's buffers.
#[pyclass(frozen, module = "yamaa_native", name = "_ReferenceCatalog")]
pub struct ReferenceCatalog {
    inner: CompiledCatalog,
}

/// Convert only transport errors; semantic conditions and limits stay in JSON outcomes.
fn error(error: TransportError) -> PyErr {
    if error == TransportError::Internal {
        pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
    } else {
        pyo3::exceptions::PyValueError::new_err(error.to_string())
    }
}

#[pymethods]
impl ReferenceCatalog {
    /// Analyze one bounded query batch against this captured immutable schema catalog.
    fn analyze(&self, request: &str) -> PyResult<String> {
        self.inner.analyze(request).map_err(error)
    }
}

/// Compile once without retaining Python buffers or granting callback/data authority.
#[pyfunction]
pub fn _compile_reference_catalog(request: &str) -> PyResult<(Option<ReferenceCatalog>, String)> {
    reference_transport::compile_reference_catalog(request)
        .map(|(catalog, outcome)| (catalog.map(|inner| ReferenceCatalog { inner }), outcome))
        .map_err(error)
}

/// Batch entrypoint shared with R; both paths use the same catalog and query evaluator.
#[pyfunction]
pub fn analyze_references(request: &str) -> PyResult<String> {
    reference_transport::analyze_references(request).map_err(error)
}
