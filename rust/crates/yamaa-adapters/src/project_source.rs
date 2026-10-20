//! Captured YAML codec and compatibility results for engine project preparation.
use crate::document::{self, Closures, Decoded};
use crate::specification_source::{
    CapturedDocument, CapturedFindings, CapturedSchema, Error, Source,
};
pub use application::{
    DecodeTarget, Kind, LockCapture, Origin, ProjectCapturePort, Reply, Request, SourceOrigins,
};
use std::sync::Arc;
use yamaa_core::language::{self, FileType, LanguageVersion};
use yamaa_core::project_preparation as application;
use yamaa_core::{
    ast, project_environment::Environment, project_function::Language, project_function_document,
};
pub type CaptureFailure<E> = application::CaptureFailure<E, CapturedDocument, Error>;
pub type SourceFailure<E> = application::SourceFailure<E, CapturedDocument, Error>;
pub type DocumentCapture = application::DocumentCapture<CapturedDocument>;
pub type RejectedEnvironment<E> = application::RejectedEnvironment<E, CapturedDocument, Error>;

/// The two separately versioned roots one project preparation decodes: the
/// project file with its codelist sources, and the environment file a project
/// links with its function definitions.
#[derive(Clone, Debug)]
pub struct ProjectSchemas {
    pub project: Arc<CapturedSchema>,
    pub environment: Arc<CapturedSchema>,
}

#[derive(Debug)]
pub struct PreparedEnvironment {
    /// The project file lowered, with the environment file the host reads.
    pub project: ast::Project,
    pub origins: SourceOrigins,
    pub root: CapturedDocument,
    pub environment_file: Option<CapturedDocument>,
    pub captures: Vec<DocumentCapture>,
    pub lock: Option<LockCapture>,
    pub environment: Environment,
}
/// Preserve captured provenance while moving definitions into immutable shared
/// execution metadata. Construction performs no recapture or activation.
#[derive(Debug)]
pub struct OwnedEnvironment {
    project: Option<ast::Project>,
    origins: SourceOrigins,
    /// The project file path as preparation was given it.
    written: Option<String>,
    root: Option<CapturedDocument>,
    environment_file: Option<CapturedDocument>,
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
    environment: yamaa_core::project_environment::ExecutionEnvironment,
    inventory_documents: Vec<crate::specification_source::PreparedDocument>,
    inventory_captured: bool,
}
/// Captured origins and exact bytes move separately from the engine's execution
/// capability. This contains no replacement definitions or activation state.
#[derive(Debug)]
pub struct CapturedEnvironment {
    project: Option<ast::Project>,
    inventory_documents: Vec<crate::specification_source::PreparedDocument>,
    origins: SourceOrigins,
    written: Option<String>,
    root: Option<CapturedDocument>,
    environment_file: Option<CapturedDocument>,
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
}
impl CapturedEnvironment {
    /// The project file lowered, with the environment file the host reads.
    pub fn project(&self) -> Option<&ast::Project> {
        self.project.as_ref()
    }
    pub fn inventory_documents(&self) -> &[crate::specification_source::PreparedDocument] {
        &self.inventory_documents
    }
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    /// The caller-supplied project file.
    pub fn root(&self) -> &CapturedDocument {
        self.root.as_ref().expect("explicit project source")
    }
    pub fn root_optional(&self) -> Option<&CapturedDocument> {
        self.root.as_ref()
    }
    /// The project file path as preparation was given it.
    pub fn written(&self) -> Option<&str> {
        self.written.as_deref()
    }
    /// The environment file the project links for the host language. It
    /// declares the lock and functions; their paths resolve from it.
    pub fn environment_file(&self) -> Option<&CapturedDocument> {
        self.environment_file.as_ref()
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
            project: Some(self.project),
            origins: self.origins,
            written: None,
            root: Some(self.root),
            environment_file: self.environment_file,
            captures: self.captures,
            lock: self.lock,
            environment: self.environment.into_execution(),
            inventory_documents: Vec::new(),
            inventory_captured: false,
        }
    }
}
impl OwnedEnvironment {
    pub(crate) fn without_environment(host: Language) -> Self {
        Self {
            project: None,
            origins: SourceOrigins::default(),
            written: None,
            root: None,
            environment_file: None,
            captures: Vec::new(),
            lock: None,
            environment: yamaa_core::build::without_environment(host),
            inventory_documents: Vec::new(),
            inventory_captured: false,
        }
    }
    pub(crate) fn inventory_captured(&self) -> bool {
        self.inventory_captured
    }
    /// Keep the project file path preparation was given, so a later refusal
    /// can name the file as written.
    pub(crate) fn with_written(mut self, written: &str) -> Self {
        self.written = Some(written.into());
        self
    }
    pub(crate) fn attach_inventory(
        &mut self,
        captured: yamaa_core::submission_inventory::Captured<
            crate::specification_source::PreparedDocument,
        >,
    ) {
        self.environment.attach_inventory(captured.inventory);
        self.inventory_documents = captured.specifications;
        self.inventory_captured = true;
    }
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
                project: self.project,
                inventory_documents: self.inventory_documents,
                origins: self.origins,
                written: self.written,
                root: self.root,
                environment_file: self.environment_file,
                captures: self.captures,
                lock: self.lock,
            },
        )
    }
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    /// The project file lowered, with the environment file the host reads.
    pub fn project(&self) -> Option<&ast::Project> {
        self.project.as_ref()
    }
    /// The caller-supplied project file.
    pub fn root(&self) -> &CapturedDocument {
        self.root.as_ref().expect("explicit project source")
    }
    pub fn root_optional(&self) -> Option<&CapturedDocument> {
        self.root.as_ref()
    }
    /// The environment file the project links for the host language.
    pub fn environment_file(&self) -> Option<&CapturedDocument> {
        self.environment_file.as_ref()
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

pub(crate) struct Decoder(pub(crate) ProjectSchemas);
impl application::ProjectDecodePort for Decoder {
    type Error = Error;
    type Document = CapturedDocument;
    fn decode(&mut self, source: Source, target: DecodeTarget) -> Result<CapturedDocument, Error> {
        match target {
            DecodeTarget::Project => self.0.project.prepare_structural(source),
            DecodeTarget::Environment => self.0.environment.prepare_structural(source),
            DecodeTarget::Function => self
                .0
                .environment
                .prepare_named_class(source, "function_definition_class"),
            DecodeTarget::Codelist => self
                .0
                .project
                .prepare_named_class(source, "codelist_source_class"),
            // This decoder admits no 1.x root, so preparation never asks it
            // for a file only a 1.0 project lists.
            DecodeTarget::FunctionFile | DecodeTarget::CodelistFile => {
                unreachable!("a 0.5 decoder admits no 1.0 project")
            }
        }
    }
    fn document(document: &CapturedDocument) -> &yamaa_core::schema::Document {
        &document.normalized().document
    }
    fn source(document: &CapturedDocument) -> &Source {
        document.source()
    }
}

/// The decoder of a build that reads 1.x files, which public calls do not
/// until the engine reads 1.0 files (#3030 D76, P1-9). A root that declares
/// the 1.x header is admitted against the 1.0 closure of its type, at the
/// version it declares, and so is each function file and codelist file a 1.0
/// project lists, which must declare the header (SCHEMA-2). A 0.5 project
/// reads as [`Decoder`] reads it.
pub(crate) struct VersionedDecoder {
    legacy: Decoder,
    closures: Closures,
    reads: LanguageVersion,
}
impl application::ProjectDecodePort for VersionedDecoder {
    type Error = Error;
    type Document = CapturedDocument;
    fn decode(&mut self, source: Source, target: DecodeTarget) -> Result<CapturedDocument, Error> {
        let expected = match target {
            DecodeTarget::Project => FileType::Project,
            DecodeTarget::FunctionFile => FileType::Function,
            DecodeTarget::CodelistFile => FileType::Codelists,
            DecodeTarget::Environment | DecodeTarget::Function | DecodeTarget::Codelist => {
                return self.legacy.decode(source, target)
            }
        };
        match document::decode(source, expected, Some(self.reads), &self.closures)? {
            Decoded::Versioned(document) => Ok(document.admit()?.into_captured()),
            Decoded::Legacy { source, .. } if target == DecodeTarget::Project => {
                self.legacy.decode(source, target)
            }
            Decoded::Legacy { source, raw } => {
                let fault = language::read_header(&raw.document, Some(self.reads), expected)
                    .expect_err("a document without the 1.x header");
                Err(Error::Findings(Box::new(CapturedFindings::new(
                    Arc::clone(self.closures.get(expected)),
                    source,
                    Some(raw.document),
                    vec![fault.finding()],
                ))))
            }
        }
    }
    fn document(document: &CapturedDocument) -> &yamaa_core::schema::Document {
        &document.normalized().document
    }
    fn source(document: &CapturedDocument) -> &Source {
        document.source()
    }
}

/// Decode through the captured schema codecs; the engine owns ordering and admission.
pub fn prepare<P: ProjectCapturePort>(
    schemas: ProjectSchemas,
    source: Source,
    host: Language,
    port: &mut P,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    adapt_preparation(application::prepare(
        &mut Decoder(schemas),
        source,
        host,
        port,
    ))
}

/// Prepare a project as an engine that reads 1.x files up to `reads` does: a
/// 1.0 project, and each function file and codelist file it lists, is read
/// against `closures` (D8, P-TYPE-1); a 0.5 project as [`prepare`] reads it.
/// Only tests call this until the engine reads 1.0 files (#3030 D76, P1-9).
pub fn prepare_versioned<P: ProjectCapturePort>(
    schemas: ProjectSchemas,
    (closures, reads): (Closures, LanguageVersion),
    source: Source,
    host: Language,
    port: &mut P,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    adapt_preparation(application::prepare(
        &mut VersionedDecoder {
            legacy: Decoder(schemas),
            closures,
            reads,
        },
        source,
        host,
        port,
    ))
}

/// Mechanical ownership transfer from the same engine environment preparation.
pub(crate) fn adapt_preparation<E>(
    result: application::PreparationResult<E, CapturedDocument, Error>,
) -> Result<PreparedEnvironment, Failure<E>> {
    match result {
        Ok(prepared) => Ok(PreparedEnvironment {
            project: prepared.project,
            origins: prepared.origins,
            root: prepared.root,
            environment_file: prepared.environment_file,
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
