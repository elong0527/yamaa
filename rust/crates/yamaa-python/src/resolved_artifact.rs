//! Text and issue-row transport for the shared resolved artifact.
use pyo3::prelude::*;

type Reply = (Option<String>, Vec<crate::report_fields::IssueRow>);

#[pyfunction]
#[pyo3(signature=(specification, environment=None))]
pub fn _resolve_specification_file(
    specification: &str,
    environment: Option<&str>,
) -> PyResult<Reply> {
    match yamaa_adapters::resolved_artifact::resolve(
        specification,
        environment,
        yamaa_core::project_function::Language::Python,
    ) {
        Ok(artifact) => Ok((
            Some(
                artifact
                    .to_json()
                    .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?,
            ),
            Vec::new(),
        )),
        Err(issues) => Ok((None, crate::report_fields::issue_rows(&issues))),
    }
}

#[pyfunction]
pub fn _read_resolved_specification(text: &str) -> PyResult<String> {
    yamaa_adapters::resolved_artifact::ResolvedArtifact::from_json(text)
        .and_then(|artifact| artifact.to_json())
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}
