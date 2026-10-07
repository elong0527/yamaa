//! Original YAML run prototype over captured schema and CSV snapshots. This is
//! not a filesystem runner, artifact publisher or qualified host frontend yet.
use crate::{
    arrow_table::{ArrowTable, TableLimits},
    csv_source::{self, TextTableError},
    dataset_transport::{self, DatasetResponse, DatasetTransportError},
    specification_source::PreparedDocument,
};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use yamaa_engine::specification::{
    BindError, PrepareError, PreparedSpecification, SourceDeclaration,
};
use yamaa_engine::specification_run as application;
#[derive(Debug)]
pub enum Error {
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    Bind(BindError),
    Execution(DatasetTransportError),
}
pub use yamaa_engine::specification_run::{SourcePort, SourceRead};
#[derive(Debug)]
pub enum PortError<E> {
    Capture(E),
    Run(Error),
    CaptureAccounting,
}
/// Observations survive failed binding/execution. The table is the exact owned
/// source snapshot used for execution; reporting never reopens or reparses data.
pub struct CapturedAttempt<E> {
    pub read: SourceRead,
    pub snapshot: Option<Arc<[u8]>>,
    pub table: Option<ArrowTable>,
    pub result: Result<DatasetResponse, PortError<E>>,
}

/// Keeps schema/source provenance alive through binding and execution.
#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    prepared: PreparedSpecification,
}
impl PreparedRun {
    /// Entire vocabulary admission precedes obtaining a study-data snapshot.
    pub fn prepare(document: PreparedDocument) -> Result<Self, Error> {
        let prepared = PreparedSpecification::prepare(document.model()).map_err(Error::Prepare)?;
        Ok(Self { document, prepared })
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub fn key_names(&self) -> impl Iterator<Item = &str> {
        self.prepared.key_names()
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        &self.prepared
    }
    pub fn source(&self) -> &SourceDeclaration {
        self.prepared.source()
    }
    /// The shared application service decodes, binds and executes held bytes.
    pub fn execute_csv(&self, bytes: &[u8]) -> Result<DatasetResponse, Error> {
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            application::execute_bytes(&self.prepared, bytes, &mut CsvDecoder, limits())
        }))
        .map_err(|_| Error::Execution(DatasetTransportError::Internal))?;
        let execution = attempt.result.map_err(run_error)?;
        response(execution)
    }

    /// Delegate capture and execution order to the engine. Preserve observations
    /// outside the panic fence; reporting never captures or decodes a source again.
    pub fn execute_with_port<P: SourcePort>(&self, port: &mut P) -> CapturedAttempt<P::Error> {
        let mut attempt = application::CapturedAttempt::new(self.source());
        let guarded = catch_unwind(AssertUnwindSafe(|| {
            application::execute_with_port_into(
                &self.prepared,
                port,
                &mut CsvDecoder,
                limits(),
                &mut attempt,
            );
        }));
        let result = if guarded.is_err() {
            Err(PortError::Run(Error::Execution(
                DatasetTransportError::Internal,
            )))
        } else {
            match attempt.result {
                Ok(execution) => response(execution).map_err(PortError::Run),
                Err(application::PortError::Incomplete) => Err(PortError::Run(Error::Execution(
                    DatasetTransportError::Internal,
                ))),
                Err(application::PortError::Capture(error)) => Err(PortError::Capture(error)),
                Err(application::PortError::Run(error)) => Err(PortError::Run(run_error(error))),
                Err(application::PortError::CaptureAccounting) => Err(PortError::CaptureAccounting),
            }
        };
        CapturedAttempt {
            read: attempt.read,
            snapshot: attempt.snapshot,
            table: attempt.table.map(|table| table.0),
            result,
        }
    }
}

fn limits() -> application::Limits {
    application::Limits {
        source_bytes: csv_source::Limits::default().bytes,
        source_cells: dataset_transport::MAX_SOURCE_CELLS,
        execution: dataset_transport::LIMITS,
    }
}

fn response(
    execution: yamaa_engine::dataset::ExecutionAttempt<crate::function_transport::CallbackError>,
) -> Result<DatasetResponse, Error> {
    catch_unwind(AssertUnwindSafe(|| dataset_transport::response(execution)))
        .map_err(|_| Error::Execution(DatasetTransportError::Internal))?
        .map_err(Error::Execution)
}

fn run_error(error: application::RunError<Error>) -> Error {
    match error {
        application::RunError::Decode(error) => error,
        application::RunError::Bind(error) => Error::Bind(error),
        application::RunError::SourceBytes { limit } => {
            Error::Source(TextTableError::Csv(csv_source::Error::Limit {
                resource: "bytes",
                limit,
            }))
        }
        application::RunError::SourceCells => Error::Execution(DatasetTransportError::Table(
            crate::table_transport::TableTransportError::ShapeLimit,
        )),
    }
}

struct CsvDecoder;
impl application::SourceDecoder for CsvDecoder {
    type Error = Error;
    type Table = dataset_transport::Snapshot;
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
    ) -> Result<Self::Table, Self::Error> {
        crate::typed_csv::parse(
            bytes,
            &source.types,
            csv_source::Limits::default(),
            TableLimits {
                max_rows: 65_536,
                max_columns: 64,
                max_batches: 1,
                max_cells: 262_144,
            },
        )
        .map(dataset_transport::Snapshot)
        .map_err(|error| match error {
            crate::typed_csv::Error::Csv(error) => Error::Source(TextTableError::Csv(error)),
            crate::typed_csv::Error::Table(error) => Error::Source(TextTableError::Table(error)),
            error => Error::TypedSource(error),
        })
    }
}
