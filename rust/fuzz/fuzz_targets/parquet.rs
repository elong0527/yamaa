//! Parquet admission and decoding: framing, footer and page admission, the
//! native decoder and the closed profile, under both source interpretations.
//! A read by partition, a run of whole row groups at a time, gives the whole
//! read's table or finding (#3792).
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::specification::{SourceInterpretation, SourceProfile};
use yamaa_fuzz::{decode, partitioned_reads_match_the_whole_read, source};

fuzz_target!(|data: &[u8]| {
    let source = source(SourceProfile::Parquet, Vec::new(), None);
    for interpretation in [SourceInterpretation::Ordinary, SourceInterpretation::Odm] {
        let _ = decode(&source, data, interpretation);
        partitioned_reads_match_the_whole_read(&source, data, interpretation);
    }
});
