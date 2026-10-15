//! Python report metadata and issue-row conversion.
use pyo3::{
    prelude::*,
    types::{PyString, PyTuple},
};
use yamaa_adapters::specification_report::Identity;

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
            if text.len() > 4096 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "report metadata limit",
                ));
            }
            Ok(text.to_owned())
        })
        .collect()
}
pub(super) fn identity(fields: &[String]) -> Identity<'_> {
    Identity {
        runtime: "python",
        runtime_version: &fields[0],
        engine_version: &fields[1],
        example: &fields[2],
        specification: &fields[3],
        base_directory: &fields[4],
    }
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
                issue.context.clone(),
            )
        })
        .collect()
}
