//! Borrow original host failures from a complete retained native graph attempt.
//! No report, callback, resource read, clone or publication is performed here.
use crate::{file_producer_build::Attempt, project_attempt};
use yamaa_engine::producer_build::Failure;

/// Visit union activation first, then actually entered node execution prefixes.
/// Native transport, retention and unwind payloads remain in their original
/// typed boundaries; they are never classified as host language exceptions.
pub fn visit_host_failures<'a, E, R, RE>(
    attempt: &'a Attempt<E, R, RE>,
    mut visit: impl FnMut(&'static str, &'a E),
) {
    if let Err(Failure::Activation(failure)) = &attempt.graph.outcome {
        project_attempt::visit_activation_failures(failure, &mut visit);
    }
    for node in &attempt.graph.nodes {
        if let Ok(execution) = &node.dataset.result {
            if let Err(error) = &execution.result {
                project_attempt::visit_execution_failure(error, &mut visit);
            }
        }
    }
}
