//! R-owned scalar representations cross Rust only as checked raw payloads.
use extendr_api::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use yamaa::{EncodedScalar, Scalar};

/// Return normally before the R facade raises a condition; catch Rust unwinds.
fn boundary(run: impl FnOnce() -> std::result::Result<yamaa::EncodedScalar, String>) -> List {
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

fn admit(tag: i32, payload: &Raw) -> std::result::Result<Scalar, String> {
    EncodedScalar::decode(tag, payload.as_slice()).map_err(|error| error.reason().into())
}

/// Validate and canonicalize a scalar without interpreting R character pointers.
#[extendr]
fn scalar_bytes(tag: i32, payload: Raw) -> List {
    boundary(|| admit(tag, &payload).map(EncodedScalar::encode))
}

/// Return exact binary64 bits directly, without an R decimal-parser round trip.
#[extendr]
fn scalar_exact_double(tag: i32, payload: Raw) -> List {
    boundary(|| {
        let value = admit(tag, &payload)?;
        Ok(EncodedScalar::encode(
            yamaa::exact_float(&value).map_err(|error| error.reason().to_owned())?,
        ))
    })
}
/// Apply a selected scalar operator to two independently admitted byte payloads.
#[extendr]
fn scalar_operator_code(name: Raw, unary: bool) -> List {
    let code = std::str::from_utf8(name.as_slice())
        .ok()
        .and_then(|name| yamaa::scalar_operator_code(name, unary));
    match code {
        Some(code) => list!(value = code, error = NULL),
        None => list!(value = NULL, error = "unsupported scalar operator"),
    }
}

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
        let left = admit(left_tag, &left)?;
        let right = if unary {
            None
        } else {
            Some(admit(right_tag, &right)?)
        };
        Ok(EncodedScalar::encode(
            yamaa::apply_scalar_operator(code, &left, right.as_ref())
                .map_err(|error| error.message().to_owned())?,
        ))
    })
}

include!("generated/scalars.rs");
