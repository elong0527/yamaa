//! Retained style CLI proof: shared YAML admission, no parser result exposure.
use pyo3::{prelude::*, types::PyBytes};
use yamaa_adapters::yaml_decode::{decode_yaml, DecodeLimits};

#[pyfunction]
pub fn _style_same_value(before: &Bound<'_, PyBytes>, after: &Bound<'_, PyBytes>) -> bool {
    let limits = DecodeLimits::default();
    match (
        decode_yaml(before.as_bytes(), limits),
        decode_yaml(after.as_bytes(), limits),
    ) {
        (Ok(before), Ok(after)) => before
            .document
            .same_style_value(&after.document, limits.document.edges),
        _ => false,
    }
}
