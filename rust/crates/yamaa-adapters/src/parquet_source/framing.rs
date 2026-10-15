//! Admission of held Parquet framing before the general codec sees metadata.
//! No decompression or semantic type decisions occur here.
use super::compact::{self, Sequence, Value};

#[derive(Debug)]
pub(super) enum Error {
    Malformed,
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
}
impl From<compact::Error> for Error {
    fn from(_: compact::Error) -> Self {
        Self::Malformed
    }
}
impl From<yamaa_core::limits::LimitExceeded> for Error {
    fn from(error: yamaa_core::limits::LimitExceeded) -> Self {
        Self::ResourceLimit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
        }
    }
}

#[derive(Debug)]
pub(super) struct Page<'a> {
    pub codec: i64,
    pub body: &'a [u8],
    pub decoded_bytes: usize,
    pub uncompressed_prefix: usize,
    pub compressed: bool,
    pub column: usize,
    pub row_group: usize,
    pub payload: Payload,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) enum Payload {
    #[default]
    Index,
    Dictionary {
        encoding: i64,
        values: usize,
    },
    DataV1 {
        encoding: i64,
        values: usize,
        definition_encoding: i64,
        repetition_encoding: i64,
    },
    DataV2 {
        encoding: i64,
        values: usize,
        nulls: usize,
        definition_bytes: usize,
        repetition_bytes: usize,
    },
}

#[derive(Debug)]
pub(super) struct Framing<'a> {
    pub metadata: &'a [u8],
    pub pages: Vec<Page<'a>>,
}

fn integer(value: Option<Value<'_>>) -> Result<i64, Error> {
    if let Some(Value::Integer(value)) = value {
        Ok(value)
    } else {
        Err(Error::Malformed)
    }
}
fn count(value: Option<Value<'_>>) -> Result<usize, Error> {
    usize::try_from(integer(value)?).map_err(|_| Error::Malformed)
}
fn sequence(value: Option<Value<'_>>) -> Result<Sequence<'_>, Error> {
    if let Some(Value::Sequence(values)) = value {
        Ok(values)
    } else {
        Err(Error::Malformed)
    }
}
// Borrowed compact nodes do not allocate, but integer claims within each
// SchemaElement can cause later allocations. The general reader
// reserves num_children slots before discovering a truncated schema tree.
fn admit_schema(schema: Sequence<'_>) -> Result<usize, Error> {
    let mut leaves = 0;
    let mut remaining_children = Vec::new();
    for (index, element) in schema.into_iter().enumerate() {
        let element = element?;
        while remaining_children.last() == Some(&0) {
            remaining_children.pop();
        }
        if let Some(remaining) = remaining_children.last_mut() {
            *remaining -= 1;
        }
        // Flat Thrift metadata can describe a deeply recursive schema tree.
        // Admit its depth independently of the compact parser's own depth.
        if remaining_children.len() + 1 > yamaa_core::limits::Limits::SEMANTIC_DEPTH {
            return Err(Error::Malformed);
        }
        let children = element
            .field(5)?
            .map(|value| count(Some(value)))
            .transpose()?;
        // A zero child count can describe either a primitive (some writers
        // emit it explicitly) or an empty group. Only primitives have a type.
        if index != 0 && children.is_none_or(|count| count == 0) && element.field(1)?.is_some() {
            leaves += 1;
        }
        if let Some(children) = children {
            if children > schema.len() - index - 1 {
                return Err(Error::Malformed);
            }
            if children != 0 {
                remaining_children.push(children);
            }
        }
    }
    yamaa_core::limits::Limits::columns(leaves)?;
    Ok(leaves)
}

/// Collect borrowed page bodies from every stored column chunk in source order.
/// The downstream codec remains responsible for physical format semantics.
pub(super) fn admit(bytes: &[u8]) -> Result<Framing<'_>, Error> {
    yamaa_core::limits::Limits::source_bytes(bytes.len())?;
    if bytes.len() < 12 || &bytes[..4] != b"PAR1" || &bytes[bytes.len() - 4..] != b"PAR1" {
        return Err(Error::Malformed);
    }
    let footer = bytes.len() - 8;
    let length = u32::from_le_bytes(bytes[footer..footer + 4].try_into().unwrap()) as usize;
    let start = footer
        .checked_sub(length)
        .filter(|n| *n >= 4)
        .ok_or(Error::Malformed)?;
    let metadata = &bytes[start..footer];
    let (root, _) = compact::parse(metadata)?;
    let declared_rows = count(root.field(3)?)?;
    let groups = sequence(root.field(4)?)?;
    let schema = sequence(root.field(2)?)?;
    let leaves = admit_schema(schema)?;
    yamaa_core::limits::Limits::table(declared_rows, leaves)?;
    let mut result = Framing {
        metadata,
        pages: Vec::new(),
    };
    let mut rows = 0usize;
    for (row_group, group) in groups.into_iter().enumerate() {
        let group = group?;
        let count_rows = count(group.field(3)?)?;
        rows = rows.checked_add(count_rows).ok_or(Error::Malformed)?;
        let columns = sequence(group.field(1)?)?;
        yamaa_core::limits::Limits::table(rows, columns.len())?;
        for (column, chunk) in columns.into_iter().enumerate() {
            let chunk = chunk?;
            let meta = chunk.field(3)?.ok_or(Error::Malformed)?;
            let codec = integer(meta.field(4)?)?;
            let data = count(meta.field(9)?)?;
            let mut offset = match meta.field(11)? {
                // Match the reader's exact chunk start. Empty columns may have
                // data_page_offset=0 and a real, empty dictionary at offset 4.
                Some(value) => count(Some(value))?,
                None => data,
            };
            let length = count(meta.field(7)?)?;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= start)
                .ok_or(Error::Malformed)?;
            if offset < 4 && length != 0 {
                return Err(Error::Malformed);
            }
            while offset < end {
                let (header, used) = compact::parse(&bytes[offset..end])?;
                let uncompressed = count(header.field(2)?)?;
                let compressed = count(header.field(3)?)?;
                let payload = match integer(header.field(1)?)? {
                    0 => {
                        let detail = header.field(5)?.ok_or(Error::Malformed)?;
                        Payload::DataV1 {
                            encoding: integer(detail.field(2)?)?,
                            values: count(detail.field(1)?)?,
                            definition_encoding: integer(detail.field(3)?)?,
                            repetition_encoding: integer(detail.field(4)?)?,
                        }
                    }
                    1 => Payload::Index,
                    2 => {
                        let detail = header.field(7)?.ok_or(Error::Malformed)?;
                        Payload::Dictionary {
                            encoding: integer(detail.field(2)?)?,
                            values: count(detail.field(1)?)?,
                        }
                    }
                    3 => {
                        let detail = header.field(8)?.ok_or(Error::Malformed)?;
                        Payload::DataV2 {
                            encoding: integer(detail.field(4)?)?,
                            values: count(detail.field(1)?)?,
                            nulls: count(detail.field(2)?)?,
                            definition_bytes: count(detail.field(5)?)?,
                            repetition_bytes: count(detail.field(6)?)?,
                        }
                    }
                    _ => return Err(Error::Malformed),
                };
                let v2 = if matches!(payload, Payload::DataV2 { .. }) {
                    header.field(8)?
                } else {
                    None
                };
                let (prefix, should_decompress) = match v2 {
                    Some(v2) => {
                        let prefix = count(v2.field(5)?)?
                            .checked_add(count(v2.field(6)?)?)
                            .ok_or(Error::Malformed)?;
                        if prefix > compressed || prefix > uncompressed {
                            return Err(Error::Malformed);
                        }
                        let compress = match v2.field(7)? {
                            None | Some(Value::Bool(true)) => true,
                            Some(Value::Bool(false)) => false,
                            _ => return Err(Error::Malformed),
                        };
                        (prefix, compress)
                    }
                    None => (0, true),
                };
                let body_start = offset.checked_add(used).ok_or(Error::Malformed)?;
                offset = body_start
                    .checked_add(compressed)
                    .filter(|end_body| *end_body <= end)
                    .ok_or(Error::Malformed)?;
                result.pages.push(Page {
                    codec,
                    body: &bytes[body_start..offset],
                    decoded_bytes: uncompressed,
                    uncompressed_prefix: prefix,
                    compressed: should_decompress,
                    column,
                    row_group,
                    payload,
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned(bytes: &mut Vec<u8>, mut value: u64) {
        while value >= 128 {
            bytes.push(value as u8 | 128);
            value >>= 7;
        }
        bytes.push(value as u8);
    }

    fn container(page: &[u8]) -> Vec<u8> {
        assert!(page.len() < 64);
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(page);
        // Framing-only metadata: one row group, one column chunk, one row.
        // Required Parquet schema details are deliberately the next decoder's
        // responsibility; this fixture exercises framing, never a valid table.
        let footer = [
            0x15,
            2,
            0x19,
            0x1c,
            0,
            0x16,
            2,
            0x19,
            0x1c,
            0x19,
            0x1c,
            0x3c,
            0x45,
            0,
            0x16,
            2,
            0x26,
            (page.len() * 2) as u8,
            0x26,
            8,
            0,
            0,
            0x26,
            2,
            0,
            0,
        ];
        bytes.extend(footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        bytes
    }
    fn page() -> Vec<u8> {
        vec![
            0x15, 0, 0x15, 8, 0x15, 8, 0x2c, 0x15, 2, 0x15, 0, 0x15, 6, 0x15, 6, 0, 0, 0, 0, 0, 0,
        ]
    }
    #[test]
    fn schema_integer_claims_and_flattened_tree_recursion_are_checked() {
        fn admit_children(children: &[i64]) -> Result<usize, Error> {
            let mut bytes = vec![0x19, 0xfc]; // field 1: extended struct list
            unsigned(&mut bytes, children.len() as u64);
            for (index, &children) in children.iter().enumerate() {
                if index != 0 && children == 0 {
                    bytes.extend([0x15, 4, 0x45]); // INT64 primitive; field 5
                } else {
                    bytes.push(0x55); // SchemaElement field 5: i32 num_children
                }
                unsigned(&mut bytes, ((children << 1) ^ (children >> 63)) as u64);
                bytes.push(0);
            }
            bytes.push(0);
            let (root, _) = compact::parse(&bytes)?;
            admit_schema(sequence(root.field(1)?)?)
        }
        assert!(matches!(
            admit_children(&[i32::MAX.into()]),
            Err(Error::Malformed)
        ));
        assert!(matches!(admit_children(&[-1]), Err(Error::Malformed)));
        let mut nested = vec![1; yamaa_core::limits::Limits::SEMANTIC_DEPTH - 1];
        nested.push(0);
        assert_eq!(admit_children(&nested).unwrap(), 1);
        nested.insert(0, 1);
        assert!(matches!(admit_children(&nested), Err(Error::Malformed)));
        let mut flat = vec![100];
        flat.extend([0; 100]);
        assert_eq!(admit_children(&flat).unwrap(), 100);
    }

    #[test]
    fn nested_group_nodes_do_not_replace_the_supported_profile_finding() {
        use parquet::{file::writer::SerializedFileWriter, schema::types::Type};
        use std::sync::Arc;
        let fields = (0..yamaa_core::limits::Limits::COLUMNS)
            .map(|index| {
                Arc::new(
                    Type::group_type_builder(&format!("G{index}"))
                        .with_repetition(parquet::basic::Repetition::OPTIONAL)
                        .with_fields(vec![Arc::new(
                            Type::primitive_type_builder("ID", parquet::basic::Type::INT64)
                                .build()
                                .unwrap(),
                        )])
                        .build()
                        .unwrap(),
                )
            })
            .collect();
        let schema = Arc::new(
            Type::group_type_builder("schema")
                .with_fields(fields)
                .build()
                .unwrap(),
        );
        let mut bytes = Vec::new();
        SerializedFileWriter::new(&mut bytes, schema, Default::default())
            .unwrap()
            .close()
            .unwrap();
        let result = super::super::parse(&bytes);
        assert!(
            matches!(&result, Err(super::super::Error::Unsupported { .. })),
            "1024 physical leaves remain within geometry; got {:?}",
            result.as_ref().err()
        );
    }

    #[test]
    fn unknown_footer_collections_preserve_independent_cells_and_reject_bad_children() {
        use yamaa_core::table::{TableAccess, ValueRef};
        let fixture = include_bytes!("../../tests/fixtures/pq/none-1-0.parquet");
        let footer = fixture.len() - 8;
        let old_length =
            u32::from_le_bytes(fixture[footer..footer + 4].try_into().unwrap()) as usize;
        let start = footer - old_length;
        assert_eq!(
            compact::parse(&fixture[start..footer]).unwrap().1,
            old_length
        );
        assert_eq!(fixture[footer - 1], 0); // FileMetaData STOP
        let mut bytes = fixture[..footer - 1].to_vec();
        bytes.push(0x09); // explicit unknown field 30000: empty-struct list
        unsigned(&mut bytes, 60000);
        bytes.push(0xfc);
        unsigned(&mut bytes, 65536);
        let elements = bytes.len();
        bytes.resize(elements + 65536, 0);
        bytes.push(0);
        let length = bytes.len() - start;
        bytes.extend((length as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        let table = super::super::parse(&bytes).unwrap();
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""));
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str("β\0z"));
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN));
        assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX));
        for column in 0..5 {
            assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing);
        }
        for (row, bits) in [(0, (-0.0f64).to_bits()), (2, 1)] {
            let ValueRef::Float(value) = table.cell(row, 2).unwrap() else {
                panic!();
            };
            assert_eq!(value.get().to_bits(), bits);
        }
        bytes[elements + 65535] = 0x1e;
        assert!(matches!(
            super::super::parse(&bytes),
            Err(super::super::Error::Malformed)
        ));
    }

    #[test]
    fn declared_cells_use_physical_leaves_instead_of_group_nodes() {
        const LEAVES: usize = 1024;
        for rows in [625000, 625001] {
            let mut footer = vec![0x29, 0xfc]; // schema: root + group/leaf pairs
            unsigned(&mut footer, (1 + 2 * LEAVES) as u64);
            footer.push(0x55);
            unsigned(&mut footer, (2 * LEAVES) as u64);
            footer.push(0);
            for _ in 0..LEAVES {
                footer.extend([0x55, 2, 0]); // one-child group
                footer.extend([0x15, 4, 0]); // INT64 primitive, no child count
            }
            footer.push(0x16); // declared num_rows
            unsigned(&mut footer, 2 * rows);
            footer.extend([0x19, 0, 0]); // no row groups; metadata STOP
            let mut bytes = b"PAR1".to_vec();
            bytes.extend(&footer);
            bytes.extend((footer.len() as u32).to_le_bytes());
            bytes.extend(b"PAR1");
            if rows == 625000 {
                admit(&bytes).unwrap();
            } else {
                assert!(matches!(
                    admit(&bytes),
                    Err(Error::ResourceLimit {
                        resource: "table_cells",
                        actual: Some(640001024),
                        ..
                    })
                ));
            }
        }
    }

    #[test]
    fn borrowed_page_ranges_preserve_exact_body_and_metadata() {
        let bytes = container(&page());
        let admitted = admit(&bytes).unwrap();
        assert_eq!(admitted.pages.len(), 1);
        let page = &admitted.pages[0];
        assert_eq!(page.body.as_ptr(), bytes[21..].as_ptr());
        assert_eq!(page.body, &[0, 0, 0, 0]);
        assert_eq!(page.codec, 0);
        assert_eq!(page.decoded_bytes, 4);
        assert_eq!(page.uncompressed_prefix, 0);
        assert!(page.compressed);
        assert_eq!(admitted.metadata.len(), 26);
    }

    #[test]
    fn footer_bounds_page_body_lengths_and_v2_prefixes_are_not_trusted() {
        let mut bytes = container(&page());
        bytes[0] = 0;
        assert!(matches!(admit(&bytes), Err(Error::Malformed)));
        let mut bytes = container(&page());
        let footer = bytes.len() - 8;
        bytes[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(admit(&bytes), Err(Error::Malformed)));
        let mut bad = page();
        bad[5] = 10; // claims five body bytes; only four exist
        assert!(matches!(admit(&container(&bad)), Err(Error::Malformed)));
        // V2: one value, zero nulls, one row, PLAIN, 3 definition bytes,
        // 2 repetition bytes, no compression; sizes claim only four bytes.
        let bad = [
            0x15, 6, 0x15, 8, 0x15, 8, 0x5c, 0x15, 2, 0x15, 0, 0x15, 2, 0x15, 0, 0x15, 6, 0x15, 4,
            0x12, 0, 0, 0, 0, 0, 0,
        ];
        assert!(matches!(admit(&container(&bad)), Err(Error::Malformed)));
        let mut good = bad;
        good[18] = 0; // Prefix is now three bytes, no compression.
        let bytes = container(&good);
        let admitted = admit(&bytes).unwrap();
        assert_eq!(admitted.pages[0].uncompressed_prefix, 3);
        assert!(!admitted.pages[0].compressed);
    }
    #[test]
    fn declared_row_boundary_is_checked_before_decoding_page_values() {
        let fixture = container(&page());
        let footer_start = fixture.len() - 8 - 26;
        for rows in [
            yamaa_core::limits::Limits::TABLE_ROWS - 1,
            yamaa_core::limits::Limits::TABLE_ROWS + 1,
        ] {
            let mut encoded = Vec::new();
            let mut value = (rows as u64) << 1;
            while value >= 128 {
                encoded.push(value as u8 | 128);
                value >>= 7;
            }
            encoded.push(value as u8);
            let mut bytes = fixture[..footer_start + 6].to_vec();
            bytes.extend(&encoded);
            bytes.extend(&fixture[footer_start + 7..fixture.len() - 8]);
            bytes.extend((25u32 + encoded.len() as u32).to_le_bytes());
            bytes.extend(b"PAR1");
            if rows <= yamaa_core::limits::Limits::TABLE_ROWS {
                admit(&bytes).unwrap();
            } else {
                assert!(
                    matches!(admit(&bytes), Err(Error::ResourceLimit { resource: "table_rows", actual: Some(actual), .. }) if actual == rows)
                );
            }
        }
    }
}
