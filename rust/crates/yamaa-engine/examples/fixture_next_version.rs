//! A fixture engine of the next boundary version, for the loading tests
//! (`tests/abi_loading.rs`, #3702): it answers every request as a mismatch
//! that names its own version, the next one, reading only the request's
//! header and writing only the outcome's fixed part, as an engine one
//! boundary version newer answers this version's request. It holds no
//! table, since no test asks for its version.
use std::mem::offset_of;

use yamaa_boundary::{abi, BOUNDARY_VERSION};

/// The negotiation entry of an engine of the next boundary version.
/// Built with the crate's `dynamic-engine` feature, under which the engine
/// exports its own entry from every library that links it, the fixture
/// exports none, and the loading tests are left out.
///
/// # Safety
///
/// As `yamaa_engine::negotiate` asks of `request` and `outcome`.
#[cfg_attr(not(feature = "dynamic-engine"), no_mangle)]
pub unsafe extern "C" fn yamaa_engine_negotiate(
    request: *const abi::Request,
    outcome: *mut abi::Outcome,
) -> *const abi::Engine {
    let header = offset_of!(abi::Request, required);
    let fixed = offset_of!(abi::Outcome, reason) + size_of::<u32>();
    // SAFETY: the caller's contract; each field is read or written through
    // the raw pointer within the header the caller's size covers.
    unsafe {
        let requested = if request.is_null() || ((*request).size as usize) < header {
            0
        } else {
            (*request).version
        };
        if !outcome.is_null() && ((*outcome).size as usize) >= fixed {
            (*outcome).accepted = 0;
            (*outcome).requested_version = requested;
            (*outcome).supported_version = BOUNDARY_VERSION + 1;
            (*outcome).reason = abi::REASON_VERSION_MISMATCH;
        }
    }
    std::ptr::null()
}
