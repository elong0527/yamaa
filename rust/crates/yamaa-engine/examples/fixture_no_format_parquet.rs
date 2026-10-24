//! A fixture engine without the `format_parquet` format, for the loading
//! tests (`tests/abi_loading.rs`, #3702): it negotiates as the engine does,
//! with `format_parquet` withheld from what it supports, so a request that
//! asks for it as optional is accepted with the format left disabled, and
//! the engine's own table is returned.
use yamaa_boundary::abi;

/// The negotiation entry of an engine without `format_parquet`.
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
    // SAFETY: the caller's contract, which is the engine's entry's.
    unsafe { yamaa_engine::negotiate_withholding(request, outcome, &["format_parquet"]) }
}
