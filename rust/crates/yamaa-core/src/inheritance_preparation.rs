//! Inheritance application lifecycle over captured documents and explicit path authority.
use crate::inheritance::{self, Layer, Source, SourcePort};
use crate::schema::{
    Document, DocumentError, DocumentNode as N, InheritanceDependencyError, LayerCompositionError,
    LayerProvenance, NormalizationError, SchemaDiagnostic, SchemaOrigin, SchemaStructure,
    SpecificationDocument, ValidationError, WindowReference,
};

/// The host respells paths using captured canonical file identities, without reopening them.
pub trait PathPort: SourcePort {
    fn rebase(
        &mut self,
        layer: &Source,
        entry: &Source,
        written: &str,
    ) -> Result<String, Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("inheritance preparation parents required")]
    ParentsRequired,
    #[error("inheritance preparation traversal: {0:?}")]
    Traversal(inheritance::Error<E>),
    #[error("inheritance preparation path: {0:?}")]
    Path(E),
    #[error("inheritance preparation document: {0:?}")]
    Document(DocumentError),
    #[error("inheritance preparation composition: {0:?}")]
    Composition(LayerCompositionError),
    #[error("inheritance preparation normalize: {0:?}")]
    Normalize(Box<FailureContext<NormalizationError>>),
    #[error("inheritance preparation dependencies: {0:?}")]
    Dependencies(Box<FailureContext<InheritanceDependencyError>>),
    #[error("inheritance preparation validation: {0:?}")]
    Validation(ValidationError),
    #[error("inheritance preparation model: {0:?}")]
    Model(Vec<SchemaDiagnostic>),
}

/// Failed passes retain the arena addressed by semantic context references.
/// Policy/internal failures need no arena copy and never become language findings.
#[derive(Debug)]
pub struct FailureContext<E> {
    pub error: E,
    pub input: Option<Document>,
    pub provenance: Vec<LayerProvenance>,
    pub sources: Vec<String>,
}
fn normalization_context(
    error: NormalizationError,
    input: &Document,
    provenance: &[LayerProvenance],
    layers: &[Layer],
) -> Box<FailureContext<NormalizationError>> {
    let retained = matches!(&error, NormalizationError::Invalid(_)).then(|| input.clone());
    Box::new(FailureContext {
        error,
        input: retained,
        provenance: provenance.to_vec(),
        sources: layers
            .iter()
            .map(|layer| layer.source.identity.clone())
            .collect(),
    })
}
fn dependency_context(
    error: InheritanceDependencyError,
    input: &Document,
    provenance: &[LayerProvenance],
    layers: &[Layer],
) -> Box<FailureContext<InheritanceDependencyError>> {
    use crate::schema::InheritanceReferenceError;
    let retained = matches!(
        &error,
        InheritanceDependencyError::Normalization(NormalizationError::Invalid(_))
            | InheritanceDependencyError::Reference(InheritanceReferenceError::Normalization(
                NormalizationError::Invalid(_)
            ))
    )
    .then(|| input.clone());
    Box::new(FailureContext {
        error,
        input: retained,
        provenance: provenance.to_vec(),
        sources: layers
            .iter()
            .map(|layer| layer.source.identity.clone())
            .collect(),
    })
}

/// Retain each contribution and the pre-normalization document addressed by final origins.
/// Written logical provenance still addresses the retained postorder layer list.
#[derive(Debug)]
pub struct PreparedInheritance {
    model: SpecificationDocument,
    layers: Vec<Layer>,
    provenance: Vec<LayerProvenance>,
    normalization_input: Document,
    origins: Vec<SchemaOrigin>,
    windows: Vec<WindowReference>,
}
impl PreparedInheritance {
    pub fn model(&self) -> &SpecificationDocument {
        &self.model
    }
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }
    pub fn provenance(&self) -> &[LayerProvenance] {
        &self.provenance
    }
    pub fn normalization_input(&self) -> &Document {
        &self.normalization_input
    }
    pub fn origins(&self) -> &[SchemaOrigin] {
        &self.origins
    }
    pub fn windows(&self) -> &[WindowReference] {
        &self.windows
    }
}

fn rebase<P: PathPort>(
    layer: &mut Layer,
    entry: &Source,
    input_bases: &std::collections::BTreeMap<String, bool>,
    port: &mut P,
) -> Result<(), Error<P::Error>> {
    let document = &layer.document;
    let mut paths = Vec::new();
    if let Some(input) = document.field(document.root(), "input") {
        if let N::Mapping(inputs) = &document.nodes()[input] {
            for &(name, source) in inputs {
                let from_entry = matches!(&document.nodes()[name], N::Text(name) if input_bases.get(name) == Some(&true));
                for field in ["path", "schema"] {
                    if let Some(id) = document.field(source, field) {
                        paths.push((id, from_entry));
                    }
                }
            }
        }
    }
    if let Some(output) = document.field(document.root(), "output") {
        let from_entry = document
            .field(output, "relative_to")
            .is_some_and(|id| matches!(&document.nodes()[id], N::Text(base) if base == "entry"));
        for field in ["path", "warning_log", "verification_log"] {
            if let Some(id) = document.field(output, field) {
                paths.push((id, from_entry));
            }
        }
    }
    let mut nodes = document.nodes().to_vec();
    for (id, from_entry) in paths {
        if let N::Text(written) = &document.nodes()[id] {
            let path = port
                .rebase(
                    if from_entry { entry } else { &layer.source },
                    entry,
                    written,
                )
                .map_err(Error::Path)?;
            nodes[id] = N::Text(path);
        }
    }
    layer.document = Document::new(nodes, document.root()).map_err(Error::Document)?;
    Ok(())
}

/// Traverse once, compose, expand non-strictly, prune, then normalize and admit the final model.
/// No table reads, function activation or output publication occur during this lifecycle.
pub fn prepare<P: PathPort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
) -> Result<PreparedInheritance, Error<P::Error>> {
    if entry_document
        .field(entry_document.root(), "parents")
        .is_none()
    {
        return Err(Error::ParentsRequired);
    }
    let mut layers = inheritance::traverse(schema, entry.clone(), entry_document, port)
        .map_err(Error::Traversal)?;
    // Input members merge immediate fields. Select the final explicit base
    // before rebasing any contributing path, including a path whose base is
    // overridden by a later layer independently from its written spelling.
    let mut input_bases = std::collections::BTreeMap::new();
    for layer in &layers {
        let document = &layer.document;
        if let Some(input) = document.field(document.root(), "input") {
            if let N::Mapping(inputs) = &document.nodes()[input] {
                for &(name, source) in inputs {
                    let N::Text(name) = &document.nodes()[name] else {
                        continue;
                    };
                    if let Some(base) = document.field(source, "relative_to") {
                        input_bases.insert(
                            name.clone(),
                            matches!(&document.nodes()[base], N::Text(base) if base == "entry"),
                        );
                    }
                }
            }
        }
    }
    for layer in &mut layers {
        rebase(layer, &entry, &input_bases, port)?;
    }
    let contributions = layers
        .iter()
        .map(|layer| layer.document.clone())
        .collect::<Vec<_>>();
    let composed = schema
        .compose_layers(&contributions)
        .map_err(Error::Composition)?;
    let expanded = schema
        .expand_named_windows(&composed.document, false)
        .map_err(|error| {
            Error::Normalize(normalization_context(
                error,
                &composed.document,
                &composed.provenance,
                &layers,
            ))
        })?;
    let pruned = schema
        .resolve_inheritance_dependencies(&expanded.document)
        .map_err(|error| {
            Error::Dependencies(dependency_context(
                error,
                &expanded.document,
                &composed.provenance,
                &layers,
            ))
        })?;
    let normalized = schema
        .normalize_resolved_document(&pruned.document)
        .map_err(|error| {
            Error::Normalize(normalization_context(
                error,
                &pruned.document,
                &composed.provenance,
                &layers,
            ))
        })?;
    let expanded = schema
        .expand_named_windows(&normalized.document, true)
        .map_err(|error| {
            Error::Normalize(normalization_context(
                error,
                &normalized.document,
                &composed.provenance,
                &layers,
            ))
        })?;
    let origins = expanded
        .origins
        .iter()
        .map(|&id| normalized.origins[id])
        .collect();
    let model = SpecificationDocument::admit(expanded.document)
        .map_err(Error::Validation)?
        .map_err(Error::Model)?;
    Ok(PreparedInheritance {
        model,
        layers,
        provenance: composed.provenance,
        normalization_input: pruned.document,
        origins,
        windows: expanded.references,
    })
}

/// Borrow original contributor identities for human attribution only. Diagnostic
/// facts and portable public context remain unchanged. Ownership of both the
/// composed path index and layer identities stays with the rejected preparation.
pub fn diagnostic_origins<E>(error: &Error<E>) -> Vec<(&str, &str)> {
    fn origins<'a, T>(
        context: &'a FailureContext<T>,
        findings: &'a [SchemaDiagnostic],
    ) -> Vec<(&'a str, &'a str)> {
        findings
            .iter()
            .filter_map(|finding| {
                let contribution = context
                    .provenance
                    .iter()
                    .filter(|origin| {
                        finding.path == origin.path
                            || finding
                                .path
                                .strip_prefix(&origin.path)
                                .is_some_and(|suffix| suffix.starts_with(['.', '[']))
                    })
                    .max_by_key(|origin| origin.path.len())?;
                Some((
                    finding.path.as_str(),
                    context.sources.get(contribution.layer)?.as_str(),
                ))
            })
            .collect()
    }
    match error {
        Error::Normalize(context) => match &context.error {
            NormalizationError::Invalid(findings) => origins(context, findings),
            _ => Vec::new(),
        },
        Error::Dependencies(context) => match &context.error {
            InheritanceDependencyError::Normalization(NormalizationError::Invalid(findings))
            | InheritanceDependencyError::Reference(
                crate::schema::InheritanceReferenceError::Normalization(
                    NormalizationError::Invalid(findings),
                ),
            ) => origins(context, findings),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}
