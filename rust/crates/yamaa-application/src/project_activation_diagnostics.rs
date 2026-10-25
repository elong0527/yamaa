//! Represent complete core-owned binding and conformance diagnostic facts.
use crate::project_diagnostic_projection::owned;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::project_activation::{BindingFailure, TestFailure};
pub use yamaa_core::project_diagnostics::{DefinitionSource, Error, HostDetails};

pub fn binding_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [BindingFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Diagnostic>, Error<'a, E>> {
    Ok(
        yamaa_core::project_diagnostics::binding_diagnostics(definitions, failures, details)?
            .into_iter()
            .map(owned)
            .collect(),
    )
}
pub fn test_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Diagnostic>, Error<'a, E>> {
    Ok(
        yamaa_core::project_diagnostics::test_diagnostics(definitions, failures, details)?
            .into_iter()
            .map(owned)
            .collect(),
    )
}
