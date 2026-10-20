//! Resolve held lock origins and represent core-owned classified diagnostic facts.
use crate::{project_lock::Finding, project_source::CapturedEnvironment};
use yamaa_core::diagnostic::{lock_diagnostics, Diagnostic, LockDiagnosticError, LockOrigins};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
}
pub fn issues(
    environment: &CapturedEnvironment,
    findings: &[Finding],
) -> Result<Vec<Diagnostic>, Error> {
    // The linked environment file declares the lock, and its path resolves there.
    let origins = environment.lock().and_then(|lock| {
        let file = environment.environment_file()?;
        let document = &file.raw().document;
        let written = document
            .field(document.root(), "lock")
            .and_then(|node| document.nodes().get(node))
            .and_then(|node| match node {
                yamaa_core::schema::DocumentNode::Text(text) => Some(text.as_str()),
                _ => None,
            })?;
        Some(LockOrigins {
            source: &file.source().identity,
            entry: &environment.root().source().identity,
            lock_source: &lock.source.identity,
            written,
        })
    });
    lock_diagnostics(origins, findings).map_err(|error| match error {
        LockDiagnosticError::InvalidContext => Error::InvalidContext,
    })
}
