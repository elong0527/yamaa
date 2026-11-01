//! Complete held-container read, followed by closed-profile admission.
//! No filesystem reads, semantic fallback, publication or host date conversion.
use super::columns::{self, Column};
use super::footer::Footer;
use super::{
    compact, compression, conversion, footer, framing, metadata, payload, profile, schema,
};
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
/// A partition's table once its checks up to the table's admission have
/// passed: each column of `schema`, in order, of `rows` records. A column
/// may end the list early, as the stored columns did (`Runs::ordinary`).
#[derive(Debug, PartialEq)]
pub(super) struct Decoded {
    pub schema: TableSchema,
    pub columns: Vec<Converted>,
    pub rows: usize,
}

/// One column of a [`Decoded`] table: the engine's column, or a temporal
/// column's stored counts, which become civil values when the table builds
/// (`table::build`), as the whole read converts them last.
#[derive(Debug, PartialEq)]
pub(super) enum Converted {
    Column(crate::column_table::Column),
    /// Days from 1970-01-01.
    Days(columns::Stored<i32>),
    /// Microseconds from 1970-01-01T00:00:00, each a whole second.
    Micros(columns::Stored<i64>),
}

impl Converted {
    fn len(&self) -> usize {
        match self {
            Self::Column(column) => column.len(),
            Self::Days(days) => days.len(),
            Self::Micros(micros) => micros.len(),
        }
    }
}

#[cfg(test)]
impl Decoded {
    /// Column `index`'s payloads: an integer column's, or a datetime's
    /// microseconds.
    pub(super) fn integers(&self, index: usize) -> &[i64] {
        match &self.columns[index] {
            Converted::Column(crate::column_table::Column::Int(values))
            | Converted::Micros(values) => values.payloads(),
            other => panic!("column {index} holds no integers: {other:?}"),
        }
    }
}

/// What the reader decodes a file's footer, Arrow schema and stored columns
/// with: the native decoders ([`NATIVE`]), and in the differential harness
/// parquet 55's, which the reader read with until #3878.
#[derive(Clone, Copy)]
pub(super) struct Decoders {
    /// The footer that framing admitted.
    pub footer: fn(&framing::Framing<'_>) -> Result<Footer, Error>,
    /// The footer's Arrow schema, with the file's embedded schema as hints.
    pub arrow: fn(
        &framing::Framing<'_>,
        &Footer,
        Option<&metadata::Hint>,
    ) -> Result<arrow_schema::Schema, Error>,
    /// The stored columns of a run of row groups (`columns::read`).
    pub columns: ReadColumns,
}

/// Read the kept leaves of row groups `groups` of the held file, from its
/// admitted pages and any bodies their admission kept, to their stored
/// columns.
pub(super) type ReadColumns = fn(
    &[u8],
    &[framing::Page],
    Option<&columns::Bodies>,
    &Footer,
    &[bool],
    &arrow_schema::Schema,
    (std::ops::Range<usize>, Option<std::ops::Range<usize>>),
) -> Result<Vec<Column>, Error>;

/// The native decoders: the footer (`footer`), its Arrow schema
/// (`conversion`) and the column reader (`columns`).
pub(super) const NATIVE: Decoders = Decoders {
    footer: |frame| Ok(footer::decode(frame.metadata)?),
    arrow: native_schema,
    columns: columns::read,
};

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

/// The counts of one second of the table's datetimes, which an Arrow
/// timestamp of microseconds holds (`TimestampMicrosecondArray`).
const MICROS: i64 = super::format::TimeUnit::Micros.per_second();

/// A held file read by partition (#3792): what its footer and pages admit,
/// and what decodes one partition's row groups at a time, each partition
/// once for its checks and again whenever a run reads it (#3830). Its
/// checks read it alone, so the threads of a scan share it, each checking
/// partitions of its own (#4220).
pub(super) struct Runs {
    decoders: Decoders,
    /// The held file, and its pages as framing admitted them.
    file: Vec<u8>,
    pages: Vec<framing::Page>,
    footer: Footer,
    /// The row groups of each partition, as framing divided them.
    groups: Vec<std::ops::Range<usize>>,
    /// What the whole read admits once every page is, in its order, or the
    /// first of those findings.
    layout: Result<Layout, Error>,
}

/// What a file's partitions decode by, once its pages are admitted.
struct Layout {
    keep: Vec<bool>,
    arrow_schema: arrow_schema::Schema,
    /// The ODM fields' stored columns, or the profile's table: what each
    /// partition's stored columns become.
    shape: Result<Shape, Error>,
    /// Whether the row groups hold the records the file declares.
    groups_hold_rows: bool,
    /// The records of each partition.
    records: Vec<usize>,
}

enum Shape {
    Odm([usize; 11]),
    Ordinary(TableSchema),
}

pub(super) fn read(bytes: &[u8]) -> Result<Decoded, Error> {
    whole(bytes, false, NATIVE)
}

pub(super) fn read_odm(bytes: &[u8]) -> Result<Decoded, Error> {
    whole(bytes, true, NATIVE)
}

/// The whole read with `decoders`: the file as one partition, its pages
/// admitted, then what the whole file admits, then its stored columns.
pub(super) fn whole(bytes: &[u8], odm: bool, decoders: Decoders) -> Result<Decoded, Error> {
    let runs = Runs::open(bytes, odm, None, decoders)?;
    let bodies = runs.admit(0)?;
    runs.layout()?;
    let raw = runs.raw(0, &bodies)?;
    drop(bodies);
    runs.deferred()?;
    runs.decoded(raw, 0, 0).map_err(|(_, error)| error)
}

/// The table schema the held file decodes to with `decoders`, from its
/// footer alone (#3792): the fields [`read`], or with `odm` [`read_odm`],
/// gives the table when it finds nothing. No page is decoded, and each row
/// group, not the whole file, is admitted within the row and cell limits.
pub(super) fn table_schema(
    bytes: &[u8],
    odm: bool,
    decoders: Decoders,
) -> Result<TableSchema, Error> {
    let frame = framing::admit_partitioned(bytes, Some(std::num::NonZeroUsize::MIN))?;
    let footer = (decoders.footer)(&frame)?;
    let hint = metadata::admit(&footer.key_values)?;
    if odm {
        odm_fields(&frame, &footer, hint.as_ref(), decoders)?;
        return odm_schema();
    }
    let arrow = (decoders.arrow)(&frame, &footer, hint.as_ref())?;
    let arrow_schema = schema::normalize(&arrow, hint.as_ref().map(|hint| &hint.schema));
    let later = hint.as_ref().map_or(&[][..], |hint| &hint.later);
    profile::columns(&arrow_schema, &footer.schema, &frame.annotated, later).map_err(Error::Profile)
}

/// Each stored column's schema facts, from the admitted footer alone: no page
/// is decoded and no profile is applied.
pub(super) fn signature(bytes: &[u8]) -> Result<Vec<super::ColumnSignature>, Error> {
    let frame = framing::admit(bytes)?;
    let footer = footer::decode(frame.metadata)?;
    Ok(footer
        .schema
        .leaves
        .iter()
        .map(|leaf| super::ColumnSignature {
            path: leaf.path.join("."),
            physical_type: leaf.physical.name().into(),
            logical_type: leaf.logical.map(|logical| logical.debug()),
            converted_type: leaf.converted.name().into(),
            precision: leaf.precision,
            scale: leaf.scale,
            max_definition_level: leaf.max_definition,
            max_repetition_level: leaf.max_repetition,
        })
        .collect())
}

/// Reconcile page counts with their held chunk and row-group metadata before
/// any codec interprets a claimed value count as an allocation size. Repeated
/// columns may contain more values than rows. The column reader may still
/// retain storage for every admitted repeated value.
fn admit_page_counts(frame: &framing::Framing<'_>, footer: &Footer) -> Result<(), Error> {
    let leaves = &footer.schema.leaves;
    let mut pages = frame.pages.iter().peekable();
    for (row_group, group) in footer.row_groups.iter().enumerate() {
        let rows = usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?;
        for (column, chunk) in group.columns.iter().enumerate() {
            let expected = usize::try_from(chunk.num_values).map_err(|_| Error::Malformed)?;
            let leaf = leaves.get(column).ok_or(Error::Malformed)?;
            if leaf.max_repetition == 0 && expected != rows {
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

/// Refuse a file whose pages share a byte, once one of its page headers would
/// be presented otherwise: a header holding a value parquet 55's page reader
/// could not skip (`compact::native_metadata`), or a skipped binary that is
/// not UTF-8 (`framing::Framing::opaque`). parquet 55 read such a header
/// rewritten in place in a copy of the held file, and the reader refused the
/// file if two pages then shared a byte, since parquet 55 could read header
/// or body bytes that admission never saw. The native reader reads the held
/// bytes, and keeps the refusal at the same point of the read, so the set of
/// admitted files does not change (D283, #3878). Written files never overlap
/// pages, and a file with nothing to present reads as held.
fn refuse_shared_presented_pages(bytes: &[u8], frame: &framing::Framing<'_>) -> Result<(), Error> {
    let mut presented = !frame.opaque.is_empty();
    for page in &frame.pages {
        let shown = compact::native_metadata(&bytes[page.header.clone()])?;
        presented |= matches!(shown, std::borrow::Cow::Owned(_));
    }
    if !presented {
        return Ok(());
    }
    let mut extents: Vec<_> = frame
        .pages
        .iter()
        .map(|page| (page.header.start, page.body.end))
        .collect();
    extents.sort_unstable();
    if extents.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err(Error::Malformed);
    }
    Ok(())
}

/// The Arrow schema of the admitted footer `footer`, with the file's embedded
/// schema `hint` (`conversion`). A hint that nests deeper than the
/// FlatBuffers verifier's default depth gives none (`metadata::Hint`).
fn native_schema(
    _: &framing::Framing<'_>,
    footer: &Footer,
    hint: Option<&metadata::Hint>,
) -> Result<arrow_schema::Schema, Error> {
    if hint.is_some_and(|hint| !hint.shallow) {
        return Err(Error::Malformed);
    }
    conversion::schema(&footer.schema, hint.map(|hint| &hint.schema))
}

/// The leaf columns that store the eleven ODM fields of the admitted file,
/// in their canonical order (ODM-1, NAME-8). Metadata alone decides them.
fn odm_fields(
    frame: &framing::Framing<'_>,
    footer: &Footer,
    hint: Option<&metadata::Hint>,
    decoders: Decoders,
) -> Result<[usize; 11], Error> {
    let leaves = &footer.schema.leaves;
    let arrow = (decoders.arrow)(frame, footer, hint)?;
    let arrow_schema = schema::normalize(&arrow, hint.map(|hint| &hint.schema));
    let fields = arrow_schema
        .fields()
        .iter()
        .map(|field| yamaa_core::odm_source::StoredField {
            name: field.name(),
            kind: leaves
                .iter()
                .enumerate()
                .find(|(_, leaf)| leaf.max_repetition == 0 && leaf.path.join(".") == *field.name())
                .and_then(|(index, leaf)| {
                    profile::column_type(leaf, field.data_type(), frame.annotated.contains(&index))
                }),
        })
        .collect::<Vec<_>>();
    let binding = yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)?;
    Ok(binding.stored().map(|root| {
        leaves
            .iter()
            .position(|leaf| leaf.path.join(".") == *arrow_schema.field(root).name())
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

/// What the whole read checks of the held file once every page is
/// admitted, in its order: the declared records, the Arrow schema, each
/// partition's records within the row and cell limits (the whole file's,
/// without `rows`), the columns the table keeps, and the pages no presented
/// header shares. The profile's findings are held in the layout's shape,
/// as they wait until every physical chunk is decoded.
fn layout(
    bytes: &[u8],
    frame: &framing::Framing<'_>,
    footer: &Footer,
    hint: Option<&metadata::Hint>,
    odm_stored: Option<[usize; 11]>,
    rows: Option<std::num::NonZeroUsize>,
    decoders: Decoders,
) -> Result<Layout, Error> {
    let leaves = &footer.schema.leaves;
    let annotated = |column: usize| frame.annotated.contains(&column);
    let row_count = usize::try_from(footer.num_rows).map_err(|_| Error::Malformed)?;
    let arrow = (decoders.arrow)(frame, footer, hint)?;
    // A whole read admits the rows the file declares; a read by partition
    // admits each partition's rows, which its row groups hold.
    let records = frame
        .partitions
        .iter()
        .map(|groups| {
            let row_groups = footer
                .row_groups
                .get(groups.clone())
                .ok_or(Error::Malformed)?;
            row_groups.iter().try_fold(0usize, |sum, group| {
                sum.checked_add(usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?)
                    .ok_or(Error::Malformed)
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    match rows {
        None => yamaa_core::limits::Limits::table(row_count, leaves.len())?,
        Some(_) => {
            for &records in &records {
                yamaa_core::limits::Limits::table(records, leaves.len())?;
            }
        }
    }
    let arrow_schema = schema::normalize(&arrow, hint.map(|hint| &hint.schema));
    // Determine retention only. Profile errors remain deferred until every
    // physical chunk has been decoded, including fields outside the profile.
    let keep: Vec<_> = leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| {
            if let Some(stored) = &odm_stored {
                return stored.contains(&index);
            }
            arrow_schema.fields().len() == leaves.len()
                && arrow_schema.fields().get(index).is_some_and(|field| {
                    leaf.max_repetition == 0
                        && leaf.path.join(".") == *field.name()
                        && profile::column_type(leaf, field.data_type(), annotated(index)).is_some()
                })
        })
        .collect();
    refuse_shared_presented_pages(bytes, frame)?;
    let group_rows = footer.row_groups.iter().try_fold(0usize, |sum, group| {
        sum.checked_add(usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?)
            .ok_or(Error::Malformed)
    })?;
    let shape = match odm_stored {
        Some(stored) => Ok(Shape::Odm(stored)),
        None => {
            let later = hint.map_or(&[][..], |hint| &hint.later);
            profile::columns(&arrow_schema, &footer.schema, &frame.annotated, later)
                .map(Shape::Ordinary)
                .map_err(Error::Profile)
        }
    };
    Ok(Layout {
        keep,
        arrow_schema,
        shape,
        groups_hold_rows: group_rows == row_count,
        records,
    })
}

impl Runs {
    /// Admit the held file for a read by partition (#3792), with `decoders`:
    /// its row groups divided into partitions of at most `rows` records
    /// each, or, without `rows`, one partition holding the whole file. What
    /// the whole read checks before any page is checked here; each
    /// partition's pages are checked by [`Self::admit`], and what the whole
    /// read checks after the pages is held for [`Self::layout`], since a
    /// page's finding comes first. Each partition, not the whole file, is
    /// admitted within the row and cell limits. The checks the whole read
    /// makes after every partition's columns are decoded wait for
    /// [`Self::deferred`].
    pub(super) fn open(
        bytes: &[u8],
        odm: bool,
        rows: Option<std::num::NonZeroUsize>,
        decoders: Decoders,
    ) -> Result<Self, Error> {
        let frame = framing::admit_partitioned(bytes, rows)?;
        let footer = (decoders.footer)(&frame)?;
        let hint = metadata::admit(&footer.key_values)?;
        // ODM field admission uses metadata alone. Ordinary profile findings
        // keep their existing precedence after all physical chunks have been
        // decoded.
        let odm_stored = if odm {
            Some(odm_fields(&frame, &footer, hint.as_ref(), decoders)?)
        } else {
            None
        };
        admit_page_counts(&frame, &footer)?;
        let layout = layout(
            bytes,
            &frame,
            &footer,
            hint.as_ref(),
            odm_stored,
            rows,
            decoders,
        );
        // The pages of every row group are checked, the last partition's
        // reaching the footer's last row group.
        let mut groups = frame.partitions.clone();
        if let Some(last) = groups.last_mut() {
            last.end = last.end.max(footer.row_groups.len());
        }
        Ok(Self {
            decoders,
            file: bytes.to_vec(),
            pages: frame.pages,
            footer,
            groups,
            layout,
        })
    }

    /// The number of partitions, at least one.
    pub(super) fn len(&self) -> usize {
        self.groups.len()
    }

    /// Check partition `partition`'s pages, as the whole read checks every
    /// page before anything later, and keep their decoded bodies for the
    /// column reader. The scan takes the partitions' checks in order, so the
    /// first page that does not admit is the whole read's finding.
    pub(super) fn admit(&self, partition: usize) -> Result<columns::Bodies, Error> {
        let leaves = &self.footer.schema.leaves;
        let count = self.footer.row_groups.len();
        let groups = &self.groups[partition];
        let groups = groups.start.min(count)..groups.end.min(count);
        let first = self
            .pages
            .partition_point(|page| page.row_group < groups.start);
        let mut bodies = Vec::new();
        let mut payloads = payload::Validation::new();
        let mut pages = self.pages[first..].iter().peekable();
        for row_group in groups {
            let group = &self.footer.row_groups[row_group];
            let expected = usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?;
            for column in 0..group.columns.len() {
                let leaf = leaves.get(column).ok_or(Error::Malformed)?;
                let mut rows = payload::Rows::new(expected);
                while pages
                    .peek()
                    .is_some_and(|page| page.row_group == row_group && page.column == column)
                {
                    let page = pages.next().unwrap();
                    let decoded = compression::decode(page, &self.file)?;
                    rows.admit(page, &decoded, leaf)?;
                    payloads.admit(page, &decoded, leaf)?;
                    bodies.push(decoded);
                }
                rows.finish()?;
            }
        }
        Ok(columns::Bodies::new(first, bodies))
    }

    /// What the whole read checks once every page is admitted: the Arrow
    /// schema, the row and cell limits, and the pages no presented header
    /// shares, or the first of their findings.
    pub(super) fn layout(&self) -> Result<(), Error> {
        self.layout.as_ref().map(|_| ()).map_err(Error::clone)
    }

    fn admitted(&self) -> &Layout {
        self.layout
            .as_ref()
            .expect("a layout the read admitted (`Runs::layout`)")
    }

    /// The records of partition `partition`.
    pub(super) fn records(&self, partition: usize) -> usize {
        self.admitted().records[partition]
    }

    /// Partition `partition`'s stored columns, decoded from its row groups,
    /// reading the pages' `bodies` its admission kept. The whole read
    /// decodes every partition's before any later check, so the first
    /// partition that does not decode is its finding.
    pub(super) fn raw(
        &self,
        partition: usize,
        bodies: &columns::Bodies,
    ) -> Result<Vec<Column>, Error> {
        self.groups((self.groups[partition].clone(), None), Some(bodies))
    }

    /// The stored columns of row groups `groups`, only the records `window`
    /// keeps of them when there is one.
    fn groups(
        &self,
        groups: (std::ops::Range<usize>, Option<std::ops::Range<usize>>),
        bodies: Option<&columns::Bodies>,
    ) -> Result<Vec<Column>, Error> {
        let layout = self.admitted();
        (self.decoders.columns)(
            &self.file,
            &self.pages,
            bodies,
            &self.footer,
            &layout.keep,
            &layout.arrow_schema,
            groups,
        )
    }

    /// The checks of the whole file the whole read makes once every
    /// partition's stored columns are decoded: the row groups hold the
    /// records the file declares, and the profile admits its fields.
    pub(super) fn deferred(&self) -> Result<(), Error> {
        let layout = self.layout.as_ref().map_err(Error::clone)?;
        if !layout.groups_hold_rows {
            return Err(Error::Malformed);
        }
        match &layout.shape {
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
        raw: Vec<Column>,
        partition: usize,
        before: usize,
    ) -> Result<Decoded, (Position, Error)> {
        let records = self.records(partition);
        match &self.admitted().shape {
            Ok(Shape::Odm(stored)) => odm_decoded(raw, *stored, records),
            Ok(Shape::Ordinary(schema)) => self.ordinary(raw, schema, records, before),
            Err(error) => Err(((0, 0, 0), error.clone())),
        }
    }

    /// The table of records `range` of the file, read from the row groups
    /// that hold them, once the scan has admitted the file (#3830): only the
    /// range's records are kept, so no table of the whole row groups is
    /// built and copied from.
    pub(super) fn range(&self, range: std::ops::Range<usize>) -> Result<Decoded, Error> {
        let (mut start, mut groups, mut offset) = (0usize, None, 0);
        for (index, group) in self.footer.row_groups.iter().enumerate() {
            let rows = usize::try_from(group.num_rows).map_err(|_| Error::Malformed)?;
            let end = start.checked_add(rows).ok_or(Error::Malformed)?;
            if start < range.end && end > range.start {
                match &mut groups {
                    None => {
                        groups = Some(index..index + 1);
                        offset = range.start - start;
                    }
                    Some(groups) => groups.end = index + 1,
                }
            }
            start = end;
        }
        let (records, groups) = (range.len(), groups.unwrap_or(0..0));
        let window = offset..offset + records;
        let raw = self.groups((groups, Some(window)), None)?;
        let decoded = match &self.admitted().shape {
            Ok(Shape::Odm(stored)) => odm_decoded(raw, *stored, records),
            Ok(Shape::Ordinary(schema)) => self.ordinary(raw, schema, records, 0),
            Err(error) => return Err(error.clone()),
        }
        .map_err(|(_, error)| error)?;
        Ok(decoded)
    }

    /// An ordinary partition's table: its temporal values, then its columns
    /// one at a time, then the table as a whole, as the whole read checks
    /// its one partition.
    fn ordinary(
        &self,
        mut raw: Vec<Column>,
        schema: &TableSchema,
        records: usize,
        before: usize,
    ) -> Result<Decoded, (Position, Error)> {
        let leaves = &self.footer.schema.leaves;
        for (index, field) in schema.columns().iter().enumerate() {
            let at = |error| ((TEMPORAL, index, 0), Error::Profile(error));
            match (field.kind, raw.get_mut(index)) {
                (ColumnType::Date, Some(Column::Int32(values))) => {
                    for row in 0..values.len() {
                        if let Some(value) = values.get(row) {
                            let row = before + row + 1;
                            profile::temporal(field.kind, i64::from(value), 1, &field.name, row)
                                .map_err(at)?;
                        }
                    }
                }
                // A datetime counts its timestamp's unit. Each stored count
                // is checked as stored, then, a whole second in range,
                // counted again in the table's microseconds, which holds
                // that second exactly (PARQUET-9).
                (ColumnType::DateTime, Some(Column::Int64(values))) => {
                    let Some(second) = leaves.get(index).and_then(profile::per_second) else {
                        return Err(((TEMPORAL, index, 0), Error::Malformed));
                    };
                    let (mut counts, validity) = std::mem::take(values).into_parts();
                    for (row, value) in counts.iter_mut().enumerate() {
                        if validity.get(row) {
                            let row = before + row + 1;
                            profile::temporal(field.kind, *value, second, &field.name, row)
                                .map_err(at)?;
                            *value = *value / second * MICROS;
                        }
                    }
                    *values =
                        columns::Stored::from_parts(counts, validity).expect("a count per slot");
                }
                _ => {}
            }
        }
        // The reference validates text when constructing its typed table,
        // after field and temporal findings.
        let mut converted = Vec::with_capacity(schema.columns().len());
        let mut raw = raw.into_iter();
        for (index, field) in schema.columns().iter().enumerate() {
            // As the whole read's columns end with the shorter list.
            let Some(stored) = raw.next() else {
                break;
            };
            let arrow = self.admitted().arrow_schema.field(index).data_type();
            let declared = leaves
                .get(index)
                .and_then(|leaf| profile::width(leaf, arrow));
            let column = convert(field.kind, stored, declared)
                .map_err(|(step, error)| ((ARRAYS, index, step), error))?;
            if column.len() != records {
                return Err(((ARRAYS, index, 2), Error::Malformed));
            }
            converted.push(column);
        }
        // The reference's record batch of no columns, which holds no
        // record count, does not build.
        if converted.is_empty() {
            return Err(((BATCH, 0, 0), Error::Malformed));
        }
        Ok(Decoded {
            schema: schema.clone(),
            columns: converted,
            rows: records,
        })
    }
}

/// One column from one partition's stored values, or the step of its check
/// that failed, with the finding: text is validated (step 0) before its
/// offsets are checked (step 1), and the whole read validates every
/// partition's text before any partition's offsets. Missing and nonfinite
/// floats are missing alike; a temporal column keeps its counts.
fn convert(
    kind: ColumnType,
    stored: Column,
    declared: Option<u8>,
) -> Result<Converted, (u8, Error)> {
    use crate::column_table::Column as Engine;
    Ok(Converted::Column(match (kind, stored) {
        (ColumnType::Str, Column::Bytes(values)) => {
            let bytes = values.bytes();
            let text = values.text().ok_or((0, Error::Malformed))?;
            if !crate::column_table::fits_utf8_offsets([bytes]) {
                return Err((1, Error::Representation));
            }
            Engine::Str(text)
        }
        (ColumnType::Int, Column::Int64(values)) => Engine::Int(values),
        // A narrower signed annotation declares the width of every stored
        // value, as its column row's `width` says (`closed_profile`). The
        // format forbids writing a value outside it and leaves reading one
        // to each reader, so such a value fails rather than reads as one of
        // their answers. Every stored value fits 32 bits, with or without an
        // annotation that says so.
        (ColumnType::Int, Column::Int32(values)) => {
            if let Some(bits) = declared {
                let bound = 1i64 << (bits - 1);
                let outside = (0..values.len())
                    .filter_map(|row| values.get(row))
                    .any(|value| !(-bound..bound).contains(&i64::from(value)));
                if outside {
                    return Err((0, Error::Malformed));
                }
            }
            Engine::Int(widened(values, i64::from))
        }
        (ColumnType::Float, Column::Double(values)) => Engine::Float(finite(values)),
        // A single-precision value widens exactly to binary64, and a
        // non-finite one is missing, as a binary64 one is (PARQUET-8).
        (ColumnType::Float, Column::Float(values)) => {
            Engine::Float(finite(widened(values, f64::from)))
        }
        (ColumnType::Date, Column::Int32(values)) => return Ok(Converted::Days(values)),
        (ColumnType::DateTime, Column::Int64(values)) => return Ok(Converted::Micros(values)),
        _ => return Err((0, Error::Malformed)),
    }))
}

/// `values` with each payload converted by `widen`, which keeps a missing
/// slot's default payload the default.
fn widened<T: Copy + Default, U: Copy + Default>(
    values: columns::Stored<T>,
    widen: fn(T) -> U,
) -> columns::Stored<U> {
    let (payloads, validity) = values.into_parts();
    let payloads = payloads.into_iter().map(widen).collect();
    columns::Stored::from_parts(payloads, validity).expect("a payload per slot")
}

/// `values` with each nonfinite value missing.
fn finite(values: columns::Stored<f64>) -> columns::Stored<f64> {
    let finite = |value: f64| yamaa_core::value::FiniteFloat::new(value).is_some();
    if values.payloads().iter().copied().all(finite) {
        return values;
    }
    (0..values.len())
        .map(|row| values.get(row).filter(|&value| finite(value)))
        .collect()
}

/// One partition of an ODM read: its eleven fields' text, a field at a
/// time, each field's text validated (step 0), then its length (step 1),
/// then its offsets (step 2), as the whole read checks each field over
/// every partition before the next step.
fn odm_decoded(
    mut raw: Vec<Column>,
    stored: [usize; 11],
    records: usize,
) -> Result<Decoded, (Position, Error)> {
    use crate::column_table::Column as Engine;
    let schema = odm_schema().map_err(|error| ((ODM_TEXT, 0, 0), error))?;
    let mut converted: Vec<Converted> = Vec::with_capacity(stored.len());
    for (position, index) in stored.into_iter().enumerate() {
        let malformed = |step| ((ODM_TEXT, position, step), Error::Malformed);
        // A stored column two fields name is the same text for both.
        if let Some(earlier) = stored[..position].iter().position(|&at| at == index) {
            let Converted::Column(Engine::Str(text)) = &converted[earlier] else {
                unreachable!("an ODM field is text");
            };
            converted.push(Converted::Column(Engine::Str(text.clone())));
            continue;
        }
        let Some(Column::Bytes(values)) = raw.get_mut(index) else {
            return Err(malformed(0));
        };
        let values = std::mem::take(values);
        let bytes = values.bytes();
        let text = values.text().ok_or_else(|| malformed(0))?;
        if text.len() != records {
            return Err(malformed(1));
        }
        if !crate::column_table::fits_utf8_offsets([bytes]) {
            return Err(((ODM_TEXT, position, 2), Error::Representation));
        }
        converted.push(Converted::Column(Engine::Str(text)));
    }
    Ok(Decoded {
        schema,
        columns: converted,
        rows: records,
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
                assert_eq!(decoded.unwrap().rows, 2);
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
        let Converted::Column(crate::column_table::Column::Str(text)) = &decoded.columns[0] else {
            panic!("a text column")
        };
        (0..text.len()).map(|row| text.get(row)).collect()
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

    /// Two row groups whose chunks name the same page: refused once its header
    /// would be presented, as parquet 55's path refused it, and read as held
    /// otherwise (`refuse_shared_presented_pages`).
    #[test]
    fn a_file_whose_chunks_name_one_presented_page_is_malformed() {
        let mut body = progression(0, 1, 2);
        body.extend(progression(1, 0, 2));
        body.extend(b"ab"); // a, ab
        let plain = data_page(2, &body);
        // The same page, with unknown fields of every kind in its header.
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
                assert_eq!(
                    strings(&decoded.unwrap()),
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
        assert_eq!(
            strings(&decoded),
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
            assert_eq!(strings(&decoded), [Some("a"), None, Some("ab")]);
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

    /// A file of the installed hosts' `parquet-containment` truth: one whose
    /// INT96 column chunk has a 13-byte `min_value` statistic, where the
    /// type's statistics hold exactly 12; one whose two row groups name one
    /// page, whose header holds fields parquet 55 could not skip; and a
    /// readable file holding the INT64 values 1 and 2.
    fn host_containment(name: &str) -> Vec<u8> {
        let truth =
            crate::json_wire::decode(include_str!("../../../../tests/host-fixtures/cases.json"))
                .unwrap();
        let hex = truth["families"]["parquet-containment"][name]["hex"]
            .as_str()
            .unwrap();
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    }

    fn values(decoded: &Decoded) -> &[i64] {
        let Converted::Column(crate::column_table::Column::Int(values)) = &decoded.columns[0]
        else {
            panic!("an integer column")
        };
        values.payloads()
    }

    /// Admission passes the hosts' INT96 statistic to the footer, whose
    /// statistics rule refuses it, so the file is invalid however often it
    /// is read, and the readable file reads.
    #[test]
    fn the_host_fixture_int96_statistic_of_thirteen_bytes_is_invalid() {
        let long = host_containment("long_int96_statistic_parquet");
        let readable = host_containment("readable_parquet");
        let frame = framing::admit(&long).unwrap();
        assert_eq!(
            footer::decode(frame.metadata).unwrap_err(),
            compact::Error::Malformed
        );
        for _ in 0..2 {
            assert_eq!(read(&long).unwrap_err(), Error::Malformed);
            assert_eq!(values(&read(&readable).unwrap()), [1, 2]);
        }
    }

    /// The hosts' shared page: its footer decodes and its table schema reads,
    /// so a check binds it, and the read refuses it where the pages are
    /// compared (`refuse_shared_presented_pages`), however often it is read.
    #[test]
    fn the_host_fixture_shared_presented_page_is_invalid_once_read() {
        let shared = host_containment("shared_presented_page_parquet");
        let frame = framing::admit(&shared).unwrap();
        assert!(footer::decode(frame.metadata).is_ok());
        assert!(table_schema(&shared, false, NATIVE).is_ok());
        assert_eq!(
            refuse_shared_presented_pages(&shared, &frame).unwrap_err(),
            Error::Malformed
        );
        for _ in 0..2 {
            assert_eq!(read(&shared).unwrap_err(), Error::Malformed);
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
