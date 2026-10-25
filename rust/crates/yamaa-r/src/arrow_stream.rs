//! The Arrow C stream a streamed run's output crosses into R as (#3697): an
//! external pointer whose address is a standard `ArrowArrayStream`, of class
//! `nanoarrow_array_stream`, the class nanoarrow and arrow read. The pointer
//! owns the stream until a reader moves it out, which leaves the stream's
//! release callback null; its finalizer releases a stream nobody moved,
//! which ends a run's production when that stream had started it. The
//! finalizer calls no R code and no project code.
use extendr_api::prelude::*;
use std::ffi::{c_int, c_void};
use yamaa::FFI_ArrowArrayStream;

// R's SEXP is an opaque pointer, as in `crate::interrupt`.
type Sexp = *mut c_void;

/// An R object owning `stream`, a standard Arrow C stream.
pub(crate) fn stream_object(stream: FFI_ArrowArrayStream) -> std::result::Result<Robj, String> {
    let address = Box::into_raw(Box::new(stream));
    // SAFETY: this runs on the R thread, inside a native call. The external
    // pointer is protected before anything else allocates: Rf_protect keeps
    // it while the finalizer is registered and extendr takes its own
    // protection, then Rf_unprotect balances the stack. Its address is the
    // boxed header, which `finalize` frees once, on the R thread.
    //
    // R_MakeExternalPtr and R_RegisterCFinalizerEx allocate, and an
    // allocation R cannot make, when memory is exhausted, raises an R error
    // whose longjmp would cross this frame and the binding's frames above it
    // without running their destructors, leaking the box. Every allocation
    // the binding makes through extendr has the same exposure, extendr's
    // `ExternalPtr::new`, which the result handles use, included; containing
    // it would need R_UnwindProtect with a cleanup that cannot stop the
    // unwind from Rust.
    let mut object = unsafe {
        let pointer = Rf_protect(R_MakeExternalPtr(address.cast(), R_NilValue, R_NilValue));
        R_RegisterCFinalizerEx(pointer, Some(finalize), 1);
        let object = Robj::from_sexp(pointer.cast());
        Rf_unprotect(1);
        object
    };
    object
        .set_class(["nanoarrow_array_stream"])
        .map_err(|_| "stream class")?;
    Ok(object)
}

/// Free the boxed header of a collected stream object, releasing the stream
/// when no reader moved it out.
unsafe extern "C" fn finalize(pointer: Sexp) {
    // SAFETY: R calls this once per object, on the R thread, with the
    // pointer `stream_object` made, whose address is the box it leaked or
    // null once cleared. Clearing it first makes a second call a no-op.
    unsafe {
        let address = R_ExternalPtrAddr(pointer).cast::<FFI_ArrowArrayStream>();
        if address.is_null() {
            return;
        }
        R_ClearExternalPtr(pointer);
        drop(Box::from_raw(address));
    }
}

extern "C" {
    static R_NilValue: Sexp;
    fn Rf_protect(object: Sexp) -> Sexp;
    fn Rf_unprotect(count: c_int);
    fn R_MakeExternalPtr(address: *mut c_void, tag: Sexp, prot: Sexp) -> Sexp;
    fn R_ExternalPtrAddr(pointer: Sexp) -> *mut c_void;
    fn R_ClearExternalPtr(pointer: Sexp);
    fn R_RegisterCFinalizerEx(
        pointer: Sexp,
        finalizer: Option<unsafe extern "C" fn(Sexp)>,
        onexit: c_int,
    );
}
