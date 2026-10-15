//! Encode admitted core codelist facts without rerunning checks or table reads.
use crate::{issue_rows::Issue, project_diagnostic_projection::issue};
pub use yamaa_core::project_codelist_diagnostics::Error;
use yamaa_core::specification::PreparedSpecification;
use yamaa_engine::dataset::CheckRecord;

pub fn codelist_issues(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
) -> Result<Vec<Issue>, Error> {
    codelist_issues_with_limit(
        compiled,
        records,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
pub fn codelist_issues_with_limit(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
    maximum: usize,
) -> Result<Vec<Issue>, Error> {
    let diagnostics =
        yamaa_engine::project_codelist_diagnostics::diagnostics(compiled, records, maximum)?;
    let issues = diagnostics
        .into_iter()
        .map(issue)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Projection)?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Limit);
    }
    Ok(issues)
}
