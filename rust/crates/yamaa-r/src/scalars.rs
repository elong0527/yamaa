//! R-owned scalar representations cross Rust only as checked raw payloads.
use extendr_api::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use yamaa_adapters::scalar_bytes::{self, ScalarBytes};

/// Return normally before the R facade raises a condition; catch Rust unwinds.
fn boundary(run: impl FnOnce() -> std::result::Result<ScalarBytes, String>) -> List {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(value)) => list!(
            tag = value.tag,
            payload = Raw::from_bytes(&value.payload),
            error = NULL
        ),
        Ok(Err(error)) => list!(tag = NULL, payload = NULL, error = error),
        Err(_) => list!(
            tag = NULL,
            payload = NULL,
            error = "internal scalar boundary failure"
        ),
    }
}
/// Validate and canonicalize a scalar without interpreting R character pointers.
#[extendr]
fn scalar_bytes(tag: i32, payload: Raw) -> List {
    boundary(|| {
        scalar_bytes::decode(tag, payload.as_slice())
            .and_then(scalar_bytes::encode)
            .map_err(String::from)
    })
}

/// Return exact binary64 bits directly, without an R decimal-parser round trip.
#[extendr]
fn scalar_exact_double(tag: i32, payload: Raw) -> List {
    boundary(|| {
        let value = scalar_bytes::decode(tag, payload.as_slice())?;
        scalar_bytes::encode(scalar_bytes::exact_float(&value)?).map_err(String::from)
    })
}
/// Apply a selected scalar operator to two independently admitted byte payloads.
#[extendr]
fn scalar_operator(
    code: i32,
    left_tag: i32,
    left: Raw,
    right_tag: i32,
    right: Raw,
    unary: bool,
) -> List {
    boundary(|| {
        let left = scalar_bytes::decode(left_tag, left.as_slice())?;
        let right = if unary {
            None
        } else {
            Some(scalar_bytes::decode(right_tag, right.as_slice())?)
        };
        scalar_bytes::encode(scalar_bytes::operate(code, &left, right.as_ref())?)
            .map_err(String::from)
    })
}

extendr_module! {
    mod scalars;
    fn scalar_bytes;
    fn scalar_exact_double;
    fn scalar_operator;
}
