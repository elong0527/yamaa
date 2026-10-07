//! The closed R023 CSV input profile over one captured byte snapshot.
//! Filesystem authority and declared type conversion are separate boundaries.
use std::collections::BTreeSet;

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
            bytes: 8_388_608,
            fields: 1_048_576,
            records: 1_048_576,
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
        condition: &'static str,
        record: usize,
        field: Field,
    },
    Limit {
        resource: &'static str,
        limit: usize,
    },
}
impl Error {
    pub fn requirement(&self) -> Option<&'static str> {
        match self {
            Self::Profile {
                condition: "invalid_text",
                ..
            } => Some("REQ-0853"),
            Self::Profile { .. } => Some("REQ-0851"),
            Self::Limit { .. } => None,
        }
    }
}
fn failure(condition: &'static str, record: usize, field: usize) -> Error {
    Error::Profile {
        condition,
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
    if content.len() > limits.bytes {
        return Err(Error::Limit {
            resource: "bytes",
            limit: limits.bytes,
        });
    }
    let text = std::str::from_utf8(content).map_err(|error| {
        let (record, field) = coordinates(&content[..error.valid_up_to()]);
        failure("invalid_text", record, field)
    })?;
    if text.starts_with('\u{feff}') {
        return Err(failure("source_byte_order_mark", 1, 1));
    }
    if text.is_empty() {
        return Err(failure("source_header_absent", 1, 1));
    }
    let records = scan(text, limits)?;
    let mut records = records.into_iter();
    let header = records.next().expect("nonempty text produces a record");
    let mut names = Vec::with_capacity(header.len());
    let mut seen = BTreeSet::new();
    for (index, name) in header.into_iter().enumerate() {
        let Some(name) = name else {
            return Err(failure("source_field_name_empty", 1, index + 1));
        };
        if !seen.insert(name.clone()) {
            return Err(Error::Profile {
                condition: "source_field_name_duplicate",
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
                "source_record_width",
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

fn scan(text: &str, limits: Limits) -> Result<Vec<Vec<Option<String>>>, Error> {
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
        fields += 1;
        let number = records.len() + 1;
        let column = record.len() + 1;
        let field = if bytes.get(index) == Some(&b'"') {
            index += 1;
            let mut start = index;
            let mut value = String::new();
            loop {
                match bytes.get(index) {
                    None => return Err(failure("source_quote_unterminated", number, column)),
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
                    Some(b'\r') => return Err(failure("source_carriage_return", number, column)),
                    _ => index += 1,
                }
            }
            if bytes
                .get(index)
                .is_some_and(|b| !matches!(b, b',' | b'\r' | b'\n'))
            {
                return Err(failure("source_text_after_quote", number, column));
            }
            value
        } else {
            let start = index;
            while let Some(byte) = bytes.get(index) {
                match byte {
                    b',' | b'\n' | b'\r' => break,
                    b'"' => return Err(failure("source_quote_in_bare_field", number, column)),
                    _ => index += 1,
                }
            }
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
                    return Err(failure("source_carriage_return", number, column));
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

/// Profile failures and Arrow/resource failures remain distinct to the compiler.
#[derive(Debug)]
pub enum TextTableError {
    Csv(Error),
    Table(crate::arrow_table::TableError),
}

/// Decode the undeclared-type CSV case into an owned canonical Arrow snapshot.
/// Declared types, producer schemas and ordinals require their separate admission;
/// callers must not silently route those declarations through this text-only helper.
pub fn parse_text_table(
    content: &[u8],
    csv_limits: Limits,
    table_limits: crate::arrow_table::TableLimits,
) -> Result<crate::arrow_table::ArrowTable, TextTableError> {
    use crate::arrow_table::{physical_schema, ArrowTable, TableError};
    use arrow_array::{ArrayRef, RecordBatch, StringArray};
    use std::sync::Arc;
    use yamaa_core::{
        table::{Column, TableSchema},
        value::ColumnType,
    };

    let parsed = parse(content, csv_limits).map_err(TextTableError::Csv)?;
    let rows = parsed.records.len();
    let columns = parsed.names.len();
    // Reserve the resulting shape before allocating Arrow arrays. try_new checks
    // independently after construction; it cannot bound preexisting arrays.
    for (resource, required, limit) in [
        ("columns", Some(columns), table_limits.max_columns),
        ("batches", Some(1), table_limits.max_batches),
        ("rows", Some(rows), table_limits.max_rows),
        ("cells", rows.checked_mul(columns), table_limits.max_cells),
    ] {
        if required.is_none_or(|n| n > limit) {
            return Err(TextTableError::Table(TableError::Limit {
                resource,
                limit,
                required,
            }));
        }
    }
    let schema = TableSchema::new(
        parsed
            .names
            .into_iter()
            .map(|name| Column {
                name,
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .expect("CSV admission established nonempty unique header names");
    let arrays: Vec<ArrayRef> = (0..columns)
        .map(|column| {
            Arc::new(StringArray::from_iter(
                parsed.records.iter().map(|row| row[column].as_deref()),
            )) as ArrayRef
        })
        .collect();
    let batch = RecordBatch::try_new(physical_schema(&schema), arrays)
        .map_err(|error| TextTableError::Table(TableError::Arrow(error)))?;
    ArrowTable::try_new(schema, vec![batch], table_limits).map_err(TextTableError::Table)
}
