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
    rejected_with_limit(run, crate::specification_check::MAX_ISSUE_BYTES)
}
pub fn rejected_with_limit(run: &RejectedRun, maximum: usize) -> Result<Vec<Issue>, Error<'_>> {
    let PrepareError::Invalid(findings) = &run.rejection.error else {
        return Err(Error::Compile(&run.rejection.error));
    };
    if findings.is_empty() {
        return Err(Error::Projection(
            crate::specification_check::Error::InvalidContext,
        ));
    }
    // Bound both JSON layers, generated paths, field overhead and collection
    // entries before projecting the first finding. Never return a fitting prefix.
    let mut remaining = Some(maximum);
    for finding in findings {
        remaining = remaining.and_then(|size| size.checked_sub(1024));
        finding.visit_diagnostic_text(|text| {
            remaining = text
                .len()
                .checked_mul(24)
                .and_then(|size| size.checked_add(128))
                .and_then(|charge| remaining.and_then(|size| size.checked_sub(charge)));
        });
        if remaining.is_none() {
            return Err(Error::Projection(crate::specification_check::Error::Limit));
        }
    }
    let issues = findings
        .iter()
        .map(|finding| {
            Issue::from_core(finding.diagnostic())
                .map_err(|_| Error::Projection(crate::specification_check::Error::InvalidContext))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !crate::issue_rows::within_limit(&issues, maximum) {
        return Err(Error::Projection(crate::specification_check::Error::Limit));
    }
    Ok(issues)
}
