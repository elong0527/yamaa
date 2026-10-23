//! Host resource authority, separate from codecs, credentials and language rules.
//!
//! Every resource an operation reads goes through one [`Resolver`]: the local
//! file resolver ([`crate::file_resolver::FileResolver`]) unless the caller
//! brings its own, such as a [`MemoryResolver`]. References are immutable host
//! identities, never content digests. Captures retain their original bytes and
//! compare those bytes on every freshness check.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Inspect,
    Capture,
    Target,
    Stage,
    Commit,
    Abort,
}
impl Operation {
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Metadata,
    Input,
    Output,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}
/// The kind of entry a name reaches (FILE-7, FILE-8). A resolver serves a
/// regular file; for any other name it reports, with [`Failure::Kind`], the
/// kind of the first entry that stops the name: a link of any sort, a
/// directory where a file is required, a file where a directory is required,
/// or another kind of entry. The engine raises FILE-7's and FILE-8's
/// conditions from that report alone, so every resolver can model them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    SymbolicLink,
    /// A Windows directory junction: a mount-point reparse point.
    Junction,
    /// Any other Windows reparse point.
    ReparsePoint,
    /// A FIFO, socket, device or any other entry.
    Other,
    /// No entry.
    Missing,
}
impl ResourceKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::File,
        Self::Directory,
        Self::SymbolicLink,
        Self::Junction,
        Self::ReparsePoint,
        Self::Other,
        Self::Missing,
    ];
    /// The stable kind name.
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::SymbolicLink => "symbolic_link",
            Self::Junction => "junction",
            Self::ReparsePoint => "reparse_point",
            Self::Other => "other",
            Self::Missing => "missing",
        }
    }
    /// The kind a stable name spells.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    /// Whether the entry is a link of any sort, which FILE-8 refuses.
    pub fn is_link(self) -> bool {
        matches!(
            self,
            Self::SymbolicLink | Self::Junction | Self::ReparsePoint
        )
    }
}
/// Only public resource facts cross this boundary. Credentials belong to the
/// resolver closure and must not appear in references or returned errors.
pub struct Request<'a> {
    pub operation: Operation,
    pub kind: Kind,
    pub declaring: &'a str,
    pub written: &'a str,
    /// The held reference on freshness/publication requests; a staging token on
    /// commit and abort. The host must reject a changed reference.
    pub reference: Option<&'a str>,
    pub bytes: Option<&'a [u8]>,
    /// The most bytes a Capture may return. A resolver may refuse a larger
    /// resource before reading it, with [`Failure::Limit`]; the engine refuses
    /// a larger reply either way.
    pub limit: Option<usize>,
}
#[derive(Clone, Debug)]
pub struct Reply {
    pub frame: Option<Arc<crate::column_table::ColumnTable>>,
    pub reference: String,
    /// Present only for Capture. Inspect and Target must never read data.
    pub bytes: Option<Arc<[u8]>>,
    /// The identity the resource has from now on, from which its own
    /// relative declarations resolve: the canonical path a walk reached. Without
    /// it the engine names the resource by its lexical location, the
    /// declaring identity's directory joined with the written path.
    pub location: Option<String>,
    /// The physical file the name reaches, when the resolver knows it, as
    /// the local file resolver does. A hint, not a key: snapshots stay keyed
    /// by reference, and a capture that names a held file under another
    /// reference, such as a hard link to it, binds that file's snapshot and
    /// never reads other bytes from it.
    pub physical: Option<PhysicalFile>,
}
impl Reply {
    /// A reply with a reference and nothing else.
    pub fn reference(reference: String) -> Self {
        Self {
            frame: None,
            reference,
            bytes: None,
            location: None,
            physical: None,
        }
    }
}
/// A physical file: its device and inode on Unix, its volume serial number
/// and file ID on Windows. File metadata, never a digest of its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalFile {
    pub volume: i128,
    pub file: i128,
}
/// A local path the file resolver refuses before it walks anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRefusal {
    /// The written form fails FILE-4 or FILE-5.
    WrittenForm(yamaa_core::resource::ResourceFailure),
    /// A spelling the resolver cannot interpret: a NUL byte, a rooted path
    /// with `.` or `..` under explicit roots, or a malformed identity.
    Invalid,
    /// A root directory the resolver cannot select.
    Root,
    /// A location outside every explicit root.
    OutsideRoots,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    Changed,
    Cancelled,
    Rejected,
    Publication,
    Cleanup,
    /// The name reaches an entry of this kind where a regular file, or on
    /// the way a directory, is required. [`ResourceKind::Missing`] is
    /// [`Failure::Missing`].
    Kind(ResourceKind),
    /// The resource exists and cannot be read.
    Unreadable,
    /// The resource holds more bytes than the request's limit: its size,
    /// when the resolver knows it.
    Limit(Option<u64>),
    /// The resolver refuses the local path itself.
    Path(PathRefusal),
}
impl From<ResourceKind> for Failure {
    fn from(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Missing => Self::Missing,
            kind => Self::Kind(kind),
        }
    }
}
impl Failure {
    /// Translate a host resolver protocol failure name. Both host resolver
    /// ports speak the same closed name set: the protocol failures and the
    /// [`ResourceKind`] names; any other name is `Rejected`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "changed" => Self::Changed,
            "cancelled" => Self::Cancelled,
            "publication" => Self::Publication,
            "cleanup" => Self::Cleanup,
            "unreadable" => Self::Unreadable,
            name => ResourceKind::from_name(name).map_or(Self::Rejected, Self::from),
        }
    }
    /// The kind a failure reports, when it reports one.
    pub fn resource_kind(self) -> Option<ResourceKind> {
        match self {
            Self::Missing | Self::Kind(ResourceKind::Missing) => Some(ResourceKind::Missing),
            Self::Kind(kind) => Some(kind),
            _ => None,
        }
    }
}
/// A resolver may serve local objects, opaque URI identities or warehouse data.
/// Inspect and Capture name one resource: a resolver serves a regular file,
/// or reports what else the name reaches with [`Failure::Kind`] or
/// [`Failure::Missing`]. Stage creates a private provisional object and
/// returns its staging token; Commit atomically publishes it; Abort removes it
/// after a failed Commit or an interrupted Stage that returned a token. Stage
/// is atomic by contract: a failed Stage must not leave provisional output, so
/// Abort receives only the staging token and never the target reference. Abort
/// is idempotent and token-scoped. Implementations must return closed failures
/// and must not panic. Python and R adapters contain exceptions/conditions,
/// retaining their original objects.
pub trait Resolver: Send + Sync {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure>;
}
#[derive(Clone)]
struct Snapshot {
    declaring: String,
    written: String,
    kind: Kind,
    reference: String,
    /// The identity the capture's reply gave the resource, if any.
    location: Option<String>,
    /// The physical file the capture's reply reported, or the one a later
    /// check found the held name reaching with the same bytes. None once a
    /// capture found no held name reaching it and took a fresh snapshot.
    physical: Option<PhysicalFile>,
    bytes: Arc<[u8]>,
    frame: Option<Arc<crate::column_table::ColumnTable>>,
}
/// The snapshots one scope captured through one resolver: every capture of
/// an operation, whatever resolver serves it, is held and verified here.
pub(crate) struct Resources {
    pub resolver: Arc<dyn Resolver>,
    pub base: String,
    snapshots: Mutex<Vec<Snapshot>>,
}
impl Resources {
    pub fn new(resolver: Arc<dyn Resolver>, _entry: String) -> Self {
        Self::at(resolver, "/".into())
    }
    /// An empty scope whose undeclared requests come from `base`.
    pub fn at(resolver: Arc<dyn Resolver>, base: String) -> Self {
        Self {
            resolver,
            base,
            snapshots: Mutex::new(Vec::new()),
        }
    }
    /// A run scope over the same resolver, starting from these snapshots.
    /// Later captures in either owner are invisible to the other.
    pub fn scope(&self) -> Self {
        Self {
            resolver: Arc::clone(&self.resolver),
            base: self.base.clone(),
            snapshots: Mutex::new(
                self.snapshots
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone(),
            ),
        }
    }
    fn call(
        &self,
        operation: Operation,
        kind: Kind,
        declaring: &str,
        written: &str,
        reference: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Reply, Failure> {
        let reply = self.resolver.request(Request {
            operation,
            kind,
            declaring,
            written,
            reference,
            bytes: None,
            limit,
        })?;
        let invalid = |text: &str| text.is_empty() || text.contains('\0');
        if invalid(&reply.reference)
            || reply.location.as_deref().is_some_and(invalid)
            || (operation != Operation::Capture && (reply.bytes.is_some() || reply.frame.is_some()))
        {
            return Err(Failure::Rejected);
        }
        Ok(reply)
    }
    /// The identity of the resource `written` names from `declaring`: the
    /// location the resolver's reply gives it, else its lexical location.
    pub fn resolve(&self, declaring: &str, written: &str, kind: Kind) -> Result<String, Failure> {
        let reply = self.call(Operation::Inspect, kind, declaring, written, None, None)?;
        match reply.location {
            Some(location) => Ok(location),
            None => Self::location(declaring, written),
        }
    }
    /// Logical declaring locations remain independent of the backend identity.
    /// A returned URI is retained evidence and is never interpreted as a path.
    pub fn location(declaring: &str, written: &str) -> Result<String, Failure> {
        if yamaa_core::resource::written_path_failure(written).is_some() || written.contains('\0') {
            return Err(Failure::Rejected);
        }
        let mut parts = if written.starts_with('/') {
            Vec::new()
        } else {
            declaring.rsplit_once('/').map_or(Vec::new(), |(base, _)| {
                base.split('/').filter(|s| !s.is_empty()).collect()
            })
        };
        for part in written.split('/') {
            match part {
                "" | "." => (),
                ".." => {
                    parts.pop();
                }
                name => parts.push(name),
            }
        }
        Ok(format!("/{}", parts.join("/")))
    }
    pub fn target(&self, declaring: &str, written: &str) -> Result<String, Failure> {
        self.call(
            Operation::Target,
            Kind::Output,
            declaring,
            written,
            None,
            None,
        )
        .map(|reply| reply.reference)
    }
    /// Capture at most `maximum` bytes. Every capture that reuses a held
    /// snapshot, the same request again or another spelling the resolver
    /// gives the same reference, captures each request that snapshot holds
    /// again and requires it unchanged (FILE-9, FILE-10), so a substituted
    /// name cannot redirect a read silently. A capture that reaches, under
    /// another reference, the physical file a held snapshot holds, such as
    /// a hard link to it, must read that snapshot's bytes (FILE-9): two names
    /// of one file never observe different bytes. It binds the snapshot
    /// while a held name still reaches the file; see [`Self::same_file`].
    pub fn capture(
        &self,
        declaring: &str,
        written: &str,
        kind: Kind,
        maximum: usize,
    ) -> Result<(Arc<[u8]>, bool), Failure> {
        let same =
            |s: &Snapshot| s.declaring == declaring && s.written == written && s.kind == kind;
        let old = self
            .snapshots
            .lock()
            .map_err(|_| Failure::Rejected)?
            .iter()
            .find(|s| same(s))
            .cloned();
        if let Some(old) = old {
            if old.bytes.len() > maximum {
                return Err(Failure::Limit(u64::try_from(old.bytes.len()).ok()));
            }
            self.check_aliases(&old.reference)?;
            return Ok((old.bytes, false));
        }
        let reply = self.call(
            Operation::Capture,
            kind,
            declaring,
            written,
            None,
            Some(maximum),
        )?;
        if (reply.bytes.is_some() == reply.frame.is_some())
            || (kind != Kind::Input && reply.frame.is_some())
        {
            return Err(Failure::Rejected);
        }
        let bytes = reply.bytes.clone().unwrap_or_else(|| Arc::from([]));
        if bytes.len() > maximum {
            return Err(Failure::Limit(u64::try_from(bytes.len()).ok()));
        }
        let known = self
            .snapshots
            .lock()
            .map_err(|_| Failure::Rejected)?
            .iter()
            .find(|s| s.reference == reply.reference)
            .cloned();
        let known = match known {
            Some(known) => Some(known),
            None => self.same_file(reply.physical, &bytes, &reply.frame)?,
        };
        if let Some(known) = &known {
            if known.bytes != bytes || !same_frame(&known.frame, &reply.frame) {
                return Err(Failure::Changed);
            }
            if known.reference == reply.reference {
                self.check_aliases(&known.reference)?;
            }
        }
        let mut snapshots = self.snapshots.lock().map_err(|_| Failure::Rejected)?;
        if let Some(old) = snapshots.iter().find(|s| same(s)) {
            // Another owner of this scope captured the same request meanwhile.
            if old.reference != reply.reference
                || old.bytes != bytes
                || !same_frame(&old.frame, &reply.frame)
            {
                return Err(Failure::Changed);
            }
            return Ok((Arc::clone(&old.bytes), false));
        }
        let created = known.is_none();
        let (bytes, frame) = match known {
            Some(known) => (known.bytes, known.frame),
            None => (bytes, reply.frame),
        };
        snapshots.push(Snapshot {
            declaring: declaring.into(),
            written: written.into(),
            kind,
            reference: reply.reference,
            location: reply.location,
            physical: reply.physical,
            bytes: Arc::clone(&bytes),
            frame,
        });
        Ok((bytes, created))
    }
    /// The held snapshot a new capture binds when it reached, under another
    /// reference, the physical file `physical` that snapshot was taken from,
    /// such as through a hard link to it. A held name confirms the snapshot
    /// while an inspection finds it still reaching that file.
    ///
    /// When no held name reaches the file, the names were renamed or removed,
    /// and the identity is either that file reached by another of its names
    /// or a new file to which the file system gave a removed file's identity.
    /// A snapshot holds no file open that would tell the two apart, so the
    /// capture fails closed: other `bytes` are a change (FILE-9), and the same
    /// bytes are a fresh snapshot, which the identity names from then on. A
    /// check moves a snapshot whose name reaches a new file with the same
    /// bytes to that file's identity ([`Self::check`]), so an identity the
    /// snapshot gave up is not refused as its file.
    fn same_file(
        &self,
        physical: Option<PhysicalFile>,
        bytes: &[u8],
        frame: &Option<Arc<crate::column_table::ColumnTable>>,
    ) -> Result<Option<Snapshot>, Failure> {
        let Some(physical) = physical else {
            return Ok(None);
        };
        let held = self
            .snapshots
            .lock()
            .map_err(|_| Failure::Rejected)?
            .iter()
            .filter(|s| s.physical == Some(physical))
            .cloned()
            .collect::<Vec<_>>();
        for snapshot in &held {
            match self.call(
                Operation::Inspect,
                snapshot.kind,
                &snapshot.declaring,
                &snapshot.written,
                Some(&snapshot.reference),
                None,
            ) {
                Ok(reply) if reply.physical == Some(physical) => return Ok(Some(snapshot.clone())),
                Err(Failure::Cancelled) => return Err(Failure::Cancelled),
                _ => {}
            }
        }
        if held
            .iter()
            .any(|s| *s.bytes != *bytes || !same_frame(&s.frame, frame))
        {
            return Err(Failure::Changed);
        }
        if !held.is_empty() {
            self.snapshots
                .lock()
                .map_err(|_| Failure::Rejected)?
                .iter_mut()
                .filter(|s| s.physical == Some(physical))
                .for_each(|s| s.physical = None);
        }
        Ok(None)
    }
    /// Capture every held request for `reference` again, each unchanged.
    fn check_aliases(&self, reference: &str) -> Result<(), Failure> {
        let aliases = self
            .snapshots
            .lock()
            .map_err(|_| Failure::Rejected)?
            .iter()
            .filter(|s| s.reference == reference)
            .cloned()
            .collect::<Vec<_>>();
        aliases.iter().try_for_each(|alias| self.check(alias))
    }
    /// Capture one held request again: the resolver must give it the same
    /// reference and the same content, and may refuse a resource longer than
    /// the held bytes before reading it. A name that now reaches another
    /// physical file with the same bytes, such as one an editor wrote in its
    /// place, gives the snapshot that file's identity: the identity it had
    /// may go to a new file, which must not then be refused as this one.
    fn check(&self, snapshot: &Snapshot) -> Result<(), Failure> {
        let reply = match self.call(
            Operation::Capture,
            snapshot.kind,
            &snapshot.declaring,
            &snapshot.written,
            Some(&snapshot.reference),
            Some(snapshot.bytes.len()),
        ) {
            Err(Failure::Limit(_)) => return Err(Failure::Changed),
            result => result?,
        };
        if (reply.bytes.is_some() == reply.frame.is_some())
            || (snapshot.kind != Kind::Input && reply.frame.is_some())
        {
            return Err(Failure::Rejected);
        }
        let bytes = reply.bytes.unwrap_or_else(|| Arc::from([]));
        if reply.reference != snapshot.reference
            || bytes != snapshot.bytes
            || !same_frame(&snapshot.frame, &reply.frame)
        {
            return Err(Failure::Changed);
        }
        if reply.physical != snapshot.physical {
            self.snapshots
                .lock()
                .map_err(|_| Failure::Rejected)?
                .iter_mut()
                .filter(|s| {
                    s.declaring == snapshot.declaring
                        && s.written == snapshot.written
                        && s.kind == snapshot.kind
                })
                .for_each(|s| s.physical = reply.physical);
        }
        Ok(())
    }
    pub fn frame(
        &self,
        declaring: &str,
        written: &str,
    ) -> Option<Arc<crate::column_table::ColumnTable>> {
        self.snapshots
            .lock()
            .ok()?
            .iter()
            .find(|s| s.declaring == declaring && s.written == written && s.kind == Kind::Input)?
            .frame
            .clone()
    }
    pub fn len(&self) -> usize {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
    pub fn sources(&self, kind: Option<Kind>) -> Vec<(String, Arc<[u8]>)> {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|s| s.frame.is_none() && kind.is_none_or(|kind| s.kind == kind))
            .map(|s| (s.reference.clone(), Arc::clone(&s.bytes)))
            .collect()
    }
    /// Check the first `count` held snapshots again, in capture order, each
    /// at its own reference and bytes. A failed one is named by the identity
    /// [`Self::resolve`] gave the document it holds, as every document
    /// carries it: the location of its capture's reply, else its lexical
    /// location.
    pub fn verify(&self, count: usize) -> Result<(), (String, Failure)> {
        let name = |snapshot: &Snapshot| {
            snapshot.location.clone().unwrap_or_else(|| {
                Self::location(&snapshot.declaring, &snapshot.written)
                    .unwrap_or_else(|_| snapshot.reference.clone())
            })
        };
        let snapshots = self
            .snapshots
            .lock()
            .map_err(|_| (String::new(), Failure::Rejected))?
            .clone();
        if count > snapshots.len() {
            return Err((String::new(), Failure::Rejected));
        }
        for snapshot in snapshots.into_iter().take(count) {
            self.check(&snapshot)
                .map_err(|failure| (name(&snapshot), failure))?;
        }
        Ok(())
    }
}

/// Whether two held frames have the same cells: floats by bits, and dates and
/// datetimes with their collected precision.
fn same_frame(
    old: &Option<Arc<crate::column_table::ColumnTable>>,
    new: &Option<Arc<crate::column_table::ColumnTable>>,
) -> bool {
    use yamaa_core::table::{TableAccess, ValueRef};
    match (old, new) {
        (None, None) => true,
        (Some(left), Some(right))
            if left.schema() == right.schema() && left.row_count() == right.row_count() =>
        {
            (0..left.row_count()).all(|row| {
                (0..left.schema().columns().len()).all(|column| {
                    match (left.cell(row, column), right.cell(row, column)) {
                        (Ok(ValueRef::Float(left)), Ok(ValueRef::Float(right))) => {
                            left.get().to_bits() == right.get().to_bits()
                        }
                        (Ok(ValueRef::Date(left)), Ok(ValueRef::Date(right))) => {
                            left.fields() == right.fields()
                                && left.collected_precision() == right.collected_precision()
                        }
                        (Ok(ValueRef::DateTime(left)), Ok(ValueRef::DateTime(right))) => {
                            left.fields() == right.fields()
                                && left.collected_precision() == right.collected_precision()
                        }
                        (Ok(left), Ok(right)) => left == right,
                        _ => false,
                    }
                })
            })
        }
        _ => false,
    }
}

/// Deterministic object store for tests and host-owned local/remote adapters.
/// Replacing a value with a new reference models a remote object-version change.
/// A name can also hold a kind fact: requests for it report that kind, as a
/// file resolver reports a directory, link or other entry (FILE-7, FILE-8).
#[derive(Default)]
pub struct MemoryResolver {
    objects: Mutex<BTreeMap<(String, String), Object>>,
    staged: Mutex<StagedObjects>,
    published: Mutex<BTreeMap<String, Arc<[u8]>>>,
    cancelled: std::sync::atomic::AtomicBool,
    next_stage: std::sync::atomic::AtomicU64,
}
type StagedObjects = BTreeMap<String, (String, Arc<[u8]>)>;
#[derive(Clone)]
enum Object {
    Served(Reply),
    Kind(ResourceKind),
}
impl MemoryResolver {
    pub fn insert_frame(
        &self,
        declaring: &str,
        written: &str,
        reference: &str,
        frame: Arc<crate::column_table::ColumnTable>,
    ) {
        self.objects.lock().unwrap().insert(
            (declaring.into(), written.into()),
            Object::Served(Reply {
                frame: Some(frame),
                ..Reply::reference(reference.into())
            }),
        );
    }
    pub fn insert(&self, declaring: &str, written: &str, reference: &str, bytes: Arc<[u8]>) {
        self.objects.lock().unwrap().insert(
            (declaring.into(), written.into()),
            Object::Served(Reply {
                bytes: Some(bytes),
                ..Reply::reference(reference.into())
            }),
        );
    }
    /// Report `kind` for this name instead of serving it. A
    /// [`ResourceKind::File`] fact names a file where a directory is
    /// required, as a walk reports one on the way to a deeper name.
    pub fn insert_kind(&self, declaring: &str, written: &str, kind: ResourceKind) {
        self.objects
            .lock()
            .unwrap()
            .insert((declaring.into(), written.into()), Object::Kind(kind));
    }
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn published(&self, reference: &str) -> Option<Arc<[u8]>> {
        self.published.lock().unwrap().get(reference).cloned()
    }
    pub fn staged_count(&self) -> usize {
        self.staged.lock().unwrap().len()
    }
}
impl Resolver for MemoryResolver {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        if request.operation == Operation::Abort {
            let reference = request.reference.ok_or(Failure::Rejected)?;
            // Attempt-owned cleanup: delete only the staged token. Never match
            // on the target identity, so overlapping publishes to the same
            // target cannot delete each other's provisional output.
            self.staged
                .lock()
                .map_err(|_| Failure::Cleanup)?
                .retain(|token, _| token != reference);
            return Ok(Reply::reference(reference.into()));
        }
        if self.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Failure::Cancelled);
        }
        match request.operation {
            Operation::Inspect | Operation::Capture => {
                let object = self
                    .objects
                    .lock()
                    .map_err(|_| Failure::Rejected)?
                    .get(&(request.declaring.into(), request.written.into()))
                    .cloned()
                    .ok_or(Failure::Missing)?;
                let object = match object {
                    Object::Served(reply) => reply,
                    Object::Kind(kind) => return Err(Failure::from(kind)),
                };
                if request
                    .reference
                    .is_some_and(|held| held != object.reference)
                {
                    return Err(Failure::Changed);
                }
                Ok(if request.operation == Operation::Inspect {
                    Reply::reference(object.reference)
                } else {
                    object
                })
            }
            Operation::Target => Resources::location(request.declaring, request.written)
                .map(|path| Reply::reference(format!("memory:{path}"))),
            Operation::Stage => {
                let reference = request.reference.ok_or(Failure::Rejected)?;
                let bytes = Arc::from(request.bytes.ok_or(Failure::Rejected)?);
                // Unique per-attempt token: overlapping publishes to the same
                // target stage independently, so a late Abort by one attempt
                // can never remove another attempt's provisional output.
                let sequence = self
                    .next_stage
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let token = format!("stage:{sequence}:{reference}");
                let mut staged = self.staged.lock().map_err(|_| Failure::Publication)?;
                if staged.contains_key(&token) {
                    return Err(Failure::Publication);
                }
                staged.insert(token.clone(), (reference.into(), bytes));
                Ok(Reply::reference(token))
            }
            Operation::Commit => {
                let token = request.reference.ok_or(Failure::Rejected)?;
                let (target, bytes) = self
                    .staged
                    .lock()
                    .map_err(|_| Failure::Publication)?
                    .remove(token)
                    .ok_or(Failure::Publication)?;
                self.published
                    .lock()
                    .map_err(|_| Failure::Publication)?
                    .insert(target.clone(), bytes);
                Ok(Reply::reference(target))
            }
            Operation::Abort => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_from_name_covers_the_closed_protocol_set() {
        assert_eq!(Failure::from_name("missing"), Failure::Missing);
        assert_eq!(Failure::from_name("changed"), Failure::Changed);
        assert_eq!(Failure::from_name("cancelled"), Failure::Cancelled);
        assert_eq!(Failure::from_name("publication"), Failure::Publication);
        assert_eq!(Failure::from_name("cleanup"), Failure::Cleanup);
        assert_eq!(Failure::from_name("unreadable"), Failure::Unreadable);
        assert_eq!(Failure::from_name("anything-else"), Failure::Rejected);
        assert_eq!(Failure::from_name(""), Failure::Rejected);
        for kind in ResourceKind::ALL {
            let failure = Failure::from_name(kind.name());
            assert_eq!(failure, Failure::from(kind));
            assert_eq!(failure.resource_kind(), Some(kind));
            assert_eq!(ResourceKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Failure::from(ResourceKind::Missing), Failure::Missing);
        assert_eq!(ResourceKind::from_name("link"), None);
    }

    /// Names that reach physical files, as a file system holds them: a hard
    /// link is a second name of one file, and a rewrite changes what every
    /// name of that file reads.
    #[derive(Default)]
    struct Files {
        names: Mutex<BTreeMap<String, i128>>,
        contents: Mutex<BTreeMap<i128, Arc<[u8]>>>,
    }
    impl Files {
        fn link(&self, name: &str, file: i128) {
            self.names.lock().unwrap().insert(name.into(), file);
        }
        fn unlink(&self, name: &str) {
            self.names.lock().unwrap().remove(name);
        }
        fn write(&self, file: i128, bytes: &[u8]) {
            self.contents.lock().unwrap().insert(file, Arc::from(bytes));
        }
    }
    impl Resolver for Files {
        fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
            let file = *self
                .names
                .lock()
                .unwrap()
                .get(request.written)
                .ok_or(Failure::Missing)?;
            let reference = format!("/{}", request.written);
            if request.reference.is_some_and(|held| held != reference) {
                return Err(Failure::Changed);
            }
            let bytes = Arc::clone(&self.contents.lock().unwrap()[&file]);
            Ok(Reply {
                bytes: (request.operation == Operation::Capture).then_some(bytes),
                physical: Some(PhysicalFile { volume: 1, file }),
                ..Reply::reference(reference)
            })
        }
    }
    fn capture(resources: &Resources, name: &str) -> Result<(Arc<[u8]>, bool), Failure> {
        resources.capture("/spec.yaml", name, Kind::Input, 64)
    }

    #[test]
    fn names_of_one_physical_file_bind_one_snapshot_and_never_read_two_contents() {
        let files = Arc::new(Files::default());
        files.write(1, b"one");
        files.link("a", 1);
        files.link("b", 1);
        let resources = Resources::at(files.clone(), "/spec.yaml".into());
        let (a, created) = capture(&resources, "a").unwrap();
        assert!(created);
        let (b, created) = capture(&resources, "b").unwrap();
        assert!(!created);
        assert!(Arc::ptr_eq(&a, &b));
        // The file is rewritten in place, then reached by a third name.
        files.write(1, b"two");
        files.link("c", 1);
        assert_eq!(capture(&resources, "c"), Err(Failure::Changed));
        assert_eq!(a.as_ref(), b"one");
        assert_eq!(resources.len(), 2);
        // Each name is still verified at its own reference and bytes.
        assert_eq!(resources.verify(2), Err(("/a".into(), Failure::Changed)));
    }

    /// FILE-9 after the captured name moves: a file rewritten once no held
    /// name reaches it is still the file a snapshot was taken from when
    /// another of its names reaches it, and its other bytes are a change.
    #[test]
    fn a_file_no_held_name_reaches_never_reads_other_bytes() {
        for moved in ["renamed", "removed", "replaced"] {
            let files = Arc::new(Files::default());
            files.write(1, b"one");
            files.link("a", 1);
            files.link("b", 1);
            let resources = Resources::at(files.clone(), "/spec.yaml".into());
            let (a, _) = capture(&resources, "a").unwrap();
            match moved {
                "renamed" => {
                    files.unlink("a");
                    files.link("a.old", 1);
                }
                "removed" => files.unlink("a"),
                _ => {
                    files.write(2, b"other");
                    files.link("a", 2);
                }
            }
            files.write(1, b"two");
            assert_eq!(capture(&resources, "b"), Err(Failure::Changed), "{moved}");
            if moved == "renamed" {
                assert_eq!(capture(&resources, "a.old"), Err(Failure::Changed));
            }
            // The refusal holds on a second try, and the held bytes stay.
            assert_eq!(capture(&resources, "b"), Err(Failure::Changed), "{moved}");
            assert_eq!(a.as_ref(), b"one");
            assert_eq!(resources.len(), 1, "{moved}");
            // Unchanged, the other name reads the held bytes as a snapshot of
            // its own, and the identity names that snapshot from then on.
            files.write(1, b"one");
            let (b, created) = capture(&resources, "b").unwrap();
            assert!(created, "{moved}");
            assert_eq!(b.as_ref(), b"one");
            files.link("c", 1);
            assert!(!capture(&resources, "c").unwrap().1, "{moved}");
            files.write(1, b"three");
            files.link("d", 1);
            assert_eq!(capture(&resources, "d"), Err(Failure::Changed), "{moved}");
        }
    }

    /// The file system may give a removed file's identity to a new file.
    /// Nothing tells that file apart from the held one reached by another
    /// name, so its other bytes fail closed and its same bytes are a fresh
    /// snapshot. A name a check finds reaching another file with the same
    /// bytes, as an editor's save leaves it, gives its snapshot that file's
    /// identity, so the identity it gave up binds nothing.
    #[test]
    fn a_reused_identity_fails_closed_until_a_check_moves_the_snapshot() {
        let files = Arc::new(Files::default());
        files.write(1, b"one");
        files.link("a", 1);
        let resources = Resources::at(files.clone(), "/spec.yaml".into());
        capture(&resources, "a").unwrap();
        // `a` is removed and a new file takes its identity.
        files.unlink("a");
        files.write(1, b"new");
        files.link("c", 1);
        assert_eq!(capture(&resources, "c"), Err(Failure::Changed));
        assert_eq!(capture(&resources, "a"), Err(Failure::Missing));
        files.write(1, b"one");
        let (c, created) = capture(&resources, "c").unwrap();
        assert!(created);
        assert_eq!(c.as_ref(), b"one");
        // An editor saves `b` as a new file with the same bytes, and another
        // new file takes the identity `b` gave up.
        files.write(3, b"held");
        files.link("b", 3);
        let resources = Resources::at(files.clone(), "/spec.yaml".into());
        capture(&resources, "b").unwrap();
        files.write(4, b"held");
        files.link("b", 4);
        files.write(3, b"another file");
        files.link("e", 3);
        // Before a check, its other bytes fail closed.
        let unchecked = resources.scope();
        assert_eq!(capture(&unchecked, "e"), Err(Failure::Changed));
        // A check finds `b` unchanged at its new file, and the identity it
        // gave up binds nothing.
        resources.verify(1).unwrap();
        let (e, created) = capture(&resources, "e").unwrap();
        assert!(created);
        assert_eq!(e.as_ref(), b"another file");
        // The new file `b` reaches binds its snapshot.
        files.link("f", 4);
        assert!(!capture(&resources, "f").unwrap().1);
    }

    #[test]
    fn memory_kind_facts_report_their_kind_without_a_snapshot() {
        let resolver = Arc::new(MemoryResolver::default());
        for kind in ResourceKind::ALL {
            resolver.insert_kind("/spec.yaml", kind.name(), kind);
        }
        let resources = Resources::at(resolver, "/spec.yaml".into());
        for kind in ResourceKind::ALL {
            for result in [
                resources
                    .resolve("/spec.yaml", kind.name(), Kind::Input)
                    .map(drop),
                resources
                    .capture("/spec.yaml", kind.name(), Kind::Input, 16)
                    .map(drop),
            ] {
                assert_eq!(result.unwrap_err().resource_kind(), Some(kind));
            }
        }
        assert_eq!(resources.len(), 0);
    }
}
