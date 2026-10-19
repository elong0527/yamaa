//! Decode the admitted footer, however many row groups it lists.
//!
//! The native decoder reads every row group of one footer and numbers none
//! of them: each row group keeps the ordinal it holds, or none. A footer that
//! lists more row groups than an `i16` ordinal numbers, as Polars writes after
//! 32,768 row groups, therefore reads whole like any other. Admission refuses
//! a repeated row-group list (`wire`), so the footer holds one.
use super::compact;
use super::reader::Error;
use crate::storage_api::contain_decode;
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};

/// Decode the admitted footer `held` through its native presentation.
pub(super) fn decode(held: &[u8]) -> Result<ParquetMetaData, Error> {
    let native = compact::native_metadata(held)?;
    contain_decode(|| ParquetMetaDataReader::decode_metadata(&native))
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

    /// Append a Compact Protocol list header for `count` elements of `kind`.
    fn header(footer: &mut Vec<u8>, kind: u8, count: usize) {
        if count < 15 {
            footer.push((count as u8) << 4 | kind);
            return;
        }
        footer.push(0xf0 | kind);
        varint(count as u64, footer);
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
    fn every_row_group_reads_in_file_order_with_its_own_chunks() {
        for ordinals in [0, 3, 8] {
            let metadata = decode(&footer(8, ordinals, 1)).unwrap();
            assert_eq!(metadata.file_metadata().num_rows(), 8);
            assert_own_chunks(metadata.row_groups(), 8);
            // Each row group keeps the ordinal it holds, or none.
            for (index, group) in metadata.row_groups().iter().enumerate() {
                let expected = (index < ordinals).then_some(index as i16);
                assert_eq!(group.ordinal(), expected, "{ordinals} {index}");
            }
        }
    }

    #[test]
    fn more_row_groups_than_an_i16_ordinal_numbers_read_whole() {
        let count = (1 << 15) + 1;
        // No ordinal, as pyarrow writes; the first 32,768 ordinals, as Polars
        // writes them; and every ordinal, wrapping after 32,767.
        for ordinals in [0, 1 << 15, count] {
            let metadata = decode(&footer(count, ordinals, 1)).unwrap();
            assert_eq!(metadata.file_metadata().num_rows(), count as i64);
            assert_own_chunks(metadata.row_groups(), count);
            let last = &metadata.row_groups()[1 << 15];
            let expected = (ordinals == count).then_some(i16::MIN);
            assert_eq!(last.ordinal(), expected, "{ordinals}");
        }
    }

    #[test]
    fn a_footer_without_a_row_group_list_does_not_decode() {
        // The decoder requires the list; admission refuses a repeated one.
        assert_eq!(decode(&footer(5, 0, 0)).unwrap_err(), Error::Malformed);
        assert_eq!(decode(&footer(5, 0, 1)).unwrap().num_row_groups(), 5);
    }
}
