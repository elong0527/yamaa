//! Admission of held Parquet framing before the general codec sees metadata.
//! No decompression or semantic type decisions occur here.
use super::compact::{self, Error, Value};

#[derive(Clone, Copy, Debug)]
pub(super) struct Limits {
    pub source_bytes: usize,
    pub metadata_bytes: usize,
    pub metadata_nodes: usize,
    pub header_bytes: usize,
    pub header_nodes: usize,
    pub row_groups: usize,
    pub columns: usize,
    pub rows: usize,
    pub cells: usize,
    pub pages: usize,
    pub page_bytes: usize,
    pub decoded_bytes: usize,
}

#[derive(Debug)]
pub(super) struct Page<'a> {
    pub codec: i64,
    pub body: &'a [u8],
    pub decoded_bytes: usize,
    pub uncompressed_prefix: usize,
    pub compressed: bool,
    pub column: usize,
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
fn charge(used: &mut usize, amount: usize, limit: usize) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or(Error::Limit)?;
    Ok(())
}
fn bound(value: usize, limit: usize) -> Result<(), Error> {
    if value > limit {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}

// A compact list's length bounds its own allocation, but not allocations made
// later from integer fields within each SchemaElement. The general reader
// reserves num_children slots before discovering a truncated schema tree.
fn admit_schema(schema: &[Value<'_>], limits: Limits) -> Result<(), Error> {
    bound(schema.len().saturating_sub(1), limits.columns)?;
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
        bound(remaining_children.len() + 1, 64)?;
        if let Some(value) = element.field(5) {
            let children = count(Some(value))?;
            if children > schema.len() - index - 1 {
                return Err(Error::Malformed);
            }
            if children != 0 {
                remaining_children.push(children);
            }
        }
        if let Some(value) = element.field(2) {
            let length = integer(Some(value))?;
            if length > 0 {
                bound(
                    usize::try_from(length).map_err(|_| Error::Limit)?,
                    limits.page_bytes,
                )?;
            }
        }
    }
    Ok(())
}

/// Collect bounded borrowed page bodies from every stored column chunk, in
/// row-group/column/page order. Unknown metadata is still included in node/work
/// admission. The downstream codec remains responsible for format semantics.
pub(super) fn admit(bytes: &[u8], limits: Limits) -> Result<Framing<'_>, Error> {
    bound(bytes.len(), limits.source_bytes)?;
    if bytes.len() < 12 || &bytes[..4] != b"PAR1" || &bytes[bytes.len() - 4..] != b"PAR1" {
        return Err(Error::Malformed);
    }
    let footer = bytes.len() - 8;
    let length = u32::from_le_bytes(bytes[footer..footer + 4].try_into().unwrap()) as usize;
    bound(length, limits.metadata_bytes)?;
    let start = footer
        .checked_sub(length)
        .filter(|n| *n >= 4)
        .ok_or(Error::Malformed)?;
    let metadata = &bytes[start..footer];
    let (root, _, _) = compact::parse(
        metadata,
        compact::Limits {
            nodes: limits.metadata_nodes,
            depth: 64,
            bytes: limits.metadata_bytes,
        },
    )?;
    bound(count(root.field(3))?, limits.rows)?;
    let groups = sequence(root.field(4))?;
    bound(groups.len(), limits.row_groups)?;
    // Include the root schema element in metadata-node accounting. Leaf/field
    // matching and the closed type contract are checked by the actual decoder.
    let schema = sequence(root.field(2))?;
    admit_schema(schema, limits)?;
    let mut result = Framing {
        metadata,
        pages: Vec::new(),
    };
    let mut rows = 0;
    let mut cells = 0;
    let mut decoded = 0;
    let mut header_nodes = 0;
    let mut data_values = 0;
    let mut dictionary_values = 0;
    for group in groups {
        let count_rows = count(group.field(3))?;
        charge(&mut rows, count_rows, limits.rows)?;
        let columns = sequence(group.field(1))?;
        bound(columns.len(), limits.columns)?;
        charge(
            &mut cells,
            count_rows.checked_mul(columns.len()).ok_or(Error::Limit)?,
            limits.cells,
        )?;
        for (column, chunk) in columns.iter().enumerate() {
            let meta = chunk.field(3).ok_or(Error::Malformed)?;
            let codec = integer(meta.field(4))?;
            bound(count(meta.field(5))?, limits.cells)?;
            let data = count(meta.field(9))?;
            let mut offset = match meta.field(11) {
                Some(value) => count(Some(value))?.min(data),
                None => data,
            };
            let length = count(meta.field(7))?;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= start)
                .ok_or(Error::Malformed)?;
            if offset < 4 {
                return Err(Error::Malformed);
            }
            while offset < end {
                bound(
                    result.pages.len().checked_add(1).ok_or(Error::Limit)?,
                    limits.pages,
                )?;
                let (header, used, nodes) = compact::parse(
                    &bytes[offset..end],
                    compact::Limits {
                        nodes: limits.header_nodes.saturating_sub(header_nodes),
                        depth: 64,
                        bytes: limits.header_bytes,
                    },
                )?;
                charge(&mut header_nodes, nodes, limits.header_nodes)?;
                let uncompressed = count(header.field(2))?;
                let compressed = count(header.field(3))?;
                bound(uncompressed, limits.page_bytes)?;
                bound(compressed, limits.page_bytes)?;
                charge(&mut decoded, uncompressed, limits.decoded_bytes)?;
                // Value counts control downstream dictionary/column staging,
                // including headers inconsistent with their page-type tag.
                for field in [5, 7, 8] {
                    if let Some(detail) = header.field(field) {
                        bound(count(detail.field(1))?, limits.cells)?;
                    }
                }
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
                        bound(count(detail.field(2))?, limits.cells)?;
                        bound(count(detail.field(3))?, limits.rows)?;
                        Payload::DataV2 {
                            encoding: integer(detail.field(4))?,
                            values: count(detail.field(1))?,
                        }
                    }
                    _ => return Err(Error::Malformed),
                };
                match payload {
                    Payload::DataV1 { values, .. } | Payload::DataV2 { values, .. } => {
                        charge(&mut data_values, values, limits.cells)?
                    }
                    Payload::Dictionary { values, .. } => {
                        charge(&mut dictionary_values, values, limits.cells)?
                    }
                    Payload::Index => {}
                }
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
    fn limits() -> Limits {
        Limits {
            source_bytes: 1024,
            metadata_bytes: 512,
            metadata_nodes: 128,
            header_bytes: 128,
            header_nodes: 128,
            row_groups: 4,
            columns: 4,
            rows: 16,
            cells: 64,
            pages: 16,
            page_bytes: 64,
            decoded_bytes: 128,
        }
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
    fn schema_integer_claims_and_flattened_tree_depth_are_admitted_before_allocation() {
        let element = |children| Value::Struct(vec![(5, Value::Integer(children))]);
        assert!(matches!(
            admit_schema(&[element(i32::MAX.into())], limits()),
            Err(Error::Malformed)
        ));
        assert!(matches!(
            admit_schema(&[element(-1)], limits()),
            Err(Error::Malformed)
        ));
        let fixed = |length| Value::Struct(vec![(2, Value::Integer(length))]);
        admit_schema(&[element(1), fixed(64)], limits()).unwrap();
        assert!(matches!(
            admit_schema(&[element(1), fixed(65)], limits()),
            Err(Error::Limit)
        ));
        let mut nested: Vec<_> = (0..63).map(|_| element(1)).collect();
        nested.push(element(0));
        let generous = Limits {
            columns: 128,
            ..limits()
        };
        admit_schema(&nested, generous).unwrap();
        nested.insert(0, element(1));
        assert!(matches!(admit_schema(&nested, generous), Err(Error::Limit)));
        // Siblings exhaust their parent rather than incrementing tree depth.
        let mut flat = vec![element(100)];
        flat.extend((0..100).map(|_| element(0)));
        admit_schema(&flat, generous).unwrap();
    }
    #[test]
    fn borrowed_page_ranges_and_aggregate_limits_are_checked_before_codec_use() {
        let bytes = container(&page());
        let admitted = admit(&bytes, limits()).unwrap();
        assert_eq!(admitted.pages.len(), 1);
        let page = &admitted.pages[0];
        assert_eq!(page.body.as_ptr(), bytes[21..].as_ptr());
        assert_eq!(page.body, &[0, 0, 0, 0]);
        assert_eq!(page.codec, 0);
        assert_eq!(page.decoded_bytes, 4);
        assert_eq!(page.uncompressed_prefix, 0);
        assert!(page.compressed);
        assert_eq!(admitted.metadata.len(), 26);
        for restricted in [
            Limits {
                source_bytes: bytes.len() - 1,
                ..limits()
            },
            Limits {
                metadata_bytes: 25,
                ..limits()
            },
            Limits {
                metadata_nodes: 1,
                ..limits()
            },
            Limits {
                header_bytes: 16,
                ..limits()
            },
            Limits {
                header_nodes: 1,
                ..limits()
            },
            Limits {
                row_groups: 0,
                ..limits()
            },
            Limits {
                columns: 0,
                ..limits()
            },
            Limits {
                rows: 0,
                ..limits()
            },
            Limits {
                cells: 0,
                ..limits()
            },
            Limits {
                pages: 0,
                ..limits()
            },
            Limits {
                page_bytes: 3,
                ..limits()
            },
            Limits {
                decoded_bytes: 3,
                ..limits()
            },
        ] {
            assert!(
                matches!(admit(&bytes, restricted), Err(Error::Limit)),
                "{restricted:?}"
            );
        }
    }
    #[test]
    fn footer_bounds_page_body_lengths_and_v2_prefixes_are_not_trusted() {
        let mut bytes = container(&page());
        bytes[0] = 0;
        assert!(matches!(admit(&bytes, limits()), Err(Error::Malformed)));
        let mut bytes = container(&page());
        let footer = bytes.len() - 8;
        bytes[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(admit(&bytes, limits()), Err(Error::Limit)));
        let mut bad = page();
        bad[5] = 10; // claims five body bytes; only four exist
        assert!(matches!(
            admit(&container(&bad), limits()),
            Err(Error::Malformed)
        ));
        let mut bad = page();
        bad[3] = 0x7e; // claimed expansion exceeds decoded budget
        assert!(matches!(
            admit(
                &container(&bad),
                Limits {
                    decoded_bytes: 62,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        ));
        // V2: one value, zero nulls, one row, PLAIN, 3 definition bytes,
        // 2 repetition bytes, no compression; sizes claim only four bytes.
        let bad = [
            0x15, 6, 0x15, 8, 0x15, 8, 0x5c, 0x15, 2, 0x15, 0, 0x15, 2, 0x15, 0, 0x15, 6, 0x15, 4,
            0x12, 0, 0, 0, 0, 0, 0,
        ];
        assert!(matches!(
            admit(&container(&bad), limits()),
            Err(Error::Malformed)
        ));
        let mut good = bad;
        good[18] = 0; // Prefix is now three bytes, no compression.
        let bytes = container(&good);
        let admitted = admit(&bytes, limits()).unwrap();
        assert_eq!(admitted.pages[0].uncompressed_prefix, 3);
        assert!(!admitted.pages[0].compressed);
    }
    #[test]
    fn actual_page_value_work_is_bounded_even_if_footer_row_counts_are_small() {
        let mut pages = page();
        pages.extend(page());
        let bytes = container(&pages);
        assert_eq!(admit(&bytes, limits()).unwrap().pages.len(), 2);
        assert!(matches!(
            admit(
                &bytes,
                Limits {
                    cells: 1,
                    ..limits()
                }
            ),
            Err(Error::Limit)
        ));
    }
}
