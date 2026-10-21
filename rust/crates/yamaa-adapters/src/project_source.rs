//! Captured YAML codec and compatibility results for engine project preparation.
use crate::document::{self, Closures, Decoded};
use crate::specification_source::{CapturedDocument, CapturedSchema, Error, Source};
pub use application::{
    DecodeTarget, Kind, LockCapture, Origin, ProjectCapturePort, Reply, Request, SourceOrigins,
};
use std::sync::Arc;
use yamaa_core::language::{FileType, LanguageVersion};
use yamaa_core::project_preparation as application;
use yamaa_core::{
    ast, project_environment::Environment, project_function::Language, project_function_document,
};
pub type CaptureFailure<E> = application::CaptureFailure<E, CapturedDocument, Error>;
pub type SourceFailure<E> = application::SourceFailure<E, CapturedDocument, Error>;
pub type DocumentCapture = application::DocumentCapture<CapturedDocument>;
pub type RejectedEnvironment<E> = application::RejectedEnvironment<E, CapturedDocument, Error>;

/// The schemas one project preparation decodes with: the two separately
/// versioned 0.5 roots, the project file with its codelist sources and the
/// environment file a project links with its function definitions, and, for
/// an engine that reads 1.x files, the 1.0 closure of each file type with the
/// newest version it reads.
#[derive(Clone, Debug)]
pub struct ProjectSchemas {
    pub project: Arc<CapturedSchema>,
    pub environment: Arc<CapturedSchema>,
    /// The 1.0 closures and the newest 1.x version a preparation reads, or
    /// none for one that reads no 1.x file. Public calls read 1.0 files
    /// (#3030 D76): `shipped_schema::capture_project` sets this.
    pub versioned: Option<(Closures, LanguageVersion)>,
}
impl ProjectSchemas {
    /// These schemas, reading 1.x files up to `reads` against `closures`: a
    /// 1.0 project, and each function file and codelist file it lists (D8,
    /// P-TYPE-1).
    pub fn reading(self, closures: Closures, reads: LanguageVersion) -> Self {
        Self {
            versioned: Some((closures, reads)),
            ..self
        }
    }
}

/// Where one admitted definition is declared: the file that holds it and the
/// field that declares it, and whether that file holds it alone, so a path in
/// it starts at that file's root (FUNC-2, D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionOrigin<'a> {
    /// The identity of the file that holds the definition: a 0.5 definition
    /// file, the 0.5 environment file that declares it inline, or a 1.0
    /// function file.
    pub source: &'a str,
    /// The field that declares it: `functions.<name>` in a 0.5 environment
    /// file, or `functions[i]` in a 1.0 project file.
    pub declaration: Declaration<'a>,
    /// Whether `source` holds this definition alone.
    pub external: bool,
}
/// The field that declares one definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declaration<'a> {
    /// `functions.<name>` in a 0.5 environment file.
    Keyed(&'a str),
    /// `functions[i]` in a 1.0 project file.
    Listed(usize),
}
impl std::fmt::Display for Declaration<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keyed(name) => write!(formatter, "functions.{name}"),
            Self::Listed(index) => write!(formatter, "functions[{index}]"),
        }
    }
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
    /// Where the admitted definition at `index`, in admission order, is
    /// declared: a 1.0 definition by the function file the project lists at
    /// `functions[i]`, and a 0.5 one by its definition file or, inline, by the
    /// environment file at `functions.<name>`. `None` when the held origins
    /// name no such definition.
    pub fn function_origin(&self, index: usize) -> Option<FunctionOrigin<'_>> {
        let name = self.origins.functions.get(index)?;
        fn identity(capture: &DocumentCapture) -> &str {
            capture.document.source().identity.as_str()
        }
        if self
            .project
            .as_ref()
            .is_some_and(|project| matches!(project.header.surface, ast::Surface::V1 { .. }))
        {
            let listed = *self.origins.function_files.get(index)?;
            let capture = self
                .captures
                .iter()
                .find(|capture| capture.origin == Origin::FunctionFile(listed))?;
            return Some(FunctionOrigin {
                source: identity(capture),
                declaration: Declaration::Listed(listed),
                external: true,
            });
        }
        let external = self
            .captures
            .iter()
            .find(|capture| matches!(&capture.origin, Origin::Function(held) if held == name));
        let source = match external {
            Some(capture) => identity(capture),
            None => self.environment_file.as_ref()?.source().identity.as_str(),
        };
        Some(FunctionOrigin {
            source,
            declaration: Declaration::Keyed(name),
            external: external.is_some(),
        })
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
    pub(crate) fn without_environment(host: Option<Language>) -> Self {
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

/// The decoder of one project preparation, configured by its schemas. A 0.5
/// project, and every file it links, is read against the 0.5 roots, which
/// refuse a file that declares the 1.x header (#3030 D76). Schemas that read
/// 1.x files ([`ProjectSchemas::versioned`]) admit a project file that
/// declares the header against the 1.0 project closure, at the version it
/// declares, and each function file and codelist file a 1.0 project lists
/// against the closure of its type; a listed file must declare the header,
/// and one whose version the engine does not read, or whose type is another,
/// is refused at the project's field that lists it (SCHEMA-2).
pub(crate) struct Decoder(pub(crate) ProjectSchemas);
impl application::ProjectDecodePort for Decoder {
    type Error = Error;
    type Document = CapturedDocument;
    fn decode(&mut self, source: Source, target: DecodeTarget) -> Result<CapturedDocument, Error> {
        let schemas = &self.0;
        let listed = match target {
            DecodeTarget::Environment => return schemas.environment.prepare_structural(source),
            DecodeTarget::Function => {
                return schemas
                    .environment
                    .prepare_named_class(source, "function_definition_class")
            }
            DecodeTarget::Codelist => {
                return schemas
                    .project
                    .prepare_named_class(source, "codelist_source_class")
            }
            DecodeTarget::Project => None,
            DecodeTarget::FunctionFile => Some(FileType::Function),
            DecodeTarget::CodelistFile => Some(FileType::Codelists),
        };
        let Some((closures, reads)) = &schemas.versioned else {
            // Schemas that read no 1.x file admit no 1.0 project, so
            // preparation never asks them for a file only one lists.
            assert!(listed.is_none(), "a 0.5 decoder admits no 1.0 project");
            return schemas.project.prepare_structural(source);
        };
        if let Some(expected) = listed {
            let document = document::decode_referenced(source, expected, *reads, closures)?;
            return Ok(document.admit()?.into_captured());
        }
        match document::decode(source, FileType::Project, Some(*reads), closures)? {
            Decoded::Versioned(document) => Ok(document.admit()?.into_captured()),
            Decoded::Legacy { source, .. } => schemas.project.prepare_structural(source),
        }
    }
    fn document(document: &CapturedDocument) -> &yamaa_core::schema::Document {
        &document.normalized().document
    }
    fn source(document: &CapturedDocument) -> &Source {
        document.source()
    }
}

/// Decode through the captured schema codecs; the engine owns ordering and
/// admission. A 1.0 project is read only by schemas that read 1.x files
/// ([`ProjectSchemas::reading`]), as public calls do (#3030 D76).
pub fn prepare<P: ProjectCapturePort>(
    schemas: ProjectSchemas,
    source: Source,
    host: Language,
    port: &mut P,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    adapt_preparation(application::prepare(
        &mut Decoder(schemas),
        source,
        Some(host),
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
