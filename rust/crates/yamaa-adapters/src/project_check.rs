//! Borrow complete static compiler findings from the actual retained project.
//! Unsupported vocabulary and policy/resource failures remain typed boundaries.
use crate::{
    issue_rows::Issue,
    project_run::{PreparedRun, RejectedRun},
};
use yamaa_core::specification::PrepareError;

#[derive(Debug)]
pub enum Error<'a> {
    Compile(&'a PrepareError),
    Projection(crate::specification_check::Error),
}
pub fn prepared(run: &PreparedRun) -> Result<Vec<Issue>, Error<'_>> {
    crate::specification_check::issue_rows(run).map_err(Error::Projection)
}
pub fn rejected(run: &RejectedRun) -> Result<Vec<Issue>, Error<'_>> {
    let PrepareError::Invalid(findings) = &run.rejection.error else {
        return Err(Error::Compile(&run.rejection.error));
    };
    if findings.is_empty() {
        return Err(Error::Projection(
            crate::specification_check::Error::InvalidContext,
        ));
    }
    findings
        .iter()
        .map(|finding| {
            Issue::from_core(finding.diagnostic())
                .map_err(|_| Error::Projection(crate::specification_check::Error::InvalidContext))
        })
        .collect()
}
