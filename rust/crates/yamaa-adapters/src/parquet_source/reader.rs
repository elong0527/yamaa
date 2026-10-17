//! Complete held-container read, followed by closed-profile admission.
//! No filesystem reads, semantic fallback, publication or host date conversion.
use super::{compact, compression, framing, metadata, payload, physical, profile, schema};
use crate::storage_api::contain_decode;
use arrow_array::{
    ArrayRef, Date32Array, Float64Array, Int64Array, LargeStringArray, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::DataType;
use parquet::{
    arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions},
    file::metadata::{ParquetMetaData, ParquetMetaDataReader},
};
use std::{borrow::Cow, sync::Arc};
use yamaa_core::{table::TableSchema, value::ColumnType};

#[derive(Debug, PartialEq)]
pub(super) enum Error {
    Malformed,
    Representation,
    ResourceLimit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
    },
    Unavailable(i64),
    Profile(profile::Error),
    OdmFields(Vec<yamaa_core::odm_source::SchemaError>),
}
impl From<framing::Error> for Error {
    fn from(error: framing::Error) -> Self {
        match error {
            framing::Error::Malformed => Self::Malformed,
            framing::Error::ResourceLimit {
                resource,
                limit,
                actual,
            } => Self::ResourceLimit {
                resource,
                limit,
                actual,
            },
        }
    }
}
impl From<compact::Error> for Error {
    fn from(error: compact::Error) -> Self {
        match error {
            compact::Error::Malformed => Self::Malformed,
            compact::Error::Representation => Self::Representation,
        }
    }
}
impl From<compression::Error> for Error {
    fn from(error: compression::Error) -> Self {
        match error {
            compression::Error::Malformed => Self::Malformed,
            compression::Error::Unavailable(codec) => Self::Unavailable(codec),
        }
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
pub(super) struct Decoded {
    pub schema: TableSchema,
    pub batches: Vec<RecordBatch>,
}

pub(super) fn read(bytes: &[u8]) -> Result<Decoded, Error> {
    read_interpreted(bytes, false)
}

pub(super) fn read_odm(bytes: &[u8]) -> Result<Decoded, Error> {
    read_interpreted(bytes, true)
}

/// Reconcile page counts with their held chunk and row-group metadata before
/// any codec interprets a claimed value count as an allocation size. Repeated
/// columns may contain more values than rows. The ordinary physical decoder
/// may still retain storage for every admitted repeated value.
fn admit_page_counts(
    frame: &framing::Framing<'_>,
    metadata: &ParquetMetaData,
) -> Result<(), Error> {
    let descriptors = metadata.file_metadata().schema_descr().columns();
    let mut pages = frame.pages.iter().peekable();
    for (row_group, group) in metadata.row_groups().iter().enumerate() {
        let rows = usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?;
        for (column, chunk) in group.columns().iter().enumerate() {
            let expected = usize::try_from(chunk.num_values()).map_err(|_| Error::Malformed)?;
            let descriptor = descriptors.get(column).ok_or(Error::Malformed)?;
            if descriptor.max_rep_level() == 0 && expected != rows {
                return Err(Error::Malformed);
            }
            let mut values = 0usize;
            while pages
                .peek()
                .is_some_and(|page| page.row_group == row_group && page.column == column)
            {
                let page = pages.next().unwrap();
                let count = match page.payload {
                    framing::Payload::DataV1 { values, .. }
                    | framing::Payload::DataV2 { values, .. } => values,
                    framing::Payload::Dictionary { .. } | framing::Payload::Index => continue,
                };
                values = values
                    .checked_add(count)
                    .filter(|&count| count <= expected)
                    .ok_or(Error::Malformed)?;
            }
            if values != expected {
                return Err(Error::Malformed);
            }
        }
    }
    if pages.next().is_some() {
        return Err(Error::Malformed);
    }
    Ok(())
}

/// Copy the held file for the native page reader, presenting each page header
/// as the footer is presented to the metadata decoder. The presentation keeps
/// each header's length, so it is written in place and no offset moves. Page
/// bodies keep their held bytes.
fn native_pages(bytes: &[u8], frame: &framing::Framing<'_>) -> Result<bytes::Bytes, Error> {
    let mut file: Option<Vec<u8>> = None;
    for header in &frame.headers {
        if let Cow::Owned(native) = compact::native_metadata(&bytes[header.clone()])? {
            file.get_or_insert_with(|| bytes.to_vec())[header.clone()].copy_from_slice(&native);
        }
    }
    Ok(match file {
        Some(file) => bytes::Bytes::from(file),
        None => bytes::Bytes::copy_from_slice(bytes),
    })
}

fn read_interpreted(bytes: &[u8], odm: bool) -> Result<Decoded, Error> {
    let frame = framing::admit(bytes)?;
    let metadata = Arc::new({
        let native = compact::native_metadata(frame.metadata)?;
        contain_decode(|| ParquetMetaDataReader::decode_metadata(&native))
            .ok_or(Error::Malformed)?
            .map_err(|_| Error::Malformed)?
    });
    let hint = metadata::admit(metadata.file_metadata().key_value_metadata())?;
    let columns = metadata.file_metadata().schema_descr().columns();
    // ODM field admission uses metadata alone. Ordinary profile findings keep
    // their existing precedence after all physical chunks have been decoded.
    let odm_metadata = if odm {
        let arrow = contain_decode(|| {
            ArrowReaderMetadata::try_new(metadata.clone(), ArrowReaderOptions::new())
        })
        .ok_or(Error::Malformed)?
        .map_err(|_| Error::Malformed)?;
        let arrow_schema = schema::normalize(arrow.schema(), hint.as_ref());
        let fields = arrow_schema
            .fields()
            .iter()
            .map(|field| yamaa_core::odm_source::StoredField {
                name: field.name(),
                kind: columns
                    .iter()
                    .find(|column| {
                        column.max_rep_level() == 0 && column.path().string() == *field.name()
                    })
                    .and_then(|column| profile::column_type(column, field.data_type())),
            })
            .collect::<Vec<_>>();
        let binding = yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)?;
        let stored = binding.stored().map(|root| {
            columns
                .iter()
                .position(|column| column.path().string() == *arrow_schema.field(root).name())
                .expect("admitted flat ODM field")
        });
        Some((arrow_schema, stored))
    } else {
        None
    };
    admit_page_counts(&frame, &metadata)?;
    let mut payloads = payload::Validation::new();
    let mut pages = frame.pages.iter().peekable();
    for (row_group, group) in metadata.row_groups().iter().enumerate() {
        let expected = usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?;
        for column in 0..group.num_columns() {
            let descriptor = columns.get(column).ok_or(Error::Malformed)?;
            let mut rows = payload::Rows::new(expected);
            while pages
                .peek()
                .is_some_and(|page| page.row_group == row_group && page.column == column)
            {
                let page = pages.next().unwrap();
                let decoded = compression::decode(page)?;
                rows.admit(page, &decoded, descriptor)?;
                payloads.admit(page, &decoded, descriptor)?;
            }
            rows.finish()?;
        }
    }
    let row_count =
        usize::try_from(metadata.file_metadata().num_rows()).map_err(|_| Error::Malformed)?;
    let arrow = contain_decode(|| {
        ArrowReaderMetadata::try_new(metadata.clone(), ArrowReaderOptions::new())
    })
    .ok_or(Error::Malformed)?
    .map_err(|_| Error::Malformed)?;
    yamaa_core::limits::Limits::table(row_count, columns.len())?;
    let arrow_schema = schema::normalize(arrow.schema(), hint.as_ref());
    // Determine retention only. Profile errors remain deferred until every
    // physical chunk has been decoded, including fields outside the profile.
    let keep: Vec<_> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            if let Some((_, stored)) = &odm_metadata {
                return stored.contains(&index);
            }
            arrow_schema.fields().len() == columns.len()
                && arrow_schema.fields().get(index).is_some_and(|field| {
                    column.max_rep_level() == 0
                        && column.path().string() == *field.name()
                        && profile::column_type(column, field.data_type()).is_some()
                })
        })
        .collect();
    let source = native_pages(bytes, &frame)?;
    let raw = contain_decode(|| physical::read(source, &metadata, &keep, &arrow_schema))
        .ok_or(Error::Malformed)??;
    let group_rows = metadata
        .row_groups()
        .iter()
        .try_fold(0usize, |sum, group| {
            sum.checked_add(usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?)
                .ok_or(Error::Malformed)
        })?;
    if group_rows != row_count {
        return Err(Error::Malformed);
    }
    if let Some((_, stored)) = odm_metadata {
        return odm_decoded(raw, stored, row_count);
    }
    let schema = profile::columns(&arrow_schema, metadata.file_metadata().schema_descr())
        .map_err(Error::Profile)?;
    for (field, column) in schema.columns().iter().zip(&raw) {
        match (field.kind, column) {
            (ColumnType::Date, physical::Column::Int32(values)) => {
                for (row, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        profile::temporal(field.kind, i64::from(*value), &field.name, row + 1)
                            .map_err(Error::Profile)?;
                    }
                }
            }
            (ColumnType::DateTime, physical::Column::Int64(values)) => {
                for (row, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        profile::temporal(field.kind, *value, &field.name, row + 1)
                            .map_err(Error::Profile)?;
                    }
                }
            }
            _ => {}
        }
    }
    // The reference validates text when constructing its typed table, after
    // field and temporal findings. Never construct unchecked Arrow strings.
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for (index, (field, column)) in schema.columns().iter().zip(raw).enumerate() {
        let array: ArrayRef = match (field.kind, column) {
            (ColumnType::Str, physical::Column::Bytes(values)) => {
                let strings = values
                    .iter()
                    .map(|v| {
                        v.as_ref()
                            .map(|b| std::str::from_utf8(b.data()).map_err(|_| Error::Malformed))
                            .transpose()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !crate::arrow_table::fits_utf8_offsets(
                    strings.iter().flatten().map(|text| text.len()),
                ) {
                    return Err(Error::Representation);
                }
                if matches!(arrow_schema.field(index).data_type(), DataType::LargeUtf8) {
                    Arc::new(LargeStringArray::from(strings))
                } else {
                    Arc::new(StringArray::from(strings))
                }
            }
            (ColumnType::Int, physical::Column::Int64(values)) => {
                Arc::new(Int64Array::from(values))
            }
            (ColumnType::Float, physical::Column::Double(values)) => {
                Arc::new(Float64Array::from_iter(values.into_iter().map(|v| {
                    v.and_then(yamaa_core::value::FiniteFloat::new)
                        .map(yamaa_core::value::FiniteFloat::get)
                })))
            }
            (ColumnType::Date, physical::Column::Int32(values)) => {
                Arc::new(Date32Array::from(values))
            }
            (ColumnType::DateTime, physical::Column::Int64(values)) => {
                Arc::new(TimestampMicrosecondArray::from(values))
            }
            _ => return Err(Error::Malformed),
        };
        if array.len() != row_count {
            return Err(Error::Malformed);
        }
        arrays.push(array);
    }

    // Normalization can make a required stored float missing. The language's
    // typed table admits missing values for every column, regardless of storage.
    let output_schema = Arc::new(arrow_schema::Schema::new(
        arrow_schema
            .fields()
            .iter()
            .map(|field| arrow_schema::Field::new(field.name(), field.data_type().clone(), true))
            .collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(output_schema, arrays).map_err(|_| Error::Malformed)?;

    Ok(Decoded {
        schema,
        batches: vec![batch],
    })
}

fn odm_decoded(
    raw: Vec<physical::Column>,
    stored: [usize; 11],
    row_count: usize,
) -> Result<Decoded, Error> {
    use yamaa_core::{odm_source::FIELDS, table::Column};
    let schema = TableSchema::new(
        FIELDS
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .map_err(|_| Error::Malformed)?;
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(FIELDS.len());
    for index in stored {
        let physical::Column::Bytes(values) = raw.get(index).ok_or(Error::Malformed)? else {
            return Err(Error::Malformed);
        };
        let strings = values
            .iter()
            .map(|value| {
                value
                    .as_ref()
                    .map(|bytes| std::str::from_utf8(bytes.data()).map_err(|_| Error::Malformed))
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        if strings.len() != row_count {
            return Err(Error::Malformed);
        }
        if !crate::arrow_table::fits_utf8_offsets(strings.iter().flatten().map(|text| text.len())) {
            return Err(Error::Representation);
        }
        arrays.push(Arc::new(StringArray::from(strings)));
    }

    let output_schema = Arc::new(arrow_schema::Schema::new(
        FIELDS
            .iter()
            .map(|name| arrow_schema::Field::new(*name, DataType::Utf8, true))
            .collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(output_schema, arrays).map_err(|_| Error::Malformed)?;

    Ok(Decoded {
        schema,
        batches: vec![batch],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned(mut value: u64, output: &mut Vec<u8>) {
        while value >= 128 {
            output.push(value as u8 | 128);
            value >>= 7;
        }
        output.push(value as u8);
    }
    fn signed(value: i64, output: &mut Vec<u8>) {
        unsigned(((value as u64) << 1) ^ ((value >> 63) as u64), output);
    }
    fn progression(first: i64, step: i64, count: usize) -> Vec<u8> {
        let mut bytes = vec![128, 1, 4];
        unsigned(count as u64, &mut bytes);
        signed(first, &mut bytes);
        for _ in (1..count).step_by(128) {
            signed(step, &mut bytes);
            bytes.extend([0, 0, 0, 0]);
        }
        bytes
    }
    fn data_page(values: usize, body: &[u8]) -> Vec<u8> {
        let mut page = vec![0x15, 0, 0x15]; // DATA_PAGE; uncompressed size
        signed(body.len() as i64, &mut page);
        page.push(0x15); // compressed size
        signed(body.len() as i64, &mut page);
        page.extend([0x2c, 0x15]); // data header; num_values
        signed(values as i64, &mut page);
        page.extend([0x15, 14, 0x15, 6, 0x15, 6, 0, 0]); // DELTA_BYTE_ARRAY; RLE levels
        page.extend(body);
        page
    }
    fn rle_levels(runs: &[(u8, usize)]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for &(value, count) in runs {
            unsigned((count as u64) * 2, &mut encoded);
            encoded.push(value);
        }
        let mut bytes = (encoded.len() as u32).to_le_bytes().to_vec();
        bytes.extend(encoded);
        bytes
    }

    // Independently encoded Parquet metadata: one UTF8 BYTE_ARRAY column S,
    // with the chosen repetition and supplied physical data pages.
    fn container(
        repetition: i64,
        rows: usize,
        values: usize,
        codec: i64,
        pages: &[Vec<u8>],
    ) -> Vec<u8> {
        let mut bytes = b"PAR1".to_vec();
        for page in pages {
            bytes.extend(page);
        }
        let chunk = bytes.len() - 4;
        let mut footer = vec![0x15, 2, 0x19, 0x2c, 0x48, 6];
        footer.extend(b"schema");
        footer.extend([0x15, 2, 0, 0x15, 12, 0x25]); // root child; BYTE_ARRAY; repetition
        signed(repetition, &mut footer);
        footer.extend([0x18, 1, b'S', 0x25, 0, 0, 0x16]); // name; UTF8; file rows
        signed(rows as i64, &mut footer);
        footer.extend([
            0x19, 0x1c, 0x19, 0x1c, 0x26, 0,
            0x1c, // row group; chunk; file_offset=0; metadata
            0x15, 12, 0x19, 0x25, 6, 14, // BYTE_ARRAY; encodings=[RLE,DELTA_BYTE_ARRAY]
            0x19, 0x18, 1, b'S', 0x15, // path_in_schema; codec
        ]);
        signed(codec, &mut footer);
        footer.push(0x16); // num_values
        signed(values as i64, &mut footer);
        for _ in 0..2 {
            footer.push(0x16); // total uncompressed / compressed chunk size
            signed(chunk as i64, &mut footer);
        }
        footer.extend([0x26, 8, 0, 0, 0x16]); // data_page_offset=4; group total_byte_size
        signed(chunk as i64, &mut footer);
        footer.push(0x16); // group num_rows
        signed(rows as i64, &mut footer);
        footer.extend([0, 0]);
        bytes.extend(&footer);
        bytes.extend((footer.len() as u32).to_le_bytes());
        bytes.extend(b"PAR1");
        bytes
    }

    #[test]
    fn malformed_huge_page_counts_are_refused_before_any_codec_is_called() {
        let huge = i32::MAX as usize;
        // Codec 3 is a safe sentinel: the old order returns Unavailable before
        // any value allocation. The format preflight must instead see both a
        // page/chunk mismatch and a nonrepeated chunk/row mismatch first.
        for claimed in [1, huge] {
            let bytes = container(0, 1, claimed, 3, &[data_page(huge, &[])]);
            assert_eq!(read(&bytes).unwrap_err(), Error::Malformed);
        }
        // Repetition permits a larger physical value count than the row count.
        // Keep the ordinary unavailable-codec result instead of adding a cap.
        let bytes = container(2, 1, huge, 3, &[data_page(huge, &[])]);
        assert_eq!(read(&bytes).unwrap_err(), Error::Unavailable(3));
    }

    #[test]
    fn data_page_totals_must_match_the_complete_column_chunk() {
        let pages = [data_page(2, &[]), data_page(2, &[])];
        for declared in [3, 5] {
            let bytes = container(0, declared, declared, 3, &pages);
            assert_eq!(read(&bytes).unwrap_err(), Error::Malformed);
        }
    }

    #[test]
    fn a_known_boolean_footer_field_with_another_wire_type_is_malformed() {
        // RowGroup.sorting_columns = [{column_idx: 0, descending, nulls_first:
        // true}]. The native decoder unwraps a known boolean's value, so an i32
        // `descending` must be refused as malformed rather than panic.
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        for (descending, valid) in [(&[0x15u8, 0][..], false), (&[0x12][..], true)] {
            let mut bytes = container(0, 2, 2, 0, &[data_page(2, &body)]);
            let end = bytes.len() - 8;
            let mut sorting = vec![0x19, 0x1c, 0x15, 0];
            sorting.extend(descending);
            sorting.extend([0x11, 0]);
            // Before the row group's stop byte and the file metadata's.
            bytes.splice(end - 2..end - 2, sorting.iter().copied());
            let length = bytes.len() - 8;
            let footer = u32::from_le_bytes(bytes[length..length + 4].try_into().unwrap());
            let footer = footer + u32::try_from(sorting.len()).unwrap();
            bytes[length..length + 4].copy_from_slice(&footer.to_le_bytes());
            let decoded = read(&bytes);
            if valid {
                assert_eq!(decoded.unwrap().batches[0].num_rows(), 2);
            } else {
                assert_eq!(decoded.unwrap_err(), Error::Malformed);
            }
        }
    }

    /// Unknown fields that parquet 57.3.0's skip cannot step over as held,
    /// beginning with `set`, the header of the first field (a set):
    /// set<bool>{true, false}, map<byte, bool>{-1: true}, a UUID, list<uuid>
    /// and a 129-byte map<bool, i16>, whose binary length takes a padded
    /// prefix.
    fn unskippable_fields(set: u8) -> Vec<u8> {
        let mut fields = vec![set, 0x21, 1, 2, 0x1b, 1, 0x31, 0xff, 1, 0x1d];
        fields.extend([7; 16]);
        fields.extend([0x19, 0x1d]);
        fields.extend([9; 16]);
        fields.extend([0x1b, 63, 0x14]);
        fields.extend((0..62).flat_map(|_| [1, 0]));
        fields.extend([1, 0x80, 0x01]);
        fields
    }

    fn strings(decoded: &Decoded) -> Vec<Option<&str>> {
        decoded.batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .iter()
            .collect()
    }

    #[test]
    fn unskippable_unknown_footer_fields_keep_the_file_readable() {
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        let mut bytes = container(0, 2, 2, 0, &[data_page(2, &body)]);
        // Before the file metadata's stop byte; its last field is 4.
        let fields = unskippable_fields(0xba);
        let end = bytes.len() - 8;
        bytes.splice(end - 1..end - 1, fields.iter().copied());
        let length = bytes.len() - 8;
        let footer = u32::from_le_bytes(bytes[length..length + 4].try_into().unwrap());
        let footer = footer + u32::try_from(fields.len()).unwrap();
        bytes[length..length + 4].copy_from_slice(&footer.to_le_bytes());
        assert_eq!(strings(&read(&bytes).unwrap()), [Some("a"), Some("ab")]);
    }

    #[test]
    fn unskippable_unknown_page_header_fields_keep_the_page_readable() {
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        let mut page = data_page(2, &body);
        // Before the page header's stop byte; its last field is 5.
        let stop = page.len() - body.len() - 1;
        page.splice(stop..stop, unskippable_fields(0xaa));
        let bytes = container(0, 2, 2, 0, &[page]);
        assert_eq!(strings(&read(&bytes).unwrap()), [Some("a"), Some("ab")]);
    }

    #[test]
    fn valid_multi_page_delta_strings_keep_independent_page_prefix_state() {
        let mut first = progression(0, 1, 2);
        first.extend(progression(1, 0, 2));
        first.extend(b"ab"); // a, ab
        let mut second = progression(0, 0, 2);
        second.extend(progression(2, -2, 2));
        second.extend(b"bc"); // bc, empty
        let bytes = container(0, 4, 4, 0, &[data_page(2, &first), data_page(2, &second)]);
        let decoded = read(&bytes).unwrap();
        let strings = decoded.batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        assert_eq!(
            strings.iter().collect::<Vec<_>>(),
            [Some("a"), Some("ab"), Some("bc"), Some("")]
        );
    }

    #[test]
    fn null_rows_count_as_page_values_before_non_null_delta_admission() {
        let mut body = vec![6, 0, 0, 0, 2, 1, 2, 0, 2, 1]; // definitions 1,0,1
        body.extend(progression(0, 1, 2));
        body.extend(progression(1, 0, 2));
        body.extend(b"ab");
        // V2 stores the same definition stream without V1's four-byte length.
        let mut v2 = vec![0x15, 6, 0x15];
        signed((body.len() - 4) as i64, &mut v2);
        v2.push(0x15);
        signed((body.len() - 4) as i64, &mut v2);
        v2.extend([
            0x5c, 0x15, 6, 0x15, 2, 0x15, 6, 0x15, 14, // values=3; nulls=1; rows=3; DELTA
            0x15, 12, 0x15, 0, 0x12, 0, 0, // definition bytes=6; no repetitions/compression
        ]);
        v2.extend(&body[4..]);
        for page in [data_page(3, &body), v2] {
            let bytes = container(1, 3, 3, 0, &[page]);
            let decoded = read(&bytes).unwrap();
            let strings = decoded.batches[0]
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(
                strings.iter().collect::<Vec<_>>(),
                [Some("a"), None, Some("ab")]
            );
        }
    }

    #[test]
    fn repeated_delta_values_keep_profile_and_malformed_payload_precedence() {
        let levels = [4, 0, 0, 0, 2, 0, 4, 1, 2, 0, 0, 0, 6, 1]; // rep=0,1,1; def=1,1,1
        for (first_prefix, malformed) in [(0, false), (1, true)] {
            let mut body = levels.to_vec();
            body.extend(progression(first_prefix, 1, 3));
            body.extend(progression(1, 0, 3));
            body.extend(b"abc");
            let bytes = container(2, 1, 3, 0, &[data_page(3, &body)]);
            let error = read(&bytes).unwrap_err();
            if malformed {
                assert_eq!(error, Error::Malformed);
            } else {
                assert!(
                    matches!(error, Error::Profile(profile::Error::Unsupported { field, .. }) if field == "S")
                );
            }
        }
    }

    #[test]
    fn huge_repeated_row_mismatch_precedes_a_safe_delta_representation_refusal() {
        let values = i32::MAX as usize;
        let non_null = 65_536;
        let mut body = rle_levels(&[(0, values)]);
        body.extend(rle_levels(&[(1, non_null), (0, values - non_null)]));
        body.extend(progression(0, 1, non_null));
        body.extend(progression(1, 0, non_null));
        body.resize(body.len() + non_null, b'x');
        // Only 65,536 strings and 64 KiB of suffixes reach DELTA in the old
        // ordering, which safely refuses their triangular reconstructed size.
        // The repeated row truth must now reject first, without authoring the
        // unsafe i32::MAX-prefix allocation used by the original report.
        let bytes = container(2, 1, values, 0, &[data_page(values, &body)]);
        assert_eq!(read(&bytes).unwrap_err(), Error::Malformed);
    }

    #[test]
    fn repeated_rows_include_empty_markers_and_continue_across_v1_pages() {
        let mut first = rle_levels(&[(0, 1), (1, 1)]);
        first.extend(rle_levels(&[(1, 2)]));
        first.extend(progression(0, 0, 2));
        first.extend(progression(1, 0, 2));
        first.extend(b"ab");
        let mut second = rle_levels(&[(1, 1), (0, 2), (1, 1)]);
        second.extend(rle_levels(&[(1, 1), (0, 1), (1, 2)]));
        second.extend(progression(0, 0, 3));
        second.extend(progression(1, 0, 3));
        second.extend(b"cde");
        // Rows are [a,b,c], [], [d,e]. The second page starts with a continued
        // value; its empty-list marker is still a row although it stores none.
        let pages = [data_page(2, &first), data_page(4, &second)];
        let bytes = container(2, 3, 6, 0, &pages);
        assert!(
            matches!(read(&bytes), Err(Error::Profile(profile::Error::Unsupported { field, .. })) if field == "S")
        );
        for rows in [2, 4] {
            let bytes = container(2, rows, 6, 0, &pages);
            assert_eq!(read(&bytes).unwrap_err(), Error::Malformed);
        }
    }
}
