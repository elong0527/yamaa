//! Facade-owned host resource authority.
//!
//! Every operation reads through one resolver: the engine's own local file
//! resolver unless [`crate::EngineOptions::with_resolver`] supplies another.
//! A resolver may serve local objects, opaque URI identities or warehouse
//! data. References are immutable host identities, never content digests.
//! Only public resource facts cross this boundary.

use std::sync::Arc;

use crate::table::InputFrame;
use yamaa_adapters::host_resources::Resolver as AdapterResolver;

/// A resolver operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolverOperation {
    Inspect,
    Capture,
    Target,
    Stage,
    Commit,
    Abort,
}

impl ResolverOperation {
    /// Return the stable operation name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Capture => "capture",
            Self::Target => "target",
            Self::Stage => "stage",
            Self::Commit => "commit",
            Self::Abort => "abort",
        }
    }

    pub(crate) fn into_core(self) -> yamaa_adapters::host_resources::Operation {
        use yamaa_adapters::host_resources::Operation as Core;
        match self {
            Self::Inspect => Core::Inspect,
            Self::Capture => Core::Capture,
            Self::Target => Core::Target,
            Self::Stage => Core::Stage,
            Self::Commit => Core::Commit,
            Self::Abort => Core::Abort,
        }
    }
}

/// A resolver resource kind.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolverKind {
    Metadata,
    Input,
    Output,
}

impl ResolverKind {
    /// Return the stable kind name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Input => "input",
            Self::Output => "output",
        }
    }

    pub(crate) fn into_core(self) -> yamaa_adapters::host_resources::Kind {
        use yamaa_adapters::host_resources::Kind as Core;
        match self {
            Self::Metadata => Core::Metadata,
            Self::Input => Core::Input,
            Self::Output => Core::Output,
        }
    }
}

/// The kind of entry a name reaches. A resolver serves a regular file; for
/// any other name it fails with [`ResolverFailure::from_kind`], naming the
/// first entry that stops the name. The engine raises the file rules'
/// conditions from that report alone: a link of any sort, a junction or
/// another reparse point included, fails as `resource_path_symlink` (FILE-8),
/// any other entry that is not a regular file as
/// `resource_path_not_regular_file` (FILE-7), and a missing name as
/// `resource_path_missing`.
///
/// [`ResolverFailure::from_kind`]: crate::error::ResolverFailure::from_kind
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// A regular file, reported where a directory is required.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    SymbolicLink,
    /// A Windows directory junction.
    Junction,
    /// Any other Windows reparse point.
    ReparsePoint,
    /// A FIFO, socket, device or any other entry.
    Other,
    /// No entry.
    Missing,
}

impl ResourceKind {
    /// Return the stable kind name, which is also the failure code that
    /// reports it.
    pub fn name(self) -> &'static str {
        self.into_core().name()
    }

    pub(crate) fn into_core(self) -> yamaa_adapters::host_resources::ResourceKind {
        use yamaa_adapters::host_resources::ResourceKind as Core;
        match self {
            Self::File => Core::File,
            Self::Directory => Core::Directory,
            Self::SymbolicLink => Core::SymbolicLink,
            Self::Junction => Core::Junction,
            Self::ReparsePoint => Core::ReparsePoint,
            Self::Other => Core::Other,
            Self::Missing => Core::Missing,
        }
    }

    pub(crate) fn from_core(kind: yamaa_adapters::host_resources::ResourceKind) -> Self {
        use yamaa_adapters::host_resources::ResourceKind as Core;
        match kind {
            Core::File => Self::File,
            Core::Directory => Self::Directory,
            Core::SymbolicLink => Self::SymbolicLink,
            Core::Junction => Self::Junction,
            Core::ReparsePoint => Self::ReparsePoint,
            Core::Other => Self::Other,
            Core::Missing => Self::Missing,
        }
    }
}

/// One resolver request. The held reference rides freshness and publication
/// requests; the host must reject a changed reference.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolverRequest<'a> {
    pub operation: ResolverOperation,
    pub kind: ResolverKind,
    pub declaring: &'a str,
    pub written: &'a str,
    pub reference: Option<&'a str>,
    pub bytes: Option<&'a [u8]>,
}

/// One closed resolver reply: the immutable reference plus capture bytes or
/// an admitted host frame. Inspect and target replies carry neither bytes
/// nor frames.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ResolverReply {
    pub reference: String,
    pub bytes: Option<Arc<[u8]>>,
    pub frame: Option<InputFrame>,
}

impl ResolverReply {
    /// Reply with an immutable reference plus optional capture payload.
    pub fn new(reference: String, bytes: Option<Arc<[u8]>>, frame: Option<InputFrame>) -> Self {
        Self {
            reference,
            bytes,
            frame,
        }
    }
}

/// Host resource authority. Implementations must return closed failures and
/// must not panic. Inspect and capture serve a regular file or report the
/// [`ResourceKind`] the name reaches. Stage creates a private provisional
/// object, commit atomically publishes it, and abort removes it after any
/// failed or interrupting stage or commit.
pub trait ResourceResolver: Send + Sync {
    /// Serve one resource request on the calling thread of the operation.
    fn request(
        &self,
        request: ResolverRequest<'_>,
    ) -> Result<ResolverReply, crate::error::ResolverFailure>;
}

/// Deterministic object store for tests and host-owned adapters. Replacing a
/// value with a new reference models a remote object-version change.
#[non_exhaustive]
#[derive(Default)]
pub struct MemoryResolver {
    inner: yamaa_adapters::host_resources::MemoryResolver,
}

impl MemoryResolver {
    /// Create an empty memory resolver.
    pub fn new() -> Self {
        Self {
            inner: yamaa_adapters::host_resources::MemoryResolver::default(),
        }
    }

    /// Serve one captured input frame under its declaring and written names.
    pub fn insert_frame(
        &self,
        declaring: &str,
        written: &str,
        reference: &str,
        frame: &InputFrame,
    ) {
        self.inner
            .insert_frame(declaring, written, reference, frame.clone().into_adapter());
    }

    /// Serve one captured byte object under its declaring and written names.
    pub fn insert(&self, declaring: &str, written: &str, reference: &str, bytes: Arc<[u8]>) {
        self.inner.insert(declaring, written, reference, bytes);
    }

    /// Report `kind` for these declaring and written names instead of
    /// serving them, as a file resolver reports a directory or a link.
    pub fn insert_kind(&self, declaring: &str, written: &str, kind: ResourceKind) {
        self.inner.insert_kind(declaring, written, kind.into_core());
    }

    /// Cancel every operation served through this resolver.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Borrow published bytes by target reference.
    pub fn published(&self, reference: &str) -> Option<Arc<[u8]>> {
        self.inner.published(reference)
    }

    /// Count staged provisional objects.
    pub fn staged_count(&self) -> usize {
        self.inner.staged_count()
    }
}

impl ResourceResolver for MemoryResolver {
    fn request(
        &self,
        request: ResolverRequest<'_>,
    ) -> Result<ResolverReply, crate::error::ResolverFailure> {
        use crate::error::ResolverFailure;
        self.inner
            .request(yamaa_adapters::host_resources::Request {
                operation: request.operation.into_core(),
                kind: request.kind.into_core(),
                declaring: request.declaring,
                written: request.written,
                reference: request.reference,
                bytes: request.bytes,
                limit: None,
            })
            .map(|reply| ResolverReply {
                reference: reply.reference,
                bytes: reply.bytes,
                frame: reply.frame.map(InputFrame::from_adapter_table),
            })
            .map_err(ResolverFailure::from_adapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_and_kind_names_are_stable() {
        assert_eq!(ResolverOperation::Capture.name(), "capture");
        assert_eq!(ResolverKind::Input.name(), "input");
        let names = [
            (ResourceKind::File, "file"),
            (ResourceKind::Directory, "directory"),
            (ResourceKind::SymbolicLink, "symbolic_link"),
            (ResourceKind::Junction, "junction"),
            (ResourceKind::ReparsePoint, "reparse_point"),
            (ResourceKind::Other, "other"),
            (ResourceKind::Missing, "missing"),
        ];
        for (kind, name) in names {
            assert_eq!(kind.name(), name);
            assert_eq!(ResourceKind::from_core(kind.into_core()), kind);
        }
    }

    #[test]
    fn memory_kind_facts_fail_with_their_kind() {
        let resolver = MemoryResolver::new();
        resolver.insert_kind("spec", "input/link.csv", ResourceKind::SymbolicLink);
        let failure = resolver
            .request(ResolverRequest {
                operation: ResolverOperation::Inspect,
                kind: ResolverKind::Input,
                declaring: "spec",
                written: "input/link.csv",
                reference: None,
                bytes: None,
            })
            .unwrap_err();
        assert_eq!(failure.code(), "symbolic_link");
        assert_eq!(failure.kind(), Some(ResourceKind::SymbolicLink));
    }

    #[test]
    fn memory_resolvers_serve_bytes_and_track_publication() {
        let resolver = MemoryResolver::new();
        assert_eq!(resolver.staged_count(), 0);
        assert!(resolver.published("memory:/out.csv").is_none());
        resolver.insert("spec", "input.csv", "v1", Arc::from(b"id,x" as &[u8]));
        let reply = resolver
            .request(ResolverRequest {
                operation: ResolverOperation::Capture,
                kind: ResolverKind::Input,
                declaring: "spec",
                written: "input.csv",
                reference: None,
                bytes: None,
            })
            .unwrap();
        assert_eq!(reply.reference, "v1");
        assert_eq!(reply.bytes.as_deref(), Some(b"id,x" as &[u8]));
        assert!(reply.frame.is_none());
        let reply = ResolverReply::new("v1".into(), None, None);
        assert_eq!(reply.reference, "v1");
        let missing = resolver.request(ResolverRequest {
            operation: ResolverOperation::Capture,
            kind: ResolverKind::Input,
            declaring: "spec",
            written: "absent.csv",
            reference: None,
            bytes: None,
        });
        assert!(missing.is_err());
        resolver.cancel();
        let cancelled = resolver.request(ResolverRequest {
            operation: ResolverOperation::Inspect,
            kind: ResolverKind::Metadata,
            declaring: "spec",
            written: "input.csv",
            reference: None,
            bytes: None,
        });
        assert_eq!(cancelled.unwrap_err().code(), "cancelled");
    }
}
