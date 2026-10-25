//! Capture, decode, bind and execute a prepared specification through the
//! resolver port. Filesystem access, codecs and host exception containment
//! belong to adapters.
use crate::dataset::{self, ExecutionAttempt};
use crate::{
    decoder::Decoder,
    resolver::{Failure, Name, Origin, Resolver},
    resource::ResourceFailure,
    specification::{
        BindError, PreparedSpecification, SourceDeclaration, SourceInterpretation, SourceProfile,
    },
    table::TableAccess,
};
use std::{sync::Arc, vec};

#[derive(Debug)]
pub enum RunError<E> {
    Decode(E),
    Sources(Vec<(usize, E)>),
    AdmittedSources(Vec<(usize, SourceIssue<E>)>),
    Bind(BindError),
    Boundary {
        source: usize,
        error: crate::limits::LimitExceeded,
    },
}

#[derive(Debug)]
pub enum SourceIssue<E> {
    Decode(E),
    OdmValues(Vec<crate::odm_source::MissingValues>),
}

fn source_errors<E>(errors: Vec<(usize, SourceIssue<E>)>) -> RunError<E> {
    if errors
        .iter()
        .all(|(_, error)| matches!(error, SourceIssue::Decode(_)))
    {
        RunError::Sources(
            errors
                .into_iter()
                .map(|(index, error)| {
                    let SourceIssue::Decode(error) = error else {
                        unreachable!("checked decode-only sources")
                    };
                    (index, error)
                })
                .collect(),
        )
    } else {
        RunError::AdmittedSources(errors)
    }
}

/// The record number of an ODM input's first data record: a CSV header is
/// record 1.
fn first_odm_record(declaration: &SourceDeclaration) -> usize {
    if declaration.profile == crate::specification::SourceProfile::Csv {
        2
    } else {
        1
    }
}

fn odm_records<T: TableAccess>(
    table: &T,
    declaration: &SourceDeclaration,
    index: usize,
    driver: usize,
) -> Result<Vec<crate::odm_source::MissingValues>, Box<dataset::ExecutionError<T::Error>>> {
    crate::odm_source::missing_values(table, first_odm_record(declaration))
        .map_err(|error| odm_failure(error, declaration, index, driver))
}

/// The ingestion failure of an ODM input whose records could not be read.
fn odm_failure<E>(
    error: crate::odm_source::RecordError<E>,
    declaration: &SourceDeclaration,
    index: usize,
    driver: usize,
) -> Box<dataset::ExecutionError<E>> {
    Box::new(match error {
        crate::odm_source::RecordError::Schema => dataset::ExecutionError::SchemaMismatch,
        crate::odm_source::RecordError::Coordinate => dataset::ExecutionError::Capacity,
        crate::odm_source::RecordError::Cell { row, column, error } => {
            let path = std::format!(
                "input.{}.{}",
                declaration.name,
                crate::odm_source::FIELDS[column]
            );
            if index == driver {
                dataset::ExecutionError::Cell {
                    path,
                    source_row: row,
                    error,
                }
            } else {
                dataset::ExecutionError::SecondaryCell {
                    path,
                    source: if index < driver { index } else { index - 1 },
                    source_row: row,
                    error,
                }
            }
        }
    })
}

/// How a partitioned run reads its driver input by partition (#3792): the
/// leading declared keys its partitions keep whole, and the driver fields
/// that hold them, none for consecutive records (#3830).
struct ByPartition {
    partitioning: dataset::Partitioning,
    keys: usize,
    fields: Vec<usize>,
}

type PartitionRead = (ByPartition, Vec<std::ops::Range<usize>>);

/// What a scan of a driver input read by partition found (#3830): the
/// records of each partition the run divides it into, cut as the scan read
/// them, and the ODM findings of its records, the first record failure
/// first, as a whole read's ODM check finds them.
struct Scan<E> {
    partitions: Vec<std::ops::Range<usize>>,
    odm: Result<Vec<crate::odm_source::MissingValues>, crate::odm_source::RecordError<E>>,
}

/// Admit the decoded input using the same geometry policy for a whole
/// table or for each partition the scan cut from its records.
fn admit_source_table<T: TableAccess>(
    table: &T,
    scan: Option<&Scan<T::Error>>,
) -> Result<(), crate::limits::LimitExceeded> {
    match scan {
        Some(scan) => {
            let width = table.schema().columns().len();
            scan.partitions
                .iter()
                .try_for_each(|range| crate::limits::Limits::table(range.len(), width))
        }
        None => crate::limits::Limits::table(table.row_count(), table.schema().columns().len()),
    }
}

/// Scan `declaration`'s held bytes by partition (#3830): every partition
/// decoded once, in order, cut as `read` asks, and its ODM records checked
/// when the input is read as ODM. `None` when the decoder reads the input
/// only whole.
#[allow(clippy::type_complexity)]
fn scan<D: Decoder>(
    decoder: &mut D,
    declaration: &SourceDeclaration,
    bytes: &[u8],
    interpretation: SourceInterpretation,
    contract: Option<&crate::producer_contract::Contract>,
    read: &ByPartition,
) -> Option<Result<(D::Table, Scan<<D::Table as TableAccess>::Error>), D::Error>> {
    use crate::odm_source::RecordError;
    let mut cut = dataset::PartitionCut::new(read.fields.clone(), read.partitioning.rows);
    let odm = interpretation == SourceInterpretation::Odm;
    let mut findings = Vec::new();
    // The whole read's check reads each field over every record before the
    // next field, so its record failure is the first in field order, then
    // in record order; a schema failure precedes every cell.
    let mut failure: Option<RecordError<<D::Table as TableAccess>::Error>> = None;
    let scanned = decoder.partitioned(
        declaration,
        bytes,
        interpretation,
        contract,
        read.partitioning.rows,
        &mut |part, first| {
            cut.read(part);
            if !odm || matches!(failure, Some(RecordError::Schema)) {
                return;
            }
            let record = first_odm_record(declaration).saturating_add(first);
            match crate::odm_source::missing_values(part, record) {
                Ok(values) => crate::odm_source::add_missing_values(&mut findings, values),
                Err(RecordError::Cell { row, column, error }) => {
                    let earlier = match &failure {
                        Some(RecordError::Cell {
                            row: before,
                            column: field,
                            ..
                        }) => (column, first + row) < (*field, *before),
                        Some(_) => false,
                        None => true,
                    };
                    if earlier {
                        failure = Some(RecordError::Cell {
                            row: first + row,
                            column,
                            error,
                        });
                    }
                }
                Err(error) => {
                    if failure.is_none() || matches!(error, RecordError::Schema) {
                        failure = Some(error);
                    }
                }
            }
        },
    )?;
    Some(scanned.map(|table| {
        let partitions = cut
            .finish()
            .unwrap_or_else(|| std::iter::once(0..table.row_count()).collect());
        let odm = match failure {
            Some(error) => Err(error),
            None => Ok(findings),
        };
        (table, Scan { partitions, odm })
    }))
}

fn ingestion_failure<E>(error: Box<dataset::ExecutionError<E>>) -> ExecutionAttempt<E> {
    ExecutionAttempt {
        result: Err(error),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    }
}

#[derive(Debug)]
pub struct SourceRead {
    pub source: SourceDeclaration,
    /// None means routing stopped before the acquisition was entered.
    pub origin: Option<Origin>,
    pub captured: bool,
    pub failure: Option<ResourceFailure>,
    /// The snapshots the capture created: none when it was never entered or
    /// an unwound host boundary stopped it, and zero for a failed capture.
    pub snapshots_created: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum PortError<C, D> {
    /// The host boundary interrupted the attempt before a final result was stored.
    #[error("specification run incomplete")]
    Incomplete,
    #[error("specification run capture: {0:?}")]
    Capture(C),
    #[error("specification run inspect: {0:?}")]
    Inspect(Vec<InspectionFailure<C>>),
    #[error("specification run run: {0:?}")]
    Run(RunError<D>),
}

#[derive(Debug)]
pub struct InspectionFailure<E> {
    pub source: SourceDeclaration,
    pub failure: Option<ResourceFailure>,
    pub error: E,
}

/// Missing inputs cannot establish current-key cardinality, but captured types
/// can still refuse a donor filter. Keep other refusals and later declaration
/// checkpoints separate, and preserve ordinary execution when inputs exist.
pub fn inspection_declaration_diagnostics<'a, E>(
    prepared: &'a PreparedSpecification,
    failures: &[InspectionFailure<E>],
) -> impl Iterator<Item = &'a crate::diagnostic::Diagnostic> + use<'a, E> {
    let missing = !failures.is_empty()
        && failures
            .iter()
            .all(|failure| failure.failure == Some(ResourceFailure::Missing));
    prepared
        .intermediate_filter_type_diagnostics()
        .filter(move |_| missing)
}

/// The exact captured bytes and decoded table remain available after failure.
/// Runtime conditions live in ExecutionAttempt and retain their original payloads.
pub struct CapturedSource<T> {
    pub read: SourceRead,
    pub snapshot: Option<Arc<[u8]>>,
    /// An imported immutable table is retained as typed ownership evidence,
    /// independently of decoding/projection, rather than fake encoded bytes.
    pub owned_snapshot: Option<Arc<dyn std::any::Any + Send + Sync>>,
    pub table: Option<T>,
}
impl<T> CapturedSource<T> {
    fn new(source: &SourceDeclaration) -> Self {
        Self {
            read: SourceRead {
                source: source.clone(),
                origin: None,
                captured: false,
                failure: None,
                snapshots_created: None,
            },
            snapshot: None,
            owned_snapshot: None,
            table: None,
        }
    }
}

pub struct CapturedAttempt<C, D, T: TableAccess> {
    pub sources: Vec<CapturedSource<T>>,
    pub result: Result<ExecutionAttempt<T::Error>, PortError<C, D>>,
}
impl<C, D, T: TableAccess> CapturedAttempt<C, D, T> {
    /// Allocate observations before entering a host's panic/exception boundary.
    pub fn new(source: &SourceDeclaration) -> Self {
        Self {
            sources: vec![CapturedSource::new(source)],
            result: Err(PortError::Incomplete),
        }
    }
}

pub struct DecodedAttempt<D, T: TableAccess> {
    pub table: Option<T>,
    pub result: Result<ExecutionAttempt<T::Error>, RunError<D>>,
}

fn decode_and_execute<D: Decoder>(
    prepared: &PreparedSpecification,
    bytes: &[u8],
    decoder: &mut D,
    captured_table: &mut Option<D::Table>,
) -> Result<ExecutionAttempt<<D::Table as TableAccess>::Error>, RunError<D::Error>> {
    crate::limits::Limits::source_bytes(bytes.len()).map_err(|error| RunError::Boundary {
        source: prepared.driver_index(),
        error,
    })?;
    *captured_table = Some(
        decoder
            .table(
                prepared.source(),
                bytes,
                prepared
                    .source_interpretation(prepared.driver_index())
                    .expect("admitted source interpretation"),
                prepared.source_contract(prepared.driver_index()),
            )
            .map_err(RunError::Decode)?,
    );
    let table = captured_table
        .as_ref()
        .expect("decoded table is held for this attempt");
    crate::limits::Limits::table(table.row_count(), table.schema().columns().len()).map_err(
        |error| RunError::Boundary {
            source: prepared.driver_index(),
            error,
        },
    )?;
    if prepared.source_interpretation(prepared.driver_index()) == Some(SourceInterpretation::Odm) {
        match odm_records(
            table,
            prepared.source(),
            prepared.driver_index(),
            prepared.driver_index(),
        ) {
            Ok(values) if !values.is_empty() => {
                return Err(RunError::AdmittedSources(vec![(
                    prepared.driver_index(),
                    SourceIssue::OdmValues(values),
                )]))
            }
            Ok(_) => (),
            Err(error) => return Ok(ingestion_failure(error)),
        }
    }
    let plan = prepared.bind(table.schema()).map_err(RunError::Bind)?;
    // Bytes a caller holds run to the end: nothing cancels them.
    Ok(plan.execute(
        table,
        &[],
        &mut crate::function_invocation::NoFunctions::new(),
        None,
        &|| false,
    ))
}

/// Run from caller-held bytes without a resource request. Reuse repeats decoding,
/// binding and execution; no mutable application state is stored in the model.
pub fn execute_bytes<D: Decoder>(
    prepared: &PreparedSpecification,
    bytes: &[u8],
    decoder: &mut D,
) -> DecodedAttempt<D::Error, D::Table> {
    let mut table = None;
    let result = decode_and_execute(prepared, bytes, decoder, &mut table);
    DecodedAttempt { table, result }
}

/// Fill caller-owned observations so panic containment retains the entered prefix.
/// The supplied function port is already activated; this service never activates it.
pub fn execute_into<R: Resolver, D: Decoder>(
    prepared: &PreparedSpecification,
    resolver: &mut R,
    decoder: &mut D,
    functions: &mut dyn crate::function_invocation::FunctionPort<
        Error = <D::Table as TableAccess>::Error,
    >,
    attempt: &mut CapturedAttempt<R::Error, D::Error, D::Table>,
    scope: crate::application_observer::Scope<'_>,
) {
    scope.phase(crate::application_observer::Phase::Input);
    let input_finished = std::cell::Cell::new(false);
    execute_inner_into(
        prepared,
        resolver,
        decoder,
        functions,
        attempt,
        (scope, &input_finished),
    );
    if !input_finished.get() {
        let outcome = match &attempt.result {
            Ok(execution) => crate::application_observer::Outcome::from_result(&execution.result),
            Err(_) => crate::application_observer::Outcome::Failed,
        };
        scope.exit(crate::application_observer::Phase::Input, outcome);
    }
}

fn execute_inner_into<R: Resolver, D: Decoder>(
    prepared: &PreparedSpecification,
    resolver: &mut R,
    decoder: &mut D,
    functions: &mut dyn crate::function_invocation::FunctionPort<
        Error = <D::Table as TableAccess>::Error,
    >,
    attempt: &mut CapturedAttempt<R::Error, D::Error, D::Table>,
    (scope, input_finished): (
        crate::application_observer::Scope<'_>,
        &std::cell::Cell<bool>,
    ),
) {
    *attempt = CapturedAttempt::new(prepared.source());
    attempt.sources.clear();
    if let Some(calls) = prepared.project_calls() {
        for (slot, plan) in calls.plans().iter().enumerate() {
            if functions.project_signature(slot) != Some(plan) {
                attempt.result = Ok(ExecutionAttempt {
                    result: Err(Box::new(dataset::ExecutionError::FunctionBinding { slot })),
                    handler_counts: Vec::new(),
                    retained_verifications: Vec::new(),
                    template_rows: Vec::new(),
                });
                return;
            }
        }
    }
    let mut inspections = Vec::new();
    for declaration in prepared.sources() {
        if let Err(error) = resolver.inspect(Name::Input(declaration)) {
            let failure = resolver
                .failure(Name::Input(declaration), &error)
                .and_then(Failure::resource);
            inspections.push(InspectionFailure {
                source: declaration.clone(),
                failure,
                error,
            });
            // Validation causes collect in declaration order. An observed
            // content change is an ingest failure and retains fail-fast order,
            // like opaque host errors and interruptions.
            if failure.is_none() || failure == Some(ResourceFailure::ContentChanged) {
                attempt.result = Err(PortError::Inspect(inspections));
                return;
            }
        }
    }
    if !inspections.is_empty() {
        attempt.result = Err(PortError::Inspect(inspections));
        return;
    }
    let mut errors = Vec::new();
    // How the driver input was read by partition (#3792), and the records
    // of each partition the run divides it into (#3830).
    let mut by_partition: Option<PartitionRead> = None;
    for (index, declaration) in prepared.sources().iter().enumerate() {
        // Record the request before crossing the host boundary so a panic leaves
        // a truthful incomplete observation without discarding earlier sources.
        attempt.sources.push(CapturedSource::new(declaration));
        let observed = attempt.sources.last_mut().expect("requested source");
        // A capture enters the resource unless it returns bytes the caller
        // holds, which never fail: mark the entry first, so a failure or an
        // unwound host boundary keeps a truthful observation.
        observed.read.origin = Some(Origin::Resource);
        let captured = resolver.capture(Name::Input(declaration));
        observed.read.captured = captured.is_ok();
        let owned = match captured {
            Ok(captured) => {
                observed.read.origin = Some(captured.origin);
                observed.read.snapshots_created = Some(usize::from(captured.created));
                observed.snapshot = Some(captured.bytes);
                captured.table
            }
            Err(error) => {
                observed.read.snapshots_created = Some(0);
                observed.read.failure = resolver
                    .failure(Name::Input(declaration), &error)
                    .and_then(Failure::resource);
                attempt.result = Err(PortError::Capture(error));
                return;
            }
        };
        let bytes = observed.snapshot.as_deref().expect("captured source");
        if let Err(error) = crate::limits::Limits::source_bytes(bytes.len()) {
            attempt.result = Err(PortError::Run(RunError::Boundary {
                source: index,
                error,
            }));
            return;
        }
        let interpretation = prepared
            .source_interpretation(index)
            .expect("admitted source interpretation");
        let contract = prepared.source_contract(index);
        let owned_decoded = owned.as_ref().and_then(|table| {
            decoder.served_table(declaration, table.as_ref(), interpretation, contract)
        });
        // Only a table the owned decoder accepted replaces the byte snapshot;
        // a byte fallback keeps the bytes it actually decoded.
        let used_owned = owned_decoded.is_some();
        let read = scope.partitioning().and_then(|partitioning| {
            if used_owned {
                return None;
            }
            reads_by_partition(
                prepared,
                decoder,
                declaration,
                bytes,
                interpretation,
                partitioning,
            )
        });
        let mut scanned = None;
        let decoded = match owned_decoded {
            Some(decoded) => decoded,
            None => {
                let scan = read.as_ref().and_then(|read| {
                    scan(decoder, declaration, bytes, interpretation, contract, read)
                });
                match scan {
                    Some(Ok((table, scan))) => {
                        scanned = Some(scan);
                        Ok(table)
                    }
                    Some(Err(error)) => Err(error),
                    None => decoder.table(declaration, bytes, interpretation, contract),
                }
            }
        };
        if used_owned {
            observed.snapshot = None;
            observed.owned_snapshot = owned;
        }
        match decoded {
            Ok(table) => {
                observed.table = Some(table);
                let table = observed.table.as_ref().expect("decoded source");
                // An input read by partition is admitted a partition at a
                // time, each on its own; any other input is admitted whole.
                let admitted = admit_source_table(table, scanned.as_ref());
                if let Err(error) = admitted {
                    attempt.result = Err(PortError::Run(RunError::Boundary {
                        source: index,
                        error,
                    }));
                    return;
                }
                if let Some(scan) = scanned {
                    match scan.odm {
                        Ok(values) if !values.is_empty() => {
                            errors.push((index, SourceIssue::OdmValues(values)))
                        }
                        Ok(_) => (),
                        Err(error) => {
                            attempt.result = Ok(ingestion_failure(odm_failure(
                                error,
                                declaration,
                                index,
                                prepared.driver_index(),
                            )));
                            return;
                        }
                    }
                    by_partition = read.map(|read| (read, scan.partitions));
                } else if prepared.source_interpretation(index) == Some(SourceInterpretation::Odm) {
                    let table = attempt.sources[index]
                        .table
                        .as_ref()
                        .expect("decoded ODM source");
                    match odm_records(table, declaration, index, prepared.driver_index()) {
                        Ok(values) if !values.is_empty() => {
                            errors.push((index, SourceIssue::OdmValues(values)))
                        }
                        Ok(_) => (),
                        Err(error) => {
                            attempt.result = Ok(ingestion_failure(error));
                            return;
                        }
                    }
                }
            }
            Err(error) => {
                let continuing = decoder.continue_after(&error);
                errors.push((index, SourceIssue::Decode(error)));
                if !continuing {
                    attempt.result = Err(PortError::Run(source_errors(errors)));
                    return;
                }
            }
        }
    }
    if !errors.is_empty() {
        attempt.result = Err(PortError::Run(source_errors(errors)));
        return;
    }
    execute_captured_into(
        prepared,
        decoder,
        functions,
        attempt,
        (scope, input_finished),
        by_partition,
    );
}

/// Bind and execute inputs after every capture and admission succeeds.
fn execute_captured_into<C, D: Decoder>(
    prepared: &PreparedSpecification,
    decoder: &mut D,
    functions: &mut dyn crate::function_invocation::FunctionPort<
        Error = <D::Table as TableAccess>::Error,
    >,
    attempt: &mut CapturedAttempt<C, D::Error, D::Table>,
    (scope, input_finished): (
        crate::application_observer::Scope<'_>,
        &std::cell::Cell<bool>,
    ),
    by_partition: Option<(ByPartition, Vec<std::ops::Range<usize>>)>,
) {
    let schemas = attempt
        .sources
        .iter()
        .map(|source| source.table.as_ref().expect("decoded source").schema())
        .collect::<Vec<_>>();
    let plan = match prepared.bind_sources(&schemas) {
        Ok(plan) => plan,
        Err(error) => {
            attempt.result = Err(PortError::Run(RunError::Bind(error)));
            return;
        }
    };
    let driver = prepared.driver_index();
    let primary = attempt.sources[driver]
        .table
        .as_ref()
        .expect("decoded driver");
    let secondary = attempt
        .sources
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != driver)
        .map(|(_, source)| {
            source.table.as_ref().expect("decoded source")
                as &dyn TableAccess<Error = <D::Table as TableAccess>::Error>
        })
        .collect::<Vec<_>>();
    let phase = std::cell::Cell::new(dataset::ExecutionPhase::Admission);
    let mut functions = scope.functions(functions, &phase);
    input_finished.set(true);
    scope.exit(
        crate::application_observer::Phase::Input,
        crate::application_observer::Outcome::Succeeded,
    );
    let mut observer = |event| match event {
        dataset::ExecutionEvent::Phase(current) => {
            phase.set(current);
            scope.phase(crate::application_observer::Phase::Dataset(current));
        }
        dataset::ExecutionEvent::Exit { phase, outcome } => {
            scope.exit(crate::application_observer::Phase::Dataset(phase), outcome)
        }
        dataset::ExecutionEvent::Refusal { phase, refusal } => {
            scope.refusal(crate::application_observer::Phase::Dataset(phase), refusal)
        }
        dataset::ExecutionEvent::Partition { number, records } => scope.partition(number, records),
        dataset::ExecutionEvent::Divided(division) => scope.divided(division),
    };
    let cancelled = || scope.cancelled();
    attempt.result = match by_partition {
        // The partitions the scan cut the input into are the run's (#3792),
        // each decoded again from the held bytes the scan read when the run
        // reaches it (#3830).
        Some((read, ranges)) => {
            let mut source = Partitions {
                decoder,
                scanned: primary,
                declaration: &prepared.sources()[driver],
                bytes: attempt.sources[driver]
                    .snapshot
                    .as_deref()
                    .expect("a scanned input keeps its bytes"),
                interpretation: prepared
                    .source_interpretation(driver)
                    .expect("admitted source interpretation"),
                contract: prepared.source_contract(driver),
                ranges,
                error: None,
            };
            let executed = plan.execute_partitioned(
                &mut source,
                &secondary,
                &mut functions,
                Some(&mut observer),
                &cancelled,
                read.keys,
            );
            match source.error {
                Some(error) => Err(PortError::Run(source_errors(vec![(
                    driver,
                    SourceIssue::Decode(error),
                )]))),
                None => Ok(executed),
            }
        }
        None => Ok(plan.execute_divided(
            divided(prepared, scope.partitioning()),
            primary,
            &secondary,
            &mut functions,
            Some(&mut observer),
            &cancelled,
        )),
    };
}

/// The partitions of a driver input read by partition (#3830), each decoded
/// again from the held bytes the scan read when the run reaches it, and
/// dropped by the run before it decodes the next.
struct Partitions<'r, D: Decoder> {
    decoder: &'r mut D,
    scanned: &'r D::Table,
    declaration: &'r SourceDeclaration,
    bytes: &'r [u8],
    interpretation: SourceInterpretation,
    contract: Option<&'r crate::producer_contract::Contract>,
    ranges: Vec<std::ops::Range<usize>>,
    /// Why a partition did not decode, which ends the run.
    error: Option<D::Error>,
}

impl<D: Decoder> dataset::PartitionSource for Partitions<'_, D> {
    type Table = D::Table;
    fn schema(&self) -> &crate::table::TableSchema {
        self.scanned.schema()
    }
    fn ranges(&self) -> &[std::ops::Range<usize>] {
        &self.ranges
    }
    fn decode(&mut self, index: usize) -> Option<D::Table> {
        let decoded = self.decoder.partition(
            self.scanned,
            self.declaration,
            self.bytes,
            self.interpretation,
            self.contract,
            self.ranges[index].clone(),
        );
        decoded.map_err(|error| self.error = Some(error)).ok()
    }
}

/// How a run that reads its driver input whole divides it: as `partitioning`
/// asks, admitting the output whole when the driver input is SAS or ODM. The
/// row and cell limits bound such an input whole, and its output too (D264);
/// only a CSV or Parquet driver's output is admitted a partition at a time.
fn divided(
    prepared: &PreparedSpecification,
    partitioning: Option<dataset::Partitioning>,
) -> Option<dataset::Partitioning> {
    let whole_output = !matches!(
        prepared.sources()[prepared.driver_index()].profile,
        SourceProfile::Csv | SourceProfile::Parquet
    );
    partitioning.map(|partitioning| dataset::Partitioning {
        whole_output: partitioning.whole_output || whole_output,
        ..partitioning
    })
}

/// How a partitioned run reads its driver input by partition (#3792), if it
/// does: the specification reads no other input, the decoder reads this
/// one's profile by partition, and the plan, bound to the schema the input's
/// header or footer declares, has no step that needs the whole table under
/// the run's partitions. Every other run reads each input whole and admits
/// it whole, so a plan with such a step, or the whole-table fallback that
/// runs it, refuses an input beyond the row or cell limit exactly as an
/// unpartitioned run does.
///
/// Key-bounded partitions (#3763) are cut as the scan reads the records, by
/// the driver fields that hold the partition keys (#3830); a plan whose
/// driver input does not hold them is read whole, as one partition.
fn reads_by_partition<D: Decoder>(
    prepared: &PreparedSpecification,
    decoder: &mut D,
    declaration: &SourceDeclaration,
    bytes: &[u8],
    interpretation: SourceInterpretation,
    partitioning: dataset::Partitioning,
) -> Option<ByPartition> {
    if prepared.sources().len() != 1 {
        return None;
    }
    let schema = decoder.partition_schema(declaration, bytes, interpretation)?;
    let plan = prepared.bind_sources(&[&schema]).ok()?;
    let keys = partitioning
        .leading_keys
        .map_or(0, |keys| keys.get().min(plan.keys().len()));
    if plan.whole_table_step(keys).is_some() {
        return None;
    }
    let fields = plan.cut_fields(keys)?;
    Some(ByPartition {
        partitioning,
        keys,
        fields,
    })
}

impl<E: std::fmt::Debug> std::fmt::Display for RunError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "specification run: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for RunError<E> {}

impl<E: std::fmt::Debug> std::fmt::Display for InspectionFailure<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "specification run: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for InspectionFailure<E> {}
