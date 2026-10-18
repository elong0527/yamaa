//! Present a footer's row-group list to the native decoder in slices.
//!
//! The native decoder reads at most `FOOTER_ROW_GROUPS` row groups from one
//! footer (`storage_api`). A footer that lists no more is presented whole,
//! exactly as before. A longer list is presented as one footer per slice of
//! at most that many row groups, in file order, and the decoded row groups
//! are joined in that order:
//!
//! - The first slice is the whole presented footer with its list cut to its
//!   first row groups, so the decoder reads every other field as before.
//! - Each later slice holds only its own row groups, with the placeholder
//!   `version` and `num_rows` the decoder requires, and is decoded against
//!   the schema the first slice decoded. Nothing else is kept from it.
//!
//! Each row group's presented bytes are copied whole, so it keeps its own
//! column chunks, their statistics and the locations of their page indexes.
//! The decoder requires the row groups it reads from one footer to hold an
//! ordinal each or none. Writers that number row groups with an `i16`
//! ordinal reach its end after the same 32,768 row groups as the decoder:
//! Polars writes no ordinal after them, so each slice still agrees. A
//! repeated row-group list is presented whole, as before.
use super::compact::{self, Cursor, Value};
use super::reader::Error;
use crate::storage_api::{contain_decode, FOOTER_ROW_GROUPS};
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataOptions, ParquetMetaDataReader};
use std::ops::Range;

/// The footer's one row-group list, `FileMetaData` field 4, as held.
#[derive(Debug)]
struct List {
    /// The list header, which each slice replaces with its own.
    header: Range<usize>,
    /// The element wire type and count the header declares.
    kind: u8,
    count: usize,
    /// The offset of the first row group of each slice, then of the list's
    /// end.
    bounds: Vec<usize>,
}

/// Decode the admitted footer `held`, which lists `row_groups` row groups.
pub(super) fn decode(held: &[u8], row_groups: usize) -> Result<ParquetMetaData, Error> {
    decode_in_slices(held, row_groups, FOOTER_ROW_GROUPS)
}

fn decode_in_slices(held: &[u8], row_groups: usize, size: usize) -> Result<ParquetMetaData, Error> {
    let native = compact::native_metadata(held)?;
    if row_groups > size {
        if let Some(list) = locate(held, size)? {
            return sliced(&native, &list, size);
        }
    }
    native_decode(&native, None)
}

/// Find the row-group list of the admitted footer `held` and the first row
/// group of each slice of `size`; `None` when the footer repeats the list.
fn locate(held: &[u8], size: usize) -> Result<Option<List>, Error> {
    let mut cursor = Cursor::new(Value::Struct(held));
    let mut previous = 0;
    let mut list = None;
    while let Some((id, kind)) = cursor.field(previous)? {
        previous = id;
        if id != 4 {
            cursor.skip_value(kind)?;
            continue;
        }
        if list.is_some() {
            return Ok(None);
        }
        let start = cursor.position();
        let (kind, count) = cursor.collection()?;
        let header = start..cursor.position();
        let mut bounds = Vec::with_capacity(count.div_ceil(size) + 1);
        for index in 0..count {
            if index % size == 0 {
                bounds.push(cursor.position());
            }
            cursor.skip_element(kind)?;
        }
        bounds.push(cursor.position());
        list = Some(List {
            header,
            kind,
            count,
            bounds,
        });
    }
    Ok(list)
}

/// Decode the presented footer `native` one slice of `list` at a time.
fn sliced(native: &[u8], list: &List, size: usize) -> Result<ParquetMetaData, Error> {
    let (&[start, after, ..], Some(&end)) = (list.bounds.as_slice(), list.bounds.last()) else {
        return Err(Error::Malformed);
    };
    let mut footer = native[..list.header.start].to_vec();
    header(&mut footer, list.kind, size.min(list.count));
    footer.extend_from_slice(&native[start..after]);
    footer.extend_from_slice(&native[end..]);
    let first = native_decode(&footer, None)?;
    let options =
        ParquetMetaDataOptions::new().with_schema(first.file_metadata().schema_descr_ptr());
    let mut metadata = first.into_builder();
    let mut groups = metadata.take_row_groups();
    for (index, slice) in list.bounds.windows(2).enumerate().skip(1) {
        footer.clear();
        // version = 0 and num_rows = 0, then field 4 with this slice.
        footer.extend_from_slice(&[0x15, 0, 0x26, 0, 0x19]);
        header(&mut footer, list.kind, size.min(list.count - index * size));
        footer.extend_from_slice(&native[slice[0]..slice[1]]);
        footer.push(0);
        let decoded = native_decode(&footer, Some(&options))?;
        groups.extend(decoded.into_builder().take_row_groups());
    }
    Ok(metadata.set_row_groups(groups).build())
}

/// Append a Compact Protocol list header for `count` elements of `kind`.
fn header(footer: &mut Vec<u8>, kind: u8, count: usize) {
    if count < 15 {
        footer.push((count as u8) << 4 | kind);
        return;
    }
    footer.push(0xf0 | kind);
    let mut rest = count;
    while rest >= 0x80 {
        footer.push(rest as u8 | 0x80);
        rest >>= 7;
    }
    footer.push(rest as u8);
}

fn native_decode(
    footer: &[u8],
    options: Option<&ParquetMetaDataOptions>,
) -> Result<ParquetMetaData, Error> {
    contain_decode(|| ParquetMetaDataReader::decode_metadata_with_options(footer, options))
        .ok_or(Error::Malformed)?
        .map_err(|_| Error::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::file::{metadata::RowGroupMetaData, statistics::Statistics};

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
        fn raw(mut self, id: i16, kind: u8, value: &[u8]) -> Self {
            match id - self.last {
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

        fn integer(self, id: i16, kind: u8, value: i64) -> Self {
            self.raw(id, kind, &zigzag(value))
        }

        fn binary(self, id: i16, value: &[u8]) -> Self {
            let mut bytes = Vec::new();
            varint(value.len() as u64, &mut bytes);
            bytes.extend(value);
            self.raw(id, 8, &bytes)
        }

        fn strct(self, id: i16, value: Fields) -> Self {
            self.raw(id, 12, &value.end())
        }

        fn list(self, id: i16, kind: u8, items: &[Vec<u8>]) -> Self {
            let mut bytes = Vec::new();
            header(&mut bytes, kind, items.len());
            bytes.extend(items.concat());
            self.raw(id, 9, &bytes)
        }

        fn end(mut self) -> Vec<u8> {
            self.bytes.push(0);
            self.bytes
        }
    }

    /// An independently authored footer: one REQUIRED INT64 column `V` in
    /// `groups` one-row row groups. Row group `i` holds `i` in a page at
    /// `page(i)`, with `i` as its statistics and its page indexes at
    /// `INDEXES` past that page. The first `ordinals` row groups hold their
    /// ordinal, as an `i16` that wraps after 32,767. `lists` repeats the
    /// row-group list.
    fn footer(groups: usize, ordinals: usize, lists: usize) -> Vec<u8> {
        let group = |index: usize| {
            let value = (index as i64).to_le_bytes();
            let statistics = Fields::default().integer(3, 6, 0);
            let statistics = statistics.binary(5, &value).binary(6, &value);
            let meta = Fields::default()
                .integer(1, 5, 2) // INT64
                .list(2, 5, &[zigzag(0)]) // encodings: PLAIN
                .list(3, 8, &[b"\x01V".to_vec()]) // path_in_schema
                .integer(4, 5, 0) // UNCOMPRESSED
                .integer(5, 6, 1) // num_values
                .integer(6, 6, 32)
                .integer(7, 6, 32)
                .integer(9, 6, page(index))
                .strct(12, statistics);
            let chunk = Fields::default().integer(2, 6, 0).strct(3, meta);
            // The offset index's and column index's offsets and lengths.
            let chunk = chunk.integer(4, 6, page(index) + INDEXES).integer(5, 5, 16);
            let chunk = chunk
                .integer(6, 6, page(index) + 2 * INDEXES)
                .integer(7, 5, 24);
            let group = Fields::default().list(1, 12, &[chunk.end()]);
            let group = group.integer(2, 6, 32).integer(3, 6, 1);
            if index < ordinals {
                group.integer(7, 4, i64::from(index as i16)).end()
            } else {
                group.end()
            }
        };
        let root = Fields::default()
            .binary(4, b"schema")
            .integer(5, 5, 1)
            .end();
        // INT64; REQUIRED; name.
        let leaf = Fields::default().integer(1, 5, 2).integer(3, 5, 0);
        let leaf = leaf.binary(4, b"V").end();
        let mut footer = Fields::default()
            .integer(1, 5, 1)
            .list(2, 12, &[root, leaf]);
        footer = footer.integer(3, 6, groups as i64);
        let list: Vec<_> = (0..groups).map(group).collect();
        for _ in 0..lists {
            footer = footer.list(4, 12, &list);
        }
        footer.binary(6, b"yamaa test").end()
    }

    /// Where row group `index` places its page.
    fn page(index: usize) -> i64 {
        4 + 32 * index as i64
    }

    /// How far past a row group's page its offset index lies, and its column
    /// index twice as far.
    const INDEXES: i64 = 1 << 32;

    fn whole(footer: &[u8]) -> Result<ParquetMetaData, Error> {
        native_decode(&compact::native_metadata(footer)?, None)
    }

    /// Each row group holds its own one row, page, page indexes and
    /// statistics.
    fn assert_own_chunks(groups: &[RowGroupMetaData], count: usize) {
        assert_eq!(groups.len(), count);
        for (index, group) in groups.iter().enumerate() {
            assert_eq!(group.num_rows(), 1, "{index}");
            let chunk = group.column(0);
            assert_eq!(chunk.data_page_offset(), page(index), "{index}");
            let page = page(index);
            assert_eq!(chunk.offset_index_offset(), Some(page + INDEXES));
            assert_eq!(chunk.offset_index_length(), Some(16));
            assert_eq!(chunk.column_index_offset(), Some(page + 2 * INDEXES));
            assert_eq!(chunk.column_index_length(), Some(24));
            let Some(Statistics::Int64(statistics)) = chunk.statistics() else {
                panic!("{index}: {:?}", chunk.statistics());
            };
            let value = index as i64;
            assert_eq!(statistics.min_opt(), Some(&value), "{index}");
            assert_eq!(statistics.max_opt(), Some(&value), "{index}");
        }
    }

    #[test]
    fn list_headers_take_the_short_form_below_fifteen_elements() {
        for (count, expected) in [
            (0, &[0x0c][..]),
            (14, &[0xec]),
            (15, &[0xfc, 15]),
            (32_768, &[0xfc, 0x80, 0x80, 2]),
        ] {
            let mut bytes = Vec::new();
            header(&mut bytes, 12, count);
            assert_eq!(bytes, expected, "{count}");
        }
    }

    #[test]
    fn slices_read_every_row_group_in_file_order_with_its_own_chunks() {
        for ordinals in [0, 8] {
            let footer = footer(8, ordinals, 1);
            let whole = whole(&footer).unwrap();
            for size in [1, 3, 7] {
                let sliced = decode_in_slices(&footer, 8, size).unwrap();
                assert_eq!(sliced.file_metadata(), whole.file_metadata(), "{size}");
                assert_own_chunks(sliced.row_groups(), 8);
                for (index, (sliced, whole)) in sliced
                    .row_groups()
                    .iter()
                    .zip(whole.row_groups())
                    .enumerate()
                {
                    // Without a held ordinal, the decoder numbers each slice
                    // from zero; nothing else differs.
                    let expected = if ordinals == 0 { index % size } else { index };
                    assert_eq!(sliced.ordinal(), Some(expected as i16), "{size} {index}");
                    let renumbered = sliced.clone().into_builder();
                    let renumbered = renumbered.set_ordinal(index as i16).build().unwrap();
                    assert_eq!(&renumbered, whole, "{size} {index}");
                }
            }
        }
    }

    #[test]
    fn more_row_groups_than_one_footer_numbers_read_through_slices() {
        let count = FOOTER_ROW_GROUPS + 1;
        // No ordinal, as pyarrow writes; the first 32,768 ordinals, as Polars
        // writes them; and every ordinal, wrapping after 32,767.
        for ordinals in [0, FOOTER_ROW_GROUPS, count] {
            let footer = footer(count, ordinals, 1);
            assert_eq!(whole(&footer).unwrap_err(), Error::Malformed);
            let metadata = decode(&footer, count).unwrap();
            assert_eq!(metadata.file_metadata().num_rows(), count as i64);
            assert_own_chunks(metadata.row_groups(), count);
            let last = &metadata.row_groups()[FOOTER_ROW_GROUPS];
            let expected = (ordinals == count).then_some(i16::MIN);
            assert_eq!(last.ordinal(), expected.or(Some(0)), "{ordinals}");
        }
        // At most 32,768 row groups are presented whole, as before.
        let footer = footer(FOOTER_ROW_GROUPS, FOOTER_ROW_GROUPS, 1);
        let metadata = decode(&footer, FOOTER_ROW_GROUPS).unwrap();
        assert_eq!(metadata, whole(&footer).unwrap());
    }

    #[test]
    fn ordinals_agree_within_each_slice() {
        // Seven row groups in slices of three: [0, 3), [3, 6), [6, 7).
        for (ordinals, readable) in [(0, true), (3, true), (6, true), (7, true), (2, false)] {
            let result = decode_in_slices(&footer(7, ordinals, 1), 7, 3);
            assert_eq!(result.is_ok(), readable, "{ordinals}");
        }
        for ordinals in [4, 5] {
            let footer = footer(7, ordinals, 1);
            assert_eq!(
                decode_in_slices(&footer, 7, 3).unwrap_err(),
                Error::Malformed,
                "{ordinals}"
            );
        }
    }

    #[test]
    fn a_repeated_row_group_list_is_presented_whole() {
        let footer = footer(5, 0, 2);
        assert_eq!(locate(&footer, 3).unwrap().map(|list| list.count), None);
        let metadata = decode_in_slices(&footer, 5, 3).unwrap();
        assert_eq!(metadata, whole(&footer).unwrap());
        assert_eq!(metadata.num_row_groups(), 5);
    }
}
