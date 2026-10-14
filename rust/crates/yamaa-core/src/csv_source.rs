//! The closed R023 CSV input profile over one captured byte snapshot.
//! Pure byte admission; filesystem authority and physical tables are outer boundaries.
use alloc::{borrow::ToOwned, collections::BTreeSet, string::String, vec::Vec};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub fields: usize,
    /// Includes the header; a zero-row source still has one record.
    pub records: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 1024 * 1024 * 1024,
            // The profile counts the header as fields and one record.
            fields: 640_000_000 + crate::specification::DEFAULT_MAX_COLUMNS,
            records: 10_000_000 + 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Index(usize),
    Name(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Profile {
        cause: ProfileCause,
        record: usize,
        field: Field,
    },
    Limit {
        resource: &'static str,
        limit: usize,
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
    /// Unbounded owned compatibility; use complete borrowed report admission for new callers.
    /// This existing Option signature is retained until its callers migrate.
    pub fn diagnostic(&self, dataset: &str, path: &str) -> Option<crate::diagnostic::Diagnostic> {
        self.finding(dataset, path)
            .map(crate::diagnostic::SourceFinding::owned_compatibility)
    }
}
fn failure(cause: ProfileCause, record: usize, field: usize) -> Error {
    Error::Profile {
        cause,
        record,
        field: Field::Index(field),
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

/// Decode completely, then scan all syntax, then validate header and row widths.
/// Limits are request policy failures, never repaired records or language findings.
pub fn parse(content: &[u8], limits: Limits) -> Result<CsvSource, Error> {
    parse_with_admission(content, limits, |_| Ok(()))
}

/// Admit modeled retained ownership before each scanner allocation. The callback
/// charges field slots (32 bytes), record owners (24 bytes), and copied text.
/// It may refuse a request without changing CSV semantic admission order.
pub fn parse_with_admission(
    content: &[u8],
    limits: Limits,
    mut admit: impl FnMut(usize) -> Result<(), Error>,
) -> Result<CsvSource, Error> {
    if content.len() > limits.bytes {
        return Err(Error::Limit {
            resource: "bytes",
            limit: limits.bytes,
        });
    }
    let text = core::str::from_utf8(content).map_err(|error| {
        let (record, field) = coordinates(&content[..error.valid_up_to()]);
        failure(ProfileCause::InvalidText, record, field)
    })?;
    if text.starts_with('\u{feff}') {
        return Err(failure(ProfileCause::ByteOrderMark, 1, 1));
    }
    if text.is_empty() {
        return Err(failure(ProfileCause::HeaderAbsent, 1, 1));
    }
    let records = scan(text, limits, &mut admit)?;
    let mut records = records.into_iter();
    let header = records.next().expect("nonempty text produces a record");
    admit(header.len().checked_mul(32).ok_or(Error::Limit {
        resource: "storage_bytes",
        limit: usize::MAX,
    })?)?;
    let mut names = Vec::with_capacity(header.len());
    let mut seen = BTreeSet::new();
    for (index, name) in header.into_iter().enumerate() {
        let Some(name) = name else {
            return Err(failure(ProfileCause::FieldNameEmpty, 1, index + 1));
        };
        admit(32)?;
        admit(name.len())?;
        if !seen.insert(name.clone()) {
            return Err(Error::Profile {
                cause: ProfileCause::FieldNameDuplicate,
                record: 1,
                field: Field::Name(name),
            });
        }
        names.push(name);
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
    Ok(CsvSource {
        names,
        records: rows,
    })
}

fn scan(
    text: &str,
    limits: Limits,
    admit: &mut impl FnMut(usize) -> Result<(), Error>,
) -> Result<Vec<Vec<Option<String>>>, Error> {
    let bytes = text.as_bytes();
    let mut records = Vec::new();
    let mut record = Vec::new();
    let (mut index, mut fields) = (0, 0);
    loop {
        if fields >= limits.fields {
            return Err(Error::Limit {
                resource: "fields",
                limit: limits.fields,
            });
        }
        if records.len() >= limits.records {
            return Err(Error::Limit {
                resource: "records",
                limit: limits.records,
            });
        }
        if record.is_empty() {
            admit(24)?;
        }
        admit(32)?;
        fields += 1;
        let number = records.len() + 1;
        let column = record.len() + 1;
        let field = if bytes.get(index) == Some(&b'"') {
            index += 1;
            let mut start = index;
            let mut value = String::new();
            loop {
                match bytes.get(index) {
                    None => return Err(failure(ProfileCause::QuoteUnterminated, number, column)),
                    Some(b'"') => {
                        admit(index - start)?;
                        value.push_str(&text[start..index]);
                        if bytes.get(index + 1) == Some(&b'"') {
                            admit(1)?;
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
            admit(index - start)?;
            text[start..index].to_owned()
        };
        record.push(if field.is_empty() { None } else { Some(field) });
        match bytes.get(index) {
            None => {
                records.push(record);
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
        records.push(record);
        record = Vec::new();
        if index >= bytes.len() {
            break;
        }
    }
    Ok(records)
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    #[test]
    fn owned_scanner_charge_stops_before_the_exact_next_byte() {
        for limit in [178, 179] {
            let mut used = 0usize;
            let result = parse_with_admission(b"A\nx\n", Limits::default(), |amount| {
                let required = used + amount;
                if required > limit {
                    return Err(Error::Limit {
                        resource: "storage_bytes",
                        limit,
                    });
                }
                used = required;
                Ok(())
            });
            assert_eq!(used, limit);
            if limit == 179 {
                assert_eq!(
                    result.unwrap(),
                    parse(b"A\nx\n", Limits::default()).unwrap()
                );
            } else {
                assert_eq!(
                    result,
                    Err(Error::Limit {
                        resource: "storage_bytes",
                        limit
                    })
                );
            }
        }
    }

    #[test]
    fn semantic_failure_keeps_the_actual_admitted_prefix() {
        let mut used = 0;
        let result = parse_with_admission(b"A\n\"x", Limits::default(), |amount| {
            used += amount;
            Ok(())
        });
        assert_eq!(used, 113);
        assert!(matches!(
            result,
            Err(Error::Profile {
                cause: ProfileCause::QuoteUnterminated,
                record: 2,
                ..
            })
        ));
    }
}
