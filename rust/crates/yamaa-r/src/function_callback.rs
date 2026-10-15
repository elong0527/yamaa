//! Synchronous R callback capability; all R access stays on the calling thread.
use extendr_api::prelude::*;
use yamaa_adapters::callback::{BoundaryError as Error, CallbackError, MAX_ERROR_BYTES};
use yamaa_engine::function_invocation::HostError;

/// Unexpected dispatcher failures are boundary errors, not repaired host results.
fn internal() -> HostError<CallbackError> {
    HostError::Raised(CallbackError::Boundary(Error::Internal))
}

/// Decode host detail bytes strictly and cap only at a Unicode scalar boundary.
/// Invalid encodings retain a stable fallback; raw invalid text is never repaired.
pub(super) fn detail(
    value: &Robj,
    fallback: &str,
) -> std::result::Result<(String, bool), HostError<CallbackError>> {
    let bytes = value.as_raw().ok_or_else(internal)?;
    let text = match std::str::from_utf8(bytes.as_slice()) {
        Ok(text) => text,
        Err(_) => return Ok((fallback.into(), false)),
    };
    let mut end = text.len().min(MAX_ERROR_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok((text[..end].into(), end < text.len()))
}
