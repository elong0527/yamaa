//! Represent core-owned facts from one retained versionless invocation failure.
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::function_signature::ProjectFunctionIdentity;
pub use yamaa_core::project_diagnostics::{Error, HostDetails};
use yamaa_core::{dataset::RowIdentity, function_invocation::InvocationFailure};

/// Preserve the original failure and identity; projection never invokes a port.
pub fn invocation<'a, E>(
    path: &str,
    failure: &'a InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&RowIdentity>,
    key_names: &[&str],
    host: Option<HostDetails<'_>>,
) -> Result<Diagnostic, Error<'a, E>> {
    yamaa_core::project_diagnostics::invocation(path, failure, row, key_names, host)
        .map(crate::project_diagnostic_projection::owned)
}
