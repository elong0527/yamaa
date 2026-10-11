//! Closed source type admission over portable storage metadata, without a codec.
use crate::{
    diagnostic::{Diagnostic, ParquetFinding, SourceFinding},
    table::{Column, TableSchema},
    value::ColumnType,
};
use alloc::{collections::BTreeSet, string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Physical {
    ByteArray,
    Int32,
    Int64,
    Double,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Second,
    Millisecond,
    Microsecond,
    Nanosecond,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Logical {
    None,
    String,
    Date,
    Timestamp { unit: Unit, utc: bool },
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Converted {
    None,
    Utf8,
    Date,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    Text,
    Int64,
    Float64,
    Date32,
    Timestamp { unit: Unit, timezone: bool },
    Other,
}

/// Physical metadata and retained diagnostic spelling supplied by a codec.
pub struct Field {
    pub name: String,
    pub leaves: usize,
    pub path: String,
    pub repetition: i16,
    pub physical: Physical,
    pub logical: Logical,
    pub converted: Converted,
    pub representation: Representation,
    pub stored_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    EmptyName {
        field: usize,
    },
    DuplicateName {
        field: String,
    },
    Unsupported {
        field: String,
        stored_type: String,
    },
    Value {
        field: String,
        row: usize,
        value: i64,
    },
}
impl Error {
    /// Borrow exact physical-schema and raw-value facts from the original failure.
    pub fn finding<'a>(&'a self, dataset: &'a str, path: &'a str) -> SourceFinding<'a> {
        let finding = match self {
            Self::Invalid => ParquetFinding::Invalid,
            Self::EmptyName { field } => ParquetFinding::EmptyName { field: *field },
            Self::DuplicateName { field } => ParquetFinding::DuplicateName { field },
            Self::Unsupported { field, stored_type } => {
                ParquetFinding::Unsupported { field, stored_type }
            }
            Self::Value { field, row, value } => ParquetFinding::Value {
                field,
                row: *row,
                value: *value,
            },
        };
        SourceFinding::Parquet {
            finding,
            dataset,
            path,
        }
    }
    /// Unbounded owned compatibility. Complete borrowed report admission is the
    /// replacement; this original always-semantic return signature is unchanged.
    pub fn diagnostic(&self, dataset: &str, path: &str) -> Diagnostic {
        self.finding(dataset, path).owned_compatibility()
    }
}

/// Legacy annotations and logical annotations admit the same closed pairs.
/// Values never infer schema, dictionary, timestamp or nested-field admission.
pub fn column_type(
    physical: Physical,
    logical: Logical,
    converted: Converted,
    representation: Representation,
) -> Option<ColumnType> {
    let string =
        logical == Logical::String || (logical == Logical::None && converted == Converted::Utf8);
    let date =
        logical == Logical::Date || (logical == Logical::None && converted == Converted::Date);
    let none = logical == Logical::None && converted == Converted::None;
    let timestamp = logical
        == Logical::Timestamp {
            unit: Unit::Microsecond,
            utc: false,
        };
    match (physical, representation) {
        (Physical::ByteArray, Representation::Text) if string => Some(ColumnType::Str),
        (Physical::Int64, Representation::Int64) if none => Some(ColumnType::Int),
        (Physical::Double, Representation::Float64) if none => Some(ColumnType::Float),
        (Physical::Int32, Representation::Date32) if date => Some(ColumnType::Date),
        (
            Physical::Int64,
            Representation::Timestamp {
                unit: Unit::Microsecond,
                timezone: false,
            },
        ) if timestamp => Some(ColumnType::DateTime),
        _ => None,
    }
}

/// Empty schema/alignment precede field findings; names precede types per field.
/// A nested root's failure belongs to that root, never an earlier/later leaf.
pub fn columns<I>(fields: I, roots: usize) -> Result<TableSchema, Error>
where
    I: IntoIterator<Item = Field>,
    I::IntoIter: ExactSizeIterator,
{
    let fields = fields.into_iter();
    if fields.len() == 0 || roots != fields.len() {
        return Err(Error::Invalid);
    }
    let mut names = BTreeSet::new();
    let mut columns = Vec::with_capacity(fields.len());
    for (index, field) in fields.enumerate() {
        if field.name.is_empty() {
            return Err(Error::EmptyName { field: index + 1 });
        }
        if !names.insert(field.name.clone()) {
            return Err(Error::DuplicateName { field: field.name });
        }
        let kind = (field.leaves == 1 && field.repetition == 0 && field.path == field.name)
            .then(|| {
                column_type(
                    field.physical,
                    field.logical,
                    field.converted,
                    field.representation,
                )
            })
            .flatten()
            .ok_or_else(|| Error::Unsupported {
                field: field.name.clone(),
                stored_type: field.stored_type,
            })?;
        columns.push(Column {
            name: field.name,
            kind,
        });
    }
    TableSchema::new(columns).map_err(|_| Error::Invalid)
}

/// Raw storage integers are checked before conversion or rounding. Temporal
/// source findings retain raw values and one-based source rows.
pub fn temporal(kind: ColumnType, value: i64, field: &str, row: usize) -> Result<(), Error> {
    const MIN_DAY: i64 = -719_162;
    const MAX_DAY: i64 = 2_932_896;
    let valid = match kind {
        ColumnType::Date => (MIN_DAY..=MAX_DAY).contains(&value),
        ColumnType::DateTime => {
            (MIN_DAY * 86_400 * 1_000_000..=(MAX_DAY * 86_400 + 86_399) * 1_000_000)
                .contains(&value)
                && value % 1_000_000 == 0
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::Value {
            field: field.into(),
            row,
            value,
        })
    }
}
