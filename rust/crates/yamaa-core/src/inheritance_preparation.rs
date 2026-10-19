//! Inheritance application lifecycle over captured documents and explicit path authority.
use crate::inheritance::{self, InheritanceSourcePort, Layer, Source};
use crate::resolution::{self, Failure, Order};
use crate::schema::{
    Document, DocumentError, DocumentNode as N, LayerCompositionError, LayerNodeOrigin,
    LayerProvenance, NormalizationError, SchemaDiagnostic, SchemaOrigin, SchemaSource,
    SchemaStructure, SpecificationDocument, ValidationError, WindowReference,
};

/// The host respells paths using captured canonical file identities, without reopening them.
pub trait PathPort: InheritanceSourcePort {
    fn rebase(
        &mut self,
        layer: &Source,
        entry: &Source,
        written: &str,
    ) -> Result<String, Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error<E> {
    #[error("inheritance preparation parent required")]
    ParentRequired,
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
    Dependencies(resolution::Error),
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
    input: Document,
    provenance: &[LayerProvenance],
    layers: &[Layer],
) -> Box<FailureContext<NormalizationError>> {
    let retained = matches!(&error, NormalizationError::Invalid(_)).then_some(input);
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
    original_origins: Vec<LayerNodeOrigin>,
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
    /// Every final occurrence retains its original raw-layer or schema-default
    /// origin across materialization, window expansion, pruning and normalization.
    /// Diagnostic origins above continue to address the retained final-pass input.
    pub fn original_origins(&self) -> &[LayerNodeOrigin] {
        &self.original_origins
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
        // REQ-0629 replaces output atomically, keeping its paths and selector together.
        let from_entry = document
            .field(output, "relative_to")
            .is_some_and(|id| matches!(&document.nodes()[id], N::Text(base) if base == "entry"));
        for field in ["path", "report"] {
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

/// Traverse once, compose, then resolve the composition in dependency order.
/// No table reads, function activation or output publication occur during this lifecycle.
pub fn prepare<P: PathPort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
) -> Result<PreparedInheritance, Error<P::Error>> {
    if !inheritance::declares_parent(&entry_document) {
        return Err(Error::ParentRequired);
    }
    let mut layers = inheritance::traverse(schema, entry.clone(), entry_document, port)
        .map_err(Error::Traversal)?;
    // Input members merge field by field. Select the final explicit base
    // before rebasing any contributing path, including a path whose base is
    // overridden or cleared by a later layer independently from its spelling.
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
    let composed_origins = composed
        .node_origins
        .iter()
        .map(|&original| {
            let Some(layer) = original.layer else {
                return original;
            };
            let mut origin = layers[layer].origins[original.origin.node];
            origin.generated |= original.origin.generated;
            LayerNodeOrigin {
                layer: matches!(origin.source, SchemaSource::Input).then_some(layer),
                origin,
            }
        })
        .collect::<Vec<_>>();
    let provenance = composed.provenance;
    let resolved = resolution::resolve(
        schema,
        composed.document,
        &composed_origins,
        Order::Dependencies,
    )
    .map_err(|failure| match failure {
        Failure::Normalize { error, input } => {
            Error::Normalize(normalization_context(error, input, &provenance, &layers))
        }
        Failure::Prune(error) => Error::Dependencies(error),
        Failure::Validation(error) => Error::Validation(error),
        Failure::Model(findings) => Error::Model(findings),
    })?;
    Ok(PreparedInheritance {
        model: resolved.model,
        layers,
        provenance,
        normalization_input: resolved.pruned,
        origins: resolved.origins,
        original_origins: resolved.traced,
        windows: resolved.windows,
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
        _ => Vec::new(),
    }
}
