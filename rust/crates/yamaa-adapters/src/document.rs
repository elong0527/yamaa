//! The one document decoder (#2845 PR 18).
//!
//! It decodes a document's YAML once and reads the header its root declares
//! before any schema closure is chosen (SCHEMA-2). A root that declares
//! `schema_version`, or no `yamaa`, keeps the 0.5 path, which reads it under
//! the rules at the `yamaa-v0.5.0` tag (#3030 D75). A root that declares
//! `yamaa` and `type` is a 1.x document: the declared type selects its 1.0
//! closure, and the document is admitted at the version it declares; a
//! specification's layers are each admitted at their own version, the chain
//! needs the newest (P-VER-1), and the layers compose into one resolved
//! specification, each layer's `clear` applied before it (LAYER-9).
//!
//! Until the engine reads 1.0 files (#3030 D76, P1-9) public calls read no 1.x
//! version: every 0.5 read refuses a 1.x header through the same header reader
//! (`yamaa_core::language::refusal`), so only tests reach the 1.0 path here.
//! Rewrite 3 routes every document read through this decoder and retires the
//! decode ports each read site has today.
use crate::shipped_schema;
use crate::specification_source::{
    CapturedFindings, CapturedSchema, Error, InheritancePort, Source, SourceFailure,
};
use crate::yaml_decode::{decode_yaml, DecodedYaml};
use std::sync::Arc;
use yamaa_core::inheritance::{self as graph, VersionedChain};
use yamaa_core::inheritance_preparation::{self as lifecycle, ResolvedVersioned, VersionedError};
use yamaa_core::language::{
    self, FileType, Header, LanguageVersion, PathBase, Reference, ReferenceFault,
};
use yamaa_core::schema::{
    Document, LayerNodeOrigin, LayerProvenance, NormalizationError, NormalizedDocument,
    SchemaDiagnostic,
};

/// The 1.0 closure of each file type.
#[derive(Clone, Debug)]
pub struct Closures {
    spec: Arc<CapturedSchema>,
    project: Arc<CapturedSchema>,
    codelists: Arc<CapturedSchema>,
    function: Arc<CapturedSchema>,
}

impl Closures {
    /// The closures `yaml/v1/` defines, embedded in the package.
    pub fn shipped() -> Result<Self, Error> {
        Ok(Self {
            spec: shipped_schema::capture_versioned(FileType::Spec)?,
            project: shipped_schema::capture_versioned(FileType::Project)?,
            codelists: shipped_schema::capture_versioned(FileType::Codelists)?,
            function: shipped_schema::capture_versioned(FileType::Function)?,
        })
    }
    /// These closures with `closure` for `file_type`, as a test of a later
    /// language version supplies.
    pub fn with(mut self, file_type: FileType, closure: Arc<CapturedSchema>) -> Self {
        *self.slot(file_type) = closure;
        self
    }
    pub fn get(&self, file_type: FileType) -> &Arc<CapturedSchema> {
        match file_type {
            FileType::Spec => &self.spec,
            FileType::Project => &self.project,
            FileType::Codelists => &self.codelists,
            FileType::Function => &self.function,
        }
    }
    fn slot(&mut self, file_type: FileType) -> &mut Arc<CapturedSchema> {
        match file_type {
            FileType::Spec => &mut self.spec,
            FileType::Project => &mut self.project,
            FileType::Codelists => &mut self.codelists,
            FileType::Function => &mut self.function,
        }
    }
}

/// One decoded document and the route its root selects.
#[derive(Debug)]
pub enum Decoded {
    /// A root that declares `schema_version`, or no `yamaa`: the 0.5 path
    /// reads it, with the closure its call chooses.
    Legacy { source: Source, raw: DecodedYaml },
    /// A 1.x document whose header the engine reads.
    Versioned(VersionedDocument),
}

/// A 1.x document, its header read and the 1.0 closure of its type chosen.
#[derive(Debug)]
pub struct VersionedDocument {
    source: Source,
    raw: DecodedYaml,
    header: Header,
    reads: LanguageVersion,
    schema: Arc<CapturedSchema>,
}

/// Decode one document that a call or a referencing field reads, expecting a
/// file of type `expected`, by an engine whose newest language version is
/// `reads`; public calls read none until P1-9. The header is read before any
/// closure is chosen: a version the engine does not read fails as
/// `unsupported_language_version`, and another type as
/// `unexpected_file_type`, at the document's own `yamaa` or `type`.
pub fn decode(
    source: Source,
    expected: FileType,
    reads: Option<LanguageVersion>,
    closures: &Closures,
) -> Result<Decoded, Error> {
    let raw = decode_yaml(&source.bytes).map_err(|error| Error::Decode {
        identity: source.identity.clone(),
        error,
    })?;
    if !language::declares_header(&raw.document) {
        return Ok(Decoded::Legacy { source, raw });
    }
    match language::read_header(&raw.document, reads, expected) {
        Ok(header) => Ok(Decoded::Versioned(VersionedDocument {
            schema: Arc::clone(closures.get(header.file_type)),
            reads: reads.expect("a read header names a version the engine reads"),
            source,
            raw,
            header,
        })),
        Err(fault) => Err(findings(
            closures.get(expected),
            source,
            raw.document,
            vec![fault.finding()],
        )),
    }
}

fn findings(
    schema: &Arc<CapturedSchema>,
    source: Source,
    document: Document,
    findings: Vec<SchemaDiagnostic>,
) -> Error {
    Error::Findings(Box::new(CapturedFindings::new(
        Arc::clone(schema),
        source,
        Some(document),
        findings,
    )))
}

/// A complete 1.x document admitted at the version it declares.
#[derive(Debug)]
pub struct AdmittedDocument {
    source: Source,
    raw: DecodedYaml,
    header: Header,
    schema: Arc<CapturedSchema>,
    normalized: NormalizedDocument,
}

impl AdmittedDocument {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    pub fn header(&self) -> Header {
        self.header
    }
    pub fn schema(&self) -> &Arc<CapturedSchema> {
        &self.schema
    }
    pub fn normalized(&self) -> &NormalizedDocument {
        &self.normalized
    }
    /// The document held as the class its declared type selects, as a
    /// structurally prepared 0.5 root is held.
    pub fn into_captured(self) -> crate::specification_source::CapturedDocument {
        crate::specification_source::CapturedDocument::admitted(
            self.schema,
            self.header.file_type.root_class(),
            self.source,
            self.raw,
            self.normalized,
        )
    }
}

/// A 1.x specification: its layers, each admitted at its own version, the
/// version the chain needs, and the resolved specification they compose into
/// (LAYER-1), with the provenance of every value. Pruning and dependency
/// order read the lowered specification and are not part of this
/// preparation.
#[derive(Debug)]
pub struct PreparedSpecification {
    schema: Arc<CapturedSchema>,
    chain: VersionedChain,
    resolved: ResolvedVersioned,
}

impl PreparedSpecification {
    pub fn schema(&self) -> &Arc<CapturedSchema> {
        &self.schema
    }
    /// The layers in contribution order, the root ancestor first.
    pub fn layers(&self) -> &[graph::VersionedLayer] {
        &self.chain.layers
    }
    /// The newest version any layer declares, which the chain needs (P-VER-1).
    pub fn version(&self) -> LanguageVersion {
        self.chain.version
    }
    /// The resolved specification: the layers composed, each layer's `clear`
    /// applied before it, and normalized at the chain's version, which `yamaa`
    /// records (LAYER-9, LAYER-14).
    pub fn resolved(&self) -> &Document {
        &self.resolved.document
    }
    /// Parallel to [`PreparedSpecification::resolved`]'s nodes: the layer and
    /// occurrence each node was written at, or the schema default it came
    /// from.
    pub fn origins(&self) -> &[LayerNodeOrigin] {
        &self.resolved.origins
    }
    /// The layer, numbered in contribution order, that contributed each
    /// written path, keyed members named by identity.
    pub fn provenance(&self) -> &[LayerProvenance] {
        &self.resolved.provenance
    }
    /// The identity of the layer that wrote the value at `path`, a finding's
    /// spec path, or the nearest value holding it.
    pub fn declaring_source(&self, path: &str) -> Option<&str> {
        Some(&self.declaring_layer(path)?.source.identity)
    }
    /// The layer that wrote the value at `path`, or the nearest value
    /// holding it, as that layer was written.
    pub fn declaring_layer(&self, path: &str) -> Option<&graph::VersionedLayer> {
        self.chain
            .layers
            .get(declaring_layer(&self.resolved.provenance, path)?)
    }
}

/// The layer that contributed `path` or the nearest written path holding it.
fn declaring_layer(provenance: &[LayerProvenance], path: &str) -> Option<usize> {
    provenance
        .iter()
        .filter(|origin| {
            path == origin.path
                || path
                    .strip_prefix(&origin.path)
                    .is_some_and(|suffix| suffix.starts_with(['.', '[']))
        })
        .max_by_key(|origin| origin.path.len())
        .map(|origin| origin.layer)
}

/// Why a 1.x specification is refused.
#[derive(Debug)]
pub enum SpecificationError<E> {
    /// The file is not a specification, or the specification its layers
    /// compose into is not valid at the chain's version: findings in the
    /// resolved specification, whose provenance names each layer.
    Entry(Error),
    /// A layer fails admission, or its parent cannot be read (LAYER-2, LAYER-4).
    Traversal(graph::Error<SourceFailure<E>>),
    /// A layer lists in `clear` a value it cannot clear: findings in that
    /// layer, each `invalid_clear` at its `clear[i]` (LAYER-9).
    Clear {
        source: graph::Source,
        document: Document,
        findings: Vec<SchemaDiagnostic>,
    },
    /// The host could not respell a layer's path from the entry (LAYER-10).
    Path(SourceFailure<E>),
}

impl VersionedDocument {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    pub fn header(&self) -> Header {
        self.header
    }
    /// The 1.0 closure the declared type selected.
    pub fn schema(&self) -> &Arc<CapturedSchema> {
        &self.schema
    }

    /// Admit a complete document against the view of its closure at the
    /// version it declares: a field newer than that version fails as
    /// `unknown_field`, with context naming the version that introduced it
    /// (SCHEMA-2, P-VER-1). A specification that names a parent is a layer;
    /// [`VersionedDocument::prepare_specification`] admits it.
    pub fn admit(self) -> Result<AdmittedDocument, Error> {
        let view = self.schema.structure().at_version(self.header.version);
        match view.normalize_root(&self.raw.document) {
            Ok(normalized) => Ok(AdmittedDocument {
                source: self.source,
                raw: self.raw,
                header: self.header,
                schema: self.schema,
                normalized,
            }),
            Err(NormalizationError::Invalid(found)) => Err(findings(
                &self.schema,
                self.source,
                self.raw.document,
                found,
            )),
            Err(error) => Err(Error::Normalize(error)),
        }
    }

    /// Prepare a 1.x specification. Its layers are followed from this entry,
    /// each admitted at the version it declares before its parent is read,
    /// and every layer must declare `type: spec` (LAYER-4); the chain needs
    /// the newest version a layer declares (P-VER-1). A specification of one
    /// layer must also be complete at its version.
    pub fn prepare_specification<P: InheritancePort>(
        self,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedSpecification, SpecificationError<P::Error>> {
        if self.header.file_type != FileType::Spec {
            let finding =
                language::read_header(&self.raw.document, Some(self.reads), FileType::Spec)
                    .expect_err("another declared type")
                    .finding();
            return Err(SpecificationError::Entry(findings(
                &self.schema,
                self.source,
                self.raw.document,
                vec![finding],
            )));
        }
        let entry = graph::Source {
            identity: self.source.identity.clone(),
            display_path,
        };
        let schema = self.schema.structure();
        let mut parents = Parents { port };
        let chain = graph::traverse_versioned(
            schema,
            self.reads,
            entry,
            self.raw.document.clone(),
            &mut parents,
        )
        .map_err(SpecificationError::Traversal)?;
        let resolved = match lifecycle::prepare_versioned(schema, &chain, &mut parents) {
            Ok(resolved) => resolved,
            Err(VersionedError::Clear { layer, findings }) => {
                let layer = &chain.layers[layer];
                return Err(SpecificationError::Clear {
                    source: layer.source.clone(),
                    document: layer.document.clone(),
                    findings,
                });
            }
            Err(VersionedError::Path(error)) => return Err(SpecificationError::Path(error)),
            Err(VersionedError::Normalize(context)) => {
                let lifecycle::FailureContext { error, input, .. } = *context;
                let error = match (error, input) {
                    (NormalizationError::Invalid(found), Some(input)) => {
                        findings(&self.schema, self.source, input, found)
                    }
                    (error, _) => Error::Normalize(error),
                };
                return Err(SpecificationError::Entry(error));
            }
            Err(VersionedError::Layer { error, .. }) => {
                return Err(SpecificationError::Entry(Error::Normalize(error)))
            }
            Err(VersionedError::Document(error)) => {
                return Err(SpecificationError::Entry(Error::Normalize(
                    NormalizationError::Document(error),
                )))
            }
        };
        Ok(PreparedSpecification {
            schema: self.schema,
            chain,
            resolved,
        })
    }
}

/// Parent capture through the caller's port, decoded with the shared YAML
/// reader. Opaque host failures keep their payload.
struct Parents<'a, P> {
    port: &'a mut P,
}

fn host<E>(error: graph::SourceError<E>) -> graph::SourceError<SourceFailure<E>> {
    match error {
        graph::SourceError::Unavailable => graph::SourceError::Unavailable,
        graph::SourceError::Raised(error) => graph::SourceError::Raised(SourceFailure::Host(error)),
    }
}

impl<P: InheritancePort> lifecycle::PathPort for Parents<'_, P> {
    fn rebase(
        &mut self,
        layer: &graph::Source,
        entry: &graph::Source,
        written: &str,
    ) -> Result<String, Self::Error> {
        self.port
            .rebase(layer, entry, written)
            .map_err(SourceFailure::Host)
    }
}

impl<P: InheritancePort> graph::InheritanceSourcePort for Parents<'_, P> {
    type Error = SourceFailure<P::Error>;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
        self.port.canonicalize(declaring, written).map_err(host)
    }
    fn read(
        &mut self,
        source: &graph::Source,
    ) -> Result<Document, graph::SourceError<Self::Error>> {
        let bytes = self.port.capture(source).map_err(host)?;
        decode_yaml(&bytes)
            .map(|decoded| decoded.document)
            .map_err(|error| {
                graph::SourceError::Raised(SourceFailure::Admission(Error::Decode {
                    identity: source.identity.clone(),
                    error,
                }))
            })
    }
}

/// A file a referencing field names, and the header it declares.
#[derive(Debug, PartialEq)]
pub struct CheckedReference {
    pub reference: Reference,
    pub source: graph::Source,
    pub header: Header,
}

/// Why the file a referencing field names is refused.
#[derive(Debug)]
pub enum ReferenceError<E> {
    /// The named file cannot be resolved or read.
    Unavailable { reference: Reference },
    /// The port failed with an opaque host error.
    Host { reference: Reference, error: E },
    /// The named file is not valid YAML.
    Decode {
        reference: Reference,
        error: Box<Error>,
    },
    /// The named file's header is refused (SCHEMA-2).
    Refused(Box<RefusedReference>),
}

/// A named file whose header is refused: a finding in that file, or a
/// diagnostic at the referencing field.
#[derive(Debug)]
pub struct RefusedReference {
    pub reference: Reference,
    pub source: graph::Source,
    pub document: Document,
    pub fault: ReferenceFault,
}

/// Check the header of each file a 1.x document's referencing fields name,
/// in the order the fields are written, by an engine whose newest version is
/// `reads` (SCHEMA-2): an input's `schema` names a specification, and a
/// project's `functions`, `codelists` and section `specs` name function
/// files, codelist files and specifications. Each file is resolved from the
/// base its field selects (FILE-2): `declaring`, the identity of the
/// document that writes the field, or, for an input with
/// `relative_to: entry`, `entry`, the identity of the specification's entry
/// file, which is `declaring` itself for an entry or a project file. Each is
/// read through `port`; the first refused file stops the check.
pub fn check_references<P: InheritancePort>(
    document: &Document,
    file_type: FileType,
    declaring: &str,
    entry: &str,
    reads: LanguageVersion,
    port: &mut P,
) -> Result<Vec<CheckedReference>, ReferenceError<P::Error>> {
    let mut checked = Vec::new();
    for reference in language::references(document, file_type) {
        let failed = |error: graph::SourceError<P::Error>, reference: Reference| match error {
            graph::SourceError::Unavailable => ReferenceError::Unavailable { reference },
            graph::SourceError::Raised(error) => ReferenceError::Host { reference, error },
        };
        let base = match reference.base {
            PathBase::Declaring => declaring,
            PathBase::Entry => entry,
        };
        let source = match port.canonicalize(base, &reference.written) {
            Ok(source) => source,
            Err(error) => return Err(failed(error, reference)),
        };
        let bytes = match port.capture(&source) {
            Ok(bytes) => bytes,
            Err(error) => return Err(failed(error, reference)),
        };
        let named = match decode_yaml(&bytes) {
            Ok(decoded) => decoded.document,
            Err(error) => {
                let error = Box::new(Error::Decode {
                    identity: source.identity.clone(),
                    error,
                });
                return Err(ReferenceError::Decode { reference, error });
            }
        };
        match reference.check(&source.identity, &named, reads) {
            Ok(header) => checked.push(CheckedReference {
                reference,
                source,
                header,
            }),
            Err(fault) => {
                return Err(ReferenceError::Refused(Box::new(RefusedReference {
                    reference,
                    source,
                    document: named,
                    fault,
                })))
            }
        }
    }
    Ok(checked)
}
