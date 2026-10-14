//! Argument marshalling for read-only installed schema metadata.
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString},
};

#[pyclass(frozen, module = "yamaa._native", name = "_SchemaMetadata")]
pub struct Schema {
    inner: yamaa_adapters::schema_metadata::Schema,
}

fn types(values: &Bound<'_, PyList>) -> PyResult<Vec<String>> {
    if values.len() > 128 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "schema query type count limit",
        ));
    }
    let mut result = Vec::new();
    let mut size = 0usize;
    for value in values {
        let value = value.cast::<PyString>()?.to_str()?;
        size = size
            .checked_add(value.len())
            .filter(|&size| size <= 65_536)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("schema query text limit"))?;
        result.push(value.into());
    }
    Ok(result)
}
#[pymethods]
impl Schema {
    fn version(&self) -> &str {
        self.inner.version()
    }
    fn admit(&self, identity: &str, source: &Bound<'_, PyBytes>) -> PyResult<String> {
        let limits = yamaa_adapters::specification_source::Limits::default();
        if identity.len() > limits.identity_bytes || source.as_bytes().len() > limits.captured_bytes
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "schema metadata capture limit",
            ));
        }
        self.inner
            .admit(identity, source.as_bytes())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
    fn fields(&self, name: &str) -> PyResult<Vec<String>> {
        self.inner
            .fields(name)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
    fn defaults(&self) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        self.inner
            .defaults()
            .map(|issues| crate::specification_result::issue_rows(&issues))
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
    fn validate(
        &self,
        source: &Bound<'_, PyBytes>,
        queries: &Bound<'_, PyList>,
        fragment: bool,
    ) -> PyResult<Vec<crate::specification_result::IssueRow>> {
        self.inner
            .validate(source.as_bytes(), &types(queries)?, fragment)
            .map(|issues| crate::specification_result::issue_rows(&issues))
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
    fn normalize(
        &self,
        source: &Bound<'_, PyBytes>,
        queries: &Bound<'_, PyList>,
        fragment: bool,
    ) -> PyResult<String> {
        self.inner
            .normalize(source.as_bytes(), &types(queries)?, fragment)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }
}
#[pyfunction]
pub fn _capture_schema_metadata(
    modules: &Bound<'_, PyList>,
    entry: usize,
    root: &str,
) -> PyResult<Schema> {
    let captured = crate::specification_service::captured_metadata_modules(modules, entry, root)?;
    Ok(Schema {
        inner: yamaa_adapters::schema_metadata::Schema(captured),
    })
}
