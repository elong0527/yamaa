//! Encode admitted core codelist facts without rerunning checks or table reads.
use crate::{issue_rows::Issue, project_diagnostic_projection::issue};
use yamaa_core::dataset::CheckRecord;
pub use yamaa_core::project_codelist_diagnostics::Error;
use yamaa_core::specification::PreparedSpecification;

pub fn codelist_issues(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
) -> Result<Vec<Issue>, Error> {
    yamaa_core::project_codelist_diagnostics::diagnostics(compiled, records)?
        .into_iter()
        .map(issue)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Projection)
}
