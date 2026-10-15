//! Serialize complete core-owned binding and conformance diagnostic facts.
use crate::{issue_rows::Issue, project_diagnostic_projection::finish};
use yamaa_core::project_activation::{BindingFailure, TestFailure};
pub use yamaa_core::project_diagnostics::{DefinitionSource, Error, HostDetails};

pub fn binding_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [BindingFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    finish(yamaa_core::project_diagnostics::binding_diagnostics(
        definitions,
        failures,
        details,
    )?)
}
pub fn test_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    finish(yamaa_core::project_diagnostics::test_diagnostics(
        definitions,
        failures,
        details,
    )?)
}
