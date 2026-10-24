//! Generated record positions retain physical source order (INPUT-3 and INPUT-4).
use crate::{
    diagnostic::SourceFinding,
    table::{Column, TableSchema},
    value::ColumnType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("source ordinal collision")]
    Collision,
    #[error("source ordinal invalid name")]
    InvalidName,
    #[error(
        "source ordinal resource limit: resource={resource:?}, limit={limit:?}, actual={actual:?}"
    )]
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
}
impl Error {
    pub fn finding<'a>(self, dataset: &'a str, field: &'a str) -> Option<SourceFinding<'a>> {
        (self == Self::Collision).then_some(SourceFinding::OrdinalCollision { dataset, field })
    }
}
/// The generated field is admitted once, before any physical allocation.
pub struct Ordinal {
    schema: TableSchema,
    /// The records of the source before the first one admitted.
    start: usize,
    rows: usize,
}
impl Ordinal {
    pub fn admit(schema: &TableSchema, rows: usize, name: &str) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::InvalidName);
        }
        if schema.columns().iter().any(|field| field.name == name) {
            return Err(Error::Collision);
        }
        let columns = schema
            .columns()
            .len()
            .checked_add(1)
            .ok_or(Error::ResourceLimit {
                resource: "columns",
                limit: crate::limits::Limits::COLUMNS,
                actual: None,
            })?;
        crate::limits::Limits::table(rows, columns).map_err(|error| Error::ResourceLimit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        })?;
        let mut columns = schema.columns().to_vec();
        columns.push(Column {
            name: String::from(name),
            kind: ColumnType::Int,
        });
        Ok(Self {
            schema: TableSchema::new(columns).map_err(|_| Error::InvalidName)?,
            start: 0,
            rows,
        })
    }
    /// The same field for admitted records that follow `start` records of
    /// their source, as one partition of a source read by partition does
    /// (#3792): each value is the record's position in the whole source.
    pub fn starting_at(self, start: usize) -> Self {
        Self { start, ..self }
    }
    pub fn schema(&self) -> &TableSchema {
        &self.schema
    }
    pub fn value(&self, position: usize) -> Option<i64> {
        (position < self.rows)
            .then(|| {
                i64::try_from(self.start.checked_add(position)?)
                    .ok()?
                    .checked_add(1)
            })
            .flatten()
    }
}

/// Decide generated-field policy once; adapters only encode the admitted positions.
pub fn prepare(
    source: &crate::specification::SourceDeclaration,
    schema: &crate::table::TableSchema,
    rows: usize,
) -> Result<Option<Ordinal>, Error> {
    source
        .ordinal
        .as_deref()
        .map(|name| Ordinal::admit(schema, rows, name))
        .transpose()
}
