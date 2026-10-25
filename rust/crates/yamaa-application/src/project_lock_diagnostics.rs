//! Resolve held lock origins and represent core-owned classified diagnostic facts.
use crate::{project_lock::Finding, project_source::CapturedEnvironment};
use yamaa_core::diagnostic::{lock_diagnostics, Diagnostic, LockDiagnosticError, LockOrigins};
use yamaa_core::project_function::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidContext,
}
/// The lock findings of `host`, the language that verified the lock, each at
/// the field that declares the lock: the project file's
/// `environments.<host>.lock`, which the lock's path resolves from (FUNC-3,
/// D8). Only a host language verifies a lock, so without one no finding has
/// an origin.
pub fn issues(
    environment: &CapturedEnvironment,
    host: Option<Language>,
    findings: &[Finding],
) -> Result<Vec<Diagnostic>, Error> {
    let entry = environment
        .root_optional()
        .map(|root| &root.source().identity);
    let origins = environment.lock().and_then(|lock| {
        let project = environment.project()?;
        let host = host?;
        let written = project.environment(host)?.lock.as_ref()?;
        Some(LockOrigins {
            source: entry?,
            entry: entry?,
            field: yamaa_core::project_environment_diagnostics::lock_field(host),
            lock_source: &lock.source.identity,
            written: &written.text,
        })
    });
    lock_diagnostics(origins, findings).map_err(|error| match error {
        LockDiagnosticError::InvalidContext => Error::InvalidContext,
    })
}
