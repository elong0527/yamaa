//! SAS7BDAT through the vendored ReadStat reader and its Rust callbacks.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::specification::SourceProfile;

fuzz_target!(|data: &[u8]| yamaa_fuzz::sas(SourceProfile::Sas7bdat, data));
