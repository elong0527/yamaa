//! Inheritance application lifecycle over captured documents and explicit path authority.
use crate::inheritance::{self, Layer, Source, SourcePort};
use alloc::{boxed::Box, string::String, vec::Vec};
use yamaa_core::schema::{
    Document, DocumentError, DocumentLimits, DocumentNode as N, InheritanceDependencyError,
    LayerCompositionError, LayerProvenance, NormalizationBudget, NormalizationError,
    SchemaDiagnostic, SchemaOrigin, SchemaStructure, SpecificationDocument, ValidationError,
    WindowReference,
};

/// The host respells paths using captured canonical file identities, without reopening them.
pub trait PathPort: SourcePort {
    fn rebase(
        &mut self,
        layer: &Source,
        entry: &Source,
        written: &str,
        maximum_bytes: usize,
    ) -> Result<String, Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub traversal: inheritance::Limits,
    pub rebased_bytes: usize,
    pub storage: DocumentLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            traversal: Default::default(),
            rebased_bytes: 1_048_576,
            storage: Default::default(),
        }
    }
}

#[derive(Debug)]
pub enum Error<E> {
    ParentsRequired,
    Traversal(inheritance::Error<E>),
    Path(E),
    PathBytes,
    Document(DocumentError),
    Composition(LayerCompositionError),
    Normalize(Box<FailureContext<NormalizationError>>),
    Dependencies(Box<FailureContext<InheritanceDependencyError>>),
    Validation(ValidationError),
    Model(Vec<SchemaDiagnostic>),
}

/// Failed passes retain the arena addressed by semantic context references.
/// Policy/internal failures need no arena copy and never become language findings.
#[derive(Debug)]
pub struct FailureContext<E> {
    pub error: E,
    pub input: Option<Document>,
}
fn normalization_context(
    error: NormalizationError,
    input: &Document,
) -> Box<FailureContext<NormalizationError>> {
    let retained = matches!(&error, NormalizationError::Invalid(_)).then(|| input.clone());
    Box::new(FailureContext {
        error,
        input: retained,
    })
}
fn dependency_context(
    error: InheritanceDependencyError,
    input: &Document,
) -> Box<FailureContext<InheritanceDependencyError>> {
    use yamaa_core::schema::InheritanceReferenceError;
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
    expanded_before_pruning: Vec<WindowReference>,
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
    /// Expansion history can include declarations subsequently removed by pruning.
    pub fn expanded_before_pruning(&self) -> &[WindowReference] {
        &self.expanded_before_pruning
    }
}

fn rebase<P: PathPort>(
    layer: &mut Layer,
    entry: &Source,
    port: &mut P,
    remaining: &mut usize,
    storage: DocumentLimits,
) -> Result<(), Error<P::Error>> {
    let document = &layer.document;
    let mut paths = Vec::new();
    if let Some(input) = document.field(document.root(), "input") {
        if let N::Mapping(inputs) = &document.nodes()[input] {
            for &(_, source) in inputs {
                for field in ["path", "schema"] {
                    if let Some(id) = document.field(source, field) {
                        paths.push(id);
                    }
                }
            }
        }
    }
    if let Some(output) = document.field(document.root(), "output") {
        for field in ["path", "warning_log", "verification_log"] {
            if let Some(id) = document.field(output, field) {
                paths.push(id);
            }
        }
    }
    let mut nodes = document.nodes().to_vec();
    for id in paths {
        if let N::Text(written) = &document.nodes()[id] {
            let path = port
                .rebase(&layer.source, entry, written, *remaining)
                .map_err(Error::Path)?;
            *remaining = remaining.checked_sub(path.len()).ok_or(Error::PathBytes)?;
            nodes[id] = N::Text(path);
        }
    }
    layer.document = Document::new(nodes, document.root(), storage).map_err(Error::Document)?;
    Ok(())
}

/// Traverse once, compose, expand non-strictly, prune, then normalize and admit the final model.
/// No table reads, function activation or output publication occur during this lifecycle.
pub fn prepare<P: PathPort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
    normalization: &mut NormalizationBudget,
    limits: Limits,
) -> Result<PreparedInheritance, Error<P::Error>> {
    if entry_document
        .field(entry_document.root(), "parents")
        .is_none()
    {
        return Err(Error::ParentsRequired);
    }
    let mut layers = inheritance::traverse(
        schema,
        entry.clone(),
        entry_document,
        port,
        normalization,
        &mut inheritance::Budget::new(limits.traversal),
    )
    .map_err(Error::Traversal)?;
    let mut remaining = limits.rebased_bytes;
    for layer in &mut layers {
        rebase(layer, &entry, port, &mut remaining, limits.storage)?;
    }
    let contributions = layers
        .iter()
        .map(|layer| layer.document.clone())
        .collect::<Vec<_>>();
    let composed = schema
        .compose_layers(&contributions, normalization)
        .map_err(Error::Composition)?;
    let expanded = schema
        .expand_named_windows(&composed.document, false, normalization)
        .map_err(|error| Error::Normalize(normalization_context(error, &composed.document)))?;
    let expanded_before_pruning = expanded.references.clone();
    let pruned = schema
        .resolve_inheritance_dependencies(&expanded.document, normalization)
        .map_err(|error| Error::Dependencies(dependency_context(error, &expanded.document)))?;
    let normalized = schema
        .normalize_document(&pruned.document, normalization)
        .map_err(|error| Error::Normalize(normalization_context(error, &pruned.document)))?;
    let expanded = schema
        .expand_named_windows(&normalized.document, true, normalization)
        .map_err(|error| Error::Normalize(normalization_context(error, &normalized.document)))?;
    let origins = expanded
        .origins
        .iter()
        .map(|&id| normalized.origins[id])
        .collect();
    let model = SpecificationDocument::admit(expanded.document, normalization.validation_scope())
        .map_err(Error::Validation)?
        .map_err(Error::Model)?;
    Ok(PreparedInheritance {
        model,
        layers,
        provenance: composed.provenance,
        normalization_input: pruned.document,
        origins,
        expanded_before_pruning,
        windows: expanded.references,
    })
}
