//! Parquet admission and decoding: framing, footer and page admission, the
//! native decoder and the closed profile, under both source interpretations.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::specification::{SourceInterpretation, SourceProfile};
use yamaa_fuzz::{contain_panics, decode, run, source};

fuzz_target!(init: contain_panics(), |data: &[u8]| {
    run(|| {
        let source = source(SourceProfile::Parquet, Vec::new(), None);
        for interpretation in [SourceInterpretation::Ordinary, SourceInterpretation::Odm] {
            let _ = decode(&source, data, interpretation);
        }
    })
});
