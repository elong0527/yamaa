//! Demand-driven inheritance traversal: each parent and each included
//! fragment is located and captured through the resolver port and decoded
//! through the decoder port.
use crate::decoder::Decoder;
use crate::inheritance_capture::{CapturedParent, SourceFailure};
use crate::language::{self, FileType, HeaderFault, LanguageVersion};
use crate::resolver::{self, File, Name, Resolver};
use crate::resource::ResourceFailure;
use crate::schema::{Document, DocumentNode as N, NormalizationError, SchemaStructure};
use std::collections::BTreeSet;

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

/// The field that names a layer a traversal reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    /// The entry, which no layer names.
    Entry,
    /// The parent a caller names for an entry checked as a fragment on it
    /// (#3694). No layer names it either, so a finding about its header is
    /// reported in that file, as the entry's is in the entry.
    CallerParent,
    /// The `parent` of the layer below it on the chain (LAYER-2).
    Parent,
    /// The `include` entry at this index of the layer that includes it
    /// (LAYER-3).
    Include(usize),
}

impl Named {
    /// The spec path of the field that names the layer, in the layer that
    /// writes it; the entry and the parent a caller names have none.
    pub fn field(self) -> Option<String> {
        match self {
            Self::Entry | Self::CallerParent => None,
            Self::Parent => Some("parent".into()),
            Self::Include(index) => Some(format!("include[{index}]")),
        }
    }
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
    /// A `parent` or an `include` entry, the field `named`, reaches a layer
    /// still being composed: `path` runs from that layer up through each
    /// layer above it to the one whose field names it again (LAYER-3).
    Cycle {
        path: Vec<String>,
        returns_to_entry: bool,
        named: Named,
    },
    /// The `include` entry at `index` of `declaring`, written `path`,
    /// reaches `layer`, which the composition already reached through
    /// another field (LAYER-3).
    Repeated {
        declaring: String,
        index: usize,
        path: String,
        layer: String,
    },
    /// The layer `source`, which an `include` entry names, declares `field`,
    /// `parent` or `include`, which a fragment does not (LAYER-4).
    Fragment {
        source: String,
        entry: String,
        field: &'static str,
    },
    /// The file the `include` entry at `index` of `declaring`, written
    /// `path`, names could not be read: missing, not a regular file, a
    /// symbolic link or changed (FILE-7, FILE-8, FILE-10).
    Included {
        declaring: String,
        index: usize,
        path: String,
        failure: ResourceFailure,
    },
    /// The parent a caller names, `path` as the caller gave it, could not be
    /// read: missing, not a regular file or a symbolic link (FILE-1).
    CallerParent {
        path: String,
        failure: ResourceFailure,
    },
    /// The entry checked as a fragment on a parent the caller names declares
    /// `field`, `parent` or `include`, which a fragment does not (LAYER-4),
    /// so that parent, `value` as the caller gave it, is refused before it is
    /// read.
    NotFragment {
        field: &'static str,
        value: String,
    },
    /// A layer declares a header this traversal does not read: a version the
    /// engine does not read, 0.5 included, or a type other than `spec`
    /// (SCHEMA-2, LAYER-4), reported at the entry's header or at the field
    /// `named` that names the layer, which the layer `declaring` writes. The
    /// mismatch is boxed, so the error stays small.
    Header {
        source: String,
        entry: String,
        mismatch: Box<HeaderMismatch>,
        named: Named,
        /// The layer that writes the field `named`; none for the entry, which
        /// no field names.
        declaring: Option<String>,
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
    /// A type other than the one every layer of the chain declares,
    /// `expected`: `spec`, or `project` for a project's layers.
    FileType {
        file_type: String,
        expected: FileType,
    },
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
                named,
            } => F::Cycle {
                path,
                returns_to_entry: *returns_to_entry,
                named: *named,
            },
            Self::Repeated {
                declaring,
                index,
                path,
                layer,
            } => F::Repeated {
                declaring,
                index: *index,
                path,
                layer,
            },
            Self::Fragment {
                source,
                entry,
                field,
            } => F::Fragment {
                source,
                entry,
                field,
            },
            Self::Included {
                declaring,
                index,
                path,
                failure,
            } => return Ok(Some(included_diagnostic(*failure, declaring, *index, path))),
            Self::CallerParent { path, failure } => {
                return Ok(Some(failure.caller_diagnostic(path)))
            }
            Self::NotFragment { field, value } => F::NotFragment { field, value },
            Self::Header {
                source,
                entry,
                mismatch,
                named,
                declaring,
            } => match mismatch.as_ref() {
                HeaderMismatch::Version { version, reads } => F::LanguageVersion {
                    source,
                    entry,
                    version,
                    reads,
                    named: *named,
                    declaring: declaring.as_deref(),
                },
                HeaderMismatch::FileType {
                    file_type,
                    expected,
                } => F::FileType {
                    source,
                    entry,
                    file_type,
                    expected: *expected,
                    named: *named,
                    declaring: declaring.as_deref(),
                },
            },
            _ => return Ok(None),
        }
        .diagnostic()
        .map(Some)
    }
}

/// The finding for the fragment that the `include` entry at `index` of the
/// layer `declaring` names, written `path`, which could not be read for
/// `failure`: FILE-7's, FILE-8's or FILE-10's condition at that entry, its
/// context naming the path as written and the layer that writes it, which
/// locates the entry in that layer (P-ISSUE-1). A resource limit is the
/// engine's refusal at the entry.
pub fn included_diagnostic(
    failure: ResourceFailure,
    declaring: &str,
    index: usize,
    path: &str,
) -> crate::diagnostic::Diagnostic {
    let field = Named::Include(index).field().expect("an include entry");
    declared_diagnostic(failure, &field, declaring, path)
}

/// The finding for the parent that the layer `declaring` names, written
/// `path`, which could not be read for `failure`: FILE-5's, FILE-7's,
/// FILE-8's or FILE-10's condition at its `parent`, its context naming the
/// path as written and the layer that writes it, which locates the field in
/// that layer, not in another layer that also writes a `parent` (P-ISSUE-1).
/// A resource limit is the engine's refusal at the field.
pub fn parent_diagnostic(
    failure: ResourceFailure,
    declaring: &str,
    path: &str,
) -> crate::diagnostic::Diagnostic {
    declared_diagnostic(failure, "parent", declaring, path)
}

/// `failure`'s finding at `field`, naming the path as written and, unless it
/// is a resource limit, the layer `declaring` that writes the field as
/// `source`.
fn declared_diagnostic(
    failure: ResourceFailure,
    field: &str,
    declaring: &str,
    path: &str,
) -> crate::diagnostic::Diagnostic {
    let mut diagnostic = failure.field_diagnostic(field, path);
    if !matches!(failure, ResourceFailure::Boundary(_)) {
        diagnostic.context.insert(
            "source".into(),
            crate::json::Value::String(declaring.into()),
        );
    }
    diagnostic
}

/// The parent a caller names for an entry checked as a fragment on it
/// (#3694): how the resolver is asked for it, as for a file a caller names
/// ([`Name::Entry`]), and the path as the caller gave it, which a finding
/// names (FILE-1).
#[derive(Clone, Copy, Debug)]
pub struct CallerParent<'a> {
    pub written: &'a str,
    pub given: &'a str,
}

/// What an admitted layer names, as written: its `parent`, if any, and its
/// `include` entries in list order.
#[derive(Debug, Default)]
struct Links {
    parent: Option<String>,
    include: Vec<String>,
}

/// One admitted layer of the chain and what it names.
struct Frame<L> {
    identity: String,
    layer: L,
    links: Links,
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

/// Whether a 1.x document of `file_type` is read as one layer of a chain,
/// which need not be complete: a specification always (LAYER-4), and a
/// project file that names a `parent` or clears what it inherits, so that a
/// clear naming nothing it inherits fails as one (PROJECT-16).
pub fn reads_as_layer(file_type: FileType, document: &Document) -> bool {
    match file_type {
        FileType::Spec => true,
        FileType::Project => {
            declares_parent(document) || document.field(document.root(), "clear").is_some()
        }
        FileType::Codelists | FileType::Function => false,
    }
}

/// The traversal error a refused header gives: a malformed header is a
/// finding in the layer, as is the `schema_version` of a layer no field names,
/// the entry or the parent a caller names; a version or type the traversal
/// does not read, a parent's or a fragment's 0.5 included, names the layer,
/// the entry, the field `named` that names the layer and the layer
/// `declaring` that writes that field.
fn header_error<E>(
    fault: HeaderFault,
    entry: &str,
    source: Source,
    input: Document,
    named: Named,
    declaring: Option<&str>,
) -> Error<E> {
    let unnamed = named.field().is_none();
    let mismatch = match fault {
        HeaderFault::Legacy { reads } if !unnamed => HeaderMismatch::Version {
            version: language::LEGACY_LANGUAGE_VERSION.into(),
            reads,
        },
        HeaderFault::Version { version, reads, .. } => HeaderMismatch::Version { version, reads },
        HeaderFault::FileType {
            file_type,
            expected,
            ..
        } => HeaderMismatch::FileType {
            file_type,
            expected,
        },
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
        mismatch: Box::new(mismatch),
        named,
        declaring: declaring.map(str::to_owned),
    }
}

/// Why a layer could not be read: the resolver's own failure, or its
/// decoding.
type ReadFailure<R, D> = SourceFailure<<R as Resolver>::Error, <D as Decoder>::DocumentError>;

/// Decode the captured `bytes` of the layer `source` locates, keep the layer
/// in `layers` as captured and decoded, and return its document as written.
fn decoded<R: Resolver, D: Decoder>(
    decoder: &mut D,
    layers: &mut Vec<CapturedParent<D::Raw>>,
    source: &Source,
    bytes: Vec<u8>,
) -> Result<Document, Error<ReadFailure<R, D>>> {
    let raw = decoder
        .raw(&source.identity, &bytes)
        .map_err(|error| Error::Host(SourceFailure::Admission(error)))?;
    let input = D::raw_document(&raw).clone();
    layers.push(CapturedParent::new(
        crate::project_preparation::Source {
            identity: source.identity.clone(),
            bytes,
        },
        raw,
    ));
    Ok(input)
}

/// Why the request `name` for the fragment the `include` entry at `index` of
/// `declaring` names, written `path`, failed: a missing file, one that is not
/// a regular file, a symbolic link or a changed file is a finding at that
/// entry (FILE-7, FILE-8, FILE-10); any other failure stays the resolver's.
fn included<R: Resolver, X>(
    resolver: &R,
    name: Name<'_>,
    error: R::Error,
    declaring: &str,
    index: usize,
    path: &str,
) -> Error<SourceFailure<R::Error, X>> {
    match resolver
        .failure(name, &error)
        .and_then(resolver::Failure::resource)
    {
        Some(failure) => Error::Included {
            declaring: declaring.into(),
            index,
            path: path.into(),
            failure,
        },
        None => Error::Host(SourceFailure::Host(error)),
    }
}

/// Why the request `name` for the parent a caller names, `given` as the
/// caller gave it, failed: a missing file, one that is not a regular file or
/// a symbolic link is the caller's own path, named as given at no field
/// (FILE-1); any other failure stays the resolver's.
fn caller_parent_error<R: Resolver, X>(
    resolver: &R,
    name: Name<'_>,
    error: R::Error,
    given: &str,
) -> Error<SourceFailure<R::Error, X>> {
    match resolver
        .failure(name, &error)
        .and_then(resolver::Failure::resource)
    {
        Some(failure) => Error::CallerParent {
            path: given.into(),
            failure,
        },
        None => Error::Host(SourceFailure::Host(error)),
    }
}

/// Follow each layer's single parent from the entry, admitting each layer
/// before reading its parent, then read each layer's included fragments, and
/// return every layer in contribution order: at each layer, its parent chain,
/// then its fragments in list order, then the layer itself, so the root
/// ancestor's fragments come first and the entry last (LAYER-3). The chain is
/// followed to its root before any fragment is read; the fragments are read
/// in contribution order, each admitted before the next is read. Every
/// `parent` and `include` entry counts as one link followed.
///
/// A `parent` that reaches a layer already on the chain is a cycle, and so is
/// an `include` entry that reaches the layer that writes it or a layer below
/// it on the chain. An `include` entry that reaches any other layer the
/// composition already reached repeats it. A layer is located as written,
/// then captured and decoded, and kept in `layers`: the parents from the
/// entry up, then the fragments in contribution order. `admit` admits one
/// layer, given the entry identity, the layer's source, its document, the
/// field that names it and the layer that writes that field, and returns the
/// layer with what it names.
///
/// With a `parent` the caller names, the entry is a fragment checked on it
/// (#3694): once the entry is admitted, that file is followed as the entry's
/// parent. It is located and captured as a file the caller names, and one
/// that cannot be read is the caller's path, named as given (FILE-1); one
/// that is the entry itself is a cycle. No layer writes the field that names
/// it. The chain then continues from that file's own parent, so its layers
/// contribute before the entry, as they do before a child that names it as
/// its parent and includes the entry.
fn walk<L, R: Resolver, D: Decoder>(
    entry: Source,
    entry_document: Document,
    parent: Option<CallerParent<'_>>,
    resolver: &mut R,
    decoder: &mut D,
    layers: &mut Vec<CapturedParent<D::Raw>>,
    mut admit: impl FnMut(
        &str,
        Source,
        Document,
        Named,
        Option<&str>,
    ) -> Result<(L, Links), Error<ReadFailure<R, D>>>,
) -> Result<Vec<L>, Error<ReadFailure<R, D>>> {
    if entry.identity.is_empty() || entry.display_path.is_empty() {
        return Err(Error::InvalidIdentity);
    }
    let entry_identity = entry.identity.clone();
    let mut frame = |source: Source, input: Document, named: Named, declaring: Option<&str>| {
        let identity = source.identity.clone();
        admit(&entry_identity, source, input, named, declaring).map(|(layer, links)| Frame {
            identity,
            layer,
            links,
        })
    };
    let mut chain = vec![frame(entry, entry_document, Named::Entry, None)?];
    let mut followed = 0;
    if let Some(parent) = parent {
        followed += 1;
        crate::limits::Limits::parent_occurrences(followed)?;
        let name = Name::Entry {
            declaring: None,
            written: parent.written,
        };
        let source = resolver
            .inspect(name)
            .map_err(|error| caller_parent_error(resolver, name, error, parent.given))?;
        if source.identity.is_empty() || source.display_path.is_empty() {
            return Err(Error::InvalidIdentity);
        }
        if source.identity == entry_identity {
            return Err(Error::Cycle {
                path: vec![entry_identity.clone(), source.identity],
                returns_to_entry: true,
                named: Named::CallerParent,
            });
        }
        let bytes = resolver
            .capture(name)
            .map(resolver::bytes)
            .map_err(|error| caller_parent_error(resolver, name, error, parent.given))?;
        let input = decoded::<R, D>(decoder, layers, &source, bytes)?;
        chain.push(frame(source, input, Named::CallerParent, None)?);
    }
    loop {
        let frame_ref = chain.last_mut().expect("chain holds the entry");
        let Some(written) = frame_ref.links.parent.take() else {
            break;
        };
        followed += 1;
        crate::limits::Limits::parent_occurrences(followed)?;
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
                named: Named::Parent,
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
        let input = decoded::<R, D>(decoder, layers, &source, bytes)?;
        chain.push(frame(source, input, Named::Parent, Some(&declaring))?);
    }
    // Every layer the composition has reached: the chain, then each fragment
    // as it is read.
    let mut reached = chain
        .iter()
        .map(|frame| frame.identity.clone())
        .collect::<BTreeSet<_>>();
    let mut contributed = Vec::with_capacity(chain.len());
    // The root ancestor first: the chain keeps only the layers below the one
    // whose fragments are read.
    while let Some(Frame {
        identity: declaring,
        layer,
        links,
    }) = chain.pop()
    {
        for (index, written) in links.include.into_iter().enumerate() {
            followed += 1;
            crate::limits::Limits::parent_occurrences(followed)?;
            let name = Name::Written {
                file: File::Resource,
                declaring: &declaring,
                written: &written,
            };
            let source = match resolver.inspect(name) {
                Ok(source) => source,
                Err(error) => {
                    return Err(included(resolver, name, error, &declaring, index, &written))
                }
            };
            if source.identity.is_empty() || source.display_path.is_empty() {
                return Err(Error::InvalidIdentity);
            }
            let below = chain.iter().position(|f| f.identity == source.identity);
            if below.is_some() || source.identity == declaring {
                let mut path = Vec::new();
                for frame in &chain[below.unwrap_or(chain.len())..] {
                    path.push(frame.identity.clone());
                }
                path.push(declaring.clone());
                let returns_to_entry = source.identity == entry_identity;
                path.push(source.identity);
                return Err(Error::Cycle {
                    path,
                    returns_to_entry,
                    named: Named::Include(index),
                });
            }
            if !reached.insert(source.identity.clone()) {
                return Err(Error::Repeated {
                    declaring,
                    index,
                    path: written,
                    layer: source.identity,
                });
            }
            let name = Name::Located {
                file: File::Resource,
                source: &source,
            };
            let bytes = match resolver.capture(name) {
                Ok(captured) => resolver::bytes(captured),
                Err(error) => {
                    return Err(included(resolver, name, error, &declaring, index, &written))
                }
            };
            let input = decoded::<R, D>(decoder, layers, &source, bytes)?;
            contributed.push(frame(source, input, Named::Include(index), Some(&declaring))?.layer);
        }
        contributed.push(layer);
    }
    Ok(contributed)
}

/// One admitted 1.x specification layer: its source, its document as written
/// and the language version it declares.
#[derive(Debug, PartialEq)]
pub struct VersionedLayer {
    pub source: Source,
    pub document: Document,
    pub version: LanguageVersion,
}

/// The layers of a 1.x specification in contribution order, the entry last,
/// and the version the chain needs: the newest any layer declares (P-VER-1).
/// At each layer of the chain come its parent chain, then the fragments it
/// includes, then the layer itself (LAYER-3).
#[derive(Debug, PartialEq)]
pub struct VersionedChain {
    pub layers: Vec<VersionedLayer>,
    pub version: LanguageVersion,
}

/// A traversed 1.x chain and each layer it read besides the entry, held as
/// captured and decoded, in the order it read them: the parents from the
/// entry up, then the included fragments in contribution order.
#[derive(Debug)]
pub struct Traversal<R> {
    pub chain: VersionedChain,
    pub parents: Vec<CapturedParent<R>>,
}

/// Follow a 1.x specification's parents from the entry, then read the
/// fragments each layer includes, admitting each layer against the view of
/// `schema`, the 1.0 specification closure, at the version that layer
/// declares (LAYER-3, LAYER-4, P-VER-1). `reads` is the newest version the
/// engine reads. Every layer declares `type: spec`; a layer that declares a
/// version the engine does not read, or another type, fails before what it
/// names is read, at the entry's header or at the `parent` or `include` entry
/// naming it. A layer that declares `schema_version` is written in language
/// version 0.5, which the engine does not read: it fails the same way, naming
/// 0.5. A fragment declares neither `parent` nor `include`. Composition is not
/// part of this traversal. Each layer is located and captured through
/// `resolver` and decoded through `decoder`.
///
/// With a `parent` the caller names, the entry is a fragment checked on it
/// (#3694): an entry that declares `parent` or `include` refuses that file
/// before it is read (LAYER-4), and otherwise that file is followed as the
/// entry's parent, its header refused in that file as the entry's is in the
/// entry.
pub fn traverse_versioned<R: Resolver, D: Decoder>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    entry_document: Document,
    parent: Option<CallerParent<'_>>,
    resolver: &mut R,
    decoder: &mut D,
) -> Result<Traversal<D::Raw>, Error<ReadFailure<R, D>>> {
    traverse(
        (schema, FileType::Spec),
        reads,
        entry,
        entry_document,
        parent,
        resolver,
        decoder,
    )
}

/// Follow a project file's parents from the entry, as
/// [`traverse_versioned`] follows a specification's, admitting each layer
/// against `schema`, the 1.0 project closure, at the version it declares:
/// every layer declares `type: project` (PROJECT-16). A project names no
/// fragment, and no file names a parent for it.
pub fn traverse_project<R: Resolver, D: Decoder>(
    schema: &SchemaStructure,
    reads: LanguageVersion,
    entry: Source,
    entry_document: Document,
    resolver: &mut R,
    decoder: &mut D,
) -> Result<Traversal<D::Raw>, Error<ReadFailure<R, D>>> {
    traverse(
        (schema, FileType::Project),
        reads,
        entry,
        entry_document,
        None,
        resolver,
        decoder,
    )
}

/// The traversal of a chain whose every layer declares `file_type`, each
/// admitted against `schema`, its 1.0 closure.
fn traverse<R: Resolver, D: Decoder>(
    (schema, file_type): (&SchemaStructure, FileType),
    reads: LanguageVersion,
    entry: Source,
    entry_document: Document,
    parent: Option<CallerParent<'_>>,
    resolver: &mut R,
    decoder: &mut D,
) -> Result<Traversal<D::Raw>, Error<ReadFailure<R, D>>> {
    let mut parents = Vec::new();
    let admission = LayerAdmission {
        schema,
        file_type,
        reads,
        given_parent: parent.map(|parent| parent.given),
    };
    let layers = walk(
        entry,
        entry_document,
        parent,
        resolver,
        decoder,
        &mut parents,
        |entry, source, input, named, declaring| {
            admit_versioned(admission, entry, source, input, named, declaring)
        },
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

/// What admits every layer of one versioned traversal: the 1.0
/// specification closure each layer is checked against, the newest version
/// the engine reads, and the parent a caller names, as the caller gave it,
/// for an entry checked as a fragment on it.
#[derive(Clone, Copy)]
struct LayerAdmission<'a> {
    schema: &'a SchemaStructure,
    /// The type every layer declares.
    file_type: FileType,
    reads: LanguageVersion,
    given_parent: Option<&'a str>,
}

/// Admit one layer at the version it declares, named by the field `named` of
/// the layer `declaring`; the admission's `given_parent` says the entry is a
/// fragment checked on it.
fn admit_versioned<E>(
    admission: LayerAdmission<'_>,
    entry: &str,
    source: Source,
    input: Document,
    named: Named,
    declaring: Option<&str>,
) -> Result<(VersionedLayer, Links), Error<E>> {
    let LayerAdmission {
        schema,
        file_type,
        reads,
        given_parent,
    } = admission;
    let header = match language::read_header(&input, Some(reads), file_type) {
        Ok(header) => header,
        Err(fault) => return Err(header_error(fault, entry, source, input, named, declaring)),
    };
    // A fragment composes on the layer that includes it, so it names no
    // layer of its own (LAYER-4). An entry checked on a parent the caller
    // names is a fragment too; one that names a layer refuses that parent.
    let fragment = match named {
        Named::Include(_) => true,
        Named::Entry => given_parent.is_some(),
        Named::CallerParent | Named::Parent => false,
    };
    if fragment {
        if let Some(field) = ["parent", "include"]
            .into_iter()
            .find(|field| input.field(input.root(), field).is_some())
        {
            return Err(match given_parent.filter(|_| named == Named::Entry) {
                Some(given) => Error::NotFragment {
                    field,
                    value: given.into(),
                },
                None => Error::Fragment {
                    source: source.identity,
                    entry: entry.into(),
                    field,
                },
            });
        }
    }
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
    // The admitted layer writes `parent` only as a nonempty path, and
    // `include` only as a list of them.
    let root = input.root();
    let parent = match input.field(root, "parent").map(|node| &input.nodes()[node]) {
        Some(N::Text(parent)) => Some(parent.clone()),
        _ => None,
    };
    let include = match input
        .field(root, "include")
        .map(|node| &input.nodes()[node])
    {
        Some(N::Sequence(items)) => items
            .iter()
            .filter_map(|&item| match &input.nodes()[item] {
                N::Text(path) => Some(path.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok((
        VersionedLayer {
            source,
            document: input,
            version: header.version,
        },
        Links { parent, include },
    ))
}

impl<E: std::fmt::Debug> std::fmt::Display for Error<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "inheritance: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for Error<E> {}
