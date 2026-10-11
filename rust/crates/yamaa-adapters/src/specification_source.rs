//! Captured raw YAML entry point. Hosts supply bytes and source identities only.
//! Filesystem authority, include discovery and publication remain explicit host ports.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits, DecodedYaml, SourcePosition};
use std::sync::Arc;
use yamaa_core::schema::{
    BundleError, BundleLimits, NormalizationBudget, NormalizationError, NormalizationLimits,
    NormalizedDocument, SchemaDiagnostic, SchemaOrigin, SchemaStructure, SpecificationDocument,
    ValidationError, WindowReference,
};
use yamaa_engine::schema_capture::{self as capture, RootSelection};
#[path = "specification_inheritance.rs"]
mod inheritance;
pub use inheritance::{CapturedParent, InheritanceError, InheritancePort, SourceFailure};

/// The engine-owned identity and byte snapshot shared with metadata preparation.
pub use yamaa_engine::project_preparation::Source;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub identity_bytes: usize,
    pub decode: DecodeLimits,
    pub bundle: BundleLimits,
    pub normalization: NormalizationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            identity_bytes: 65_536,
            decode: Default::default(),
            bundle: Default::default(),
            normalization: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Decode {
        identity: String,
        error: DecodeFailure,
    },
    Bundle(BundleError),
    Normalize(NormalizationError),
    Validation(ValidationError),
    Findings(Box<CapturedFindings>),
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

/// One immutable admitted schema closure, reusable across separate specification runs.
#[derive(Debug)]
pub struct CapturedSchema {
    sources: Vec<Source>,
    locations: Vec<Vec<SourcePosition>>,
    structure: SchemaStructure,
    limits: Limits,
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
/// to the engine. DecodeLimits and original occurrence locations stay here.
struct SchemaDecoder(DecodeLimits);
impl capture::DecodePort for SchemaDecoder {
    type Error = DecodeFailure;
    type Locations = Vec<SourcePosition>;
    fn decode(
        &mut self,
        _: &str,
        bytes: &[u8],
    ) -> Result<capture::DecodedModule<Self::Locations>, DecodeFailure> {
        let decoded = decode_yaml(bytes, self.0)?;
        Ok(capture::DecodedModule {
            document: decoded.document,
            locations: decoded.locations,
        })
    }
}

/// Physical entry decoding retains its source-limit order and YAML locations.
struct StructuralDecoder<'a>(&'a CapturedSchema);
impl yamaa_engine::structural_preparation::DecodePort for StructuralDecoder<'_> {
    type Error = Error;
    type Locations = Vec<SourcePosition>;
    fn decode(
        &mut self,
        source: &Source,
    ) -> Result<yamaa_engine::structural_preparation::Decoded<Self::Locations>, Error> {
        let DecodedYaml {
            document,
            locations,
        } = self.0.decode_entry(source)?;
        Ok(yamaa_engine::structural_preparation::Decoded {
            document,
            locations,
        })
    }
}

impl CapturedSchema {
    /// Decode the complete host-captured closure using the shared YAML semantics.
    /// Cumulative byte/count admission happens before any source is decoded.
    pub fn admit(sources: Vec<Source>, entry: usize, limits: Limits) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, RootSelection::Default, limits)
    }
    /// Capture a closed schema root without interpreting it as a specification.
    pub fn admit_root(
        sources: Vec<Source>,
        entry: usize,
        root_class: &str,
        limits: Limits,
    ) -> Result<Arc<Self>, Error> {
        Self::admit_with_root(sources, entry, RootSelection::Named(root_class), limits)
    }
    fn admit_with_root(
        sources: Vec<Source>,
        entry: usize,
        root: RootSelection<'_>,
        limits: Limits,
    ) -> Result<Arc<Self>, Error> {
        let closure = capture::admit(
            sources,
            entry,
            root,
            capture::Limits {
                captured_bytes: limits.captured_bytes,
                identity_bytes: limits.identity_bytes,
                bundle: limits.bundle,
            },
            &mut SchemaDecoder(limits.decode),
        )
        .map_err(|error| match error {
            capture::Error::Limit(resource) => Error::Limit(match resource {
                capture::Resource::RootClass => "root_class",
                capture::Resource::SchemaModules => "schema_modules",
                capture::Resource::CapturedBytes => "captured_bytes",
                capture::Resource::IdentityBytes => "identity_bytes",
            }),
            capture::Error::Decode { identity, error } => Error::Decode { identity, error },
            capture::Error::Bundle(error) => Error::Bundle(error),
        })?;
        let (sources, locations, structure) = closure.into_parts();
        Ok(Arc::new(Self {
            sources,
            locations,
            structure,
            limits,
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
            yamaa_engine::structural_preparation::Selection::StructuralRoot,
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
            yamaa_engine::structural_preparation::Selection::NamedClass(class_name),
        )
    }

    fn prepare_structural_selected(
        self: &Arc<Self>,
        source: Source,
        selection: yamaa_engine::structural_preparation::Selection<'_>,
    ) -> Result<CapturedDocument, Error> {
        use yamaa_engine::structural_preparation::{self, Error as StructuralError};
        let prepared = structural_preparation::prepare(
            &self.structure,
            source,
            selection,
            structural_preparation::Limits {
                class_name_bytes: self.limits.identity_bytes,
                normalization: self.limits.normalization,
            },
            &mut StructuralDecoder(self),
        )
        .map_err(|error| match error {
            StructuralError::ClassNameLimit => Error::Limit("class_name"),
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
    /// Presence of `parents`, including null, must use the inheritance lifecycle.
    pub fn prepare_standalone(self: &Arc<Self>, source: Source) -> Result<PreparedDocument, Error> {
        let raw = self.decode_entry(&source)?;
        self.prepare_standalone_decoded(source, raw)
    }
    fn decode_entry(&self, source: &Source) -> Result<DecodedYaml, Error> {
        if source.bytes.len() > self.limits.captured_bytes {
            return Err(Error::Limit("captured_bytes"));
        }
        if source.identity.len() > self.limits.identity_bytes {
            return Err(Error::Limit("identity_bytes"));
        }
        decode_yaml(&source.bytes, self.limits.decode).map_err(|error| Error::Decode {
            identity: source.identity.clone(),
            error,
        })
    }
    fn prepare_standalone_decoded(
        self: &Arc<Self>,
        source: Source,
        raw: DecodedYaml,
    ) -> Result<PreparedDocument, Error> {
        use yamaa_engine::standalone_preparation::{self, Error as StandaloneError};
        let DecodedYaml {
            document,
            locations,
        } = raw;
        let mut budget = NormalizationBudget::new(self.limits.normalization);
        let prepared = match standalone_preparation::prepare(&self.structure, document, &mut budget)
        {
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
        })
    }

    /// Select the shared preparation lifecycle from one decoded raw entry.
    /// Hosts never inspect YAML or retry standalone errors as inheritance.
    pub fn prepare_document<P: InheritancePort>(
        self: &Arc<Self>,
        source: Source,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedDocument, InheritanceError<P::Error>> {
        use yamaa_engine::document_preparation::{self as application, Content, Error as Failure};
        let prepared = application::prepare(
            &self.structure,
            source,
            display_path,
            &mut DocumentEntry(self),
            &mut inheritance::Decoder::new(self.limits.decode),
            port,
            yamaa_engine::inheritance_capture::Limits {
                captured_bytes: self.limits.captured_bytes,
                identity_bytes: self.limits.identity_bytes,
                normalization: self.limits.normalization,
                ..Default::default()
            },
        )
        .map_err(|error| match error {
            Failure::Entry(error) => InheritanceError::Entry(error),
            Failure::Standalone { source, error } => {
                InheritanceError::Entry(standalone_dispatch_error(self, source, error))
            }
            Failure::Inherited(yamaa_engine::inheritance_capture::Error::Entry(error)) => {
                InheritanceError::Entry(error)
            }
            Failure::Inherited(yamaa_engine::inheritance_capture::Error::Preparation {
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
        Ok(PreparedDocument {
            schema: Arc::clone(self),
            source,
            raw,
            content,
            parents,
        })
    }
}

// Physical entry admission remains exactly decode_entry's byte/identity/codec policy.
struct DocumentEntry<'a>(&'a CapturedSchema);
impl yamaa_engine::document_preparation::EntryCodec for DocumentEntry<'_> {
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
    error: yamaa_engine::standalone_preparation::Error,
) -> Error {
    use yamaa_engine::standalone_preparation::Error as Failure;
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
        Failure::Model(findings) => Error::Findings(Box::new(CapturedFindings {
            schema: Arc::clone(schema),
            source,
            document: None,
            findings,
        })),
    }
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
}
#[derive(Debug)]
enum PreparedContent {
    Standalone {
        model: SpecificationDocument,
        origins: Vec<SchemaOrigin>,
        windows: Vec<WindowReference>,
    },
    Inherited(Box<yamaa_engine::inheritance_preparation::PreparedInheritance>),
}
impl yamaa_engine::file_preparation::PreparedView for PreparedDocument {
    type Parent = CapturedParent;
    fn parents(&self) -> &[CapturedParent] {
        &self.parents
    }
    fn parent_source(parent: &CapturedParent) -> &Source {
        parent.source()
    }
}
impl PreparedDocument {
    pub fn schema(&self) -> &CapturedSchema {
        &self.schema
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn raw(&self) -> &DecodedYaml {
        &self.raw
    }
    pub fn model(&self) -> &SpecificationDocument {
        match &self.content {
            PreparedContent::Standalone { model, .. } => model,
            PreparedContent::Inherited(prepared) => prepared.model(),
        }
    }
    pub fn origins(&self) -> &[SchemaOrigin] {
        match &self.content {
            PreparedContent::Standalone { origins, .. } => origins,
            PreparedContent::Inherited(prepared) => prepared.origins(),
        }
    }
    pub fn windows(&self) -> &[WindowReference] {
        match &self.content {
            PreparedContent::Standalone { windows, .. } => windows,
            PreparedContent::Inherited(prepared) => prepared.windows(),
        }
    }
    /// Input occurrences in final origins address this retained document.
    pub fn normalization_input(&self) -> &yamaa_core::schema::Document {
        match &self.content {
            PreparedContent::Standalone { .. } => &self.raw.document,
            PreparedContent::Inherited(prepared) => prepared.normalization_input(),
        }
    }
    pub fn inheritance(
        &self,
    ) -> Option<&yamaa_engine::inheritance_preparation::PreparedInheritance> {
        match &self.content {
            PreparedContent::Standalone { .. } => None,
            PreparedContent::Inherited(prepared) => Some(prepared),
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
        match self.inheritance() {
            None => Some(&self.source.identity),
            Some(inherited) => {
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

    /// Resolve an input's authored path against held source provenance. Composition
    /// may have rebased the executable path; diagnostics must not disclose that
    /// spelling instead of the value written by the contributing layer.
    pub fn written_source_path(&self, dataset: &str) -> Option<&str> {
        self.written_input_origin(dataset, "path")
            .map(|p| p.written)
    }
    /// Retain the layer that authored each producer link independently from the
    /// layer that authored its artifact path. No source is reopened or resolved.
    pub fn written_input_origin(
        &self,
        dataset: &str,
        field: &str,
    ) -> Option<yamaa_core::producer_admission::WrittenPath<'_>> {
        if !["path", "schema"].contains(&field) {
            return None;
        }
        use yamaa_core::schema::DocumentNode;
        let (identity, raw) = self.authored_document(&format!("input.{dataset}.{field}"))?;
        let input = raw.field(raw.root(), "input")?;
        let source = raw.field(input, dataset)?;
        let node = match raw.nodes().get(source)? {
            DocumentNode::Text(_) if field == "path" => source,
            _ => raw.field(source, field)?,
        };
        match raw.nodes().get(node)? {
            DocumentNode::Text(path) => Some(yamaa_core::producer_admission::WrittenPath {
                declaring_source: identity,
                written: path,
            }),
            _ => None,
        }
    }
    /// The producer's diagnostic path also belongs to its contributing layer.
    pub fn written_output_origin(&self) -> Option<yamaa_core::producer_admission::WrittenPath<'_>> {
        self.written_output_field_origin("path")
    }
    /// Every governed output field retains its original atomic output layer.
    pub(crate) fn written_output_field_origin(
        &self,
        field: &str,
    ) -> Option<yamaa_core::producer_admission::WrittenPath<'_>> {
        if !["path", "verification_log", "warning_log"].contains(&field) {
            return None;
        }
        // Output is an atomic top-level field in shared layer composition.
        let (identity, raw) = self.authored_document("output")?;
        let output = raw.field(raw.root(), "output")?;
        let node = raw.field(output, field)?;
        match &raw.nodes()[node] {
            yamaa_core::schema::DocumentNode::Text(path) => {
                Some(yamaa_core::producer_admission::WrittenPath {
                    declaring_source: identity,
                    written: path,
                })
            }
            _ => None,
        }
    }
    fn authored_document(&self, path: &str) -> Option<(&str, &yamaa_core::schema::Document)> {
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
