//! Typed failure projection shared by native inheritance consumers. Opaque host
//! failures and unclassified resource findings retain their original ownership.
use crate::inheritance::Error;
use yamaa_core::diagnostic::{Diagnostic, ProjectionError};

pub enum Outcome<E> {
    Diagnostic(Diagnostic),
    Original(Error<E>),
}

pub fn traversal_outcome<E>(
    error: Error<E>,
    maximum: usize,
) -> Result<Outcome<E>, ProjectionError> {
    match error.diagnostic_with_limit(maximum)? {
        Some(finding) => Ok(Outcome::Diagnostic(finding)),
        None => Ok(Outcome::Original(error)),
    }
}
