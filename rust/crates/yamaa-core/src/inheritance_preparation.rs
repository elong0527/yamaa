//! The resolution of a 1.x chain over captured documents, its paths respelled
//! through the resolver port ([`prepare_versioned`]).
use crate::inheritance::{Source, VersionedChain};
use crate::resolution::{self, Order};
use crate::resolver::Resolver;
use crate::schema::{
    shorthand_key, written_path, CompositionOrigin, Document, DocumentError, DocumentNode as N,
    LayerNodeOrigin, LayerProvenance, NormalizationError, NormalizedDocument, SchemaDiagnostic,
    SchemaOrigin, SchemaSource, SchemaStructure, SpecPath, VersionedCompositionError,
};
use std::collections::BTreeMap;

/// Failed passes retain the arena addressed by semantic context references.
/// Policy/internal failures need no arena copy and never become language findings.
#[derive(Debug)]
pub struct FailureContext<E> {
    pub error: E,
    pub input: Option<Document>,
    pub provenance: Vec<LayerProvenance>,
    pub sources: Vec<String>,
}

/// The layer at `source`, its relative paths respelled from the entry
/// (LAYER-10): an input's `path` and `schema`, and `output`'s `path` and
/// `report`, each from the base its declaration's composed `relative_to`
/// selects, whichever layer writes it. Node indices are kept, so origins
/// addressing the layer still hold.
fn rebased<R: Resolver>(
    document: &Document,
    source: &Source,
    entry: &Source,
    bases: &Bases,
    resolver: &mut R,
) -> Result<Document, Rebase<R::Error>> {
    let mut paths = Vec::new();
    if let Some(input) = document.field(document.root(), "input") {
        if let N::Mapping(inputs) = &document.nodes()[input] {
            for &(name, declaration) in inputs {
                let from_entry = matches!(&document.nodes()[name], N::Text(name) if bases.inputs.get(name) == Some(&true));
                for field in ["path", "schema"] {
                    if let Some(id) = document.field(declaration, field) {
                        paths.push((id, from_entry));
                    }
                }
            }
        }
    }
    if let Some(output) = document.field(document.root(), "output") {
        for field in ["path", "report"] {
            if let Some(id) = document.field(output, field) {
                paths.push((id, bases.output));
            }
        }
    }
    let mut nodes = document.nodes().to_vec();
    for (id, from_entry) in paths {
        if let N::Text(written) = &document.nodes()[id] {
            let path = resolver
                .rebase(if from_entry { entry } else { source }, entry, written)
                .map_err(Rebase::Path)?;
            nodes[id] = N::Text(path);
        }
    }
    Document::new(nodes, document.root()).map_err(Rebase::Document)
}

/// Why a layer's paths were not respelled.
enum Rebase<E> {
    Path(E),
    Document(DocumentError),
}

/// Whether each declaration's composed `relative_to` selects the entry's
/// directory (FILE-2): each input's, by name, and `output`'s. A declaration
/// that no layer gives one selects the declaring layer.
#[derive(Default)]
struct Bases {
    inputs: BTreeMap<String, bool>,
    output: bool,
}

/// Record the explicit `relative_to` each input of a layer and its `output`
/// write: whether it selects the entry's directory (FILE-2).
fn record_bases(document: &Document, bases: &mut Bases) {
    let entry = |base: usize| matches!(&document.nodes()[base], N::Text(base) if base == "entry");
    if let Some(N::Mapping(inputs)) = document
        .field(document.root(), "input")
        .map(|input| &document.nodes()[input])
    {
        for &(name, source) in inputs {
            let N::Text(name) = &document.nodes()[name] else {
                continue;
            };
            if let Some(base) = document.field(source, "relative_to") {
                bases.inputs.insert(name.clone(), entry(base));
            }
        }
    }
    if let Some(base) = document
        .field(document.root(), "output")
        .and_then(|output| document.field(output, "relative_to"))
    {
        bases.output = entry(base);
    }
}

/// The resolved specification of a 1.x chain: its layers composed root
/// ancestor first and normalized at the version the chain needs (LAYER-1,
/// P-VER-1). It holds no `parent` and no `clear`, uses the long form of every
/// shorthand, holds only the declarations its roots reach, lists a layered
/// specification's columns in dependency order, keeps root and member fields
/// in schema order, expands every named window (LAYER-14), and records the
/// chain's version in `yamaa`.
#[derive(Debug)]
pub struct ResolvedVersioned {
    pub document: Document,
    /// Parallel to `document`'s nodes: each node's occurrence in the layer
    /// that wrote it, as written, or the schema default it came from.
    pub origins: Vec<LayerNodeOrigin>,
    /// The layer, numbered in contribution order, that contributed each
    /// written path, keyed members named by identity (P-CLR-2).
    pub provenance: Vec<LayerProvenance>,
}

/// Why a 1.x chain does not resolve.
#[derive(Debug)]
pub enum VersionedError<E> {
    /// The host could not respell a path from the entry (LAYER-10).
    Path(E),
    Document(DocumentError),
    /// An admitted layer, numbered in contribution order, did not normalize.
    Layer {
        layer: usize,
        error: NormalizationError,
    },
    /// The layer numbered `layer` lists in `clear` a value it cannot clear:
    /// findings in that layer, each `invalid_clear` at its `clear[i]`
    /// (LAYER-9).
    Clear {
        layer: usize,
        findings: Vec<SchemaDiagnostic>,
    },
    /// A copy entry of the layer numbered `layer` names a dataset that is
    /// not an input linked through `schema`, or a column its producer does
    /// not output: findings in that layer, each at the entry's `dataset` or
    /// listed name (SPEC-9).
    Copy {
        layer: usize,
        findings: Vec<SchemaDiagnostic>,
    },
    /// The producing specification input `dataset` links, which a copy entry
    /// copies from, could not be read; the producer reader
    /// [`prepare_versioned`] was given keeps why (SPEC-9).
    Producer {
        dataset: String,
    },
    /// The composed specification is not valid at the chain's version, a
    /// required field missing among them (LAYER-7, LAYER-12). Findings
    /// address the retained composed document, whose provenance names the
    /// layer that wrote each path.
    Normalize(Box<FailureContext<NormalizationError>>),
    /// A live column reads a name no column declares, or the columns read
    /// each other in a cycle (LAYER-11, LAYER-13).
    Prune(Box<resolution::Error>),
}

impl<E> From<Rebase<E>> for VersionedError<E> {
    fn from(error: Rebase<E>) -> Self {
        match error {
            Rebase::Path(error) => Self::Path(error),
            Rebase::Document(error) => Self::Document(error),
        }
    }
}

/// Set each declaration whose `relative_to` a layer clears back to the
/// declaring layer's directory, the default the resolved specification takes
/// again.
fn clear_bases(document: &Document, bases: &mut Bases) {
    let Some(N::Sequence(entries)) = document
        .field(document.root(), "clear")
        .map(|list| &document.nodes()[list])
    else {
        return;
    };
    for &entry in entries {
        let N::Text(written) = &document.nodes()[entry] else {
            continue;
        };
        match SpecPath::parse(written).as_ref().map(SpecPath::keys) {
            Some([input, name, base]) if input == "input" && base == "relative_to" => {
                bases.inputs.insert(name.clone(), false);
            }
            Some([output, base]) if output == "output" && base == "relative_to" => {
                bases.output = false;
            }
            _ => {}
        }
    }
}

/// The layers of a 1.x chain that [`crate::inheritance::traverse_versioned`]
/// admitted against `schema`, as they compose, in contribution order: each
/// normalized at the version it declares, its copy entries replaced by the
/// columns they declare, read from the producers their inputs link through
/// `producer` (SPEC-9), and its relative paths respelled from the entry
/// through `resolver` (LAYER-10). [`prepare_versioned`] composes them; the
/// lint compares the entry's with what it composes on.
pub fn contributed_layers<R: Resolver>(
    schema: &SchemaStructure,
    chain: &VersionedChain,
    resolver: &mut R,
    producer: impl FnMut(
        &mut R,
        &Source,
        &str,
        &str,
        &[&Source],
    ) -> Option<crate::producer_contract::Contract>,
) -> Result<Vec<NormalizedDocument>, VersionedError<R::Error>> {
    let mut normalized = Vec::with_capacity(chain.layers.len());
    for (index, layer) in chain.layers.iter().enumerate() {
        let view = schema.at_version(layer.version);
        normalized.push(
            view.normalize_versioned_layer(&layer.document)
                .map_err(|error| VersionedError::Layer {
                    layer: index,
                    error,
                })?,
        );
    }
    crate::copy_expansion::expand(schema, chain, &mut normalized, resolver, producer)?;
    let entry = &chain.layers.last().expect("a chain holds its entry").source;
    // The final base of each input and of `output` is selected before any
    // contributing path is respelled; a cleared `relative_to` selects the
    // declaring layer again.
    let mut bases = Bases::default();
    for layer in &normalized {
        clear_bases(&layer.document, &mut bases);
        record_bases(&layer.document, &mut bases);
    }
    for (layer, written) in normalized.iter_mut().zip(&chain.layers) {
        layer.document = rebased(&layer.document, &written.source, entry, &bases, resolver)?;
    }
    Ok(normalized)
}

/// Resolve a 1.x chain that [`crate::inheritance::traverse_versioned`] admitted
/// against `schema`, the 1.0 specification closure: the layers
/// [`contributed_layers`] gives compose. Each layer is normalized
/// at the version it declares, and its copy entries replaced by the columns
/// they declare, read from the producers their inputs link (SPEC-9); its
/// relative paths are respelled from the entry (LAYER-10); the layers
/// compose, each layer's `clear` applied before its own fields (LAYER-9); the
/// chain's version replaces `yamaa`; the composition is pruned and ordered on
/// its 1.0 lowering (LAYER-11, LAYER-13); and what survives is normalized at
/// the chain's version, which materializes defaults and checks requiredness
/// (LAYER-7, LAYER-12).
///
/// `resolver` respells each relative path. `producer` reads the output
/// contract of the producing specification that a copy entry copies columns
/// from (SPEC-9): given `resolver`, the layer that declares the link, the
/// link as written, the input `dataset` it belongs to, and the layers that
/// count as being resolved while it is read, entry first: the entry and each
/// parent layer up to the last one with a copy entry that reads `dataset`.
/// The producer is read and resolved, never compiled or run (INPUT-10); it
/// returns `None` when it cannot be, keeps why, and the chain does not
/// resolve.
pub fn prepare_versioned<R: Resolver>(
    schema: &SchemaStructure,
    chain: &VersionedChain,
    resolver: &mut R,
    producer: impl FnMut(
        &mut R,
        &Source,
        &str,
        &str,
        &[&Source],
    ) -> Option<crate::producer_contract::Contract>,
) -> Result<ResolvedVersioned, VersionedError<R::Error>> {
    let normalized = contributed_layers(schema, chain, resolver, producer)?;
    let view = schema.at_version(chain.version);
    let mut composed = view
        .compose_versioned_layers(&normalized)
        .map_err(|error| match error {
            VersionedCompositionError::Clear { layer, findings } => {
                VersionedError::Clear { layer, findings }
            }
            VersionedCompositionError::Normalization(error) => VersionedError::Layer {
                layer: chain.layers.len() - 1,
                error,
            },
        })?;
    // The resolved specification records the newest version any layer
    // declares, written by the latest layer that declares it (P-VER-1).
    let newest = chain
        .layers
        .iter()
        .rposition(|layer| layer.version == chain.version)
        .expect("the chain's version is a layer's");
    let document = &composed.document;
    if let (Some(value), Some(written)) = (
        document.field(document.root(), "yamaa"),
        normalized[newest]
            .document
            .field(normalized[newest].document.root(), "yamaa"),
    ) {
        let mut nodes = document.nodes().to_vec();
        nodes[value] = N::Text(chain.version.to_string());
        composed.document =
            Document::new(nodes, document.root()).map_err(VersionedError::Document)?;
        composed.node_origins[value] = LayerNodeOrigin {
            layer: Some(newest),
            origin: SchemaOrigin {
                source: SchemaSource::Input,
                node: written,
                generated: false,
            },
        };
        for origin in &mut composed.provenance {
            if origin.path == "yamaa" {
                origin.layer = newest;
            }
        }
    }
    // Every composed node, traced to its occurrence in the layer as written,
    // and whether it is a key that shorthand expansion wrote, which a
    // finding's path names its value through (P-CLR-2).
    let composed_origins = composed
        .node_origins
        .iter()
        .map(|&original| {
            let Some(layer) = original.layer else {
                return original;
            };
            let mut origin = normalized[layer].origins[original.origin.node];
            origin.generated |= original.origin.generated;
            LayerNodeOrigin {
                layer: matches!(origin.source, SchemaSource::Input).then_some(layer),
                origin,
            }
        })
        .collect::<Vec<_>>();
    let composed_shorthand = composed
        .node_origins
        .iter()
        .map(|origin| {
            shorthand_key(
                &normalized,
                origin.layer.map(|input| CompositionOrigin {
                    input,
                    node: origin.origin.node,
                }),
            )
        })
        .collect::<Vec<_>>();
    let failed = |error: NormalizationError, document: Document, shorthand: &[bool]| {
        normalize_failure(error, document, shorthand, &composed.provenance, chain)
    };
    // Named windows expand, keeping unknown references for pruning; the
    // declarations no root reaches are pruned, and a layered specification's
    // columns are ordered by dependency (LAYER-11, LAYER-13); requiredness and
    // defaults apply to what survives (LAYER-7, LAYER-12); and the strict
    // expansion refuses an unknown window and drops the definitions.
    let expanded = match view.expand_named_windows(&composed.document, false) {
        Ok(expanded) => expanded,
        Err(error) => return Err(failed(error, composed.document, &composed_shorthand)),
    };
    let expanded_origins = expanded
        .origins
        .iter()
        .map(|&node| composed_origins[node])
        .collect::<Vec<_>>();
    let expanded_shorthand = expanded
        .origins
        .iter()
        .map(|&node| composed_shorthand[node])
        .collect::<Vec<_>>();
    // A specification that names no parent and includes no fragment is one
    // layer, which keeps its declaration order; that order must be a
    // dependency order (COLUMN-5, LAYER-13, D33).
    let order = if chain.layers.len() > 1 {
        Order::Dependencies
    } else {
        Order::Declared
    };
    let lowered = crate::front_end::v1::spec::lower(&expanded.document, None);
    let pruned = resolution::prune_lowered(&expanded.document, lowered.spec(), order)
        .map_err(|error| VersionedError::Prune(Box::new(error)))?;
    let pruned_origins = pruned
        .origins
        .iter()
        .map(|&origin| LayerNodeOrigin::through(origin, &expanded_origins))
        .collect::<Vec<_>>();
    let pruned_shorthand = pruned
        .origins
        .iter()
        .map(|origin| {
            matches!(origin.source, SchemaSource::Input) && expanded_shorthand[origin.node]
        })
        .collect::<Vec<_>>();
    let resolved = match view.normalize_resolved_root(&pruned.document) {
        Ok(resolved) => resolved,
        Err(error) => return Err(failed(error, pruned.document, &pruned_shorthand)),
    };
    let resolved_origins = resolved
        .origins
        .iter()
        .map(|&origin| LayerNodeOrigin::through(origin, &pruned_origins))
        .collect::<Vec<_>>();
    let strict = match view.expand_named_windows(&resolved.document, true) {
        Ok(strict) => strict,
        Err(error) => {
            let shorthand = resolved
                .origins
                .iter()
                .map(|origin| {
                    matches!(origin.source, SchemaSource::Input) && pruned_shorthand[origin.node]
                })
                .collect::<Vec<_>>();
            return Err(failed(error, resolved.document, &shorthand));
        }
    };
    Ok(ResolvedVersioned {
        origins: strict
            .origins
            .iter()
            .map(|&node| resolved_origins[node])
            .collect(),
        document: strict.document,
        provenance: composed.provenance,
    })
}

/// A resolved 1.x specification's pass failed: its findings address
/// `document`, and a finding inside a value a layer wrote bare names it
/// through its shorthand, as the layer wrote it (P-CLR-2).
fn normalize_failure<E>(
    error: NormalizationError,
    document: Document,
    shorthand: &[bool],
    provenance: &[LayerProvenance],
    chain: &VersionedChain,
) -> VersionedError<E> {
    let error = match error {
        NormalizationError::Invalid(findings) => NormalizationError::Invalid(
            findings
                .into_iter()
                .map(|mut finding| {
                    finding.path = written_path(&document, &|key| shorthand[key], &finding.path);
                    finding
                })
                .collect(),
        ),
        error => error,
    };
    let retained = matches!(&error, NormalizationError::Invalid(_));
    VersionedError::Normalize(Box::new(FailureContext {
        error,
        input: retained.then_some(document),
        provenance: provenance.to_vec(),
        sources: chain
            .layers
            .iter()
            .map(|layer| layer.source.identity.clone())
            .collect(),
    }))
}
