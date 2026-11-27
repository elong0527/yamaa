//! The positional reading of the csv profile (CSV-18): the leading lines it
//! skips, then each record's fields at their declared character positions.
//! The first bytes of each input declare the reading, as a validated
//! `fixed_width` would: a `skip` count and up to four fields, each beginning
//! after the previous one's last position (the declaration's check). The rest
//! is the text, read undeclared, then with every field declared each typed
//! column type in turn. Each read by partition, the record scan then one
//! range of records at a time, gives the whole read's table or finding
//! (#3792).
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::{
    csv_source::{FixedWidth, PositionalField},
    specification::{SourceInterpretation, SourceProfile},
    value::ColumnType,
};
use yamaa_fuzz::{decode, partitioned_reads_match_the_whole_read, source};

/// A reading from the input's first bytes, and the text after them: one
/// byte for the skip count and the field count, then one byte per field for
/// its gap after the previous field and its width.
fn layout(data: &[u8]) -> Option<(FixedWidth, &[u8])> {
    let (&head, rest) = data.split_first()?;
    let skip = usize::from(head & 0x3);
    let count = 1 + usize::from((head >> 2) & 0x3);
    if rest.len() < count {
        return None;
    }
    let (widths, text) = rest.split_at(count);
    let mut last = 0;
    let fields = widths
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            let first = last + 1 + usize::from(byte >> 6);
            last = first + usize::from(byte & 0x3f);
            PositionalField {
                name: format!("F{}", index + 1),
                first,
                last,
            }
        })
        .collect();
    Some((FixedWidth { skip, fields }, text))
}

fuzz_target!(|data: &[u8]| {
    let Some((layout, text)) = layout(data) else {
        return;
    };
    let names = layout.names();
    for kind in [
        None,
        Some(ColumnType::Int),
        Some(ColumnType::Float),
        Some(ColumnType::Date),
        Some(ColumnType::DateTime),
        Some(ColumnType::Time),
    ] {
        let types = kind
            .map(|kind| names.iter().map(|name| (name.clone(), kind)).collect())
            .unwrap_or_default();
        let mut declared = source(SourceProfile::Csv, types, None);
        declared.fixed_width = Some(layout.clone());
        let _ = decode(&declared, text, SourceInterpretation::Ordinary);
        partitioned_reads_match_the_whole_read(&declared, text, SourceInterpretation::Ordinary);
    }
});
