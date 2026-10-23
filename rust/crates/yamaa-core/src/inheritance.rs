//! Demand-driven inheritance traversal: each parent is located and captured
//! through the resolver port and decoded through the decoder port.
use crate::decoder::Decoder;
use crate::inheritance_capture::{CapturedParent, SourceFailure};
use crate::language::{self, FileType, HeaderFault, LanguageVersion};
use crate::resolver::{self, File, Name, Resolver};
use crate::schema::{Document, DocumentNode as N, NormalizationError, SchemaStructure};

/// The canonical identity is compared directly. Display spelling retains read-error context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub identity: String,
    pub display_path: String,
}

/// Missing/unreadable/non-file sources have a portable meaning; other host errors stay opaque.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SourceError<E> {
    #[error("inheritance unavailable")]
    Unavailable,
    #[error("inheritance raised: {0:?}")]
    Raised(E),
}

/// No partial contribution list escapes a traversal failure.
#[derive(Debug, PartialEq)]
pub enum Error<E> {
    Host(E),
    Resource(crate::limits::LimitExceeded),
    Unavailable {
        declaring: String,
        path: String,
    },
    InvalidParent {
        declaring: String,
        path: String,
    },
    Cycle {
        path: Vec<String>,
        returns_to_entry: bool,
    },
    /// A layer declares a header this traversal does not read: a version the
    /// engine does not read, 0.5 included, or a type other than `spec`
    /// (SCHEMA-2, LAYER-4).
    Header {
        source: String,
        entry: String,
        mismatch: HeaderMismatch,
        is_entry: bool,
    },
    Layer {
        source: String,
        entry: String,
        input: Document,
        error: NormalizationError,
    },
    InvalidIdentity,
}

/// What a layer's 1.x header declares that its traversal does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeaderMismatch {
    /// A version the engine does not read, beside the versions it reads.
    Version { version: String, reads: Vec<String> },
    /// A type other than `spec`.
    FileType { file_type: String },
}

impl<E> From<crate::limits::LimitExceeded> for Error<E> {
    fn from(error: crate::limits::LimitExceeded) -> Self {
        Self::Resource(error)
    }
}

impl<E> Error<E> {
    /// Project only classified traversal causes. The original error remains
    /// borrowed, including non-Clone host payloads and retained schema inputs.
    pub fn diagnostic(
        &self,
    ) -> Result<Option<crate::diagnostic::Diagnostic>, crate::diagnostic::ProjectionError> {
        use crate::diagnostic::InheritanceFinding as F;
        match self {
            Self::InvalidParent { declaring, path } => F::InvalidParent { declaring, path },
            Self::Unavailable { declaring, path } => F::ParentUnavailable { declaring, path },
            Self::Cycle {
                path,
                returns_to_entry,
            } => F::Cycle {
                path,
                returns_to_entry: *returns_to_entry,
            },
            Self::Header {
                source,
                entry,
                mismatch: HeaderMismatch::Version { version, reads },
                is_entry,
            } => F::LanguageVersion {
                source,
                entry,
                version,
                reads,
                is_entry: *is_entry,
            },
            Self::Header {
                source,
                entry,
                mismatch: HeaderMismatch::FileType { file_type },
                is_entry,
            } => F::FileType {
                source,
                entry,
                file_type,
                is_entry: *is_entry,
            },
            _ => return Ok(None),
        }
        .diagnostic()
        .map(Some)
    }
}

/// One admitted layer and the parent it names, if any.
struct Frame<L> {
    identity: String,
    layer: L,
    parent: Option<String>,
}

/// URI syntax is rejected before granting filesystem authority. Drive-rooted paths stay local.
pub fn is_local_parent(value: &str) -> bool {
    let b = value.as_bytes();
    if b.is_empty() {
        return false;
    }
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\') {
        return true;
    }
    if !b[0].is_ascii_alphabetic() {
        return true;
    }
    for &c in &b[1..] {
        if c == b':' {
            return false;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-')) {
            break;
        }
    }
    true
}

/// A specification inherits when its raw entry declares a non-null `parent`;
/// a null parent is absent, and the entry is then a composition of one layer.
pub fn declares_parent(document: &Document) -> bool {
    document
        .field(document.root(), "parent")
        .is_some_and(|node| !matches!(document.nodes()[node], N::Null))
}

/// The traversal error a refused header gives: a malformed header is a
/// finding in the layer, as is the entry's `schema_version`; a version or type
/// the traversal does not read, a parent's 0.5 included, names the layer and
/// the entry.
fn header_error<E>(fault: HeaderFault, entry: &str, source: Source, input: Document) -> Error<E> {
    let is_entry = source.identity == entry;
    let mismatch = match fault {
        HeaderFault::Legacy { reads } if !is_entry => HeaderMismatch::Version {
            version: language::LEGACY_LANGUAGE_VERSION.into(),
            reads,
        },
        HeaderFault::Version { version, reads, .. } => HeaderMismatch::Version { version, reads },
        HeaderFault::FileType { file_type, .. } => HeaderMismatch::FileType { file_type },
        HeaderFault::Shape(_) | HeaderFault::Legacy { .. } => {
            return Error::Layer {
                source: source.identity,
                entry: entry.into(),
                input,
                error: NormalizationError::Invalid(vec![fault.finding()]),
            };
        }
    };
    Error::Header {
        source: source.identity,
        entry: entry.into(),
        mismatch,
        is_entry,
    }
}

/// Why a parent could not be read: the resolver's own failure, or its
/// decoding.
type ReadFailure<R, D> = SourceFailure<<R as Resolver>::Error, <D as Decoder>::DocumentError>;

/// Follow each layer's single parent from the entry, admitting each layer
/// before reading its parent, and return the chain in contribution order:
/// the root ancestor first and the entry last. Reaching a layer already on
/// the chain is a cycle. A parent is located as written, then captured and
/// decoded, and kept in `parents`. `admit` admits one layer, given the entry
/// identity, the layer's source and its document, and returns the layer with
/// the parent it names.
fn walk<L, R: Resolver, D: Decoder>(
    entry: Source,
    entry_document: Document,
    resolver: &mut R,
    decoder: &mut D,
    parents: &mut Vec<CapturedParent<D::Raw>>,
    mut admit: impl FnMut(
        &str,
        Source,
        Document,
    ) -> Result<(L, Option<String>), Error<ReadFailure<R, D>>>,
) -> Result<Vec<L>, Error<ReadFailure<R, D>>> {
    if entry.identity.is_empty() || entry.display_path.is_empty() {
        return Err(Error::InvalidIdentity);
    }
    let entry_identity = entry.identity.clone();
    let mut frame = |source: Source, input: Document| {
        let identity = source.identity.clone();
        admit(&entry_identity, source, input).map(|(layer, parent)| Frame {
            identity,
            layer,
            parent,
        })
    };
    let mut chain = vec![frame(entry, entry_document)?];
    let mut parent_occurrences = 0;
    loop {
        let frame_ref = chain.last_mut().expect("chain holds the entry");
        let Some(written) = frame_ref.parent.take() else {
            break;
        };
        parent_occurrences += 1;
        crate::limits::Limits::parent_occurrences(parent_occurrences)?;
        let declaring = frame_ref.identity.clone();
        if !is_local_parent(&written) {
            return Err(Error::InvalidParent {
                declaring,
                path: written,
            });
        }
        let name = Name::Written {
            file: File::Parent,
            declaring: &declaring,
            written: &written,
        };
        let source = match resolver.inspect(name) {
            Ok(source) => source,
            Err(error) => match resolver::source_error(resolver, name, error) {
                SourceError::Unavailable => {
                    return Err(Error::Unavailable {
                        declaring,
                        path: written,
                    })
                }
                SourceError::Raised(error) => return Err(Error::Host(SourceFailure::Host(error))),
            },
        };
        if source.identity.is_empty() || source.display_path.is_empty() {
            return Err(Error::InvalidIdentity);
        }
        if let Some(first) = chain.iter().position(|f| f.identity == source.identity) {
            let mut path = Vec::new();
            for frame in &chain[first..] {
                path.push(frame.identity.clone());
            }
            let returns_to_entry = source.identity == entry_identity;
            path.push(source.identity);
            return Err(Error::Cycle {
                path,
                returns_to_entry,
            });
        }
        let name = Name::Located {
            file: File::Parent,
            source: &source,
        };
        let bytes = match resolver.capture(name) {
            Ok(captured) => resolver::bytes(captured),
            Err(error) => match resolver::source_error(resolver, name, error) {
                SourceError::Unavailable => {
                    return Err(Error::Unavailable {
                        declaring,
                        path: source.display_path,
                    })
                }
                SourceError::Raised(error) => return Err(Error::Host(SourceFailure::Host(error))),
            },
        };
        let raw = decoder
            .raw(&source.identity, &bytes)
            .map_err(|error| Error::Host(SourceFailure::Admission(error)))?;
        let input = D::raw_document(&raw).clone();
        parents.push(CapturedParent::new(
            crate::project_preparation::Source {
                identity: source.identity.clone(),
                bytes,
            },
            raw,
        ));
        chain.push(frame(source, input)?);
    }
    Ok(chain.into_iter().rev().map(|frame| frame.layer).collect())
}

/// One admitted 1.x specification layer: its source, its document as written
/// and the language version it declares.
#[derive(Debug, PartialEq)]
pub struct VersionedLayer {
    pub source: Source,
    pub document: Document,
    pub version: LanguageVersion,
}

/// The layers of a 1.x specification in contribution order, the root ancestor
/// first and the entry last, and the version the chain needs: the newest any
/// layer declares (P-VER-1).
#[derive(Debug, PartialEq)]
pub struct VersionedChain {
    pub layers: Vec<VersionedLayer>,
    pub version: LanguageVersion,
}

/// A traversed 1.x chain and each parent layer it read, held as captured and
/// decoded, in the order it read them: from the entry up.
#[derive(Debug)]
pub struct Traversal<R> {
    pub chain: VersionedChain,
    pub parents: Vec<CapturedParent<R>>,
}

/// Follow a 1.x specification's parents from the entry, admitting each
/// layer against the view of `schema`, the 1.0 specification closure, at the
/// version that layer declares (LAYER-4, P-VER-1). `reads` is the newest
/// version the engine reads. Every layer declares `type: spec`; a layer that
/// declares a version the engine does not read, or another type, fails before
/// its parent is read, at the entry's header or at the `parent` naming it.
/// A layer that declares `schema_version` is written in language version
/// 0.5, which the engine does not read: it fails the same way, naming 0.5.
/// Composition is not part of this traversal. Each parent is located and
/// captured through `resolver` and decoded through `decoder`.
pub fn traverse_versioned<R: Resolver, D: Decoder>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    entry_document: Document,
    resolver: &mut R,
    decoder: &mut D,
) -> Result<Traversal<D::Raw>, Error<ReadFailure<R, D>>> {
    let mut parents = Vec::new();
    let layers = walk(
        entry,
        entry_document,
        resolver,
        decoder,
        &mut parents,
        |entry, source, input| admit_versioned(schema, reads, entry, source, input),
    )?;
    let version = layers
        .iter()
        .map(|layer| layer.version)
        .max()
        .expect("a chain holds its entry");
    Ok(Traversal {
        chain: VersionedChain { layers, version },
        parents,
    })
}

fn admit_versioned<E>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: &str,
    source: Source,
    input: Document,
) -> Result<(VersionedLayer, Option<String>), Error<E>> {
    let header = match language::read_header(&input, Some(reads), FileType::Spec) {
        Ok(header) => header,
        Err(fault) => return Err(header_error(fault, entry, source, input)),
    };
    let findings = schema.at_version(header.version).validate_layer(&input);
    let error = match findings {
        Ok(findings) if findings.is_empty() => None,
        Ok(findings) => Some(NormalizationError::Invalid(findings)),
        Err(error) => Some(NormalizationError::Validation(error)),
    };
    if let Some(error) = error {
        return Err(Error::Layer {
            source: source.identity,
            entry: entry.into(),
            input,
            error,
        });
    }
    // The admitted layer writes `parent` only as a nonempty path.
    let parent = match input
        .field(input.root(), "parent")
        .map(|node| &input.nodes()[node])
    {
        Some(N::Text(parent)) => Some(parent.clone()),
        _ => None,
    };
    Ok((
        VersionedLayer {
            source,
            document: input,
            version: header.version,
        },
        parent,
    ))
}

impl<E: std::fmt::Debug> std::fmt::Display for Error<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "inheritance: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for Error<E> {}
