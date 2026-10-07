//! Original YAML run prototype over captured schema and CSV snapshots. This is
//! not a filesystem runner, artifact publisher or qualified host frontend yet.
use crate::{
    arrow_table::{ArrowTable, TableLimits},
    csv_source::{self, TextTableError},
    dataset_transport::{self, DatasetResponse, DatasetTransportError},
    specification_source::PreparedDocument,
};
use std::sync::Arc;
use yamaa_engine::specification::{
    BindError, PrepareError, PreparedSpecification, SourceDeclaration,
};
#[derive(Debug)]
pub enum Error {
    Prepare(PrepareError),
    Source(TextTableError),
    TypedSource(crate::typed_csv::Error),
    Bind(BindError),
    Execution(DatasetTransportError),
}
/// Host authority for resource capture. Implementations must bound reads before
/// allocation, retain immutable snapshots, and count newly created snapshots.
/// No parser, model, binding or expression semantics belong in this port.
pub trait SourcePort {
    type Error;
    fn capture_reads(&self) -> usize;
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        byte_limit: usize,
    ) -> Result<Arc<[u8]>, Self::Error>;
}
#[derive(Debug)]
pub struct SourceRead {
    pub source: SourceDeclaration,
    pub captured: bool,
    /// A regressing host counter is a boundary failure, never an invented zero.
    pub snapshots_created: Option<usize>,
}
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
    /// One run borrows raw captured bytes, creates an owned lossless snapshot,
    /// binds its actual schema, then executes with the existing portable response.
    pub fn execute_csv(&self, bytes: &[u8]) -> Result<DatasetResponse, Error> {
        let table = self.decode_csv(bytes)?;
        self.execute_table(&table)
    }
    fn decode_csv(&self, bytes: &[u8]) -> Result<ArrowTable, Error> {
        crate::typed_csv::parse(
            bytes,
            &self.source().types,
            csv_source::Limits::default(),
            TableLimits {
                max_rows: 65_536,
                max_columns: 64,
                max_batches: 1,
                max_cells: 262_144,
            },
        )
        .map_err(|error| match error {
            crate::typed_csv::Error::Csv(error) => Error::Source(TextTableError::Csv(error)),
            crate::typed_csv::Error::Table(error) => Error::Source(TextTableError::Table(error)),
            error => Error::TypedSource(error),
        })
    }
    fn execute_table(&self, table: &ArrowTable) -> Result<DatasetResponse, Error> {
        let plan = self
            .prepared
            .bind(yamaa_core::table::TableAccess::schema(table))
            .map_err(Error::Bind)?;
        dataset_transport::execute_specification_plan(&plan, table).map_err(Error::Execution)
    }
    /// Invoke the actual resource port once. Cached captures remain observable
    /// requests, while snapshots_created comes from the port's actual counter.
    pub fn execute_with_port<P: SourcePort>(&self, port: &mut P) -> CapturedAttempt<P::Error> {
        let before = port.capture_reads();
        let captured = port.capture(self.source(), csv_source::Limits::default().bytes);
        let snapshots_created = port.capture_reads().checked_sub(before);
        let read = SourceRead {
            source: self.source().clone(),
            captured: captured.is_ok(),
            snapshots_created,
        };
        let mut attempt = CapturedAttempt {
            read,
            snapshot: None,
            table: None,
            result: Err(PortError::CaptureAccounting),
        };
        match captured {
            Err(error) => {
                if snapshots_created.is_some() {
                    attempt.result = Err(PortError::Capture(error));
                }
            }
            Ok(bytes) => {
                attempt.snapshot = Some(bytes);
                if snapshots_created.is_none() {
                    return attempt;
                }
                match self.decode_csv(attempt.snapshot.as_deref().expect("captured bytes")) {
                    Err(error) => attempt.result = Err(PortError::Run(error)),
                    Ok(table) => {
                        attempt.result = self.execute_table(&table).map_err(PortError::Run);
                        attempt.table = Some(table);
                    }
                }
            }
        }
        attempt
    }
}
