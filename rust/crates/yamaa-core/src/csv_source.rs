//! The closed R023 CSV input profile over one captured byte snapshot.
//! Pure byte admission; filesystem authority and physical tables are outer boundaries.
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Index(usize),
    Name(String),
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("csv source profile: cause={cause:?}, record={record:?}, field={field:?}")]
    Profile {
        cause: ProfileCause,
        record: usize,
        field: Field,
    },
    #[error("csv source limit: resource={resource:?}, limit={limit:?}, actual={actual:?}")]
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
}
/// Closed semantic causes detected by the profile scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileCause {
    InvalidText,
    ByteOrderMark,
    HeaderAbsent,
    FieldNameEmpty,
    FieldNameDuplicate,
    RecordWidth,
    QuoteUnterminated,
    CarriageReturn,
    TextAfterQuote,
    QuoteInBareField,
}
impl ProfileCause {
    pub fn code(self) -> crate::diagnostic::ConditionCode {
        use crate::diagnostic::ConditionCode as C;
        match self {
            Self::InvalidText => C::CsvInvalidText,
            Self::ByteOrderMark => C::CsvByteOrderMark,
            Self::HeaderAbsent => C::CsvHeaderAbsent,
            Self::FieldNameEmpty => C::CsvFieldNameEmpty,
            Self::FieldNameDuplicate => C::CsvFieldNameDuplicate,
            Self::RecordWidth => C::CsvRecordWidth,
            Self::QuoteUnterminated => C::CsvQuoteUnterminated,
            Self::CarriageReturn => C::CsvCarriageReturn,
            Self::TextAfterQuote => C::CsvTextAfterQuote,
            Self::QuoteInBareField => C::CsvQuoteInBareField,
        }
    }
}
impl Error {
    pub fn requirement(&self) -> Option<&'static str> {
        match self {
            Self::Profile { cause, .. } => cause.code().definition().requirement,
            Self::Limit { .. } => None,
        }
    }
    /// Borrow classified profile facts without allocating diagnostic context.
    pub fn finding<'a>(
        &'a self,
        dataset: &'a str,
        path: &'a str,
    ) -> Option<crate::diagnostic::SourceFinding<'a>> {
        let Self::Profile {
            cause,
            record,
            field,
        } = self
        else {
            return None;
        };
        Some(crate::diagnostic::SourceFinding::Csv {
            cause: *cause,
            record: *record,
            field,
            dataset,
            path,
        })
    }
}
fn failure(cause: ProfileCause, record: usize, field: usize) -> Error {
    Error::Profile {
        cause,
        record,
        field: Field::Index(field),
    }
}
fn limit_failure(error: crate::limits::LimitExceeded) -> Error {
    Error::Limit {
        resource: error.resource,
        limit: error.limit,
        actual: error.actual,
    }
}

/// Every value is collected text or missing; no numeric, NA-token or locale inference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvSource {
    pub names: Vec<String>,
    pub records: Vec<Vec<Option<String>>>,
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

/// Decode completely, then scan all syntax before header and row-width findings.
/// File and table boundaries precede retaining the corresponding bytes or shape.
pub fn parse(content: &[u8]) -> Result<CsvSource, Error> {
    crate::limits::Limits::source_bytes(content.len()).map_err(limit_failure)?;
    let text = std::str::from_utf8(content).map_err(|error| {
        let (record, field) = coordinates(&content[..error.valid_up_to()]);
        failure(ProfileCause::InvalidText, record, field)
    })?;
    if text.starts_with('\u{feff}') {
        return Err(failure(ProfileCause::ByteOrderMark, 1, 1));
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
    crate::limits::Limits::table(rows.len(), names.len()).map_err(limit_failure)?;
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
    crate::limits::Limits::table(previous_records, columns).map_err(limit_failure)
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
        crate::limits::Limits::columns(column).map_err(limit_failure)?;
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

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use crate::limits::Limits;

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
            let source = format!("{prefix}{}", ",".repeat(crate::limits::Limits::COLUMNS));
            assert_eq!(
                scan(&source).err(),
                Some(Error::Limit {
                    resource: "columns",
                    limit: crate::limits::Limits::COLUMNS,
                    actual: Some(crate::limits::Limits::COLUMNS + 1),
                }),
                "{prefix:?}"
            );
        }
    }

    #[test]
    fn exact_column_boundary_is_accepted_and_next_field_stops_before_decoding() {
        let header = (0..crate::limits::Limits::COLUMNS)
            .map(|index| format!("C{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let admitted = parse(header.as_bytes()).unwrap();
        assert_eq!(admitted.names.len(), crate::limits::Limits::COLUMNS);
        assert!(admitted.records.is_empty());
        // The next field is not scanned or allocated, even if its quoted
        // spelling would otherwise fail only at the end of a long payload.
        let oversized = format!("{header},\"");
        assert_eq!(
            parse(oversized.as_bytes()),
            Err(Error::Limit {
                resource: "columns",
                limit: crate::limits::Limits::COLUMNS,
                actual: Some(crate::limits::Limits::COLUMNS + 1),
            })
        );
    }
}
