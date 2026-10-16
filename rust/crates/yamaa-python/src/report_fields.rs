//! Python report metadata and issue-row conversion.
use pyo3::{
    prelude::*,
    types::{PyString, PyTuple},
};

pub(super) fn fields(metadata: &Bound<'_, PyTuple>) -> PyResult<Vec<String>> {
    if metadata.len() != 5 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "invalid report metadata",
        ));
    }
    metadata
        .iter()
        .map(|item| {
            let text = item.cast::<PyString>()?.to_str()?;
            Ok(text.to_owned())
        })
        .collect()
}
pub(super) type IssueRow = (String, String, Option<String>, Vec<String>, String);
pub(super) fn issue_rows(issues: &[yamaa_adapters::issue_rows::Issue]) -> Vec<IssueRow> {
    issues
        .iter()
        .map(|issue| {
            (
                issue.phase.clone(),
                issue.condition.clone(),
                issue.requirement.clone(),
                issue.spec_paths.clone(),
                yamaa_adapters::json_wire::encode(&issue.context),
            )
        })
        .collect()
}
