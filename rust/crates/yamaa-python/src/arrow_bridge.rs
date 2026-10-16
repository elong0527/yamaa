//! Python Arrow PyCapsule protocol; no IPC serialization at the host boundary.
use pyo3::{prelude::*, types::PyCapsule};
use yamaa_adapters::public_table::PublicTable;

#[pyclass(frozen, module = "yamaa._native", name = "_ArrowTable")]
pub struct ArrowTable {
    table: PublicTable,
}
impl From<&PublicTable> for ArrowTable {
    fn from(table: &PublicTable) -> Self {
        Self {
            table: table.clone(),
        }
    }
}
#[pymethods]
impl ArrowTable {
    #[pyo3(signature=(requested_schema=None))]
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        if requested_schema.is_some_and(|schema| !schema.is_none()) {
            return Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "Arrow schema conversion is not supported; import the preserved schema first",
            ));
        }
        // PyO3 stores the stream header at the capsule pointer and drops its
        // value on capsule destruction. A consumer moves that header and clears
        // release; dropping the consumed capsule then only frees the shell.
        // Unconsumed capsules release their reader and buffers on destruction.
        PyCapsule::new(py, self.table.stream(), Some(c"arrow_array_stream".into()))
    }
}
