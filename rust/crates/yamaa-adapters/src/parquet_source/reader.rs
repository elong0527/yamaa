//! Complete held-container read, followed by closed-profile admission.
//! No filesystem reads, semantic fallback, publication or host date conversion.
use super::{
    compact, compression, framing, metadata, payload, physical, profile, row_groups, schema,
};
use crate::storage_api::contain_decode;
use arrow_array::{
    ArrayRef, Date32Array, Float64Array, Int64Array, LargeStringArray, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::DataType;
use parquet::file::metadata::ParquetMetaData;
use std::{borrow::Cow, sync::Arc};
use yamaa_core::{table::TableSchema, value::ColumnType};

#[derive(Clone, Debug, PartialEq)]
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

/// Where a finding of one partition's checks falls in the whole read's
/// order (#3830): its stage, the column the stage was checking, and the step
/// of that column's check. The whole read runs each stage over every
/// partition before the next stage, a column at a time where it checks
/// columns, so findings order by position, then by partition.
pub(super) type Position = (u8, usize, u8);

/// The ODM fields' text (ODM-1).
const ODM_TEXT: u8 = 12;
/// Temporal values (PARQUET-5).
const TEMPORAL: u8 = 14;
/// Each column's arrays: its text, then its offsets, then its length.
const ARRAYS: u8 = 15;
/// Each partition's record batch.
const BATCH: u8 = 16;

/// A held file read by partition (#3792): what its footer and pages admit,
/// and what decodes one partition's row groups at a time, each partition
/// once for its checks and again whenever a run reads it (#3830).
pub(super) struct Runs {
    /// The pages, as the native decoder is shown them.
    source: bytes::Bytes,
    metadata: Arc<ParquetMetaData>,
    keep: Vec<bool>,
    arrow_schema: arrow_schema::Schema,
    /// The ODM fields' stored columns, or the profile's table: what each
    /// partition's stored columns become.
    shape: Result<Shape, Error>,
    /// Whether the row groups hold the records the file declares.
    groups_hold_rows: bool,
    /// The row groups of each partition, and its records.
    partitions: Vec<(std::ops::Range<usize>, usize)>,
}

enum Shape {
    Odm([usize; 11]),
    Ordinary(TableSchema),
}

pub(super) fn read(bytes: &[u8]) -> Result<Decoded, Error> {
    whole(bytes, false)
}

pub(super) fn read_odm(bytes: &[u8]) -> Result<Decoded, Error> {
    whole(bytes, true)
}

/// The whole read: the file as one partition.
fn whole(bytes: &[u8], odm: bool) -> Result<Decoded, Error> {
    let runs = Runs::open(bytes, odm, None)?;
    let raw = runs.raw(0)?;
    runs.deferred()?;
    runs.decoded(raw, 0, 0).map_err(|(_, error)| error)
}

/// The table schema the held file decodes to, from its footer alone
/// (#3792): the fields [`read`], or with `odm` [`read_odm`], gives the table
/// when it finds nothing. No page is decoded, and each row group, not the
/// whole file, is admitted within the row and cell limits.
pub(super) fn table_schema(bytes: &[u8], odm: bool) -> Result<TableSchema, Error> {
    let frame = framing::admit_partitioned(bytes, Some(std::num::NonZeroUsize::MIN))?;
    let metadata = row_groups::decode(frame.metadata, &frame.footer)?;
    let hint = metadata::admit(metadata.file_metadata().key_value_metadata())?;
    if odm {
        odm_fields(&metadata, hint.as_ref(), &frame.annotated)?;
        return odm_schema();
    }
    let arrow = native_schema(&metadata, hint.as_ref())?;
    let arrow_schema = schema::normalize(&arrow, hint.as_ref().map(|hint| &hint.schema));
    let later = hint.as_ref().map_or(&[][..], |hint| &hint.later);
    profile::columns(
        &arrow_schema,
        metadata.file_metadata().schema_descr(),
        &frame.annotated,
        later,
    )
    .map_err(Error::Profile)
}

/// Each stored column's schema facts, from the admitted footer alone: no page
/// is decoded and no profile is applied.
pub(super) fn signature(bytes: &[u8]) -> Result<Vec<super::ColumnSignature>, Error> {
    let frame = framing::admit(bytes)?;
    let metadata = row_groups::decode(frame.metadata, &frame.footer)?;
    Ok(metadata
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| super::ColumnSignature {
            path: column.path().string(),
            physical_type: format!("{:?}", column.physical_type()),
            logical_type: column.logical_type().map(|logical| format!("{logical:?}")),
            converted_type: format!("{:?}", column.converted_type()),
            precision: column.type_precision(),
            scale: column.type_scale(),
            max_definition_level: column.max_def_level(),
            max_repetition_level: column.max_rep_level(),
        })
        .collect())
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
    for (header, _) in &frame.headers {
        if let Cow::Owned(native) = compact::native_metadata(&bytes[header.clone()])? {
            file.get_or_insert_with(|| bytes.to_vec())[header.clone()].copy_from_slice(&native);
        }
    }
    // A skipped binary that is not UTF-8 (`wire::Presentation`).
    for opaque in &frame.opaque {
        file.get_or_insert_with(|| bytes.to_vec())[opaque.clone()].fill(0);
    }
    let Some(file) = file else {
        return Ok(bytes::Bytes::copy_from_slice(bytes));
    };
    // Once a header is presented, no two pages may share a byte, or the native
    // reader could read header or body bytes that admission never saw. Written
    // files never overlap pages, and a file with nothing to present is read
    // exactly as held.
    let mut extents: Vec<_> = frame
        .headers
        .iter()
        .map(|(header, end)| (header.start, *end))
        .collect();
    extents.sort_unstable();
    if extents.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err(Error::Malformed);
    }
    Ok(bytes::Bytes::from(file))
}

/// The Arrow schema of the admitted file `metadata`, as the native conversion
/// gives it with the file's embedded schema `hint`.
fn native_schema(
    metadata: &ParquetMetaData,
    hint: Option<&metadata::Hint>,
) -> Result<arrow_schema::Schema, Error> {
    let held = hint.and_then(metadata::Hint::held);
    contain_decode(|| crate::storage_api::arrow_schema(metadata, held))
        .ok_or(Error::Malformed)?
        .map_err(|_| Error::Malformed)
}

/// The leaf columns that store the eleven ODM fields of the admitted file
/// `metadata`, in their canonical order (ODM-1, NAME-8). Metadata alone
/// decides them.
fn odm_fields(
    metadata: &ParquetMetaData,
    hint: Option<&metadata::Hint>,
    annotated: &[usize],
) -> Result<[usize; 11], Error> {
    let columns = metadata.file_metadata().schema_descr().columns();
    let arrow = native_schema(metadata, hint)?;
    let arrow_schema = schema::normalize(&arrow, hint.map(|hint| &hint.schema));
    let fields = arrow_schema
        .fields()
        .iter()
        .map(|field| yamaa_core::odm_source::StoredField {
            name: field.name(),
            kind: columns
                .iter()
                .enumerate()
                .find(|(_, column)| {
                    column.max_rep_level() == 0 && column.path().string() == *field.name()
                })
                .and_then(|(index, column)| {
                    profile::column_type(column, field.data_type(), annotated.contains(&index))
                }),
        })
        .collect::<Vec<_>>();
    let binding = yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)?;
    Ok(binding.stored().map(|root| {
        columns
            .iter()
            .position(|column| column.path().string() == *arrow_schema.field(root).name())
            .expect("admitted flat ODM field")
    }))
}

/// The table schema of an ODM read: the eleven canonical text fields.
fn odm_schema() -> Result<TableSchema, Error> {
    use yamaa_core::{odm_source::FIELDS, table::Column};
    TableSchema::new(
        FIELDS
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .map_err(|_| Error::Malformed)
}

impl Runs {
    /// Admit the held file for a read by partition (#3792): its row groups
    /// divided into partitions of at most `rows` records each, or, without
    /// `rows`, one partition holding the whole file. Every page is decoded
    /// and checked here, as the whole read checks it before any column is
    /// built, and dropped; each partition, not the whole file, is admitted
    /// within the row and cell limits. The checks the whole read makes after
    /// every partition's columns are decoded wait for [`Self::deferred`].
    pub(super) fn open(
        bytes: &[u8],
        odm: bool,
        rows: Option<std::num::NonZeroUsize>,
    ) -> Result<Self, Error> {
        let frame = framing::admit_partitioned(bytes, rows)?;
        let metadata = Arc::new(row_groups::decode(frame.metadata, &frame.footer)?);
        let hint = metadata::admit(metadata.file_metadata().key_value_metadata())?;
        let held_hint = hint.as_ref().map(|hint| &hint.schema);
        let columns = metadata.file_metadata().schema_descr().columns();
        let annotated = |column: usize| frame.annotated.contains(&column);
        // ODM field admission uses metadata alone. Ordinary profile findings
        // keep their existing precedence after all physical chunks have been
        // decoded.
        let odm_stored = if odm {
            Some(odm_fields(&metadata, hint.as_ref(), &frame.annotated)?)
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
        let arrow = native_schema(&metadata, hint.as_ref())?;
        // A whole read admits the rows the file declares; a read by
        // partition admits each partition's rows, which its row groups hold.
        let partitions = frame
            .partitions
            .iter()
            .map(|groups| {
                let groups_metadata = metadata
                    .row_groups()
                    .get(groups.clone())
                    .ok_or(Error::Malformed)?;
                let records = groups_metadata.iter().try_fold(0usize, |sum, group| {
                    sum.checked_add(
                        usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?,
                    )
                    .ok_or(Error::Malformed)
                })?;
                Ok((groups.clone(), records))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        match rows {
            None => yamaa_core::limits::Limits::table(row_count, columns.len())?,
            Some(_) => {
                for &(_, records) in &partitions {
                    yamaa_core::limits::Limits::table(records, columns.len())?;
                }
            }
        }
        let arrow_schema = schema::normalize(&arrow, held_hint);
        // Determine retention only. Profile errors remain deferred until
        // every physical chunk has been decoded, including fields outside the
        // profile.
        let keep: Vec<_> = columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                if let Some(stored) = &odm_stored {
                    return stored.contains(&index);
                }
                arrow_schema.fields().len() == columns.len()
                    && arrow_schema.fields().get(index).is_some_and(|field| {
                        column.max_rep_level() == 0
                            && column.path().string() == *field.name()
                            && profile::column_type(column, field.data_type(), annotated(index))
                                .is_some()
                    })
            })
            .collect();
        let source = native_pages(bytes, &frame)?;
        let group_rows = metadata
            .row_groups()
            .iter()
            .try_fold(0usize, |sum, group| {
                sum.checked_add(usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?)
                    .ok_or(Error::Malformed)
            })?;
        let shape = match odm_stored {
            Some(stored) => Ok(Shape::Odm(stored)),
            None => {
                let later = hint.as_ref().map_or(&[][..], |hint| &hint.later);
                profile::columns(
                    &arrow_schema,
                    metadata.file_metadata().schema_descr(),
                    &frame.annotated,
                    later,
                )
                .map(Shape::Ordinary)
                .map_err(Error::Profile)
            }
        };
        Ok(Self {
            source,
            groups_hold_rows: group_rows == row_count,
            metadata,
            keep,
            arrow_schema,
            shape,
            partitions,
        })
    }

    /// The number of partitions, at least one.
    pub(super) fn len(&self) -> usize {
        self.partitions.len()
    }

    /// The records of partition `partition`.
    pub(super) fn records(&self, partition: usize) -> usize {
        self.partitions[partition].1
    }

    /// Partition `partition`'s stored columns, decoded from its row groups.
    /// The whole read decodes every partition's before any later check, so
    /// the first partition that does not decode is its finding.
    pub(super) fn raw(&self, partition: usize) -> Result<Vec<physical::Column>, Error> {
        self.groups(self.partitions[partition].0.clone())
    }

    /// The stored columns of row groups `groups`.
    fn groups(&self, groups: std::ops::Range<usize>) -> Result<Vec<physical::Column>, Error> {
        physical::read(
            self.source.clone(),
            &self.metadata,
            &self.keep,
            &self.arrow_schema,
            groups,
        )
    }

    /// The checks of the whole file the whole read makes once every
    /// partition's stored columns are decoded: the row groups hold the
    /// records the file declares, and the profile admits its fields.
    pub(super) fn deferred(&self) -> Result<(), Error> {
        if !self.groups_hold_rows {
            return Err(Error::Malformed);
        }
        match &self.shape {
            Ok(_) => Ok(()),
            Err(error) => Err(error.clone()),
        }
    }

    /// Partition `partition`'s table from its stored columns `raw`, whose
    /// first record is record `before` of the file, or its first finding
    /// with the finding's [`Position`]. A value's row is its row in the
    /// whole file. Call it only once [`Self::deferred`] admits the file.
    pub(super) fn decoded(
        &self,
        raw: Vec<physical::Column>,
        partition: usize,
        before: usize,
    ) -> Result<Decoded, (Position, Error)> {
        let records = self.records(partition);
        match &self.shape {
            Ok(Shape::Odm(stored)) => odm_decoded(raw, *stored, records),
            Ok(Shape::Ordinary(schema)) => self.ordinary(raw, schema, records, before),
            Err(error) => Err(((0, 0, 0), error.clone())),
        }
    }

    /// The table of the row groups that hold records `range` of the file,
    /// and the position of the range's first record in it, once the scan
    /// has admitted the file (#3830).
    pub(super) fn range(&self, range: std::ops::Range<usize>) -> Result<(Decoded, usize), Error> {
        let (mut start, mut groups, mut offset, mut records) = (0usize, None, 0, 0);
        for (index, group) in self.metadata.row_groups().iter().enumerate() {
            let rows = usize::try_from(group.num_rows()).map_err(|_| Error::Malformed)?;
            let end = start.checked_add(rows).ok_or(Error::Malformed)?;
            if start < range.end && end > range.start {
                match &mut groups {
                    None => {
                        groups = Some(index..index + 1);
                        offset = range.start - start;
                    }
                    Some(groups) => groups.end = index + 1,
                }
                records += rows;
            }
            start = end;
        }
        let raw = self.groups(groups.unwrap_or(0..0))?;
        let decoded = match &self.shape {
            Ok(Shape::Odm(stored)) => odm_decoded(raw, *stored, records),
            Ok(Shape::Ordinary(schema)) => self.ordinary(raw, schema, records, 0),
            Err(error) => return Err(error.clone()),
        }
        .map_err(|(_, error)| error)?;
        Ok((decoded, offset))
    }

    /// An ordinary partition's table: its temporal values, then its arrays a
    /// column at a time, then its record batch, as the whole read checks its
    /// one partition.
    fn ordinary(
        &self,
        raw: Vec<physical::Column>,
        schema: &TableSchema,
        records: usize,
        before: usize,
    ) -> Result<Decoded, (Position, Error)> {
        let columns = self.metadata.file_metadata().schema_descr().columns();
        for (index, field) in schema.columns().iter().enumerate() {
            let at = |error| ((TEMPORAL, index, 0), Error::Profile(error));
            match (field.kind, raw.get(index)) {
                (ColumnType::Date, Some(physical::Column::Int32(values))) => {
                    for (row, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            let row = before + row + 1;
                            profile::temporal(field.kind, i64::from(*value), &field.name, row)
                                .map_err(at)?;
                        }
                    }
                }
                (ColumnType::DateTime, Some(physical::Column::Int64(values))) => {
                    for (row, value) in values.iter().enumerate() {
                        if let Some(value) = value {
                            let row = before + row + 1;
                            profile::temporal(field.kind, *value, &field.name, row).map_err(at)?;
                        }
                    }
                }
                _ => {}
            }
        }
        // The reference validates text when constructing its typed table,
        // after field and temporal findings. Never construct unchecked Arrow
        // strings.
        let mut arrays = Vec::new();
        let mut raw = raw.into_iter();
        for (index, field) in schema.columns().iter().enumerate() {
            // As the whole read's columns end with the shorter list.
            let Some(stored) = raw.next() else {
                break;
            };
            let large = || {
                matches!(
                    self.arrow_schema.field(index).data_type(),
                    DataType::LargeUtf8
                )
            };
            let declared = columns.get(index).and_then(|c| profile::declared_width(c));
            let array = column_array(field.kind, stored, large, declared)
                .map_err(|(step, error)| ((ARRAYS, index, step), error))?;
            if array.len() != records {
                return Err(((ARRAYS, index, 2), Error::Malformed));
            }
            arrays.push(array);
        }
        // Normalization can make a required stored float missing. The
        // language's typed table admits missing values for every column,
        // regardless of storage. Each field takes its array's type: a view or
        // dictionary hint names only a layout (PARQUET-7), and the values
        // above are already plain.
        let output_schema = Arc::new(arrow_schema::Schema::new(
            self.arrow_schema
                .fields()
                .iter()
                .zip(&arrays)
                .map(|(field, array)| {
                    arrow_schema::Field::new(field.name(), array.data_type().clone(), true)
                })
                .collect::<Vec<_>>(),
        ));
        let batch = RecordBatch::try_new(output_schema, arrays)
            .map_err(|_| ((BATCH, 0, 0), Error::Malformed))?;
        Ok(Decoded {
            schema: schema.clone(),
            batches: vec![batch],
        })
    }
}

/// One column's array from one partition's stored values, or the step of
/// its check that failed, with the finding: text is validated (step 0)
/// before its offsets are checked (step 1), and the whole read validates
/// every partition's text before any partition's offsets.
fn column_array(
    kind: ColumnType,
    stored: physical::Column,
    large: impl Fn() -> bool,
    declared: Option<u8>,
) -> Result<ArrayRef, (u8, Error)> {
    if kind == ColumnType::Str {
        let physical::Column::Bytes(values) = &stored else {
            return Err((0, Error::Malformed));
        };
        let strings = values
            .iter()
            .map(|v| {
                v.as_ref()
                    .map(|b| std::str::from_utf8(b.data()).map_err(|_| (0, Error::Malformed)))
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !crate::column_table::fits_utf8_offsets(strings.iter().flatten().map(|text| text.len()))
        {
            return Err((1, Error::Representation));
        }
        return Ok(if large() {
            Arc::new(LargeStringArray::from(strings))
        } else {
            Arc::new(StringArray::from(strings))
        });
    }
    Ok(match (kind, stored) {
        (ColumnType::Int, physical::Column::Int64(values)) => Arc::new(Int64Array::from(values)),
        // A signed annotation declares the width of every stored value. The
        // format forbids writing a value outside it and leaves reading one
        // to each reader, so such a value fails rather than reads as one of
        // their answers. Every stored value fits 32 bits, with or without an
        // annotation that says so.
        (ColumnType::Int, physical::Column::Int32(values)) => {
            if let Some(bits @ (8 | 16)) = declared {
                let bound = 1i64 << (bits - 1);
                if values
                    .iter()
                    .flatten()
                    .any(|value| !(-bound..bound).contains(&i64::from(*value)))
                {
                    return Err((0, Error::Malformed));
                }
            }
            Arc::new(Int64Array::from_iter(
                values.into_iter().map(|value| value.map(i64::from)),
            ))
        }
        (ColumnType::Float, physical::Column::Double(values)) => {
            Arc::new(Float64Array::from_iter(values.into_iter().map(|v| {
                v.and_then(yamaa_core::value::FiniteFloat::new)
                    .map(yamaa_core::value::FiniteFloat::get)
            })))
        }
        (ColumnType::Date, physical::Column::Int32(values)) => Arc::new(Date32Array::from(values)),
        (ColumnType::DateTime, physical::Column::Int64(values)) => {
            Arc::new(TimestampMicrosecondArray::from(values))
        }
        _ => return Err((0, Error::Malformed)),
    })
}

/// One partition of an ODM read: its eleven fields' text, a field at a
/// time, each field's text validated (step 0), then its length (step 1),
/// then its offsets (step 2), as the whole read checks each field over
/// every partition before the next step.
fn odm_decoded(
    raw: Vec<physical::Column>,
    stored: [usize; 11],
    records: usize,
) -> Result<Decoded, (Position, Error)> {
    use yamaa_core::odm_source::FIELDS;
    let schema = odm_schema().map_err(|error| ((ODM_TEXT, 0, 0), error))?;
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(FIELDS.len());
    for (position, index) in stored.into_iter().enumerate() {
        let malformed = |step| ((ODM_TEXT, position, step), Error::Malformed);
        let Some(physical::Column::Bytes(values)) = raw.get(index) else {
            return Err(malformed(0));
        };
        let strings = values
            .iter()
            .map(|value| {
                value
                    .as_ref()
                    .map(|bytes| std::str::from_utf8(bytes.data()).map_err(|_| malformed(0)))
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        if strings.len() != records {
            return Err(malformed(1));
        }
        if !crate::column_table::fits_utf8_offsets(strings.iter().flatten().map(|text| text.len()))
        {
            return Err(((ODM_TEXT, position, 2), Error::Representation));
        }
        arrays.push(Arc::new(StringArray::from(strings)));
    }
    let output_schema = Arc::new(arrow_schema::Schema::new(
        FIELDS
            .iter()
            .map(|name| arrow_schema::Field::new(*name, DataType::Utf8, true))
            .collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(output_schema, arrays)
        .map_err(|_| ((BATCH, 0, 0), Error::Malformed))?;
    Ok(Decoded {
        schema,
        batches: vec![batch],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::file::metadata::ParquetMetaDataReader;

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

    #[test]
    fn a_known_boolean_page_header_field_with_another_wire_type_is_malformed() {
        // A PLAIN dictionary page {a, ab}, then an RLE_DICTIONARY data page
        // [a, ab]. The native page reader unwraps a known boolean's value, so
        // an i32 DictionaryPageHeader.is_sorted must be refused as malformed
        // rather than panic; false is the control.
        let indices = [1, 3, 2]; // bit width 1; one bit-packed group 0, 1
        let mut data = data_page(2, &indices);
        let encoding = data.len() - indices.len() - 7;
        data[encoding] = 16; // RLE_DICTIONARY in place of DELTA_BYTE_ARRAY
        for (sorted, valid) in [(&[0x15u8, 0][..], false), (&[0x12][..], true)] {
            // DICTIONARY_PAGE; both sizes 11; header: 2 values, PLAIN
            let mut dictionary = vec![0x15, 4, 0x15, 22, 0x15, 22, 0x4c, 0x15, 4, 0x15, 0];
            dictionary.extend(sorted);
            dictionary.extend([0, 0, 1, 0, 0, 0, b'a', 2, 0, 0, 0, b'a', b'b']);
            let decoded = read(&container(0, 2, 2, 0, &[dictionary, data.clone()]));
            if valid {
                assert_eq!(strings(&decoded.unwrap()), [Some("a"), Some("ab")]);
            } else {
                assert_eq!(decoded.unwrap_err(), Error::Malformed);
            }
        }
    }

    /// Unknown fields that the native decoder's skip cannot step over as held,
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

    /// Where a two-row `container` writes the file metadata's row_groups
    /// field header: after version, schema and num_rows.
    fn row_groups(bytes: &[u8]) -> usize {
        let trailer = bytes.len() - 8;
        let length = u32::from_le_bytes(bytes[trailer..trailer + 4].try_into().unwrap());
        let position = trailer - length as usize + 27;
        // num_rows 2, then row_groups: a list of one struct.
        assert_eq!(bytes[position - 2..position + 2], [0x16, 4, 0x19, 0x1c]);
        position
    }

    /// Replace `range` of the footer with `with`, and update the footer
    /// length in the trailer.
    fn splice_footer(bytes: &mut Vec<u8>, range: std::ops::Range<usize>, with: &[u8]) {
        let trailer = bytes.len() - 8;
        let length = u32::from_le_bytes(bytes[trailer..trailer + 4].try_into().unwrap());
        let length = length as usize - range.len() + with.len();
        bytes.splice(range, with.iter().copied());
        let trailer = bytes.len() - 8;
        let length = u32::try_from(length).unwrap().to_le_bytes();
        bytes[trailer..trailer + 4].copy_from_slice(&length);
    }

    #[test]
    fn unskippable_unknown_footer_fields_keep_the_file_readable() {
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        let mut bytes = container(0, 2, 2, 0, &[data_page(2, &body)]);
        // The native decoder ends the file metadata at the first stop byte it
        // reads and ignores the bytes after it, so after the last field a
        // wrongly presented value, such as a binary prefix of the wrong width,
        // could still decode. Before row_groups, a misread loses that
        // required field. The fields follow num_rows (3), so row_groups takes
        // an explicit id.
        let position = row_groups(&bytes);
        let mut fields = unskippable_fields(0xca);
        fields.extend([0x09, 8]); // a list, field id 4
        splice_footer(&mut bytes, position..position + 1, &fields);
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
    fn presented_page_headers_refuse_pages_that_share_bytes() {
        // A page header holding only an unknown empty set, which the native
        // copy presents as a list, then a one-byte body.
        let presented = [0x1a, 0x01, 0, 0xee];
        let frame = |headers| framing::Framing {
            metadata: &[],
            footer: Default::default(),
            annotated: Vec::new(),
            pages: Vec::new(),
            headers,
            opaque: Vec::new(),
            partitions: Vec::new(),
        };
        let mut file = presented.to_vec();
        file.extend(presented);
        let native = native_pages(&file, &frame(vec![(0..3, 4), (4..7, 8)])).unwrap();
        assert_eq!(native.as_ref(), [0x19, 0x01, 0, 0xee, 0x19, 0x01, 0, 0xee]);
        // Two chunks that name the same page share its bytes.
        let shared = frame(vec![(0..3, 4), (0..3, 4)]);
        assert_eq!(
            native_pages(&presented, &shared).unwrap_err(),
            Error::Malformed
        );
        // With nothing to present, the held bytes are read as they are.
        let held = [0x19, 0x01, 0, 0xee];
        assert_eq!(native_pages(&held, &shared).unwrap().as_ref(), held);
    }

    /// The shared-bytes guard of `native_pages`, end to end: two row groups
    /// whose chunks name the same page.
    #[test]
    fn a_file_whose_chunks_name_one_presented_page_is_malformed() {
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        let plain = data_page(2, &body);
        // The same page, with unknown fields that its header must present.
        let mut unskippable = plain.clone();
        let stop = plain.len() - body.len() - 1;
        unskippable.splice(stop..stop, unskippable_fields(0xaa));
        for (page, presented) in [(plain, false), (unskippable, true)] {
            let mut bytes = container(0, 2, 2, 0, &[page]);
            // A second row group, a copy of the first, so its chunk names the
            // same page; the file then declares four rows.
            let position = row_groups(&bytes);
            let trailer = bytes.len() - 8;
            let group = bytes[position + 2..trailer - 1].to_vec();
            bytes[position - 1] = 8; // num_rows: 4
            bytes[position + 1] = 0x2c; // two row groups
            splice_footer(&mut bytes, trailer - 1..trailer - 1, &group);
            let decoded = read(&bytes);
            if presented {
                assert_eq!(decoded.unwrap_err(), Error::Malformed);
            } else {
                // With nothing to present, the shared page is read as held.
                let decoded = decoded.unwrap();
                assert_eq!(
                    strings(&decoded),
                    [Some("a"), Some("ab"), Some("a"), Some("ab")]
                );
            }
        }
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

    /// The installed hosts' `parquet-containment` truth: a file whose INT96
    /// column chunk has a 13-byte `min_value` statistic, which the metadata
    /// decoder asserts holds 12 bytes, and a readable file holding the INT64
    /// values 1 and 2.
    fn host_containment() -> (Vec<u8>, Vec<u8>) {
        let truth =
            crate::json_wire::decode(include_str!("../../../../tests/host-fixtures/cases.json"))
                .unwrap();
        let family = &truth["families"]["parquet-containment"];
        let bytes = |name: &str| -> Vec<u8> {
            let hex = family[name]["hex"].as_str().unwrap();
            (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect()
        };
        (bytes("contained_panic_parquet"), bytes("readable_parquet"))
    }

    fn values(decoded: &Decoded) -> &[i64] {
        let column = decoded.batches[0].column(0);
        column
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values()
    }

    #[test]
    fn the_host_containment_input_reaches_a_contained_decoder_panic() {
        let (contained, readable) = host_containment();
        // Admission passes it to the native decoder, which panics, so the
        // host suites that read it exercise a contained panic.
        let frame = framing::admit(&contained).unwrap();
        let native = compact::native_metadata(frame.metadata).unwrap();
        assert!(contain_decode(|| ParquetMetaDataReader::decode_metadata(&native)).is_none());
        for _ in 0..2 {
            assert_eq!(read(&contained).unwrap_err(), Error::Malformed);
            assert_eq!(values(&read(&readable).unwrap()), [1, 2]);
        }
    }

    /// Selects what `contained_panics_print_nothing_and_others_print_as_before`
    /// does when it runs as a child process.
    const PANIC_CHILD: &str = "YAMAA_CONTAINED_PANIC_CHILD";

    /// A panic outside `contain_decode`, caught so the test goes on.
    fn uncontained_panic() {
        assert!(std::panic::catch_unwind(|| panic!("a panic outside contain_decode")).is_err());
    }

    /// Panics when it is dropped.
    struct PanicOnDrop;

    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("a destructor panicked while a contained panic unwound");
        }
    }

    /// Hook output without the thread id that follows the thread's name,
    /// which differs between processes.
    fn without_thread_id(text: &str) -> String {
        let Some(start) = text.find("' (") else {
            return text.to_owned();
        };
        let rest = &text[start + 3..];
        match rest.find(')') {
            Some(end) if rest[..end].bytes().all(|byte| byte.is_ascii_digit()) => {
                format!("{}'{}", &text[..start], &rest[end + 1..])
            }
            _ => text.to_owned(),
        }
    }

    #[test]
    fn contained_panics_print_nothing_and_others_print_as_before() {
        match std::env::var(PANIC_CHILD).as_deref() {
            Ok("contained") => {
                let (contained, readable) = host_containment();
                assert_eq!(read(&contained).unwrap_err(), Error::Malformed);
                assert_eq!(values(&read(&readable).unwrap()), [1, 2]);
                return;
            }
            Ok("uncontained") => {
                uncontained_panic();
                return;
            }
            Ok("after-contained") => {
                assert!(contain_decode::<()>(|| panic!("a contained panic")).is_none());
                uncontained_panic();
                return;
            }
            // The runtime aborts the process when a destructor panics while
            // another panic unwinds.
            Ok("abort") => {
                contain_decode::<()>(|| {
                    let _armed = PanicOnDrop;
                    panic!("a contained panic");
                });
                unreachable!("the process aborted");
            }
            _ => {}
        }
        // Each child runs this test alone. Its output is not captured, so the
        // panic hook writes to the child's stderr, as it does in a host.
        let test = concat!(
            module_path!(),
            "::contained_panics_print_nothing_and_others_print_as_before"
        );
        let child = |role: &str| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test.split_once("::").unwrap().1])
                .args(["--no-capture", "--test-threads=1"])
                .env(PANIC_CHILD, role)
                .env_remove("RUST_BACKTRACE")
                .output()
                .unwrap()
        };
        let stderr = |role: &str| {
            let output = child(role);
            assert!(output.status.success(), "{role}: {output:?}");
            String::from_utf8(output.stderr).unwrap()
        };
        assert_eq!(stderr("contained"), "");
        let before = without_thread_id(&stderr("uncontained"));
        assert!(
            before.contains("a panic outside contain_decode"),
            "{before}"
        );
        assert_eq!(without_thread_id(&stderr("after-contained")), before);
        // A panic while the contained one unwinds reaches the hook the
        // wrapper wraps, so the abort keeps its message. The contained panic
        // stays silent.
        let aborted = child("abort");
        let message = String::from_utf8_lossy(&aborted.stderr);
        assert!(!aborted.status.success(), "{aborted:?}");
        assert!(
            message.contains("a destructor panicked while a contained panic unwound"),
            "{message}"
        );
        assert!(!message.contains("a contained panic\n"), "{message}");
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
