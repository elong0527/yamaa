//! The one document decoder (#2845 PR 18).
//!
//! It decodes a document's YAML once and reads the header its root declares
//! before any schema closure is chosen (SCHEMA-2). A root that declares
//! `yamaa` and `type` is a 1.x document: the declared type selects its 1.0
//! closure, and the document is admitted at the version it declares; a
//! specification's layers are each admitted at their own version, the chain
//! needs the newest (P-VER-1), and the layers compose into one resolved
//! specification, each layer's `clear` applied before it (LAYER-9). A root
//! that declares `schema_version` is written in language version 0.5, which
//! the engine does not read: it is refused as `unsupported_language_version`.
//!
//! The engine reads language version 1.0 (#3030 D76, P1-9): every public call
//! decodes the files it reads here, and a specification is prepared by
//! [`VersionedDocument::prepare_specification`].
use crate::shipped_schema;
use crate::specification_source::{
    CapturedFindings, CapturedParent, CapturedSchema, Error, Source, SourceFailure,
};
use crate::yaml_decode::{decode_yaml, DecodedYaml, SourcePosition};
use std::sync::Arc;
use yamaa_core::ast::{FileId, FileRef, Span};
use yamaa_core::front_end::v1::spec::{self as front_end, Lowered, Positions};
use yamaa_core::inheritance::{self as graph, VersionedChain};
use yamaa_core::inheritance_preparation::{self as lifecycle, ResolvedVersioned, VersionedError};
use yamaa_core::language::{
    self, FileType, Header, LanguageVersion, PathBase, Reference, ReferenceFault,
};
use yamaa_core::resolver::{self, File, Name, Resolver};
use yamaa_core::schema::{
    Document, LayerNodeOrigin, LayerProvenance, NormalizationError, NormalizedDocument,
    SchemaDiagnostic, SchemaSource,
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
    /// The closures the schema files in `yaml/` define, embedded in the
    /// package.
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
/// `reads`; public calls read [`yamaa_core::language::LANGUAGE_VERSION`]
/// (#3030 D76). The header is read before any
/// closure is chosen: a version the engine does not read fails as
/// `unsupported_language_version`, 0.5 at `schema_version` and any other at
/// `yamaa`, and another type as `unexpected_file_type` at `type`.
pub fn decode(
    source: Source,
    expected: FileType,
    reads: Option<LanguageVersion>,
    closures: &Closures,
) -> Result<VersionedDocument, Error> {
    let raw = decode_yaml(&source.bytes).map_err(|error| Error::Decode {
        identity: source.identity.clone(),
        error,
    })?;
    match language::read_header(&raw.document, reads, expected) {
        Ok(header) => Ok(VersionedDocument {
            schema: Arc::clone(closures.get(header.file_type)),
            reads: reads.expect("a read header names a version the engine reads"),
            source,
            raw,
            header,
        }),
        Err(fault) => Err(findings(
            closures.get(expected),
            source,
            raw.document,
            vec![fault.finding()],
        )),
    }
}

/// Decode one file a referencing field names, expecting a file of type
/// `expected`, by an engine whose newest language version is `reads`: a
/// function file or codelist file a 1.0 project lists (SCHEMA-2). The file
/// must declare the 1.x header. A missing or malformed header is a finding in
/// the file, at its own field, as [`decode`] reports it; a version the engine
/// does not read, or another type, is refused at the field instead
/// ([`Error::Reference`]), which the reader of the field names with
/// `yamaa_core::language::Reference::refuse`.
pub fn decode_referenced(
    source: Source,
    expected: FileType,
    reads: LanguageVersion,
    closures: &Closures,
) -> Result<VersionedDocument, Error> {
    let raw = decode_yaml(&source.bytes).map_err(|error| Error::Decode {
        identity: source.identity.clone(),
        error,
    })?;
    match language::read_header(&raw.document, Some(reads), expected) {
        Ok(header) => Ok(VersionedDocument {
            schema: Arc::clone(closures.get(header.file_type)),
            reads,
            source,
            raw,
            header,
        }),
        Err(language::HeaderFault::Shape(finding)) => Err(findings(
            closures.get(expected),
            source,
            raw.document,
            vec![finding],
        )),
        Err(fault) => Err(Error::Reference(Box::new(
            crate::specification_source::RefusedHeader::new(source, fault),
        ))),
    }
}

/// Read the type `source` declares, expecting none, by an engine whose newest
/// language version is `reads`: what the command line dispatches on before
/// any call reads the file (#3030 D122). The document is decoded once and
/// its header read as [`language::read_declared_header`] reads it. A
/// document that declares `schema_version` declares no type, and a refused
/// header is a finding in the document, at its own field, as [`decode`]
/// reports one.
pub fn file_type(source: Source, reads: LanguageVersion) -> Result<FileType, Error> {
    let raw = decode_yaml(&source.bytes).map_err(|error| Error::Decode {
        identity: source.identity.clone(),
        error,
    })?;
    match language::read_declared_header(&raw.document, reads) {
        Ok(header) => Ok(header.file_type),
        Err(fault) => {
            // A header finding names no descriptor, so any closure resolves
            // it: the one the declared type selects, or a specification's.
            let closure = crate::shipped_schema::capture_versioned(
                language::declared_type(&raw.document).unwrap_or(FileType::Spec),
            )?;
            Err(findings(
                &closure,
                source,
                raw.document,
                vec![fault.finding()],
            ))
        }
    }
}

pub(crate) fn findings(
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
    /// The document held as the class its declared type selects.
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
/// version the chain needs, and the resolved specification they compose
/// into, pruned and ordered (LAYER-1, LAYER-14), with the provenance of every
/// value.
#[derive(Debug)]
pub struct PreparedSpecification {
    schema: Arc<CapturedSchema>,
    chain: VersionedChain,
    resolved: ResolvedVersioned,
    positions: Positions,
    end_positions: Vec<Option<Span>>,
    lowered: Lowered,
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
    /// applied before it, pruned, ordered and normalized at the chain's
    /// version, which `yamaa` records, with every named window expanded
    /// (LAYER-9, LAYER-14).
    pub fn resolved(&self) -> &Document {
        &self.resolved.document
    }
    /// The resolved specification lowered into the AST by the 1.0 front end
    /// (P1-5). Every node carries the position of the deepest node its layer
    /// wrote, in the layer whose value took effect (P-ISSUE-1).
    pub fn lowered(&self) -> &Lowered {
        &self.lowered
    }
    /// The lowered specification, owned.
    pub fn into_lowered(self) -> Lowered {
        self.lowered
    }
    /// Parallel to [`PreparedSpecification::resolved`]'s nodes: where the
    /// layer that wrote each node, numbered by [`Span::file`] in contribution
    /// order, wrote it, at the line and column its reader decoded, or none
    /// for a schema default or a node composition made (P-ISSUE-1).
    pub fn positions(&self) -> &Positions {
        &self.positions
    }
    /// The exclusive scanner end of a resolved node a layer wrote.
    pub fn end_position(&self, node: usize) -> Option<Span> {
        self.end_positions.get(node).copied().flatten()
    }
    /// Lower `materialized`, the resolved specification with each dictionary
    /// file's admitted content in place of its path, in place of the lowering
    /// of the resolved specification: binding reads the dictionary's entries
    /// as it reads an inline one. Each node keeps the position of the node it
    /// copies, and a dictionary's content the position of its `dict` path.
    pub fn lower_materialized(&mut self, materialized: &Document, sources: &[usize]) {
        let positions = Positions {
            files: self.positions.files.clone(),
            nodes: sources
                .iter()
                .map(|&node| self.positions.nodes.get(node).copied().flatten())
                .collect(),
        };
        self.lowered = front_end::lower(materialized, Some(&positions));
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
    /// A copy entry names a dataset that is not an input linked through
    /// `schema`, or a column its producer does not output: findings in the
    /// layer that writes it (SPEC-9).
    Copy {
        source: graph::Source,
        document: Document,
        findings: Vec<SchemaDiagnostic>,
    },
    /// The producing specification a copy entry copies from could not be
    /// read (SPEC-9).
    Producer(ProducerFailure<E>),
    /// A parent the port refused as written, or a layer it would not respell,
    /// with the finding it classified, for a project file's layers
    /// (PROJECT-16).
    Finding(Box<yamaa_core::diagnostic::Diagnostic>),
}

/// Why the producing specification a copy entry copies from was not read
/// (SPEC-9), as a `schema` link reports it (INPUT-10).
#[derive(Debug)]
pub enum ProducerFailure<E> {
    /// Findings at the link, `input.<dataset>.schema`: a file the port
    /// classified, an invalid output contract, a link to the consuming
    /// specification, or copies that lead back to a specification being
    /// resolved.
    Findings(Vec<yamaa_core::diagnostic::Diagnostic>),
    /// The producer did not prepare: its own findings.
    Preparation(Box<crate::specification_source::InheritanceError<E>>),
    /// The port failed with an opaque host error.
    Host(E),
}

/// A 1.x specification prepared with the files it was read from: the entry
/// as captured and decoded, and each parent layer and included fragment in
/// first-capture order.
#[derive(Debug)]
pub struct PreparedSources {
    pub specification: PreparedSpecification,
    pub source: Source,
    pub raw: DecodedYaml,
    pub parents: Vec<CapturedParent>,
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

    /// Admit one layer of a project file's chain, a project file that names a
    /// `parent`, against the view of its closure at the version it declares,
    /// as a specification's layers are admitted: a layer need not be
    /// complete, and inside a section requiredness and defaults wait for the
    /// composed project (LAYER-4, PROJECT-16).
    pub fn admit_layer(self) -> Result<AdmittedDocument, Error> {
        let view = self.schema.structure().at_version(self.header.version);
        match view.validate_layer(&self.raw.document) {
            Ok(found) if found.is_empty() => {}
            Ok(found) => {
                return Err(findings(
                    &self.schema,
                    self.source,
                    self.raw.document,
                    found,
                ))
            }
            Err(error) => return Err(Error::Validation(error)),
        }
        let normalized = view
            .normalize_versioned_layer(&self.raw.document)
            .map_err(Error::Normalize)?;
        Ok(AdmittedDocument {
            source: self.source,
            raw: self.raw,
            header: self.header,
            schema: self.schema,
            normalized,
        })
    }

    /// Prepare a 1.x specification. Its layers are followed from this entry,
    /// each admitted at the version it declares before its parent or its
    /// fragments are read, and every layer must declare `type: spec`
    /// (LAYER-3, LAYER-4); the chain needs the newest version a layer
    /// declares (P-VER-1). A specification of one layer must also be complete
    /// at its version.
    pub fn prepare_specification<P: Resolver>(
        self,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedSpecification, SpecificationError<P::Error>> {
        self.prepare_sources(display_path, port)
            .map(|prepared| prepared.specification)
    }

    /// [`VersionedDocument::prepare_specification`], keeping the entry as
    /// captured and decoded and each parent layer and fragment the traversal
    /// read.
    pub fn prepare_sources<P: Resolver>(
        self,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedSources, SpecificationError<P::Error>> {
        self.prepare_sources_within(display_path, port, &[], None)
    }

    /// [`VersionedDocument::prepare_sources`] for a producer that a copy
    /// entry copies from while the specifications and layers `resolving`
    /// names, entry first, are being resolved: a copy that leads back to one
    /// of them fails (SPEC-9). With a `parent` the caller names, this entry
    /// is a fragment checked on that file as its parent (#3694).
    pub(crate) fn prepare_sources_within<P: Resolver>(
        self,
        display_path: String,
        port: &mut P,
        resolving: &[String],
        parent: Option<graph::CallerParent<'_>>,
    ) -> Result<PreparedSources, SpecificationError<P::Error>> {
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
        let mut resolving = resolving.to_vec();
        resolving.push(self.source.identity.clone());
        let graph::Traversal { chain, parents } = graph::traverse_versioned(
            schema,
            self.reads,
            entry,
            self.raw.document.clone(),
            parent,
            port,
            &mut crate::decoder::Decoder::<std::convert::Infallible>::default(),
        )
        .map_err(SpecificationError::Traversal)?;
        // Why the last producer that could not be read was not read.
        let mut producer = None;
        let prepared = lifecycle::prepare_versioned(
            schema,
            &chain,
            port,
            |port, declaring, written, dataset, layers| {
                read_producer(port, &resolving, declaring, written, dataset, layers)
                    .map_err(|failure| producer = Some(failure))
                    .ok()
            },
        );
        let resolved = match prepared {
            Ok(resolved) => resolved,
            Err(VersionedError::Clear { layer, findings }) => {
                let layer = &chain.layers[layer];
                return Err(SpecificationError::Clear {
                    source: layer.source.clone(),
                    document: layer.document.clone(),
                    findings,
                });
            }
            Err(VersionedError::Copy { layer, findings }) => {
                let layer = &chain.layers[layer];
                return Err(SpecificationError::Copy {
                    source: layer.source.clone(),
                    document: layer.document.clone(),
                    findings,
                });
            }
            Err(VersionedError::Producer { .. }) => {
                let failure = producer
                    .take()
                    .expect("a producer that could not be read keeps why");
                return Err(SpecificationError::Producer(failure));
            }
            Err(VersionedError::Path(error)) => {
                return Err(SpecificationError::Path(SourceFailure::Host(error)))
            }
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
            // An unknown dependency or a cycle of the live columns
            // (LAYER-11, LAYER-13).
            Err(VersionedError::Prune(error)) => {
                return Err(SpecificationError::Entry(Error::Dependencies(*error)))
            }
        };
        // Lower once, each node located where its layer wrote it, and check
        // what the 1.0 schema cannot state before any input is read.
        let locations = |layer: &graph::VersionedLayer| {
            layer_locations(&self.source, &self.raw, &parents, layer)
        };
        let (positions, end_positions) = positions(&chain, &resolved, locations);
        let lowered = front_end::lower(&resolved.document, Some(&positions));
        let found = front_end::checks::findings(lowered.spec());
        if !found.is_empty() {
            return Err(SpecificationError::Entry(findings(
                &self.schema,
                self.source,
                resolved.document,
                found,
            )));
        }
        Ok(PreparedSources {
            specification: PreparedSpecification {
                schema: self.schema,
                chain,
                resolved,
                positions,
                end_positions,
                lowered,
            },
            source: self.source,
            raw: self.raw,
            parents,
        })
    }
}

/// The layers of `chain`, read through `port`, as preparation composes them
/// ([`lifecycle::contributed_layers`]): each copy entry replaced by the
/// columns it declares, read from its producer as preparation reads it
/// (SPEC-9), and each relative path respelled from the entry (LAYER-10).
/// None when a layer does not normalize or a producer cannot be read:
/// preparation then reports why.
#[cfg(feature = "yaml")]
pub(crate) fn contributed_layers<P: Resolver>(
    schema: &yamaa_core::schema::SchemaStructure,
    chain: &VersionedChain,
    port: &mut P,
) -> Option<Vec<NormalizedDocument>> {
    let resolving = [chain.layers.last()?.source.identity.clone()];
    lifecycle::contributed_layers(
        schema,
        chain,
        port,
        |port, declaring, written, dataset, layers| {
            read_producer(port, &resolving, declaring, written, dataset, layers).ok()
        },
    )
    .ok()
}

/// Where `layer` was decoded: `raw`, the entry read as `source`, or the
/// parent or fragment of `parents` read under its identity.
fn layer_locations<'a>(
    source: &Source,
    raw: &'a DecodedYaml,
    parents: &'a [CapturedParent],
    layer: &graph::VersionedLayer,
) -> Option<&'a DecodedYaml> {
    if layer.source.identity == source.identity {
        Some(raw)
    } else {
        parents
            .iter()
            .find(|parent| parent.source().identity == layer.source.identity)
            .map(CapturedParent::raw)
    }
}

/// Where each resolved node was written: the decoded position of the node
/// its layer wrote, or none for a schema default or a node composition made.
/// `locations` gives each layer's decoded positions.
fn positions<'a, 'b>(
    chain: &'b VersionedChain,
    resolved: &ResolvedVersioned,
    locations: impl Fn(&'b graph::VersionedLayer) -> Option<&'a DecodedYaml>,
) -> (Positions, Vec<Option<Span>>) {
    let decoded = chain.layers.iter().map(locations).collect::<Vec<_>>();
    let span = |layer: usize, at: &SourcePosition| Span {
        file: FileId(u32::try_from(layer).expect("a chain's layers fit u32")),
        line: u32::try_from(at.line).unwrap_or(u32::MAX),
        column: u32::try_from(at.column).unwrap_or(u32::MAX),
    };
    let (nodes, end_positions) = resolved
        .origins
        .iter()
        .map(|origin| {
            let written = || {
                let layer = origin.layer?;
                if !matches!(origin.origin.source, SchemaSource::Input) {
                    return None;
                }
                let raw = decoded.get(layer)?.as_ref()?;
                let node = origin.origin.node;
                Some((
                    span(layer, raw.locations.get(node)?),
                    span(layer, raw.end_locations.get(node)?),
                ))
            };
            match written() {
                Some((start, end)) => (Some(start), Some(end)),
                None => (None, None),
            }
        })
        .unzip();
    (
        Positions {
            files: chain
                .layers
                .iter()
                .map(|layer| FileRef {
                    identity: layer.source.identity.clone(),
                })
                .collect(),
            nodes,
        },
        end_positions,
    )
}

/// The output contract of the producer that input `dataset`'s link,
/// `written` in `declaring`, names, read through `port` and resolved as a
/// specification (INPUT-10, D115). `resolving` names the specifications
/// whose copies are being resolved, entry first, ending with this one, and
/// `layers` are this specification's layers that count as being resolved
/// while the producer is read, entry first (SPEC-9). A producer that copies
/// from its own producers reads them in turn; a link back to a specification
/// or layer being resolved fails, as a link to the consuming specification
/// when it names one of `layers`.
fn read_producer<P: Resolver>(
    port: &mut P,
    resolving: &[String],
    declaring: &graph::Source,
    written: &str,
    dataset: &str,
    layers: &[&graph::Source],
) -> Result<yamaa_core::producer_contract::Contract, ProducerFailure<P::Error>> {
    use yamaa_core::producer_contract::{self as contract, Error as Invalid};
    let field = format!("input.{dataset}.schema");
    let failed = |port: &P, name: Name<'_>, error: P::Error| match port
        .failure(name, &error)
        .and_then(resolver::Failure::resource)
    {
        Some(failure) => ProducerFailure::Findings(vec![failure.field_diagnostic(&field, written)]),
        None => ProducerFailure::Host(error),
    };
    let name = Name::Written {
        file: File::Resource,
        declaring: &declaring.identity,
        written,
    };
    let source = port
        .inspect(name)
        .map_err(|error| failed(port, name, error))?;
    // This specification's layers that count as being resolved begin
    // with its entry, which they replace (SPEC-9).
    let own = resolving.len() - 1;
    debug_assert_eq!(
        layers.first().map(|layer| &layer.identity),
        resolving.last()
    );
    let resolving = resolving[..own]
        .iter()
        .cloned()
        .chain(layers.iter().map(|layer| layer.identity.clone()))
        .collect::<Vec<_>>();
    if let Some(entered) = resolving.iter().position(|id| *id == source.identity) {
        return Err(ProducerFailure::Findings(vec![
            if resolving[own..].contains(&source.identity) {
                contract::self_reference(dataset)
            } else {
                let mut cycle = resolving[entered..]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                cycle.push(&source.identity);
                yamaa_core::copy_expansion::producer_cycle(dataset, &cycle)
            },
        ]));
    }
    let name = Name::Located {
        file: File::Resource,
        source: &source,
    };
    let bytes = port
        .capture(name)
        .map(resolver::bytes)
        .map_err(|error| failed(port, name, error))?;
    let captured = Source {
        identity: source.identity,
        bytes,
    };
    let prepared = crate::specification_source::prepare_versioned_within(
        captured,
        written.into(),
        &mut *port,
        &resolving,
        None,
    )
    .map_err(|error| ProducerFailure::Preparation(Box::new(error)))?;
    contract::prepare_document(prepared.document()).map_err(|error| match error {
        Invalid::Invalid(causes) => ProducerFailure::Findings(
            causes
                .iter()
                .map(|cause| cause.diagnostic(dataset))
                .collect(),
        ),
        Invalid::Limit(limit) => ProducerFailure::Preparation(Box::new(
            crate::specification_source::InheritanceError::Entry(Error::Limit(limit)),
        )),
        Invalid::Boundary => ProducerFailure::Preparation(Box::new(
            crate::specification_source::InheritanceError::Entry(Error::Validation(
                yamaa_core::schema::ValidationError::InvalidDescriptor,
            )),
        )),
    })
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
pub fn check_references<P: Resolver>(
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
        let name = Name::Written {
            file: File::Resource,
            declaring: base,
            written: &reference.written,
        };
        let source = match port.inspect(name) {
            Ok(source) => source,
            Err(error) => return Err(failed(resolver::source_error(port, name, error), reference)),
        };
        let name = Name::Located {
            file: File::Resource,
            source: &source,
        };
        let bytes = match port.capture(name) {
            Ok(captured) => resolver::bytes(captured),
            Err(error) => return Err(failed(resolver::source_error(port, name, error), reference)),
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
