//! Read one retained typed attempt without host calls, retries or publication.
use crate::project_run::{Attempt, BoundaryFailure};
use yamaa_engine::{
    dataset::{Execution, ExecutionError},
    function_invocation::FailureKind,
    project_activation::{Failure, TestFailure},
};

/// Dataset execution success is a separate gate from output admission and save.
pub fn execution<C, E>(attempt: &Attempt<C, E>) -> Option<&Execution> {
    attempt.boundary.as_ref().ok()?;
    attempt.dataset.result.as_ref().ok()?.result.as_ref().ok()
}

/// Borrow original host payloads in actual failure order. Scalar/type failures
/// have no host payload; unwind payloads remain in the original typed boundary.
pub fn visit_host_failures<'a, C, E>(
    attempt: &'a Attempt<C, E>,
    mut visit: impl FnMut(&'static str, &'a E),
) {
    let invocation =
        |stage, kind: &'a FailureKind<E>, visit: &mut dyn FnMut(&'static str, &'a E)| {
            if let FailureKind::CallFailed(error) | FailureKind::InvalidHostResult(error) = kind {
                visit(stage, error);
            }
        };
    if let Err(BoundaryFailure::Activation(failure)) = &attempt.boundary {
        match failure {
            Failure::Lock(error) => visit("lock", error),
            Failure::Bindings(failures) => {
                for failure in failures {
                    visit("binding", &failure.error);
                }
            }
            Failure::Tests(failures) => {
                for failure in failures {
                    if let TestFailure::Invocation { error, .. } = failure {
                        invocation("conformance", &error.kind, &mut visit);
                    }
                }
            }
            Failure::InterruptedBinding { error, .. } => visit("binding", error),
            Failure::InterruptedInvocation { error, .. } => {
                invocation("conformance", &error.kind, &mut visit)
            }
            _ => {}
        }
    }
    if let Ok(execution) = &attempt.dataset.result {
        if let Err(error) = &execution.result {
            match error.as_ref() {
                ExecutionError::Function { error, .. } => {
                    invocation("derivation", &error.kind, &mut visit)
                }
                ExecutionError::ProjectFunction { error, .. } => {
                    invocation("derivation", &error.kind, &mut visit)
                }
                _ => {}
            }
        }
    }
}
