//! The read-only Dataset-JSON source profiles (DSJSON-1 to DSJSON-5).
//!
//! A `.json` input is a Dataset-JSON 1.1 file in its single-object
//! representation and an `.ndjson` input one in its newline-delimited
//! representation: a first line holding the metadata object without `rows`,
//! then one row per line. The file alone is the source. Each column's
//! `dataType` supplies its field's type through one closed table, which a
//! `types` entry may only narrow, and every value is read as that type or
//! the read fails. The input is read whole, from its captured bytes, in its
//! stored field and record order.
use crate::column_table::{Column, ColumnTable, TableError, UTF8_OFFSETS};
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use std::fmt;
use yamaa_core::{
    csv_source::Field,
    diagnostic::{DatasetJsonCause, SourceFinding},
    limits::{LimitExceeded, Limits},
    specification::{SourceDeclaration, SourceProfile},
    table::{self, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

/// Why a Dataset-JSON input was not read.
#[derive(Debug)]
pub enum Error {
    /// One closed cause of DSJSON-5.
    Invalid(Invalid),
    /// A value that does not parse as its field's type (DSJSON-3): the type
    /// its `dataType` supplies, or the one a `types` entry `declared`.
    Value {
        field: String,
        target: ColumnType,
        value: String,
        record: usize,
        declared: bool,
    },
    /// A `types` entry naming no field the file stores (INPUT-6).
    UnknownField { field: String },
    /// A fixed limit the file's bytes, columns or rows exceed.
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    /// The table its values make cannot be held.
    Table(TableError),
}

/// The closed causes of DSJSON-5, with the facts that locate each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invalid {
    Malformed,
    Version,
    Member {
        member: &'static str,
        field: Option<usize>,
    },
    FieldName(Field),
    DataType {
        field: String,
        data_type: String,
    },
    FieldType {
        field: String,
        data_type: String,
        declared: ColumnType,
    },
    Records,
    RowWidth {
        record: usize,
    },
    Cell {
        record: usize,
        field: String,
    },
}

impl Error {
    /// The registered finding of a refusal of the file or of one of its
    /// values; `None` for a limit or a table the engine cannot hold.
    pub fn finding<'a>(&'a self, dataset: &'a str, path: &'a str) -> Option<SourceFinding<'a>> {
        let cause = match self {
            Self::Invalid(invalid) => match invalid {
                Invalid::Malformed => DatasetJsonCause::Malformed,
                Invalid::Version => DatasetJsonCause::Version,
                Invalid::Member { member, field } => DatasetJsonCause::Member {
                    member,
                    field: *field,
                },
                Invalid::FieldName(field) => DatasetJsonCause::FieldName { field },
                Invalid::DataType { field, data_type } => {
                    DatasetJsonCause::DataType { field, data_type }
                }
                Invalid::FieldType {
                    field,
                    data_type,
                    declared,
                } => DatasetJsonCause::FieldType {
                    field,
                    data_type,
                    declared: *declared,
                },
                Invalid::Records => DatasetJsonCause::Records,
                Invalid::RowWidth { record } => DatasetJsonCause::RowWidth { record: *record },
                Invalid::Cell { record, field } => DatasetJsonCause::Cell {
                    record: *record,
                    field,
                },
            },
            Self::Value {
                field,
                target,
                value,
                record,
                declared,
            } => {
                return Some(SourceFinding::DatasetJsonValue {
                    dataset,
                    field,
                    target: *target,
                    value,
                    record: *record,
                    declared: *declared,
                })
            }
            Self::UnknownField { field } => {
                return Some(SourceFinding::UnknownField { dataset, field })
            }
            Self::Limit { .. } | Self::Table(_) => return None,
        };
        Some(SourceFinding::DatasetJson {
            dataset,
            path,
            cause,
        })
    }
}

impl From<LimitExceeded> for Error {
    fn from(error: LimitExceeded) -> Self {
        Self::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        }
    }
}

impl From<Invalid> for Error {
    fn from(invalid: Invalid) -> Self {
        Self::Invalid(invalid)
    }
}

/// A column's `dataType`, from the standard's closed list without
/// `boolean`, which names no column type (DSJSON-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataType {
    String,
    Uri,
    Time,
    Integer,
    Float,
    Double,
    Decimal,
    Date,
    Datetime,
}

impl DataType {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "string" => Self::String,
            "URI" => Self::Uri,
            "time" => Self::Time,
            "integer" => Self::Integer,
            "float" => Self::Float,
            "double" => Self::Double,
            "decimal" => Self::Decimal,
            "date" => Self::Date,
            "datetime" => Self::Datetime,
            _ => return None,
        })
    }

    /// The type the closed table gives, inverting JSON-10.
    fn supplies(self) -> ColumnType {
        match self {
            Self::String | Self::Uri | Self::Time => ColumnType::Str,
            Self::Integer => ColumnType::Int,
            Self::Float | Self::Double | Self::Decimal => ColumnType::Float,
            Self::Date => ColumnType::Date,
            Self::Datetime => ColumnType::DateTime,
        }
    }

    /// Whether a value is a JSON string, as JSON-11 writes text and temporal
    /// values and the standard writes a decimal; otherwise it is a number.
    fn text(self) -> bool {
        !matches!(self, Self::Integer | Self::Float | Self::Double)
    }

    /// The narrowing table: a `types` entry may restate the supplied type,
    /// narrow text as a delimited field narrows, narrow a number other than
    /// an integer to `int`, and read any other text as `str`.
    fn admits(self, declared: ColumnType) -> bool {
        declared == self.supplies()
            || match self {
                Self::String | Self::Uri | Self::Time => true,
                Self::Integer => false,
                Self::Float | Self::Double => declared == ColumnType::Int,
                Self::Decimal => matches!(declared, ColumnType::Int | ColumnType::Str),
                Self::Date | Self::Datetime => declared == ColumnType::Str,
            }
    }
}

/// One stored field: its name, its `dataType`, the type it is read as and
/// whether a `types` entry declared that type.
struct Stored {
    name: String,
    data_type: DataType,
    target: ColumnType,
    declared: bool,
}

const VERSION: usize = 0;
const RECORDS: usize = 3;
const COLUMNS: usize = 6;
const ROWS: usize = 7;
/// The members of the file's object the profile reads, in the order it
/// checks them: the version, then the standard's other required members,
/// then `rows`.
const MEMBERS: &[&str] = &[
    "datasetJSONVersion",
    "datasetJSONCreationDateTime",
    "itemGroupOID",
    "records",
    "name",
    "label",
    "columns",
    "rows",
];
/// The members of a `columns` entry the profile reads: the standard's
/// required ones. `targetDataType`, `length`, `displayFormat` and
/// `keySequence` select nothing.
const COLUMN_MEMBERS: &[&str] = &["itemOID", "name", "label", "dataType"];

/// Read a Dataset-JSON input's captured bytes into its table (DSJSON-1).
pub fn parse(bytes: &[u8], source: &SourceDeclaration) -> Result<ColumnTable, Error> {
    Limits::source_bytes(bytes.len())?;
    match source.profile {
        SourceProfile::DatasetJson => {
            let mut reader = serde_json::Deserializer::from_slice(bytes);
            let object = ObjectSeed(MEMBERS)
                .deserialize(&mut reader)
                .and_then(|object| reader.end().map(|()| object))
                .map_err(|_| Invalid::Malformed)?;
            let mut table = Table::new(&object, source, false)?;
            if let Some(rows) = object.values[ROWS] {
                let mut reader = serde_json::Deserializer::from_str(rows.get());
                let read = RowsSeed(&mut table).deserialize(&mut reader);
                table.settle(read)?;
            }
            table.finish()
        }
        SourceProfile::DatasetNdjson => {
            // Each line ends in U+000A, which the last may omit, and a
            // U+000D before it belongs to the line ending (JSON-13).
            let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
            let mut lines = body
                .split(|&byte| byte == b'\n')
                .map(|line| line.strip_suffix(b"\r").unwrap_or(line));
            let first = lines.next().unwrap_or_default();
            let mut reader = serde_json::Deserializer::from_slice(first);
            let object = ObjectSeed(MEMBERS)
                .deserialize(&mut reader)
                .and_then(|object| reader.end().map(|()| object))
                .map_err(|_| Invalid::Malformed)?;
            let mut table = Table::new(&object, source, true)?;
            for line in lines {
                let mut reader = serde_json::Deserializer::from_slice(line);
                let read = RowSeed(&mut table)
                    .deserialize(&mut reader)
                    .and_then(|()| reader.end());
                table.settle(read)?;
            }
            table.finish()
        }
        _ => unreachable!("{} is not a Dataset-JSON input", source.path),
    }
}

/// The member at `index` of [`MEMBERS`] is absent, repeated or of another
/// JSON kind.
fn member(index: usize) -> Invalid {
    Invalid::Member {
        member: MEMBERS[index],
        field: None,
    }
}

/// A JSON string's text, or `None` for any other JSON value. A string whose
/// escapes name no Unicode scalar value is no JSON text the profile reads.
fn text(raw: &RawValue) -> Result<Option<String>, Invalid> {
    if !raw.get().starts_with('"') {
        return Ok(None);
    }
    serde_json::from_str::<String>(raw.get())
        .map(Some)
        .map_err(|_| Invalid::Malformed)
}

/// The admitted header and the values read so far.
struct Table<'s> {
    source: &'s SourceDeclaration,
    fields: Vec<Stored>,
    columns: Vec<Column>,
    /// The rows `records` counts.
    records: usize,
    /// The rows read so far.
    rows: usize,
    /// The finding that stopped a read of the rows.
    failure: Option<Error>,
}

impl<'s> Table<'s> {
    /// Admit the file's object, its columns and the input's `types` in the
    /// order DSJSON-5 states, before any row is read. A newline-delimited
    /// file's first line holds no `rows` (JSON-7).
    fn new(object: &Object<'_>, source: &'s SourceDeclaration, lines: bool) -> Result<Self, Error> {
        let present = |index: usize| match object.values[index] {
            Some(raw) if object.repeated != Some(index) => Ok(raw),
            _ => Err(member(index)),
        };
        let version = text(present(VERSION)?)?.ok_or(member(VERSION))?;
        if !version_1_1(&version) {
            return Err(Invalid::Version.into());
        }
        // The other members in the standard's order, each of its JSON kind.
        let mut records = 0;
        for index in 1..ROWS {
            let raw = present(index)?;
            let kind = match index {
                RECORDS => match serde_json::from_str::<u64>(raw.get())
                    .ok()
                    .and_then(|count| usize::try_from(count).ok())
                {
                    Some(count) => {
                        records = count;
                        true
                    }
                    None => false,
                },
                COLUMNS => raw.get().starts_with('['),
                _ => text(raw)?.is_some(),
            };
            if !kind {
                return Err(member(index).into());
            }
        }
        // `rows` is an array of rows, and a newline-delimited first line
        // holds none (JSON-7).
        let rows_kind = object.values[ROWS].is_none_or(|rows| rows.get().starts_with('['));
        if object.repeated == Some(ROWS) || !rows_kind || lines && object.values[ROWS].is_some() {
            return Err(member(ROWS).into());
        }
        let fields = fields(present(COLUMNS)?, source)?;
        Limits::table(records, fields.len())?;
        let columns = fields
            .iter()
            .map(|field| Column::empty(field.target))
            .collect();
        Ok(Self {
            source,
            fields,
            columns,
            records,
            rows: 0,
            failure: None,
        })
    }

    /// The outcome of reading rows: the finding a row made, or the bytes'
    /// own fault.
    fn settle(&mut self, read: Result<(), serde_json::Error>) -> Result<(), Error> {
        match (self.failure.take(), read) {
            (Some(failure), _) => Err(failure),
            (None, Err(_)) => Err(Invalid::Malformed.into()),
            (None, Ok(())) => Ok(()),
        }
    }

    /// Stop reading the rows with `error`.
    fn fail<E: de::Error>(&mut self, error: impl Into<Error>) -> E {
        self.failure = Some(error.into());
        E::custom("dataset_json")
    }

    /// Read one value of the row being read into its column.
    fn value(&mut self, raw: &RawValue, column: usize) -> Result<(), Error> {
        let record = self.rows + 1;
        let field = &self.fields[column];
        let token = raw.get();
        let cell = || Invalid::Cell {
            record,
            field: field.name.clone(),
        };
        let value = match token.as_bytes().first() {
            Some(b'n') => Value::Missing,
            Some(b'"') if field.data_type.text() => {
                let stored = text(raw)?.ok_or_else(cell)?;
                // A zero-length string of a field read as text follows the
                // input's empty-string convention (INPUT-9).
                let as_text = field.data_type.supplies() == ColumnType::Str
                    || field.target == ColumnType::Str;
                if as_text && self.source.text_is_missing(&stored) {
                    Value::Missing
                } else {
                    self.parsed(column, stored, record)?
                }
            }
            Some(b'-' | b'0'..=b'9') if !field.data_type.text() => {
                self.parsed(column, token.to_owned(), record)?
            }
            _ => return Err(cell().into()),
        };
        self.columns[column].push(ValueRef::from(&value));
        Ok(())
    }

    /// A stored text read as its field's type by the `str` row of VALUE-3,
    /// as INPUT-7 reads a delimited field.
    fn parsed(&self, column: usize, stored: String, record: usize) -> Result<Value, Error> {
        let field = &self.fields[column];
        let value = Value::Str(stored);
        yamaa_core::conversion::convert(&value, field.target).map_err(|_| {
            let Value::Str(value) = value else {
                unreachable!("the stored value is text")
            };
            Error::Value {
                field: field.name.clone(),
                target: field.target,
                value,
                record,
                declared: field.declared,
            }
        })
    }

    /// The table, once every row is read and `records` counts them.
    fn finish(self) -> Result<ColumnTable, Error> {
        if self.rows != self.records {
            return Err(Invalid::Records.into());
        }
        for column in &self.columns {
            if let Column::Str(text) = column {
                if i32::try_from(text.bytes_in(0..self.rows)).is_err() {
                    return Err(Error::Table(TableError::Representation {
                        representation: UTF8_OFFSETS,
                    }));
                }
            }
        }
        let schema = TableSchema::new(
            self.fields
                .into_iter()
                .map(|field| table::Column {
                    name: field.name,
                    kind: field.target,
                })
                .collect(),
        )
        .map_err(|_| Error::Table(TableError::SchemaMismatch { batch: 0 }))?;
        ColumnTable::new(schema, self.columns, self.rows).map_err(Error::Table)
    }
}

/// `datasetJSONVersion` names a 1.1 release: `1.1`, or `1.1.` and a release
/// number without a leading zero, as the standard's schema writes it.
fn version_1_1(version: &str) -> bool {
    match version.strip_prefix("1.1") {
        Some("") => true,
        Some(rest) => rest.strip_prefix('.').is_some_and(|number| {
            number == "0"
                || (!number.starts_with('0')
                    && !number.is_empty()
                    && number.bytes().all(|byte| byte.is_ascii_digit()))
        }),
        None => false,
    }
}

/// The stored fields `columns` declares, with each one's type and the
/// input's `types` admitted against them.
fn fields(columns: &RawValue, source: &SourceDeclaration) -> Result<Vec<Stored>, Error> {
    let mut limit = None;
    let entries = ColumnsSeed(&mut limit)
        .deserialize(&mut serde_json::Deserializer::from_str(columns.get()))
        .map_err(|_| match limit.take() {
            Some(limit) => Error::from(limit),
            None => Error::from(member(COLUMNS)),
        })?;
    let mut declared = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let position = Some(index + 1);
        let entry_member = |member: &'static str| Invalid::Member {
            member,
            field: position,
        };
        let object = ObjectSeed(COLUMN_MEMBERS)
            .deserialize(&mut serde_json::Deserializer::from_str(entry.get()))
            .map_err(|_| entry_member("columns"))?;
        let mut values = Vec::with_capacity(COLUMN_MEMBERS.len());
        for (at, name) in COLUMN_MEMBERS.iter().enumerate() {
            let value = match object.values[at].filter(|_| object.repeated != Some(at)) {
                Some(raw) => text(raw)?,
                None => None,
            };
            values.push(value.ok_or_else(|| entry_member(name))?);
        }
        let [_, name, _, data_type] = <[String; 4]>::try_from(values)
            .unwrap_or_else(|_| unreachable!("one value per column member"));
        declared.push((data_type, name));
    }
    let mut names = std::collections::BTreeSet::new();
    for (index, (_, name)) in declared.iter().enumerate() {
        if name.is_empty() {
            return Err(Invalid::FieldName(Field::Index(index + 1)).into());
        }
        if !names.insert(name.as_str()) {
            return Err(Invalid::FieldName(Field::Name(name.clone())).into());
        }
    }
    let mut fields = Vec::with_capacity(declared.len());
    for (data_type, name) in declared {
        let Some(kind) = DataType::named(&data_type) else {
            return Err(Invalid::DataType {
                field: name,
                data_type,
            }
            .into());
        };
        fields.push((name, kind, data_type));
    }
    // The input's `types` entries in written order, each naming a stored
    // field its `dataType` admits the declared type of.
    for (name, declared) in &source.types {
        let Some((_, data_type, written)) = fields.iter().find(|(field, ..)| field == name) else {
            return Err(Error::UnknownField {
                field: name.clone(),
            });
        };
        if !data_type.admits(*declared) {
            return Err(Invalid::FieldType {
                field: name.clone(),
                data_type: written.clone(),
                declared: *declared,
            }
            .into());
        }
    }
    Ok(fields
        .into_iter()
        .map(|(name, data_type, _)| {
            let declared = source
                .types
                .iter()
                .find(|(field, _)| *field == name)
                .map(|&(_, kind)| kind);
            Stored {
                name,
                data_type,
                target: declared.unwrap_or(data_type.supplies()),
                declared: declared.is_some(),
            }
        })
        .collect())
}

/// The members of one JSON object the profile reads, each unparsed, and the
/// first of them written twice.
struct Object<'a> {
    values: Vec<Option<&'a RawValue>>,
    repeated: Option<usize>,
}

/// Read a JSON object, keeping only the members `.0` names; any other value
/// is skipped unread.
struct ObjectSeed(&'static [&'static str]);

impl<'de> DeserializeSeed<'de> for ObjectSeed {
    type Value = Object<'de>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Object<'de>, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ObjectSeed {
    type Value = Object<'de>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Object<'de>, A::Error> {
        let mut values = vec![None; self.0.len()];
        let mut repeated = None;
        while let Some(index) = map.next_key_seed(KeySeed(self.0))? {
            match index {
                Some(index) => {
                    let value = map.next_value::<&'de RawValue>()?;
                    if values[index].replace(value).is_some() && repeated.is_none() {
                        repeated = Some(index);
                    }
                }
                None => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Object { values, repeated })
    }
}

/// A member name: its position among the names the profile reads, if any.
struct KeySeed(&'static [&'static str]);

impl<'de> DeserializeSeed<'de> for KeySeed {
    type Value = Option<usize>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Option<usize>, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed {
    type Value = Option<usize>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a member name")
    }
    fn visit_str<E: de::Error>(self, name: &str) -> Result<Option<usize>, E> {
        Ok(self.0.iter().position(|known| *known == name))
    }
}

/// The entries of `columns`, unparsed, admitting their count under the
/// column limit before any is kept.
struct ColumnsSeed<'l>(&'l mut Option<LimitExceeded>);

impl<'de> DeserializeSeed<'de> for ColumnsSeed<'_> {
    type Value = Vec<&'de RawValue>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ColumnsSeed<'_> {
    type Value = Vec<&'de RawValue>;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of column objects")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        while let Some(entry) = seq.next_element::<&'de RawValue>()? {
            if let Err(limit) = Limits::columns(entries.len() + 1) {
                *self.0 = Some(limit);
                return Err(de::Error::custom("columns"));
            }
            entries.push(entry);
        }
        Ok(entries)
    }
}

/// The single-object form's `rows`: each entry one row.
struct RowsSeed<'t, 's>(&'t mut Table<'s>);

impl<'de> DeserializeSeed<'de> for RowsSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for RowsSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of rows")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(RowSeed(self.0))?.is_some() {}
        Ok(())
    }
}

/// One row: an array of one value per column, in `columns` order. A row
/// beyond the ones `records` counts is refused before it is read.
struct RowSeed<'t, 's>(&'t mut Table<'s>);

impl<'de> DeserializeSeed<'de> for RowSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        if self.0.rows == self.0.records {
            return Err(self.0.fail(Invalid::Records));
        }
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for RowSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a row")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let table = self.0;
        let record = table.rows + 1;
        for column in 0..table.fields.len() {
            let Some(raw) = seq.next_element::<&'de RawValue>()? else {
                return Err(table.fail(Invalid::RowWidth { record }));
            };
            if let Err(error) = table.value(raw, column) {
                return Err(table.fail(error));
            }
        }
        if seq.next_element::<IgnoredAny>()?.is_some() {
            return Err(table.fail(Invalid::RowWidth { record }));
        }
        table.rows = record;
        Ok(())
    }
    // Any other JSON value is not a row.
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<(), A::Error> {
        Err(self.not_a_row())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Err(self.not_a_row())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Err(self.not_a_row())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Err(self.not_a_row())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Err(self.not_a_row())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Err(self.not_a_row())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Err(self.not_a_row())
    }
}

impl RowSeed<'_, '_> {
    fn not_a_row<E: de::Error>(self) -> E {
        let record = self.0.rows + 1;
        self.0.fail(Invalid::RowWidth { record })
    }
}
