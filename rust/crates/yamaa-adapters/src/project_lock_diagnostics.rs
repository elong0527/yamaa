//! Resolve held lock origins and represent core-owned classified diagnostic facts.
use crate::{issue_rows::Issue, project_lock::Finding, project_source::CapturedEnvironment};
use yamaa_core::diagnostic::{lock_diagnostics_with_limit, LockDiagnosticError, LockOrigins};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
    Limit,
    Projection,
}
pub fn issues(
    environment: &CapturedEnvironment,
    findings: &[Finding],
) -> Result<Vec<Issue>, Error> {
    issues_with_limit(
        environment,
        findings,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
pub fn issues_with_limit(
    environment: &CapturedEnvironment,
    findings: &[Finding],
    maximum: usize,
) -> Result<Vec<Issue>, Error> {
    let document = &environment.root().raw().document;
    let origins = environment.lock().and_then(|lock| {
        let written = document
            .field(document.root(), "lock")
            .and_then(|node| document.nodes().get(node))
            .and_then(|node| match node {
                yamaa_core::schema::DocumentNode::Text(text) => Some(text.as_str()),
                _ => None,
            })?;
        Some(LockOrigins {
            source: &environment.root().source().identity,
            lock_source: &lock.source.identity,
            written,
        })
    });
    let diagnostics =
        lock_diagnostics_with_limit(origins, findings, maximum).map_err(|error| match error {
            LockDiagnosticError::InvalidContext => Error::InvalidContext,
            LockDiagnosticError::Limit => Error::Limit,
        })?;
    if diagnostics.is_empty() {
        return Ok(Vec::new());
    }
    let result = diagnostics
        .into_iter()
        .map(Issue::from_core)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Projection)?;
    if !crate::issue_rows::within_limit(&result, maximum) {
        return Err(Error::Limit);
    }
    Ok(result)
}
