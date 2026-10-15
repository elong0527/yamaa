//! Owned callback failures; no request/reply protocol crosses a host callback.
use std::fmt;
use yamaa_core::project_diagnostics::HostDetails;
/// Boundary failures are distinct from normative function conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryError {
    Internal,
    Interrupted,
}
impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Internal => "internal function boundary failure",
            Self::Interrupted => "function callback interrupted",
        })
    }
}
impl std::error::Error for BoundaryError {}
type Error = BoundaryError;

/// Owned host facts. Borrowed diagnostic views retain their exact text.
#[derive(Debug)]
pub enum CallbackError {
    Exception {
        class: String,
        message: String,
    },
    Rejected {
        reason: String,
        returned: Option<String>,
    },
    Boundary(Error),
}

/// Project the admitted host classification without copying or rendering it.
pub fn details(error: &CallbackError) -> Option<HostDetails<'_>> {
    match error {
        CallbackError::Exception { class, message } => {
            Some(HostDetails::Exception { class, message })
        }
        CallbackError::Rejected { reason, returned } => Some(HostDetails::Rejected {
            reason,
            returned: returned.as_deref(),
        }),
        CallbackError::Boundary(_) => None,
    }
}
