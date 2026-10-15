//! Capture, decode, bind and execute a prepared specification through native ports.
//! Filesystem access, codecs and host exception containment belong to adapters.
use crate::dataset::{self, ExecutionAttempt};
use crate::{
    resource::ResourceFailure,
    specification::{BindError, PreparedSpecification, SourceDeclaration, SourceInterpretation},
    table::TableAccess,
};
use std::{sync::Arc, vec};

/// Capture authority for a declared source. Bound allocation before returning,
/// hold immutable bytes and compare cached snapshots on bytes, never a digest.
pub trait SourcePort {
    type Error;
    /// Opt in only for known resource causes. Opaque host errors retain their
    /// identity and are not converted by inspecting their text or class name.
    fn resource_failure(&self, error: &Self::Error) -> Option<ResourceFailure>;
    /// Inspect location and file kind without reading study bytes or creating a
    /// snapshot.
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Self::Error>;
    fn capture_reads(&self) -> usize;
    /// Mark the entered acquisition branch before crossing the source boundary.
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        origin: &mut Option<SourceOrigin>,
    ) -> Result<Arc<[u8]>, Self::Error>;
}

/// Decode one captured source with its declared logical types. The returned table
/// owns its storage and must not retain borrowed input or temporary host buffers.
pub trait SourceDecoder {
    type Error;
    type Table: TableAccess;
    /// Semantic ingestion findings can be collected across the remaining declared
    /// sources. Resource, transport and opaque boundary failures must abort.
    fn continue_after(&self, error: &Self::Error) -> bool;
    /// Apply the declared interpretation and any actual producer contract before
    /// value conversion. The table owns its decoded physical storage.
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        interpretation: SourceInterpretation,
        contract: Option<&crate::producer_contract::Contract>,
    ) -> Result<Self::Table, Self::Error>;
}

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

fn odm_records<T: TableAccess>(
    table: &T,
    declaration: &SourceDeclaration,
    index: usize,
    driver: usize,
) -> Result<Vec<crate::odm_source::MissingValues>, Box<dataset::ExecutionError<T::Error>>> {
    crate::odm_source::missing_values(
        table,
        if declaration.profile == crate::specification::SourceProfile::Csv {
            2
        } else {
            1
        },
    )
    .map_err(|error| {
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
    })
}

fn ingestion_failure<E>(error: Box<dataset::ExecutionError<E>>) -> ExecutionAttempt<E> {
    ExecutionAttempt {
        result: Err(error),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
    }
}

/// The acquisition actually entered, separate from snapshot creation counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceOrigin {
    Resource,
    Generated,
}

#[derive(Debug)]
pub struct SourceRead {
    pub source: SourceDeclaration,
    /// None means routing stopped before either acquisition branch was entered.
    pub origin: Option<SourceOrigin>,
    pub captured: bool,
    pub failure: Option<ResourceFailure>,
    /// A regressing counter is a boundary failure, never an invented zero.
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
    #[error("specification run capture accounting")]
    CaptureAccounting,
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
) -> impl Iterator<Item = &'a crate::diagnostic::Diagnostic> {
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

fn decode_and_execute<D: SourceDecoder>(
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
            .decode(
                prepared.source(),
                bytes,
                prepared
                    .source_interpretation(prepared.driver_index())
                    .expect("admitted source interpretation"),
                None,
            )
            .map_err(RunError::Decode)?,
    );
    let table = captured_table
        .as_ref()
        .expect("decoded table is held for this attempt");
    if prepared.source_interpretation(prepared.driver_index()) == Some(SourceInterpretation::Odm) {
        crate::limits::Limits::table(table.row_count(), table.schema().columns().len()).map_err(
            |error| RunError::Boundary {
                source: prepared.driver_index(),
                error,
            },
        )?;
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
    Ok(plan.execute(
        table,
        &[],
        &mut crate::function_invocation::NoFunctions::new(),
        None,
    ))
}

/// Run from caller-held bytes without a resource request. Reuse repeats decoding,
/// binding and execution; no mutable application state is stored in the model.
pub fn execute_bytes<D: SourceDecoder>(
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
pub fn execute_into<P: SourcePort, D: SourceDecoder>(
    prepared: &PreparedSpecification,
    port: &mut P,
    decoder: &mut D,
    functions: &mut dyn crate::function_invocation::FunctionPort<
        Error = <D::Table as TableAccess>::Error,
    >,
    attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
    scope: crate::application_observer::Scope<'_>,
) {
    scope.phase(crate::application_observer::Phase::Input);
    let input_finished = std::cell::Cell::new(false);
    execute_inner_with_observer_into(
        prepared,
        port,
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

#[allow(clippy::too_many_arguments)]
fn execute_inner_with_observer_into<P: SourcePort, D: SourceDecoder>(
    prepared: &PreparedSpecification,
    port: &mut P,
    decoder: &mut D,
    functions: &mut dyn crate::function_invocation::FunctionPort<
        Error = <D::Table as TableAccess>::Error,
    >,
    attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
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
                });
                return;
            }
        }
    }
    let mut inspections = Vec::new();
    for declaration in prepared.sources() {
        let before = port.capture_reads();
        let inspected = port.inspect(declaration);
        if port.capture_reads() != before {
            attempt.result = Err(PortError::CaptureAccounting);
            return;
        }
        if let Err(error) = inspected {
            let failure = port.resource_failure(&error);
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
    for (index, declaration) in prepared.sources().iter().enumerate() {
        // Record the request before crossing the host boundary so a panic leaves
        // a truthful incomplete observation without discarding earlier sources.
        attempt.sources.push(CapturedSource::new(declaration));
        let observed = attempt.sources.last_mut().expect("requested source");
        let before = port.capture_reads();
        let captured = port.capture(declaration, &mut observed.read.origin);
        observed.read.captured = captured.is_ok();
        observed.read.snapshots_created = port.capture_reads().checked_sub(before);
        match captured {
            Ok(bytes) => observed.snapshot = Some(bytes),
            Err(error) => {
                observed.read.failure = port.resource_failure(&error);
                attempt.result = Err(if observed.read.snapshots_created.is_none() {
                    PortError::CaptureAccounting
                } else {
                    PortError::Capture(error)
                });
                return;
            }
        }
        if observed.read.snapshots_created.is_none()
            || observed.read.origin.is_none()
            || (observed.read.origin == Some(SourceOrigin::Generated)
                && observed.read.snapshots_created != Some(0))
        {
            attempt.result = Err(PortError::CaptureAccounting);
            return;
        }
        let bytes = observed.snapshot.as_deref().expect("captured source");
        if let Err(error) = crate::limits::Limits::source_bytes(bytes.len()) {
            attempt.result = Err(PortError::Run(RunError::Boundary {
                source: index,
                error,
            }));
            return;
        }
        match decoder.decode(
            declaration,
            bytes,
            prepared
                .source_interpretation(index)
                .expect("admitted source interpretation"),
            None,
        ) {
            Ok(table) => {
                if let Err(error) =
                    crate::limits::Limits::table(table.row_count(), table.schema().columns().len())
                {
                    attempt.result = Err(PortError::Run(RunError::Boundary {
                        source: index,
                        error,
                    }));
                    return;
                }
                observed.table = Some(table);
                if prepared.source_interpretation(index) == Some(SourceInterpretation::Odm) {
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
    let primary = attempt.sources[prepared.driver_index()]
        .table
        .as_ref()
        .expect("decoded driver");
    let secondary = attempt
        .sources
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != prepared.driver_index())
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
    };
    attempt.result = Ok(plan.execute(primary, &secondary, &mut functions, Some(&mut observer)));
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
