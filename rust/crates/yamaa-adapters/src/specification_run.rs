//! Original YAML run prototype over captured schema and CSV snapshots. This is
//! not a filesystem runner, artifact publisher or qualified host frontend yet.
use crate::{
    arrow_table::{ArrowTable, TableLimits},
    csv_source::{self, TextTableError},
    dataset_transport::{self, DatasetResponse, DatasetTransportError},
    specification_source::PreparedDocument,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use yamaa_engine::domain::{self, CheckedSpecification};
use yamaa_engine::specification::{
    BindError, PrepareError, PreparedSpecification, SourceDeclaration,
};
use yamaa_engine::specification_run as application;
#[derive(Debug)]
pub enum Error {
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    Sources(Vec<(SourceDeclaration, Error)>),
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
pub type CapturedSource = application::CapturedSource<ArrowTable>;
pub struct CapturedAttempt<E> {
    pub sources: Vec<CapturedSource>,
    pub result: Result<DatasetResponse, PortError<E>>,
}

/// Keeps schema/source provenance alive through binding and execution.
#[derive(Debug)]
pub struct PreparedRun {
    document: PreparedDocument,
    checked: CheckedSpecification,
}
impl PreparedRun {
    /// Entire vocabulary admission precedes obtaining a study-data snapshot.
    pub fn prepare(document: PreparedDocument) -> Result<Self, Error> {
        let checked = domain::check(document.model()).map_err(Error::Prepare)?;
        Ok(Self { document, checked })
    }
    pub fn document(&self) -> &PreparedDocument {
        &self.document
    }
    pub fn key_names(&self) -> impl Iterator<Item = &str> {
        self.compiled().key_names()
    }
    pub fn compiled(&self) -> &PreparedSpecification {
        self.checked.compiled()
    }
    pub fn source(&self) -> &SourceDeclaration {
        self.compiled().source()
    }
    /// The shared application service decodes, binds and executes held bytes.
    pub fn execute_csv(&self, bytes: &[u8]) -> Result<DatasetResponse, Error> {
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            application::execute_bytes(self.compiled(), bytes, &mut CsvDecoder, limits())
        }))
        .map_err(|_| Error::Execution(DatasetTransportError::Internal))?;
        let execution = attempt
            .result
            .map_err(|error| run_error(error, self.compiled()))?;
        response(execution)
    }

    /// Delegate capture and execution order to the engine. Preserve observations
    /// outside the panic fence; reporting never captures or decodes a source again.
    pub fn execute_with_port<P: SourcePort>(&self, port: &mut P) -> CapturedAttempt<P::Error> {
        let mut attempt = application::CapturedAttempt::new(self.source());
        let guarded = catch_unwind(AssertUnwindSafe(|| {
            self.checked
                .build_into(port, &mut CsvDecoder, limits(), &mut attempt);
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
                Err(application::PortError::Run(error)) => {
                    Err(PortError::Run(run_error(error, self.compiled())))
                }
                Err(application::PortError::CaptureAccounting) => Err(PortError::CaptureAccounting),
            }
        };
        CapturedAttempt {
            sources: attempt
                .sources
                .into_iter()
                .map(|source| CapturedSource {
                    read: source.read,
                    snapshot: source.snapshot,
                    table: source.table.map(|table| table.0),
                })
                .collect(),
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

fn run_error(error: application::RunError<Error>, prepared: &PreparedSpecification) -> Error {
    match error {
        application::RunError::Decode(error) => error,
        application::RunError::Sources(mut errors) if prepared.sources().len() == 1 => {
            errors.pop().expect("source failure").1
        }
        application::RunError::Sources(errors) => Error::Sources(
            errors
                .into_iter()
                .map(|(index, error)| (prepared.sources()[index].clone(), error))
                .collect(),
        ),
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
    fn continue_after(&self, error: &Self::Error) -> bool {
        matches!(
            error,
            Error::Source(TextTableError::Csv(csv_source::Error::Profile { .. }))
                | Error::TypedSource(
                    crate::typed_csv::Error::UnknownField { .. }
                        | crate::typed_csv::Error::FieldParse { .. }
                )
        )
    }
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
