//! The wire type of each Thrift field the native decoder reads.
//!
//! The native decoder (`footer`, and `framing` for page headers) reads a
//! known field id by the wire type this table gives it, as the readers that
//! the Thrift compiler generated for parquet 55.2.0 (`parquet::format`) read
//! it by its declared type, whatever wire type its field header carries, and
//! each element of a known list by its declared type, whatever element type
//! the list header carries. They skip only an unknown id, by that field's own
//! wire type. A known field with another wire type would make a decoder read
//! other bytes, or other values, than the ones admission checked. Admission
//! therefore refuses it as malformed wherever it occurs.
//!
//! A known field also occurs at most once. The native decoder keeps the last
//! copy of a repeated field, as admission reads it, but writers never repeat
//! a field, so admission refuses a repeated known field as malformed in every
//! struct it walks.
//!
//! parquet 55.2.0's decoder also reserved room for every element a known list
//! declares before it read the first. The compact parser bounds a count only
//! by one byte per element, while one empty struct in a schema or key-value
//! list takes one byte and reserved about 100 or 48 bytes. Admission
//! therefore refuses a list unless its bytes can hold the declared count at
//! the fewest bytes a decoder accepts for each element, so that a
//! reservation never exceeds what decoding that many valid elements takes.
//!
//! Those minimums tie a reservation to the list's own bytes, not to any
//! limit. So admission also refuses a list that declares more elements than a
//! file the engine reads holds (`Shape::most`), and `check` reports the counts
//! that framing compares with the schema's leaves (`Lists`). Framing bounds
//! the schema and the row groups themselves.
//!
//! The table follows the generated readers as this build calls them: the
//! metadata decoder (`ParquetMetaDataReader::decode_metadata`) reads
//! `FileMetaData` and everything below it, and the page reader reads each
//! `PageHeader`. They read every field that the format's Thrift definition
//! declares, whatever features the build enables: the encryption fields of
//! `FileMetaData` and `ColumnChunk`, a column chunk's path and key-value
//! metadata, and each data page header's statistics. The table also keeps the
//! later fields that parquet 57 reads: the variant, geometry and geography
//! logical types and the IEEE 754 total order, which 55 refuses, and
//! geospatial statistics, which it skips. Checking their types refuses only
//! files that no writer produces.
//!
//! The walk also finds what parquet 55's decoder must be shown differently
//! from the held bytes (`Presentation`, which only the differential harness
//! applies), because parquet 55 refuses some files that the rules read or
//! refuse otherwise:
//!
//! - It reads a skipped binary as UTF-8 text, and refuses one that is not.
//!   Such a binary holds only metadata (PARQUET-10), so it is shown as zero
//!   bytes, in a footer and in a page header alike.
//! - It knows no column order but the type-defined one. Column orders bear
//!   only on statistics, which the engine never reads (PARQUET-10), so any
//!   other order, such as the IEEE 754 total order, is shown as the
//!   type-defined one.
//! - It knows none of the variant, geometry and geography logical types,
//!   which PARQUET-7 refuses as `source_field_type_unsupported` (#3155). A
//!   schema element annotated with one as parquet 57 admitted it is shown
//!   without the annotation, so the footer decodes to the types parquet 57
//!   gave such a field, and the walk reports the element (`Checked`), whose
//!   leaf the closed mapping then refuses. An annotation parquet 57 refused,
//!   such as a geometry that is not a `BYTE_ARRAY`, stays, and the decoder
//!   refuses it as an invalid file (PARQUET-5).
//!
//! The native decoder reads the held bytes by those rules themselves.

use super::closed_profile;
use super::compact::{self, Cursor, Error, Value};
use std::ops::Range;
use yamaa_core::limits::Limits;

/// The most key-value metadata entries a footer, or a column chunk's
/// metadata, may declare. The format bounds them by nothing; writers write a
/// few and the engine reads one, so the column limit serves as the bound.
const KEY_VALUES: usize = Limits::COLUMNS;

/// The most geospatial types a column chunk's statistics may list. The format
/// asks only that the codes be distinct, and its table names the most common
/// ones rather than all. It defines these statistics for geometry and
/// geography columns alone, which the engine does not read, so no file the
/// engine reads lists any. The column limit serves as the bound, as for
/// key-value metadata: a geospatial file's own list, a few of the codes the
/// table names, still reaches the refusal of its field type.
const GEOSPATIAL_TYPES: usize = Limits::COLUMNS;

/// A Thrift struct or union that the native decoder reads field by field.
#[derive(Clone, Copy, Debug)]
pub(super) enum Shape {
    FileMetaData,
    SchemaElement,
    /// A union: the native decoder reads the one variant present.
    LogicalType,
    DecimalType,
    /// `TimeType` and `TimestampType`, which share one declaration.
    TimestampType,
    /// A union of empty structs.
    TimeUnit,
    IntType,
    VariantType,
    GeometryType,
    GeographyType,
    RowGroup,
    SortingColumn,
    ColumnChunk,
    ColumnMetaData,
    Statistics,
    PageEncodingStats,
    SizeStatistics,
    GeospatialStatistics,
    BoundingBox,
    KeyValue,
    /// A union whose one known variant is an empty struct.
    ColumnOrder,
    /// A union of `AesGcmV1` and `AesGcmCtrV1`.
    EncryptionAlgorithm,
    /// `AesGcmV1` and `AesGcmCtrV1`, which share one declaration.
    AesGcm,
    /// A union: an empty struct, or `EncryptionWithColumnKey`.
    ColumnCryptoMetaData,
    EncryptionWithColumnKey,
    PageHeader,
    DataPageHeader,
    DataPageHeaderV2,
    DictionaryPageHeader,
    /// No field is read: `IndexPageHeader`, and the empty structs of the
    /// union variants that carry no value.
    Empty,
}

/// The wire type the native decoder reads a known field with.
#[derive(Clone, Copy, Debug)]
enum Wire {
    Bool,
    Byte,
    I16,
    I32,
    I64,
    Double,
    Binary,
    List(&'static Wire),
    Struct(Shape),
}

impl Wire {
    /// Whether a held wire type is this one. Either boolean wire type carries
    /// a boolean field's value.
    fn admits(self, kind: u8) -> bool {
        matches!(
            (self, kind),
            (Self::Bool, 1 | 2)
                | (Self::Byte, 3)
                | (Self::I16, 4)
                | (Self::I32, 5)
                | (Self::I64, 6)
                | (Self::Double, 7)
                | (Self::Binary, 8)
                | (Self::List(_), 9)
                | (Self::Struct(_), 12)
        )
    }

    /// The fewest bytes the native decoder accepts for the first element of a
    /// list of this wire type, and for each later element.
    fn minimum(self) -> (usize, usize) {
        match self {
            Self::Struct(shape) => shape.minimum(),
            // Every other element takes at least one byte.
            _ => (1, 1),
        }
    }

    /// Whether `bytes` can hold `count` elements of this wire type.
    fn fits(self, count: usize, bytes: usize) -> bool {
        let (first, each) = self.minimum();
        count == 0
            || bytes
                .checked_sub(first)
                .is_some_and(|rest| count - 1 <= rest / each)
    }
}

impl Shape {
    /// The wire type of field `id`, or `None` for a field the native decoder
    /// skips by its own wire type.
    fn field(self, id: i16) -> Option<Wire> {
        use Shape::*;
        use Wire::{Binary, Bool, Byte, Double, List, I16, I32, I64};
        Some(match (self, id) {
            (FileMetaData, 1) => I32,
            (FileMetaData, 2) => List(&Wire::Struct(SchemaElement)),
            (FileMetaData, 3) => I64,
            (FileMetaData, 4) => List(&Wire::Struct(RowGroup)),
            (FileMetaData, 5) => List(&Wire::Struct(KeyValue)),
            (FileMetaData, 6) => Binary,
            (FileMetaData, 7) => List(&Wire::Struct(ColumnOrder)),
            (FileMetaData, 8) => Wire::Struct(EncryptionAlgorithm),
            (FileMetaData, 9) => Binary,
            (SchemaElement, 1..=3 | 5..=9) => I32,
            (SchemaElement, 4) => Binary,
            (SchemaElement, 10) => Wire::Struct(LogicalType),
            // Id 9 is reserved, and an unknown variant is skipped.
            (LogicalType, 1..=4 | 6 | 11..=15) => Wire::Struct(Empty),
            (LogicalType, 5) => Wire::Struct(DecimalType),
            (LogicalType, 7 | 8) => Wire::Struct(TimestampType),
            (LogicalType, 10) => Wire::Struct(IntType),
            (LogicalType, 16) => Wire::Struct(VariantType),
            (LogicalType, 17) => Wire::Struct(GeometryType),
            (LogicalType, 18) => Wire::Struct(GeographyType),
            (DecimalType, 1 | 2) => I32,
            (TimestampType, 1) => Bool,
            (TimestampType, 2) => Wire::Struct(TimeUnit),
            (TimeUnit, 1..=3) => Wire::Struct(Empty),
            (IntType, 1) => Byte,
            (IntType, 2) => Bool,
            (VariantType, 1) => Byte,
            (GeometryType | GeographyType, 1) => Binary,
            (GeographyType, 2) => I32,
            (RowGroup, 1) => List(&Wire::Struct(ColumnChunk)),
            (RowGroup, 2 | 3 | 5 | 6) => I64,
            (RowGroup, 4) => List(&Wire::Struct(SortingColumn)),
            (RowGroup, 7) => I16,
            (SortingColumn, 1) => I32,
            (SortingColumn, 2 | 3) => Bool,
            (ColumnChunk, 1) => Binary,
            (ColumnChunk, 2 | 4 | 6) => I64,
            (ColumnChunk, 3) => Wire::Struct(ColumnMetaData),
            (ColumnChunk, 5 | 7) => I32,
            (ColumnChunk, 8) => Wire::Struct(ColumnCryptoMetaData),
            (ColumnChunk, 9) => Binary,
            (ColumnMetaData, 1 | 4 | 15) => I32,
            (ColumnMetaData, 2) => List(&I32),
            (ColumnMetaData, 3) => List(&Binary),
            (ColumnMetaData, 8) => List(&Wire::Struct(KeyValue)),
            (ColumnMetaData, 5..=7 | 9..=11 | 14) => I64,
            (ColumnMetaData, 12) => Wire::Struct(Statistics),
            (ColumnMetaData, 13) => List(&Wire::Struct(PageEncodingStats)),
            (ColumnMetaData, 16) => Wire::Struct(SizeStatistics),
            (ColumnMetaData, 17) => Wire::Struct(GeospatialStatistics),
            (Statistics, 1 | 2 | 5 | 6) => Binary,
            (Statistics, 3 | 4) => I64,
            (Statistics, 7 | 8) => Bool,
            (PageEncodingStats, 1..=3) => I32,
            (SizeStatistics, 1) => I64,
            (SizeStatistics, 2 | 3) => List(&I64),
            (GeospatialStatistics, 1) => Wire::Struct(BoundingBox),
            (GeospatialStatistics, 2) => List(&I32),
            (BoundingBox, 1..=8) => Double,
            (KeyValue, 1 | 2) => Binary,
            // TYPE_ORDER, then IEEE_754_TOTAL_ORDER, both empty structs.
            (ColumnOrder, 1 | 2) => Wire::Struct(Empty),
            (EncryptionAlgorithm, 1 | 2) => Wire::Struct(AesGcm),
            (AesGcm, 1 | 2) => Binary,
            (AesGcm, 3) => Bool,
            (ColumnCryptoMetaData, 1) => Wire::Struct(Empty),
            (ColumnCryptoMetaData, 2) => Wire::Struct(EncryptionWithColumnKey),
            (EncryptionWithColumnKey, 1) => List(&Binary),
            (EncryptionWithColumnKey, 2) => Binary,
            (PageHeader, 1..=4) => I32,
            (PageHeader, 5) => Wire::Struct(DataPageHeader),
            (PageHeader, 6) => Wire::Struct(Empty),
            (PageHeader, 7) => Wire::Struct(DictionaryPageHeader),
            (PageHeader, 8) => Wire::Struct(DataPageHeaderV2),
            (DataPageHeader, 1..=4) => I32,
            (DataPageHeader, 5) => Wire::Struct(Statistics),
            (DataPageHeaderV2, 1..=6) => I32,
            (DataPageHeaderV2, 7) => Bool,
            (DataPageHeaderV2, 8) => Wire::Struct(Statistics),
            (DictionaryPageHeader, 1 | 2) => I32,
            (DictionaryPageHeader, 3) => Bool,
            _ => return None,
        })
    }

    /// The fewest bytes the native decoder accepts for the first element of a
    /// list of this struct, and for each later element: the stop byte, and a
    /// one-byte header and the shortest value for each field it requires.
    fn minimum(self) -> (usize, usize) {
        use Shape::*;
        match self {
            // The required name (a header and an empty length), and from the
            // second element on the repetition type (a header and a one-byte
            // value), which the schema conversion requires of every element
            // but the root.
            SchemaElement => (3, 5),
            // The required columns (a header and an empty list header), byte
            // size and row count.
            RowGroup => (7, 7),
            // The required column index, and two required booleans that take
            // only their headers.
            SortingColumn => (5, 5),
            // The three required integers.
            PageEncodingStats => (7, 7),
            // The required key.
            KeyValue => (3, 3),
            // Exactly one variant, whose value can take no byte, as an unknown
            // boolean variant does. The decoder is shown any variant but
            // TYPE_ORDER as that empty struct, a byte longer (`Presentation`),
            // and reserves nothing for the list, whose elements hold nothing.
            ColumnOrder => (2, 2),
            // The required file offset. Framing refuses a chunk without the
            // metadata it locates pages by before the decoder sees the list.
            ColumnChunk => (3, 3),
            // The stop byte.
            _ => (1, 1),
        }
    }

    /// The most elements the known list field `id` declares in a file the
    /// engine reads, whatever its schema; `None` for a list bounded only by
    /// its bytes, or by the schema.
    fn most(self, id: i16) -> Option<usize> {
        match (self, id) {
            (Shape::FileMetaData, 5) | (Shape::ColumnMetaData, 8) => Some(KEY_VALUES),
            (Shape::ColumnMetaData, 13) => Some(closed_profile::encoding_statistics()),
            (Shape::GeospatialStatistics, 2) => Some(GEOSPATIAL_TYPES),
            // A level histogram holds one count for each level from zero to
            // the column's highest, which is below the schema depth that
            // admission allows; a column's path holds one name for each level
            // below the root.
            (Shape::SizeStatistics, 2 | 3)
            | (Shape::ColumnMetaData, 3)
            | (Shape::EncryptionWithColumnKey, 1) => Some(Limits::SEMANTIC_DEPTH),
            _ => None,
        }
    }
}

/// The declared lengths of the footer lists that admission bounds by the
/// schema's leaf count, which it knows only once it has admitted the schema.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Lists {
    /// How many column orders the footer declares, if it holds the list.
    pub column_orders: Option<usize>,
    /// The most sorting columns that any one row group declares.
    pub sorting_columns: usize,
}

impl Lists {
    /// Note that the known list field `id` of `shape` declares `count`
    /// elements, and refuse more than `Shape::most` allows.
    fn declare(&mut self, shape: Shape, id: i16, count: usize) -> Result<(), Error> {
        match (shape, id) {
            (Shape::FileMetaData, 7) => self.column_orders = Some(count),
            (Shape::RowGroup, 4) => self.sorting_columns = self.sorting_columns.max(count),
            _ if shape.most(id).is_some_and(|most| count > most) => {
                return Err(Error::Malformed);
            }
            _ => {}
        }
        Ok(())
    }
}

/// The bytes parquet 55's decoder reads in place of a held struct, where they
/// differ from it, for the differential harness. Offsets count from the
/// start of the struct.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Presentation {
    /// Each skipped binary whose bytes are not UTF-8: shown as zero bytes.
    pub opaque: Vec<Range<usize>>,
    /// Each range shown as other bytes, in order: a column order shown as
    /// the type-defined one, and a later logical type removed from its schema
    /// element. Only a footer holds them, so a page header keeps its length.
    pub splices: Vec<(Range<usize>, Vec<u8>)>,
}

#[cfg(test)]
impl Presentation {
    /// Show parquet 55's decoder `native`, the compact presentation of the
    /// held struct (`compact::native_metadata`), with these edits.
    pub(super) fn apply<'a>(
        &self,
        mut native: std::borrow::Cow<'a, [u8]>,
    ) -> std::borrow::Cow<'a, [u8]> {
        for range in &self.opaque {
            native.to_mut()[range.clone()].fill(0);
        }
        if self.splices.is_empty() {
            return native;
        }
        let mut shown = Vec::with_capacity(native.len());
        let mut copied = 0;
        for (range, bytes) in &self.splices {
            shown.extend_from_slice(&native[copied..range.start]);
            shown.extend_from_slice(bytes);
            copied = range.end;
        }
        shown.extend_from_slice(&native[copied..]);
        std::borrow::Cow::Owned(shown)
    }
}

/// What checking one held struct found.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Checked {
    /// The declared lengths of the lists that framing bounds by the schema.
    pub lists: Lists,
    /// How parquet 55's decoder is shown the struct.
    pub presentation: Presentation,
    /// Each schema element, by its position in the footer's schema list,
    /// whose variant, geometry or geography annotation the presentation
    /// removes.
    pub annotated: Vec<usize>,
}

/// What the walk has found so far, and how many schema elements it has met.
struct Walk {
    found: Checked,
    elements: usize,
}

/// One struct the walk read: how many fields it holds and the id of the
/// first, which together name a union's variant.
struct Walked {
    fields: usize,
    first: Option<i16>,
}

/// A `ColumnOrder` union holding TYPE_ORDER, the type-defined order.
const TYPE_ORDER: [u8; 3] = [0x1c, 0, 0];

/// Refuse a known field of the admitted struct `value`, read as `shape`,
/// whose held wire type is not the one the native decoder reads it with, a
/// repeated known field, and a known list too short for the elements it
/// declares or longer than its limit, at every depth the decoder reaches;
/// and find how parquet 55's decoder must be shown the struct. Each byte is
/// read once, but for the rare schema element that a later logical type
/// annotates.
pub(super) fn check(value: Value<'_>, shape: Shape) -> Result<Checked, Error> {
    let mut state = Walk {
        found: Checked::default(),
        elements: 0,
    };
    walk(&mut Cursor::new(value), shape, &mut state)?;
    Ok(state.found)
}

fn walk(cursor: &mut Cursor<'_>, shape: Shape, state: &mut Walk) -> Result<Walked, Error> {
    let start = cursor.position();
    // Schema elements are met only in the footer's schema list, in order.
    let element = matches!(shape, Shape::SchemaElement).then(|| {
        state.elements += 1;
        state.elements - 1
    });
    let mut walked = Walked {
        fields: 0,
        first: None,
    };
    let mut previous = 0;
    // One bit for each known field already read: every known id is between
    // 1 and 18.
    let mut read = 0u32;
    // Where a field holding a later logical type begins, and the variant,
    // until the next header shows how to remove the field; then the removal.
    let mut later: Option<(usize, i16)> = None;
    let mut removal: Option<(Range<usize>, Vec<u8>, i16)> = None;
    loop {
        let header = cursor.position();
        let next = cursor.field(previous)?;
        if let Some((from, variant)) = later.take() {
            removal = Some(match next {
                // The field that follows names its id afresh, in the long
                // form, which does not depend on the removed field's id.
                Some((id, kind)) => (from..cursor.position(), long_header(id, kind), variant),
                None => (from..header, Vec::new(), variant),
            });
        }
        let Some((id, kind)) = next else {
            break;
        };
        walked.fields += 1;
        walked.first.get_or_insert(id);
        previous = id;
        let Some(wire) = shape.field(id) else {
            skip_unknown(cursor, kind, &mut state.found.presentation.opaque)?;
            continue;
        };
        let bit = 1u32 << id;
        if !wire.admits(kind) || read & bit != 0 {
            return Err(Error::Malformed);
        }
        read |= bit;
        match wire {
            Wire::Struct(inner) => {
                let union = walk(cursor, inner, state)?;
                if matches!(shape, Shape::SchemaElement) && id == 10 && union.fields == 1 {
                    if let Some(variant @ 16..=18) = union.first {
                        later = Some((header, variant));
                    }
                }
            }
            Wire::List(element) => {
                let (held, count) = cursor.collection()?;
                // The native decoder reads no element of an empty list.
                if count != 0 && !element.admits(held) {
                    return Err(Error::Malformed);
                }
                state.found.lists.declare(shape, id, count)?;
                // It reserves room for every element before it reads one.
                // Refuse a count that the struct's remaining bytes, and then
                // the list's own bytes, cannot hold.
                let remaining = cursor.remaining();
                if !element.fits(count, remaining) {
                    return Err(Error::Malformed);
                }
                for _ in 0..count {
                    match *element {
                        Wire::Struct(inner) => {
                            walk(cursor, inner, state)?;
                        }
                        _ => cursor.skip_element(held)?,
                    }
                }
                if !element.fits(count, remaining - cursor.remaining()) {
                    return Err(Error::Malformed);
                }
            }
            _ => cursor.skip_value(kind)?,
        }
    }
    let presentation = &mut state.found.presentation;
    match shape {
        Shape::SchemaElement => {
            if let Some((range, bytes, variant)) = removal {
                if accepted(cursor.held(start..cursor.position()), variant)? {
                    presentation.splices.push((range, bytes));
                    state.found.annotated.extend(element);
                }
            }
        }
        // A union of one variant the native decoder does not know.
        Shape::ColumnOrder if walked.fields == 1 && walked.first != Some(1) => {
            let order = start..cursor.position();
            presentation.splices.push((order, TYPE_ORDER.to_vec()));
        }
        _ => {}
    }
    Ok(walked)
}

/// Skip the value, of wire type `kind`, of a field the native decoder does
/// not know, and note each binary in it whose bytes are not UTF-8 (in
/// `opaque`): parquet 55's skip reads each binary as text. A map or
/// UUID is shown to it as one opaque binary already
/// (`compact::native_metadata`), so nothing inside one is noted.
fn skip_unknown(
    cursor: &mut Cursor<'_>,
    kind: u8,
    opaque: &mut Vec<Range<usize>>,
) -> Result<(), Error> {
    match kind {
        8 => {
            let bytes = cursor.binary()?;
            if std::str::from_utf8(cursor.held(bytes.clone())).is_err() {
                opaque.push(bytes);
            }
        }
        9 | 10 => {
            let (element, count) = cursor.collection()?;
            for _ in 0..count {
                match element {
                    8..=10 | 12 => skip_unknown(cursor, element, opaque)?,
                    _ => cursor.skip_element(element)?,
                }
            }
        }
        12 => {
            let mut previous = 0;
            while let Some((id, kind)) = cursor.field(previous)? {
                previous = id;
                skip_unknown(cursor, kind, opaque)?;
            }
        }
        _ => cursor.skip_value(kind)?,
    }
    Ok(())
}

/// A field header in the long form: the wire type the native decoder skips
/// the field's value by (`compact::native_opaque`), then the id itself.
fn long_header(id: i16, kind: u8) -> Vec<u8> {
    let mut header = vec![compact::native_opaque(kind)];
    let mut value = ((i64::from(id) << 1) ^ (i64::from(id) >> 63)) as u64;
    while value >= 0x80 {
        header.push(value as u8 | 0x80);
        value >>= 7;
    }
    header.push(value as u8);
    header
}

/// Whether parquet 57 admitted the logical type `variant` (16, 17 or 18) of
/// the schema element held in `bytes`, so that a presentation without it
/// decodes to the type parquet 57 gave the element. Parquet 57 read the CRS of
/// a geometry or geography as text. On a primitive it admitted a geometry or
/// geography only on a `BYTE_ARRAY` without a converted type, and a variant
/// never; on a group it admitted each, and checked nothing.
pub(super) fn accepted(bytes: &[u8], variant: i16) -> Result<bool, Error> {
    let (element, _) = compact::parse(bytes)?;
    let annotation = match element.field(10)? {
        Some(union) => union.field(variant)?,
        None => None,
    };
    if variant != 16 {
        let crs = match annotation {
            Some(annotation) => annotation.field(1)?,
            None => None,
        };
        if let Some(Value::Binary(crs)) = crs {
            if std::str::from_utf8(crs).is_err() {
                return Ok(false);
            }
        }
    }
    let leaf = matches!(element.field(5)?, None | Some(Value::Integer(0)));
    let physical = element.field(1)?;
    if !leaf || physical.is_none() {
        return Ok(true);
    }
    Ok(variant != 16 && physical == Some(Value::Integer(6)) && element.field(6)?.is_none())
}

#[cfg(test)]
mod tests {
    use super::super::{framing, reader};
    use arrow_array::{Int64Array, TimestampMicrosecondArray};
    use parquet::file::metadata::ParquetMetaData;

    fn varint(mut value: u64, output: &mut Vec<u8>) {
        while value >= 0x80 {
            output.push(value as u8 | 0x80);
            value >>= 7;
        }
        output.push(value as u8);
    }

    fn zigzag(value: i64) -> Vec<u8> {
        let mut bytes = Vec::new();
        varint(((value << 1) ^ (value >> 63)) as u64, &mut bytes);
        bytes
    }

    /// A Compact Protocol struct, written one field at a time.
    #[derive(Default)]
    struct Fields {
        bytes: Vec<u8>,
        last: i16,
    }

    impl Fields {
        /// A field header of wire type `kind`, then `value` as given.
        fn raw(mut self, id: i16, kind: u8, value: &[u8]) -> Self {
            match i32::from(id) - i32::from(self.last) {
                delta @ 1..=15 => self.bytes.push((delta as u8) << 4 | kind),
                _ => {
                    self.bytes.push(kind);
                    self.bytes.extend(zigzag(id.into()));
                }
            }
            self.last = id;
            self.bytes.extend(value);
            self
        }

        fn i32(self, id: i16, value: i64) -> Self {
            self.raw(id, 5, &zigzag(value))
        }

        fn i64(self, id: i16, value: i64) -> Self {
            self.raw(id, 6, &zigzag(value))
        }

        fn double(self, id: i16, value: f64) -> Self {
            self.raw(id, 7, &value.to_le_bytes())
        }

        fn bool(self, id: i16, value: bool) -> Self {
            self.raw(id, if value { 1 } else { 2 }, &[])
        }

        fn binary(self, id: i16, value: &[u8]) -> Self {
            let mut bytes = Vec::new();
            varint(value.len() as u64, &mut bytes);
            bytes.extend(value);
            self.raw(id, 8, &bytes)
        }

        fn uuid(self, id: i16) -> Self {
            self.raw(id, 13, &[0x75; 16])
        }

        fn strct(self, id: i16, value: Fields) -> Self {
            self.raw(id, 12, &value.end())
        }

        /// A list of encoded elements of wire type `element`.
        fn list(self, id: i16, element: u8, items: &[Vec<u8>]) -> Self {
            self.collection(id, 9, element, items)
        }

        /// A list (`kind` 9) or a set (`kind` 10) of at most 14 elements.
        fn collection(self, id: i16, kind: u8, element: u8, items: &[Vec<u8>]) -> Self {
            assert!(items.len() < 15);
            let mut bytes = vec![(items.len() as u8) << 4 | element];
            bytes.extend(items.concat());
            self.raw(id, kind, &bytes)
        }

        /// The first form when `wrong`, the second otherwise.
        fn pick(self, wrong: bool, mistyped: fn(Self) -> Self, typed: fn(Self) -> Self) -> Self {
            if wrong {
                mistyped(self)
            } else {
                typed(self)
            }
        }

        /// The fields `field` adds, twice when `repeat`.
        fn twice(self, repeat: bool, field: fn(Self) -> Self) -> Self {
            let once = field(self);
            if repeat {
                field(once)
            } else {
                once
            }
        }

        fn end(mut self) -> Vec<u8> {
            self.bytes.push(0);
            self.bytes
        }
    }

    /// Where a case adds fields: after the fields of one struct of `file`.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum At {
        File,
        Leaf,
        Group,
        Chunk,
        Meta,
        Page,
        /// The data page header writes all of its known fields, so here a
        /// case writes the last one, its repetition level encoding, itself.
        Data,
    }

    /// The fields one case adds, and where.
    type Addition = (At, fn(Fields) -> Fields);

    /// An independently authored file: one REQUIRED INT64 column `V` holding
    /// `values` in one uncompressed PLAIN V1 data page, with only the fields
    /// parquet requires. `extra` adds fields to the struct `at` names.
    fn file(values: &[i64], at: At, extra: impl Fn(Fields) -> Fields) -> Vec<u8> {
        let add = |here: At, fields: Fields| if here == at { extra(fields) } else { fields };
        let rows = values.len() as i64;
        let body: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let size = body.len() as i64;
        // num_values; PLAIN; RLE definition and repetition levels.
        let data = Fields::default().i32(1, rows).i32(2, 0).i32(3, 3);
        let data = if at == At::Data {
            extra(data)
        } else {
            data.i32(4, 3)
        };
        // DATA_PAGE; uncompressed and compressed sizes.
        let page = Fields::default().i32(1, 0).i32(2, size).i32(3, size);
        let page = add(At::Page, page.strct(5, data)).end();
        let chunk = (page.len() + body.len()) as i64;
        let meta = Fields::default()
            .i32(1, 2) // INT64
            .list(2, 5, &[zigzag(0)]) // encodings: PLAIN
            .list(3, 8, &[b"\x01V".to_vec()]) // path_in_schema
            .i32(4, 0) // UNCOMPRESSED
            .i64(5, rows)
            .i64(6, chunk)
            .i64(7, chunk)
            .i64(9, 4); // data_page_offset
        let column = Fields::default().i64(2, 0).strct(3, add(At::Meta, meta));
        let group = Fields::default().list(1, 12, &[add(At::Chunk, column).end()]);
        let group = add(At::Group, group.i64(2, chunk).i64(3, rows)).end();
        let root = Fields::default().binary(4, b"schema").i32(5, 1).end();
        // INT64; REQUIRED; name.
        let leaf = add(
            At::Leaf,
            Fields::default().i32(1, 2).i32(3, 0).binary(4, b"V"),
        )
        .end();
        let footer = Fields::default().i32(1, 1).list(2, 12, &[root, leaf]);
        let footer = add(At::File, footer.i64(3, rows).list(4, 12, &[group])).end();
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(page);
        bytes.extend(body);
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        bytes
    }

    fn malformed(bytes: &[u8]) -> bool {
        matches!(framing::admit(bytes), Err(framing::Error::Malformed))
    }

    fn timestamp(unit: Fields) -> Fields {
        Fields::default().strct(8, Fields::default().bool(1, false).strct(2, unit))
    }

    fn integer() -> Fields {
        Fields::default().raw(1, 3, &[64]).bool(2, true)
    }

    fn bounding_box() -> Fields {
        let corners = Fields::default().double(1, 0.0).double(2, 1.0);
        corners.double(3, 0.0).double(4, 1.0)
    }

    #[test]
    fn the_authored_file_reads_through_admission_and_the_native_decoder() {
        let decoded = reader::read(&file(&[7, -1], At::File, |fields| fields)).unwrap();
        let column = decoded.batches[0].column(0);
        let values = column.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(values.values(), &[7, -1]);
    }

    /// For each Thrift struct the native decoder reads, one known field with
    /// another wire type is malformed, and the same field with its declared
    /// wire type is admitted.
    #[test]
    fn a_mistyped_known_field_of_each_struct_is_malformed() {
        type Case = (&'static str, At, fn(Fields, bool) -> Fields);
        let cases: [Case; 38] = [
            // created_by as a UUID, which the native copy presents as a binary.
            ("FileMetaData", At::File, |f, wrong| {
                f.pick(wrong, |f| f.uuid(6), |f| f.binary(6, b"yamaa test"))
            }),
            ("FileMetaData", At::File, |f, wrong| {
                f.pick(wrong, |f| f.i32(9, 0), |f| f.binary(9, b"key"))
            }),
            // AES_GCM_V1 as an empty binary, one byte like an empty struct.
            ("EncryptionAlgorithm", At::File, |f, wrong| {
                let union = Fields::default().pick(
                    wrong,
                    |u| u.binary(1, b""),
                    |u| u.strct(1, Fields::default()),
                );
                f.strct(8, union)
            }),
            ("AesGcm", At::File, |f, wrong| {
                let aes = Fields::default().pick(wrong, |a| a.i32(3, 0), |a| a.bool(3, false));
                f.strct(8, Fields::default().strct(2, aes))
            }),
            ("SchemaElement", At::Leaf, |f, wrong| {
                f.pick(wrong, |f| f.binary(9, b"7"), |f| f.i32(9, 7))
            }),
            // The INTEGER variant as a binary holding an IntType.
            ("LogicalType", At::Leaf, |f, wrong| {
                let union = Fields::default();
                let union = union.pick(
                    wrong,
                    |u| u.binary(10, &integer().end()),
                    |u| u.strct(10, integer()),
                );
                f.strct(10, union)
            }),
            ("DecimalType", At::Leaf, |f, wrong| {
                let decimal = Fields::default().pick(wrong, |d| d.i64(1, 2), |d| d.i32(1, 2));
                f.strct(10, Fields::default().strct(5, decimal.i32(2, 10)))
            }),
            ("TimestampType", At::Leaf, |f, wrong| {
                let unit = Fields::default().strct(2, Fields::default());
                let time = Fields::default().pick(wrong, |t| t.i32(1, 0), |t| t.bool(1, false));
                f.strct(10, Fields::default().strct(8, time.strct(2, unit)))
            }),
            // MICROS as an empty binary, one byte like an empty struct.
            ("TimeUnit", At::Leaf, |f, wrong| {
                let unit = Fields::default().pick(
                    wrong,
                    |u| u.binary(2, b""),
                    |u| u.strct(2, Fields::default()),
                );
                f.strct(10, timestamp(unit))
            }),
            ("IntType", At::Leaf, |f, wrong| {
                let int = Fields::default().pick(wrong, |i| i.i32(1, 64), |i| i.raw(1, 3, &[64]));
                f.strct(10, Fields::default().strct(10, int.bool(2, true)))
            }),
            ("IntType", At::Leaf, |f, wrong| {
                let int = Fields::default().raw(1, 3, &[64]);
                let int = int.pick(wrong, |i| i.i32(2, 1), |i| i.bool(2, true));
                f.strct(10, Fields::default().strct(10, int))
            }),
            ("VariantType", At::Leaf, |f, wrong| {
                let variant = Fields::default().pick(wrong, |v| v.i32(1, 1), |v| v.raw(1, 3, &[1]));
                f.strct(10, Fields::default().strct(16, variant))
            }),
            ("GeometryType", At::Leaf, |f, wrong| {
                let crs = Fields::default().pick(wrong, |g| g.i32(1, 0), |g| g.binary(1, b"crs"));
                f.strct(10, Fields::default().strct(17, crs))
            }),
            ("GeographyType", At::Leaf, |f, wrong| {
                let edges = Fields::default().pick(wrong, |g| g.binary(2, b"1"), |g| g.i32(2, 1));
                f.strct(10, Fields::default().strct(18, edges))
            }),
            ("RowGroup", At::Group, |f, wrong| {
                f.pick(wrong, |g| g.binary(5, b"4"), |g| g.i64(5, 4))
            }),
            ("RowGroup", At::Group, |f, wrong| {
                f.pick(wrong, |g| g.binary(6, b"0"), |g| g.i64(6, 0))
            }),
            ("SortingColumn", At::Group, |f, wrong| {
                let column = Fields::default().i32(1, 0);
                let column = column.pick(wrong, |s| s.i32(2, 0), |s| s.bool(2, false));
                f.list(4, 12, &[column.bool(3, true).end()])
            }),
            // offset_index_length as a UUID.
            ("ColumnChunk", At::Chunk, |f, wrong| {
                f.pick(wrong, |c| c.uuid(5), |c| c.i32(5, 16))
            }),
            ("ColumnChunk", At::Chunk, |f, wrong| {
                f.pick(wrong, |c| c.i32(9, 0), |c| c.binary(9, b"sealed"))
            }),
            // ENCRYPTION_WITH_FOOTER_KEY as an empty binary.
            ("ColumnCryptoMetaData", At::Chunk, |f, wrong| {
                let union = Fields::default().pick(
                    wrong,
                    |u| u.binary(1, b""),
                    |u| u.strct(1, Fields::default()),
                );
                f.strct(8, union)
            }),
            // The column key's path as one binary rather than a list.
            ("EncryptionWithColumnKey", At::Chunk, |f, wrong| {
                let key = Fields::default().pick(
                    wrong,
                    |k| k.binary(1, b"V"),
                    |k| k.list(1, 8, &[b"\x01V".to_vec()]),
                );
                f.strct(8, Fields::default().strct(2, key))
            }),
            ("ColumnMetaData", At::Meta, |f, wrong| {
                f.pick(wrong, |m| m.binary(14, b"0"), |m| m.i64(14, 0))
            }),
            // key_value_metadata holding binaries rather than entries.
            ("ColumnMetaData", At::Meta, |f, wrong| {
                f.pick(
                    wrong,
                    |m| m.list(8, 8, &[b"\x01k".to_vec()]),
                    |m| m.list(8, 12, &[Fields::default().binary(1, b"k").end()]),
                )
            }),
            ("Statistics", At::Meta, |f, wrong| {
                let stats = Fields::default().i64(3, 0);
                f.strct(12, stats.pick(wrong, |s| s.i32(7, 0), |s| s.bool(7, true)))
            }),
            ("PageEncodingStats", At::Meta, |f, wrong| {
                let stats = Fields::default().i32(1, 0).i32(2, 0);
                let stats = stats.pick(wrong, |s| s.binary(3, b"1"), |s| s.i32(3, 1));
                f.list(13, 12, &[stats.end()])
            }),
            ("SizeStatistics", At::Meta, |f, wrong| {
                let size = Fields::default().pick(wrong, |s| s.binary(1, b""), |s| s.i64(1, 0));
                f.strct(16, size)
            }),
            // geospatial_types holding binaries rather than i32 values.
            ("GeospatialStatistics", At::Meta, |f, wrong| {
                let geo = Fields::default().pick(
                    wrong,
                    |g| g.list(2, 8, &[b"\x011".to_vec()]),
                    |g| g.list(2, 5, &[zigzag(1)]),
                );
                f.strct(17, geo)
            }),
            ("BoundingBox", At::Meta, |f, wrong| {
                let xmin = Fields::default().pick(wrong, |b| b.i64(1, 0), |b| b.double(1, 0.0));
                let corners = xmin.double(2, 1.0).double(3, 0.0).double(4, 1.0);
                f.strct(17, Fields::default().strct(1, corners))
            }),
            ("KeyValue", At::File, |f, wrong| {
                let pair = Fields::default().pick(wrong, |k| k.i32(1, 0), |k| k.binary(1, b"k"));
                f.list(5, 12, &[pair.binary(2, b"v").end()])
            }),
            // TYPE_ORDER as an i32 zero, one byte like an empty struct.
            ("ColumnOrder", At::File, |f, wrong| {
                let order = Fields::default().pick(
                    wrong,
                    |o| o.i32(1, 0),
                    |o| o.strct(1, Fields::default()),
                );
                f.list(7, 12, &[order.end()])
            }),
            // IEEE_754_TOTAL_ORDER as an i32 zero.
            ("ColumnOrder", At::File, |f, wrong| {
                let order = Fields::default().pick(
                    wrong,
                    |o| o.i32(2, 0),
                    |o| o.strct(2, Fields::default()),
                );
                f.list(7, 12, &[order.end()])
            }),
            ("PageHeader", At::Page, |f, wrong| {
                f.pick(wrong, |p| p.binary(4, b"crc"), |p| p.i32(4, 0))
            }),
            // RLE repetition levels as a raw byte 3, which admission would
            // read as 3 and the native decoder as the varint -2.
            ("DataPageHeader", At::Data, |f, wrong| {
                f.pick(wrong, |d| d.raw(4, 3, &[3]), |d| d.i32(4, 3))
            }),
            // The page's statistics as an empty binary.
            ("DataPageHeader", At::Data, |f, wrong| {
                let f = f.i32(4, 3);
                f.pick(
                    wrong,
                    |d| d.binary(5, b""),
                    |d| d.strct(5, Fields::default()),
                )
            }),
            // A V2 header the native decoder reads although the page is V1.
            ("DataPageHeaderV2", At::Page, |f, wrong| {
                let v2 = Fields::default().i32(1, 1).i32(2, 0);
                let v2 = v2.pick(wrong, |v| v.binary(3, b"1"), |v| v.i32(3, 1));
                f.strct(8, v2.i32(4, 0).i32(5, 0).i32(6, 0))
            }),
            ("DataPageHeaderV2", At::Page, |f, wrong| {
                let v2 = Fields::default().i32(1, 1).i32(2, 0).i32(3, 1).i32(4, 0);
                let v2 = v2.i32(5, 0).i32(6, 0).pick(
                    wrong,
                    |v| v.binary(8, b""),
                    |v| v.strct(8, Fields::default()),
                );
                f.strct(8, v2)
            }),
            ("DictionaryPageHeader", At::Page, |f, wrong| {
                let dictionary = Fields::default().i32(1, 0).i32(2, 0);
                let dictionary = dictionary.pick(wrong, |d| d.i32(3, 0), |d| d.bool(3, false));
                f.strct(7, dictionary)
            }),
            // An IndexPageHeader holds no field the native decoder reads.
            ("IndexPageHeader", At::Page, |f, wrong| {
                f.pick(
                    wrong,
                    |p| p.binary(6, b""),
                    |p| p.strct(6, Fields::default().binary(1, b"any")),
                )
            }),
        ];
        for (name, at, add) in cases {
            let refused = file(&[7], at, |fields| add(fields, true));
            assert!(malformed(&refused), "{name}: mistyped");
            let admitted = file(&[7], at, |fields| add(fields, false));
            framing::admit(&admitted).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }

    /// For each Thrift struct the native decoder reads, a known field written
    /// twice is malformed, and the same field written once is admitted.
    #[test]
    fn a_repeated_known_field_of_each_struct_is_malformed() {
        type Case = (&'static str, At, fn(Fields, bool) -> Fields);
        let cases: [Case; 32] = [
            ("FileMetaData", At::File, |f, repeat| {
                f.twice(repeat, |f| f.binary(6, b"yamaa test"))
            }),
            ("EncryptionAlgorithm", At::File, |f, repeat| {
                let union = Fields::default().twice(repeat, |u| u.strct(1, Fields::default()));
                f.strct(8, union)
            }),
            ("AesGcm", At::File, |f, repeat| {
                let aes = Fields::default().twice(repeat, |a| a.binary(1, b"aad"));
                f.strct(8, Fields::default().strct(1, aes))
            }),
            ("SchemaElement", At::Leaf, |f, repeat| {
                f.twice(repeat, |f| f.i32(9, 7))
            }),
            ("LogicalType", At::Leaf, |f, repeat| {
                let union = Fields::default().twice(repeat, |u| u.strct(10, integer()));
                f.strct(10, union)
            }),
            ("DecimalType", At::Leaf, |f, repeat| {
                let decimal = Fields::default().twice(repeat, |d| d.i32(1, 2));
                f.strct(10, Fields::default().strct(5, decimal.i32(2, 10)))
            }),
            ("TimestampType", At::Leaf, |f, repeat| {
                let unit = Fields::default().strct(2, Fields::default());
                let time = Fields::default().twice(repeat, |t| t.bool(1, false));
                f.strct(10, Fields::default().strct(8, time.strct(2, unit)))
            }),
            ("TimeUnit", At::Leaf, |f, repeat| {
                let unit = Fields::default().twice(repeat, |u| u.strct(2, Fields::default()));
                f.strct(10, timestamp(unit))
            }),
            ("IntType", At::Leaf, |f, repeat| {
                let int = Fields::default().twice(repeat, |i| i.raw(1, 3, &[64]));
                f.strct(10, Fields::default().strct(10, int.bool(2, true)))
            }),
            ("VariantType", At::Leaf, |f, repeat| {
                let variant = Fields::default().twice(repeat, |v| v.raw(1, 3, &[1]));
                f.strct(10, Fields::default().strct(16, variant))
            }),
            ("GeometryType", At::Leaf, |f, repeat| {
                let crs = Fields::default().twice(repeat, |g| g.binary(1, b"crs"));
                f.strct(10, Fields::default().strct(17, crs))
            }),
            ("GeographyType", At::Leaf, |f, repeat| {
                let edges = Fields::default().twice(repeat, |g| g.i32(2, 1));
                f.strct(10, Fields::default().strct(18, edges))
            }),
            ("RowGroup", At::Group, |f, repeat| {
                f.twice(repeat, |g| g.i64(5, 4))
            }),
            ("SortingColumn", At::Group, |f, repeat| {
                let column = Fields::default().twice(repeat, |s| s.i32(1, 0));
                f.list(4, 12, &[column.bool(2, false).bool(3, true).end()])
            }),
            ("ColumnChunk", At::Chunk, |f, repeat| {
                f.twice(repeat, |c| c.i32(5, 16))
            }),
            ("ColumnCryptoMetaData", At::Chunk, |f, repeat| {
                let union = Fields::default().twice(repeat, |u| u.strct(1, Fields::default()));
                f.strct(8, union)
            }),
            ("EncryptionWithColumnKey", At::Chunk, |f, repeat| {
                let key = Fields::default().twice(repeat, |k| k.binary(2, b"key"));
                let key = key.list(1, 8, &[b"\x01V".to_vec()]);
                f.strct(8, Fields::default().strct(2, key))
            }),
            ("ColumnMetaData", At::Meta, |f, repeat| {
                f.twice(repeat, |m| m.i64(14, 0))
            }),
            ("ColumnMetaData", At::Meta, |f, repeat| {
                f.twice(repeat, |m| {
                    m.list(8, 12, &[Fields::default().binary(1, b"k").end()])
                })
            }),
            ("Statistics", At::Meta, |f, repeat| {
                let stats = Fields::default().twice(repeat, |s| s.i64(3, 0));
                f.strct(12, stats)
            }),
            ("PageEncodingStats", At::Meta, |f, repeat| {
                let stats = Fields::default().i32(1, 0).i32(2, 0);
                let stats = stats.twice(repeat, |s| s.i32(3, 1));
                f.list(13, 12, &[stats.end()])
            }),
            ("SizeStatistics", At::Meta, |f, repeat| {
                let size = Fields::default().twice(repeat, |s| s.i64(1, 0));
                f.strct(16, size)
            }),
            ("GeospatialStatistics", At::Meta, |f, repeat| {
                let geo = Fields::default().twice(repeat, |g| g.list(2, 5, &[zigzag(1)]));
                f.strct(17, geo)
            }),
            ("BoundingBox", At::Meta, |f, repeat| {
                let xmin = Fields::default().twice(repeat, |b| b.double(1, 0.0));
                let corners = xmin.double(2, 1.0).double(3, 0.0).double(4, 1.0);
                f.strct(17, Fields::default().strct(1, corners))
            }),
            ("KeyValue", At::File, |f, repeat| {
                let pair = Fields::default().twice(repeat, |k| k.binary(1, b"k"));
                f.list(5, 12, &[pair.binary(2, b"v").end()])
            }),
            ("ColumnOrder", At::File, |f, repeat| {
                let order = Fields::default().twice(repeat, |o| o.strct(1, Fields::default()));
                f.list(7, 12, &[order.end()])
            }),
            ("ColumnOrder", At::File, |f, repeat| {
                let order = Fields::default().twice(repeat, |o| o.strct(2, Fields::default()));
                f.list(7, 12, &[order.end()])
            }),
            ("PageHeader", At::Page, |f, repeat| {
                f.twice(repeat, |p| p.i32(4, 0))
            }),
            ("DataPageHeader", At::Data, |f, repeat| {
                f.twice(repeat, |d| d.i32(4, 3))
            }),
            ("DataPageHeader", At::Data, |f, repeat| {
                f.i32(4, 3).twice(repeat, |d| d.strct(5, Fields::default()))
            }),
            ("DataPageHeaderV2", At::Page, |f, repeat| {
                let v2 = Fields::default().twice(repeat, |v| v.i32(1, 1));
                let v2 = v2.i32(2, 0).i32(3, 1).i32(4, 0);
                f.strct(8, v2.i32(5, 0).i32(6, 0))
            }),
            ("DictionaryPageHeader", At::Page, |f, repeat| {
                let dictionary = Fields::default().i32(1, 0).i32(2, 0);
                f.strct(7, dictionary.twice(repeat, |d| d.bool(3, false)))
            }),
        ];
        for (name, at, add) in cases {
            let refused = file(&[7], at, |fields| add(fields, true));
            assert!(malformed(&refused), "{name}: repeated");
            let admitted = file(&[7], at, |fields| add(fields, false));
            framing::admit(&admitted).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }

    /// parquet 57.3.0 decoded the first copy of a repeated schema, and merged
    /// the copies of a column chunk's metadata, while admission read the
    /// last copy. Neither copy is read once the field is refused.
    #[test]
    fn a_repeated_schema_or_column_metadata_is_malformed() {
        // A chain of nested groups whose leaf lies one level deeper than
        // admission allows: the root, REQUIRED groups with an empty name and
        // one child each, and an INT64 leaf.
        let depth = super::Limits::SEMANTIC_DEPTH + 1;
        let root = Fields::default().binary(4, b"").i32(5, 1).end();
        let group = Fields::default().i32(3, 0).binary(4, b"").i32(5, 1).end();
        let mut deep = vec![0xfc];
        varint(depth as u64, &mut deep);
        deep.extend(&root);
        for _ in 2..depth {
            deep.extend(&group);
        }
        deep.extend([0x15, 4, 0x25, 0, 0x18, 0, 0]);
        let rows = |f: Fields| f.raw(4, 9, &[0x0c]);
        assert!(malformed(&columnless(&deep, rows)));
        framing::admit(&columnless(&leaves(1), rows)).unwrap();
        let first_unchecked = columnless(&deep, |f| rows(f).raw(2, 9, &leaves(1)));
        assert!(malformed(&first_unchecked));
        // The column chunk's metadata once more, with a dictionary page
        // offset, as framing would read it.
        let held = file(&[7], At::File, |f| f);
        let trailer = held.len() - 8;
        let footer = u32::from_le_bytes(held[trailer..trailer + 4].try_into().unwrap());
        let chunk = (trailer - footer as usize - 4) as i64;
        let repeated = file(&[7], At::Chunk, move |f| {
            let meta = Fields::default()
                .i32(1, 2)
                .list(2, 5, &[zigzag(0)])
                .list(3, 8, &[b"\x01V".to_vec()])
                .i32(4, 0)
                .i64(5, 1)
                .i64(6, chunk)
                .i64(7, chunk)
                .i64(9, 4)
                .i64(11, 4);
            f.strct(3, meta)
        });
        assert!(malformed(&repeated));
        assert_eq!(
            reader::read(&repeated).unwrap_err(),
            reader::Error::Malformed
        );
    }

    /// Each list that admission caps declares at most as many elements as a
    /// file the engine reads holds, and one more is malformed.
    #[test]
    fn a_list_longer_than_any_file_the_engine_reads_is_malformed() {
        use super::{closed_profile, Limits, GEOSPATIAL_TYPES, KEY_VALUES};
        let list = |count, element: &[u8]| repeated(12, count, element, element);
        let entries = move |count| list(count, &[0x18, 0, 0]);
        let stats = move |count| list(count, &[0x15, 0, 0x15, 0, 0x15, 2, 0]);
        // Zero-valued i64 counts.
        let levels = |count| repeated(6, count, &[0], &[0]);
        // Geospatial statistics listing `count` codes, each 1 (a point).
        let geospatial = |count| Fields::default().raw(2, 9, &repeated(5, count, &[2], &[2]));
        // A column key's path of `count` empty names.
        let names = |count| repeated(8, count, &[0], &[0]);
        type Case = (&'static str, usize, Box<dyn Fn(usize) -> Vec<u8>>);
        let cases: [Case; 9] = [
            (
                "key_value_metadata",
                KEY_VALUES,
                Box::new(move |n| file(&[7], At::File, |f| f.raw(5, 9, &entries(n)))),
            ),
            (
                "column key_value_metadata",
                KEY_VALUES,
                Box::new(move |n| file(&[7], At::Meta, |f| f.raw(8, 9, &entries(n)))),
            ),
            (
                "column key path_in_schema",
                Limits::SEMANTIC_DEPTH,
                Box::new(move |n| {
                    let key = |f: Fields| f.raw(1, 9, &names(n));
                    let crypto = |f: Fields| f.strct(2, key(Fields::default()));
                    file(&[7], At::Chunk, |f| f.strct(8, crypto(Fields::default())))
                }),
            ),
            (
                "encoding_stats",
                closed_profile::encoding_statistics(),
                Box::new(move |n| file(&[7], At::Meta, |f| f.raw(13, 9, &stats(n)))),
            ),
            (
                "geospatial_types",
                GEOSPATIAL_TYPES,
                Box::new(move |n| file(&[7], At::Meta, |f| f.strct(17, geospatial(n)))),
            ),
            (
                "repetition_level_histogram",
                Limits::SEMANTIC_DEPTH,
                Box::new(move |n| {
                    let size = |f: Fields| f.raw(2, 9, &levels(n));
                    file(&[7], At::Meta, |f| f.strct(16, size(Fields::default())))
                }),
            ),
            (
                "definition_level_histogram",
                Limits::SEMANTIC_DEPTH,
                Box::new(move |n| {
                    let size = |f: Fields| f.raw(3, 9, &levels(n));
                    file(&[7], At::Meta, |f| f.strct(16, size(Fields::default())))
                }),
            ),
            // As many as the schema has leaves, here two.
            (
                "column_orders",
                2,
                Box::new(move |n| {
                    let orders = list(n, &[0x1c, 0, 0]);
                    columnless(&leaves(2), |f| f.raw(4, 9, &[0x0c]).raw(7, 9, &orders))
                }),
            ),
            (
                "sorting_columns",
                2,
                Box::new(move |n| {
                    let sorting = list(n, &[0x15, 0, 0x11, 0x11, 0]);
                    columnless(&leaves(2), |f| f.raw(4, 9, &sorted(&sorting)))
                }),
            ),
        ];
        for (name, most, file) in cases {
            framing::admit(&file(most)).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert!(malformed(&file(most + 1)), "{name}");
        }
        // A column's path, which the authored file writes once, checked here
        // within its metadata alone.
        let path = |count| {
            let meta = Fields::default().raw(3, 9, &names(count)).end();
            let (value, _) = super::super::compact::parse(&meta).unwrap();
            super::check(value, super::Shape::ColumnMetaData).map(drop)
        };
        assert_eq!(path(Limits::SEMANTIC_DEPTH), Ok(()));
        assert_eq!(
            path(Limits::SEMANTIC_DEPTH + 1),
            Err(super::Error::Malformed)
        );
        // Each leaf has exactly one column order.
        let orders = list(1, &[0x1c, 0, 0]);
        let fewer = columnless(&leaves(2), |f| f.raw(4, 9, &[0x0c]).raw(7, 9, &orders));
        assert!(malformed(&fewer));
        // The key-value and geospatial caps are the column limit, and a file
        // listing that many entries or types reads.
        assert_eq!(KEY_VALUES, Limits::COLUMNS);
        let keys = file(&[7], At::File, |f| f.raw(5, 9, &entries(KEY_VALUES)));
        assert_eq!(reader::read(&keys).unwrap().batches[0].num_rows(), 1);
        assert_eq!(GEOSPATIAL_TYPES, Limits::COLUMNS);
        let types = file(&[7], At::Meta, |f| {
            f.strct(17, geospatial(GEOSPATIAL_TYPES))
        });
        assert_eq!(reader::read(&types).unwrap().batches[0].num_rows(), 1);
    }

    /// `walk` gives each known field id one bit of a `u32`.
    #[test]
    fn every_known_field_id_has_a_bit_of_its_own() {
        use super::Shape::{self, *};
        let shapes: [Shape; 30] = [
            FileMetaData,
            SchemaElement,
            LogicalType,
            DecimalType,
            TimestampType,
            TimeUnit,
            IntType,
            VariantType,
            GeometryType,
            GeographyType,
            RowGroup,
            SortingColumn,
            ColumnChunk,
            ColumnMetaData,
            Statistics,
            PageEncodingStats,
            SizeStatistics,
            GeospatialStatistics,
            BoundingBox,
            KeyValue,
            ColumnOrder,
            EncryptionAlgorithm,
            AesGcm,
            ColumnCryptoMetaData,
            EncryptionWithColumnKey,
            PageHeader,
            DataPageHeader,
            DataPageHeaderV2,
            DictionaryPageHeader,
            Empty,
        ];
        for shape in shapes {
            for id in i16::MIN..=i16::MAX {
                if shape.field(id).is_some() {
                    assert!((1..32).contains(&id), "{shape:?} {id}");
                }
            }
        }
    }

    #[test]
    fn sets_maps_element_types_and_every_repeat_of_a_known_field_are_checked() {
        let pair = || Fields::default().binary(1, b"k").binary(2, b"v").end();
        let cases: [Addition; 4] = [
            // A set where the schema declares a list: the native copy would
            // present it as the identically encoded list.
            (At::File, |f| {
                f.collection(5, 10, 12, &[Fields::default().end()])
            }),
            // A map, which no known field declares, as file_path.
            (At::Chunk, |f| f.raw(1, 11, &[0])),
            // key_value_metadata declaring binary elements.
            (At::File, |f| f.list(5, 8, &[b"\x00".to_vec()])),
            // A mistyped first crc, then a well-typed last one.
            (At::Page, |f| f.binary(4, b"").i32(4, 0)),
        ];
        for (index, (at, add)) in cases.into_iter().enumerate() {
            assert!(malformed(&file(&[7], at, add)), "case {index}");
        }
        let well_typed_pairs = file(&[7], At::File, |f| f.list(5, 12, &[pair(), pair()]));
        framing::admit(&well_typed_pairs).unwrap();
        // The native decoder reads no element of an empty list and skips an
        // unknown field, or an unknown union variant, by its own wire type.
        let admitted: [Addition; 4] = [
            (At::File, |f| f.list(5, 8, &[])),
            (At::Group, |f| f.list(4, 0, &[])),
            (At::Meta, |f| {
                let set = f.uuid(90).collection(91, 10, 5, &[zigzag(1)]);
                set.raw(92, 11, &[0]).binary(93, b"an unknown binary")
            }),
            (At::Leaf, |f| {
                f.strct(10, Fields::default().binary(9, b"reserved"))
            }),
        ];
        for (index, (at, add)) in admitted.into_iter().enumerate() {
            framing::admit(&file(&[7], at, add)).unwrap_or_else(|e| panic!("{index}: {e:?}"));
        }
    }

    /// The native copy of the footer presents a UUID or a map as a binary of
    /// the same length and a set as the identically encoded list. Under a
    /// known binary or list id, the presented footer passes the check, so the
    /// reader must check the held footer before it presents it. No page
    /// header field the native reader reads is a binary or a list.
    #[test]
    fn the_footer_is_checked_as_held_before_its_native_presentation() {
        use super::super::compact;
        use super::{check, Error, Shape};
        let checked =
            |footer: &[u8]| check(compact::parse(footer)?.0, Shape::FileMetaData).map(drop);
        let cases: [fn(Fields) -> Fields; 3] = [
            // created_by as a UUID.
            |f| f.uuid(6),
            // key_value_metadata as a set.
            |f| {
                let pair = Fields::default().binary(1, b"k").binary(2, b"v");
                f.collection(5, 10, 12, &[pair.end()])
            },
            // A key-value value as an empty map.
            |f| {
                let pair = Fields::default().binary(1, b"k").raw(2, 11, &[0]);
                f.list(5, 12, &[pair.end()])
            },
        ];
        for (index, add) in cases.into_iter().enumerate() {
            let bytes = file(&[7], At::File, add);
            let trailer = bytes.len() - 8;
            let length = u32::from_le_bytes(bytes[trailer..trailer + 4].try_into().unwrap());
            let held = &bytes[trailer - length as usize..trailer];
            let presented = compact::native_metadata(held).unwrap();
            assert_eq!(checked(held), Err(Error::Malformed), "case {index}: held");
            assert_eq!(checked(&presented), Ok(()), "case {index}: presented");
            let error = reader::read(&bytes).unwrap_err();
            assert_eq!(error, reader::Error::Malformed, "case {index}: read");
        }
    }

    #[test]
    fn well_typed_optional_fields_still_read() {
        let bytes = file(&[0, 1_000_000], At::Leaf, |f| {
            f.strct(10, timestamp(Fields::default().strct(2, Fields::default())))
        });
        let decoded = reader::read(&bytes).unwrap();
        let column = decoded.batches[0].column(0);
        let values = column.as_any().downcast_ref::<TimestampMicrosecondArray>();
        assert_eq!(values.unwrap().values(), &[0, 1_000_000]);
        let cases: [Addition; 5] = [
            (At::File, |f| {
                let pair = Fields::default().binary(1, b"k").binary(2, b"v").end();
                let order = Fields::default().strct(1, Fields::default()).end();
                let f = f.list(5, 12, &[pair]).binary(6, b"yamaa test");
                f.list(7, 12, &[order])
            }),
            (At::Group, |f| {
                let sorting = Fields::default().i32(1, 0).bool(2, false).bool(3, true);
                f.list(4, 12, &[sorting.end()])
                    .i64(5, 4)
                    .raw(7, 4, &zigzag(0))
            }),
            (At::Chunk, |f| f.i64(4, 0).i32(5, 0).i64(6, 0).i32(7, 0)),
            (At::Meta, |f| {
                let stats = Fields::default()
                    .i64(3, 0)
                    .i64(4, 2)
                    .bool(7, true)
                    .bool(8, true);
                let encodings = Fields::default().i32(1, 0).i32(2, 0).i32(3, 1).end();
                let size = Fields::default().i64(1, 0);
                let size = size.list(2, 6, &[zigzag(2)]).list(3, 6, &[zigzag(2)]);
                let geo = Fields::default().strct(1, bounding_box());
                let geo = geo.list(2, 5, &[zigzag(1)]);
                let f = f.i64(10, 0).strct(12, stats).list(13, 12, &[encodings]);
                f.i64(14, 0).i32(15, 0).strct(16, size).strct(17, geo)
            }),
            (At::Page, |f| {
                let dictionary = Fields::default().i32(1, 0).i32(2, 0).bool(3, false);
                let v2 = Fields::default().i32(1, 2).i32(2, 0).i32(3, 2).i32(4, 0);
                let v2 = v2.i32(5, 0).i32(6, 0).bool(7, false);
                let f = f
                    .strct(6, Fields::default())
                    .strct(7, dictionary)
                    .strct(8, v2);
                f.i32(4, 0)
            }),
        ];
        for (at, add) in cases {
            let decoded = reader::read(&file(&[7, -1], at, add));
            let decoded = decoded.unwrap_or_else(|error| panic!("{at:?}: {error:?}"));
            let column = decoded.batches[0].column(0);
            let values = column.as_any().downcast_ref::<Int64Array>().unwrap();
            assert_eq!(values.values(), &[7, -1], "{at:?}");
        }
    }

    /// The values of the authored file's column `V`, read whole.
    fn read_values(bytes: &[u8]) -> Result<Vec<i64>, reader::Error> {
        let decoded = reader::read(bytes)?;
        let column = decoded.batches[0].column(0);
        let values = column.as_any().downcast_ref::<Int64Array>().unwrap();
        Ok(values.values().to_vec())
    }

    /// Column orders bear only on statistics, which the engine never reads,
    /// so the native decoder is shown each order it does not know as the
    /// type-defined one: a file recording the IEEE 754 total order reads, as
    /// do later orders (PARQUET-10). A union that holds no variant, or two,
    /// is still refused, by the native decoder, and the IEEE 754 total order
    /// is an empty struct, as the format defines it.
    #[test]
    fn every_column_order_reads_as_the_type_defined_one() {
        let order = |fields: Fields| fields.end();
        let empty = Fields::default;
        type Order = (&'static str, Vec<u8>, bool);
        let orders: [Order; 6] = [
            ("TYPE_ORDER", order(empty().strct(1, empty())), false),
            (
                "IEEE_754_TOTAL_ORDER",
                order(empty().strct(2, empty())),
                true,
            ),
            // The same, its header in the long form.
            ("long IEEE_754_TOTAL_ORDER", vec![0x0c, 0x04, 0, 0], true),
            ("later empty struct", order(empty().strct(3, empty())), true),
            ("later integer", order(empty().i32(3, 7)), true),
            (
                "later struct holding a binary that is not text",
                order(empty().strct(3, empty().binary(1, &[0xff]))),
                true,
            ),
        ];
        for (name, element, shown) in orders {
            // A later footer field still reads after the edited list.
            let add = move |f: Fields| {
                f.list(7, 12, std::slice::from_ref(&element))
                    .binary(90, b"later")
            };
            let bytes = file(&[7, -1], At::File, add);
            let frame = framing::admit(&bytes).unwrap();
            assert_eq!(frame.footer.splices.len(), usize::from(shown), "{name}");
            assert_eq!(read_values(&bytes), Ok(vec![7, -1]), "{name}");
        }
        // Two variants pass admission, and the native decoder refuses them.
        let two = order(empty().strct(1, empty()).strct(2, empty()));
        let bytes = file(&[7], At::File, move |f| {
            f.list(7, 12, std::slice::from_ref(&two))
        });
        assert!(framing::admit(&bytes).unwrap().footer.splices.is_empty());
        assert_eq!(read_values(&bytes), Err(reader::Error::Malformed));
        // No variant is shorter than any order the decoder reads (`minimum`).
        assert!(malformed(&file(&[7], At::File, |f| f.list(
            7,
            12,
            &[vec![0]]
        ))));
        let mistyped = file(&[7], At::File, |f| {
            f.list(7, 12, &[order(empty().i32(2, 0))])
        });
        assert!(malformed(&mistyped));
    }

    /// The native decoder reads a skipped binary as UTF-8 text and refuses
    /// one that is not. Such a binary holds only metadata, so the decoder is
    /// shown zero bytes in its place wherever it skips it, in the footer and
    /// in a page header alike (PARQUET-10). A known string that is not UTF-8
    /// still refuses the file.
    #[test]
    fn every_skipped_binary_that_is_not_text_reads() {
        let cases: [Addition; 12] = [
            (At::File, |f| f.binary(90, &[0xff])),
            (At::Page, |f| f.binary(90, &[0xff])),
            (At::Meta, |f| f.binary(90, &[0xff])),
            (At::Leaf, |f| f.binary(90, &[0xff])),
            (At::Group, |f| f.binary(90, &[0xc3])),
            // In an unknown struct, list, set and list of structs.
            (At::File, |f| {
                f.strct(90, Fields::default().binary(1, &[0xff]))
            }),
            (At::File, |f| f.list(90, 8, &[vec![1, b'a'], vec![1, 0xff]])),
            (At::Page, |f| {
                f.collection(90, 10, 8, &[vec![2, 0xe2, 0x82]])
            }),
            (At::File, |f| {
                f.list(90, 12, &[Fields::default().binary(1, &[0xff]).end()])
            }),
            // In structs whose every field the decoder skips: TYPE_ORDER and
            // an index page header.
            (At::File, |f| {
                let order = Fields::default().strct(1, Fields::default().binary(1, &[0xff]));
                f.list(7, 12, &[order.end()])
            }),
            (At::Page, |f| {
                f.strct(6, Fields::default().binary(1, &[0xff]))
            }),
            // An unknown field of a known struct in a known list.
            (At::File, |f| {
                let pair = Fields::default().binary(1, b"k").binary(2, b"v");
                f.list(5, 12, &[pair.binary(3, &[0xff]).end()])
            }),
        ];
        for (index, (at, add)) in cases.into_iter().enumerate() {
            let bytes = file(&[7, -1], at, add);
            assert_eq!(read_values(&bytes), Ok(vec![7, -1]), "case {index}");
        }
        // The decoder is shown zeros exactly where the binary's bytes lie.
        let bytes = file(&[7], At::Page, |f| f.binary(90, &[0xff, 0xfe]));
        let frame = framing::admit(&bytes).unwrap();
        let [opaque] = frame.opaque.as_slice() else {
            panic!("{:?}", frame.opaque);
        };
        assert_eq!(bytes[opaque.clone()], [0xff, 0xfe]);
        let bytes = file(&[7], At::File, |f| {
            f.binary(90, b"text").binary(91, &[0xff])
        });
        let frame = framing::admit(&bytes).unwrap();
        assert_eq!(frame.footer.opaque.len(), 1);
        assert_eq!(frame.metadata[frame.footer.opaque[0].clone()], [0xff]);
        // Known strings: the writer, and a key-value entry's value.
        for add in [
            (|f: Fields| f.binary(6, &[0xff])) as fn(Fields) -> Fields,
            |f| {
                let pair = Fields::default().binary(1, b"k").binary(2, &[0xff]);
                f.list(5, 12, &[pair.end()])
            },
        ] {
            let bytes = file(&[7], At::File, add);
            assert_eq!(read_values(&bytes), Err(reader::Error::Malformed));
        }
    }

    /// A footer with no row group and no row: version 1 and the schema
    /// elements `schema`, the root first.
    fn schema_footer(schema: &[Vec<u8>]) -> Vec<u8> {
        let footer = Fields::default().i32(1, 1).list(2, 12, schema).i64(3, 0);
        footer.list(4, 12, &[]).end()
    }

    /// Check `footer` as admission does, then decode it as the native decoder
    /// is shown it.
    fn shown(footer: &[u8]) -> (super::Checked, Result<ParquetMetaData, reader::Error>) {
        let (root, _) = super::super::compact::parse(footer).unwrap();
        let checked = super::check(root, super::Shape::FileMetaData).unwrap();
        let decoded = super::super::row_groups::decode(footer, &checked.presentation);
        (checked, decoded)
    }

    /// Parquet 55 knows none of the variant, geometry and geography logical
    /// types. Where parquet 57 read one, the native decoder is shown the
    /// schema element without it, so the footer decodes to the type parquet
    /// 57 gave the element, and the check reports the element; any field
    /// that follows the removed one names its id afresh. Where parquet 57
    /// refused one, the decoder is shown it and refuses the footer too.
    #[test]
    fn a_later_logical_type_is_removed_where_parquet_57_read_it() {
        use parquet::basic::{ConvertedType, Type as Physical};
        let empty = Fields::default;
        let root = |children| empty().binary(4, b"schema").i32(5, children).end();
        let geometry = || empty().strct(17, empty().binary(1, b"OGC:CRS84"));
        let geography = || empty().strct(18, empty().i32(2, 0));
        let variant = || empty().strct(16, empty().raw(1, 3, &[1]));
        // A REQUIRED leaf G of physical type `physical`.
        let leaf = |physical| empty().i32(1, physical).i32(3, 0).binary(4, b"G");
        let binary = |logical: Fields| leaf(6).strct(10, logical).end();
        type Element = (&'static str, Vec<u8>);
        let leaves: [Element; 9] = [
            ("geometry", binary(geometry())),
            ("geography", binary(geography())),
            (
                "an unknown edge algorithm",
                binary(empty().strct(18, empty().i32(2, 99))),
            ),
            (
                "an unknown field that is not text",
                binary(empty().strct(17, empty().binary(5, &[0xff]))),
            ),
            // A known field after the logical type, then a later field, and a
            // later field that is a set; and the logical type first.
            (
                "a name after it",
                empty()
                    .i32(1, 6)
                    .i32(3, 0)
                    .strct(10, geometry())
                    .binary(4, b"G")
                    .end(),
            ),
            (
                "a later field after it",
                leaf(6).strct(10, geometry()).i32(11, 5).end(),
            ),
            (
                "a later set after it",
                leaf(6)
                    .strct(10, geometry())
                    .collection(11, 10, 5, &[vec![2]])
                    .end(),
            ),
            (
                "the logical type first",
                empty()
                    .strct(10, geometry())
                    .i32(1, 6)
                    .i32(3, 0)
                    .binary(4, b"G")
                    .end(),
            ),
            (
                "a long field id",
                leaf(6).strct(10, geometry()).raw(300, 5, &[2]).end(),
            ),
        ];
        for (name, element) in leaves {
            let (checked, decoded) = shown(&schema_footer(&[root(1), element]));
            assert_eq!(checked.annotated, [1], "{name}");
            let decoded = decoded.unwrap_or_else(|error| panic!("{name}: {error:?}"));
            let column = decoded.file_metadata().schema_descr().column(0);
            assert_eq!(column.name(), "G", "{name}");
            assert_eq!(column.physical_type(), Physical::BYTE_ARRAY, "{name}");
            assert_eq!(column.logical_type(), None, "{name}");
            assert_eq!(column.converted_type(), ConvertedType::NONE, "{name}");
        }
        // On groups parquet 57 checked nothing: a variant, as the format
        // places it, and a geometry.
        let child = |name: &[u8]| empty().i32(1, 6).i32(3, 0).binary(4, name).end();
        for (name, logical) in [("variant", variant()), ("geometry", geometry())] {
            let group = empty()
                .i32(3, 0)
                .binary(4, b"X")
                .i32(5, 2)
                .strct(10, logical);
            let schema = [root(1), group.end(), child(b"metadata"), child(b"value")];
            let (checked, decoded) = shown(&schema_footer(&schema));
            assert_eq!(checked.annotated, [1], "{name}");
            let decoded = decoded.unwrap_or_else(|error| panic!("{name}: {error:?}"));
            let schema = decoded.file_metadata().schema_descr();
            let group = &schema.root_schema().get_fields()[0];
            assert_eq!(group.get_basic_info().logical_type(), None, "{name}");
            assert_eq!(schema.num_columns(), 2, "{name}");
        }
        // Parquet 57 refused these, and so does the native decoder.
        let refused: [Element; 6] = [
            ("variant on a leaf", binary(variant())),
            ("geometry on an INT64", leaf(2).strct(10, geometry()).end()),
            (
                "geography on an INT32",
                leaf(1).strct(10, geography()).end(),
            ),
            (
                "geometry with a converted type",
                leaf(6).i32(6, 0).strct(10, geometry()).end(),
            ),
            (
                "a CRS that is not text",
                binary(empty().strct(17, empty().binary(1, &[0xff]))),
            ),
            ("two variants", binary(geometry().strct(1, empty()))),
        ];
        for (name, element) in refused {
            let (checked, decoded) = shown(&schema_footer(&[root(1), element]));
            assert!(checked.annotated.is_empty(), "{name}");
            assert!(checked.presentation.splices.is_empty(), "{name}");
            assert_eq!(decoded.unwrap_err(), reader::Error::Malformed, "{name}");
        }
    }

    /// The check counts schema elements across the whole list, so the
    /// element it reports is the annotated one, and framing names its leaf.
    #[test]
    fn the_annotated_element_and_its_leaf_are_reported_by_position() {
        let empty = Fields::default;
        let int64 = |name: &[u8]| empty().i32(1, 2).i32(3, 0).binary(4, name).end();
        let geometry = empty().strct(17, empty());
        let annotated = empty()
            .i32(1, 6)
            .i32(3, 0)
            .binary(4, b"G")
            .strct(10, geometry);
        let group = empty().i32(3, 0).binary(4, b"S").i32(5, 1).end();
        let root = empty().binary(4, b"schema").i32(5, 3).end();
        let schema = [root, int64(b"A"), group, int64(b"B"), annotated.end()];
        let footer = schema_footer(&schema);
        let (checked, decoded) = shown(&footer);
        assert_eq!(checked.annotated, [4]);
        decoded.unwrap();
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        assert_eq!(framing::admit(&bytes).unwrap().annotated, [2]);
    }

    #[test]
    fn a_crc_binary_cannot_hide_another_data_page_header() {
        use yamaa_core::diagnostic::{ParquetFinding, SourceFinding};
        // The native decoder read the binary's length prefix as the crc, then
        // its content as a second data page header declaring
        // BYTE_STREAM_SPLIT. It decoded the PLAIN page admission checked as
        // [1, 2] into [8589934593, 0].
        let hidden = Fields {
            bytes: Vec::new(),
            last: 4,
        };
        let hidden = hidden.strct(5, Fields::default().i32(1, 2).i32(2, 9).i32(3, 3).i32(4, 3));
        let bytes = file(&[1, 2], At::Page, |f| f.binary(4, &hidden.bytes));
        let Err(error) = super::super::parse(&bytes) else {
            panic!("the hidden data page header was read");
        };
        assert!(matches!(
            error.finding("RAW", "raw.parquet"),
            Some(SourceFinding::Parquet {
                finding: ParquetFinding::Invalid,
                ..
            })
        ));
    }

    #[test]
    fn mistyped_known_booleans_are_malformed_before_the_native_decoder_unwraps_them() {
        // The native decoder takes a known boolean from its field header, or
        // reads it from the next byte when an i32 zero carries it instead,
        // so it would read other bytes than admission checked. (parquet
        // 57.3.0 panicked there.)
        for bytes in [
            file(&[7], At::Group, |f| {
                let sorting = Fields::default().i32(1, 0).i32(2, 0).bool(3, true);
                f.list(4, 12, &[sorting.end()])
            }),
            file(&[7], At::Meta, |f| f.strct(12, Fields::default().i32(7, 0))),
            file(&[7], At::Page, |f| {
                f.strct(7, Fields::default().i32(1, 0).i32(2, 0).i32(3, 0))
            }),
        ] {
            assert_eq!(reader::read(&bytes).unwrap_err(), reader::Error::Malformed);
        }
    }

    /// A list value of wire type `kind` holding `count` elements: `first`,
    /// then copies of `other`. It may be longer than `Fields::list` writes.
    fn repeated(kind: u8, count: usize, first: &[u8], other: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xf0 | kind];
        varint(count as u64, &mut bytes);
        bytes.extend(first);
        for _ in 1..count {
            bytes.extend(other);
        }
        bytes
    }

    /// A file with no page and no column: version 1, the schema list value
    /// `schema`, no rows, then the fields `rest` adds.
    fn columnless(schema: &[u8], rest: impl Fn(Fields) -> Fields) -> Vec<u8> {
        let footer = rest(Fields::default().i32(1, 1).raw(2, 9, schema).i64(3, 0)).end();
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        bytes
    }

    /// A schema list value: a root and `count` REQUIRED INT64 leaves, each
    /// with an empty name, the shortest leaf the native decoder accepts.
    fn leaves(count: usize) -> Vec<u8> {
        let root = Fields::default().binary(4, b"").i32(5, count as i64).end();
        repeated(12, count + 1, &root, &[0x15, 4, 0x25, 0, 0x18, 0, 0])
    }

    /// A row-group list value: one row group with no column and no row,
    /// whose sorting columns are the list value `sorting`.
    fn sorted(sorting: &[u8]) -> Vec<u8> {
        let group = Fields::default().raw(1, 9, &[0x0c]).i64(2, 0).i64(3, 0);
        repeated(12, 1, &group.raw(4, 9, sorting).end(), &[])
    }

    /// The issue's crafted footers held 64 Mi empty key-value entries and 64
    /// Mi empty schema elements, and the native decoder reserves about 3 and
    /// 6.5 GiB for them before it refuses the first. Here they are scaled down to as
    /// many elements as each list may declare, beside the other lists the
    /// decoder reserves by their declared count: a list of empty structs is
    /// malformed, and a list of elements the decoder accepts, each as short
    /// as it can be, is admitted.
    #[test]
    fn a_list_too_short_for_the_elements_it_declares_is_malformed() {
        use super::{closed_profile, Limits};
        // As many key-value entries, schema leaves and empty groups as
        // admission allows.
        const N: usize = Limits::COLUMNS;
        let empty = |count| repeated(12, count, &[0], &[0]);
        let list = |count, element: &[u8]| repeated(12, count, element, element);
        // A root, and empty groups that name only their repetition.
        let root = Fields::default().binary(4, b"").i32(5, N as i64 - 1).end();
        let groups = repeated(12, N, &root, &[0x35, 0, 0x18, 0, 0]);
        let root_only = repeated(12, 1, &[0x48, 0, 0], &[]);
        type Case = (&'static str, Vec<u8>, Vec<u8>);
        let cases: [Case; 6] = [
            (
                "key_value_metadata",
                file(&[7], At::File, |f| f.raw(5, 9, &empty(N))),
                file(&[7], At::File, |f| f.raw(5, 9, &list(N, &[0x18, 0, 0]))),
            ),
            (
                "schema",
                columnless(&empty(N), |f| f.raw(4, 9, &[0x0c])),
                columnless(&groups, |f| f.raw(4, 9, &[0x0c])),
            ),
            // One column order per leaf. An unknown boolean variant takes no
            // byte past its header; the IEEE 754 total order (2) is known.
            (
                "column_orders",
                columnless(&leaves(N), |f| f.raw(4, 9, &[0x0c]).raw(7, 9, &empty(N))),
                columnless(&leaves(N), |f| {
                    f.raw(4, 9, &[0x0c]).raw(7, 9, &list(N, &[0x31, 0]))
                }),
            ),
            // Framing reads only the columns and row count of each row
            // group; the native decoder also requires its byte size.
            (
                "row_groups",
                columnless(&root_only, |f| {
                    f.raw(4, 9, &list(N, &[0x19, 0x0c, 0x26, 0, 0]))
                }),
                columnless(&root_only, |f| {
                    f.raw(4, 9, &list(N, &[0x19, 0x0c, 0x16, 0, 0x16, 0, 0]))
                }),
            ),
            // At most one sorting column per leaf.
            (
                "sorting_columns",
                columnless(&leaves(N), |f| f.raw(4, 9, &sorted(&empty(N)))),
                columnless(&leaves(N), |f| {
                    f.raw(4, 9, &sorted(&list(N, &[0x15, 0, 0x11, 0x11, 0])))
                }),
            ),
            (
                "encoding_stats",
                file(&[7], At::Meta, |f| {
                    f.raw(13, 9, &empty(closed_profile::encoding_statistics()))
                }),
                file(&[7], At::Meta, |f| {
                    let stats = [0x15, 0, 0x15, 0, 0x15, 2, 0];
                    f.raw(13, 9, &list(closed_profile::encoding_statistics(), &stats))
                }),
            ),
        ];
        for (name, refused, admitted) in cases {
            assert!(malformed(&refused), "{name}");
            framing::admit(&admitted).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
        // The native decoder reads each of the shortest key-value entries.
        let keys = file(&[7], At::File, |f| f.raw(5, 9, &list(N, &[0x18, 0, 0])));
        let decoded = reader::read(&keys).unwrap();
        let column = decoded.batches[0].column(0);
        let values = column.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!(values.values(), &[7]);
        // Every schema element but the root also names its repetition.
        let names = repeated(12, N, &root, &[0x48, 0, 0]);
        assert!(malformed(&columnless(&names, |f| f.raw(4, 9, &[0x0c]))));
        // One byte short: the struct's remaining bytes could hold the
        // entries, but the list's own bytes cannot, since its first entry
        // holds only an unknown boolean field.
        let short = repeated(12, N, &[0x31, 0], &[0x18, 0, 0]);
        assert!(malformed(&file(&[7], At::File, |f| f.raw(5, 9, &short))));
    }
}
