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
        let written = environment
            .project()?
            .environments
            .iter()
            .find_map(|environment| environment.lock.as_ref())?;
        Some(LockOrigins {
            source: &file.source().identity,
            entry: &environment.root().source().identity,
            lock_source: &lock.source.identity,
            written: &written.text,
        })
    });
    lock_diagnostics(origins, findings).map_err(|error| match error {
        LockDiagnosticError::InvalidContext => Error::InvalidContext,
    })
}
