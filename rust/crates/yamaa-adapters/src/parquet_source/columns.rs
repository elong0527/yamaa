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

/// One leaf column's stored values, held as the engine's columns hold them
/// (`crate::columns`): payloads beside their validity, byte arrays in one
/// arena. `Discard` is a column the table does not keep, whose pages are
/// read and their values dropped.
#[derive(Debug, PartialEq)]
pub(super) enum Column {
    Int32(Stored<i32>),
    Int64(Stored<i64>),
    Float(Stored<f32>),
    Double(Stored<f64>),
    Bytes(Binary),
    Discard,
}

/// Fixed-width stored values with their validity.
pub(super) type Stored<T> = crate::columns::Values<T>;

impl Column {
    /// An empty column of the values `leaf` stores, which a kept column's
    /// physical type is, with room for `rows` slots.
    fn new(leaf: &Leaf, rows: usize) -> Result<Self, Error> {
        fn room<T: Copy + Default>(rows: usize) -> Stored<T> {
            Stored::from_parts(
                Vec::with_capacity(rows),
                crate::columns::Validity::default(),
            )
            .expect("no slots")
        }
        Ok(match leaf.physical {
            Physical::Int32 => Self::Int32(room(rows)),
            Physical::Int64 => Self::Int64(room(rows)),
            Physical::Float => Self::Float(room(rows)),
            Physical::Double => Self::Double(room(rows)),
            Physical::ByteArray => {
                let mut binary = Binary::default();
                binary.offsets.reserve(rows);
                Self::Bytes(binary)
            }
            _ => return Err(Error::Malformed),
        })
    }
}

/// Byte array values in one arena: value `i` is
/// `data[offsets[i]..offsets[i + 1]]`, and a missing value holds no bytes.
#[derive(Debug, PartialEq)]
pub(super) struct Binary {
    offsets: Vec<usize>,
    data: Vec<u8>,
    validity: crate::columns::Validity,
}

impl Default for Binary {
    fn default() -> Self {
        Self {
            offsets: vec![0],
            data: Vec::new(),
            validity: crate::columns::Validity::default(),
        }
    }
}

impl Binary {
    /// Append one value, or a missing one.
    #[cfg(test)]
    pub(super) fn push(&mut self, value: Option<&[u8]>) {
        self.validity.push(value.is_some());
        self.data.extend_from_slice(value.unwrap_or_default());
        self.offsets.push(self.data.len());
    }

    /// The values as the engine's text, or `None` when one is not UTF-8
    /// (`crate::columns::Text::from_utf8_parts`).
    pub(super) fn text(self) -> Option<crate::columns::Text> {
        crate::columns::Text::from_utf8_parts(self.offsets, self.data, self.validity)
    }

    /// The bytes of every value together.
    pub(super) fn bytes(&self) -> usize {
        self.data.len()
    }
}

/// The present values of one page, a dictionary or a chunk, of a byte
/// array type, end to end: value `i` ends at `ends[i]`.
#[derive(Debug, Default)]
struct Arena {
    data: Vec<u8>,
    ends: Vec<usize>,
}

impl Arena {
    fn len(&self) -> usize {
        self.ends.len()
    }

    fn push(&mut self, value: &[u8]) {
        self.data.extend_from_slice(value);
        self.ends.push(self.data.len());
    }

    /// Append `count` copies of value `index` of `from`. A short value is
    /// copied as a whole word of its arena, then cut to its length, which
    /// copies without a call.
    fn repeat(&mut self, from: &Arena, index: usize, count: usize) {
        const WORD: usize = 16;
        let start = index.checked_sub(1).map_or(0, |before| from.ends[before]);
        let end = from.ends[index];
        let length = end - start;
        self.ends.reserve(count);
        self.data.reserve(length * count);
        match from.data.get(start..start + WORD) {
            Some(word) if length <= WORD => {
                let word: [u8; WORD] = word.try_into().expect("a word");
                for _ in 0..count {
                    let at = self.data.len();
                    self.data.extend_from_slice(&word);
                    self.data.truncate(at + length);
                    self.ends.push(at + length);
                }
            }
            _ => {
                for _ in 0..count {
                    self.push(&from.data[start..end]);
                }
            }
        }
    }

    /// Append `other`'s values.
    fn append(&mut self, other: Arena) {
        if self.ends.is_empty() && self.data.is_empty() {
            *self = other;
            return;
        }
        let base = self.data.len();
        self.data.extend_from_slice(&other.data);
        self.ends.extend(other.ends.iter().map(|end| base + end));
    }
}

/// The decoded bodies of a run of the file's pages, as their admission
/// decoded them (`reader::Runs::admit`), so that the column reader does not
/// decode a page twice.
pub(super) struct Bodies {
    /// The position of the first page among the file's pages.
    first: usize,
    bodies: Vec<Vec<u8>>,
}

impl Bodies {
    pub(super) fn new(first: usize, bodies: Vec<Vec<u8>>) -> Self {
        Self { first, bodies }
    }

    /// The body of page `page` of the file, when it is one of these.
    fn get(&self, page: usize) -> Option<&[u8]> {
        let index = page.checked_sub(self.first)?;
        self.bodies.get(index).map(Vec::as_slice)
    }
}

/// Present values of one page, a dictionary or a chunk: a kept column's, as
/// each fixed-width value's little-endian bytes or as byte arrays, or only
/// how many there are, for a physical type that no kept column holds.
#[derive(Debug)]
enum Values {
    Fixed { width: usize, bytes: Vec<u8> },
    Bytes(Arena),
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
            Physical::ByteArray => Self::Bytes(Arena::default()),
            _ => Self::Count(0),
        }
    }

    /// Append `count` copies of value `index` of `from`, of the same kind.
    fn repeat(&mut self, from: &Values, index: usize, count: usize) {
        match (self, from) {
            (Self::Fixed { width, bytes }, Self::Fixed { bytes: source, .. }) => {
                let value = &source[index * *width..(index + 1) * *width];
                bytes.reserve(value.len() * count);
                // A width the compiler knows copies without a call.
                fn copies<const N: usize>(bytes: &mut Vec<u8>, value: &[u8], count: usize) {
                    let value: [u8; N] = value.try_into().expect("a value of its width");
                    for _ in 0..count {
                        bytes.extend_from_slice(&value);
                    }
                }
                match value.len() {
                    4 => copies::<4>(bytes, value, count),
                    8 => copies::<8>(bytes, value, count),
                    _ => {
                        for _ in 0..count {
                            bytes.extend_from_slice(value);
                        }
                    }
                }
            }
            (Self::Bytes(values), Self::Bytes(source)) => values.repeat(source, index, count),
            (Self::Count(total), _) => *total += count,
            _ => unreachable!("values of one physical type"),
        }
    }

    /// Append `other`, of the same kind.
    fn append(&mut self, other: Values) {
        match (self, other) {
            (Self::Fixed { bytes, .. }, Self::Fixed { bytes: more, .. }) => {
                if bytes.is_empty() {
                    *bytes = more;
                } else {
                    bytes.extend_from_slice(&more);
                }
            }
            (Self::Bytes(values), Self::Bytes(more)) => values.append(more),
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
/// file order, as `pages`, each page's body read from `bodies` where its
/// admission kept it, and decoded again otherwise. A column `keep` marks keeps its values; every
/// column is read. Each fixed-size list of the Arrow schema `arrow` checks
/// its rows' sizes from the levels (`shape`).
pub(super) fn read(
    file: &[u8],
    pages: &[Page],
    bodies: Option<&Bodies>,
    footer: &Footer,
    keep: &[bool],
    arrow: &arrow_schema::Schema,
    (groups, window): (std::ops::Range<usize>, Option<std::ops::Range<usize>>),
) -> Result<Vec<Column>, Error> {
    let leaves = &footer.schema.leaves;
    let shapes = shape::constraints(arrow, &footer.schema)?;
    if keep.len() != leaves.len() {
        return Err(Error::Malformed);
    }
    let mut result: Vec<_> = leaves.iter().map(|_| Column::Discard).collect();
    let first = groups.start;
    let row_groups = footer.row_groups.get(groups).ok_or(Error::Malformed)?;
    // Room for the records the window keeps, or the row groups declare,
    // within the table's bound; a count that does not read is refused
    // where it is read.
    let records = match &window {
        Some(window) => window.len(),
        None => row_groups
            .iter()
            .filter_map(|group| usize::try_from(group.num_rows).ok())
            .fold(0usize, usize::saturating_add),
    }
    .min(yamaa_core::limits::Limits::TABLE_ROWS);
    // The first record of the row group being read, among the groups'.
    let mut before = 0usize;
    for (offset, group) in row_groups.iter().enumerate() {
        let row_group = first + offset;
        let rows = usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?;
        // The group's records the window keeps, every one without it.
        let kept = match &window {
            Some(window) => {
                let start = window.start.saturating_sub(before).min(rows);
                start..window.end.saturating_sub(before).clamp(start, rows)
            }
            None => 0..rows,
        };
        before = before.saturating_add(rows);
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
            let mut chunk = Chunk::new(file, (&pages[start..end], start), bodies, leaf);
            let mut shapes = shapes[index].clone();
            let present = chunk.collect((rows, levels), &mut shapes, keep[index])?;
            if keep[index] {
                if matches!(result[index], Column::Discard) {
                    result[index] = Column::new(leaf, records)?;
                }
                append(
                    &mut result[index],
                    &present,
                    chunk.values,
                    (records, kept.clone()),
                )?;
            }
        }
    }
    // A file of no rows may hold no row groups; its kept columns keep their
    // physical type.
    for (index, leaf) in leaves.iter().enumerate() {
        if keep[index] && matches!(result[index], Column::Discard) {
            result[index] = Column::new(leaf, 0)?;
        }
    }
    Ok(result)
}

/// Append one chunk's values to the kept column read so far: a value for
/// each level `present` marks, in order, and a missing value for the others.
/// Only the levels `window` keeps are appended, and the column makes room
/// for the `records` of every chunk it will hold.
fn append(
    column: &mut Column,
    present: &[bool],
    values: Values,
    (records, window): (usize, std::ops::Range<usize>),
) -> Result<(), Error> {
    let count = |slots: &[bool]| slots.iter().filter(|&&present| present).count();
    if count(present) != values.len() {
        return Err(Error::Malformed);
    }
    // The present values before the window, and those in it.
    let window = window.start.min(present.len())..window.end.min(present.len());
    let skip = count(&present[..window.start]);
    let present = &present[window];
    let taken = skip..skip + count(present);
    match (column, values) {
        (Column::Int32(out), Values::Fixed { width: 4, bytes }) => {
            fixed_slots(
                out,
                present,
                &bytes[4 * taken.start..4 * taken.end],
                i32::from_le_bytes,
            );
        }
        (Column::Int64(out), Values::Fixed { width: 8, bytes }) => {
            fixed_slots(
                out,
                present,
                &bytes[8 * taken.start..8 * taken.end],
                i64::from_le_bytes,
            );
        }
        (Column::Float(out), Values::Fixed { width: 4, bytes }) => {
            fixed_slots(
                out,
                present,
                &bytes[4 * taken.start..4 * taken.end],
                f32::from_le_bytes,
            );
        }
        (Column::Double(out), Values::Fixed { width: 8, bytes }) => {
            fixed_slots(
                out,
                present,
                &bytes[8 * taken.start..8 * taken.end],
                f64::from_le_bytes,
            );
        }
        (Column::Bytes(out), Values::Bytes(arena)) => {
            // The window's present values end to end: their bytes, moved
            // when they are the chunk's and the column holds none yet.
            let end_of = |value: Option<usize>| value.map_or(0, |value| arena.ends[value]);
            let from = end_of(taken.start.checked_sub(1));
            let to = end_of(taken.end.checked_sub(1)).max(from);
            let base = out.data.len();
            if base == 0 {
                // Room for `records` slots of the window's mean length,
                // within the offsets a column's text fits (`UTF8_OFFSETS`).
                let room = ((to - from) / present.len().max(1))
                    .saturating_mul(records)
                    .min(i32::MAX as usize);
                if from == 0 && to == arena.data.len() {
                    out.data = arena.data;
                } else {
                    out.data.extend_from_slice(&arena.data[from..to]);
                }
                out.data.reserve(room.saturating_sub(out.data.len()));
            } else {
                out.data.extend_from_slice(&arena.data[from..to]);
            }
            // Missing values hold no bytes, so each slot ends where the
            // last present value does.
            let mut end = *out.offsets.last().expect("an offset before each slot");
            let mut next = taken.start;
            out.offsets.extend(present.iter().map(|&present| {
                if present {
                    end = base + arena.ends[next] - from;
                    next += 1;
                }
                end
            }));
            out.validity.extend_from_slice(present);
        }
        _ => return Err(Error::Malformed),
    }
    Ok(())
}

/// Append to `out` a slot for each of `present`, holding the next of the
/// little-endian `bytes`' values where it is present, as `decode` reads it.
fn fixed_slots<T: Copy + Default, const N: usize>(
    out: &mut Stored<T>,
    present: &[bool],
    bytes: &[u8],
    decode: fn([u8; N]) -> T,
) {
    let (mut payloads, mut validity) = std::mem::take(out).into_parts();
    let mut values = bytes
        .chunks_exact(N)
        .map(|value| decode(value.try_into().expect("a value of its width")));
    if values.len() == present.len() {
        payloads.extend(values);
    } else {
        payloads.reserve(present.len());
        for &present in present {
            payloads.push(match present {
                true => values.next().expect("a value per present slot"),
                false => T::default(),
            });
        }
    }
    validity.extend_from_slice(present);
    *out = Stored::from_parts(payloads, validity).expect("a payload per slot");
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
    /// The position of the chunk's first page among the file's pages.
    first: usize,
    bodies: Option<&'a Bodies>,
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
    fn new(
        file: &'a [u8],
        (pages, first): (&'a [Page], usize),
        bodies: Option<&'a Bodies>,
        leaf: &'a Leaf,
    ) -> Self {
        Self {
            file,
            pages,
            first,
            bodies,
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
        let mut present = Vec::with_capacity(if keep { rows.min(levels) } else { 0 });
        let (mut seen_rows, mut seen_levels) = (0usize, 0usize);
        let (mut definitions, mut repetitions) = (Vec::new(), Vec::new());
        loop {
            definitions.clear();
            repetitions.clear();
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
            if !shapes.is_empty() {
                for index in 0..count {
                    let definition = definitions.get(index).copied().unwrap_or(0);
                    let repetition = repetitions.get(index).copied().unwrap_or(0);
                    for shape in shapes.iter_mut() {
                        shape.observe(definition, repetition)?;
                    }
                }
            }
            if keep {
                // A level beyond the definitions read, of a column without
                // them, is of definition zero.
                let maximum = self.leaf.max_definition;
                let read = definitions.len().min(count);
                present.extend(definitions[..read].iter().map(|&level| level == maximum));
                present.extend(std::iter::repeat_n(maximum == 0, count - read));
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
                    self.configure_dictionary(self.next - 1, encoding, values)?;
                }
                Payload::DataV1 { .. } | Payload::DataV2 { .. } => {
                    self.page = Some(self.data_page(self.next - 1)?);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Page `index`'s body: the one its admission kept, or decoded again.
    fn body(&self, index: usize) -> Result<std::borrow::Cow<'a, [u8]>, Error> {
        let kept = self
            .bodies
            .and_then(|bodies| bodies.get(self.first + index));
        Ok(match kept {
            Some(body) => std::borrow::Cow::Borrowed(body),
            None => std::borrow::Cow::Owned(compression::decode(&self.pages[index], self.file)?),
        })
    }

    /// The chunk's dictionary, from its dictionary page: one per chunk, in a
    /// plain encoding, read whole.
    fn configure_dictionary(&mut self, index: usize, code: i64, count: usize) -> Result<(), Error> {
        let encoding = closed_profile::encoding(code).ok_or(Error::Malformed)?;
        if !closed_profile::dictionary_page(encoding) || self.dictionary.is_some() {
            return Err(Error::Malformed);
        }
        let decoded = self.body(index)?;
        self.dictionary = Some(plain(&decoded, count, self.leaf)?);
        Ok(())
    }

    /// Load the data page `page`: its levels and, when it holds any, its
    /// present values.
    fn data_page(&mut self, index: usize) -> Result<Loaded, Error> {
        let (leaf, pages) = (self.leaf, self.pages);
        let page = &pages[index];
        check_header(self.file, page, leaf)?;
        let decoded = self.body(index)?;
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
                let dictionary = self.dictionary.as_ref();
                decoder.values(&mut self.values, data.body, present, leaf, dictionary)?;
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

    /// Append the page's `present` values, from its value bytes `body`, to
    /// `values`; a dictionary's are copied there directly.
    fn values(
        self,
        values: &mut Values,
        body: &[u8],
        present: usize,
        leaf: &Leaf,
        dictionary: Option<&Values>,
    ) -> Result<(), Error> {
        match self {
            Self::Plain => values.append(plain(body, present, leaf)?),
            Self::Split => values.append(split(body, present, leaf)?),
            Self::Delta(encoding) => values.append(delta_values(body, encoding, present, leaf)?),
            Self::Dictionary => {
                let dictionary = dictionary.ok_or(Error::Malformed)?;
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
            }
            Self::Rle(..) => unreachable!("RLE booleans decode as they are read"),
        }
        Ok(())
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
            let mut values = Arena::default();
            let mut rest = body;
            for _ in 0..count {
                let header = rest.get(..4).ok_or(Error::Malformed)?;
                let length = u32::from_le_bytes(header.try_into().unwrap()) as usize;
                let end = length.checked_add(4).ok_or(Error::Malformed)?;
                values.push(rest.get(4..end).ok_or(Error::Malformed)?);
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
            let mut values = Arena::default();
            for length in lengths.into_iter().take(count) {
                let end = at.checked_add(length).ok_or(Error::Malformed)?;
                values.push(body.get(at..end).ok_or(Error::Malformed)?);
                at = end;
            }
            Ok(Values::Bytes(values))
        }
        _ => {
            let (offset, prefixes) = lengths(body)?;
            let rest = &body[offset..];
            let (mut at, suffixes) = lengths(rest)?;
            let mut previous: Vec<u8> = Vec::new();
            let mut values = Arena::default();
            for (prefix, suffix) in prefixes.into_iter().zip(suffixes).take(count) {
                let end = at.checked_add(suffix).ok_or(Error::Malformed)?;
                let mut value = previous.get(..prefix).ok_or(Error::Malformed)?.to_vec();
                value.extend_from_slice(rest.get(at..end).ok_or(Error::Malformed)?);
                at = end;
                values.push(&value);
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
