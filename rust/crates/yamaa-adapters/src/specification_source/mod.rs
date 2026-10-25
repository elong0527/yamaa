//! Captured raw YAML entry point. Hosts supply bytes and source identities only.
//! Filesystem authority, include discovery and publication remain explicit host ports.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodedYaml, SourcePosition};
use std::sync::Arc;
use yamaa_core::schema::{
    BundleError, NormalizationError, NormalizedDocument, SchemaDiagnostic, SchemaStructure,
    ValidationError,
};
mod inheritance;
pub use inheritance::{CapturedParent, InheritanceError, SourceFailure};
use yamaa_core::resolver::Resolver;

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
}

/// A failed semantic pass retains the exact source, schema and context arena.
/// Node references belong to the pass input, which may differ from the raw entry.
#[derive(Debug)]
pub struct CapturedFindings {
    schema: Arc<CapturedSchema>,
    source: Source,
    document: Option<yamaa_core::schema::Document>,
    findings: Vec<SchemaDiagnostic>,
    /// Whether the findings are those of a file a referencing field names,
    /// which each names as its `source` and `entry` ([`Self::name_file`]).
    named: bool,
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
            named: false,
        }
    }
    /// Read the findings as those of the file a referencing field names,
    /// prepared as the entry of its own preparation: each names that file as
    /// its `source` and `entry`, as every finding further into it does, so
    /// that it is located there (SCHEMA-2, P-ISSUE-1).
    pub fn name_file(&mut self) {
        self.named = true;
    }
    /// The provenance each finding names: the named file as `source` and
    /// `entry`, or none for the call's own file.
    pub(crate) fn source_context(&self) -> Vec<(&'static str, &str)> {
        if self.named {
            vec![
                ("source", self.source.identity.as_str()),
                ("entry", self.source.identity.as_str()),
            ]
        } else {
            Vec::new()
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
impl CapturedSchema {
    /// Decode the complete host-captured closure using the shared YAML semantics.
    /// Cumulative byte/count admission happens before any source is decoded.
    pub fn admit(sources: Vec<Source>, entry: usize) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, "root_class")
    }
    /// Capture a closed schema root without interpreting it as a specification.
    pub fn admit_root(
        sources: Vec<Source>,
        entry: usize,
        root_class: &str,
    ) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, root_class)
    }
    /// Every module decodes, in the order supplied, before the closure's
    /// semantic admission, which reports entry and name errors as well.
    fn admit_with_root(
        sources: Vec<Source>,
        entry: usize,
        root_class: &str,
    ) -> Result<Arc<Self>, Error> {
        let mut locations = Vec::with_capacity(sources.len());
        let mut modules = Vec::with_capacity(sources.len());
        for source in &sources {
            let decoded = decode_yaml(&source.bytes).map_err(|error| Error::Decode {
                identity: source.identity.clone(),
                error,
            })?;
            locations.push(decoded.locations);
            modules.push(yamaa_core::schema::SchemaModule {
                name: source.identity.clone(),
                document: decoded.document,
            });
        }
        let structure =
            SchemaStructure::admit(modules, entry, root_class).map_err(Error::Bundle)?;
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

/// One prepared 1.x specification: its layers, composed and resolved, the
/// tree its front end lowered, every raw snapshot of a parent or an included
/// fragment, and the dictionary files it reads (#3030 D76). Construction is private; structural admission
/// is never execution qualification.
#[derive(Debug)]
pub struct PreparedDocument {
    schema: Arc<CapturedSchema>,
    source: Source,
    raw: DecodedYaml,
    specification: Box<crate::document::PreparedSpecification>,
    parents: Vec<CapturedParent>,
    /// The dictionary files its `mapping.dict` paths name; its lowering holds
    /// their content.
    dictionaries: Vec<yamaa_core::dictionary_preparation::Captured<DecodedYaml>>,
    resource_aliases: yamaa_core::file_preparation::Aliases,
}

/// Prepare a specification read from `source` with the 1.0 closures the
/// package ships, by an engine that reads
/// [`yamaa_core::language::LANGUAGE_VERSION`] (#3030 D76): its layers are read
/// and composed, and the dictionary files it names are read. A file that
/// declares `schema_version` is refused as `unsupported_language_version`,
/// naming 0.5 (SCHEMA-2).
pub(crate) fn prepare_versioned<P: Resolver>(
    source: Source,
    display_path: String,
    port: &mut P,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    prepare_versioned_within(source, display_path, port, &[], None)
}

/// [`prepare_versioned`] of a fragment checked on the parent a caller names
/// (#3694). A fragment that declares `parent` or `include` refuses that
/// parent (LAYER-4).
pub fn prepare_fragment<P: Resolver>(
    source: Source,
    display_path: String,
    parent: yamaa_core::inheritance::CallerParent<'_>,
    port: &mut P,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    prepare_versioned_within(source, display_path, port, &[], Some(parent))
}

/// [`prepare_versioned`] for a producer that a copy entry copies from while
/// the specifications and layers `resolving` names, entry first, are being
/// resolved (SPEC-9), or, with a `parent` the caller names, of a fragment
/// checked on it.
pub(crate) fn prepare_versioned_within<P: Resolver>(
    source: Source,
    display_path: String,
    port: &mut P,
    resolving: &[String],
    parent: Option<yamaa_core::inheritance::CallerParent<'_>>,
) -> Result<PreparedDocument, InheritanceError<P::Error>> {
    use crate::document::{self, Closures};
    use yamaa_core::language::{FileType, LANGUAGE_VERSION};
    let closures = Closures::shipped().map_err(InheritanceError::Entry)?;
    let entry = source.identity.clone();
    let versioned = document::decode(source, FileType::Spec, Some(LANGUAGE_VERSION), &closures)
        .map_err(InheritanceError::Entry)?;
    let schema = Arc::clone(versioned.schema());
    let refuse = |error| InheritanceError::Versioned {
        schema: Arc::clone(&schema),
        entry: entry.clone(),
        error: Box::new(error),
    };
    let prepared = versioned
        .prepare_sources_within(display_path, port, resolving, parent)
        .map_err(refuse)?;
    PreparedDocument {
        schema,
        source: prepared.source,
        raw: prepared.raw,
        specification: Box::new(prepared.specification),
        parents: prepared.parents,
        dictionaries: Vec::new(),
        resource_aliases: Default::default(),
    }
    .prepare_dictionaries(port)
}
impl PreparedDocument {
    /// The identity of each file this document holds besides its entry: its
    /// parent layers and included fragments in first-capture order, then its
    /// dictionary files.
    pub fn held(&self) -> impl Iterator<Item = &str> {
        self.parents
            .iter()
            .map(CapturedParent::source)
            .chain(self.dictionary_sources())
            .map(|source| source.identity.as_str())
    }
    /// The finding at the field that wrote `written` in the file `declaring`
    /// names, when this document holds that occurrence: its `parent` or an
    /// `include` entry, naming `declaring` as `source`, or a dictionary path.
    /// Resolved identities never replace the authored field or path.
    pub fn resource_diagnostic(
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
                    return Some(yamaa_core::inheritance::parent_diagnostic(
                        failure, declaring, written,
                    ));
                }
            }
            if let Some(N::Sequence(entries)) = raw
                .field(raw.root(), "include")
                .map(|list| &raw.nodes()[list])
            {
                if let Some(index) = entries.iter().position(
                    |&entry| matches!(&raw.nodes()[entry], N::Text(path) if path == written),
                ) {
                    return Some(yamaa_core::inheritance::included_diagnostic(
                        failure, declaring, index, written,
                    ));
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
    /// Keep the spellings that located each file this document holds, once
    /// every one of them was captured again.
    pub fn retain_resource_aliases(&mut self, aliases: yamaa_core::file_preparation::Aliases) {
        self.resource_aliases = aliases;
    }
    pub fn resource_diagnostic_for_identity(
        &self,
        identity: &str,
        failure: yamaa_core::resource::ResourceFailure,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
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
    /// The specification the 1.0 front end prepared.
    pub fn specification(&self) -> &crate::document::PreparedSpecification {
        &self.specification
    }
    /// The resolved specification: its layers composed, pruned and ordered.
    pub fn document(&self) -> &yamaa_core::schema::Document {
        self.specification.resolved()
    }
    /// The specification lowered into the AST, which compilation reads and
    /// composition reads each specification's derivations in (SUB-19,
    /// SUB-20).
    pub fn tree(&self) -> &yamaa_core::ast::Spec {
        self.specification.lowered().spec()
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
        self.dictionaries.iter().map(|capture| capture.source())
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
    /// Read the dictionary files the specification's `mapping.dict` paths
    /// name, each from the layer that wrote its path; the lowering binding
    /// reads holds their content, and the resolved specification keeps the
    /// paths.
    fn prepare_dictionaries<P: Resolver>(
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
        let origins = Origins {
            provenance: self.specification.provenance(),
            layers: self.specification.layers(),
        };
        let materialized = application::prepare_document(
            self.schema.structure(),
            self.specification.resolved(),
            origins,
            &seeds,
            port,
            &mut crate::decoder::Decoder::<std::convert::Infallible>::default(),
        );
        drop(seeds);
        match materialized {
            Ok(None) => Ok(self),
            Ok(Some(materialized)) => {
                let Self {
                    source,
                    raw,
                    parents,
                    specification,
                    ..
                } = &mut self;
                specification.lower_materialized(
                    &materialized.document,
                    &materialized.sources,
                    |layer| crate::document::layer_locations(source, raw, parents, layer),
                );
                self.dictionaries = materialized.captures;
                Ok(self)
            }
            Err(error) => Err(InheritanceError::Dictionary {
                document: Box::new(self),
                error,
            }),
        }
    }
    /// Raw snapshots of the layers besides the entry, its parents and then
    /// its included fragments, remain in first-capture order, separately from
    /// postorder layers.
    pub fn parents(&self) -> &[CapturedParent] {
        &self.parents
    }
    /// The layer that contributed a diagnostic field, by its path, which names
    /// keyed members by identity (P-CLR-2). Logical descendants retain their
    /// nearest composed ancestor's source.
    pub fn declaring_source(&self, path: &str) -> Option<&str> {
        self.specification.declaring_source(path)
    }

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
    pub fn written_output_field_origin(
        &self,
        field: &str,
    ) -> Option<yamaa_core::resource::WrittenPath<'_>> {
        if !["path", "report"].contains(&field) {
            return None;
        }
        // The field names the layer that wrote it.
        let (identity, raw) = self.authored_document(&format!("output.{field}"))?;
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
    /// The layer that wrote `path`, or the nearest value holding it, as that
    /// layer was written.
    fn authored_document(&self, path: &str) -> Option<(&str, &yamaa_core::schema::Document)> {
        let layer = self.specification.declaring_layer(path)?;
        Some((layer.source.identity.as_str(), &layer.document))
    }
}

fn entry_relative(document: &yamaa_core::schema::Document, node: usize) -> bool {
    document.field(node, "relative_to").is_some_and(|field| {
        matches!(&document.nodes()[field], yamaa_core::schema::DocumentNode::Text(base) if base == "entry")
    })
}

/// A port that holds only the entry: every parent and dictionary it names
/// is unavailable, and every path keeps the spelling its layer wrote.
pub struct EntryOnly;
impl yamaa_core::resolver::Resolver for EntryOnly {
    type Error = ();
    fn inspect(
        &mut self,
        _: yamaa_core::resolver::Name<'_>,
    ) -> Result<yamaa_core::resolver::Located, ()> {
        Err(())
    }
    fn capture(
        &mut self,
        _: yamaa_core::resolver::Name<'_>,
    ) -> Result<yamaa_core::resolver::Captured, ()> {
        Err(())
    }
    fn failure(
        &self,
        _: yamaa_core::resolver::Name<'_>,
        _: &(),
    ) -> Option<yamaa_core::resolver::Failure> {
        Some(yamaa_core::resolver::Failure::Resource(
            yamaa_core::resource::ResourceFailure::Missing,
        ))
    }
    fn rebase(
        &mut self,
        _: &yamaa_core::resolver::Located,
        _: &yamaa_core::resolver::Located,
        written: &str,
    ) -> Result<String, ()> {
        Ok(written.into())
    }
}
