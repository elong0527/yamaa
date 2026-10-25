//! Captured YAML codec and compatibility results for engine project preparation.
use crate::document::{self, Closures};
use crate::specification_source::{CapturedDocument, Error, Source};
pub use application::{DecodeTarget, Kind, LockCapture, Origin, SourceOrigins};
use yamaa_core::language::{FileType, LANGUAGE_VERSION};
use yamaa_core::project_preparation as application;
use yamaa_core::{
    ast, project_environment::Environment, project_function::Language, project_function_document,
};
pub type CaptureFailure<E> = application::CaptureFailure<E, CapturedDocument, Error>;
pub type SourceFailure<E> = application::SourceFailure<E, CapturedDocument, Error>;
pub type DocumentCapture = application::DocumentCapture<CapturedDocument>;
pub type RejectedEnvironment<E> = application::RejectedEnvironment<E, CapturedDocument, Error>;

/// Where one admitted definition is declared: the function file that holds it,
/// which a path in it starts at the root of, and the project's field that
/// lists that file (FUNC-2, D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionOrigin<'a> {
    /// The identity of the function file.
    pub source: &'a str,
    /// The index in the project's `functions` of the field that lists it.
    pub listed: usize,
}
impl FunctionOrigin<'_> {
    /// The project field that lists the function file, `functions[i]`.
    pub fn declaration(&self) -> String {
        format!("functions[{}]", self.listed)
    }
}

#[derive(Debug)]
pub struct PreparedEnvironment {
    /// The project file lowered.
    pub project: ast::Project,
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
    project: Option<ast::Project>,
    origins: SourceOrigins,
    /// The project file path as preparation was given it.
    written: Option<String>,
    root: Option<CapturedDocument>,
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
    captures: Vec<DocumentCapture>,
    lock: Option<LockCapture>,
}
impl CapturedEnvironment {
    /// The project file lowered.
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
    pub fn captures(&self) -> &[DocumentCapture] {
        &self.captures
    }
    pub fn lock(&self) -> Option<&LockCapture> {
        self.lock.as_ref()
    }
    /// Where the admitted definition at `index`, in admission order, is
    /// declared: by the function file the project lists at `functions[i]`.
    /// `None` when the held origins name no such definition.
    pub fn function_origin(&self, index: usize) -> Option<FunctionOrigin<'_>> {
        let listed = *self.origins.function_files.get(index)?;
        let capture = self
            .captures
            .iter()
            .find(|capture| capture.origin == Origin::FunctionFile(listed))?;
        Some(FunctionOrigin {
            source: capture.document.source().identity.as_str(),
            listed,
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
            captures: self.captures,
            lock: self.lock,
            environment: self.environment.into_execution(),
            inventory_documents: Vec::new(),
            inventory_captured: false,
        }
    }
}
impl OwnedEnvironment {
    pub fn without_environment(host: Option<Language>) -> Self {
        Self {
            project: None,
            origins: SourceOrigins::default(),
            written: None,
            root: None,
            captures: Vec::new(),
            lock: None,
            environment: yamaa_core::build::without_environment(host),
            inventory_documents: Vec::new(),
            inventory_captured: false,
        }
    }
    pub fn inventory_captured(&self) -> bool {
        self.inventory_captured
    }
    /// The project file path as preparation was given it.
    pub fn written(&self) -> Option<&str> {
        self.written.as_deref()
    }
    /// The specifications the project lists, captured for the submission
    /// inventory in listing order.
    pub fn inventory_documents(&self) -> &[crate::specification_source::PreparedDocument] {
        &self.inventory_documents
    }
    /// Keep the project file path preparation was given, so a later refusal
    /// can name the file as written.
    pub fn with_written(mut self, written: &str) -> Self {
        self.written = Some(written.into());
        self
    }
    pub fn attach_inventory(
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
                captures: self.captures,
                lock: self.lock,
            },
        )
    }
    pub fn origins(&self) -> &SourceOrigins {
        &self.origins
    }
    /// The project file lowered.
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

/// Decode `source`, a file a project names, and admit it as `target` against
/// the 1.0 closure of each file type: an engine that reads
/// [`yamaa_core::language::LANGUAGE_VERSION`] admits the project file against
/// the project closure, at the version it declares, and each function file
/// and codelist file it lists against the closure of its type (#3030 D76).
/// Every file must declare the 1.x header: a project file that declares
/// `schema_version` is refused as `unsupported_language_version`, naming 0.5,
/// and one that declares neither `yamaa` nor `schema_version` as
/// `missing_required_field` at `yamaa`. A listed file whose version the engine
/// does not read, or whose type is another, is refused at the project's field
/// that lists it (SCHEMA-2).
pub(crate) fn admit(
    closures: &Closures,
    source: Source,
    target: DecodeTarget,
) -> Result<CapturedDocument, Error> {
    let document = match target {
        DecodeTarget::Project => {
            document::decode(source, FileType::Project, Some(LANGUAGE_VERSION), closures)?
        }
        DecodeTarget::FunctionFile => {
            document::decode_referenced(source, FileType::Function, LANGUAGE_VERSION, closures)?
        }
        DecodeTarget::CodelistFile => {
            document::decode_referenced(source, FileType::Codelists, LANGUAGE_VERSION, closures)?
        }
    };
    Ok(document.admit()?.into_captured())
}

/// Decode through the 1.0 closures `closures`; the engine owns ordering and
/// admission.
pub fn prepare<P: yamaa_core::resolver::Resolver>(
    closures: Closures,
    source: Source,
    host: Language,
    port: &mut P,
) -> Result<PreparedEnvironment, Failure<P::Error>> {
    adapt_preparation(application::prepare(
        &mut crate::decoder::Decoder::<std::convert::Infallible>::with_closures(closures),
        source,
        Some(host),
        port,
    ))
}

/// Mechanical ownership transfer from the same engine environment preparation.
pub fn adapt_preparation<E>(
    result: application::PreparationResult<E, CapturedDocument, Error>,
) -> Result<PreparedEnvironment, Failure<E>> {
    match result {
        Ok(prepared) => Ok(PreparedEnvironment {
            project: prepared.project,
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
