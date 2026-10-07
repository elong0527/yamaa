//! Thin raw-source callbacks for the shared inherited-document lifecycle.
use crate::specification_service::{captured_schema, from_document, Specification};
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString, PyTuple},
};
use yamaa_adapters::{
    specification_diagnostics::inheritance_failure,
    specification_source::{InheritancePort, Source},
};
use yamaa_engine::inheritance::{Source as Identity, SourceError};

fn text(value: &Bound<'_, PyAny>, maximum: usize) -> PyResult<String> {
    let value = value.cast::<PyString>()?.to_str()?;
    if value.len() > maximum {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "inheritance text limit",
        ));
    }
    Ok(value.into())
}
fn available<T>(result: PyResult<Option<T>>) -> Result<T, SourceError<PyErr>> {
    result
        .map_err(SourceError::Raised)?
        .ok_or(SourceError::Unavailable)
}
struct Port<'a, 'py> {
    canonicalize: &'a Bound<'py, PyAny>,
    capture: &'a Bound<'py, PyAny>,
    rebase: &'a Bound<'py, PyAny>,
}
impl InheritancePort for Port<'_, '_> {
    type Error = PyErr;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<PyErr>> {
        available((|| {
            let result = self.canonicalize.call1((declaring, written))?;
            if result.is_none() {
                return Ok(None);
            }
            let result = result.cast::<PyTuple>()?;
            if result.len() != 2 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid inheritance identity",
                ));
            }
            Ok(Some(Identity {
                identity: text(&result.get_item(0)?, 65_536)?,
                display_path: text(&result.get_item(1)?, 65_536)?,
            }))
        })())
    }
    fn capture(
        &mut self,
        source: &Identity,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<PyErr>> {
        available((|| {
            let result = self
                .capture
                .call1((&source.identity, &source.display_path, maximum))?;
            if result.is_none() {
                return Ok(None);
            }
            let result = result.cast::<PyBytes>()?;
            if result.as_bytes().len() > maximum {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "inheritance capture limit",
                ));
            }
            Ok(Some(result.as_bytes().to_vec()))
        })())
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> PyResult<String> {
        text(
            &self
                .rebase
                .call1((&layer.identity, &entry.identity, written, maximum))?,
            maximum,
        )
    }
}
/// Internal prototype: callbacks supply identity/raw bytes/path authority only.
#[pyfunction]
pub fn _prepare_inherited_specification(
    modules: &Bound<'_, PyList>,
    entry: usize,
    identity: &str,
    source: &Bound<'_, PyBytes>,
    canonicalize: &Bound<'_, PyAny>,
    capture: &Bound<'_, PyAny>,
    rebase: &Bound<'_, PyAny>,
) -> PyResult<Specification> {
    if !canonicalize.is_callable() || !capture.is_callable() || !rebase.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "inheritance ports must be callable",
        ));
    }
    let schema = captured_schema(modules, entry, identity, source)?;
    let document = schema
        .prepare_inherited(
            Source {
                identity: identity.into(),
                bytes: source.as_bytes().to_vec(),
            },
            identity.into(),
            &mut Port {
                canonicalize,
                capture,
                rebase,
            },
        )
        .map_err(|error| match inheritance_failure(error) {
            Ok(message) => pyo3::exceptions::PyValueError::new_err(message),
            Err(error) => error,
        })?;
    from_document(document)
}
