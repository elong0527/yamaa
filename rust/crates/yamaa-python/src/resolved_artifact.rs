//! Text and issue-row transport for the shared resolved artifact.
use pyo3::prelude::*;
use yamaa::{Engine, EngineOptions, HostLanguage, Request};

type Reply = (Option<String>, Vec<crate::report_fields::IssueRow>);

fn engine(py: Python<'_>) -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "python".into(),
        py.version().into(),
    ))
}

#[pyfunction]
#[pyo3(signature=(specification, environment=None))]
pub fn _resolve_specification_file(
    py: Python<'_>,
    specification: &str,
    environment: Option<&str>,
) -> PyResult<Reply> {
    let mut request = Request::new(specification);
    if let Some(environment) = environment {
        request = request.with_environment(environment);
    }
    match engine(py).resolve_specification(&request) {
        Ok(artifact) => Ok((
            Some(
                artifact
                    .to_json()
                    .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?,
            ),
            Vec::new(),
        )),
        Err(yamaa::Error::Rejected(report)) => {
            Ok((None, crate::report_fields::issue_rows(&report.diagnostics)))
        }
        Err(error) => Err(pyo3::exceptions::PyValueError::new_err(error.to_string())),
    }
}

#[pyfunction]
pub fn _read_resolved_specification(text: &str) -> PyResult<String> {
    yamaa::ResolvedSpecification::from_json(text)
        .and_then(|artifact| artifact.to_json())
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}
