//! Owned callback failures; no request/reply protocol crosses a host callback.
use std::fmt;
pub const MAX_RESULT_BYTES: usize = 1_048_576;
pub const MAX_ERROR_BYTES: usize = yamaa_core::project_diagnostics::MAX_HOST_DETAIL_BYTES;
/// Boundary failures are distinct from normative function conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryError {
    OutputLimit,
    Internal,
    Interrupted,
}
impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutputLimit => "function output exceeds resource limit",
            Self::Internal => "internal function boundary failure",
            Self::Interrupted => "function callback interrupted",
        })
    }
}
impl std::error::Error for BoundaryError {}
type Error = BoundaryError;

/// Host-owned details; the callback boundary enforces byte caps before serializing them.
#[derive(Debug)]
pub enum CallbackError {
    Exception {
        class: String,
        message: String,
        truncated: bool,
    },
    Rejected {
        reason: String,
        returned: Option<String>,
    },
    Boundary(Error),
}
