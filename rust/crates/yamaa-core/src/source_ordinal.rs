//! Generated record positions retain physical source order (REQ-1291 to REQ-1294).
use crate::{
    diagnostic::SourceFinding,
    table::{Column, TableSchema},
    value::ColumnType,
};
use alloc::string::String;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub columns: usize,
    pub cells: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Collision,
    InvalidName,
    InvalidTable,
    Limit,
}
impl Error {
    pub fn finding<'a>(self, dataset: &'a str, field: &'a str) -> Option<SourceFinding<'a>> {
        (self == Self::Collision).then_some(SourceFinding::OrdinalCollision { dataset, field })
    }
}
/// Producer contracts describe stored fields before the admitted generated field.
pub fn stored_fields<'a>(
    schema: &'a TableSchema,
    name: Option<&str>,
) -> Result<&'a [Column], Error> {
    let fields = schema.columns();
    let Some(name) = name else { return Ok(fields) };
    let Some((generated, stored)) = fields.split_last() else {
        return Err(Error::InvalidTable);
    };
    if generated.name != name || generated.kind != ColumnType::Int {
        return Err(Error::InvalidTable);
    }
    Ok(stored)
}
/// The generated field is admitted once, before any physical allocation.
pub struct Ordinal {
    schema: TableSchema,
    rows: usize,
}
impl Ordinal {
    pub fn admit(
        schema: &TableSchema,
        rows: usize,
        name: &str,
        limits: Limits,
    ) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::InvalidName);
        }
        if schema.columns().iter().any(|field| field.name == name) {
            return Err(Error::Collision);
        }
        let columns = schema.columns().len().checked_add(1).ok_or(Error::Limit)?;
        if columns > limits.columns
            || rows
                .checked_mul(columns)
                .is_none_or(|cells| cells > limits.cells)
            || i64::try_from(rows).is_err()
        {
            return Err(Error::Limit);
        }
        let mut columns = schema.columns().to_vec();
        columns.push(Column {
            name: String::from(name),
            kind: ColumnType::Int,
        });
        Ok(Self {
            schema: TableSchema::new(columns).map_err(|_| Error::InvalidName)?,
            rows,
        })
    }
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
    pub fn value(&self, position: usize) -> Option<i64> {
        (position < self.rows)
            .then(|| i64::try_from(position).ok()?.checked_add(1))
            .flatten()
    }
}
