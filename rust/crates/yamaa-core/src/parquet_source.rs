//! The closed Parquet profile's source findings and their order, over the
//! fields a codec read; the codec reads each field's column type from the
//! profile's data file.
use crate::{
    diagnostic::{ParquetFinding, SourceFinding},
    table::{Column, TableSchema},
    temporal::{Date, DateTime, Precision},
    value::ColumnType,
};
use std::collections::BTreeSet;

/// One field of a Parquet file, as its codec read it: the column type the
/// closed profile's column rows give it, if any, and the stored type a
/// refused field's finding spells.
pub struct Field {
    pub name: String,
    pub kind: Option<ColumnType>,
    pub stored_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("parquet source invalid")]
    Invalid,
    #[error("parquet source empty name: field={field:?}")]
    EmptyName { field: usize },
    #[error("parquet source duplicate name: field={field:?}")]
    DuplicateName { field: String },
    #[error("parquet source unsupported: field={field:?}, stored_type={stored_type:?}")]
    Unsupported { field: String, stored_type: String },
    #[error("parquet source value: field={field:?}, row={row:?}, value={value:?}")]
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
        let kind = field.kind.ok_or_else(|| Error::Unsupported {
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

/// The second a count names in a unit of which `per_second` make one
/// second, or None for a count short of a whole second (PARQUET-9). A file's
/// stored count and a host frame's count are both judged here, so the two
/// ways into the engine admit the same values.
pub fn whole_second(value: i64, per_second: i64) -> Option<i64> {
    (value % per_second == 0).then_some(value / per_second)
}

/// Raw storage integers are checked before conversion or rounding: a date
/// counts days, and a datetime counts a unit of which `per_second` make one
/// second, its timestamp's, and must be a whole second (PARQUET-9). Temporal
/// source findings retain raw values and one-based source rows.
pub fn temporal(
    kind: ColumnType,
    value: i64,
    per_second: i64,
    field: &str,
    row: usize,
) -> Result<(), Error> {
    // The calendar's days and seconds are those `Date` and `DateTime` decode.
    let valid = match kind {
        ColumnType::Date => Date::from_epoch_day(value, Precision::Day).is_some(),
        ColumnType::DateTime => whole_second(value, per_second)
            .is_some_and(|second| DateTime::from_epoch_second(second, Precision::Second).is_some()),
        ColumnType::Time => {
            whole_second(value, per_second).is_some_and(|second| (0..86_400).contains(&second))
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
