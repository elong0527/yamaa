//! Project retained activation failures through core without host callbacks.
use crate::{
    dataset::RowIdentity,
    function_invocation::InvocationFailure,
    project_activation::{BindingFailure, TestFailure},
};
use alloc::vec::Vec;
pub use core::{DefinitionSource, HostDetails};
use yamaa_core::{
    function_signature::ProjectFunctionIdentity,
    project_diagnostics::{self as core, Diagnostic, Error},
};

pub fn invocation<'a, 'e: 'a, E>(
    path: &str,
    failure: &'e InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&'a RowIdentity>,
    key_names: &[&'a str],
    host: Option<HostDetails<'a>>,
) -> Result<Diagnostic<'a>, Error<'e, E>> {
    core::invocation(
        path,
        &failure.diagnostic_facts(),
        row.map(|row| row.values.as_slice()),
        key_names,
        host,
    )
}
pub fn binding_diagnostics<'a, 'e, E>(
    definitions: &[DefinitionSource<'a>],
    failures: &'e [BindingFailure<E>],
    details: &[Option<HostDetails<'a>>],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error<'e, E>> {
    // Bound shallow view allocation before borrowing each retained payload.
    if failures.len() > 1_024 {
        return Err(Error::Limit);
    }
    let facts: Vec<_> = failures
        .iter()
        .map(|failure| core::BindingFailure {
            function: failure.function,
            error: &failure.error,
        })
        .collect();
    core::binding_diagnostics(definitions, &facts, details, maximum)
}
pub fn test_diagnostics<'a, 'e: 'a, E>(
    definitions: &[DefinitionSource<'a>],
    failures: &'e [TestFailure<E>],
    details: &[Option<HostDetails<'a>>],
    maximum: usize,
) -> Result<Vec<Diagnostic<'a>>, Error<'e, E>> {
    if failures.len() > 65_536 {
        return Err(Error::Limit);
    }
    let facts: Vec<_> = failures
        .iter()
        .map(|failure| match failure {
            TestFailure::Invocation {
                function,
                case,
                error,
            } => core::TestFailure::Invocation {
                function: *function,
                case: *case,
                error: error.diagnostic_facts(),
            },
            TestFailure::Result {
                function,
                case,
                actual,
                expected,
            } => core::TestFailure::Result {
                function: *function,
                case: *case,
                actual,
                expected,
            },
        })
        .collect();
    core::test_diagnostics(definitions, &facts, details, maximum)
}
