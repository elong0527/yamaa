//! The closed CSV profile and declared typing: undeclared under both source
//! interpretations, then every admitted field declared as each typed column
//! type in turn, so each conversion runs over text the fuzzer controls. A
//! declared byte-order mark is skipped once (CSV-10): the mark followed by
//! the input reads as the input alone, unless the input starts with a mark of
//! its own. Each read by partition, the record scan then one range of records
//! at a time, gives the whole read's table or finding (#3792).
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::{
    specification::{SourceInterpretation, SourceProfile},
    value::ColumnType,
};
use yamaa_fuzz::{decode, partitioned_reads_match_the_whole_read, source};

fuzz_target!(|data: &[u8]| {
    let undeclared = source(SourceProfile::Csv, Vec::new(), None);
    for interpretation in [SourceInterpretation::Ordinary, SourceInterpretation::Odm] {
        let _ = decode(&undeclared, data, interpretation);
        partitioned_reads_match_the_whole_read(&undeclared, data, interpretation);
    }
    let mut marked = undeclared.clone();
    marked.byte_order_mark = true;
    let _ = decode(&marked, data, SourceInterpretation::Ordinary);
    partitioned_reads_match_the_whole_read(&marked, data, SourceInterpretation::Ordinary);
    if !data.starts_with(b"\xef\xbb\xbf") {
        let declared = [b"\xef\xbb\xbf".as_slice(), data].concat();
        assert_eq!(
            yamaa_adapters::csv_source::parse_declared(&declared, true),
            yamaa_adapters::csv_source::parse(data)
        );
    }
    let Ok(parsed) = yamaa_adapters::csv_source::parse(data) else {
        return;
    };
    for kind in [
        ColumnType::Int,
        ColumnType::Float,
        ColumnType::Date,
        ColumnType::DateTime,
    ] {
        let types = parsed
            .names
            .iter()
            .map(|name| (name.clone(), kind))
            .collect();
        let typed = source(SourceProfile::Csv, types, None);
        let _ = decode(&typed, data, SourceInterpretation::Ordinary);
        partitioned_reads_match_the_whole_read(&typed, data, SourceInterpretation::Ordinary);
    }
});
