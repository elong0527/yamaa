//! Demand-driven inheritance traversal; filesystem identity and bytes remain host ports.
use crate::language::{self, FileType, HeaderFault, LanguageVersion};
use crate::schema::{Document, DocumentNode as N, NormalizationError, SchemaStructure};
use std::vec;

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

/// Synchronous source authority. No discovery, retry, concurrency or implicit ambient reader.
pub trait InheritanceSourcePort {
    type Error;
    /// Resolve relative to the declaring canonical file and verify a regular local file.
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Source, SourceError<Self::Error>>;
    /// Read/decode only when requested. Shared schema admission remains in this service.
    fn read(&mut self, source: &Source) -> Result<Document, SourceError<Self::Error>>;
}

/// A normalized contribution, retained in contribution order with its source identity.
#[derive(Debug, PartialEq)]
pub struct Layer {
    pub source: Source,
    pub document: Document,
    pub origins: Vec<crate::schema::SchemaOrigin>,
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
    Version {
        source: String,
        entry: String,
        expected: String,
        actual: String,
        is_entry: bool,
    },
    /// A layer declares a 1.x header this traversal does not read: a version
    /// the engine does not read, or a type other than `spec` (SCHEMA-2, LAYER-4).
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
    /// An admitted custom schema gave control fields a shape this application cannot use.
    UnsupportedControlField {
        source: String,
        field: &'static str,
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
            Self::Version {
                source,
                entry,
                expected,
                actual,
                is_entry,
            } => F::Version {
                source,
                entry,
                expected,
                actual,
                is_entry: *is_entry,
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

fn text_field<'a>(document: &'a Document, name: &str) -> Option<&'a str> {
    document
        .field(document.root(), name)
        .and_then(|node| match &document.nodes()[node] {
            N::Text(value) => Some(value.as_str()),
            _ => None,
        })
}

/// The traversal error a refused 1.x header gives: a malformed header is a
/// finding in the layer, and a version or type the traversal does not read
/// names the layer and the entry.
fn header_error<E>(fault: HeaderFault, entry: &str, source: Source, input: Document) -> Error<E> {
    let is_entry = source.identity == entry;
    let mismatch = match fault {
        HeaderFault::Shape(finding) => {
            return Error::Layer {
                source: source.identity,
                entry: entry.into(),
                input,
                error: NormalizationError::Invalid(vec![finding]),
            };
        }
        HeaderFault::Version { version, reads, .. } => HeaderMismatch::Version { version, reads },
        HeaderFault::FileType { file_type, .. } => HeaderMismatch::FileType { file_type },
    };
    Error::Header {
        source: source.identity,
        entry: entry.into(),
        mismatch,
        is_entry,
    }
}

fn admit<E>(
    schema: &SchemaStructure,
    entry: &str,
    source: Source,
    input: Document,
) -> Result<(Layer, Option<String>), Error<E>> {
    // The 0.5 chain reads no 1.x layer (SCHEMA-2, #3030 D76).
    if let Some(fault) = language::refusal(&input) {
        return Err(header_error(fault, entry, source, input));
    }
    let normalized = match schema.normalize_layer(&input) {
        Ok(value) => value,
        Err(error) => {
            return Err(Error::Layer {
                source: source.identity,
                entry: entry.into(),
                input,
                error,
            });
        }
    };
    let Some(version) = text_field(&normalized.document, "schema_version") else {
        return Err(Error::UnsupportedControlField {
            source: source.identity,
            field: "schema_version",
        });
    };
    if version != schema.version() {
        let is_entry = source.identity == entry;
        return Err(Error::Version {
            source: source.identity,
            entry: entry.into(),
            expected: schema.version().into(),
            actual: version.into(),
            is_entry,
        });
    }
    // A null parent is absent: the layer has no parent.
    let parent = match normalized
        .document
        .field(normalized.document.root(), "parent")
        .map(|node| &normalized.document.nodes()[node])
    {
        None | Some(N::Null) => None,
        Some(N::Text(parent)) => Some(parent.clone()),
        Some(_) => {
            return Err(Error::UnsupportedControlField {
                source: source.identity,
                field: "parent",
            })
        }
    };
    Ok((
        Layer {
            source,
            document: normalized.document,
            origins: normalized.origins,
        },
        parent,
    ))
}

/// Follow each layer's single parent from the entry, admitting each layer before
/// reading its parent, and return the chain in contribution order: the root
/// ancestor first and the entry last. Reaching a layer already on the chain is a
/// cycle. The caller supplies the already captured entry.
pub fn traverse<P: InheritanceSourcePort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
) -> Result<Vec<Layer>, Error<P::Error>> {
    walk(entry, entry_document, port, |entry, source, input| {
        admit(schema, entry, source, input)
    })
}

/// The traversal both languages share. `admit` admits one layer, given the
/// entry identity, the layer's source and its document, and returns the layer
/// with the parent it names.
fn walk<L, P: InheritanceSourcePort>(
    entry: Source,
    entry_document: Document,
    port: &mut P,
    mut admit: impl FnMut(&str, Source, Document) -> Result<(L, Option<String>), Error<P::Error>>,
) -> Result<Vec<L>, Error<P::Error>> {
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
        let source = match port.canonicalize(&declaring, &written) {
            Ok(source) => source,
            Err(SourceError::Unavailable) => {
                return Err(Error::Unavailable {
                    declaring,
                    path: written,
                })
            }
            Err(SourceError::Raised(error)) => return Err(Error::Host(error)),
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
        let input = match port.read(&source) {
            Ok(input) => input,
            Err(SourceError::Unavailable) => {
                return Err(Error::Unavailable {
                    declaring,
                    path: source.display_path,
                })
            }
            Err(SourceError::Raised(error)) => return Err(Error::Host(error)),
        };
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

/// Follow a 1.x specification's parents as [`traverse`] does, admitting each
/// layer against the view of `schema`, the 1.0 specification closure, at the
/// version that layer declares (LAYER-4, P-VER-1). `reads` is the newest
/// version the engine reads. Every layer declares `type: spec`; a layer that
/// declares a version the engine does not read, or another type, fails before
/// its parent is read, at the entry's header or at the `parent` naming it.
/// Composition is not part of this traversal.
pub fn traverse_versioned<P: InheritanceSourcePort>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    entry_document: Document,
    port: &mut P,
) -> Result<VersionedChain, Error<P::Error>> {
    let layers = walk(entry, entry_document, port, |entry, source, input| {
        admit_versioned(schema, reads, entry, source, input)
    })?;
    let version = layers
        .iter()
        .map(|layer| layer.version)
        .max()
        .expect("a chain holds its entry");
    Ok(VersionedChain { layers, version })
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
