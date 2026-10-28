//! The layers of a project file that names a `parent` (PROJECT-16), composed
//! by the machinery a specification's layers compose with: the chain is
//! followed from the entry ([`inheritance::traverse_project`]), each layer
//! admitted as a partial project and normalized, each relative path it
//! writes respelled from the entry so that it names the location beside the
//! file that writes it (PROJECT-4, LAYER-10), and the layers composed root
//! ancestor first, each after its `clear` (LAYER-5, LAYER-6, LAYER-9). The
//! composed project is then admitted as a complete project file (LAYER-7).
use crate::decoder::Decoder;
use crate::file_preparation::AliasCache;
use crate::inheritance::{self, Source};
use crate::inheritance_capture::{CapturedParent, SourceFailure};
use crate::language::LanguageVersion;
use crate::resolver::Resolver;
use crate::schema::{
    Document, DocumentNode as N, NormalizationError, SchemaDiagnostic, SchemaStructure,
    VersionedCompositionError,
};

/// A composed project and the parent layers read for it.
#[derive(Debug)]
pub struct Composed<R> {
    /// The composed project, admitted and normalized as a complete project
    /// file.
    pub document: Document,
    /// Each parent layer, from the entry up, as captured and decoded.
    pub parents: Vec<CapturedParent<R>>,
}

/// Why the layers of a project do not compose.
#[derive(Debug)]
pub enum Failure<E, X> {
    /// A parent could not be followed, read or admitted (LAYER-2, LAYER-3,
    /// LAYER-4).
    Traversal(inheritance::Error<SourceFailure<E, X>>),
    /// The layer `source`, written as `document`, lists in `clear` a value it
    /// cannot clear: each finding is `invalid_clear` at its `clear[i]`
    /// (LAYER-9).
    Clear {
        source: Source,
        document: Document,
        findings: Vec<SchemaDiagnostic>,
    },
    /// The resolver refused to respell a path a parent layer writes
    /// (LAYER-10).
    Path(E),
    /// A layer, or the composed project, is not what its schema admits; the
    /// findings of the composed project address `document`.
    Normalize {
        document: Option<Document>,
        error: NormalizationError,
    },
    /// A `parent` the resolver refused as written, or a layer it would not
    /// respell, with the finding it classified (FILE-5, FILE-8, FILE-10,
    /// LAYER-10).
    Finding(Box<crate::diagnostic::Diagnostic>),
}

/// A composed project, or why its layers do not compose, read through the
/// resolver `R` and the decoder `D`.
pub type Composition<R, D> = Result<
    Composed<<D as Decoder>::Raw>,
    Failure<<R as Resolver>::Error, <D as Decoder>::DocumentError>,
>;

/// The nodes of the relative paths a project layer writes that resolve from
/// the file that writes them (PROJECT-4): each function file, host lock,
/// codelist file, section specification and Define-XML target. A section's
/// `stylesheet` and each document's `link` are relative to the generated
/// document instead, and are written as they are.
fn paths(document: &Document) -> Vec<usize> {
    let texts = |list: Option<usize>| match list.map(|node| &document.nodes()[node]) {
        Some(N::Sequence(items)) => items
            .iter()
            .copied()
            .filter(|&item| matches!(document.nodes()[item], N::Text(_)))
            .collect(),
        _ => Vec::new(),
    };
    let root = document.root();
    let mut paths = texts(document.field(root, "functions"));
    if let Some(N::Mapping(hosts)) = document
        .field(root, "environments")
        .map(|node| &document.nodes()[node])
    {
        paths.extend(
            hosts
                .iter()
                .filter_map(|&(_, host)| document.field(host, "lock")),
        );
    }
    paths.extend(texts(document.field(root, "codelists")));
    for section in ["sdtm", "adam", "send"] {
        if let Some(section) = document.field(root, section) {
            paths.extend(texts(document.field(section, "specs")));
            paths.extend(document.field(section, "define"));
        }
    }
    paths
}

/// The layer `source`, written as `document`, with each of its relative
/// paths respelled from the entry (LAYER-10). Node indices are kept, so
/// origins addressing the layer still hold.
fn rebased<R: Resolver, X>(
    document: &Document,
    source: &Source,
    entry: &Source,
    resolver: &mut R,
) -> Result<Document, Failure<R::Error, X>> {
    let mut nodes = document.nodes().to_vec();
    for node in paths(document) {
        if let N::Text(written) = &document.nodes()[node] {
            let path = resolver
                .rebase(source, entry, written)
                .map_err(Failure::Path)?;
            nodes[node] = N::Text(path);
        }
    }
    Document::new(nodes, document.root()).map_err(|error| Failure::Normalize {
        document: None,
        error: NormalizationError::Document(error),
    })
}

/// Compose the project file `entry`, written as `document`, which names a
/// `parent`, with its parent chain against `schema`, the 1.0 project closure;
/// `reads` is the newest version the engine reads. Each parent is located
/// and captured through `resolver` and decoded through `decoder`.
pub fn compose<R: Resolver, D: Decoder>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    document: Document,
    resolver: &mut R,
    decoder: &mut D,
) -> Composition<R, D> {
    // Each parent is captured by the spelling that located it, and a refusal
    // the resolver classified is its finding, as for a specification's
    // layers.
    let mut aliases = AliasCache::new(resolver);
    compose_through(schema, reads, entry, document, &mut aliases, decoder).map_err(|failure| {
        match aliases.refused(failure) {
            crate::file_preparation::Error::PreparationFinding { diagnostic, .. } => {
                Failure::Finding(diagnostic)
            }
            crate::file_preparation::Error::Preparation(failure) => failure,
            _ => unreachable!("a refused composition keeps its failure"),
        }
    })
}

/// [`compose`] through the resolver `resolver`.
fn compose_through<R: Resolver, D: Decoder>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    document: Document,
    resolver: &mut R,
    decoder: &mut D,
) -> Composition<R, D> {
    let inheritance::Traversal { chain, parents } =
        inheritance::traverse_project(schema, reads, entry.clone(), document, resolver, decoder)
            .map_err(Failure::Traversal)?;
    let mut normalized = Vec::with_capacity(chain.layers.len());
    for layer in &chain.layers {
        let mut written = schema
            .at_version(layer.version)
            .normalize_versioned_layer(&layer.document)
            .map_err(|error| Failure::Normalize {
                document: None,
                error,
            })?;
        // The entry's paths are already written from the entry.
        if layer.source.identity != entry.identity {
            written.document = rebased(&written.document, &layer.source, &entry, resolver)?;
        }
        normalized.push(written);
    }
    let view = schema.at_version(chain.version);
    let composed = view
        .compose_versioned_layers(&normalized)
        .map_err(|error| match error {
            VersionedCompositionError::Clear { layer, findings } => {
                let layer = &chain.layers[layer];
                Failure::Clear {
                    source: layer.source.clone(),
                    document: layer.document.clone(),
                    findings,
                }
            }
            VersionedCompositionError::Normalization(error) => Failure::Normalize {
                document: None,
                error,
            },
        })?;
    // The composed project records the newest version any layer declares
    // (P-VER-1).
    let mut nodes = composed.document.nodes().to_vec();
    if let Some(version) = composed.document.field(composed.document.root(), "yamaa") {
        nodes[version] = N::Text(chain.version.to_string());
    }
    let document =
        Document::new(nodes, composed.document.root()).map_err(|error| Failure::Normalize {
            document: None,
            error: NormalizationError::Document(error),
        })?;
    // Requiredness and defaults apply to the composed project (LAYER-7).
    let complete = view
        .normalize_root(&document)
        .map_err(|error| Failure::Normalize {
            document: Some(document.clone()),
            error,
        })?;
    Ok(Composed {
        document: complete.document,
        parents,
    })
}
