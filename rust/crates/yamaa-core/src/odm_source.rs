//! Fixed ODM metadata admission, independent of physical storage and study reads.
use crate::value::ColumnType;
use alloc::{string::String, vec::Vec};

pub const FIELDS: [&str; 11] = [
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "FormOID",
    "FormRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
    "ItemOID",
    "Value",
];
pub const HIERARCHY: [usize; 8] = [0, 2, 3, 4, 5, 6, 7, 8];
pub const REQUIRED: [usize; 7] = [0, 1, 2, 3, 5, 7, 9];
pub const IDENTIFYING: [usize; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

/// Only ASCII letters fold. Unicode lookalikes remain vendor field names.
pub fn canonical(name: &str) -> Option<usize> {
    FIELDS
        .iter()
        .position(|field| field.eq_ignore_ascii_case(name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredField<'a> {
    pub name: &'a str,
    /// None is a physical type outside the ordinary closed profile. Vendor
    /// fields need no logical type and are never admitted as exposed columns.
    pub kind: Option<ColumnType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaError {
    Missing {
        fields: Vec<&'static str>,
    },
    Ambiguous {
        field: &'static str,
        stored: Vec<String>,
    },
    Type {
        field: &'static str,
        stored_field: String,
        kind: Option<ColumnType>,
    },
}
impl SchemaError {
    pub fn finding<'a>(&'a self, dataset: &'a str) -> crate::diagnostic::OdmFinding<'a> {
        use crate::diagnostic::OdmFinding as F;
        match self {
            Self::Missing { fields } => F::MissingFields { dataset, fields },
            Self::Ambiguous { field, stored } => F::AmbiguousField {
                dataset,
                field,
                stored,
            },
            Self::Type {
                field,
                stored_field,
                kind,
            } => F::FieldType {
                dataset,
                field,
                stored_field,
                kind: *kind,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    MetadataLimit,
}

/// Coordinates are admitted from metadata alone, before any record is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldBinding {
    stored: [usize; 11],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingValues {
    pub field: &'static str,
    pub record: usize,
    pub records: usize,
}
impl MissingValues {
    pub fn finding(self, dataset: &str) -> crate::diagnostic::OdmFinding<'_> {
        crate::diagnostic::OdmFinding::MissingValues {
            dataset,
            field: self.field,
            record: self.record,
            records: self.records,
        }
    }
}
#[derive(Debug, PartialEq)]
pub enum RecordError<E> {
    Schema,
    Coordinate,
    Cell {
        row: usize,
        column: usize,
        error: crate::table::CellError<E>,
    },
}

/// Verify identifiers in fixed field order before granting expression reads.
/// The original table error is moved unchanged; no diagnostic hides a read failure.
pub fn missing_values<T: crate::table::TableAccess + ?Sized>(
    table: &T,
    first_record: usize,
) -> Result<Vec<MissingValues>, RecordError<T::Error>> {
    let columns = table.schema().columns();
    if columns.len() != FIELDS.len()
        || columns
            .iter()
            .enumerate()
            .any(|(index, column)| column.name != FIELDS[index] || column.kind != ColumnType::Str)
    {
        return Err(RecordError::Schema);
    }
    let mut findings = Vec::new();
    for column in REQUIRED {
        let mut first = None;
        let mut count = 0usize;
        for row in 0..table.row_count() {
            let value = table.cell(row, column).map_err(|error| RecordError::Cell {
                row,
                column,
                error,
            })?;
            if matches!(value, crate::table::ValueRef::Missing) {
                first.get_or_insert(row);
                count += 1;
            }
        }
        if let Some(row) = first {
            findings.push(MissingValues {
                field: FIELDS[column],
                record: first_record
                    .checked_add(row)
                    .ok_or(RecordError::Coordinate)?,
                records: count,
            });
        }
    }
    Ok(findings)
}
impl FieldBinding {
    pub fn stored(&self) -> &[usize; 11] {
        &self.stored
    }
}

pub fn bind_fields(
    fields: &[StoredField<'_>],
) -> Result<Result<FieldBinding, Vec<SchemaError>>, AdmissionError> {
    if fields.len() > 4096
        || fields
            .iter()
            .try_fold(0usize, |bytes, field| bytes.checked_add(field.name.len()))
            .is_none_or(|bytes| bytes > 1_048_576)
    {
        return Err(AdmissionError::MetadataLimit);
    }
    let mut first = [None; 11];
    let mut counts = [0usize; 11];
    let mut order = Vec::new();
    for (stored, field) in fields.iter().enumerate() {
        if let Some(canonical) = canonical(field.name) {
            if first[canonical].is_none() {
                first[canonical] = Some(stored);
                order.push(canonical);
            }
            counts[canonical] += 1;
        }
    }
    let missing = FIELDS
        .iter()
        .enumerate()
        .filter_map(|(index, &name)| (counts[index] == 0).then_some(name))
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    if !missing.is_empty() {
        errors.push(SchemaError::Missing { fields: missing });
    }
    // Ambiguities retain first appearance in the stored metadata. Missing
    // fields retain the fixed schema order; type findings retain stored order.
    for &index in &order {
        if counts[index] > 1 {
            errors.push(SchemaError::Ambiguous {
                field: FIELDS[index],
                stored: fields
                    .iter()
                    .filter(|field| canonical(field.name) == Some(index))
                    .map(|field| field.name.into())
                    .collect(),
            });
        }
    }
    for &index in &order {
        if counts[index] == 1 {
            let field = fields[first[index].expect("present metadata coordinate")];
            if field.kind != Some(ColumnType::Str) {
                errors.push(SchemaError::Type {
                    field: FIELDS[index],
                    stored_field: field.name.into(),
                    kind: field.kind,
                });
            }
        }
    }
    Ok(if errors.is_empty() {
        Ok(FieldBinding {
            stored: first.map(|field| field.expect("complete admitted metadata")),
        })
    } else {
        Err(errors)
    })
}
