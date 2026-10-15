//! Borrow retained dataset failures without cloning or inspecting host payloads.
use super::ExecutionError;
use yamaa_core::diagnostic::{
    DatasetFinding as F, Diagnostic, ProjectionError, MAX_PROJECTION_BYTES,
};

impl<E> ExecutionError<E> {
    fn classified_finding(&self) -> Option<F<'_>> {
        Some(match self {
            Self::MultipleValues {
                path,
                identifier,
                value_count,
                ..
            } => F::MultipleValues {
                path,
                identifier,
                count: *value_count,
            },
            Self::UnmatchedKey {
                path,
                dataset,
                intermediate,
                ..
            } => F::UnmatchedKey {
                path,
                dataset,
                intermediate,
            },
            Self::MultipleMatches {
                path,
                dataset,
                intermediate,
                match_count,
                ..
            } => F::MultipleMatches {
                path,
                dataset,
                intermediate,
                count: *match_count,
            },
            Self::BaselineAmbiguity {
                path,
                column,
                date,
                match_count,
                ..
            } => F::BaselineAmbiguity {
                path,
                column,
                date,
                count: *match_count,
            },
            Self::ReductionType {
                path,
                expression,
                reducer,
                source,
                actual,
            } => F::ReductionInputType {
                path,
                expression,
                reducer: *reducer,
                source,
                actual: *actual,
            },
            _ => return None,
        })
    }

    /// Project the five known dataset causes without consuming this failure.
    /// Other semantic families and opaque host errors retain their existing APIs.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        self.diagnostic_with_limit(MAX_PROJECTION_BYTES).ok()
    }

    /// A refusal leaves every original key, partition and host payload in place.
    pub fn diagnostic_with_limit(&self, maximum: usize) -> Result<Diagnostic, ProjectionError> {
        self.classified_finding()
            .ok_or(ProjectionError::InvalidContext)?
            .diagnostic_with_limit(maximum)
    }
}
