//! The native writer of the Parquet output profile (#3878).
//!
//! A saved file promises its values, not its bytes (D305 on #4184): the
//! layout below may change between releases while every file reads back as
//! PARQUET-4 asks. Each projected column is an OPTIONAL leaf of the type
//! PARQUET-1 maps its column type to. Rows fill row groups of 131,072. A
//! column chunk whose distinct values take at most 1 MiB as PLAIN values is
//! dictionary-encoded: one dictionary page of those values, then data pages
//! of RLE_DICTIONARY indices; any other chunk's data pages hold PLAIN values.
//! A data page is a V1 page of consecutive rows, their RLE definition levels
//! then their values, closed once its rows' PLAIN values reach 1 MiB. Every
//! page is compressed with Zstandard. There are no statistics, page index or
//! key-value metadata.
//!
//! The footer and page headers are Thrift compact structs (`Compact`), and the
//! definition levels and dictionary indices are the RLE/bit-packed hybrid
//! (`hybrid`).
use super::Error;
use crate::epoch::{date_days, datetime_seconds};
use std::collections::HashMap;
use std::ops::Range;
use yamaa_core::{
    limits::Limits,
    table::{ColumnReader, TableAccess, ValueRef},
    value::ColumnType,
};

/// The rows of a full row group.
const ROW_GROUP_ROWS: usize = 1 << 17;

/// The PLAIN values at which a data page closes, and the most a column
/// chunk's dictionary holds as PLAIN values.
const PAGE_BYTES: usize = 1 << 20;

/// The Zstandard level of every page.
const ZSTD_LEVEL: i32 = 1;

/// The writer that the footer names.
const CREATED_BY: &str = concat!("yamaa version ", env!("CARGO_PKG_VERSION"));

/// The magic bytes that begin and end a Parquet file.
const MAGIC: &[u8] = b"PAR1";

/// A page's size as the Thrift i32 its header holds, or the refusal of a
/// body larger than an i32 counts (#4075).
fn page_size<E>(body: usize) -> Result<i32, Error<E>> {
    i32::try_from(body).map_err(|_| Error::Limit {
        resource: "parquet_page_bytes",
        limit: i32::MAX as usize,
        actual: Some(body),
    })
}

/// A plain string's length as the four bytes that precede it, or the page's
/// refusal of a value longer than a u32 counts: that value alone makes its
/// page one no i32 counts, so it is refused as that page is (#4075).
fn value_length<E>(value: usize) -> Result<u32, Error<E>> {
    u32::try_from(value).or_else(|_| page_size(value).map(|size| size as u32))
}

/// The footer's length as the four bytes that end it, or the refusal of
/// metadata longer than a u32 counts: each row group repeats every field's
/// name, so long names over many row groups can pass it within the output
/// byte limit (#4075).
fn footer_length<E>(metadata: usize) -> Result<u32, Error<E>> {
    u32::try_from(metadata).map_err(|_| Error::Limit {
        resource: "parquet_footer_bytes",
        limit: u32::MAX as usize,
        actual: Some(metadata),
    })
}

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
    pub const RLE_DICTIONARY: i32 = 8;
    pub const ZSTD: i32 = 6;
    pub const DATA_PAGE: i32 = 0;
    pub const DICTIONARY_PAGE: i32 = 2;
}

/// Thrift compact wire types.
mod wire {
    pub const TRUE: u8 = 1;
    pub const FALSE: u8 = 2;
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
            varint(&mut self.bytes, zigzag(i64::from(id)));
        }
    }

    fn i32(&mut self, id: i16, value: i32) {
        self.field(id, wire::I32);
        varint(&mut self.bytes, zigzag(i64::from(value)));
    }

    fn i64(&mut self, id: i16, value: i64) {
        self.field(id, wire::I64);
        varint(&mut self.bytes, zigzag(value));
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
            varint(&mut self.bytes, size as u64);
        }
    }

    fn element_i32(&mut self, value: i32) {
        varint(&mut self.bytes, zigzag(i64::from(value)));
    }

    fn element_binary(&mut self, value: &[u8]) {
        varint(&mut self.bytes, value.len() as u64);
        self.bytes.extend_from_slice(value);
    }

    /// The finished struct: the file metadata or a page header.
    fn finish(mut self) -> Vec<u8> {
        self.end();
        self.bytes
    }
}

/// An unsigned LEB128 varint, as Thrift's compact protocol and the hybrid's
/// run headers write one.
fn varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push(value as u8 | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// The values from `start` that equal the value at `start`.
fn run<T: PartialEq>(values: &[T], start: usize) -> usize {
    values[start..]
        .iter()
        .take_while(|&value| *value == values[start])
        .count()
}

/// `values`, each of `width` bits (1 to 32), in the RLE/bit-packed hybrid:
/// a value repeated eight times or more from where a run may begin is an RLE
/// run, and the values between such runs are bit-packed eight to a group,
/// only the last group of all padded with zeros.
fn hybrid<T: Copy + PartialEq + Into<u32>>(values: &[T], width: u32, bytes: &mut Vec<u8>) {
    let mut start = 0;
    while start < values.len() {
        let repeated = run(values, start);
        if repeated >= 8 {
            varint(bytes, (repeated as u64) << 1);
            let value = values[start].into().to_le_bytes();
            bytes.extend_from_slice(&value[..width.div_ceil(8) as usize]);
            start += repeated;
            continue;
        }
        let mut end = start;
        loop {
            end += 8;
            if end >= values.len() {
                end = values.len();
                break;
            }
            if run(values, end) >= 8 {
                break;
            }
        }
        let groups = (end - start).div_ceil(8);
        varint(bytes, (groups as u64) << 1 | 1);
        let (mut buffer, mut bits) = (0u64, 0);
        for index in start..start + groups * 8 {
            let value = values.get(index).map_or(0, |&value| value.into());
            buffer |= u64::from(value) << bits;
            bits += width;
            while bits >= 8 {
                bytes.push(buffer as u8);
                buffer >>= 8;
                bits -= 8;
            }
        }
        start = end;
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

/// The dictionary's hash of a present value: aHash under fixed seeds, as
/// grouping hashes rows (#4188). It only finds a value's candidate entry;
/// equal values decide, and a `DOUBLE` is keyed on its bits, as its PLAIN
/// bytes are, and entries follow first occurrence, so no output depends on
/// the hasher.
fn value_hasher() -> ahash::RandomState {
    ahash::RandomState::with_seeds(
        0x243f_6a88_85a3_08d3,
        0x1319_8a2e_0370_7344,
        0xa409_3822_299f_31d0,
        0x082e_fa98_ec4e_6c89,
    )
}

/// A value whose PLAIN bytes a page holds: a string with the four length
/// bytes before it, or a fixed-width value's little-endian bytes.
trait Plain: Copy + Eq + std::hash::Hash {
    fn width(self) -> usize;
    fn write(self, bytes: &mut Vec<u8>);
}

impl Plain for &str {
    fn width(self) -> usize {
        4 + self.len()
    }
    fn write(self, bytes: &mut Vec<u8>) {
        // Admitted by `value_length` as the value was read.
        bytes.extend((self.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.as_bytes());
    }
}

/// An `INT64` or `DOUBLE`, or a timestamp's microseconds, as its 8 bytes.
impl Plain for u64 {
    fn width(self) -> usize {
        8
    }
    fn write(self, bytes: &mut Vec<u8>) {
        bytes.extend(self.to_le_bytes());
    }
}

/// A `DATE`'s days as its 4 bytes.
impl Plain for u32 {
    fn width(self) -> usize {
        4
    }
    fn write(self, bytes: &mut Vec<u8>) {
        bytes.extend(self.to_le_bytes());
    }
}

/// Where a closed data page of a column chunk ends: after its rows and its
/// present values.
#[derive(Clone, Copy, Default)]
struct Bound {
    rows: usize,
    values: usize,
}

/// One column chunk's rows, read from its column: each row's definition
/// level, its present values, and the bounds of its data pages, each closed
/// once its rows' PLAIN values reach `PAGE_BYTES`, the last at the chunk's
/// end.
struct Read<V> {
    levels: Vec<u8>,
    values: Vec<V>,
    pages: Vec<Bound>,
}

impl<V: Plain> Read<V> {
    /// The rows of `segments`, in order, of a column whose present values
    /// `present` turns into PLAIN values, or refuses as of another type.
    fn rows<'a, T: TableAccess + ?Sized>(
        reader: &ColumnReader<'a, T>,
        segments: &[Range<usize>],
        mut present: impl FnMut(ValueRef<'a>) -> Result<V, Error<T::Error>>,
    ) -> Result<Self, Error<T::Error>> {
        let rows = segments.iter().map(ExactSizeIterator::len).sum();
        let mut read = Self {
            levels: Vec::with_capacity(rows),
            values: Vec::with_capacity(rows),
            pages: Vec::new(),
        };
        let mut open = 0;
        for segment in segments {
            let visit = |value| {
                if matches!(value, ValueRef::Missing) {
                    read.levels.push(0);
                    return Ok(());
                }
                let value = present(value)?;
                read.levels.push(1);
                read.values.push(value);
                open += value.width();
                if open >= PAGE_BYTES {
                    read.close();
                    open = 0;
                }
                Ok(())
            };
            reader.try_for_each(segment.clone(), visit, Error::Cell)?;
        }
        if read.levels.len() > read.closed().rows || read.pages.is_empty() {
            read.close();
        }
        Ok(read)
    }

    /// The end of the last closed page, or the chunk's start.
    fn closed(&self) -> Bound {
        self.pages.last().copied().unwrap_or_default()
    }

    /// Close the open page at the rows read so far.
    fn close(&mut self) {
        self.pages.push(Bound {
            rows: self.levels.len(),
            values: self.values.len(),
        });
    }

    /// The chunk's dictionary, unless it has no present value or its PLAIN
    /// values pass `PAGE_BYTES`. A value equal to the one before it takes
    /// its index without a lookup.
    fn dictionary(&self) -> Option<Dictionary> {
        let first = *self.values.first()?;
        let mut entries: HashMap<V, u32, _> = HashMap::with_hasher(value_hasher());
        let mut page = Vec::new();
        let mut indices = Vec::with_capacity(self.values.len());
        let mut last = (first, 0);
        first.write(&mut page);
        if page.len() > PAGE_BYTES {
            return None;
        }
        entries.insert(first, 0);
        for &value in &self.values {
            if value != last.0 {
                // At most ROW_GROUP_ROWS entries.
                let next = entries.len() as u32;
                let index = *entries.entry(value).or_insert_with(|| {
                    value.write(&mut page);
                    next
                });
                if page.len() > PAGE_BYTES {
                    return None;
                }
                last = (value, index);
            }
            indices.push(last.1);
        }
        Some(Dictionary {
            page,
            entries: entries.len(),
            indices,
        })
    }

    /// The column chunk's pages, or the refusal of a page its header cannot
    /// count.
    fn chunk<E>(&self, compressor: &mut Compressor) -> Result<Encoded, Error<E>> {
        let dictionary = self.dictionary();
        let mut encoded = Encoded {
            dictionary: None,
            data: Vec::new(),
            pages: self.pages.len(),
            uncompressed: 0,
        };
        // Bit width of the largest index, at least one.
        let width = dictionary
            .as_ref()
            .map(|dictionary| (u32::BITS - (dictionary.entries as u32 - 1).leading_zeros()).max(1));
        if let Some(dictionary) = &dictionary {
            let mut header = Compact::new();
            header.i32(1, code::DICTIONARY_PAGE);
            let (page, uncompressed) = compressor.page(header, &dictionary.page, |header| {
                header.begin(7);
                // At most ROW_GROUP_ROWS entries.
                header.i32(1, dictionary.entries as i32);
                header.i32(2, code::PLAIN);
                header.end();
            })?;
            encoded.uncompressed += uncompressed;
            encoded.dictionary = Some(page);
        }
        let mut start = Bound::default();
        let mut body = Vec::new();
        for &end in &self.pages {
            body.clear();
            body.extend([0; 4]);
            hybrid(&self.levels[start.rows..end.rows], 1, &mut body);
            // At most ROW_GROUP_ROWS levels.
            let levels = (body.len() - 4) as u32;
            body[..4].copy_from_slice(&levels.to_le_bytes());
            match (&dictionary, width) {
                (Some(dictionary), Some(width)) => {
                    body.push(width as u8);
                    hybrid(
                        &dictionary.indices[start.values..end.values],
                        width,
                        &mut body,
                    );
                }
                _ => {
                    for &value in &self.values[start.values..end.values] {
                        value.write(&mut body);
                    }
                }
            }
            let mut header = Compact::new();
            header.i32(1, code::DATA_PAGE);
            let rows = end.rows - start.rows;
            let encoding = match dictionary {
                Some(_) => code::RLE_DICTIONARY,
                None => code::PLAIN,
            };
            let (page, uncompressed) = compressor.page(header, &body, |header| {
                header.begin(5);
                // At most ROW_GROUP_ROWS levels.
                header.i32(1, rows as i32);
                header.i32(2, encoding);
                header.i32(3, code::RLE);
                header.i32(4, code::RLE);
                header.end();
            })?;
            encoded.uncompressed += uncompressed;
            encoded.data.extend(page);
            start = end;
        }
        Ok(encoded)
    }
}

/// The pages of one row group's column chunk of `kind`, read from `reader`
/// over the rows of `segments`, with its levels: each present value as the
/// PLAIN value of the type PARQUET-1 maps `kind` to.
fn column_chunk<T: TableAccess + ?Sized>(
    kind: ColumnType,
    reader: &ColumnReader<'_, T>,
    segments: &[Range<usize>],
    compressor: &mut Compressor,
) -> Result<(Encoded, usize), Error<T::Error>> {
    fn encode<V: Plain, E>(
        read: Read<V>,
        compressor: &mut Compressor,
    ) -> Result<(Encoded, usize), Error<E>> {
        Ok((read.chunk(compressor)?, read.levels.len()))
    }
    match kind {
        ColumnType::Str => encode(
            Read::rows(reader, segments, |value| match value {
                ValueRef::Str(value) => value_length(value.len()).map(|_| value),
                _ => Err(Error::ValueType),
            })?,
            compressor,
        ),
        ColumnType::Int => encode(
            Read::rows(reader, segments, |value| match value {
                ValueRef::Int(value) => Ok(value as u64),
                _ => Err(Error::ValueType),
            })?,
            compressor,
        ),
        ColumnType::Float => encode(
            Read::rows(reader, segments, |value| match value {
                ValueRef::Float(value) => Ok(value.get().to_bits()),
                _ => Err(Error::ValueType),
            })?,
            compressor,
        ),
        ColumnType::Date => encode(
            Read::rows(reader, segments, |value| match value {
                ValueRef::Date(value) => Ok(date_days(value) as u32),
                _ => Err(Error::ValueType),
            })?,
            compressor,
        ),
        ColumnType::DateTime => encode(
            Read::rows(reader, segments, |value| match value {
                ValueRef::DateTime(value) => datetime_seconds(value)
                    .checked_mul(1_000_000)
                    .map(|micros| micros as u64)
                    .ok_or(Error::ValueType),
                _ => Err(Error::ValueType),
            })?,
            compressor,
        ),
    }
}

/// A column chunk's dictionary: its PLAIN values, their count, and each
/// present value's index into it.
struct Dictionary {
    page: Vec<u8>,
    entries: usize,
    indices: Vec<u32>,
}

/// A column chunk's encoded pages: its dictionary page if it has one, its
/// data pages one after another, how many data pages, and the bytes all its
/// pages would take uncompressed, headers included.
struct Encoded {
    dictionary: Option<Vec<u8>>,
    data: Vec<u8>,
    pages: usize,
    uncompressed: usize,
}

/// The Zstandard compressor every page of a file goes through.
struct Compressor(zstd::bulk::Compressor<'static>);

impl Compressor {
    /// A page of `body` after its header: `header` holds the page type, then
    /// the sizes, then the fields `rest` writes. Returns the page and the
    /// bytes it would take uncompressed, or the refusal of a body its header
    /// cannot count.
    fn page<E>(
        &mut self,
        mut header: Compact,
        body: &[u8],
        rest: impl FnOnce(&mut Compact),
    ) -> Result<(Vec<u8>, usize), Error<E>> {
        let uncompressed = page_size(body.len())?;
        let compressed = self.0.compress(body).map_err(|_| Error::Compression)?;
        header.i32(2, uncompressed);
        header.i32(3, page_size(compressed.len())?);
        rest(&mut header);
        let mut page = header.finish();
        let size = page.len() + body.len();
        page.extend(compressed);
        Ok((page, size))
    }
}

/// A written column chunk: where its dictionary page, if any, and its first
/// data page begin, its bytes compressed and not, its levels and data pages.
struct Chunk {
    dictionary: Option<usize>,
    data: usize,
    compressed: usize,
    uncompressed: usize,
    values: usize,
    pages: usize,
}

impl Chunk {
    /// The offset of the chunk's first page.
    fn start(&self) -> usize {
        self.dictionary.unwrap_or(self.data)
    }
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
    compressor: Option<Compressor>,
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

    /// Write the `rows` rows of `segments` as one row group, a column at a
    /// time: each of `readers`, of the column type of `kinds` beside it.
    fn row_group<T: TableAccess + ?Sized>(
        &mut self,
        readers: &[ColumnReader<'_, T>],
        kinds: &[ColumnType],
        segments: &[Range<usize>],
        rows: usize,
    ) -> Result<(), Error<T::Error>> {
        let mut compressor = match self.compressor.take() {
            Some(compressor) => compressor,
            None => {
                Compressor(zstd::bulk::Compressor::new(ZSTD_LEVEL).map_err(|_| Error::Compression)?)
            }
        };
        let mut chunks = Vec::with_capacity(readers.len());
        for (reader, &kind) in readers.iter().zip(kinds) {
            let (encoded, values) = column_chunk(kind, reader, segments, &mut compressor)?;
            let start = self.bytes.len();
            let dictionary = match &encoded.dictionary {
                Some(page) => {
                    self.write(page)?;
                    Some(start)
                }
                None => None,
            };
            let data = self.bytes.len();
            self.write(&encoded.data)?;
            chunks.push(Chunk {
                dictionary,
                data,
                compressed: self.bytes.len() - start,
                uncompressed: encoded.uncompressed,
                values,
                pages: encoded.pages,
            });
        }
        self.compressor = Some(compressor);
        self.groups.push(Group { chunks, rows });
        Ok(())
    }

    /// The footer: the file metadata, its length and the closing magic, or
    /// the refusal of metadata its length cannot count.
    fn footer<E>(
        &self,
        names: &[&str],
        kinds: &[ColumnType],
        rows: usize,
    ) -> Result<Vec<u8>, Error<E>> {
        let mut footer = Compact::new();
        footer.i32(1, 1);
        footer.list(2, wire::STRUCT, names.len() + 1);
        footer.element();
        footer.binary(4, b"schema");
        // At most Limits::COLUMNS fields, which encode admits.
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
        for group in &self.groups {
            footer.element();
            footer.list(1, wire::STRUCT, group.chunks.len());
            for ((chunk, name), &kind) in group.chunks.iter().zip(names).zip(kinds) {
                footer.element();
                footer.i64(2, 0);
                footer.begin(3);
                footer.i32(1, leaf(kind).0);
                let encodings: &[i32] = match chunk.dictionary {
                    Some(_) => &[code::PLAIN, code::RLE, code::RLE_DICTIONARY],
                    None => &[code::PLAIN, code::RLE],
                };
                footer.list(2, wire::I32, encodings.len());
                for &encoding in encodings {
                    footer.element_i32(encoding);
                }
                footer.list(3, wire::BINARY, 1);
                footer.element_binary(name.as_bytes());
                footer.i32(4, code::ZSTD);
                footer.i64(5, chunk.values as i64);
                footer.i64(6, chunk.uncompressed as i64);
                footer.i64(7, chunk.compressed as i64);
                footer.i64(9, chunk.data as i64);
                if let Some(offset) = chunk.dictionary {
                    footer.i64(11, offset as i64);
                }
                // Each page type and encoding, with its pages.
                let statistics: &[(i32, i32, usize)] = match chunk.dictionary {
                    Some(_) => &[
                        (code::DICTIONARY_PAGE, code::PLAIN, 1),
                        (code::DATA_PAGE, code::RLE_DICTIONARY, chunk.pages),
                    ],
                    None => &[(code::DATA_PAGE, code::PLAIN, chunk.pages)],
                };
                footer.list(13, wire::STRUCT, statistics.len());
                for &(page, encoding, count) in statistics {
                    footer.element();
                    footer.i32(1, page);
                    footer.i32(2, encoding);
                    // At most one page per row.
                    footer.i32(3, count as i32);
                    footer.end();
                }
                footer.end();
                footer.end();
            }
            let uncompressed: usize = group.chunks.iter().map(|chunk| chunk.uncompressed).sum();
            let compressed: usize = group.chunks.iter().map(|chunk| chunk.compressed).sum();
            footer.i64(2, uncompressed as i64);
            footer.i64(3, group.rows as i64);
            footer.i64(5, group.chunks.first().map_or(0, Chunk::start) as i64);
            footer.i64(6, compressed as i64);
            footer.end();
        }
        footer.binary(6, CREATED_BY.as_bytes());
        footer.list(7, wire::STRUCT, names.len());
        for _ in names {
            // TYPE_ORDER: an empty TypeDefinedOrder.
            footer.element();
            footer.begin(1);
            footer.end();
            footer.end();
        }
        let mut metadata = footer.finish();
        let length = footer_length(metadata.len())?;
        metadata.extend(length.to_le_bytes());
        metadata.extend(MAGIC);
        Ok(metadata)
    }
}

/// Encode `partitions` of `table`, one after another, returning where each
/// partition's bytes end. Rows wait until a row group is full, and the last
/// group is written only when the file ends, so the row groups are the whole
/// table's whatever the partitions (D264). A row group is encoded a column at
/// a time, each column read over the group's rows, which may span partitions.
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
    let admit = |rows| -> Result<(), Error<T::Error>> {
        Limits::table(rows, projection.len()).map_err(|error| Error::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        })
    };
    // The columns, which the footer counts in an i32, whatever the partitions.
    admit(0)?;
    let names: Vec<&str> = projection
        .iter()
        .map(|&index| columns[index].name.as_str())
        .collect();
    let kinds: Vec<ColumnType> = projection
        .iter()
        .map(|&index| columns[index].kind)
        .collect();
    let readers: Vec<_> = projection
        .iter()
        .map(|&index| ColumnReader::new(table, index))
        .collect();
    file.write(MAGIC)?;
    // The row ranges the row group being filled holds, and their rows.
    let (mut held, mut segments) = (0, Vec::new());
    let mut rows = 0;
    let mut ends = Vec::with_capacity(partitions.len());
    for partition in partitions {
        admit(partition.len())?;
        let mut start = partition.start;
        while start < partition.end {
            let end = partition.end.min(start + (ROW_GROUP_ROWS - held));
            segments.push(start..end);
            held += end - start;
            rows += end - start;
            start = end;
            if held == ROW_GROUP_ROWS {
                file.row_group(&readers, &kinds, &segments, held)?;
                (held, segments) = (0, Vec::new());
            }
        }
        ends.push(file.bytes.len());
    }
    if held > 0 {
        file.row_group(&readers, &kinds, &segments, held)?;
    }
    let footer = file.footer(&names, &kinds, rows)?;
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
        compressor: None,
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

    /// Seam: the size checks of a page, a footer and a string value, at
    /// lengths no test could build. A page header counts its body in a
    /// Thrift i32 and the file's tail counts the footer in four unsigned
    /// bytes, so each admits its field's largest value and refuses the next
    /// byte; a string past its four length bytes is refused as its page
    /// (#4075).
    #[test]
    fn a_page_or_footer_its_size_field_cannot_count_is_refused() {
        use std::convert::Infallible;
        assert_eq!(
            (
                page_size::<Infallible>(2_147_483_647),
                page_size::<Infallible>(2_147_483_648),
                footer_length::<Infallible>(4_294_967_295),
                footer_length::<Infallible>(4_294_967_296),
                value_length::<Infallible>(4_294_967_295),
                value_length::<Infallible>(4_294_967_296),
            ),
            (
                Ok(2_147_483_647),
                Err(Error::Limit {
                    resource: "parquet_page_bytes",
                    limit: 2_147_483_647,
                    actual: Some(2_147_483_648),
                }),
                Ok(4_294_967_295),
                Err(Error::Limit {
                    resource: "parquet_footer_bytes",
                    limit: 4_294_967_295,
                    actual: Some(4_294_967_296),
                }),
                Ok(4_294_967_295),
                Err(Error::Limit {
                    resource: "parquet_page_bytes",
                    limit: 2_147_483_647,
                    actual: Some(4_294_967_296),
                }),
            ),
            "a page, a footer and a string value at their size fields' largest value and one byte past it"
        );
    }

    /// The hybrid's runs, written by hand from the format's definition
    /// (Parquet's Encodings.md, "Run Length Encoding / Bit-Packing Hybrid"):
    /// an RLE run's header is its length shifted left once, then the value in
    /// the bytes its width needs; a bit-packed run's header is its groups of
    /// eight shifted left once with the low bit set, then each group's values
    /// packed from the least significant bit, the last group padded.
    #[test]
    fn the_hybrid_writes_runs_of_eight_as_rle_and_the_rest_bit_packed() {
        let cases: [(&[u32], u32, &[u8], &str); 5] = [
            (&[1; 10], 1, &[20, 1], "ten ones, one bit wide"),
            (
                &[5; 8],
                9,
                &[16, 5, 0],
                "eight fives, nine bits wide, in two bytes",
            ),
            (
                &[0, 1, 0, 1, 1],
                1,
                &[3, 0b0001_1010],
                "five values, one padded group",
            ),
            (
                &[1, 2, 3, 0, 1, 2, 3, 0, 7, 7, 7, 7, 7, 7, 7, 7, 7],
                2,
                &[3, 0b0011_1001, 0b0011_1001, 18, 7],
                "a packed group, then nine sevens",
            ),
            (&[], 3, &[], "no values"),
        ];
        for (values, width, expected, label) in cases {
            let mut bytes = Vec::new();
            hybrid(values, width, &mut bytes);
            assert_eq!(bytes, expected, "{label}");
        }
    }
}
