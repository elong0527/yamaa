//! The closed CSV input profile's parser over one captured byte snapshot,
//! and the undeclared-type text table it builds. Pure byte admission: the
//! profile's models (the text table and its refusals) are core's, and
//! filesystem authority stays with the resolver port (#3030 section 2.10).
use std::collections::BTreeSet;
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

/// Decode completely, then scan all syntax before header and row-width findings.
/// File and table boundaries precede retaining the corresponding bytes or shape.
///
/// `byte_order_mark` is the input's declaration (CSV-10): when it is true the
/// source must begin with one mark, which is skipped, and the first record
/// starts after it. Any other mark at the start, including a second one after
/// the declared mark, is refused: the declaration decides, never the file.
pub fn parse_declared(content: &[u8], byte_order_mark: bool) -> Result<CsvSource, Error> {
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
    let Scanned {
        records,
        width_failure,
    } = scan(text)?;
    let mut records = records.into_iter();
    let header = records.next().expect("nonempty text produces a record");
    let mut names = Vec::with_capacity(header.len());
    let mut seen = BTreeSet::new();
    for (index, name) in header.into_iter().enumerate() {
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
    if let Some(error) = width_failure {
        return Err(error);
    }
    let mut rows = Vec::new();
    for (index, record) in records.enumerate() {
        if record.len() != names.len() {
            return Err(failure(
                ProfileCause::RecordWidth,
                index + 2,
                record.len().min(names.len()) + 1,
            ));
        }
        rows.push(record);
    }
    yamaa_core::limits::Limits::table(rows.len(), names.len()).map_err(limit_failure)?;
    Ok(CsvSource {
        names,
        records: rows,
    })
}

struct Scanned {
    records: Vec<Vec<Option<String>>>,
    width_failure: Option<Error>,
}
/// The previous physical record count includes the header; adding a data record
/// gives exactly that many data rows. The header itself has zero data rows.
fn admit_record(previous_records: usize, columns: usize) -> Result<(), Error> {
    yamaa_core::limits::Limits::table(previous_records, columns).map_err(limit_failure)
}
impl Scanned {
    fn push(&mut self, record: Vec<Option<String>>, number: usize) -> Result<(), Error> {
        let width = self.records.first().map_or(record.len(), Vec::len);
        admit_record(number - 1, width)?;
        if number > 1 && record.len() != width && self.width_failure.is_none() {
            self.width_failure = Some(failure(
                ProfileCause::RecordWidth,
                number,
                record.len().min(width) + 1,
            ));
        }
        // A malformed record cannot form a table. Keep its first finding and
        // finish lexical scanning, while retaining only the valid prefix.
        if self.width_failure.is_none() {
            self.records.push(record);
        }
        Ok(())
    }
}

fn scan(text: &str) -> Result<Scanned, Error> {
    let bytes = text.as_bytes();
    let mut records = Scanned {
        records: Vec::new(),
        width_failure: None,
    };
    let mut record = Vec::new();
    let mut index = 0;
    let mut number = 1;
    loop {
        let column = record.len() + 1;
        yamaa_core::limits::Limits::columns(column).map_err(limit_failure)?;
        let field = if bytes.get(index) == Some(&b'"') {
            index += 1;
            let mut start = index;
            let mut value = String::new();
            loop {
                match bytes.get(index) {
                    None => return Err(failure(ProfileCause::QuoteUnterminated, number, column)),
                    Some(b'"') => {
                        value.push_str(&text[start..index]);
                        if bytes.get(index + 1) == Some(&b'"') {
                            value.push('"');
                            index += 2;
                            start = index;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    Some(b'\r') => {
                        return Err(failure(ProfileCause::CarriageReturn, number, column))
                    }
                    _ => index += 1,
                }
            }
            if bytes
                .get(index)
                .is_some_and(|b| !matches!(b, b',' | b'\r' | b'\n'))
            {
                return Err(failure(ProfileCause::TextAfterQuote, number, column));
            }
            value
        } else {
            let start = index;
            while let Some(byte) = bytes.get(index) {
                match byte {
                    b',' | b'\n' | b'\r' => break,
                    b'"' => return Err(failure(ProfileCause::QuoteInBareField, number, column)),
                    _ => index += 1,
                }
            }

            text[start..index].to_owned()
        };
        record.push(if field.is_empty() { None } else { Some(field) });
        match bytes.get(index) {
            None => {
                records.push(record, number)?;
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
        records.push(record, number)?;
        record = Vec::new();
        number += 1;
        if index >= bytes.len() {
            break;
        }
    }
    Ok(records)
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
                scan(&source).err(),
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
