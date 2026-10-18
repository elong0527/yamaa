//! R external-pointer identity; no pointee is dereferenced.
use extendr_api::prelude::*;

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
