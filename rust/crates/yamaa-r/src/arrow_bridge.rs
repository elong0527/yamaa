//! R nanoarrow bridge: fill a fresh nanoarrow-owned C Stream header.
use extendr_api::prelude::*;
use yamaa_adapters::{arrow_bridge::FFI_ArrowArrayStream, public_table::PublicTable};

pub(crate) fn import(
    source: Robj,
) -> std::result::Result<yamaa_adapters::arrow_table::ArrowTable, String> {
    if !source.is_external_pointer() || !source.inherits("nanoarrow_array_stream") {
        return Err("expected a nanoarrow array stream".into());
    }
    // SAFETY: a compliant native nanoarrow producer owns this aligned standard
    // stream shell. Move clears its release callback while the R owner is rooted.
    let pointer =
        unsafe { extendr_api::R_ExternalPtrAddr(source.get()) }.cast::<FFI_ArrowArrayStream>();
    if pointer.is_null() {
        return Err("null Arrow stream source".into());
    }
    let stream = unsafe { FFI_ArrowArrayStream::from_raw(pointer) };
    yamaa_adapters::arrow_bridge::import_public_stream(stream)
        .map_err(|_| "unsupported host frame".into())
}

pub(crate) fn export(table: &PublicTable, destination: Robj) -> std::result::Result<Robj, String> {
    if !destination.is_external_pointer() || !destination.inherits("nanoarrow_array_stream") {
        return Err("expected a fresh nanoarrow array stream".into());
    }
    // SAFETY: The private R facade supplies nanoarrow_allocate_array_stream().
    // Its external pointer directly addresses an aligned ArrowArrayStream and
    // its nanoarrow finalizer owns that shell. Interoperating native producers
    // must satisfy that standard ABI contract; a forged pointer is not valid.
    let pointer =
        unsafe { extendr_api::R_ExternalPtrAddr(destination.get()) }.cast::<FFI_ArrowArrayStream>();
    if pointer.is_null() {
        return Err("null Arrow stream destination".into());
    }
    // SAFETY: A fresh allocated stream has release=NULL. Refuse to overwrite a
    // live owner. Transfer our header once; nanoarrow now invokes its release
    // on consumption/error/GC, and then frees its own allocated shell.
    unsafe {
        if (*pointer).release().is_some() {
            return Err("Arrow stream destination already owns a stream".into());
        }
        pointer.write(table.stream());
    }
    Ok(destination)
}
