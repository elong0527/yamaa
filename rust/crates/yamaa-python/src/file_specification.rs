//! Private native file lifecycle. Python marshals arguments and owned results only.
use pyo3::{
    prelude::*,
    types::{PyList, PyString, PyTuple},
};
use std::sync::{Mutex, MutexGuard};
use yamaa_adapters::{file_preparation::FileSpecification, file_resources::Resources};

#[pyclass(frozen, module = "yamaa_native", name = "_FileSpecification")]
pub struct Specification {
    inner: Mutex<FileSpecification>,
}
impl Specification {
    fn get(&self) -> PyResult<MutexGuard<'_, FileSpecification>> {
        self.inner.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("native file specification handle poisoned")
        })
    }
}
#[pymethods]
impl Specification {
    fn source(&self) -> PyResult<(String, String)> {
        let state = self.get()?;
        let source = state.run().source();
        Ok((source.name.clone(), source.path.clone()))
    }
    fn capture_reads(&self) -> PyResult<usize> {
        Ok(self.get()?.capture_reads())
    }
    fn check_issues(&self) -> PyResult<String> {
        yamaa_adapters::specification_check::issues(self.get()?.run())
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))
    }
    fn build(
        &self,
        metadata: &Bound<'_, PyTuple>,
    ) -> PyResult<crate::specification_result::BuildResult> {
        use crate::specification_result::{fields, identity, BuildResult};
        let fields = fields(metadata)?;
        let mut state = self.get()?;
        let attempt = state.build();
        if let Some(error) = yamaa_adapters::file_preparation::opaque_resource_failure(&attempt) {
            return Err(pyo3::exceptions::PyValueError::new_err(error.message()));
        }
        let inner = yamaa_adapters::specification_report::build_result(
            state.run(),
            &attempt,
            identity(&fields),
        )
        .map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("unsupported or invalid build observation")
        })?;
        Ok(BuildResult { inner })
    }
}

#[pyfunction]
pub fn _prepare_file_specification(
    project_root: &str,
    base_directory: &str,
    entry: &str,
    data_roots: &Bound<'_, PyList>,
) -> PyResult<Specification> {
    if data_roots.len() >= 64
        || [project_root, base_directory, entry]
            .iter()
            .any(|text| text.len() > 65536)
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "file specification path or root limit",
        ));
    }
    let roots = data_roots
        .iter()
        .map(|value| {
            let value = value.cast::<PyString>()?;
            let text = value.to_str()?;
            if text.len() > 65536 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "file specification path byte limit",
                ));
            }
            Ok(text.to_owned())
        })
        .collect::<PyResult<Vec<_>>>()?;
    let resources = Resources::new(project_root, base_directory, &roots)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.message()))?;
    let inner = FileSpecification::prepare(resources, entry)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.into_message()))?;
    Ok(Specification {
        inner: Mutex::new(inner),
    })
}
