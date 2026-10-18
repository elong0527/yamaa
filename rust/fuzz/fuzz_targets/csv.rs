//! The closed CSV profile and declared typing: undeclared under both source
//! interpretations, then every admitted field declared as each typed column
//! type in turn, so each conversion runs over text the fuzzer controls.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::{
    specification::{SourceInterpretation, SourceProfile},
    value::ColumnType,
};
use yamaa_fuzz::{decode, source};

fuzz_target!(|data: &[u8]| {
    let undeclared = source(SourceProfile::Csv, Vec::new(), None);
    for interpretation in [SourceInterpretation::Ordinary, SourceInterpretation::Odm] {
        let _ = decode(&undeclared, data, interpretation);
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
        let _ = decode(
            &source(SourceProfile::Csv, types, None),
            data,
            SourceInterpretation::Ordinary,
        );
    }
});
