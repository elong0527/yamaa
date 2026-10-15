//! Typed failure projection shared by native inheritance consumers. Opaque host
//! failures and unclassified resource findings retain their original ownership.
use crate::diagnostic::{Diagnostic, ProjectionError};
use crate::inheritance::Error;

pub enum Outcome<E> {
    Diagnostic(Diagnostic),
    Original(Error<E>),
}

pub fn traversal_outcome<E>(error: Error<E>) -> Result<Outcome<E>, ProjectionError> {
    match error.diagnostic()? {
        Some(finding) => Ok(Outcome::Diagnostic(finding)),
        None => Ok(Outcome::Original(error)),
    }
}
