//! Serialize core-owned facts from one retained versionless invocation failure.
use crate::{issue_rows::Issue, project_diagnostic_projection::finish};
use yamaa_core::function_signature::ProjectFunctionIdentity;
pub use yamaa_core::project_diagnostics::{Error, HostDetails};
use yamaa_engine::{dataset::RowIdentity, function_invocation::InvocationFailure};

/// Preserve the original failure and identity; projection never invokes a port.
pub fn invocation<'a, E>(
    path: &str,
    failure: &'a InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&RowIdentity>,
    key_names: &[&str],
    host: Option<HostDetails<'_>>,
) -> Result<Issue, Error<'a, E>> {
    let diagnostic =
        yamaa_engine::project_diagnostics::invocation(path, failure, row, key_names, host)?;
    let mut issues = finish(
        vec![diagnostic],
        crate::specification_check::MAX_ISSUE_BYTES,
    )?;
    Ok(issues.remove(0))
}
