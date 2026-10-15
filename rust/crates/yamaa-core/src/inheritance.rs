//! Demand-driven inheritance traversal; filesystem identity and bytes remain host ports.
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
pub trait SourcePort {
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

/// A normalized contribution, retained in deterministic postorder with its source identity.
#[derive(Debug, PartialEq)]
pub struct Layer {
    pub source: Source,
    pub document: Document,
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
            _ => return Ok(None),
        }
        .diagnostic()
        .map(Some)
    }
}

struct Frame {
    layer: Layer,
    parents: Vec<String>,
    next: usize,
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

fn text_field<'a>(document: &'a Document, name: &str) -> Option<&'a str> {
    document
        .field(document.root(), name)
        .and_then(|node| match &document.nodes()[node] {
            N::Text(value) => Some(value.as_str()),
            _ => None,
        })
}

fn admit<E>(
    schema: &SchemaStructure,
    entry: &str,
    source: Source,
    input: Document,
) -> Result<Frame, Error<E>> {
    let normalized = match schema.normalize_layer(&input) {
        Ok(value) => value.document,
        Err(error) => {
            return Err(Error::Layer {
                source: source.identity,
                entry: entry.into(),
                input,
                error,
            });
        }
    };
    let Some(version) = text_field(&normalized, "schema_version") else {
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
    let mut parents = Vec::new();
    if let Some(node) = normalized.field(normalized.root(), "parents") {
        let N::Sequence(items) = &normalized.nodes()[node] else {
            return Err(Error::UnsupportedControlField {
                source: source.identity,
                field: "parents",
            });
        };
        for &child in items {
            let N::Text(parent) = &normalized.nodes()[child] else {
                return Err(Error::UnsupportedControlField {
                    source: source.identity,
                    field: "parents",
                });
            };
            parents.push(parent.clone());
        }
    }
    Ok(Frame {
        layer: Layer {
            source,
            document: normalized,
        },
        parents,
        next: 0,
    })
}

/// Visit normalized parents left-to-right, admit each unique layer before reading its parents,
/// and return once-only postorder contributions. The caller supplies the already captured entry.
pub fn traverse<P: SourcePort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
) -> Result<Vec<Layer>, Error<P::Error>> {
    if entry.identity.is_empty() || entry.display_path.is_empty() {
        return Err(Error::InvalidIdentity);
    }
    let entry_identity = entry.identity.clone();
    let mut active = vec![admit(schema, &entry_identity, entry, entry_document)?];
    let mut completed: Vec<Layer> = Vec::new();
    let mut parent_occurrences = 0;
    while let Some(frame) = active.last_mut() {
        if frame.next == frame.parents.len() {
            completed.push(active.pop().expect("active frame").layer);
            continue;
        }
        parent_occurrences += 1;
        crate::limits::Limits::parent_occurrences(parent_occurrences)?;
        // Each written occurrence is visited, including already captured parents.
        let written = &frame.parents[frame.next];
        let written = written.clone();
        let declaring = frame.layer.source.identity.clone();
        frame.next += 1;
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
        if let Some(first) = active
            .iter()
            .position(|f| f.layer.source.identity == source.identity)
        {
            let mut path = Vec::new();
            for frame in &active[first..] {
                path.push(frame.layer.source.identity.clone());
            }
            let returns_to_entry = source.identity == entry_identity;
            path.push(source.identity);
            return Err(Error::Cycle {
                path,
                returns_to_entry,
            });
        }
        if completed
            .iter()
            .any(|layer| layer.source.identity == source.identity)
        {
            continue;
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
        active.push(admit(schema, &entry_identity, source, input)?);
    }
    Ok(completed)
}

impl<E: std::fmt::Debug> std::fmt::Display for Error<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "inheritance: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for Error<E> {}
