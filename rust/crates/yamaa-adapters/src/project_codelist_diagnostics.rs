//! Encode admitted core codelist facts without rerunning checks or table reads.
use crate::project_diagnostic_projection::owned;
use yamaa_core::dataset::CheckRecord;
use yamaa_core::diagnostic::Diagnostic;
pub use yamaa_core::project_codelist_diagnostics::Error;
use yamaa_core::specification::PreparedSpecification;

pub fn codelist_issues(
    compiled: &PreparedSpecification,
    records: &[CheckRecord],
) -> Result<Vec<Diagnostic>, Error> {
    Ok(
        yamaa_core::project_codelist_diagnostics::diagnostics(compiled, records)?
            .into_iter()
            .map(owned)
            .collect(),
    )
}
