//! Captured raw YAML entry point. Hosts supply bytes and source identities only.
//! Filesystem authority, include discovery and publication remain explicit host ports.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodedYaml, SourcePosition};
use std::sync::Arc;
use yamaa_core::schema::{
    BundleError, NormalizationError, NormalizedDocument, SchemaDiagnostic, SchemaOrigin,
    SchemaStructure, SpecificationDocument, ValidationError, WindowReference,
};
use yamaa_core::schema_capture::{self as capture, RootSelection};
mod inheritance;
pub use inheritance::{CapturedParent, InheritanceError, InheritancePort, SourceFailure};

/// The engine-owned identity and byte snapshot shared with metadata preparation.
pub use yamaa_core::project_preparation::Source;
#[derive(Debug)]
pub enum Error {
    Limit(yamaa_core::limits::LimitExceeded),
    Decode {
        identity: String,
        error: DecodeFailure,
    },
    Bundle(BundleError),
    Normalize(NormalizationError),
    Dependencies(yamaa_core::resolution::Error),
    Validation(ValidationError),
    Findings(Box<CapturedFindings>),
    /// A file a referencing field names declares a version the engine does
    /// not read, or a type the field does not expect: refused at that field,
    /// which the reader of the field names, not in the file (SCHEMA-2).
    Reference(Box<RefusedHeader>),
    /// This entry point cannot silently treat an inherited layer as standalone.
    InheritanceRequired,
    /// A versioned root must retain its schema-version admission gate.
    VersionedRootRequired,
    /// Only admitted named classes can be selected from the captured closure.
    UnknownClass(String),
}

/// A failed semantic pass retains the exact source, schema and context arena.
/// Node references belong to the pass input, which may differ from the raw entry.
#[derive(Debug)]
pub struct CapturedFindings {
    schema: Arc<CapturedSchema>,
    source: Source,
    document: Option<yamaa_core::schema::Document>,
    findings: Vec<SchemaDiagnostic>,
}
impl CapturedFindings {
    /// Findings of one pass over `document`, read from `source`, whose context
    /// resolves against `schema` and `document`.
    pub(crate) fn new(
        schema: Arc<CapturedSchema>,
        source: Source,
        document: Option<yamaa_core::schema::Document>,
        findings: Vec<SchemaDiagnostic>,
    ) -> Self {
        Self {
            schema,
            source,
            document,
            findings,
        }
    }
    pub fn schema(&self) -> &CapturedSchema {
        &self.schema
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn document(&self) -> Option<&yamaa_core::schema::Document> {
        self.document.as_ref()
    }
    pub fn findings(&self) -> &[SchemaDiagnostic] {
        &self.findings
    }
}

/// The header of a file a referencing field names, refused at that field: it
/// declares a version the engine does not read or a type the field does not
/// expect (SCHEMA-2). The reader of the field reports it with
/// `yamaa_core::language::Reference::refuse`.
#[derive(Debug)]
pub struct RefusedHeader {
    source: Source,
    fault: yamaa_core::language::HeaderFault,
}
impl RefusedHeader {
    pub(crate) fn new(source: Source, fault: yamaa_core::language::HeaderFault) -> Self {
        Self { source, fault }
    }
    /// The named file, as captured.
    pub fn source(&self) -> &Source {
        &self.source
    }
    /// Its version or type fault.
    pub fn fault(&self) -> &yamaa_core::language::HeaderFault {
        &self.fault
    }
}

/// One immutable admitted schema closure, reusable across separate specification runs.
#[derive(Debug)]
pub struct CapturedSchema {
    sources: Vec<Source>,
    locations: Vec<Vec<SourcePosition>>,
    structure: SchemaStructure,
}

/// An owned structural root. It grants no specification, code or data authority.
#[derive(Debug)]
pub struct CapturedDocument {
    schema: Arc<CapturedSchema>,
    class_name: String,
    source: Source,
    raw: DecodedYaml,
    normalized: NormalizedDocument,
}
impl CapturedDocument {
    /// A 1.x document admitted at the version it declares, held as the class
    /// its declared type selects (`crate::document::AdmittedDocument`).
    pub(crate) fn admitted(
        schema: Arc<CapturedSchema>,
        class_name: &str,
        source: Source,
        raw: DecodedYaml,
        normalized: NormalizedDocument,
    ) -> Self {
        Self {
            schema,
            class_name: class_name.into(),
            source,
            raw,
            normalized,
        }
    }
    pub fn class_name(&self) -> &str {
        &self.class_name
    }
    pub fn schema(&self) -> &Arc<CapturedSchema> {
        &self.schema
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    pub fn normalized(&self) -> &NormalizedDocument {
        &self.normalized
    }
}
/// Physical schema YAML representation; closure ordering and admission belong
/// to the engine. Original occurrence locations stay here.
struct SchemaDecoder;
impl capture::SchemaDecodePort for SchemaDecoder {
    type Error = DecodeFailure;
    type Locations = Vec<SourcePosition>;
    fn decode(
        &mut self,
        _: &str,
        bytes: &[u8],
    ) -> Result<capture::DecodedModule<Self::Locations>, DecodeFailure> {
        let decoded = decode_yaml(bytes)?;
        Ok(capture::DecodedModule {
            document: decoded.document,
            locations: decoded.locations,
        })
    }
}

/// Physical entry decoding retains its source-limit order and YAML locations.
struct StructuralDecoder<'a>(&'a CapturedSchema);
impl yamaa_core::structural_preparation::StructuralDecodePort for StructuralDecoder<'_> {
    type Error = Error;
    type Locations = Vec<SourcePosition>;
    fn decode(
        &mut self,
        source: &Source,
    ) -> Result<yamaa_core::structural_preparation::Decoded<Self::Locations>, Error> {
        let DecodedYaml {
            document,
            locations,
        } = self.0.decode_entry(source)?;
        Ok(yamaa_core::structural_preparation::Decoded {
            document,
            locations,
        })
    }
}

impl CapturedSchema {
    /// Decode the complete host-captured closure using the shared YAML semantics.
    /// Cumulative byte/count admission happens before any source is decoded.
    pub fn admit(sources: Vec<Source>, entry: usize) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, RootSelection::Default)
    }
    /// Capture a closed schema root without interpreting it as a specification.
    pub fn admit_root(
        sources: Vec<Source>,
        entry: usize,
        root_class: &str,
    ) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, RootSelection::Named(root_class))
    }
    fn admit_with_root(
        sources: Vec<Source>,
        entry: usize,
        root: RootSelection<'_>,
    ) -> Result<Arc<Self>, Error> {
        let closure = capture::admit(sources, entry, root, &mut SchemaDecoder).map_err(
            |error| match error {
                capture::Error::Decode { identity, error } => Error::Decode { identity, error },
                capture::Error::Bundle(error) => Error::Bundle(error),
            },
        )?;
        let (sources, locations, structure) = closure.into_parts();
        Ok(Arc::new(Self {
            sources,
            locations,
            structure,
        }))
    }
    /// Preserve the exact captured bytes for provenance and direct equality checks.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn locations(&self) -> &[Vec<SourcePosition>] {
        &self.locations
    }
    pub fn structure(&self) -> &SchemaStructure {
        &self.structure
    }

    /// Validate and normalize a versioned root without windows, inheritance or IO.
    /// Environment-specific admission follows this captured structural gate.
    pub fn prepare_structural(self: &Arc<Self>, source: Source) -> Result<CapturedDocument, Error> {
        self.prepare_structural_selected(
            source,
            yamaa_core::structural_preparation::Selection::StructuralRoot,
        )
    }

    /// Normalize a versionless class using the version of its captured schema.
    /// Function definitions and codelist sources never fabricate document versions.
    /// A class declaring schema_version must use prepare_structural instead.
    pub fn prepare_class(self: &Arc<Self>, source: Source) -> Result<CapturedDocument, Error> {
        self.prepare_named_class(source, &self.structure.root_class().name)
    }
    /// Reuse one captured environment schema for independent function/CT sources.
    /// A selected class cannot bypass its schema-version gate.
    pub fn prepare_named_class(
        self: &Arc<Self>,
        source: Source,
        class_name: &str,
    ) -> Result<CapturedDocument, Error> {
        self.prepare_structural_selected(
            source,
            yamaa_core::structural_preparation::Selection::NamedClass(class_name),
        )
    }

    fn prepare_structural_selected(
        self: &Arc<Self>,
        source: Source,
        selection: yamaa_core::structural_preparation::Selection<'_>,
    ) -> Result<CapturedDocument, Error> {
        use yamaa_core::structural_preparation::{self, Error as StructuralError};
        let prepared = structural_preparation::prepare(
            &self.structure,
            source,
            selection,
            &mut StructuralDecoder(self),
        )
        .map_err(|error| match error {
            StructuralError::UnknownClass(name) => Error::UnknownClass(name),
            StructuralError::VersionedRootRequired => Error::VersionedRootRequired,
            StructuralError::Decode(error) => error,
            StructuralError::Validation(error) => Error::Validation(error),
            StructuralError::Normalize(error) => Error::Normalize(error),
            StructuralError::Findings(failure) => {
                let (source, document, findings) = failure.into_parts();
                Error::Findings(Box::new(CapturedFindings {
                    schema: Arc::clone(self),
                    source,
                    document: Some(document),
                    findings,
                }))
            }
        })?;
        let (source, document, locations, class_name, normalized) = prepared.into_parts();
        Ok(CapturedDocument {
            schema: Arc::clone(self),
            class_name,
            source,
            raw: DecodedYaml {
                document,
                locations,
            },
            normalized,
        })
    }

    /// Structural preparation only: no source data, executable plan or effects.
    /// A non-null `parent` must use the inheritance lifecycle.
    pub fn prepare_standalone(self: &Arc<Self>, source: Source) -> Result<PreparedDocument, Error> {
        let raw = self.decode_entry(&source)?;
        self.prepare_standalone_decoded(source, raw)
    }
    fn decode_entry(&self, source: &Source) -> Result<DecodedYaml, Error> {
        decode_yaml(&source.bytes).map_err(|error| Error::Decode {
            identity: source.identity.clone(),
            error,
        })
    }
    fn prepare_standalone_decoded(
        self: &Arc<Self>,
        source: Source,
        raw: DecodedYaml,
    ) -> Result<PreparedDocument, Error> {
        use yamaa_core::standalone_preparation::{self, Error as StandaloneError};
        let DecodedYaml {
            document,
            locations,
        } = raw;
        let prepared = match standalone_preparation::prepare(&self.structure, document) {
            Ok(prepared) => prepared,
            Err(StandaloneError::InheritanceRequired) => return Err(Error::InheritanceRequired),
            Err(StandaloneError::Normalize(failure)) => {
                let standalone_preparation::FailureContext { error, input } = *failure;
                return Err(match error {
                    NormalizationError::Invalid(findings) => {
                        Error::Findings(Box::new(CapturedFindings {
                            schema: Arc::clone(self),
                            source,
                            document: input,
                            findings,
                        }))
                    }
                    error => Error::Normalize(error),
                });
            }
            Err(StandaloneError::Validation(error)) => return Err(Error::Validation(error)),
            Err(StandaloneError::Dependencies(error)) => return Err(Error::Dependencies(*error)),
            Err(StandaloneError::Model(findings)) => {
                return Err(Error::Findings(Box::new(CapturedFindings {
                    schema: Arc::clone(self),
                    source,
                    document: None,
                    findings,
                })));
            }
        };
        let (document, model, origins, windows) = prepared.into_parts();
        Ok(PreparedDocument {
            schema: Arc::clone(self),
            source,
            raw: DecodedYaml {
                document,
                locations,
            },
            content: PreparedContent::Standalone {
                model,
                origins,
                windows,
            },
            parents: Vec::new(),
            dictionaries: None,
            resource_aliases: Default::default(),
            legacy_tree: Default::default(),
        })
    }

    /// Select the shared preparation lifecycle from one decoded raw entry.
    /// Hosts never inspect YAML or retry standalone errors as inheritance.
    ///
    /// An entry that declares the 1.x header is a 1.x specification, read
    /// with the 1.0 closures the package ships whatever this closure is
    /// (SCHEMA-2, #3030 D76); every other entry is read by this closure under
    /// the rules at the `yamaa-v0.5.0` tag (D75).
    pub fn prepare_document<P: InheritancePort>(
        self: &Arc<Self>,
        source: Source,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedDocument, InheritanceError<P::Error>> {
        use yamaa_core::document_preparation::{self as application, Content, Error as Failure};
        // An entry that does not decode is reported by the 0.5 lifecycle,
        // which decodes it again, as before the engine read 1.0 files.
        if decode_yaml(&source.bytes)
            .is_ok_and(|decoded| yamaa_core::language::declares_header(&decoded.document))
        {
            return prepare_versioned(source, display_path, port);
        }
        let prepared = application::prepare(
            &self.structure,
            source,
            display_path,
            &mut DocumentEntry(self),
            &mut inheritance::Decoder,
            port,
        )
        .map_err(|error| match error {
            Failure::Entry(error) => InheritanceError::Entry(error),
            Failure::Standalone { source, error } => {
                InheritanceError::Entry(standalone_dispatch_error(self, source, error))
            }
            Failure::Inherited(yamaa_core::inheritance_capture::Error::Preparation {
                error,
                captured_parents,
            }) => InheritanceError::Preparation {
                schema: Arc::clone(self),
                error,
                captured_parents,
            },
        })?;
        let (source, content) = prepared.into_parts();
        let (raw, content, parents) = match content {
            Content::Standalone {
                prepared,
                locations,
            } => {
                let (document, model, origins, windows) = prepared.into_parts();
                (
                    DecodedYaml {
                        document,
                        locations,
                    },
                    PreparedContent::Standalone {
                        model,
                        origins,
                        windows,
                    },
                    Vec::new(),
                )
            }
            Content::Inherited { raw, prepared } => (
                raw,
                PreparedContent::Inherited(Box::new(prepared.inheritance)),
                prepared.parents,
            ),
        };
        let document = PreparedDocument {
            schema: Arc::clone(self),
            source,
            raw,
            content,
            parents,
            dictionaries: None,
            resource_aliases: Default::default(),
            legacy_tree: Default::default(),
        };
        document.prepare_dictionaries(port)
    }
}

// Physical entry admission remains exactly decode_entry's byte/identity/codec policy.
struct DocumentEntry<'a>(&'a CapturedSchema);
impl yamaa_core::document_preparation::EntryCodec for DocumentEntry<'_> {
    type Raw = DecodedYaml;
    type Locations = Vec<SourcePosition>;
    type Error = Error;
    fn decode(&mut self, source: &Source) -> Result<DecodedYaml, Error> {
        self.0.decode_entry(source)
    }
    fn document(raw: &DecodedYaml) -> &yamaa_core::schema::Document {
        &raw.document
    }
    fn into_parts(raw: DecodedYaml) -> (yamaa_core::schema::Document, Vec<SourcePosition>) {
        (raw.document, raw.locations)
    }
}
fn standalone_dispatch_error(
    schema: &Arc<CapturedSchema>,
    source: Source,
    error: yamaa_core::standalone_preparation::Error,
) -> Error {
    use yamaa_core::standalone_preparation::Error as Failure;
    match error {
        Failure::InheritanceRequired => Error::InheritanceRequired,
        Failure::Normalize(failure) => match failure.error {
            NormalizationError::Invalid(findings) => Error::Findings(Box::new(CapturedFindings {
                schema: Arc::clone(schema),
                source,
                document: failure.input,
                findings,
            })),
            error => Error::Normalize(error),
        },
        Failure::Validation(error) => Error::Validation(error),
        Failure::Dependencies(error) => Error::Dependencies(*error),
        Failure::Model(findings) => Error::Findings(Box::new(CapturedFindings {
            schema: Arc::clone(schema),
            source,
            document: None,
            findings,
        })),
    }
}

/// One `schema` link of a consuming document: the input dataset, the producing
/// specification's canonical identity and its prepared document. A link naming
/// the consuming specification itself holds no separate document.
#[derive(Debug)]
pub struct LinkedSpecification {
    pub dataset: String,
    pub identity: String,
    pub document: Option<Arc<PreparedDocument>>,
}
/// Borrow the engine's link metadata for one consuming document. Only the
/// producers' output contracts are read; no producer is compiled or executed.
/// A consumer and a producer may each be written in either language.
pub fn link_metadata<'a>(
    consumer: &'a PreparedDocument,
    links: &'a [LinkedSpecification],
) -> Vec<yamaa_core::producer_contract::Link<'a>> {
    links
        .iter()
        .map(|link| yamaa_core::producer_contract::Link {
            dataset: &link.dataset,
            identity: &link.identity,
            document: link
                .document
                .as_deref()
                .map_or(consumer.document(), PreparedDocument::document),
        })
        .collect()
}

/// What compilation reads of a prepared specification: a 0.5 model, which
/// the 0.5 front end lowers, or the tree the 1.0 front end lowered, beside
/// the resolved document that declares its linked inputs (#3030 D76).
#[derive(Clone, Copy, Debug)]
pub enum Compilation<'a> {
    Legacy(&'a SpecificationDocument),
    Versioned {
        tree: &'a yamaa_core::ast::Spec,
        document: &'a yamaa_core::schema::Document,
    },
}

/// Retains every final occurrence's raw/schema-default origin and schema snapshot.
/// Construction is private; structural admission is never execution qualification.
#[derive(Debug)]
pub struct PreparedDocument {
    schema: Arc<CapturedSchema>,
    source: Source,
    raw: DecodedYaml,
    content: PreparedContent,
    parents: Vec<CapturedParent>,
    dictionaries: Option<Dictionaries>,
    resource_aliases: std::collections::BTreeMap<String, Vec<(String, String)>>,
    /// A 0.5 model lowered into the AST on first use ([`PreparedDocument::tree`]).
    legacy_tree: std::sync::OnceLock<yamaa_core::ast::Spec>,
}
#[derive(Debug)]
enum PreparedContent {
    Standalone {
        model: SpecificationDocument,
        origins: Vec<SchemaOrigin>,
        windows: Vec<WindowReference>,
    },
    Inherited(Box<yamaa_core::inheritance_preparation::PreparedInheritance>),
    /// A specification that declares the 1.x header, prepared by the 1.0
    /// front end (#3030 D76).
    Versioned(Box<crate::document::PreparedSpecification>),
}
/// The dictionary files a specification's `mapping.dict` paths name.
#[derive(Debug)]
enum Dictionaries {
    /// A 0.5 model with each dictionary's content in place of its path,
    /// which compilation reads.
    Legacy(yamaa_core::dictionary_preparation::Prepared<DecodedYaml>),
    /// The files a 1.x specification read; its lowering holds their content.
    Versioned(Vec<yamaa_core::dictionary_preparation::Captured<DecodedYaml>>),
}
impl Dictionaries {
    fn captures(&self) -> &[yamaa_core::dictionary_preparation::Captured<DecodedYaml>] {
        match self {
            Self::Legacy(prepared) => &prepared.captures,
            Self::Versioned(captures) => captures,
        }
    }
}

/// Prepare a specification that declares the 1.x header, read from `source`,
/// with the 1.0 closures the package ships, by an engine that reads
/// [`yamaa_core::language::LANGUAGE_VERSION`] (#3030 D76): its layers are read
/// and composed, and the dictionary files it names are read.
pub(crate) fn prepare_versioned<P: InheritancePort>(
    source: Source,
    display_path: String,
    port: &mut P,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    prepare_versioned_within(source, display_path, port, &[])
}

/// [`prepare_versioned`] for the producer a copy entry of each specification
/// `resolving` names, entry first, copies from (SPEC-9).
pub(crate) fn prepare_versioned_within<P: InheritancePort>(
    source: Source,
    display_path: String,
    port: &mut P,
    resolving: &[String],
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    use crate::document::{self, Closures, Decoded};
    use yamaa_core::language::{FileType, LANGUAGE_VERSION};
    let closures = Closures::shipped().map_err(InheritanceError::Entry)?;
    let entry = source.identity.clone();
    let versioned =
        match document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures) {
            Ok(Decoded::Versioned(versioned)) => versioned,
            // The caller routes only a root that declares the header here.
            Ok(Decoded::Legacy { .. }) => {
                return Err(InheritanceError::Entry(Error::VersionedRootRequired))
            }
            Err(error) => return Err(InheritanceError::Entry(error)),
        };
    let schema = Arc::clone(versioned.schema());
    let refuse = |error| InheritanceError::Versioned {
        schema: Arc::clone(&schema),
        entry: entry.clone(),
        error: Box::new(error),
    };
    let prepared = versioned
        .prepare_sources_within(display_path, port, resolving)
        .map_err(refuse)?;
    PreparedDocument {
        schema,
        source: prepared.source,
        raw: prepared.raw,
        content: PreparedContent::Versioned(Box::new(prepared.specification)),
        parents: prepared.parents,
        dictionaries: None,
        resource_aliases: Default::default(),
        legacy_tree: Default::default(),
    }
    .prepare_dictionaries(port)
}
impl yamaa_core::file_preparation::PreparedView for PreparedDocument {
    type Parent = CapturedParent;
    fn parents(&self) -> &[CapturedParent] {
        &self.parents
    }
    fn parent_source(parent: &CapturedParent) -> &Source {
        parent.source()
    }
    fn resources(&self) -> impl Iterator<Item = &Source> {
        self.dictionary_sources()
    }
    fn resource_diagnostic(
        &self,
        declaring: &str,
        written: &str,
        failure: yamaa_core::resource::ResourceFailure,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        use yamaa_core::schema::DocumentNode as N;
        let documents = core::iter::once((&self.source, &self.raw.document)).chain(
            self.parents
                .iter()
                .map(|parent| (parent.source(), &parent.raw().document)),
        );
        for (source, raw) in documents {
            if source.identity != declaring {
                continue;
            }
            if let Some(parent) = raw.field(raw.root(), "parent") {
                if matches!(&raw.nodes()[parent], N::Text(path) if path == written) {
                    return Some(failure.field_diagnostic("parent", written));
                }
            }
        }
        let references = self
            .schema
            .structure
            .mapping_dictionary_references(self.document())
            .ok()?;
        references.into_iter().find_map(|reference| {
            (reference.written() == written
                && self.declaring_source(reference.path()) == Some(declaring))
            .then(|| failure.field_diagnostic(reference.path(), written))
        })
    }
    fn retain_resource_aliases(
        &mut self,
        aliases: std::collections::BTreeMap<String, Vec<(String, String)>>,
    ) {
        self.resource_aliases = aliases;
    }
}
impl PreparedDocument {
    pub(crate) fn resource_diagnostic_for_identity(
        &self,
        identity: &str,
        failure: yamaa_core::resource::ResourceFailure,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        use yamaa_core::file_preparation::PreparedView;
        self.resource_aliases
            .get(identity)?
            .iter()
            .find_map(|(declaring, written)| self.resource_diagnostic(declaring, written, failure))
    }
    pub fn schema(&self) -> &CapturedSchema {
        &self.schema
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    /// The 0.5 model of a specification that declares `schema_version`; a 1.x
    /// specification has none ([`PreparedDocument::legacy_model`]).
    ///
    /// # Panics
    ///
    /// On a 1.x specification, which the engine reads through
    /// [`PreparedDocument::document`] and [`PreparedDocument::compilation`].
    pub fn model(&self) -> &SpecificationDocument {
        self.legacy_model()
            .expect("a 1.x specification has no 0.5 model")
    }
    /// The 0.5 model, or none for a specification that declares the 1.x
    /// header.
    pub fn legacy_model(&self) -> Option<&SpecificationDocument> {
        match &self.content {
            PreparedContent::Standalone { model, .. } => Some(model),
            PreparedContent::Inherited(prepared) => Some(prepared.model()),
            PreparedContent::Versioned(_) => None,
        }
    }
    /// The 1.x specification the 1.0 front end prepared, or none for a 0.5
    /// specification.
    pub fn versioned(&self) -> Option<&crate::document::PreparedSpecification> {
        match &self.content {
            PreparedContent::Versioned(prepared) => Some(prepared),
            _ => None,
        }
    }
    /// The resolved specification, normalized, in the language it is written
    /// in: a 0.5 model's document, or a 1.x specification's layers composed,
    /// pruned and ordered. Fields both languages spell alike are read here.
    pub fn document(&self) -> &yamaa_core::schema::Document {
        match &self.content {
            PreparedContent::Versioned(prepared) => prepared.resolved(),
            _ => self.model().document(),
        }
    }
    /// The specification lowered into the AST: a 1.x specification's
    /// lowering, or a 0.5 model's, lowered once on first use. Composition
    /// reads each specification's derivations here (SUB-19, SUB-20).
    pub fn tree(&self) -> &yamaa_core::ast::Spec {
        match &self.content {
            PreparedContent::Versioned(prepared) => prepared.lowered().spec(),
            _ => self.legacy_tree.get_or_init(|| {
                yamaa_core::front_end::v0_5::lower(self.compilation_model(), None).into_spec()
            }),
        }
    }
    /// What compilation reads: the 0.5 model with its dictionaries in place,
    /// or the 1.0 lowering, which holds them.
    pub fn compilation(&self) -> Compilation<'_> {
        match &self.content {
            PreparedContent::Versioned(prepared) => Compilation::Versioned {
                tree: prepared.lowered().spec(),
                document: prepared.resolved(),
            },
            _ => Compilation::Legacy(self.compilation_model()),
        }
    }
    /// The specification's `domain`, when it is written as text: the dataset
    /// name a run's printed summary names.
    pub fn domain(&self) -> Option<&str> {
        let model = self.document();
        match &model.nodes()[model.field(model.root(), "domain")?] {
            yamaa_core::schema::DocumentNode::Text(text) => Some(text),
            _ => None,
        }
    }
    pub fn dictionary_sources(&self) -> impl Iterator<Item = &Source> {
        self.dictionaries.iter().flat_map(|dictionaries| {
            dictionaries
                .captures()
                .iter()
                .map(|capture| capture.source())
        })
    }
    pub(crate) fn dictionary_input<'a, E>(
        &'a self,
        error: &'a yamaa_core::dictionary_preparation::Error<E, Error, DecodedYaml>,
        arena: yamaa_core::dictionary_preparation::Arena,
    ) -> Option<&'a yamaa_core::schema::Document> {
        use yamaa_core::dictionary_preparation::Arena;
        match arena {
            Arena::Seed(0) => Some(&self.raw.document),
            Arena::Seed(index) => self
                .parents
                .get(index - 1)
                .map(|parent| &parent.raw().document),
            Arena::Captured(index) => error
                .captures
                .get(index)
                .map(|capture| &capture.raw().document),
        }
    }
    pub(crate) fn compilation_model(&self) -> &SpecificationDocument {
        match &self.dictionaries {
            Some(Dictionaries::Legacy(prepared)) => &prepared.model,
            _ => self.model(),
        }
    }
    fn prepare_dictionaries<P: InheritancePort>(
        mut self,
        port: &mut P,
    ) -> Result<Self, InheritanceError<P::Error>> {
        use yamaa_core::dictionary_preparation::{self as application, Origins, Seed};
        let seeds = core::iter::once(Seed {
            source: &self.source,
            raw: &self.raw,
        })
        .chain(self.parents.iter().map(|parent| Seed {
            source: parent.source(),
            raw: parent.raw(),
        }))
        .collect::<Vec<_>>();
        if let PreparedContent::Versioned(prepared) = &self.content {
            // A 1.x specification's dictionary paths are read from the layer
            // that wrote each, and the lowering binding reads holds their
            // content; the resolved specification keeps the paths.
            let origins = Origins::Versioned {
                provenance: prepared.provenance(),
                layers: prepared.layers(),
            };
            let materialized = application::prepare_document(
                self.schema.structure(),
                prepared.resolved(),
                origins,
                &seeds,
                port,
                &mut inheritance::Decoder,
            );
            drop(seeds);
            return match materialized {
                Ok(None) => Ok(self),
                Ok(Some(materialized)) => {
                    let Self {
                        source,
                        raw,
                        parents,
                        content,
                        ..
                    } = &mut self;
                    if let PreparedContent::Versioned(prepared) = content {
                        prepared.lower_materialized(
                            &materialized.document,
                            &materialized.sources,
                            |layer| crate::document::layer_locations(source, raw, parents, layer),
                        );
                    }
                    self.dictionaries = Some(Dictionaries::Versioned(materialized.captures));
                    Ok(self)
                }
                Err(error) => Err(InheritanceError::Dictionary {
                    document: Box::new(self),
                    error,
                }),
            };
        }
        let origins = self.inheritance().map_or(
            Origins::Standalone(&self.source.identity),
            Origins::Inherited,
        );
        let prepared = application::prepare(
            self.schema.structure(),
            self.model(),
            origins,
            &seeds,
            port,
            &mut inheritance::Decoder,
        );
        drop(seeds);
        match prepared {
            Ok(prepared) => {
                self.dictionaries = prepared.map(Dictionaries::Legacy);
                Ok(self)
            }
            Err(error) => Err(InheritanceError::Dictionary {
                document: Box::new(self),
                error,
            }),
        }
    }
    /// The schema origin of each node of a 0.5 model; a 1.x specification
    /// keeps its origins with its layers ([`PreparedDocument::versioned`]).
    pub fn origins(&self) -> &[SchemaOrigin] {
        match &self.content {
            PreparedContent::Standalone { origins, .. } => origins,
            PreparedContent::Inherited(prepared) => prepared.origins(),
            PreparedContent::Versioned(_) => &[],
        }
    }
    /// The named windows a 0.5 model expanded; a 1.x specification's
    /// composition expands its own.
    pub fn windows(&self) -> &[WindowReference] {
        match &self.content {
            PreparedContent::Standalone { windows, .. } => windows,
            PreparedContent::Inherited(prepared) => prepared.windows(),
            PreparedContent::Versioned(_) => &[],
        }
    }
    /// Input occurrences in final origins address this retained document.
    pub fn normalization_input(&self) -> &yamaa_core::schema::Document {
        match &self.content {
            PreparedContent::Standalone { .. } | PreparedContent::Versioned(_) => {
                &self.raw.document
            }
            PreparedContent::Inherited(prepared) => prepared.normalization_input(),
        }
    }
    pub fn inheritance(&self) -> Option<&yamaa_core::inheritance_preparation::PreparedInheritance> {
        match &self.content {
            PreparedContent::Inherited(prepared) => Some(prepared),
            PreparedContent::Standalone { .. } | PreparedContent::Versioned(_) => None,
        }
    }
    /// Raw parent snapshots remain in first-capture order, separately from postorder layers.
    pub fn parents(&self) -> &[CapturedParent] {
        &self.parents
    }
    /// Borrow the contributing original layer for a diagnostic field. Logical
    /// descendants retain their nearest composed ancestor's source. A missing
    /// inheritance origin remains unavailable rather than inventing one.
    pub fn declaring_source(&self, path: &str) -> Option<&str> {
        if let PreparedContent::Versioned(prepared) = &self.content {
            // A 1.x path names keyed members by identity already (P-CLR-2).
            return prepared.declaring_source(path);
        }
        match self.inheritance() {
            None => Some(&self.source.identity),
            Some(inherited) => {
                // Native findings use final physical collection indices;
                // composition retains the literal admitted member identity.
                // This is a representation mapping over the held model, never
                // a source/path lookup or a split of an authored identity.
                let logical = self.indexed_diagnostic_path(path)?;
                let path = logical.as_deref().unwrap_or(path);
                let origin = inherited
                    .provenance()
                    .iter()
                    .filter(|origin| {
                        path == origin.path
                            || path
                                .strip_prefix(&origin.path)
                                .is_some_and(|suffix| suffix.starts_with(['.', '[']))
                    })
                    .max_by_key(|origin| origin.path.len())?;
                Some(&inherited.layers().get(origin.layer)?.source.identity)
            }
        }
    }

    fn indexed_diagnostic_path(&self, path: &str) -> Option<Option<String>> {
        let Some((collection, remaining)) = path.split_once('[') else {
            return Some(None);
        };
        let identity = match collection {
            "rows" | "intermediates" => "id",
            "columns" => "name",
            _ => return Some(None),
        };
        let (written_index, suffix) = remaining.split_once(']')?;
        if written_index.is_empty()
            || !written_index.bytes().all(|byte| byte.is_ascii_digit())
            || written_index.len() > 1 && written_index.starts_with('0')
            || !suffix.is_empty() && !suffix.starts_with(['.', '['])
        {
            return None;
        }
        let index = written_index.parse::<usize>().ok()?;
        let document = self.model().document();
        let members = document.field(document.root(), collection)?;
        let yamaa_core::schema::DocumentNode::Sequence(members) = &document.nodes()[members] else {
            return None;
        };
        let member = *members.get(index)?;
        let identity = document.field(member, identity)?;
        let yamaa_core::schema::DocumentNode::Text(identity) = &document.nodes()[identity] else {
            return None;
        };
        Some(Some(format!("{collection}.{identity}{suffix}")))
    }

    /// Resolve an input's authored path against held source provenance. Composition
    /// may have rebased the executable path; diagnostics must not disclose that
    /// spelling instead of the value written by the contributing layer.
    pub fn written_source_path(&self, dataset: &str) -> Option<&str> {
        self.written_input_origin(dataset, "path")
            .map(|p| p.written)
    }
    /// Retain the layer that authored each `schema` link independently from the
    /// layer that authored its artifact path. No source is reopened or resolved.
    pub fn written_input_origin(
        &self,
        dataset: &str,
        field: &str,
    ) -> Option<yamaa_core::resource::WrittenPath<'_>> {
        if !["path", "schema"].contains(&field) {
            return None;
        }
        use yamaa_core::schema::DocumentNode;
        let (identity, raw) = self.authored_document(&format!("input.{dataset}.{field}"))?;
        let resolved = self.document();
        let inputs = resolved.field(resolved.root(), "input")?;
        let declaration = resolved.field(inputs, dataset)?;
        let base_source = if entry_relative(resolved, declaration) {
            self.source.identity.as_str()
        } else {
            identity
        };
        let input = raw.field(raw.root(), "input")?;
        let source = raw.field(input, dataset)?;
        let node = match raw.nodes().get(source)? {
            DocumentNode::Text(_) if field == "path" => source,
            _ => raw.field(source, field)?,
        };
        match raw.nodes().get(node)? {
            DocumentNode::Text(path) => Some(yamaa_core::resource::WrittenPath {
                declaring_source: identity,
                base_source,
                written: path,
            }),
            _ => None,
        }
    }
    /// The output path's diagnostic origin belongs to its contributing layer.
    pub fn written_output_origin(&self) -> Option<yamaa_core::resource::WrittenPath<'_>> {
        self.written_output_field_origin("path")
    }
    /// Every governed output field retains its original atomic output layer.
    pub(crate) fn written_output_field_origin(
        &self,
        field: &str,
    ) -> Option<yamaa_core::resource::WrittenPath<'_>> {
        if !["path", "report"].contains(&field) {
            return None;
        }
        // Output is an atomic top-level field in a 0.5 chain's composition; a
        // 1.x chain's field names the layer that wrote it.
        let written = match &self.content {
            PreparedContent::Versioned(_) => format!("output.{field}"),
            _ => "output".into(),
        };
        let (identity, raw) = self.authored_document(&written)?;
        let resolved = self.document();
        let output = resolved.field(resolved.root(), "output")?;
        let base_source = if entry_relative(resolved, output) {
            self.source.identity.as_str()
        } else {
            identity
        };
        let output = raw.field(raw.root(), "output")?;
        let node = raw.field(output, field)?;
        match &raw.nodes()[node] {
            yamaa_core::schema::DocumentNode::Text(path) => {
                Some(yamaa_core::resource::WrittenPath {
                    declaring_source: identity,
                    base_source,
                    written: path,
                })
            }
            _ => None,
        }
    }
    fn authored_document(&self, path: &str) -> Option<(&str, &yamaa_core::schema::Document)> {
        if let PreparedContent::Versioned(prepared) = &self.content {
            // The layer that wrote `path`, or the nearest value holding it,
            // as that layer was written.
            let layer = prepared.declaring_layer(path)?;
            return Some((layer.source.identity.as_str(), &layer.document));
        }
        if let Some(inherited) = self.inheritance() {
            let origin = inherited.provenance().iter().find(|p| p.path == path)?;
            let identity = &inherited.layers().get(origin.layer)?.source.identity;
            if identity == &self.source.identity {
                Some((identity.as_str(), &self.raw.document))
            } else {
                Some((
                    identity.as_str(),
                    &self
                        .parents
                        .iter()
                        .find(|p| &p.source().identity == identity)?
                        .raw()
                        .document,
                ))
            }
        } else {
            Some((self.source.identity.as_str(), &self.raw.document))
        }
    }
}

fn entry_relative(document: &yamaa_core::schema::Document, node: usize) -> bool {
    document.field(node, "relative_to").is_some_and(|field| {
        matches!(&document.nodes()[field], yamaa_core::schema::DocumentNode::Text(base) if base == "entry")
    })
}
