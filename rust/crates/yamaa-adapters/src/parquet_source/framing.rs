//! Admission of held Parquet framing before the general codec sees metadata.
//! No decompression or semantic type decisions occur here.
use super::closed_profile;
use super::compact::{self, Sequence, Value};
use super::format::PageType;
use super::wire::{self, Shape};

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

/// One page of the held file, by where its header and body lie in it.
#[derive(Debug)]
pub(super) struct Page {
    pub codec: i64,
    pub header: std::ops::Range<usize>,
    pub body: std::ops::Range<usize>,
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
    /// How parquet 55's decoder is shown the footer (`wire::Presentation`),
    /// which the differential harness reads by.
    #[cfg(test)]
    pub footer: wire::Presentation,
    /// The leaf columns, in schema order, whose variant, geometry or
    /// geography annotation the footer's presentation removes: they still
    /// fall outside the closed mapping (PARQUET-7).
    pub annotated: Vec<usize>,
    pub pages: Vec<Page>,
    /// Where page headers hold a skipped binary whose bytes are not UTF-8,
    /// in the held file: shown to parquet 55's decoder as zero bytes. A file
    /// that holds one is presented, so no two of its pages may share a byte
    /// (`reader::refuse_shared_presented_pages`).
    pub opaque: Vec<std::ops::Range<usize>>,
    /// The row groups of each partition, in order (#3792): every row group
    /// for a whole read, and one empty partition for a file without any.
    pub partitions: Vec<std::ops::Range<usize>>,
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
// reserves num_children slots before discovering a truncated schema tree,
// and keeps each open group's slots while it reads the group's children.
// Admission therefore holds the claims of all open groups together to the
// elements left in the list, so that those reservations stay within the
// schema's length. Returns the primitive leaves, the schema's physical
// columns, and which of them are among the `annotated` elements (ascending
// positions in the schema list).
fn admit_schema(schema: Sequence<'_>, annotated: &[usize]) -> Result<(usize, Vec<usize>), Error> {
    let mut leaves = 0;
    let mut annotated_leaves = Vec::new();
    // Every element without children but the message root: a primitive leaf
    // or an empty group. An empty group holds no physical column, but the
    // native decoder builds it like a leaf, so it counts toward the column
    // limit too. Every other element is a group above one of these, at most
    // `SEMANTIC_DEPTH - 1` levels up, so an admitted schema holds at most
    // `COLUMNS * SEMANTIC_DEPTH + 1` elements.
    let mut childless = 0;
    let mut remaining_children = Vec::new();
    // The sum of `remaining_children`: the children that open groups claim
    // and that later elements must supply.
    let mut claimed = 0;
    for (index, element) in schema.into_iter().enumerate() {
        let element = element?;
        while remaining_children.last() == Some(&0) {
            remaining_children.pop();
        }
        if let Some(remaining) = remaining_children.last_mut() {
            *remaining -= 1;
            claimed -= 1;
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
        if index != 0 && children.is_none_or(|count| count == 0) {
            childless += 1;
            if matches!(element.field(1)?, Some(Value::Integer(_))) {
                if annotated.binary_search(&index).is_ok() {
                    annotated_leaves.push(leaves);
                }
                leaves += 1;
            }
        }
        if let Some(children) = children {
            // Each claimed child is a later element of its own, so this claim
            // must fit in the elements left beside the open groups' claims.
            // Those claims never exceed the elements left, as checked at the
            // previous element, so the subtraction cannot wrap.
            if children > schema.len() - index - 1 - claimed {
                return Err(Error::Malformed);
            }
            if children != 0 {
                remaining_children.push(children);
                claimed += children;
            }
        }
    }
    yamaa_core::limits::Limits::columns(childless)?;
    Ok((leaves, annotated_leaves))
}

/// Refuse a footer that lists more row groups than the row limit allows rows,
/// counting the row groups that hold no rows. Every other row group holds at
/// least one row, so a table within the limit needs no more, and writers
/// write a row group without rows only for an empty table.
fn admit_row_groups(count: usize) -> Result<(), Error> {
    // No columns: only the row bound applies to the count.
    Ok(yamaa_core::limits::Limits::table(count, 0)?)
}

/// Collect borrowed page bodies from every stored column chunk in source order.
/// The downstream codec remains responsible for physical format semantics.
pub(super) fn admit(bytes: &[u8]) -> Result<Framing<'_>, Error> {
    admit_partitioned(bytes, None)
}

/// [`admit`], dividing the row groups into partitions of at most `rows`
/// records each when `rows` is given (#3792): a partition takes the next row
/// group while the group fits, and a row group of more records is a
/// partition of its own. Each partition, not the whole file, is then
/// admitted within the row and cell limits, as its row groups are reached.
/// Without `rows`, the whole file is one partition, admitted as declared.
pub(super) fn admit_partitioned(
    bytes: &[u8],
    rows: Option<std::num::NonZeroUsize>,
) -> Result<Framing<'_>, Error> {
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
    // Every field the native decoder knows holds its declared type, once.
    let footer = wire::check(root, Shape::FileMetaData)?;
    let lists = footer.lists;
    let declared_rows = count(root.field(3)?)?;
    let groups = sequence(root.field(4)?)?;
    let schema = sequence(root.field(2)?)?;
    let (leaves, annotated) = admit_schema(schema, &footer.annotated)?;
    if rows.is_none() {
        yamaa_core::limits::Limits::table(declared_rows, leaves)?;
    }
    admit_row_groups(groups.len())?;
    let group_count = groups.len();
    // The column orders follow the leaves one for one: the native decoder
    // refuses any other count, but only after it has read them all. A row
    // group's sorting columns are a subset of its columns, at most one per
    // leaf.
    let orders = lists.column_orders.is_some_and(|orders| orders != leaves);
    if orders || lists.sorting_columns > leaves {
        return Err(Error::Malformed);
    }
    let mut result = Framing {
        metadata,
        #[cfg(test)]
        footer: footer.presentation,
        annotated,
        pages: Vec::new(),
        opaque: Vec::new(),
        partitions: Vec::new(),
    };
    // The records of the partition being reached, and its first row group.
    let (mut partition_rows, mut partition_start) = (0usize, 0);
    for (row_group, group) in groups.into_iter().enumerate() {
        let group = group?;
        let count_rows = count(group.field(3)?)?;
        if let Some(rows) = rows {
            let fits = partition_rows
                .checked_add(count_rows)
                .is_some_and(|sum| sum <= rows.get());
            if partition_rows > 0 && !fits {
                result.partitions.push(partition_start..row_group);
                (partition_rows, partition_start) = (0, row_group);
            }
        }
        partition_rows = partition_rows
            .checked_add(count_rows)
            .ok_or(Error::Malformed)?;
        let columns = sequence(group.field(1)?)?;
        yamaa_core::limits::Limits::table(partition_rows, columns.len())?;
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
                // A page header holds no list that `wire::Lists` reports, and
                // nothing that the presentation splices.
                let shown = wire::check(header, Shape::PageHeader)?.presentation;
                result.opaque.extend(
                    shown
                        .opaque
                        .into_iter()
                        .map(|range| offset + range.start..offset + range.end),
                );
                let uncompressed = count(header.field(2)?)?;
                let compressed = count(header.field(3)?)?;
                let kind = closed_profile::page(integer(header.field(1)?)?);
                let payload = match kind.ok_or(Error::Malformed)? {
                    PageType::DataPage => {
                        let detail = header.field(5)?.ok_or(Error::Malformed)?;
                        Payload::DataV1 {
                            encoding: integer(detail.field(2)?)?,
                            values: count(detail.field(1)?)?,
                            definition_encoding: integer(detail.field(3)?)?,
                            repetition_encoding: integer(detail.field(4)?)?,
                        }
                    }
                    PageType::IndexPage => Payload::Index,
                    PageType::DictionaryPage => {
                        let detail = header.field(7)?.ok_or(Error::Malformed)?;
                        Payload::Dictionary {
                            encoding: integer(detail.field(2)?)?,
                            values: count(detail.field(1)?)?,
                        }
                    }
                    PageType::DataPageV2 => {
                        let detail = header.field(8)?.ok_or(Error::Malformed)?;
                        Payload::DataV2 {
                            encoding: integer(detail.field(4)?)?,
                            values: count(detail.field(1)?)?,
                            nulls: count(detail.field(2)?)?,
                            definition_bytes: count(detail.field(5)?)?,
                            repetition_bytes: count(detail.field(6)?)?,
                        }
                    }
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
                let header = offset;
                let body_start = offset.checked_add(used).ok_or(Error::Malformed)?;
                offset = body_start
                    .checked_add(compressed)
                    .filter(|end_body| *end_body <= end)
                    .ok_or(Error::Malformed)?;
                result.pages.push(Page {
                    codec,
                    header: header..body_start,
                    body: body_start..offset,
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
    result.partitions.push(partition_start..group_count);
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
        // The one schema element holds only an empty name, the shortest
        // element the native decoder accepts. Other required Parquet schema
        // details are deliberately the next decoder's responsibility; this
        // fixture exercises framing, never a valid table.
        let footer = [
            0x15,
            2,
            0x19,
            0x1c,
            0x48,
            0,
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
        let bytes = [0x19, 0x3c, 0x55, 4, 0, 0x55, 0, 0, 0x15, 4, 0, 0];
        let (root, _) = compact::parse(&bytes).unwrap();
        let schema = sequence(root.field(1).unwrap()).unwrap();
        assert_eq!(admit_schema(schema, &[]).unwrap(), (1, Vec::new()));
    }

    #[test]
    fn empty_groups_count_toward_the_column_limit_without_holding_a_column() {
        // A root whose children are `primitives` INT64 leaves, then `empty`
        // elements with neither a type nor children.
        fn admit_flat(primitives: usize, empty: usize) -> Result<usize, Error> {
            let mut bytes = vec![0x19, 0xfc]; // field 1: extended struct list
            unsigned(&mut bytes, (1 + primitives + empty) as u64);
            bytes.push(0x55); // the root's num_children
            unsigned(&mut bytes, ((primitives + empty) as u64) << 1);
            bytes.push(0);
            for _ in 0..primitives {
                bytes.extend([0x15, 4, 0]);
            }
            bytes.resize(bytes.len() + empty, 0);
            bytes.push(0);
            let (root, _) = compact::parse(&bytes)?;
            admit_schema(sequence(root.field(1)?)?, &[]).map(|(leaves, _)| leaves)
        }
        let columns = |actual| {
            move |result: Result<usize, Error>| {
                matches!(result, Err(Error::ResourceLimit {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(found),
                }) if found == actual)
            }
        };
        assert_eq!(admit_flat(1023, 1).unwrap(), 1023);
        assert_eq!(admit_flat(0, 1024).unwrap(), 0);
        assert!(columns(1025)(admit_flat(1024, 1)));
        assert!(columns(1025)(admit_flat(0, 1025)));
        assert!(columns(2048)(admit_flat(1024, 1024)));
    }

    #[test]
    fn a_footer_lists_no_more_row_groups_than_the_row_limit_has_rows() {
        let rows = yamaa_core::limits::Limits::TABLE_ROWS;
        admit_row_groups(rows).unwrap();
        assert!(matches!(
            admit_row_groups(rows + 1),
            Err(Error::ResourceLimit {
                resource: "table_rows",
                limit,
                actual: Some(actual),
            }) if limit == rows && actual == rows + 1
        ));
    }

    #[test]
    fn malformed_physical_types_do_not_become_a_column_limit_refusal() {
        let mut footer = vec![0x15, 2, 0x19, 0xfc]; // version=1; schema list
        unsigned(&mut footer, 1026);
        footer.extend([0x48, 6]);
        footer.extend(b"schema");
        footer.push(0x15);
        unsigned(&mut footer, 2050); // 1025 children
        footer.push(0);
        for _ in 0..1025 {
            // Physical type field 1 is a bool, not the required enum integer.
            footer.extend([0x11, 0x25, 0, 0x18, 1, b'X', 0]);
        }
        footer.extend([0x16, 0, 0x19, 0x0c, 0]); // rows=0; row_groups=[]
        let mut bytes = b"PAR1".to_vec();
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        assert!(matches!(
            crate::parquet_source::reader::read(&bytes),
            Err(crate::parquet_source::reader::Error::Malformed)
        ));
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
        assert_eq!(decoded.rows, 0);
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
            assert!(matches!(
                crate::parquet_source::reader::read(&bytes),
                Err(crate::parquet_source::reader::Error::Malformed)
            ));
        }
    }

    /// Admit a flat schema list whose elements claim `children` children each:
    /// the root, then groups, and an INT64 primitive for each zero after it.
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
        admit_schema(sequence(root.field(1)?)?, &[]).map(|(leaves, _)| leaves)
    }

    #[test]
    fn schema_integer_claims_and_flattened_tree_recursion_are_checked() {
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
    fn open_groups_together_claim_no_more_children_than_elements_are_left() {
        // The native decoder reserves a group's claimed children before it
        // reads them and keeps the reservation while the group is open. Here
        // every claim fits in the elements after it on its own, but not
        // beside the claims of the groups above it.
        //
        // A root of three children, a group of one primitive and two
        // primitives, and a root of two groups of one primitive each: the
        // claims fill each list exactly.
        assert_eq!(admit_children(&[3, 1, 0, 0, 0]).unwrap(), 3);
        assert_eq!(admit_children(&[2, 1, 0, 1, 0]).unwrap(), 2);
        // The group claims two of the three elements left, while the root
        // still claims two more children: four children, three elements.
        assert!(matches!(
            admit_children(&[3, 2, 0, 0, 0]),
            Err(Error::Malformed)
        ));
        // Each group of this path claims every element after it, so together
        // they would reserve some 8,000 slots for a list of 128 elements.
        let depth = yamaa_core::limits::Limits::SEMANTIC_DEPTH as i64;
        let chain: Vec<i64> = (0..depth).rev().collect();
        assert!(matches!(admit_children(&chain), Err(Error::Malformed)));
    }

    #[test]
    fn page_ranges_locate_the_exact_header_body_and_metadata() {
        let bytes = container(&page());
        let admitted = admit(&bytes).unwrap();
        assert_eq!(admitted.pages.len(), 1);
        let page = &admitted.pages[0];
        // The page header is the 17 bytes after the magic number.
        assert_eq!((page.header.clone(), page.body.clone()), (4..21, 21..25));
        assert_eq!(bytes[page.body.clone()], [0, 0, 0, 0]);
        assert_eq!(page.codec, 0);
        assert_eq!(page.decoded_bytes, 4);
        assert_eq!(page.uncompressed_prefix, 0);
        assert!(page.compressed);
        assert_eq!(admitted.metadata.len(), 28);
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
        let footer_start = fixture.len() - 8 - 28;
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
            let mut bytes = fixture[..footer_start + 8].to_vec();
            bytes.extend(&encoded);
            bytes.extend(&fixture[footer_start + 9..fixture.len() - 8]);
            bytes.extend((27u32 + encoded.len() as u32).to_le_bytes());
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
        fn unsigned(bytes: &mut Vec<u8>, mut value: u64) {
            while value >= 128 {
                bytes.push(value as u8 | 128);
                value >>= 7;
            }
            bytes.push(value as u8);
        }
        let fixture = super::super::test_files::file("none-1-0");
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
    fn leaf_column_cell_boundary_keeps_the_one_row_edge() {
        for rows in [625_000, 625_001] {
            let bytes = geometry_container(rows, 1024, true);
            if rows == 625_000 {
                admit(&bytes).unwrap();
            } else {
                assert!(matches!(
                    admit(&bytes),
                    Err(Error::ResourceLimit {
                        resource: "table_cells",
                        limit: 640_000_000,
                        actual: Some(640_001_024),
                    })
                ));
            }
        }
    }
}
