//! Serialize bounded core-owned binding and conformance diagnostic facts.
use crate::{issue_rows::Issue, project_diagnostic_projection::finish};
pub use yamaa_core::project_diagnostics::{DefinitionSource, Error, HostDetails};
use yamaa_engine::project_activation::{BindingFailure, TestFailure};

pub fn binding_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [BindingFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    let maximum = crate::specification_check::MAX_ISSUE_BYTES;
    finish(
        yamaa_engine::project_diagnostics::binding_diagnostics(
            definitions,
            failures,
            details,
            maximum,
        )?,
        maximum,
    )
}
pub fn test_issues<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
) -> Result<Vec<Issue>, Error<'a, E>> {
    test_issues_with_limit(
        definitions,
        failures,
        details,
        crate::specification_check::MAX_ISSUE_BYTES,
    )
}
pub fn test_issues_with_limit<'a, E>(
    definitions: &[DefinitionSource<'_>],
    failures: &'a [TestFailure<E>],
    details: &[Option<HostDetails<'_>>],
    maximum: usize,
) -> Result<Vec<Issue>, Error<'a, E>> {
    finish(
        yamaa_engine::project_diagnostics::test_diagnostics(
            definitions,
            failures,
            details,
            maximum,
        )?,
        maximum,
    )
}
