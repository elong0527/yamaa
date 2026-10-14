//! Arrow PyCapsule ingestion on the attached calling interpreter thread.
use pyo3::{
    exceptions::{PyTypeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyCapsule},
};
use yamaa_adapters::{
    c_data_table::{self, FFI_ArrowArrayStream, ImportBudget},
    specification_run::PreparedRun,
};

pub(super) fn execute<'py>(
    py: Python<'py>,
    run: &PreparedRun,
    sources: &[Bound<'py, PyAny>],
) -> PyResult<(Option<Bound<'py, PyBytes>>, String)> {
    c_data_table::admit_sources(run, sources.len())
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let mut budget = ImportBudget::default();
    let mut tables = Vec::with_capacity(sources.len());
    for source in sources {
        let capsule = source.call_method0("__arrow_c_stream__")?;
        let capsule = capsule.cast::<PyCapsule>().map_err(|_| {
            PyTypeError::new_err("__arrow_c_stream__ must return an Arrow stream capsule")
        })?;
        let pointer = capsule
            .pointer_checked(Some(c"arrow_array_stream"))?
            .cast::<FFI_ArrowArrayStream>()
            .as_ptr();
        if !pointer.is_aligned() {
            return Err(PyValueError::new_err("unaligned Arrow stream capsule"));
        }
        // SAFETY: the named capsule comes from a trusted Arrow protocol producer.
        // Moving empties its stream, so the capsule destructor cannot double-release
        // it and a second consumer is refused. Keep the GIL and source alive through
        // import, execution and all release callbacks.
        let stream = unsafe { FFI_ArrowArrayStream::from_raw(pointer) };
        let table = unsafe { budget.import(stream) }
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        tables.push(table);
    }
    let result = c_data_table::execute(run, tables)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok((
        result.table.as_deref().map(|bytes| PyBytes::new(py, bytes)),
        result.outcome,
    ))
}
