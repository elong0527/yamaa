//! Authored TSV fixture tables, read the one way every reader in the suite
//! reads them.
//!
//! A table is a header line naming its columns, then one case per line, with
//! one tab between fields. `cases` compares the header with the columns its
//! caller names, so a renamed, added or reordered column fails where the
//! table is read; refuses a line whose fields do not fill exactly those
//! columns, citing the line; and compares the number of cases with the
//! authored count, so a truncated or emptied table fails instead of passing
//! with no cases (CONTRIBUTING.md#rust). Each case comes back as an array, so
//! its reader names the fields by destructuring rather than by position.

/// The cases of `table`, each split into exactly `columns`.
///
/// Panics, at the caller, when the header is not `columns`, when a line has
/// another number of fields, or when the table holds other than `count`
/// cases.
#[track_caller]
pub fn cases<'a, const N: usize>(
    table: &'a str,
    columns: [&str; N],
    count: usize,
) -> Vec<[&'a str; N]> {
    let mut lines = table.lines();
    let header: Vec<_> = lines.next().unwrap_or_default().split('\t').collect();
    assert_eq!(header, columns, "the fixture's header");
    let mut cases = Vec::new();
    for (index, line) in lines.enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        match <[&str; N]>::try_from(fields) {
            Ok(case) => cases.push(case),
            Err(fields) => panic!(
                "the fixture's line {} has {} fields where its header names {N}: {line:?}",
                index + 2,
                fields.len(),
            ),
        }
    }
    assert_eq!(cases.len(), count, "the fixture's case count");
    cases
}
