//! Synchronous R callback capability; all R access stays on the calling thread.
use extendr_api::prelude::*;

/// Decode exact host detail bytes strictly.
/// Invalid encodings retain a stable fallback; raw invalid text is never repaired.
pub(super) fn detail(value: &Robj, fallback: &str) -> std::result::Result<String, String> {
    let bytes = value
        .as_raw()
        .ok_or_else(|| "invalid host detail".to_owned())?;
    let text = match std::str::from_utf8(bytes.as_slice()) {
        Ok(text) => text,
        Err(_) => return Ok(fallback.into()),
    };
    Ok(text.into())
}
