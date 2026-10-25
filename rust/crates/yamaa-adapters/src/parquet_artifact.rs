//! Closed Parquet output profile over an admitted, immutable typed table.
//! The file writer enforces fixed output bytes and table geometry.
//! No filesystem authority, Arrow metadata, compression or numeric rounding.
use yamaa_core::{
    limits::Limits,
    table::{CellError, TableAccess, ValueRef},
};
#[cfg(feature = "parquet")]
mod encode;
#[cfg(all(test, feature = "parquet"))]
mod upstream;

#[derive(Debug)]
pub enum Error<E> {
    /// This build omits the optional Parquet reader and writer.
    Unavailable,
    Projection,
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Cell(CellError<E>),
    ValueType,
    /// More row groups than an i16 ordinal numbers, which the 1.0 writer
    /// refused (D160).
    RowGroups,
}

/// Artifact encoding retains a capability boundary without the codec dependency.
pub fn render<T: TableAccess>(table: &T, projection: &[usize]) -> Result<Vec<u8>, Error<T::Error>> {
    let whole = 0..table.row_count();
    render_partitions(table, projection, std::slice::from_ref(&whole)).map(|(bytes, _)| bytes)
}

/// An encoded file, and where each of its partitions' bytes end, the last at
/// its length.
pub type Partitioned = (Vec<u8>, Vec<usize>);

/// [`render`] one partition of rows at a time (D264), over `partitions`, the
/// consecutive ranges that divide the table's rows in order. The writer
/// holds rows until its row group is full and writes the last group when
/// the file ends, so the row groups, and so the bytes, equal the
/// whole-table render's whatever the partitions (D160). Each partition is
/// admitted on its own; the end of each partition's bytes is returned with
/// them, the last at the file's length.
pub fn render_partitions<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[std::ops::Range<usize>],
) -> Result<Partitioned, Error<T::Error>> {
    #[cfg(feature = "parquet")]
    {
        encode::render_partitions(table, projection, partitions)
    }
    #[cfg(not(feature = "parquet"))]
    {
        let _ = (table, projection, partitions);
        Err(Error::Unavailable)
    }
}

struct RecordText {
    current: String,
}
impl RecordText {
    fn append(&mut self, text: &str) {
        self.current.push_str(text);
    }
    fn quoted(&mut self, text: &str) {
        self.append("\"");
        for ch in text.chars() {
            match ch {
                '"' => self.append("\\\""),
                '\\' => self.append("\\\\"),
                '\n' => self.append("\\n"),
                '\r' => self.append("\\r"),
                '\t' => self.append("\\t"),
                '\u{8}' => self.append("\\b"),
                '\u{c}' => self.append("\\f"),
                ' '..='~' => self.append(ch.encode_utf8(&mut [0; 4])),
                _ => {
                    for value in ch.encode_utf16(&mut [0; 2]) {
                        self.append(&format!("\\u{value:04x}"));
                    }
                }
            }
        }
        self.append("\"")
    }
}

/// Portable report records use the reference JSON spelling, independently of
/// Parquet's binary encoding. Dates name complete stored calendar fields. The
/// table is admitted whole, or each of its partitions of at most `partition`
/// rows for an output held in partitions (D264).
pub fn records<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partition: Option<std::num::NonZeroUsize>,
) -> Result<Vec<String>, Error<T::Error>> {
    if projection.is_empty()
        || projection
            .iter()
            .any(|&i| i >= table.schema().columns().len())
    {
        return Err(Error::Projection);
    }
    Limits::partitioned(table.row_count(), partition, projection.len()).map_err(|error| {
        Error::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        }
    })?;
    let mut text = RecordText {
        current: String::new(),
    };
    text.append("[");
    for (i, &column) in projection.iter().enumerate() {
        if i != 0 {
            text.append(", ");
        }
        text.quoted(&table.schema().columns()[column].name);
    }
    text.append("]");
    let mut records = vec![std::mem::take(&mut text.current)];
    for row in 0..table.row_count() {
        text.append("[");
        for (i, &column) in projection.iter().enumerate() {
            if i != 0 {
                text.append(", ");
            }
            match table.cell(row, column).map_err(Error::Cell)? {
                ValueRef::Missing => text.append("null"),
                ValueRef::Str(value) => text.quoted(value),
                ValueRef::Int(value) => text.append(&value.to_string()),
                ValueRef::Float(value) => {
                    // The shared finite-scalar renderer already pins Python
                    // shortest-round-trip spelling, exponent padding and -0.0.
                    let node = yamaa_core::schema::DocumentNode::Float(value.get());
                    let scalar = yamaa_core::schema::scalar_diagnostic_label(&node)
                        .ok_or(Error::ValueType)?;
                    text.append(&scalar);
                }
                ValueRef::Date(value) => {
                    let (year, month, day) = value.fields();
                    text.quoted(&format!("{year:04}-{month:02}-{day:02}"));
                }
                ValueRef::DateTime(value) => {
                    let (year, month, day, hour, minute, second) = value.fields();
                    text.quoted(&format!(
                        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
                    ));
                }
                ValueRef::Bool(_) => return Err(Error::ValueType),
            }
        }
        text.append("]");
        records.push(std::mem::take(&mut text.current));
    }
    Ok(records)
}
