//! The native column reader (#3878): each admitted column chunk's pages,
//! read to their values as parquet 55.2.0's column reader read them, so that
//! a file reads to the same values, or is refused, as it was then.
//!
//! Admission (`framing`, `compression`, `payload`) has decoded and checked
//! every page by the time a chunk is read here. What is left is what the
//! column reader itself did with them:
//!
//! - It reads a chunk [`BATCH`] records at a time and loads a page only when
//!   it needs the next one. A data page of no values ends the read that
//!   loads it, so one that a read loads first ends the chunk there, and the
//!   chunk is refused for its missing records.
//! - Loading a data page checks its encodings, a version 2 page's row count,
//!   and its statistics, as the footer's are checked (`footer::statistics`).
//!   Its value encoding must be one its physical type decodes
//!   (`closed_profile::decodes`), and a dictionary-encoded page needs the
//!   chunk's dictionary. Each RLE decoder reads its first run when it is
//!   given its bytes, and the repetition levels' decoder reads ahead
//!   [`LEVELS`] levels at a time.
//! - A repeated column's data page reads the next page header to tell
//!   whether it ends a record. That read takes an index page's body for a
//!   page header, which no index page's body is, so an index page with a
//!   body after a repeated column's data page is refused.
//! - Reading a page's values checks what admission leaves to the decoders:
//!   RLE booleans, byte stream split values and fixed-length values of no
//!   length.
//!
//! A chunk is refused, as `Error::Malformed`, wherever the column reader
//! failed or panicked, and its values are those the column reader gave.
use super::closed_profile;
use super::compact::{self, Value};
use super::compression;
use super::footer::{Footer, Leaf};
use super::format::{Codec, Encoding, Physical};
use super::framing::{Page, Payload};
use super::payload;
use super::reader::Error;
use super::{delta, rle, shape};

/// The records one read of the column reader asks for.
const BATCH: usize = 256;
/// The repetition levels its decoder reads at a time.
const LEVELS: usize = 1024;

/// One leaf column's stored values, or `Discard` for a column the table
/// does not keep, whose pages are read and their values dropped.
#[derive(Debug, PartialEq)]
pub(super) enum Column {
    Int32(Vec<Option<i32>>),
    Int64(Vec<Option<i64>>),
    Float(Vec<Option<f32>>),
    Double(Vec<Option<f64>>),
    Bytes(Binary),
    Discard,
}

impl Column {
    /// An empty column of the values `leaf` stores, which a kept column's
    /// physical type is.
    fn new(leaf: &Leaf) -> Result<Self, Error> {
        Ok(match leaf.physical {
            Physical::Int32 => Self::Int32(Vec::new()),
            Physical::Int64 => Self::Int64(Vec::new()),
            Physical::Float => Self::Float(Vec::new()),
            Physical::Double => Self::Double(Vec::new()),
            Physical::ByteArray => Self::Bytes(Binary::default()),
            _ => return Err(Error::Malformed),
        })
    }
}

/// Byte array values: their bytes, end to end, and each value's range in
/// them, or none for a missing value.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Binary {
    data: Vec<u8>,
    values: Vec<Option<(usize, usize)>>,
}

impl Binary {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Each value's bytes, in order.
    pub fn iter(&self) -> impl Iterator<Item = Option<&[u8]>> {
        (0..self.values.len()).map(|index| self.get(index))
    }

    fn get(&self, index: usize) -> Option<&[u8]> {
        self.values[index].map(|(start, end)| &self.data[start..end])
    }

    pub(super) fn push(&mut self, value: Option<&[u8]>) {
        self.values.push(value.map(|bytes| {
            let start = self.data.len();
            self.data.extend_from_slice(bytes);
            (start, self.data.len())
        }));
    }
}

/// Present values of one page, a dictionary or a chunk: a kept column's, as
/// each fixed-width value's little-endian bytes or as byte arrays, or only
/// how many there are, for a physical type that no kept column holds.
#[derive(Debug)]
enum Values {
    Fixed { width: usize, bytes: Vec<u8> },
    Bytes(Binary),
    Count(usize),
}

impl Values {
    fn len(&self) -> usize {
        match self {
            Self::Fixed { width, bytes } => bytes.len() / width,
            Self::Bytes(values) => values.len(),
            Self::Count(count) => *count,
        }
    }

    /// No values of the physical type of `leaf`.
    fn empty(leaf: &Leaf) -> Self {
        match leaf.physical {
            Physical::Int32 | Physical::Int64 | Physical::Float | Physical::Double => Self::Fixed {
                width: width(leaf),
                bytes: Vec::new(),
            },
            Physical::ByteArray => Self::Bytes(Binary::default()),
            _ => Self::Count(0),
        }
    }

    /// Append `count` copies of value `index` of `from`, of the same kind.
    fn repeat(&mut self, from: &Values, index: usize, count: usize) {
        match (self, from) {
            (Self::Fixed { width, bytes }, Self::Fixed { bytes: source, .. }) => {
                let value = &source[index * *width..(index + 1) * *width];
                for _ in 0..count {
                    bytes.extend_from_slice(value);
                }
            }
            (Self::Bytes(values), Self::Bytes(source)) => {
                for _ in 0..count {
                    values.push(source.get(index));
                }
            }
            (Self::Count(total), _) => *total += count,
            _ => unreachable!("values of one physical type"),
        }
    }

    /// Append `other`, of the same kind.
    fn append(&mut self, other: Values) {
        match (self, other) {
            (Self::Fixed { bytes, .. }, Self::Fixed { bytes: more, .. }) => {
                bytes.extend_from_slice(&more);
            }
            (Self::Bytes(values), Self::Bytes(more)) => {
                for value in more.iter() {
                    values.push(value);
                }
            }
            (Self::Count(total), other) => *total += other.len(),
            _ => unreachable!("values of one physical type"),
        }
    }
}

/// The bytes of one value of `leaf`'s physical type, when it has a fixed
/// width; a fixed-length type of a negative length, which the footer
/// refuses, has none.
fn width(leaf: &Leaf) -> usize {
    match leaf.physical {
        Physical::Int32 | Physical::Float => 4,
        Physical::Int64 | Physical::Double => 8,
        Physical::Int96 => 12,
        Physical::FixedLenByteArray => usize::try_from(leaf.type_length).unwrap_or(0),
        Physical::Boolean | Physical::ByteArray => 0,
    }
}

/// Read the stored columns of the row groups `groups` of the admitted file
/// `file`, whose footer is `footer` and whose pages framing admitted, in
/// file order, as `pages`. A column `keep` marks keeps its values; every
/// column is read. Each fixed-size list of the Arrow schema `arrow` checks
/// its rows' sizes from the levels (`shape`).
pub(super) fn read(
    file: &[u8],
    pages: &[Page],
    footer: &Footer,
    keep: &[bool],
    arrow: &arrow_schema::Schema,
    groups: std::ops::Range<usize>,
) -> Result<Vec<Column>, Error> {
    let leaves = &footer.schema.leaves;
    let shapes = shape::constraints(arrow, &footer.schema)?;
    if keep.len() != leaves.len() {
        return Err(Error::Malformed);
    }
    let mut result: Vec<_> = leaves.iter().map(|_| Column::Discard).collect();
    let first = groups.start;
    let row_groups = footer.row_groups.get(groups).ok_or(Error::Malformed)?;
    for (offset, group) in row_groups.iter().enumerate() {
        let row_group = first + offset;
        let rows = usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?;
        for (index, leaf) in leaves.iter().enumerate() {
            let chunk = group.columns.get(index).ok_or(Error::Malformed)?;
            // The column reader makes a decompressor for the chunk's codec
            // before it reads a page, and has none for LZO.
            if chunk.codec == Codec::Lzo {
                return Err(Error::Malformed);
            }
            let levels = usize::try_from(chunk.num_values).map_err(|_| Error::Malformed)?;
            let at = (row_group, index);
            let start = pages.partition_point(|page| (page.row_group, page.column) < at);
            let end = pages.partition_point(|page| (page.row_group, page.column) <= at);
            let mut chunk = Chunk::new(file, &pages[start..end], leaf);
            let mut shapes = shapes[index].clone();
            let present = chunk.collect((rows, levels), &mut shapes, keep[index])?;
            if keep[index] {
                if matches!(result[index], Column::Discard) {
                    result[index] = Column::new(leaf)?;
                }
                append(&mut result[index], &present, chunk.values)?;
            }
        }
    }
    // A file of no rows may hold no row groups; its kept columns keep their
    // physical type.
    for (index, leaf) in leaves.iter().enumerate() {
        if keep[index] && matches!(result[index], Column::Discard) {
            result[index] = Column::new(leaf)?;
        }
    }
    Ok(result)
}

/// Append one chunk's values to the kept column read so far: a value for
/// each level `present` marks, in order, and a missing value for the others.
fn append(column: &mut Column, present: &[bool], values: Values) -> Result<(), Error> {
    if present.iter().filter(|&&present| present).count() != values.len() {
        return Err(Error::Malformed);
    }
    let mut next = 0;
    let mut take = |present: bool| {
        present.then(|| {
            next += 1;
            next - 1
        })
    };
    match (column, &values) {
        (Column::Int32(out), Values::Fixed { width: 4, bytes }) => {
            out.extend(present.iter().map(|&present| {
                take(present)
                    .map(|i| i32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()))
            }));
        }
        (Column::Int64(out), Values::Fixed { width: 8, bytes }) => {
            out.extend(present.iter().map(|&present| {
                take(present)
                    .map(|i| i64::from_le_bytes(bytes[8 * i..8 * i + 8].try_into().unwrap()))
            }));
        }
        (Column::Float(out), Values::Fixed { width: 4, bytes }) => {
            out.extend(present.iter().map(|&present| {
                take(present)
                    .map(|i| f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()))
            }));
        }
        (Column::Double(out), Values::Fixed { width: 8, bytes }) => {
            out.extend(present.iter().map(|&present| {
                take(present)
                    .map(|i| f64::from_le_bytes(bytes[8 * i..8 * i + 8].try_into().unwrap()))
            }));
        }
        (Column::Bytes(out), Values::Bytes(binary)) => {
            for &present in present {
                out.push(take(present).and_then(|i| binary.get(i)));
            }
        }
        _ => return Err(Error::Malformed),
    }
    Ok(())
}

/// A data page the column reader has loaded: its levels, its values, and
/// how far it has read them.
struct Loaded {
    levels: usize,
    repetition: Vec<i16>,
    definition: Vec<i16>,
    /// RLE booleans, which the reader decodes as it reads them, and how many
    /// the decoder was told the page holds; every other page's values are
    /// decoded when it loads (`Chunk::values`).
    booleans: Option<(Rle<Box<[u8]>>, usize)>,
    /// Whether the page ends a record: it is the chunk's last.
    delimiter: bool,
    /// The levels read.
    read: usize,
}

/// One column chunk as the column reader reads it.
struct Chunk<'a> {
    file: &'a [u8],
    pages: &'a [Page],
    leaf: &'a Leaf,
    /// The next page to load.
    next: usize,
    dictionary: Option<Values>,
    page: Option<Loaded>,
    /// The present values of every page loaded.
    values: Values,
    /// Whether a record is in progress (the reader's `has_partial`).
    partial: bool,
}

impl<'a> Chunk<'a> {
    fn new(file: &'a [u8], pages: &'a [Page], leaf: &'a Leaf) -> Self {
        Self {
            file,
            pages,
            leaf,
            next: 0,
            dictionary: None,
            page: None,
            values: Values::empty(leaf),
            partial: false,
        }
    }

    /// Read the chunk whole, as the old reader did: reads of [`BATCH`]
    /// records until one reads nothing, then `rows` records and `levels`
    /// levels in all, each level observed by the fixed-size lists `shapes`.
    /// With `keep`, return whether each level holds a value.
    fn collect(
        &mut self,
        (rows, levels): (usize, usize),
        shapes: &mut [shape::List],
        keep: bool,
    ) -> Result<Vec<bool>, Error> {
        let mut present = Vec::new();
        let (mut seen_rows, mut seen_levels) = (0usize, 0usize);
        loop {
            let (mut definitions, mut repetitions) = (Vec::new(), Vec::new());
            let (records, count) = self.read_records(&mut definitions, &mut repetitions)?;
            if records == 0 && count == 0 {
                break;
            }
            seen_rows = seen_rows
                .checked_add(records)
                .filter(|seen| *seen <= rows)
                .ok_or(Error::Malformed)?;
            seen_levels = seen_levels
                .checked_add(count)
                .filter(|seen| *seen <= levels)
                .ok_or(Error::Malformed)?;
            for index in 0..count {
                let definition = definitions.get(index).copied().unwrap_or(0);
                let repetition = repetitions.get(index).copied().unwrap_or(0);
                for shape in shapes.iter_mut() {
                    shape.observe(definition, repetition)?;
                }
                if keep {
                    present.push(definition == self.leaf.max_definition);
                }
            }
        }
        if seen_rows != rows || seen_levels != levels {
            return Err(Error::Malformed);
        }
        for shape in shapes {
            shape.finish()?;
        }
        Ok(present)
    }

    /// One read of [`BATCH`] records: the records and levels it read, with
    /// each level's definition and repetition.
    fn read_records(
        &mut self,
        definitions: &mut Vec<i16>,
        repetitions: &mut Vec<i16>,
    ) -> Result<(usize, usize), Error> {
        let (mut records, mut levels) = (0usize, 0usize);
        while records < BATCH && self.has_next()? {
            let leaf = self.leaf;
            let page = self.page.as_mut().expect("a loaded page");
            let remaining_records = BATCH - records;
            let remaining_levels = page.levels - page.read;
            let (records_read, to_read) = if leaf.max_repetition > 0 {
                let (mut ended, mut read) = (0, 0);
                for &level in &page.repetition[page.read..page.levels] {
                    // A level of zero begins a record, and so ends the one in
                    // progress. The level that completes the read stays
                    // unread and begins the next.
                    if level == 0 && self.partial {
                        ended += 1;
                        if ended == remaining_records {
                            self.partial = false;
                            break;
                        }
                    }
                    self.partial = true;
                    read += 1;
                }
                if read == remaining_levels && page.delimiter {
                    ended += usize::from(std::mem::take(&mut self.partial));
                }
                (ended, read)
            } else {
                let read = remaining_records.min(remaining_levels);
                (read, read)
            };
            let range = page.read..page.read + to_read;
            if leaf.max_definition > 0 {
                definitions.extend_from_slice(&page.definition[range.clone()]);
            }
            if leaf.max_repetition > 0 {
                repetitions.extend_from_slice(&page.repetition[range.clone()]);
            }
            if let Some((decoder, left)) = &mut page.booleans {
                let wanted = if leaf.max_definition > 0 {
                    page.definition[range]
                        .iter()
                        .filter(|&&level| level == leaf.max_definition)
                        .count()
                } else {
                    to_read
                };
                let read = decoder.get(wanted.min(*left))?;
                if read != wanted {
                    return Err(Error::Malformed);
                }
                *left -= read;
            }
            page.read += to_read;
            records += records_read;
            levels += to_read;
        }
        Ok((records, levels))
    }

    /// Whether the chunk has more levels, loading the next data page when the
    /// loaded one is read. A page of no levels has none, though a later page
    /// may.
    fn has_next(&mut self) -> Result<bool, Error> {
        match &self.page {
            Some(page) if page.levels != 0 && page.read != page.levels => Ok(true),
            _ => Ok(self.load()? && self.page.as_ref().is_some_and(|page| page.levels != 0)),
        }
    }

    /// Load the next data page, after configuring the dictionary from any
    /// dictionary page before it; false when the chunk has no more.
    fn load(&mut self) -> Result<bool, Error> {
        while let Some(page) = self.pages.get(self.next) {
            self.next += 1;
            match page.payload {
                Payload::Index => {}
                Payload::Dictionary { encoding, values } => {
                    self.configure_dictionary(page, encoding, values)?;
                }
                Payload::DataV1 { .. } | Payload::DataV2 { .. } => {
                    self.page = Some(self.data_page(page)?);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The chunk's dictionary, from its dictionary page: one per chunk, in a
    /// plain encoding, read whole.
    fn configure_dictionary(&mut self, page: &Page, code: i64, count: usize) -> Result<(), Error> {
        let encoding = closed_profile::encoding(code).ok_or(Error::Malformed)?;
        if !closed_profile::dictionary_page(encoding) || self.dictionary.is_some() {
            return Err(Error::Malformed);
        }
        let decoded = compression::decode(page, self.file)?;
        self.dictionary = Some(plain(&decoded, count, self.leaf)?);
        Ok(())
    }

    /// Load the data page `page`: its levels and, when it holds any, its
    /// present values.
    fn data_page(&mut self, page: &Page) -> Result<Loaded, Error> {
        let leaf = self.leaf;
        check_header(self.file, page, leaf)?;
        let decoded = compression::decode(page, self.file)?;
        let data = payload::data(page, &decoded, leaf)?.ok_or(Error::Malformed)?;
        let encoding = closed_profile::encoding(data.encoding).ok_or(Error::Malformed)?;
        let delimiter = leaf.max_repetition > 0 && self.at_record_boundary()?;
        let repetition = levels(data.repetition, leaf.max_repetition, data.levels, true)?;
        let definition = levels(data.definition, leaf.max_definition, data.levels, false)?;
        let present = if leaf.max_definition > 0 {
            definition
                .iter()
                .filter(|&&level| level == leaf.max_definition)
                .count()
        } else {
            data.levels
        };
        // The decoder is told how many values the page holds: its levels on
        // a version 1 page, and its present values on a version 2 page.
        let told = match page.payload {
            Payload::DataV2 { values, nulls, .. } => values - nulls,
            _ => data.levels,
        };
        let decoder = Decoder::new(encoding, data.body, leaf, self.dictionary.is_some())?;
        let mut booleans = None;
        match decoder {
            Decoder::Rle(runs) => {
                // The decoder keeps its own copy of the runs, which it reads
                // as the page is read. No kept column holds booleans.
                let runs: Box<[u8]> = data.body[runs].into();
                booleans = Some((Rle::new(runs, 1)?, told));
                self.values.append(Values::Count(present));
            }
            // The reader reads values only from a page that has levels.
            decoder if data.levels > 0 => {
                let values = decoder.values(data.body, present, leaf, self.dictionary.as_ref())?;
                self.values.append(values);
            }
            _ => {}
        }
        Ok(Loaded {
            levels: data.levels,
            repetition,
            definition,
            booleans,
            delimiter,
            read: 0,
        })
    }

    /// Whether the loaded page is the chunk's last, as the column reader
    /// tells by reading the next page header. It reads past an index page's
    /// header to the bytes after it, so it reads an index page's body, if it
    /// has one, as a page header.
    fn at_record_boundary(&self) -> Result<bool, Error> {
        for page in &self.pages[self.next..] {
            match page.payload {
                Payload::Index if !page.body.is_empty() => return Err(Error::Malformed),
                Payload::Index => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
}

/// What loading a data page checks of its header: the level encodings of a
/// version 1 page are codes the format knows, even for levels the column
/// does not have; a version 2 page's row count is not negative; and the
/// page's statistics fit the leaf's physical type.
fn check_header(file: &[u8], page: &Page, leaf: &Leaf) -> Result<(), Error> {
    let (header, _) = compact::parse(&file[page.header.clone()])?;
    let detail = match page.payload {
        Payload::DataV1 {
            definition_encoding,
            repetition_encoding,
            ..
        } => {
            for code in [definition_encoding, repetition_encoding] {
                closed_profile::encoding(code).ok_or(Error::Malformed)?;
            }
            header.field(5)?
        }
        _ => {
            let detail = header.field(8)?;
            let rows = detail.map(|detail| detail.field(3)).transpose()?.flatten();
            if !matches!(rows, Some(Value::Integer(rows)) if rows >= 0) {
                return Err(Error::Malformed);
            }
            detail
        }
    };
    let detail = detail.ok_or(Error::Malformed)?;
    let id = if matches!(page.payload, Payload::DataV1 { .. }) {
        5
    } else {
        8
    };
    if let Some(statistics) = detail.field(id)? {
        super::footer::statistics(statistics, leaf.physical)?;
    }
    Ok(())
}

/// A page's levels of a column whose highest is `maximum`, `count` of them,
/// from their bytes and encoding; none when the column has no such levels.
/// An RLE decoder reads its first run when it is given its bytes, and the
/// repetition levels' decoder, `ahead`, reads [`LEVELS`] levels at a time
/// until it has read the page's.
fn levels(
    (bytes, encoding): (&[u8], i64),
    maximum: i16,
    count: usize,
    ahead: bool,
) -> Result<Vec<i16>, Error> {
    if maximum == 0 {
        return Ok(Vec::new());
    }
    let width = 16 - (maximum as u16).leading_zeros();
    if closed_profile::encoding(encoding) == Some(Encoding::Rle) {
        let mut decoder = Rle::new(bytes, width)?;
        if ahead {
            let mut read = 0;
            while read < count {
                match decoder.get(LEVELS)? {
                    0 => return Err(Error::Malformed),
                    more => read += more,
                }
            }
        }
    }
    let mut levels = Vec::with_capacity(count);
    payload::level_runs(bytes, maximum, encoding, count, |value, run| {
        levels.extend(std::iter::repeat_n(value as i16, run));
        Ok(())
    })?;
    Ok(levels)
}

/// The column reader's RLE decoder, counting the values it reads from the
/// runs `bytes`, with its bit reader's position.
struct Rle<B> {
    bytes: B,
    byte: usize,
    bit: usize,
    width: u32,
    repeated: usize,
    packed: usize,
}

impl<B: AsRef<[u8]>> Rle<B> {
    /// Given its bytes, the decoder reads its first run.
    fn new(bytes: B, width: u32) -> Result<Self, Error> {
        let mut decoder = Self {
            bytes,
            byte: 0,
            bit: 0,
            width,
            repeated: 0,
            packed: 0,
        };
        decoder.reload()?;
        Ok(decoder)
    }

    /// Read `count` whole bytes from the next byte boundary; false when they
    /// are not there.
    fn aligned(&mut self, count: usize) -> bool {
        self.byte += self.bit.div_ceil(8);
        self.bit = 0;
        let end = self.byte.saturating_add(count);
        if end > self.bytes.as_ref().len() {
            return false;
        }
        self.byte = end;
        true
    }

    /// Read an unsigned varint; none when the bytes end first. An eleventh
    /// byte of one is malformed.
    fn vlq(&mut self) -> Result<Option<i64>, Error> {
        let mut value = 0i64;
        for index in 0..=10 {
            if !self.aligned(1) {
                return Ok(None);
            }
            if index == 10 {
                return Err(Error::Malformed);
            }
            let byte = self.bytes.as_ref()[self.byte - 1];
            value |= i64::from(byte & 0x7f) << (7 * index);
            if byte & 0x80 == 0 {
                return Ok(Some(value));
            }
        }
        unreachable!("the loop returns")
    }

    /// Read the next run's header, and a repeated run's value, which must be
    /// there; false when there is no next run. A header of zero ends the
    /// runs of one read.
    fn reload(&mut self) -> Result<bool, Error> {
        match self.vlq()? {
            None | Some(0) => Ok(false),
            Some(indicator) if indicator & 1 == 1 => {
                self.packed = (indicator >> 1).wrapping_mul(8) as u32 as usize;
                Ok(true)
            }
            Some(indicator) => {
                self.repeated = (indicator >> 1) as u32 as usize;
                if !self.aligned(self.width.div_ceil(8) as usize) {
                    return Err(Error::Malformed);
                }
                Ok(true)
            }
        }
    }

    /// Read up to `count` values; how many it read.
    fn get(&mut self, count: usize) -> Result<usize, Error> {
        let mut read = 0;
        while read < count {
            if self.repeated > 0 {
                let run = self.repeated.min(count - read);
                self.repeated -= run;
                read += run;
            } else if self.packed > 0 {
                // As many of the bit-packed values as its bits hold.
                let width = self.width as usize;
                let left = (self.bytes.as_ref().len() - self.byte) * 8 - self.bit;
                let wanted = self.packed.min(count - read);
                let run = left
                    .checked_div(width)
                    .map_or(wanted, |most| wanted.min(most));
                if run == 0 {
                    self.packed = 0;
                    continue;
                }
                let bits = self.bit + run * width;
                self.byte += bits / 8;
                self.bit = bits % 8;
                self.packed -= run;
                read += run;
            } else if !self.reload()? {
                break;
            }
        }
        Ok(read)
    }
}

/// A data page's value decoder, as the column reader makes it when it loads
/// the page: one that its physical type has for its encoding. RLE booleans
/// lie in the range it holds of the page's value bytes.
enum Decoder {
    Plain,
    Rle(std::ops::Range<usize>),
    Delta(Encoding),
    Split,
    Dictionary,
}

impl Decoder {
    fn new(encoding: Encoding, body: &[u8], leaf: &Leaf, dictionary: bool) -> Result<Self, Error> {
        if closed_profile::dictionary_data(encoding) {
            let (&width, indices) = body.split_first().ok_or(Error::Malformed)?;
            if !dictionary {
                return Err(Error::Malformed);
            }
            Rle::new(indices, u32::from(width))?;
            return Ok(Self::Dictionary);
        }
        if !closed_profile::decodes(leaf.physical, encoding) {
            return Err(Error::Malformed);
        }
        Ok(match encoding {
            Encoding::Plain => Self::Plain,
            Encoding::Rle => {
                let size = body.get(..4).ok_or(Error::Malformed)?;
                let size = i32::from_le_bytes(size.try_into().unwrap()) as usize;
                if body.len() - 4 < size {
                    return Err(Error::Malformed);
                }
                Self::Rle(4..4 + size)
            }
            Encoding::ByteStreamSplit => {
                // A fixed-length type is split by its length, which must
                // divide the page's bytes.
                if leaf.physical == Physical::FixedLenByteArray {
                    let width = width(leaf);
                    if width == 0 || body.len() % width != 0 {
                        return Err(Error::Malformed);
                    }
                }
                Self::Split
            }
            encoding => Self::Delta(encoding),
        })
    }

    /// The page's `present` values, from its value bytes `body`.
    fn values(
        self,
        body: &[u8],
        present: usize,
        leaf: &Leaf,
        dictionary: Option<&Values>,
    ) -> Result<Values, Error> {
        match self {
            Self::Plain => plain(body, present, leaf),
            Self::Split => split(body, present, leaf),
            Self::Delta(encoding) => delta_values(body, encoding, present, leaf),
            Self::Dictionary => {
                let dictionary = dictionary.ok_or(Error::Malformed)?;
                let mut values = Values::empty(leaf);
                let entries = dictionary.len();
                let mut taken = 0;
                rle::hybrid(&body[1..], u32::from(body[0]), present, |index, run| {
                    if index as usize >= entries {
                        return Err(compact::Error::Malformed);
                    }
                    values.repeat(dictionary, index as usize, run);
                    taken += run;
                    Ok(())
                })?;
                if taken != present {
                    return Err(Error::Malformed);
                }
                Ok(values)
            }
            Self::Rle(..) => unreachable!("RLE booleans decode as they are read"),
        }
    }
}

/// `count` plain values of `leaf`'s physical type from `body`. A
/// fixed-length type of no length has none to read.
fn plain(body: &[u8], count: usize, leaf: &Leaf) -> Result<Values, Error> {
    match leaf.physical {
        Physical::Boolean => {
            if count.div_ceil(8) > body.len() {
                return Err(Error::Malformed);
            }
            Ok(Values::Count(count))
        }
        Physical::ByteArray => {
            let mut values = Binary::default();
            let mut rest = body;
            for _ in 0..count {
                let header = rest.get(..4).ok_or(Error::Malformed)?;
                let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
                let end = length.checked_add(4).ok_or(Error::Malformed)?;
                values.push(Some(rest.get(4..end).ok_or(Error::Malformed)?));
                rest = &rest[end..];
            }
            Ok(Values::Bytes(values))
        }
        _ => {
            let width = width(leaf);
            if width == 0 {
                return Err(Error::Malformed);
            }
            let size = count.checked_mul(width).ok_or(Error::Malformed)?;
            let bytes = body.get(..size).ok_or(Error::Malformed)?;
            Ok(fixed(leaf, width, bytes.to_vec()))
        }
    }
}

/// Fixed-width values: a kept column's bytes, or how many of another's.
fn fixed(leaf: &Leaf, width: usize, bytes: Vec<u8>) -> Values {
    match leaf.physical {
        Physical::Int32 | Physical::Int64 | Physical::Float | Physical::Double => {
            Values::Fixed { width, bytes }
        }
        _ => Values::Count(bytes.len() / width),
    }
}

/// `count` byte stream split values from `body`: byte `j` of value `k` is
/// byte `k + j * stride` of the page, the stride being the page's bytes over
/// the value's width, and every such byte must be there.
fn split(body: &[u8], count: usize, leaf: &Leaf) -> Result<Values, Error> {
    let width = width(leaf);
    let stride = body.len() / width;
    if count > 0 && (count - 1) + (width - 1) * stride >= body.len() {
        return Err(Error::Malformed);
    }
    let mut bytes = Vec::with_capacity(count * width);
    for value in 0..count {
        bytes.extend((0..width).map(|byte| body[value + byte * stride]));
    }
    Ok(fixed(leaf, width, bytes))
}

/// `count` delta-encoded values from `body`, which admission has checked
/// against that count (`delta::admit`).
fn delta_values(
    body: &[u8],
    encoding: Encoding,
    count: usize,
    leaf: &Leaf,
) -> Result<Values, Error> {
    let lengths = |bytes: &[u8]| -> Result<(usize, Vec<usize>), Error> {
        let mut lengths = Vec::new();
        let (offset, _) = delta::scan(bytes, 32, count, |length| {
            lengths.push(usize::try_from(length).map_err(|_| compact::Error::Malformed)?);
            Ok(())
        })?;
        Ok((offset, lengths))
    };
    match encoding {
        Encoding::DeltaBinaryPacked => {
            let width = width(leaf);
            let mut bytes = Vec::with_capacity(count * width);
            delta::scan(body, 8 * width as u32, count, |value| {
                bytes.extend_from_slice(&value.to_le_bytes()[..width]);
                Ok(())
            })?;
            Ok(fixed(leaf, width, bytes))
        }
        Encoding::DeltaLengthByteArray => {
            let (mut at, lengths) = lengths(body)?;
            let mut values = Binary::default();
            for length in lengths.into_iter().take(count) {
                let end = at.checked_add(length).ok_or(Error::Malformed)?;
                values.push(Some(body.get(at..end).ok_or(Error::Malformed)?));
                at = end;
            }
            Ok(Values::Bytes(values))
        }
        _ => {
            let (offset, prefixes) = lengths(body)?;
            let rest = &body[offset..];
            let (mut at, suffixes) = lengths(rest)?;
            let mut previous: Vec<u8> = Vec::new();
            let mut values = Binary::default();
            for (prefix, suffix) in prefixes.into_iter().zip(suffixes).take(count) {
                let end = at.checked_add(suffix).ok_or(Error::Malformed)?;
                let mut value = previous.get(..prefix).ok_or(Error::Malformed)?.to_vec();
                value.extend_from_slice(rest.get(at..end).ok_or(Error::Malformed)?);
                at = end;
                values.push(Some(&value));
                previous = value;
            }
            if leaf.physical == Physical::ByteArray {
                Ok(Values::Bytes(values))
            } else {
                Ok(Values::Count(values.len()))
            }
        }
    }
}
