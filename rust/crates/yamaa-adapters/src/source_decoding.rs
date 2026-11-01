//! Decode a specification's held sources into tables, whole or by partition,
//! for the decoder port, and the error a specification run reports: a
//! source refused, or a preparation, binding or execution failure.
use crate::{
    column_table::ColumnTable,
    csv_source::{self, TextTableError},
    dataset_result::DatasetError,
};
use yamaa_core::specification::{BindError, PrepareError, SourceDeclaration};
#[derive(Debug)]
pub enum Error {
    ResourceLimit(crate::resource_diagnostics::Refusal),
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    ParquetSource(crate::parquet_source::Error),
    SasSource(crate::sas_source::Error),
    /// The Dataset-JSON profiles' read of a `.json` or `.ndjson` input
    /// (DSJSON-1).
    DatasetJsonSource(crate::dataset_json_source::Error),
    Ordinal(yamaa_core::source_ordinal::Error),
    OdmFields(Vec<yamaa_core::odm_source::SchemaError>),
    OdmValues(Vec<yamaa_core::odm_source::MissingValues>),
    /// The ODM XML profile's read of an `.xml` input (ODM-1), or a build
    /// without its reader.
    OdmSource(crate::odm_xml::Error),
    ProducerContract(yamaa_core::producer_contract::Error),
    ProducerMetadata(yamaa_core::diagnostic::Diagnostic),
    Sources(Vec<(SourceDeclaration, Error)>),
    Bind(BindError),
    Execution(DatasetError),
}

/// A linked artifact's stored fields, in order and with their decoded types,
/// must equal the producing specification's output contract (INPUT-11). The
/// generated ordinal is appended only after this comparison.
fn stored_contract(
    source: &SourceDeclaration,
    contract: &yamaa_core::producer_contract::Contract,
    table: &ColumnTable,
) -> Result<(), Error> {
    use yamaa_core::table::TableAccess;
    let fields = table
        .schema()
        .columns()
        .iter()
        .map(|field| (field.name.as_str(), field.kind))
        .collect::<Vec<_>>();
    match contract
        .validate_typed_fields(&source.name, &fields)
        .map_err(Error::ProducerContract)?
    {
        Some(diagnostic) => Err(Error::ProducerMetadata(diagnostic)),
        None => Ok(()),
    }
}

/// A frame has already admitted physical types at the Arrow boundary. Preserve
/// those values, enforce explicit source declarations and reuse ODM/ordinal,
/// empty-string and producer-contract admission before ordinary binding and
/// execution.
pub(crate) fn decode_frame(
    source: &SourceDeclaration,
    table: ColumnTable,
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
) -> Result<ColumnTable, Error> {
    use yamaa_core::table::TableAccess;
    // A linked artifact's declared types are its producer's contract; compare
    // the whole stored field list before any single declared field.
    if let Some(contract) = contract {
        stored_contract(source, contract, &table)?;
    }
    yamaa_core::typed_csv::PreparedTypes::new(&source.types)
        .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
    for (name, kind) in &source.types {
        let field = table
            .schema()
            .columns()
            .iter()
            .find(|field| field.name == *name)
            .ok_or_else(|| {
                typed_source_error(crate::typed_csv::Error::Typing(
                    yamaa_core::typed_csv::Error::UnknownField {
                        field: name.clone(),
                    },
                ))
            })?;
        if field.kind != *kind {
            return Err(typed_source_error(crate::typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::InvalidSource,
            )));
        }
    }
    let table = if interpretation == yamaa_core::specification::SourceInterpretation::Odm {
        project_odm(table)?
    } else {
        table
    };
    crate::source_ordinal::append(source, empty_string_convention(source, table)?)
}

pub fn decode_source(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
) -> Result<ColumnTable, Error> {
    let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
    let table = match source.profile {
        yamaa_core::specification::SourceProfile::Parquet => decode_parquet(source, bytes, odm)?,
        yamaa_core::specification::SourceProfile::Xport
        | yamaa_core::specification::SourceProfile::Sas7bdat => {
            let table = crate::sas_source::parse(bytes, source).map_err(Error::SasSource)?;
            if odm {
                project_odm(table)?
            } else {
                table
            }
        }
        yamaa_core::specification::SourceProfile::DatasetJson
        | yamaa_core::specification::SourceProfile::DatasetNdjson => {
            // Each column's `dataType` types its field, which a `types`
            // entry may narrow (DSJSON-2); the profile applies the
            // empty-string convention as it reads each text (DSJSON-3).
            let table = crate::dataset_json_source::parse(bytes, source)
                .map_err(Error::DatasetJsonSource)?;
            if odm {
                project_odm(table)?
            } else {
                table
            }
        }
        yamaa_core::specification::SourceProfile::OdmXml => {
            // The fixed clinical-item table, whose fields the profile names
            // and types; an `odm` read binds its eleven fields (NAME-8).
            let table = crate::odm_xml::read_input(bytes).map_err(Error::OdmSource)?;
            let table = if odm { project_odm(table)? } else { table };
            empty_string_convention(source, table)?
        }
        yamaa_core::specification::SourceProfile::Csv => {
            // Admit declared types before scanning, retaining raw producer header
            // validation before any typed cell conversion.
            let types = yamaa_core::typed_csv::PreparedTypes::new(&source.types)
                .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
            // The input's declaration decides whether the source begins with
            // a byte-order mark (CSV-10).
            let parsed = csv_source::parse_declared(bytes, source.byte_order_mark)
                .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
            if odm {
                let binding = odm_header(&parsed.names)?;
                odm_records(parsed, &binding)?
            } else {
                producer_header(source, contract, &parsed.names)?;
                crate::typed_csv::convert(parsed, types).map_err(typed_source_error)?
            }
        }
    };
    if let Some(contract) = contract {
        stored_contract(source, contract, &table)?;
    }
    crate::source_ordinal::append(source, table)
}

/// The ODM fields a CSV header stores (NAME-8), all read as text.
fn odm_header(names: &[String]) -> Result<yamaa_core::odm_source::FieldBinding, Error> {
    let fields = names
        .iter()
        .map(|name| yamaa_core::odm_source::StoredField {
            name,
            kind: Some(yamaa_core::value::ColumnType::Str),
        })
        .collect::<Vec<_>>();
    yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)
}

/// CSV records as the fixed ODM table: the eleven fields `binding` locates,
/// in their canonical order, as text.
fn odm_records(
    parsed: csv_source::CsvSource,
    binding: &yamaa_core::odm_source::FieldBinding,
) -> Result<ColumnTable, Error> {
    let projected = csv_source::CsvSource {
        names: yamaa_core::odm_source::FIELDS
            .iter()
            .map(|name| (*name).into())
            .collect(),
        records: parsed
            .records
            .into_iter()
            .map(|mut row| {
                binding
                    .stored()
                    .iter()
                    .map(|&column| row[column].take())
                    .collect()
            })
            .collect(),
    };
    crate::typed_csv::convert(
        projected,
        yamaa_core::typed_csv::PreparedTypes::new(&[]).expect("fixed text declarations"),
    )
    .map_err(typed_source_error)
}

/// A linked CSV artifact's raw header against its producer's output contract
/// (INPUT-11), before any typed cell conversion.
fn producer_header(
    source: &SourceDeclaration,
    contract: Option<&yamaa_core::producer_contract::Contract>,
    names: &[String],
) -> Result<(), Error> {
    let Some(contract) = contract else {
        return Ok(());
    };
    let header = names.iter().map(String::as_str).collect::<Vec<_>>();
    match contract
        .validate_header(&source.name, &header)
        .map_err(Error::ProducerContract)?
    {
        Some(diagnostic) => Err(Error::ProducerMetadata(diagnostic)),
        None => Ok(()),
    }
}

/// An input read by partition (#3792): what its scan admitted, holding no
/// record, from which any range of its records decodes again (#3830).
pub struct Scanned {
    /// The schema of the table the input decodes to.
    pub schema: yamaa_core::table::TableSchema,
    /// The input's records.
    pub records: usize,
    /// The record ranges admitted separately by the scan. Decoding a report
    /// must not combine them into a table with larger text offsets.
    pub partitions: Vec<std::ops::Range<usize>>,
    source: SourceDeclaration,
    reader: Reader,
}

/// The partitions of a Parquet input its scan decoded and kept for the
/// run's partitions (#4212): the first ones, while together they hold no
/// more records than one partition of the run and stay within the row and
/// cell limits (LIMIT-3). The run holds a partition decoded anyway, so
/// keeping one costs no more than the read by partition holds (#3830), and
/// the run decodes those records once. Each table is the one the scan
/// admitted and visited, and is dropped once every one of its records has
/// been taken; any record no table holds decodes again from the held bytes.
#[derive(Default)]
pub struct Kept {
    tables: std::sync::Mutex<Vec<KeptTable>>,
}

/// One kept partition: its first record among the input's, its table, and
/// how many of its records the run has taken.
struct KeptTable {
    start: usize,
    table: ColumnTable,
    taken: usize,
}

impl Kept {
    /// Keep `table`, whose first record is `start`, when it is the input's
    /// first or follows the tables kept so far, and they stay within `fits`
    /// together with it.
    fn offer(&mut self, start: usize, table: &ColumnTable, fits: impl Fn(usize) -> bool) {
        use yamaa_core::table::TableAccess;
        let tables = self
            .tables
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner());
        let records = tables
            .iter()
            .map(|kept| kept.table.row_count())
            .sum::<usize>();
        // Only the first partitions are kept, each following the last.
        let follows = tables.last().map_or(start == 0, |kept| {
            kept.start + kept.table.row_count() == start
        });
        let rows = table.row_count();
        if rows > 0 && follows && fits(records + rows) {
            tables.push(KeptTable {
                start,
                table: table.clone(),
                taken: 0,
            });
        }
    }

    /// Take the kept records `range` holds: each kept table it overlaps,
    /// in order, with its first record and the rows of it in `range`. A
    /// table all of whose records have been taken is no longer kept.
    fn take(&self, range: &std::ops::Range<usize>) -> Vec<Taken> {
        use yamaa_core::table::TableAccess;
        let mut tables = self
            .tables
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut taken = Vec::new();
        tables.retain_mut(|kept| {
            let end = kept.start + kept.table.row_count();
            let (from, to) = (range.start.max(kept.start), range.end.min(end));
            if from >= to {
                return true;
            }
            kept.taken += to - from;
            taken.push((
                kept.start,
                kept.table.clone(),
                from - kept.start..to - kept.start,
            ));
            kept.taken < kept.table.row_count()
        });
        taken
    }
}

/// A kept table's records a range takes: the table's first record among
/// the input's, the table, and the rows of it the range holds.
type Taken = (usize, ColumnTable, std::ops::Range<usize>);

/// What decodes a range of a scanned input's records.
enum Reader {
    Csv {
        offsets: csv_source::Offsets,
        odm: Option<yamaa_core::odm_source::FieldBinding>,
    },
    Parquet(crate::parquet_source::Scanned),
}

/// The first of the findings a read by partition makes once its profile has
/// decoded every partition: the ordering stage the whole read checks first,
/// then the earliest partition (#3830).
struct First(Option<((u8, usize), Error)>);

impl First {
    /// A finding of `stage`, in partition `partition`.
    fn offer(&mut self, stage: u8, partition: usize, error: Error) {
        if self
            .0
            .as_ref()
            .is_none_or(|(at, _)| (stage, partition) < *at)
        {
            self.0 = Some(((stage, partition), error));
        }
    }
}

/// The checks the whole read makes once its profile has decoded the whole
/// table, in their order: the empty-string convention, the producer
/// contract over the stored fields, then the generated ordinal.
const CONVENTION: u8 = 0;
const CONTRACT: u8 = 1;
const ORDINAL: u8 = 2;

/// Scan `source`'s held bytes by partition (#3792): a CSV source in
/// partitions of `rows` records, the last holding the rest, and a Parquet
/// source in runs of whole row groups of at most `rows` records, a larger
/// row group being a partition of its own. Each partition's table is decoded
/// once, given to `visit` on the calling thread, in order, with the position
/// of its first record among the input's records, and dropped (#3830). A
/// Parquet source's partitions decode up to `threads` at once, each on a
/// thread of its own (#4220); a CSV source's convert on the calling thread
/// in turn, since its records are not row groups. `None` for any other
/// profile, which is read whole.
///
/// Every finding [`decode_source`] reports is reported in the same order, at
/// the same record, row and field, and with the same context: the CSV scan
/// reads every record before any converts, records convert in order, and the
/// checks the whole read makes after converting wait until every partition
/// has converted. Only the row and cell limits differ, with the text-offset
/// bound of each column: each partition, not the whole table, is admitted
/// within them. A partition's generated ordinal is its records' positions in
/// the whole source.
pub(crate) fn scan_partitions(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
    contract: Option<&yamaa_core::producer_contract::Contract>,
    (rows, threads): (std::num::NonZeroUsize, std::num::NonZeroUsize),
    visit: &mut dyn FnMut(&ColumnTable, usize),
) -> Option<Result<(Scanned, Kept), Error>> {
    use yamaa_core::specification::SourceProfile;
    use yamaa_core::table::TableAccess;
    let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
    let mut first = First(None);
    let mut kept = Kept::default();
    let mut schema = None;
    let mut admitted = Vec::new();
    let mut partition = 0usize;
    // The checks the whole read makes once the table has decoded, each
    // partition's on its own, and its visit. A Parquet partition's table
    // comes with the empty-string convention applied, on the thread that
    // built it (#4220), or with the convention's finding.
    let mut finish = |converted: Result<ColumnTable, Error>, start: usize, first: &mut First| {
        let table = match converted {
            Ok(table) => table,
            Err(error) => {
                first.offer(CONVENTION, partition, error);
                partition += 1;
                return;
            }
        };
        if partition == 0 {
            if let Some(contract) = contract {
                if let Err(error) = stored_contract(source, contract, &table) {
                    first.offer(CONTRACT, partition, error);
                }
            }
        }
        match crate::source_ordinal::append_at(source, table, start) {
            Ok(table) => {
                admitted.push(start..start + table.row_count());
                if schema.is_none() {
                    schema = Some(table.schema().clone());
                }
                if first.0.is_none() {
                    visit(&table, start);
                    // The first Parquet partitions are kept for the run, up
                    // to one partition's records (#4212); CSV decodes again.
                    if source.profile == SourceProfile::Parquet {
                        let width = table.schema().columns().len();
                        kept.offer(start, &table, |records| {
                            records <= rows.get()
                                && yamaa_core::limits::Limits::table(records, width).is_ok()
                        });
                    }
                }
            }
            Err(error) => first.offer(ORDINAL, partition, error),
        }
        partition += 1;
    };
    let (reader, records) = match source.profile {
        SourceProfile::Csv => {
            let scanned = (|| {
                let types = yamaa_core::typed_csv::PreparedTypes::new(&source.types)
                    .map_err(|error| typed_source_error(crate::typed_csv::Error::Typing(error)))?;
                let partitions = csv_source::scan(bytes, source.byte_order_mark, rows)
                    .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
                let binding = if odm {
                    Some(odm_header(partitions.names())?)
                } else {
                    producer_header(source, contract, partitions.names())?;
                    None
                };
                let mut start = 0usize;
                for index in 0..partitions.len() {
                    let records = partitions
                        .records(index)
                        .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
                    // Records convert in order, so the first that does not
                    // convert is the whole read's.
                    let table = match &binding {
                        Some(binding) => odm_records(records, binding)?,
                        None => {
                            crate::typed_csv::convert(records, types).map_err(typed_source_error)?
                        }
                    };
                    let count = table.row_count();
                    finish(Ok(table), start, &mut first);
                    start += count;
                }
                Ok((
                    Reader::Csv {
                        offsets: partitions.offsets(),
                        odm: binding,
                    },
                    start,
                ))
            })();
            match scanned {
                Ok(scanned) => scanned,
                Err(error) => return Some(Err(error)),
            }
        }
        SourceProfile::Parquet => {
            let mut start = 0usize;
            let convention = |table| empty_string_convention(source, table);
            let mut visited = |converted: Result<ColumnTable, Error>, at: usize| {
                // A partition the convention refuses ends the scan, whose
                // records are then not needed.
                if let Ok(table) = &converted {
                    start = at + table.row_count();
                }
                finish(converted, at, &mut first);
            };
            let scanned = crate::parquet_source::scan(
                bytes,
                odm,
                (rows, threads),
                (&convention, &mut visited),
            );
            match scanned {
                Ok(scanned) => (Reader::Parquet(scanned), start),
                Err(error) => return Some(Err(parquet_error(error))),
            }
        }
        // Every other profile is read whole.
        SourceProfile::Xport
        | SourceProfile::Sas7bdat
        | SourceProfile::OdmXml
        | SourceProfile::DatasetJson
        | SourceProfile::DatasetNdjson => return None,
    };
    if let Some((_, error)) = first.0 {
        return Some(Err(error));
    }
    Some(Ok((
        Scanned {
            schema: schema.expect("a scan reads at least one partition"),
            records,
            partitions: admitted,
            source: source.clone(),
            reader,
        },
        kept,
    )))
}

/// Decode records `range` of an input [`scan_partitions`] scanned for a
/// run's partition (#4212): the records `kept` holds are taken from the
/// tables the scan kept and the rest decoded again from the held bytes, as
/// [`decode_range`] decodes them, joined in order. When the joined records
/// do not admit as one table, the range decodes again whole, so it gives
/// what [`decode_range`] gives, finding and all.
pub fn decode_partition(
    scanned: &Scanned,
    kept: &Kept,
    bytes: &[u8],
    range: std::ops::Range<usize>,
) -> Result<ColumnTable, Error> {
    let taken = kept.take(&range);
    if taken.is_empty() {
        return decode_range(scanned, bytes, range);
    }
    match joined(scanned, bytes, range.clone(), taken) {
        Some(table) => Ok(table),
        None => decode_range(scanned, bytes, range),
    }
}

/// Records `range` from the kept tables `taken` holds them in, and the rest
/// decoded from the held bytes, joined in order; `None` when a part does
/// not decode or the parts do not admit as one table.
fn joined(
    scanned: &Scanned,
    bytes: &[u8],
    range: std::ops::Range<usize>,
    taken: Vec<Taken>,
) -> Option<ColumnTable> {
    use yamaa_core::table::TableAccess;
    let mut parts = Vec::with_capacity(taken.len() + 2);
    let mut at = range.start;
    let decoded = |records: std::ops::Range<usize>| {
        let table = decode_range(scanned, bytes, records).ok()?;
        let rows = 0..table.row_count();
        Some((table, rows))
    };
    for (start, table, rows) in taken {
        if start + rows.start > at {
            parts.push(decoded(at..start + rows.start)?);
        }
        at = start + rows.end;
        parts.push((table, rows));
    }
    if at < range.end {
        parts.push(decoded(at..range.end)?);
    }
    // A kept table the range holds whole is the range's table.
    if let [(table, rows)] = &parts[..] {
        if *rows == (0..table.row_count()) {
            return Some(table.clone());
        }
    }
    ColumnTable::concat(parts).ok()
}

/// Decode records `range` of an input [`scan_partitions`] scanned, from the
/// same held bytes (#3830): its table as the whole read gives those
/// records, its generated ordinal their positions in the whole source.
pub fn decode_range(
    scanned: &Scanned,
    bytes: &[u8],
    range: std::ops::Range<usize>,
) -> Result<ColumnTable, Error> {
    let source = &scanned.source;
    let table = match &scanned.reader {
        Reader::Csv { offsets, odm } => {
            let records = offsets
                .records(bytes, source.byte_order_mark, range.clone())
                .map_err(|error| Error::Source(TextTableError::Csv(error)))?;
            match odm {
                Some(binding) => odm_records(records, binding)?,
                None => {
                    let types = yamaa_core::typed_csv::PreparedTypes::new(&source.types).map_err(
                        |error| typed_source_error(crate::typed_csv::Error::Typing(error)),
                    )?;
                    crate::typed_csv::convert(records, types).map_err(typed_source_error)?
                }
            }
        }
        Reader::Parquet(parquet) => empty_string_convention(
            source,
            crate::parquet_source::decode_range(parquet, range.clone()).map_err(parquet_error)?,
        )?,
    };
    crate::source_ordinal::append_at(source, table, range.start)
}

/// The schema `source`'s held bytes decode to, read from a CSV source's
/// header or a Parquet source's footer alone (#3792): the fields
/// [`decode_source`] gives the table when it finds nothing, the generated
/// ordinal included. `None` for any other profile, or a header or footer
/// that does not read, whose source is read whole.
pub(crate) fn source_schema(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: yamaa_core::specification::SourceInterpretation,
) -> Option<yamaa_core::table::TableSchema> {
    use yamaa_core::specification::SourceProfile;
    use yamaa_core::table::TableAccess;
    match source.profile {
        // The header alone decodes as the whole source does, to no records.
        SourceProfile::Csv => {
            let header = &bytes[..csv_source::header_end(bytes)];
            let table = decode_source(source, header, interpretation, None).ok()?;
            Some(table.schema().clone())
        }
        SourceProfile::Parquet => {
            let odm = interpretation == yamaa_core::specification::SourceInterpretation::Odm;
            let schema = crate::parquet_source::schema(bytes, odm)?;
            match yamaa_core::source_ordinal::prepare(source, &schema, 0) {
                Ok(Some(ordinal)) => Some(ordinal.schema().clone()),
                Ok(None) => Some(schema),
                Err(_) => None,
            }
        }
        SourceProfile::Xport
        | SourceProfile::Sas7bdat
        | SourceProfile::OdmXml
        | SourceProfile::DatasetJson
        | SourceProfile::DatasetNdjson => None,
    }
}

pub(crate) fn typed_source_error(error: crate::typed_csv::Error) -> Error {
    match error {
        crate::typed_csv::Error::Csv(error) => Error::Source(TextTableError::Csv(error)),
        crate::typed_csv::Error::Table(error) => Error::Source(TextTableError::Table(error)),
        error => Error::TypedSource(error),
    }
}

fn project_odm(table: ColumnTable) -> Result<ColumnTable, Error> {
    use yamaa_core::table::{Column, TableAccess, TableSchema};
    let fields = table
        .schema()
        .columns()
        .iter()
        .map(|column| yamaa_core::odm_source::StoredField {
            name: column.name.as_str(),
            kind: Some(column.kind),
        })
        .collect::<Vec<_>>();
    let binding = yamaa_core::odm_source::bind_fields(&fields).map_err(Error::OdmFields)?;
    let schema = TableSchema::new(
        yamaa_core::odm_source::FIELDS
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: yamaa_core::value::ColumnType::Str,
            })
            .collect(),
    )
    .map_err(|_| Error::SasSource(crate::sas_source::Error::Internal))?;
    table
        .project(schema, binding.stored())
        .map_err(|error| Error::Source(TextTableError::Table(error)))
}

fn decode_parquet(
    source: &SourceDeclaration,
    bytes: &[u8],
    odm: bool,
) -> Result<ColumnTable, Error> {
    use crate::parquet_source;
    let table = if odm {
        parquet_source::parse_odm(bytes)
    } else {
        parquet_source::parse(bytes)
    }
    .map_err(parquet_error)?;
    // All strings were admitted by the bounded codec.
    empty_string_convention(source, table)
}

fn parquet_error(error: crate::parquet_source::Error) -> Error {
    match error {
        crate::parquet_source::Error::OdmFields(fields) => Error::OdmFields(fields),
        error => Error::ParquetSource(error),
    }
}

/// Apply the dataset's empty-string convention after the profile decodes, so
/// equal logical input yields equal values whichever transport carried it.
/// Only a text column holding a convention-missing value is rebuilt, with
/// that value missing; every other column stays shared, and an unaffected
/// table is returned unchanged.
fn empty_string_convention(
    source: &SourceDeclaration,
    table: ColumnTable,
) -> Result<ColumnTable, Error> {
    use crate::column_table::Column;
    let replaced = table.replaced(|column| match column {
        Column::Str(text) => text
            .missing_where(|value| source.text_is_missing(value))
            .map(Column::Str),
        _ => None,
    });
    match replaced {
        None => Ok(table),
        Some(table) => table.map_err(|error| Error::Source(TextTableError::Table(error))),
    }
}

/// The partitions a scan keeps for the run (#4212): the seam is `Kept`,
/// whose tables no read by partition exposes.
#[cfg(test)]
mod kept_tests {
    use super::*;
    use yamaa_core::table::{Column as Field, TableAccess, TableSchema};

    /// A table of `ID` values `range`, the records `range` of an input.
    fn records(range: std::ops::Range<usize>) -> ColumnTable {
        let schema = TableSchema::new(vec![Field {
            name: "ID".into(),
            kind: yamaa_core::value::ColumnType::Int,
        }])
        .unwrap();
        let ids = range.clone().map(|id| Some(id as i64)).collect();
        ColumnTable::new(
            schema,
            vec![crate::column_table::Column::Int(ids)],
            range.len(),
        )
        .unwrap()
    }

    /// Partitions of 4, 4 and 2 records offered within 8 records: the
    /// first two are kept, the third is not, and neither is one that does
    /// not follow them; past a first partition that does not fit, none is
    /// kept. Each range then takes the kept records it holds,
    /// with each table's first record and its rows in the range, and a
    /// table is no longer kept once its every record is taken, so a range
    /// taken again finds none.
    #[test]
    fn a_scan_keeps_its_first_partitions_within_the_budget_until_the_run_takes_them() {
        let mut kept = Kept::default();
        for range in [0..4, 4..8, 8..10, 12..13] {
            kept.offer(range.start, &records(range), |records| records <= 8);
        }
        let taken = |kept: &Kept, range| {
            kept.take(&range)
                .into_iter()
                .map(|(start, table, rows)| (start, table.row_count(), rows))
                .collect::<Vec<_>>()
        };
        type Case = (
            std::ops::Range<usize>,
            Vec<(usize, usize, std::ops::Range<usize>)>,
        );
        let cases: [Case; 5] = [
            (0..3, vec![(0, 4, 0..3)]),
            (3..6, vec![(0, 4, 3..4), (4, 4, 0..2)]),
            (0..3, vec![]),
            (6..10, vec![(4, 4, 2..4)]),
            (4..8, vec![]),
        ];
        for (range, expected) in cases {
            assert_eq!(taken(&kept, range.clone()), expected, "records {range:?}");
        }
        assert!(
            kept.tables.lock().unwrap().is_empty(),
            "every kept table taken"
        );
        let mut late = Kept::default();
        for range in [0..9, 9..12] {
            late.offer(range.start, &records(range), |records| records <= 8);
        }
        assert!(
            late.tables.lock().unwrap().is_empty(),
            "no partition after one that does not fit"
        );
    }
}

/// The budget a scan keeps partitions within (#4212): the seam is
/// `scan_partitions`, whose kept tables no read by partition exposes.
#[cfg(all(test, feature = "parquet"))]
mod kept_scan_tests {
    use super::*;
    use arrow_array::{Int64Array, RecordBatch};
    use arrow_schema::{DataType, Field, Schema};
    use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
    use yamaa_core::specification::{SourceInterpretation, SourceProfile};
    use yamaa_core::table::TableAccess;

    fn declaration(profile: SourceProfile) -> SourceDeclaration {
        SourceDeclaration {
            name: "SRC".into(),
            path: match profile {
                SourceProfile::Parquet => "input.parquet".into(),
                _ => "input.csv".into(),
            },
            types: vec![("ID".into(), yamaa_core::value::ColumnType::Int)],
            profile,
            encoding: None,
            byte_order_mark: false,
            empty_string_present: false,
            ordinal: None,
        }
    }

    /// `ID` 1 to 15 in five row groups of three.
    fn fifteen() -> Vec<u8> {
        let schema =
            std::sync::Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![std::sync::Arc::new(Int64Array::from_iter_values(1..=15))],
        )
        .unwrap();
        let properties = WriterProperties::builder()
            .set_max_row_group_size(3)
            .build();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        bytes
    }

    /// The first record and the records of each table `bytes` keeps when
    /// scanned as `profile` in partitions of `rows` records, `threads` at
    /// once.
    fn kept(
        profile: SourceProfile,
        bytes: &[u8],
        (rows, threads): (usize, usize),
    ) -> Vec<(usize, usize)> {
        let count = |count| std::num::NonZeroUsize::new(count).unwrap();
        let declaration = declaration(profile);
        let scanned = scan_partitions(
            &declaration,
            bytes,
            SourceInterpretation::Ordinary,
            None,
            (count(rows), count(threads)),
            &mut |_, _| {},
        );
        let (_, kept) = scanned
            .expect("a scan by partition")
            .expect("an admitted input");
        let tables = kept.tables.into_inner().unwrap();
        tables
            .iter()
            .map(|kept| (kept.start, kept.table.row_count()))
            .collect()
    }

    /// A Parquet input's partitions take whole row groups of three while
    /// they fit in a partition's records, and the scan keeps the first
    /// partitions while together they hold no more than one partition's
    /// records: in partitions of 6 (6, 6, 3) the first; of 3 (five of 3)
    /// the first; of 9 (9, 6) the first; of 15 the one, whether the scan
    /// runs on one thread or on four (#4220). A CSV input's partitions are
    /// all decoded again, so its scan keeps none.
    #[test]
    fn a_scan_keeps_a_parquet_inputs_first_partitions_up_to_one_partitions_records() {
        let parquet = fifteen();
        let csv = (1..=15).fold(String::from("ID\n"), |text, id| format!("{text}{id}\n"));
        // A label, the input, its partitions' records, the kept tables.
        type Case<'a> = (&'a str, SourceProfile, &'a [u8], usize, Vec<(usize, usize)>);
        let cases: [Case; 5] = [
            (
                "Parquet in partitions of 6",
                SourceProfile::Parquet,
                &parquet,
                6,
                vec![(0, 6)],
            ),
            (
                "Parquet in partitions of 3",
                SourceProfile::Parquet,
                &parquet,
                3,
                vec![(0, 3)],
            ),
            (
                "Parquet in partitions of 9",
                SourceProfile::Parquet,
                &parquet,
                9,
                vec![(0, 9)],
            ),
            (
                "Parquet in one partition",
                SourceProfile::Parquet,
                &parquet,
                15,
                vec![(0, 15)],
            ),
            (
                "CSV in partitions of 6",
                SourceProfile::Csv,
                csv.as_bytes(),
                6,
                vec![],
            ),
        ];
        for (label, profile, bytes, rows, expected) in cases {
            for threads in [1, 4] {
                assert_eq!(
                    kept(profile, bytes, (rows, threads)),
                    expected,
                    "{label}, {threads} threads"
                );
            }
        }
    }
}

/// Host frames decode as their input declares: the empty-string convention
/// and the `odm` projection.
#[cfg(all(test, feature = "arrow"))]
mod frame_tests {
    use super::*;
    use arrow_array::{ArrayRef, LargeStringArray, RecordBatch, StringArray, StringViewArray};
    use arrow_schema::{Field, Schema};
    use std::sync::Arc;
    use yamaa_core::{
        specification::{SourceInterpretation, SourceProfile},
        table::{TableAccess, ValueRef},
        value::ColumnType,
    };

    fn source(empty_string_present: bool) -> SourceDeclaration {
        SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            byte_order_mark: false,
            empty_string_present,
            ordinal: None,
        }
    }

    /// A host frame of these named columns, through a C stream.
    fn frame(columns: Vec<(&str, ArrayRef)>) -> ColumnTable {
        let schema = Arc::new(Schema::new(
            columns
                .iter()
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let arrays = columns.into_iter().map(|(_, array)| array).collect();
        let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
        crate::arrow_bridge::import_public_stream(crate::arrow_bridge::export_stream(
            schema,
            vec![batch],
        ))
        .unwrap()
    }

    #[test]
    fn frames_apply_the_declared_convention_to_every_text_layout() {
        let values = vec![Some(""), Some("x"), None];
        for array in [
            Arc::new(StringArray::from(values.clone())) as ArrayRef,
            Arc::new(LargeStringArray::from(values.clone())),
            Arc::new(StringViewArray::from(values.clone())),
        ] {
            let kind = array.data_type().clone();
            let missing = decode_frame(
                &source(false),
                frame(vec![("S", array.clone())]),
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(missing.cell(0, 0).unwrap(), ValueRef::Missing, "{kind}");
            assert_eq!(missing.cell(1, 0).unwrap(), ValueRef::Str("x"), "{kind}");
            assert_eq!(missing.cell(2, 0).unwrap(), ValueRef::Missing, "{kind}");
            let present = decode_frame(
                &source(true),
                frame(vec![("S", array)]),
                SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(present.cell(0, 0).unwrap(), ValueRef::Str(""), "{kind}");
            assert_eq!(present.cell(2, 0).unwrap(), ValueRef::Missing, "{kind}");
        }
    }

    /// An `odm` read binds the eleven fields by name, ignoring ASCII case
    /// (NAME-8), and takes them in their canonical order and spelling,
    /// whatever text layout the frame holds them in.
    #[test]
    fn an_odm_frame_binds_the_eleven_canonical_fields_whatever_their_case_and_layout() {
        use yamaa_core::odm_source::FIELDS;
        // Lower-cased and in reverse order, as large strings, each holding
        // its own name with a suffix.
        let names = FIELDS
            .iter()
            .rev()
            .map(|name| name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let columns = names
            .iter()
            .map(|name| {
                let values = LargeStringArray::from(vec![Some(format!("{name}-1"))]);
                (name.as_str(), Arc::new(values) as ArrayRef)
            })
            .collect();
        let table = decode_frame(
            &source(false),
            frame(columns),
            SourceInterpretation::Odm,
            None,
        )
        .unwrap();
        let fields = table
            .schema()
            .columns()
            .iter()
            .map(|column| (column.name.as_str(), column.kind))
            .collect::<Vec<_>>();
        assert_eq!(fields, FIELDS.map(|name| (name, ColumnType::Str)));
        for (column, name) in FIELDS.iter().enumerate() {
            let expected = format!("{}-1", name.to_ascii_lowercase());
            assert_eq!(
                table.cell(0, column).unwrap(),
                ValueRef::Str(&expected),
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod width_tests {
    use super::*;
    use yamaa_core::{
        specification::SourceProfile,
        table::{TableAccess, ValueRef},
        value::ColumnType,
    };

    #[test]
    #[cfg(feature = "parquet")]
    fn parquet_decode_admits_small_groups_beyond_prior_group_and_metadata_frontiers() {
        use arrow_array::{Int64Array, RecordBatch};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            byte_order_mark: false,
            empty_string_present: false,
            ordinal: None,
        };
        let schema = Arc::new(Schema::new(vec![Field::new("ID", DataType::Int64, true)]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from_iter_values(0..5000))],
        )
        .unwrap();
        let properties = crate::storage_api::writer_properties_max_rows(1);
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties)).unwrap();
        // One row per write still yields one row group per row; a single
        // 5,000-row write makes the writer recurse once per group it splits
        // out and overflows the test thread.
        for row in 0..batch.num_rows() {
            writer.write(&batch.slice(row, 1)).unwrap();
        }
        writer.close().unwrap();
        let table = decode_parquet(&source, &bytes, false).unwrap();
        assert_eq!(table.row_count(), 5000);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(0));
        assert_eq!(table.cell(4999, 0).unwrap(), ValueRef::Int(4999));
    }

    #[test]
    #[cfg(feature = "parquet")]
    fn parquet_decode_admits_1024_and_refuses_1025_source_fields() {
        use arrow_array::{ArrayRef, RecordBatch, StringArray};
        use arrow_schema::{DataType, Field, Schema};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;

        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "input.parquet".into(),
            types: vec![],
            profile: SourceProfile::Parquet,
            encoding: None,
            byte_order_mark: false,
            empty_string_present: false,
            ordinal: None,
        };
        for width in [1_024, 1_025] {
            let schema = Arc::new(Schema::new(
                (0..width)
                    .map(|column| Field::new(format!("F{column:04}"), DataType::Utf8, true))
                    .collect::<Vec<_>>(),
            ));
            let value = Arc::new(StringArray::from(vec!["x"])) as ArrayRef;
            let batch = RecordBatch::try_new(schema.clone(), vec![value; width]).unwrap();
            let mut bytes = Vec::new();
            let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).unwrap();
            writer.write(&batch).unwrap();
            writer.close().unwrap();
            let result = decode_source(
                &source,
                &bytes,
                yamaa_core::specification::SourceInterpretation::Ordinary,
                None,
            );
            if width == 1_024 {
                let table = result.unwrap();
                assert_eq!(table.schema().columns().len(), 1_024);
                assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            } else {
                assert!(matches!(
                    result,
                    Err(Error::ParquetSource(
                        crate::parquet_source::Error::ResourceLimit {
                            resource: "columns",
                            limit: 1024,
                            actual: Some(1025)
                        }
                    ))
                ));
            }
        }
    }

    #[test]
    fn csv_decode_admits_1024_typed_and_untyped_fields_with_bounded_headers() {
        let names = (0..1_024)
            .map(|column| format!("F{column:04}"))
            .collect::<Vec<_>>();
        let bytes = format!("{}\n{}\n", names.join(","), vec!["x"; 1_024].join(","));
        for typed in [false, true] {
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: "input.csv".into(),
                types: if typed {
                    names
                        .iter()
                        .map(|name| (name.clone(), ColumnType::Str))
                        .collect()
                } else {
                    vec![]
                },
                profile: SourceProfile::Csv,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            // One complete header plus one row must fit: the old 64-header
            // allowance refused this in-range table before width admission.
            let table = decode_source(
                &source,
                bytes.as_bytes(),
                yamaa_core::specification::SourceInterpretation::Ordinary,
                None,
            )
            .unwrap();
            assert_eq!(table.schema().columns().len(), 1_024);
            assert_eq!(table.cell(0, 1_023).unwrap(), ValueRef::Str("x"));
            let excess = format!("{},EXCESS\n", names.join(","));
            assert!(matches!(
                decode_source(
                    &source,
                    excess.as_bytes(),
                    yamaa_core::specification::SourceInterpretation::Ordinary,
                    None
                ),
                Err(Error::Source(TextTableError::Csv(
                    crate::csv_source::Error::Limit {
                        resource: "columns",
                        limit: 1_024,
                        actual: Some(1_025)
                    }
                )))
            ));
        }
    }
}
