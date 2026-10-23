//! A fixture library without the negotiation entry, for the loading tests
//! (`tests/abi_loading.rs`, #3702): a library under the engine's file name
//! that exports no `yamaa_engine_negotiate`, as one built before the
//! boundary or for something else.

/// The library's one export, which no loader looks up.
#[no_mangle]
pub extern "C" fn yamaa_engine_fixture_without_entry() -> u32 {
    0
}
