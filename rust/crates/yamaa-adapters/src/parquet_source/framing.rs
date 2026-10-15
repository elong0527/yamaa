//! Admission of held Parquet framing before the general codec sees metadata.
//! No decompression or semantic type decisions occur here.
use super::compact::{self, Value};

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

fn integer(value: Option<&Value<'_>>) -> Result<i64, Error> {
    if let Some(Value::Integer(value)) = value {
        Ok(*value)
    } else {
        Err(Error::Malformed)
    }
}
fn count(value: Option<&Value<'_>>) -> Result<usize, Error> {
    usize::try_from(integer(value)?).map_err(|_| Error::Malformed)
}
fn sequence<'a, 'b>(value: Option<&'b Value<'a>>) -> Result<&'b [Value<'a>], Error> {
    if let Some(Value::Sequence(values)) = value {
        Ok(values)
    } else {
        Err(Error::Malformed)
    }
}
// A compact list's length bounds its own allocation, but not allocations made
// later from integer fields within each SchemaElement. The general reader
// reserves num_children slots before discovering a truncated schema tree.
fn admit_schema(schema: &[Value<'_>]) -> Result<usize, Error> {
    let mut leaves = 0;
    let mut remaining_children = Vec::new();
    for (index, element) in schema.iter().enumerate() {
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
            .field(5)
            .map(|value| count(Some(value)))
            .transpose()?
            .unwrap_or(0);
        if children > schema.len() - index - 1 {
            return Err(Error::Malformed);
        }
        if children != 0 {
            remaining_children.push(children);
        } else if index != 0 && matches!(element.field(1), Some(Value::Integer(_))) {
            // Match the downstream descriptor: only primitive fields supply
            // columns. Groups, including empty groups, have no physical leaf.
            // Parquet writers may omit num_children or explicitly store zero.
            leaves += 1;
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
    let (root, _) = compact::metadata(metadata)?;
    let declared_rows = count(root.field(3))?;
    let groups = sequence(root.field(4))?;
    let schema = sequence(root.field(2))?;
    let leaves = admit_schema(schema)?;
    yamaa_core::limits::Limits::table(declared_rows, leaves)?;
    let mut result = Framing {
        metadata,
        pages: Vec::new(),
    };
    let mut rows = 0usize;
    for (row_group, group) in groups.iter().enumerate() {
        let count_rows = count(group.field(3))?;
        rows = rows.checked_add(count_rows).ok_or(Error::Malformed)?;
        let columns = sequence(group.field(1))?;
        yamaa_core::limits::Limits::table(rows, columns.len())?;
        for (column, chunk) in columns.iter().enumerate() {
            let meta = chunk.field(3).ok_or(Error::Malformed)?;
            let codec = integer(meta.field(4))?;
            let data = count(meta.field(9))?;
            let mut offset = match meta.field(11) {
                // Match the reader's exact chunk start. Empty columns may have
                // data_page_offset=0 and a real, empty dictionary at offset 4.
                Some(value) => count(Some(value))?,
                None => data,
            };
            let length = count(meta.field(7))?;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= start)
                .ok_or(Error::Malformed)?;
            if offset < 4 && length != 0 {
                return Err(Error::Malformed);
            }
            while offset < end {
                let (header, used) = compact::page(&bytes[offset..end])?;
                let uncompressed = count(header.field(2))?;
                let compressed = count(header.field(3))?;
                let payload = match integer(header.field(1))? {
                    0 => {
                        let detail = header.field(5).ok_or(Error::Malformed)?;
                        Payload::DataV1 {
                            encoding: integer(detail.field(2))?,
                            values: count(detail.field(1))?,
                            definition_encoding: integer(detail.field(3))?,
                            repetition_encoding: integer(detail.field(4))?,
                        }
                    }
                    1 => Payload::Index,
                    2 => {
                        let detail = header.field(7).ok_or(Error::Malformed)?;
                        Payload::Dictionary {
                            encoding: integer(detail.field(2))?,
                            values: count(detail.field(1))?,
                        }
                    }
                    3 => {
                        let detail = header.field(8).ok_or(Error::Malformed)?;
                        Payload::DataV2 {
                            encoding: integer(detail.field(4))?,
                            values: count(detail.field(1))?,
                            nulls: count(detail.field(2))?,
                            definition_bytes: count(detail.field(5))?,
                            repetition_bytes: count(detail.field(6))?,
                        }
                    }
                    _ => return Err(Error::Malformed),
                };
                let v2 = if matches!(payload, Payload::DataV2 { .. }) {
                    header.field(8)
                } else {
                    None
                };
                let (prefix, should_decompress) = match v2 {
                    Some(v2) => {
                        let prefix = count(v2.field(5))?
                            .checked_add(count(v2.field(6))?)
                            .ok_or(Error::Malformed)?;
                        if prefix > compressed || prefix > uncompressed {
                            return Err(Error::Malformed);
                        }
                        let compress = match v2.field(7) {
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

    // Independently encoded, zero-row-group footer. Each INT64 leaf is either
    // at the root or inside its own required group; no page values are needed
    // to exercise geometry declared by the schema and file metadata.
    fn geometry_container(rows: usize, leaves: usize, wrapped: bool) -> Vec<u8> {
        fn unsigned(mut value: usize, output: &mut Vec<u8>) {
            while value >= 128 {
                output.push(value as u8 | 128);
                value >>= 7;
            }
            output.push(value as u8);
        }
        fn name(value: &str, output: &mut Vec<u8>) {
            unsigned(value.len(), output);
            output.extend(value.as_bytes());
        }
        let mut footer = vec![0x15, 2, 0x19, 0xfc]; // version=1; schema list
        unsigned(1 + leaves * if wrapped { 2 } else { 1 }, &mut footer);
        footer.push(0x48); // root name, then num_children
        name("schema", &mut footer);
        footer.push(0x15);
        unsigned(leaves * 2, &mut footer);
        footer.push(0);
        for index in 0..leaves {
            if wrapped {
                footer.extend([0x35, 0, 0x18]); // REQUIRED group with one child
                name(&format!("G{index}"), &mut footer);
                footer.extend([0x15, 2, 0]);
            }
            footer.extend([0x15, 4, 0x25, 0, 0x18]); // INT64; REQUIRED; name
            name(&format!("V{index}"), &mut footer);
            if index % 2 == 0 {
                footer.extend([0x15, 0]); // explicit zero children is also valid
            }
            footer.push(0);
        }
        footer.push(0x16); // file num_rows
        unsigned(rows * 2, &mut footer);
        footer.extend([0x19, 0x0c, 0]); // empty row_groups; STOP
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        bytes
    }

    #[test]
    fn column_boundary_counts_leaves_instead_of_group_nodes() {
        for wrapped in [false, true] {
            for leaves in [1023, 1024, 1025] {
                let bytes = geometry_container(0, leaves, wrapped);
                if leaves <= 1024 {
                    admit(&bytes).unwrap();
                } else {
                    assert!(matches!(
                        admit(&bytes),
                        Err(Error::ResourceLimit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025),
                        })
                    ));
                }
            }
        }
        // Neither the message root nor an empty group supplies a physical leaf.
        let schema = [
            Value::Struct(vec![(5, Value::Integer(2))]),
            Value::Struct(vec![(5, Value::Integer(0))]),
            Value::Struct(vec![(1, Value::Integer(2))]),
        ];
        assert_eq!(admit_schema(&schema).unwrap(), 1);
    }

    #[test]
    fn declared_cell_boundary_uses_physical_leaf_columns() {
        let at = geometry_container(10_000_000, 64, true);
        admit(&at).unwrap(); // 640,000,000 cells despite the 64 extra group nodes
        let above = geometry_container(10_000_000, 65, true);
        assert!(matches!(
            admit(&above),
            Err(Error::ResourceLimit {
                resource: "table_cells",
                limit: 640_000_000,
                actual: Some(650_000_000),
            })
        ));
    }

    #[test]
    fn nested_schema_at_leaf_boundary_reaches_unsupported_profile_finding() {
        let bytes = geometry_container(0, 1024, true);
        assert!(matches!(
            crate::parquet_source::reader::read(&bytes),
            Err(crate::parquet_source::reader::Error::Profile(
                crate::parquet_source::profile::Error::Unsupported { field, .. }
            )) if field == "G0"
        ));
    }

    #[test]
    fn unknown_footer_fanout_keeps_a_valid_empty_table_decodable() {
        let mut bytes = geometry_container(0, 1, false);
        // Remove root STOP and trailer. Unknown field 15 follows field 4: a
        // 65,536-element byte list, valid for both Compact decoders.
        bytes.truncate(bytes.len() - 9);
        bytes.extend([0xb9, 0xf3, 0x80, 0x80, 4]);
        for _ in 0..32_768 {
            bytes.extend([1, 2]);
        }
        bytes.push(0);
        let footer_length = (bytes.len() - 4) as u32;
        bytes.extend(footer_length.to_le_bytes());
        bytes.extend(b"PAR1");
        let decoded = crate::parquet_source::reader::read(&bytes).unwrap();
        assert_eq!(decoded.schema.columns()[0].name, "V0");
        assert_eq!(decoded.batches[0].num_rows(), 0);
    }

    #[test]
    fn wrong_schema_and_row_group_container_shapes_are_malformed() {
        for footer in [
            // schema is a bool list, rows=0, row_groups=[]
            &[0x15, 2, 0x19, 0x31, 1, 2, 1, 0x16, 0, 0x19, 0x0c, 0][..],
            // schema=[{num_children:0}], rows=0, row_groups is a bool list
            &[
                0x15, 2, 0x19, 0x1c, 0x55, 0, 0, 0x16, 0, 0x19, 0x31, 1, 2, 1, 0,
            ][..],
        ] {
            let mut bytes = b"PAR1".to_vec();
            bytes.extend(footer);
            bytes.extend((footer.len() as u32).to_le_bytes());
            bytes.extend(b"PAR1");
            assert!(matches!(admit(&bytes), Err(Error::Malformed)));
        }
    }

    #[test]
    fn schema_integer_claims_and_flattened_tree_recursion_are_checked() {
        let element = |children| Value::Struct(vec![(5, Value::Integer(children))]);
        assert!(matches!(
            admit_schema(&[element(i32::MAX.into())]),
            Err(Error::Malformed)
        ));
        assert!(matches!(
            admit_schema(&[element(-1)]),
            Err(Error::Malformed)
        ));
        let mut nested: Vec<_> = (0..yamaa_core::limits::Limits::SEMANTIC_DEPTH - 1)
            .map(|_| element(1))
            .collect();
        nested.push(element(0));
        admit_schema(&nested).unwrap();
        nested.insert(0, element(1));
        assert!(matches!(admit_schema(&nested), Err(Error::Malformed)));
        let mut flat = vec![element(100)];
        flat.extend((0..100).map(|_| element(0)));
        admit_schema(&flat).unwrap();
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
