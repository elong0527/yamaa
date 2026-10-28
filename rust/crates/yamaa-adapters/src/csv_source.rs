//! The closed CSV input profile's parser over one captured byte snapshot,
//! and the undeclared-type text table it builds. Pure byte admission: the
//! profile's models (the text table and its refusals) are core's, and
//! filesystem authority stays with the resolver port (#3030 section 2.10).
//!
//! One lexer reads the profile (CSV-1 to CSV-8). The whole read keeps every
//! record as it scans. A read by partition (#3792) scans every record once,
//! keeping only where each partition of records begins, and then decodes one
//! partition's records on demand ([`Partitions::records`]). Both scans report
//! the same finding at the same record and field; only the row and cell
//! limits differ: the whole read admits the whole table, a read by partition
//! each partition on its own.
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
pub use yamaa_core::csv_source::{CsvSource, Error, Field, ProfileCause};

fn failure(cause: ProfileCause, record: usize, field: usize) -> Error {
    Error::Profile {
        cause,
        record,
        field: Field::Index(field),
    }
}
fn limit_failure(error: yamaa_core::limits::LimitExceeded) -> Error {
    Error::Limit {
        resource: error.resource,
        limit: error.limit,
        actual: error.actual,
    }
}

/// UTF-8 failure coordinates follow quote-aware prefix scanning even if that
/// prefix has an earlier CSV defect. Decode failure precedes syntax admission.
fn coordinates(prefix: &[u8]) -> (usize, usize) {
    let (mut record, mut field, mut index) = (1, 1, 0);
    while index < prefix.len() {
        match prefix[index] {
            b'"' => {
                index += 1;
                while index < prefix.len() {
                    if prefix[index] == b'"' {
                        if prefix.get(index + 1) == Some(&b'"') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            b',' => field += 1,
            b'\n' => {
                record += 1;
                field = 1;
            }
            _ => {}
        }
        index += 1;
    }
    (record, field)
}

/// The UTF-8 byte-order mark, `EF BB BF` (CSV-10).
const BYTE_ORDER_MARK: char = '\u{feff}';

/// Read a source whose input declares no byte-order mark: a leading mark is
/// refused (CSV-10).
pub fn parse(content: &[u8]) -> Result<CsvSource, Error> {
    parse_declared(content, false)
}

/// The text of a source, decoded completely before any syntax is admitted:
/// within the source byte limit, valid UTF-8, beginning with a byte-order
/// mark exactly when `byte_order_mark` declares one, which is skipped, and
/// holding a header, up to the end of its last record.
///
/// `byte_order_mark` is the input's declaration (CSV-10): when it is true the
/// source must begin with one mark, which is skipped, and the first record
/// starts after it. Any other mark at the start, including a second one after
/// the declared mark, is refused: the declaration decides, never the file.
fn admitted_text(content: &[u8], byte_order_mark: bool) -> Result<&str, Error> {
    yamaa_core::limits::Limits::source_bytes(content.len()).map_err(limit_failure)?;
    let text = std::str::from_utf8(content).map_err(|error| {
        let (record, field) = coordinates(&content[..error.valid_up_to()]);
        failure(ProfileCause::InvalidText, record, field)
    })?;
    let mark = |declared| failure(ProfileCause::ByteOrderMark { declared }, 1, 1);
    let text = if byte_order_mark {
        text.strip_prefix(BYTE_ORDER_MARK)
            .ok_or_else(|| mark(true))?
    } else {
        text
    };
    if text.starts_with(BYTE_ORDER_MARK) {
        return Err(mark(byte_order_mark));
    }
    if text.is_empty() {
        return Err(failure(ProfileCause::HeaderAbsent, 1, 1));
    }
    Ok(without_trailing_blank_lines(text))
}

/// `text` up to the end of its last record (CSV-11): the line terminators
/// that end the source are the last record's terminator, and the blank
/// lines after it, which are no record, whatever the header's width. The
/// header is always read, and a blank line before the last record is an
/// empty record. Every read lexes this text, and a read by partition the
/// spans its scan found in it, so no path reads the blank lines.
///
/// A run of terminators inside a quoted field the source never closes keeps
/// its first terminator, so the field still fails as it would whole.
fn without_trailing_blank_lines(text: &str) -> &str {
    let mut end = text.len();
    let mut last = end;
    while let Some(rest) = text[..end].strip_suffix('\n') {
        last = end;
        end = rest.strip_suffix('\r').unwrap_or(rest).len();
    }
    &text[..last]
}

/// Decode completely, then scan all syntax before header and row-width findings.
/// File and table boundaries precede retaining the corresponding bytes or shape.
///
/// `byte_order_mark` is the input's declaration (CSV-10), as
/// [`admitted_text`] reads it.
pub fn parse_declared(content: &[u8], byte_order_mark: bool) -> Result<CsvSource, Error> {
    let text = admitted_text(content, byte_order_mark)?;
    let mut records = Records::new(usize::MAX, true);
    lex(text, 1, &mut records)?;
    let names = records.names()?;
    yamaa_core::limits::Limits::table(records.kept.len(), names.len()).map_err(limit_failure)?;
    Ok(CsvSource {
        names,
        records: records.kept,
    })
}

/// A source read by partition (#3792): its header, and where each partition
/// of its records begins, from one scan of every record. The scan reports
/// every finding the whole read reports, in the same order and at the same
/// record and field, except that each partition, not the whole table, is
/// admitted within the row and cell limits. A partition's records then
/// decode on demand.
#[derive(Debug)]
pub struct Partitions<'t> {
    text: &'t str,
    names: Vec<String>,
    /// The records of every partition but the last, which holds the rest.
    rows: usize,
    /// Where each partition's first record begins in `text`, then where the
    /// last one ends: an empty source has one empty partition.
    starts: Vec<usize>,
    /// The records the scan read.
    count: usize,
}

impl Partitions<'_> {
    /// The header's field names.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The number of partitions, at least one.
    pub fn len(&self) -> usize {
        self.starts.len() - 1
    }

    /// Whether there are no partitions, which never holds.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The position, among the source's records, of partition `index`'s
    /// first record: the records of the partitions before it.
    pub fn first_record(&self, index: usize) -> usize {
        index.saturating_mul(self.rows)
    }

    /// Decode partition `index`'s records as the whole read decodes them.
    /// The scan admitted every record, so this finds nothing; a finding
    /// would be the scan's.
    pub fn records(&self, index: usize) -> Result<CsvSource, Error> {
        let first = self.first_record(index);
        let last = if index + 1 < self.len() {
            first + self.rows
        } else {
            self.count
        };
        self.offsets().range(self.text, 0, first..last)
    }

    /// What decodes any range of the scanned records again from the held
    /// bytes, without the scan (#3830).
    pub fn offsets(&self) -> Offsets {
        Offsets {
            names: self.names.clone(),
            rows: self.rows,
            starts: self.starts.clone(),
            count: self.count,
        }
    }
}

/// Where each partition of a scanned source's records begins in its text
/// (#3830): what decodes any range of its records again, the scan having
/// admitted every record.
#[derive(Clone, Debug)]
pub struct Offsets {
    names: Vec<String>,
    rows: usize,
    starts: Vec<usize>,
    count: usize,
}

impl Offsets {
    /// The records the scan read.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The bytes of the source's text, after any byte-order mark, that hold
    /// the partitions of records `range`.
    fn span(&self, range: &std::ops::Range<usize>) -> std::ops::Range<usize> {
        let first = range.start / self.rows;
        let last = range
            .end
            .div_ceil(self.rows)
            .clamp(first, self.starts.len() - 1);
        self.starts[first.min(self.starts.len() - 1)]..self.starts[last]
    }

    /// Decode records `range` as the whole read decodes them, lexed from the
    /// start of the partition holding the range's first record. `text` is
    /// the source's text from byte `base` on, through the partitions of the
    /// range.
    fn range(
        &self,
        text: &str,
        base: usize,
        range: std::ops::Range<usize>,
    ) -> Result<CsvSource, Error> {
        let mut kept = Kept {
            skip: 0,
            take: range.len(),
            records: Vec::new(),
        };
        if !range.is_empty() {
            let first = range.start / self.rows;
            kept.skip = range.start - first * self.rows;
            let span = self.span(&range);
            // The header is record 1, so data record `n` is record `n + 2`.
            lex(
                &text[span.start - base..span.end - base],
                first * self.rows + 2,
                &mut kept,
            )?;
        }
        Ok(CsvSource {
            names: self.names.clone(),
            records: kept.records,
        })
    }

    /// Decode records `range` of `content`, the bytes the scan read, whose
    /// input declares `byte_order_mark`. Only the bytes of the range's own
    /// partitions are decoded again; the scan admitted the rest.
    pub fn records(
        &self,
        content: &[u8],
        byte_order_mark: bool,
        range: std::ops::Range<usize>,
    ) -> Result<CsvSource, Error> {
        let mark = if byte_order_mark {
            BYTE_ORDER_MARK.len_utf8()
        } else {
            0
        };
        let span = self.span(&range);
        let bytes = &content[mark + span.start..mark + span.end];
        let text = std::str::from_utf8(bytes).map_err(|error| {
            let (record, field) = coordinates(&bytes[..error.valid_up_to()]);
            failure(ProfileCause::InvalidText, record, field)
        })?;
        self.range(text, span.start, range)
    }
}

/// Scan a source as [`parse_declared`] reads it, keeping its records'
/// partitions of `rows` records each, the last holding the rest, and admit
/// each partition, not the whole table, within the row and cell limits.
pub fn scan(
    content: &[u8],
    byte_order_mark: bool,
    rows: NonZeroUsize,
) -> Result<Partitions<'_>, Error> {
    let text = admitted_text(content, byte_order_mark)?;
    let mut records = Records::new(rows.get(), false);
    lex(text, 1, &mut records)?;
    let names = records.names()?;
    let mut starts = records.starts;
    if records.count % rows != 0 || records.count == 0 {
        starts.push(text.len());
    }
    Ok(Partitions {
        text,
        names,
        rows: rows.get(),
        starts,
        count: records.count,
    })
}

/// Where a source's first record, its header, ends: after the first line
/// feed outside a quoted field, or at the end of the source. The header
/// alone reads as the source would, holding no records.
pub fn header_end(content: &[u8]) -> usize {
    let mut quoted = false;
    for (index, &byte) in content.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'\n' if !quoted => return index + 1,
            _ => {}
        }
    }
    content.len()
}

/// One field as the lexer read it: the text of a bare field, or the text
/// between the quotes of a quoted field, each doubled quote still doubled.
#[derive(Clone, Copy)]
enum Lexed<'t> {
    Bare(&'t str),
    Quoted(&'t str),
}

impl Lexed<'_> {
    /// The field's collected text, missing when it is empty, quoted or not.
    fn value(self) -> Option<String> {
        match self {
            Self::Bare("") | Self::Quoted("") => None,
            Self::Bare(text) => Some(text.to_owned()),
            Self::Quoted(text) => Some(text.replace("\"\"", "\"")),
        }
    }
}

/// What one pass of the lexer does with each record it reads.
trait Sink<'t> {
    /// Record `number`, whose fields are `fields`, ending at byte `end` of
    /// the text: after its line ending, or at the end of the text.
    fn record(&mut self, number: usize, fields: &[Lexed<'t>], end: usize) -> Result<(), Error>;
}

/// Read `text`, which begins at a record, record by record, numbering the
/// first record `first`: a field's count is admitted before it is read, and
/// a finding stops the pass at its record and field.
fn lex<'t>(text: &'t str, first: usize, sink: &mut impl Sink<'t>) -> Result<(), Error> {
    let bytes = text.as_bytes();
    let mut fields = Vec::new();
    let mut index = 0;
    let mut number = first;
    loop {
        let column = fields.len() + 1;
        yamaa_core::limits::Limits::columns(column).map_err(limit_failure)?;
        let field = if bytes.get(index) == Some(&b'"') {
            index += 1;
            let start = index;
            loop {
                match bytes.get(index) {
                    None => return Err(failure(ProfileCause::QuoteUnterminated, number, column)),
                    Some(b'"') => {
                        if bytes.get(index + 1) == Some(&b'"') {
                            index += 2;
                            continue;
                        }
                        break;
                    }
                    Some(b'\r') => {
                        return Err(failure(ProfileCause::CarriageReturn, number, column))
                    }
                    _ => index += 1,
                }
            }
            let quoted = Lexed::Quoted(&text[start..index]);
            index += 1;
            if bytes
                .get(index)
                .is_some_and(|b| !matches!(b, b',' | b'\r' | b'\n'))
            {
                return Err(failure(ProfileCause::TextAfterQuote, number, column));
            }
            quoted
        } else {
            let start = index;
            while let Some(byte) = bytes.get(index) {
                match byte {
                    b',' | b'\n' | b'\r' => break,
                    b'"' => return Err(failure(ProfileCause::QuoteInBareField, number, column)),
                    _ => index += 1,
                }
            }
            Lexed::Bare(&text[start..index])
        };
        fields.push(field);
        match bytes.get(index) {
            None => {
                sink.record(number, &fields, index)?;
                break;
            }
            Some(b',') => {
                index += 1;
                continue;
            }
            Some(b'\r') => {
                if bytes.get(index + 1) != Some(&b'\n') {
                    return Err(failure(ProfileCause::CarriageReturn, number, column));
                }
                index += 2;
            }
            Some(b'\n') => index += 1,
            _ => unreachable!("scanned field stops at a delimiter"),
        }
        sink.record(number, &fields, index)?;
        fields.clear();
        number += 1;
        if index >= bytes.len() {
            break;
        }
    }
    Ok(())
}

/// The previous physical record count includes the header; adding a data record
/// gives exactly that many data rows. The header itself has zero data rows.
fn admit_record(previous_records: usize, columns: usize) -> Result<(), Error> {
    yamaa_core::limits::Limits::table(previous_records, columns).map_err(limit_failure)
}

/// The records of a scan from the header on: the header's fields, the first
/// record whose width is not the header's, each data record admitted with
/// the records before it in its partition, and either every record (the
/// whole read) or where each partition begins (a read by partition).
struct Records {
    /// The data records of each partition; `usize::MAX` admits the whole
    /// source as one table.
    rows: usize,
    /// Whether every data record is kept, as the whole read keeps them.
    keep: bool,
    header: Vec<Option<String>>,
    kept: Vec<Vec<Option<String>>>,
    /// The data records read.
    count: usize,
    /// Where each partition's first record begins: after the header, then
    /// after every `rows` data records.
    starts: Vec<usize>,
    width_failure: Option<Error>,
}

impl Records {
    fn new(rows: usize, keep: bool) -> Self {
        Self {
            rows,
            keep,
            header: Vec::new(),
            kept: Vec::new(),
            count: 0,
            starts: Vec::new(),
            width_failure: None,
        }
    }

    /// The header's names, refusing an empty or a repeated one, then the
    /// first width finding: complete syntax precedes both.
    fn names(&mut self) -> Result<Vec<String>, Error> {
        let mut names = Vec::with_capacity(self.header.len());
        let mut seen = BTreeSet::new();
        for (index, name) in std::mem::take(&mut self.header).into_iter().enumerate() {
            let Some(name) = name else {
                return Err(failure(ProfileCause::FieldNameEmpty, 1, index + 1));
            };
            if !seen.insert(name.clone()) {
                return Err(Error::Profile {
                    cause: ProfileCause::FieldNameDuplicate,
                    record: 1,
                    field: Field::Name(name),
                });
            }
            names.push(name);
        }
        match self.width_failure.take() {
            Some(error) => Err(error),
            None => Ok(names),
        }
    }
}

impl<'t> Sink<'t> for Records {
    fn record(&mut self, number: usize, fields: &[Lexed<'t>], end: usize) -> Result<(), Error> {
        if number == 1 {
            admit_record(0, fields.len())?;
            self.header = fields.iter().map(|field| field.value()).collect();
            self.starts.push(end);
            return Ok(());
        }
        let width = self.header.len();
        admit_record(self.count % self.rows + 1, width)?;
        // A malformed record cannot form a table. Keep its first finding and
        // finish lexical scanning, while retaining only the valid prefix.
        if fields.len() != width && self.width_failure.is_none() {
            self.width_failure = Some(failure(
                ProfileCause::RecordWidth,
                number,
                fields.len().min(width) + 1,
            ));
        }
        if self.keep && self.width_failure.is_none() {
            self.kept
                .push(fields.iter().map(|field| field.value()).collect());
        }
        self.count += 1;
        if self.count % self.rows == 0 {
            self.starts.push(end);
        }
        Ok(())
    }
}

/// The records of a range of a scanned source, as the whole read keeps
/// them: the first `skip` records lexed are passed over, and `take` kept.
struct Kept {
    skip: usize,
    take: usize,
    records: Vec<Vec<Option<String>>>,
}

impl<'t> Sink<'t> for Kept {
    fn record(&mut self, _: usize, fields: &[Lexed<'t>], _: usize) -> Result<(), Error> {
        if self.skip > 0 {
            self.skip -= 1;
        } else if self.records.len() < self.take {
            self.records
                .push(fields.iter().map(|field| field.value()).collect());
        }
        Ok(())
    }
}

/// Profile failures and table failures remain distinct to the compiler.
#[derive(Debug)]
pub enum TextTableError {
    Csv(Error),
    Table(crate::column_table::TableError),
}

/// Decode the undeclared-type CSV case into an owned text table, each column
/// fitting Arrow Utf8 offsets as a whole. Declared types, producer schemas
/// and ordinals require their separate admission; callers must not silently
/// route those declarations through this text-only helper.
pub fn parse_text_table(
    content: &[u8],
) -> Result<crate::column_table::ColumnTable, TextTableError> {
    use crate::column_table::{fits_utf8_offsets, Column, ColumnTable, TableError, UTF8_OFFSETS};
    use yamaa_core::{
        table::{Column as Declared, TableSchema},
        value::ColumnType,
    };

    let parsed = parse(content).map_err(TextTableError::Csv)?;
    let width = parsed.names.len();
    let schema = TableSchema::new(
        parsed
            .names
            .into_iter()
            .map(|name| Declared {
                name,
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .expect("CSV admission established nonempty unique header names");
    let columns = (0..width)
        .map(|column| {
            let values = || parsed.records.iter().map(|row| row[column].as_deref());
            if !fits_utf8_offsets(values().flatten().map(str::len)) {
                return Err(TextTableError::Table(TableError::Representation {
                    representation: UTF8_OFFSETS,
                }));
            }
            Ok(Column::Str(values().collect()))
        })
        .collect::<Result<_, _>>()?;
    ColumnTable::new(schema, columns, parsed.records.len()).map_err(TextTableError::Table)
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use yamaa_core::limits::Limits;

    #[test]
    fn record_geometry_excludes_the_header_without_allocating_rows_or_cells() {
        assert_eq!(admit_record(0, Limits::COLUMNS), Ok(()));
        assert_eq!(admit_record(Limits::TABLE_ROWS, 64), Ok(()));
        assert_eq!(
            admit_record(Limits::TABLE_ROWS + 1, 1),
            Err(Error::Limit {
                resource: "table_rows",
                limit: Limits::TABLE_ROWS,
                actual: Some(Limits::TABLE_ROWS + 1),
            })
        );
        assert_eq!(admit_record(3_575_418, 179), Ok(()));
        assert_eq!(
            admit_record(3_575_419, 179),
            Err(Error::Limit {
                resource: "table_cells",
                limit: Limits::TABLE_CELLS,
                actual: Some(Limits::TABLE_CELLS + 1),
            })
        );
    }

    /// A read by partition admits each data record with the records before
    /// it in its partition: the first record of every partition is admitted
    /// as one row, and the last of a full partition as `rows`, whatever its
    /// position in the source. Records are admitted without being lexed, so
    /// the geometry is the scan's own.
    #[test]
    fn a_partition_scan_admits_each_record_with_its_partition_only() {
        // 179 fields, the widest a 3,575,418-record table admits. With
        // partitions of 3,575,418 records, the record after a full partition
        // starts the next one: the whole read refuses it, the scan admits it.
        let fields = vec![Lexed::Bare("x"); 179];
        let full = 3_575_418;
        let mut records = Records::new(full, false);
        records.record(1, &fields, 0).unwrap();
        records.count = full - 1;
        assert_eq!(records.record(full + 1, &fields, 1), Ok(()));
        assert_eq!(records.starts, [0, 1]);
        assert_eq!(records.record(full + 2, &fields, 2), Ok(()));
        let mut whole = Records::new(usize::MAX, false);
        whole.record(1, &fields, 0).unwrap();
        whole.count = full;
        assert_eq!(
            whole.record(full + 2, &fields, 1),
            Err(Error::Limit {
                resource: "table_cells",
                limit: Limits::TABLE_CELLS,
                actual: Some(Limits::TABLE_CELLS + 1),
            })
        );
    }
}

#[cfg(test)]
mod field_boundary_tests {
    use super::*;

    #[test]
    fn field_admission_covers_empty_leading_and_post_line_ending_fields() {
        for prefix in ["", "A\n", "A\r\n", "\n", "\r\n"] {
            let source = format!(
                "{prefix}{}",
                ",".repeat(yamaa_core::limits::Limits::COLUMNS)
            );
            assert_eq!(
                lex(&source, 1, &mut Records::new(usize::MAX, true)).err(),
                Some(Error::Limit {
                    resource: "columns",
                    limit: yamaa_core::limits::Limits::COLUMNS,
                    actual: Some(yamaa_core::limits::Limits::COLUMNS + 1),
                }),
                "{prefix:?}"
            );
        }
    }

    #[test]
    fn exact_column_boundary_is_accepted_and_next_field_stops_before_decoding() {
        let header = (0..yamaa_core::limits::Limits::COLUMNS)
            .map(|index| format!("C{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let admitted = parse(header.as_bytes()).unwrap();
        assert_eq!(admitted.names.len(), yamaa_core::limits::Limits::COLUMNS);
        assert!(admitted.records.is_empty());
        // The next field is not scanned or allocated, even if its quoted
        // spelling would otherwise fail only at the end of a long payload.
        let oversized = format!("{header},\"");
        assert_eq!(
            parse(oversized.as_bytes()),
            Err(Error::Limit {
                resource: "columns",
                limit: yamaa_core::limits::Limits::COLUMNS,
                actual: Some(yamaa_core::limits::Limits::COLUMNS + 1),
            })
        );
    }
}
