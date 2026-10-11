//! Captured YAML codec and compatibility results for engine project preparation.
use crate::specification_source::{CapturedDocument, CapturedSchema, Error, Source};
pub use application::{
    CapturePort, DecodeTarget, Kind, Limits, LockCapture, Origin, Reply, Request, SourceOrigins,
};
use std::sync::Arc;
use yamaa_core::{
    project_environment::Environment, project_function::Language, project_function_document,
};
use yamaa_engine::project_preparation as application;
pub type CaptureFailure<E> = application::CaptureFailure<E, CapturedDocument, Error>;
pub type SourceFailure<E> = application::SourceFailure<E, CapturedDocument, Error>;
pub type DocumentCapture = application::DocumentCapture<CapturedDocument>;
pub type RejectedEnvironment<E> = application::RejectedEnvironment<E, CapturedDocument, Error>;

#[derive(Debug)]
pub struct PreparedEnvironment {
    pub origins: SourceOrigins,
    pub root: CapturedDocument,
    pub captures: Vec<DocumentCapture>,
    pub lock: Option<LockCapture>,
    pub environment: Environment,
}
/// Preserve captured provenance while moving definitions into immutable shared
/// execution metadata. Construction performs no recapture or activation.
#[derive(Debug)]
pub struct OwnedEnvironment {
    origins: SourceOrigins,
    root: CapturedDocument,
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
    environment: yamaa_core::project_environment::ExecutionEnvironment,
}
/// Captured origins and exact bytes move separately from the engine's execution
/// capability. This contains no replacement definitions or activation state.
#[derive(Debug)]
pub struct CapturedEnvironment {
    origins: SourceOrigins,
    root: CapturedDocument,
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
}
impl CapturedEnvironment {
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    pub fn root(&self) -> &CapturedDocument {
        &self.root
    }
    pub fn captures(&self) -> &[DocumentCapture] {
        &self.captures
    }
    pub fn lock(&self) -> Option<&LockCapture> {
        self.lock.as_ref()
    }
}
impl PreparedEnvironment {
    pub fn into_owned(self) -> OwnedEnvironment {
        OwnedEnvironment {
            origins: self.origins,
            root: self.root,
            captures: self.captures,
            lock: self.lock,
            environment: self.environment.into_execution(),
        }
    }
}
impl OwnedEnvironment {
    /// Transfer both checked metadata and provenance without copying or reading.
    pub fn into_parts(
        self,
    ) -> (
        yamaa_core::project_environment::ExecutionEnvironment,
        CapturedEnvironment,
    ) {
        (
            self.environment,
            CapturedEnvironment {
                origins: self.origins,
                root: self.root,
                captures: self.captures,
                lock: self.lock,
            },
        )
    }
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    pub fn root(&self) -> &CapturedDocument {
        &self.root
    }
    pub fn captures(&self) -> &[DocumentCapture] {
        &self.captures
    }
    pub fn lock(&self) -> Option<&LockCapture> {
        self.lock.as_ref()
    }
    pub fn environment(&self) -> &yamaa_core::project_environment::ExecutionEnvironment {
        &self.environment
    }
}
#[derive(Debug)]
pub enum Failure<E> {
    Interrupted {
        origin: Origin,
        written: String,
        error: E,
    },
    Root(Error),
    RootScalar {
        root: Box<CapturedDocument>,
        findings: Vec<project_function_document::Finding>,
    },
    Rejected(Box<RejectedEnvironment<E>>),
}

struct Decoder(Arc<CapturedSchema>);
impl application::DecodePort for Decoder {
    type Error = Error;
    type Document = CapturedDocument;
    fn decode(&mut self, source: Source, target: DecodeTarget) -> Result<CapturedDocument, Error> {
        match target {
            DecodeTarget::Environment => self.0.prepare_structural(source),
            DecodeTarget::Function => self
                .0
                .prepare_named_class(source, "function_definition_class"),
            DecodeTarget::Codelist => self.0.prepare_named_class(source, "codelist_source_class"),
        }
    }
    fn document(document: &CapturedDocument) -> &yamaa_core::schema::Document {
        &document.normalized().document
    }
    fn source(document: &CapturedDocument) -> &Source {
        document.source()
    }
}

/// Decode through the captured schema codec; the engine owns ordering and admission.
pub fn prepare<P: CapturePort>(
    schema: Arc<CapturedSchema>,
    source: Source,
    host: Language,
    port: &mut P,
    limits: Limits,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    match application::prepare(&mut Decoder(schema), source, host, port, limits) {
        Ok(prepared) => Ok(PreparedEnvironment {
            origins: prepared.origins,
            root: prepared.root,
            captures: prepared.captures,
            lock: prepared.lock,
            environment: prepared.environment,
        }),
        Err(error) => Err(match error {
            application::Failure::Interrupted {
                origin,
                written,
                error,
            } => Failure::Interrupted {
                origin,
                written,
                error,
            },
            application::Failure::Root(error) => Failure::Root(error),
            application::Failure::RootLimit(code) => Failure::Root(Error::Limit(code)),
            application::Failure::RootScalar { root, findings } => {
                Failure::RootScalar { root, findings }
            }
            application::Failure::Rejected(rejected) => Failure::Rejected(rejected),
        }),
    }
}
