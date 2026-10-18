//! SAS7BDAT through the vendored ReadStat reader and its Rust callbacks.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::specification::SourceProfile;
use yamaa_fuzz::{contain_panics, run, sas};

fuzz_target!(init: contain_panics(), |data: &[u8]| {
    run(|| sas(SourceProfile::Sas7bdat, data))
});
