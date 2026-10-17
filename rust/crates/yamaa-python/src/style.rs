//! Retained style CLI proof: shared YAML admission, no parser result exposure.
use pyo3::{prelude::*, types::PyBytes};

#[pyfunction]
pub fn _style_same_value(before: &Bound<'_, PyBytes>, after: &Bound<'_, PyBytes>) -> bool {
    yamaa::style_same_value(before.as_bytes(), after.as_bytes())
}
