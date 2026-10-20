//! Borrow complete static compiler findings from the actual retained project.
//! Unsupported vocabulary and policy/resource failures remain typed boundaries.
use crate::project_run::{PreparedRun, RejectedRun};
use yamaa_core::{diagnostic::Diagnostic, specification::PrepareError};

#[derive(Debug)]
pub enum Error<'a> {
    Compile(&'a PrepareError),
    /// A rejected compilation that names no finding.
    Projection,
}
pub fn prepared(run: &PreparedRun) -> Vec<Diagnostic> {
    crate::specification_check::issues(run)
}
pub fn rejected(run: &RejectedRun) -> Result<Vec<Diagnostic>, Error<'_>> {
    let PrepareError::Invalid(findings) = &run.rejection.error else {
        return Err(Error::Compile(&run.rejection.error));
    };
    if findings.is_empty() {
        return Err(Error::Projection);
    }
    Ok(findings
        .iter()
        .map(|finding| finding.diagnostic())
        .collect())
}
