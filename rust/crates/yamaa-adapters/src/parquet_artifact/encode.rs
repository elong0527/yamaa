//! The native writer of the closed Parquet output profile (#3878).
//!
//! It writes the bytes that the 1.0 writer, parquet 55.2.0's
//! `SerializedFileWriter` under the settings below, saved (D160 on #3030), so
//! every 1.x release keeps them, the footer's `created_by` included. Each
//! projected column is an OPTIONAL leaf of the type PARQUET-1 maps its column
//! type to. Rows fill row groups of 1,024; each column chunk of a row group is
//! one uncompressed V1 data page of PLAIN values after its RLE definition
//! levels, with no dictionary, statistics, page index or key-value metadata.
//! The footer lists the column chunks' encodings, PLAIN and RLE, one page
//! encoding statistic and a type-defined column order for each column.
//!
//! The footer and page headers are Thrift compact structs (`Compact`), and the
//! definition levels are the RLE/bit-packed hybrid as parquet 55's
//! `RleEncoder` chose its runs (`Levels`), so the bytes are the same.
use super::Error;
use crate::epoch::{date_days, datetime_seconds};
use std::ops::Range;
use yamaa_core::{
    limits::Limits,
    table::{TableAccess, ValueRef},
    value::ColumnType,
};

/// The rows of a full row group.
const ROW_GROUP_ROWS: usize = 1024;

/// The writer that the footer names (D160).
const CREATED_BY: &[u8] = b"parquet-rs version 55.2.0";

/// The magic bytes that begin and end a Parquet file.
const MAGIC: &[u8] = b"PAR1";

/// The most row groups a file holds: the 1.0 writer numbered each with an
/// i16 ordinal and refused the next.
const ROW_GROUPS: usize = 1 << 15;

/// Format codes of the Parquet Thrift definition.
mod code {
    pub const INT32: i32 = 1;
    pub const INT64: i32 = 2;
    pub const DOUBLE: i32 = 5;
    pub const BYTE_ARRAY: i32 = 6;
    pub const OPTIONAL: i32 = 1;
    pub const UTF8: i32 = 0;
    pub const DATE: i32 = 6;
    pub const TIMESTAMP_MICROS: i32 = 10;
    pub const PLAIN: i32 = 0;
    pub const RLE: i32 = 3;
    pub const UNCOMPRESSED: i32 = 0;
    pub const DATA_PAGE: i32 = 0;
}

/// Thrift compact wire types.
mod wire {
    pub const TRUE: u8 = 1;
    pub const FALSE: u8 = 2;
    pub const I16: u8 = 4;
    pub const I32: u8 = 5;
    pub const I64: u8 = 6;
    pub const BINARY: u8 = 8;
    pub const LIST: u8 = 9;
    pub const STRUCT: u8 = 12;
}

/// A Thrift compact struct being written, with the last field id of each
/// open struct, from which a field header's delta counts.
struct Compact {
    bytes: Vec<u8>,
    last: Vec<i16>,
}

impl Compact {
    /// An open struct: the file metadata or a page header.
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            last: vec![0],
        }
    }

    fn varint(&mut self, mut value: u64) {
        while value >= 0x80 {
            self.bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }

    /// The header of field `id` of wire type `kind`: the delta from the last
    /// id in its four high bits when it is 1 to 15, or else the type, then
    /// the id as a zigzag varint.
    fn field(&mut self, id: i16, kind: u8) {
        let last = self.last.last_mut().expect("an open struct");
        let delta = id - *last;
        *last = id;
        if (1..=15).contains(&delta) {
            self.bytes.push((delta as u8) << 4 | kind);
        } else {
            self.bytes.push(kind);
            self.varint(zigzag(i64::from(id)));
        }
    }

    fn i16(&mut self, id: i16, value: i16) {
        self.field(id, wire::I16);
        self.varint(zigzag(i64::from(value)));
    }

    fn i32(&mut self, id: i16, value: i32) {
        self.field(id, wire::I32);
        self.varint(zigzag(i64::from(value)));
    }

    fn i64(&mut self, id: i16, value: i64) {
        self.field(id, wire::I64);
        self.varint(zigzag(value));
    }

    fn bool(&mut self, id: i16, value: bool) {
        self.field(id, if value { wire::TRUE } else { wire::FALSE });
    }

    fn binary(&mut self, id: i16, value: &[u8]) {
        self.field(id, wire::BINARY);
        self.element_binary(value);
    }

    /// Open the struct of field `id`.
    fn begin(&mut self, id: i16) {
        self.field(id, wire::STRUCT);
        self.last.push(0);
    }

    /// Open a struct element of a list.
    fn element(&mut self) {
        self.last.push(0);
    }

    /// Close the innermost open struct with its stop byte.
    fn end(&mut self) {
        self.bytes.push(0);
        self.last.pop();
    }

    /// Field `id`, a list of `size` elements of wire type `kind`.
    fn list(&mut self, id: i16, kind: u8, size: usize) {
        self.field(id, wire::LIST);
        if size < 15 {
            self.bytes.push((size as u8) << 4 | kind);
        } else {
            self.bytes.push(0xf0 | kind);
            self.varint(size as u64);
        }
    }

    fn element_i32(&mut self, value: i32) {
        self.varint(zigzag(i64::from(value)));
    }

    fn element_binary(&mut self, value: &[u8]) {
        self.varint(value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    /// The finished struct: the file metadata or a page header.
    fn finish(mut self) -> Vec<u8> {
        self.end();
        self.bytes
    }
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// Definition levels of an OPTIONAL leaf, one bit wide, in the RLE/bit-packed
/// hybrid as parquet 55's `RleEncoder` writes them: values wait in groups of
/// eight, a value repeated eight times or more becomes an RLE run, and other
/// groups join a bit-packed run of at most 63 groups, whose header byte is
/// reserved before its first group and written when the run ends.
struct Levels {
    bytes: Vec<u8>,
    buffered: [u8; 8],
    count: usize,
    current: u8,
    repeat: usize,
    packed: usize,
    indicator: Option<usize>,
}

impl Levels {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            buffered: [0; 8],
            count: 0,
            current: 0,
            repeat: 0,
            packed: 0,
            indicator: None,
        }
    }

    fn put(&mut self, value: u8) {
        if self.current == value {
            self.repeat += 1;
            if self.repeat > 8 {
                return;
            }
        } else {
            if self.repeat >= 8 {
                self.flush_rle();
            }
            self.repeat = 1;
            self.current = value;
        }
        self.buffered[self.count] = value;
        self.count += 1;
        if self.count == 8 {
            self.flush_buffered();
        }
    }

    fn varint(&mut self, mut value: usize) {
        while value >= 0x80 {
            self.bytes.push(value as u8 | 0x80);
            value >>= 7;
        }
        self.bytes.push(value as u8);
    }

    fn flush_rle(&mut self) {
        self.varint(self.repeat << 1);
        self.bytes.push(self.current);
        self.count = 0;
        self.repeat = 0;
    }

    /// Write the buffered group, if any, to the bit-packed run, and its header
    /// byte when `end` ends the run.
    fn flush_packed(&mut self, end: bool) {
        let indicator = *self.indicator.get_or_insert_with(|| {
            self.bytes.push(0);
            self.bytes.len() - 1
        });
        if self.count > 0 {
            let byte = (0..self.count).fold(0, |byte, i| byte | self.buffered[i] << i);
            self.bytes.push(byte);
        }
        self.count = 0;
        if end {
            self.bytes[indicator] = ((self.packed / 8) << 1 | 1) as u8;
            self.indicator = None;
            self.packed = 0;
        }
    }

    fn flush_buffered(&mut self) {
        if self.repeat >= 8 {
            self.count = 0;
            if self.packed > 0 {
                self.flush_packed(true);
            }
            return;
        }
        self.packed += self.count;
        self.flush_packed(self.packed / 8 + 1 >= 64);
        self.repeat = 0;
    }

    /// The encoded levels, after their length as four little-endian bytes.
    fn finish(mut self) -> Vec<u8> {
        if self.packed > 0 || self.repeat > 0 || self.count > 0 {
            let all_repeat = self.packed == 0 && (self.repeat == self.count || self.count == 0);
            if self.repeat > 0 && all_repeat {
                self.flush_rle();
            } else {
                if self.count > 0 {
                    self.buffered[self.count..].fill(0);
                    self.count = 8;
                }
                self.packed += self.count;
                self.flush_packed(true);
                self.repeat = 0;
            }
        }
        let mut page = (self.bytes.len() as u32).to_le_bytes().to_vec();
        page.extend(self.bytes);
        page
    }
}

/// The physical type and converted type of a leaf of `kind`.
fn leaf(kind: ColumnType) -> (i32, Option<i32>) {
    match kind {
        ColumnType::Str => (code::BYTE_ARRAY, Some(code::UTF8)),
        ColumnType::Int => (code::INT64, None),
        ColumnType::Float => (code::DOUBLE, None),
        ColumnType::Date => (code::INT32, Some(code::DATE)),
        ColumnType::DateTime => (code::INT64, Some(code::TIMESTAMP_MICROS)),
    }
}

/// The logical type annotation of a leaf of `kind`, if it has one: STRING,
/// DATE, or a TIMESTAMP of microseconds not adjusted to UTC.
fn logical(footer: &mut Compact, kind: ColumnType) {
    let variant = match kind {
        ColumnType::Str => 1,
        ColumnType::Date => 6,
        ColumnType::DateTime => 8,
        ColumnType::Int | ColumnType::Float => return,
    };
    footer.begin(10);
    footer.begin(variant);
    if kind == ColumnType::DateTime {
        footer.bool(1, false);
        footer.begin(2);
        footer.begin(2);
        footer.end();
        footer.end();
    }
    footer.end();
    footer.end();
}

/// One projected column's levels and PLAIN values for the row group being
/// filled.
struct Pending {
    kind: ColumnType,
    levels: Vec<u8>,
    values: Vec<u8>,
}

impl Pending {
    fn new(kind: ColumnType) -> Self {
        Self {
            kind,
            levels: Vec::with_capacity(ROW_GROUP_ROWS),
            values: Vec::new(),
        }
    }

    /// Append one cell: a missing value as a null level.
    fn push<E>(&mut self, value: ValueRef<'_>) -> Result<(), Error<E>> {
        if matches!(value, ValueRef::Missing) {
            self.levels.push(0);
            return Ok(());
        }
        match (self.kind, value) {
            (ColumnType::Str, ValueRef::Str(value)) => {
                let length = u32::try_from(value.len()).map_err(|_| Error::ValueType)?;
                self.values.extend(length.to_le_bytes());
                self.values.extend(value.as_bytes());
            }
            (ColumnType::Int, ValueRef::Int(value)) => self.values.extend(value.to_le_bytes()),
            (ColumnType::Float, ValueRef::Float(value)) => {
                self.values.extend(value.get().to_le_bytes())
            }
            (ColumnType::Date, ValueRef::Date(value)) => {
                self.values.extend(date_days(value).to_le_bytes())
            }
            (ColumnType::DateTime, ValueRef::DateTime(value)) => {
                let micros = datetime_seconds(value)
                    .checked_mul(1_000_000)
                    .ok_or(Error::ValueType)?;
                self.values.extend(micros.to_le_bytes());
            }
            _ => return Err(Error::ValueType),
        }
        self.levels.push(1);
        Ok(())
    }

    /// The column chunk's one data page, its header then its body, emptying
    /// the levels and values for the next row group.
    fn page(&mut self) -> Vec<u8> {
        let mut levels = Levels::new();
        for &level in &self.levels {
            levels.put(level);
        }
        let mut body = levels.finish();
        body.append(&mut self.values);
        // The 1.0 writer cast each size to Thrift's i32.
        let size = body.len() as i32;
        let mut header = Compact::new();
        header.i32(1, code::DATA_PAGE);
        header.i32(2, size);
        header.i32(3, size);
        header.begin(5);
        header.i32(1, self.levels.len() as i32);
        header.i32(2, code::PLAIN);
        header.i32(3, code::RLE);
        header.i32(4, code::RLE);
        header.end();
        let mut page = header.finish();
        page.extend(body);
        self.levels.clear();
        page
    }
}

/// A written column chunk: its first page's offset, its size and its levels.
struct Chunk {
    offset: usize,
    size: usize,
    values: usize,
}

/// A written row group: its column chunks, in projection order, and rows.
struct Group {
    chunks: Vec<Chunk>,
    rows: usize,
}

/// The file being written, within the output byte limit.
struct File {
    bytes: Vec<u8>,
    groups: Vec<Group>,
    /// The size the file would have reached at the write that exceeded the
    /// limit.
    exceeded: Option<Option<usize>>,
}

impl File {
    /// Append `bytes`, or record that they exceed the output byte limit.
    fn write<E>(&mut self, bytes: &[u8]) -> Result<(), Error<E>> {
        let total = self.bytes.len().checked_add(bytes.len());
        if total.is_none_or(|total| total > Limits::OUTPUT_BYTES) {
            self.exceeded = Some(total);
            return Err(Error::Limit {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual: total,
            });
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    /// Write every pending row as one row group, column by column.
    fn row_group<E>(&mut self, pending: &mut [Pending], rows: usize) -> Result<(), Error<E>> {
        if self.groups.len() == ROW_GROUPS {
            return Err(Error::RowGroups);
        }
        let mut chunks = Vec::with_capacity(pending.len());
        for column in pending {
            let values = column.levels.len();
            let page = column.page();
            let offset = self.bytes.len();
            self.write(&page)?;
            chunks.push(Chunk {
                offset,
                size: page.len(),
                values,
            });
        }
        self.groups.push(Group { chunks, rows });
        Ok(())
    }

    /// The footer: the file metadata, its length and the closing magic.
    fn footer(&self, names: &[&str], kinds: &[ColumnType], rows: usize) -> Vec<u8> {
        let mut footer = Compact::new();
        footer.i32(1, 1);
        footer.list(2, wire::STRUCT, names.len() + 1);
        footer.element();
        footer.binary(4, b"schema");
        footer.i32(5, names.len() as i32);
        footer.end();
        for (name, &kind) in names.iter().zip(kinds) {
            let (physical, converted) = leaf(kind);
            footer.element();
            footer.i32(1, physical);
            footer.i32(3, code::OPTIONAL);
            footer.binary(4, name.as_bytes());
            if let Some(converted) = converted {
                footer.i32(6, converted);
            }
            logical(&mut footer, kind);
            footer.end();
        }
        footer.i64(3, rows as i64);
        footer.list(4, wire::STRUCT, self.groups.len());
        for (ordinal, group) in self.groups.iter().enumerate() {
            footer.element();
            footer.list(1, wire::STRUCT, group.chunks.len());
            for ((chunk, name), &kind) in group.chunks.iter().zip(names).zip(kinds) {
                footer.element();
                footer.i64(2, 0);
                footer.begin(3);
                footer.i32(1, leaf(kind).0);
                footer.list(2, wire::I32, 2);
                footer.element_i32(code::PLAIN);
                footer.element_i32(code::RLE);
                footer.list(3, wire::BINARY, 1);
                footer.element_binary(name.as_bytes());
                footer.i32(4, code::UNCOMPRESSED);
                footer.i64(5, chunk.values as i64);
                footer.i64(6, chunk.size as i64);
                footer.i64(7, chunk.size as i64);
                footer.i64(9, chunk.offset as i64);
                // One data page of PLAIN values.
                footer.list(13, wire::STRUCT, 1);
                footer.element();
                footer.i32(1, code::DATA_PAGE);
                footer.i32(2, code::PLAIN);
                footer.i32(3, 1);
                footer.end();
                footer.end();
                footer.end();
            }
            let size: usize = group.chunks.iter().map(|chunk| chunk.size).sum();
            footer.i64(2, size as i64);
            footer.i64(3, group.rows as i64);
            footer.i64(
                5,
                group.chunks.first().map_or(0, |chunk| chunk.offset) as i64,
            );
            footer.i64(6, size as i64);
            footer.i16(7, ordinal as i16);
            footer.end();
        }
        footer.binary(6, CREATED_BY);
        footer.list(7, wire::STRUCT, names.len());
        for _ in names {
            // TYPE_ORDER: an empty TypeDefinedOrder.
            footer.element();
            footer.begin(1);
            footer.end();
            footer.end();
        }
        let mut metadata = footer.finish();
        let length = metadata.len() as u32;
        metadata.extend(length.to_le_bytes());
        metadata.extend(MAGIC);
        metadata
    }
}

/// Encode `partitions` of `table`, one after another, returning where each
/// partition's bytes end. Rows wait in `pending` until a row group is full,
/// and the last group is written only when the file ends, so the row groups
/// are the whole table's whatever the partitions (D264, D160).
fn encode<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[Range<usize>],
    file: &mut File,
) -> Result<Vec<usize>, Error<T::Error>> {
    let columns = table.schema().columns();
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= columns.len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    let names: Vec<&str> = projection
        .iter()
        .map(|&index| columns[index].name.as_str())
        .collect();
    let kinds: Vec<ColumnType> = projection
        .iter()
        .map(|&index| columns[index].kind)
        .collect();
    file.write(MAGIC)?;
    let mut pending: Vec<Pending> = kinds.iter().map(|&kind| Pending::new(kind)).collect();
    let (mut held, mut rows) = (0, 0);
    let mut ends = Vec::with_capacity(partitions.len());
    for partition in partitions {
        Limits::table(partition.len(), projection.len()).map_err(|error| Error::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        })?;
        for row in partition.clone() {
            for (column, &index) in pending.iter_mut().zip(projection) {
                column.push(table.cell(row, index).map_err(Error::Cell)?)?;
            }
            held += 1;
            rows += 1;
            if held == ROW_GROUP_ROWS {
                file.row_group(&mut pending, held)?;
                held = 0;
            }
        }
        ends.push(file.bytes.len());
    }
    if held > 0 {
        file.row_group(&mut pending, held)?;
    }
    let footer = file.footer(&names, &kinds, rows);
    file.write(&footer)?;
    Ok(ends)
}

/// Retain only a complete file. Any error or limit discards all bytes.
/// Zero records still produce a readable schema with every projected field.
/// The last partition's bytes end with the file, its last row group and
/// footer included.
pub(super) fn render_partitions<T: TableAccess>(
    table: &T,
    projection: &[usize],
    partitions: &[Range<usize>],
) -> Result<super::Partitioned, Error<T::Error>> {
    let mut file = File {
        bytes: Vec::new(),
        groups: Vec::new(),
        exceeded: None,
    };
    let result = encode(table, projection, partitions, &mut file);
    if let Some(actual) = file.exceeded {
        return Err(Error::Limit {
            resource: "output_bytes",
            limit: Limits::OUTPUT_BYTES,
            actual,
        });
    }
    let mut ends = result?;
    if let Some(last) = ends.last_mut() {
        *last = file.bytes.len();
    }
    Ok((file.bytes, ends))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every host's default partition (#3794) holds whole row groups, so the
    /// row groups a partitioned write ends are the whole-table write's, and
    /// it is the most of them whose partition of the widest table the cell
    /// limit admits.
    #[test]
    fn the_default_partition_is_the_most_whole_row_groups_the_cell_limit_admits() {
        let rows = Limits::PARTITION_ROWS;
        assert_eq!(
            (
                rows % ROW_GROUP_ROWS,
                Limits::table(rows, Limits::COLUMNS),
                Limits::table(rows + ROW_GROUP_ROWS, Limits::COLUMNS)
                    .map_err(|error| error.resource),
            ),
            (0, Ok(()), Err("table_cells")),
            "the default partition of {rows} rows"
        );
    }
}
