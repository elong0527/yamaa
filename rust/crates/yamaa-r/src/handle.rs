//! R panic and external-pointer type boundaries; no pointee is dereferenced.
use extendr_api::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub(super) fn boundary(run: impl FnOnce() -> std::result::Result<Robj, String>) -> List {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(Ok(value)) => list!(value = value, error = NULL),
        Ok(Err(error)) => list!(value = NULL, error = error),
        Err(_) => list!(value = NULL, error = "internal host boundary failure"),
    }
}
pub(super) fn address(value: &Robj) -> std::result::Result<usize, String> {
    if !value.is_external_pointer() {
        return Err("invalid native handle".into());
    }
    // Merely reads the pointer value after checking SEXPTYPE; no pointee access.
    let address = unsafe { value.external_ptr_addr::<()>() as usize };
    if address == 0 {
        return Err("expired native handle".into());
    }
    Ok(address)
}
