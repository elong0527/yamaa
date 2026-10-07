//! Capture, decode, bind and execute a prepared specification through native ports.
//! Filesystem access, codecs and host exception containment belong to adapters.
use crate::dataset::{self, DatasetExecution, ExecutionAttempt};
use alloc::{sync::Arc, vec, vec::Vec};
use yamaa_core::{
    specification::{BindError, PreparedSpecification, SourceDeclaration},
    table::TableAccess,
};

/// Capture authority for a declared source. Bound allocation before returning,
/// hold immutable bytes and compare cached snapshots on bytes, never a digest.
pub trait SourcePort {
    type Error;
    fn capture_reads(&self) -> usize;
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        byte_limit: usize,
    ) -> Result<Arc<[u8]>, Self::Error>;
}

/// Decode one captured source with its declared logical types. The returned table
/// owns its storage and must not retain borrowed input or temporary host buffers.
pub trait SourceDecoder {
    type Error;
    type Table: TableAccess;
    /// Semantic ingestion findings can be collected across the remaining declared
    /// sources. Resource, transport and opaque boundary failures must abort.
    fn continue_after(&self, _error: &Self::Error) -> bool {
        false
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
    ) -> Result<Self::Table, Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source_bytes: usize,
    pub source_cells: usize,
    pub execution: dataset::Limits,
}

#[derive(Debug)]
pub enum RunError<E> {
    Decode(E),
    Sources(Vec<(usize, E)>),
    Bind(BindError),
    SourceBytes { limit: usize },
    SourceCells,
}

#[derive(Debug)]
pub struct SourceRead {
    pub source: SourceDeclaration,
    pub captured: bool,
    /// A regressing counter is a boundary failure, never an invented zero.
    pub snapshots_created: Option<usize>,
}

#[derive(Debug)]
pub enum PortError<C, D> {
    /// The host boundary interrupted the attempt before a final result was stored.
    Incomplete,
    Capture(C),
    Run(RunError<D>),
    CaptureAccounting,
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
                captured: false,
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
    limits: Limits,
    captured_table: &mut Option<D::Table>,
) -> Result<ExecutionAttempt<<D::Table as TableAccess>::Error>, RunError<D::Error>> {
    if bytes.len() > limits.source_bytes {
        return Err(RunError::SourceBytes {
            limit: limits.source_bytes,
        });
    }
    *captured_table = Some(
        decoder
            .decode(prepared.source(), bytes)
            .map_err(RunError::Decode)?,
    );
    let table = captured_table
        .as_ref()
        .expect("decoded table is held for this attempt");
    let plan = prepared.bind(table.schema()).map_err(RunError::Bind)?;
    let cells = table
        .row_count()
        .checked_mul(table.schema().columns().len());
    if cells.is_none_or(|cells| cells > limits.source_cells) {
        return Err(RunError::SourceCells);
    }
    Ok(plan.execute_observed(table, limits.execution))
}

/// Run from caller-held bytes without a resource request. Reuse repeats decoding,
/// binding and execution; no mutable application state is stored in the model.
pub fn execute_bytes<D: SourceDecoder>(
    prepared: &PreparedSpecification,
    bytes: &[u8],
    decoder: &mut D,
    limits: Limits,
) -> DecodedAttempt<D::Error, D::Table> {
    let mut table = None;
    let result = decode_and_execute(prepared, bytes, decoder, limits, &mut table);
    DecodedAttempt { table, result }
}

/// Fill a fresh attempt in place so adapters can contain a panic while retaining
/// observations already made. No returned error retries a read, decode or cell.
pub fn execute_with_port_into<P: SourcePort, D: SourceDecoder>(
    prepared: &PreparedSpecification,
    port: &mut P,
    decoder: &mut D,
    limits: Limits,
    attempt: &mut CapturedAttempt<P::Error, D::Error, D::Table>,
) {
    *attempt = CapturedAttempt::new(prepared.source());
    attempt.sources.clear();
    let mut bytes_left = limits.source_bytes;
    let mut errors = Vec::new();
    for (index, declaration) in prepared.sources().iter().enumerate() {
        // Record the request before crossing the host boundary so a panic leaves
        // a truthful incomplete observation without discarding earlier sources.
        attempt.sources.push(CapturedSource::new(declaration));
        let observed = attempt.sources.last_mut().expect("requested source");
        let before = port.capture_reads();
        let captured = port.capture(declaration, bytes_left);
        observed.read.captured = captured.is_ok();
        observed.read.snapshots_created = port.capture_reads().checked_sub(before);
        match captured {
            Ok(bytes) => observed.snapshot = Some(bytes),
            Err(error) => {
                attempt.result = Err(if observed.read.snapshots_created.is_none() {
                    PortError::CaptureAccounting
                } else {
                    PortError::Capture(error)
                });
                return;
            }
        }
        if observed.read.snapshots_created.is_none() {
            attempt.result = Err(PortError::CaptureAccounting);
            return;
        }
        let bytes = observed.snapshot.as_deref().expect("captured source");
        let Some(remaining) = bytes_left.checked_sub(bytes.len()) else {
            attempt.result = Err(PortError::Run(RunError::SourceBytes {
                limit: limits.source_bytes,
            }));
            return;
        };
        // Invalid snapshots still occupy held memory and consume the same budget.
        bytes_left = remaining;
        match decoder.decode(declaration, bytes) {
            Ok(table) => observed.table = Some(table),
            Err(error) => {
                let continuing = decoder.continue_after(&error);
                errors.push((index, error));
                if !continuing {
                    attempt.result = Err(PortError::Run(RunError::Sources(errors)));
                    return;
                }
            }
        }
    }
    if !errors.is_empty() {
        attempt.result = Err(PortError::Run(RunError::Sources(errors)));
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
    // Preserve binding-before-capacity precedence while bounding the whole input
    // collection before the engine reads any cell.
    let cells = attempt.sources.iter().try_fold(0usize, |used, source| {
        let table = source.table.as_ref().expect("decoded source");
        used.checked_add(
            table
                .row_count()
                .checked_mul(table.schema().columns().len())?,
        )
    });
    if cells.is_none_or(|cells| cells > limits.source_cells) {
        attempt.result = Err(PortError::Run(RunError::SourceCells));
        return;
    }
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
    attempt.result = Ok(plan.execute_observed_sources(primary, &secondary, limits.execution));
}
