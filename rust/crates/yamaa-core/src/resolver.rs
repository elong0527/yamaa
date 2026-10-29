//! The resolver port: the engine inspects and captures a named resource
//! through it, and it reports what kept a request from reading one. Core
//! reads every study input, specification, layer, dictionary, producer and
//! project file through this one trait. The adapters implement it over one
//! scope's snapshots of the resolver that serves the operation (the local
//! file resolver, the memory resolver or an embedder's bridge), so a resource
//! read twice reads the same bytes or fails as changed (FILE-9, FILE-10).
use crate::{
    diagnostic::Diagnostic, inheritance::SourceError, resource::ResourceFailure,
    specification::SourceDeclaration,
};
use std::{any::Any, sync::Arc};

/// A resource as an inspection located it: the identity the resolver gave it,
/// from which its own relative paths resolve, and the path a finding shows
/// for it.
pub type Located = crate::inheritance::Source;

/// A file the engine reads as metadata, by what it is to the file that names
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum File {
    /// A layer's `parent` (LAYER-2): judged as written before any resolver is
    /// asked for it, and unavailable when it cannot be read.
    Parent,
    /// A file a layer names for its content: a mapping dictionary, or the
    /// producer a `schema` link names.
    Resource,
    /// A file a project names: its lock, a function file or a codelist file.
    Project(crate::project_preparation::Kind),
}

/// A resource a request names.
#[derive(Clone, Copy, Debug)]
pub enum Name<'a> {
    /// A declared input of the specification being run.
    Input(&'a SourceDeclaration),
    /// A specification or project file an operation reads first: the file a
    /// caller names, from the scope's base, or a specification a project's
    /// section lists, as the project file `declaring` writes it.
    Entry {
        declaring: Option<&'a str>,
        written: &'a str,
    },
    /// A metadata file, written as the file `declaring` writes it.
    Written {
        file: File,
        declaring: &'a str,
        written: &'a str,
    },
    /// A metadata file an earlier inspection located.
    Located { file: File, source: &'a Located },
}

/// Where a capture's bytes came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    /// The resource the name reaches.
    Resource,
    /// Bytes the caller already built and holds, such as an artifact an
    /// earlier specification of a project run produced.
    Generated,
    /// A caller-owned frame supplied for one declared input alias.
    Memory,
}

/// What a capture read.
#[derive(Clone)]
pub struct Captured {
    /// The bytes, held for the scope: empty when the resolver served a table.
    pub bytes: Arc<[u8]>,
    /// Whether this capture took a new snapshot, rather than reading one the
    /// scope already held or bytes the caller holds.
    pub created: bool,
    pub origin: Origin,
    /// An immutable table the resolver served in place of bytes, owned
    /// independently of host handles; the decoder admits it as it admits
    /// bytes.
    pub table: Option<Arc<dyn Any + Send + Sync>>,
}

impl Captured {
    /// The bytes of a resource's new snapshot, or of one the scope held.
    pub fn resource(bytes: Arc<[u8]>, created: bool) -> Self {
        Self {
            bytes,
            created,
            origin: Origin::Resource,
            table: None,
        }
    }
}

/// What a failed request reports about its resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// A cause with a finding of its own (FILE-4 to FILE-10).
    Resource(ResourceFailure),
    /// The resource exists and cannot be read.
    Unreadable,
}

impl Failure {
    /// The finding's cause, when the failure has one.
    pub fn resource(self) -> Option<ResourceFailure> {
        match self {
            Self::Resource(failure) => Some(failure),
            Self::Unreadable => None,
        }
    }

    /// Whether the resource cannot be read at all: it is missing, is not a
    /// regular file, or exists and cannot be read. A parent that cannot be
    /// read is unavailable (LAYER-2).
    pub fn unavailable(self) -> bool {
        matches!(
            self,
            Self::Unreadable
                | Self::Resource(ResourceFailure::Missing | ResourceFailure::NotRegularFile)
        )
    }
}

/// The engine's one way to a resource. Inspection reads no bytes and holds no
/// snapshot. A capture holds its bytes for the scope: a later capture of the
/// same resource, by any name that reaches it, reads the same bytes or fails
/// as changed. Implementations return their own errors unchanged; the engine
/// learns a failure's cause only from [`Resolver::failure`].
pub trait Resolver {
    type Error;
    /// Locate the named resource without reading it.
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, Self::Error>;
    /// Read the named resource.
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, Self::Error>;
    /// What a failed request for `name` reports, or `None` when its error
    /// stays opaque: a host's own error, an interruption, or a refusal the
    /// engine reports where it reads the file.
    fn failure(&self, name: Name<'_>, error: &Self::Error) -> Option<Failure>;
    /// Whether an error is a user interrupt, which ends the attempt at once.
    fn is_interrupt(&self, _error: &Self::Error) -> bool {
        false
    }
    /// Respell the path `written` that the layer `layer` writes so that it
    /// names the same location from the entry `entry` (LAYER-10). Lexical:
    /// nothing is inspected or opened.
    fn rebase(
        &mut self,
        layer: &Located,
        entry: &Located,
        written: &str,
    ) -> Result<String, Self::Error>;
    /// The finding for a rebase refused because of where the layer lies, such
    /// as a layer on a network share the entry is not on (LAYER-10), or `None`
    /// when the refusal stays opaque.
    fn rebase_failure(
        &self,
        _error: &Self::Error,
        _layer: &Located,
        _entry: &Located,
        _written: &str,
    ) -> Option<Diagnostic> {
        None
    }
}

/// A failed request as a reader of named metadata files sees it: a file that
/// cannot be read at all, such as a layer's missing `parent`, is unavailable
/// (LAYER-2); any other failure keeps the resolver's error.
pub fn source_error<R: Resolver + ?Sized>(
    resolver: &R,
    name: Name<'_>,
    error: R::Error,
) -> SourceError<R::Error> {
    if resolver
        .failure(name, &error)
        .is_some_and(Failure::unavailable)
    {
        SourceError::Unavailable
    } else {
        SourceError::Raised(error)
    }
}

/// The bytes of a capture, owned.
pub fn bytes(captured: Captured) -> Vec<u8> {
    captured.bytes.to_vec()
}
