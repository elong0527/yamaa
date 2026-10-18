//! Borrow retained dataset failures without cloning or inspecting host payloads.
use super::ExecutionError;
use crate::diagnostic::{DatasetFinding as F, Diagnostic, ProjectionError};

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
                implicit_join,
                ..
            } => F::UnmatchedKey {
                path,
                dataset,
                intermediate,
                implicit_join: *implicit_join,
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
    pub fn diagnostic(&self) -> Result<Option<Diagnostic>, ProjectionError> {
        self.classified_finding().map(F::diagnostic).transpose()
    }
}
