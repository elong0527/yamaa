//! The local file resolver: descriptor-anchored walks from caller-selected
//! directories (FILE-7) that report what a name reaches, links included
//! (FILE-8). It is the default [`Resolver`]: every local file an operation
//! inspects or captures goes through it, exactly as a caller's resolver would
//! be asked, and [`crate::file_resources::Resources`] holds and verifies the
//! snapshots. Its references and locations are canonical paths in the
//! engine's spelling, and each reply names the physical file it reached, so
//! two hard links to one file bind one snapshot (FILE-9). Publication stays
//! with [`crate::file_publication`], which this resolver only anchors.
#[cfg(windows)]
use crate::windows_file::{Directory as OwnedFd, Kind as FileType};
#[cfg(unix)]
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, PoisonError, RwLock},
};

use crate::{
    file_resources::Error,
    host_resources::{
        Failure, Operation, PathRefusal, PhysicalFile, Reply, Request, Resolver, ResourceKind,
    },
};

type Identity = (i128, i128, FileType);
type Segments = Vec<String>;
struct Root {
    descriptor: OwnedFd,
    canonical: Segments,
    spellings: Vec<Segments>,
}
struct Anchor {
    descriptor: OwnedFd,
    #[cfg(windows)]
    canonical: Segments,
    key: Segments,
    remainder: Segments,
}
struct Opened {
    file: File,
    key: Segments,
    /// The physical file the walk opened.
    physical: PhysicalFile,
}

/// Local files under the caller-selected directories. Each request names a
/// declaring identity, which is a canonical file path or the base identity
/// [`FileResolver::new`] returns for a selected directory, and a written path.
pub struct FileResolver {
    roots: RwLock<Vec<Root>>,
    /// The public caller-path policy: a declaration outside every selected
    /// directory is walked from its own filesystem root with the same link
    /// checks. Without it, explicit roots bound every declaration and a
    /// relative path falls back across them in order.
    local_paths: bool,
    /// Caller paths in the engine's spelling. A caller path on a network
    /// share is the only written path whose root may be a share
    /// (`//server/share/...`, D15); a declared path never may (FILE-5).
    callers: RwLock<Vec<String>>,
}

impl FileResolver {
    /// The public caller-path policy over the existing directory `base`,
    /// selected once, with the identity undeclared requests come from.
    pub fn for_entry(base: &str) -> Result<(Self, String), Error> {
        let (mut resolver, base) = Self::new(base, base, &[])?;
        resolver.local_paths = true;
        Ok((resolver, base))
    }
    /// Explicit roots: the first is the project root, and later roots retain
    /// fallback order. Returns the identity undeclared requests come from:
    /// the canonical directory `base` names.
    pub fn new(root: &str, base: &str, data_roots: &[String]) -> Result<(Self, String), Error> {
        let mut roots = Vec::new();
        for written in std::iter::once(root).chain(data_roots.iter().map(String::as_str)) {
            roots.push(selected_root(written)?);
        }
        let path = crate::canonical_path::canonicalize(base).map_err(|_| Error::InvalidBase)?;
        if !path.is_dir() {
            return Err(Error::InvalidBase);
        }
        let base = canonical_directory_identity(&path).ok_or(Error::InvalidBase)?;
        Ok((
            Self {
                roots: RwLock::new(roots),
                local_paths: false,
                callers: RwLock::new(Vec::new()),
            },
            base,
        ))
    }
    /// The project file a caller passes is a caller path like the entry
    /// (PROJECT-1): its existing parent is selected once, and the caller's own
    /// spelling of that directory keeps its authority (FILE-7). A project
    /// operation names each listed specification by its canonical identity,
    /// so without this root the caller's spelling of the project file, such
    /// as the macOS `/var` alias of `/private/var`, lies outside the entry
    /// parent and is walked from its filesystem root and refused as a link.
    /// A parent that cannot be selected adds no root: reading the project
    /// file then reports its own failure.
    pub fn select_caller_parent(&self, path: &str) {
        if !self.local_paths {
            return;
        }
        self.callers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(path.to_owned());
        let Some(parent) = Path::new(path).parent().and_then(Path::to_str) else {
            return;
        };
        let Ok(root) = selected_root(parent) else {
            return;
        };
        let mut roots = self.roots.write().unwrap_or_else(PoisonError::into_inner);
        // An entry beside its project file already holds every spelling.
        if !roots.iter().any(|known| {
            root.spellings
                .iter()
                .all(|spelling| known.spellings.contains(spelling))
        }) {
            roots.push(root);
        }
    }
    /// The lexical location a declaration names under this resolver's
    /// policy: its anchored canonical path. The target may be absent; no
    /// file below a root is opened.
    pub(crate) fn location(&self, declaring: &str, written: &str) -> Result<String, Error> {
        let base = declaring_directory(declaring).ok_or(Error::InvalidPath)?;
        let anchor = self.declaration_anchor(&base, written)?;
        Ok(path_text(&anchor.key))
    }
    /// Publication holds the declaration location, including absent targets.
    /// Retain its physical descriptor; the parent chain is checked at save.
    pub fn publication_target(
        &self,
        declaring: &str,
        written: &str,
    ) -> Result<crate::file_publication::AnchoredTarget, Error> {
        let base = declaring_directory(declaring).ok_or(Error::InvalidPath)?;
        let anchor = self.declaration_anchor(&base, written)?;
        Ok(crate::file_publication::AnchoredTarget::new(
            anchor.descriptor,
            anchor.remainder,
            path_text(&anchor.key),
        ))
    }
    fn declaration_anchor(&self, base: &[String], written: &str) -> Result<Anchor, Error> {
        if let Some(failure) = yamaa_core::resource::written_path_failure(written) {
            return Err(Error::WrittenForm(failure));
        }
        if matches!(
            written.rsplit(['/', '\\']).next(),
            None | Some("" | "." | "..")
        ) {
            return Err(Error::InvalidPath);
        }
        self.anchors(base, written)?
            .into_iter()
            .next()
            .ok_or(Error::OutsideRoots)
    }
    /// The caller's own file `name` in the selected directory `base`, a base
    /// identity [`Self::for_entry`] returned, opened for reading: one name
    /// below that directory's held descriptor, a regular file and never a
    /// link (FILE-7, FILE-8). `name` is the host file name of a path the
    /// caller passed, not a written path, so FILE-5's written form does not
    /// apply to it: on Unix it may hold a `:` or a `\`. Windows refuses a
    /// `:` in a name, which would name a stream of another file.
    #[cfg(feature = "odm")]
    pub(crate) fn open_named(&self, base: &str, name: &str) -> Result<File, Failure> {
        self.named(base, name).map(|opened| opened.file)
    }
    /// [`Self::open_named`], read under `limit` bytes and confirmed as one
    /// snapshot as every capture is.
    #[cfg(feature = "odm")]
    pub(crate) fn capture_named(
        &self,
        base: &str,
        name: &str,
        limit: usize,
    ) -> Result<Arc<[u8]>, Failure> {
        capture(|| self.named(base, name), limit, None).map(|(_, bytes)| bytes)
    }
    /// The caller's own file `name` in the selected directory `base` as a
    /// publication target: one name below that directory's held descriptor,
    /// as [`Self::open_named`] reads one. The name is checked as
    /// [`Self::open_named`] checks it; the publisher refuses, when it opens
    /// the target, a name that is anything but absent or a regular file
    /// (OUTPUT-8, FILE-8).
    pub fn publication_target_named(
        &self,
        base: &str,
        name: &str,
    ) -> Result<crate::file_publication::AnchoredTarget, Failure> {
        let anchor = self.named_anchor(base, name)?;
        Ok(crate::file_publication::AnchoredTarget::new(
            anchor.descriptor,
            anchor.remainder,
            path_text(&anchor.key),
        ))
    }
    #[cfg(feature = "odm")]
    fn named(&self, base: &str, name: &str) -> Result<Opened, Failure> {
        open_anchor(self.named_anchor(base, name)?)
    }
    fn named_anchor(&self, base: &str, name: &str) -> Result<Anchor, Failure> {
        let separators: &[char] = if cfg!(windows) {
            &['/', '\\', ':', '\0']
        } else {
            &['/', '\0']
        };
        if matches!(name, "" | "." | "..") || name.contains(separators) {
            return Err(Failure::Path(PathRefusal::Invalid));
        }
        let base = declaring_directory(base).ok_or(Failure::Path(PathRefusal::Invalid))?;
        let roots = self.roots.read().unwrap_or_else(PoisonError::into_inner);
        relative_anchor(&roots, &base, name).map_err(refusal)
    }
    fn is_caller(&self, written: &str) -> bool {
        self.callers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|caller| caller == written)
    }
    fn anchors(&self, base: &[String], written: &str) -> Result<Vec<Anchor>, Error> {
        // A caller path's network-share root is a separator like `/`: judge
        // only what follows it.
        let judged = match share_root(written, cfg!(windows)) {
            Some((_, rest)) if self.is_caller(written) => rest,
            _ => written,
        };
        if let Some(failure) = yamaa_core::resource::written_path_failure(judged) {
            return Err(Error::WrittenForm(failure));
        }
        if written.contains('\0') {
            return Err(Error::InvalidPath);
        }
        let rooted = rooted_segments(written);
        if rooted.as_ref().map_or_else(
            || written.split('/').any(str::is_empty),
            |s| {
                s.len() == 1
                    || s[1..]
                        .iter()
                        .any(|s| s.is_empty() || (!self.local_paths && (s == "." || s == "..")))
            },
        ) {
            return Err(Error::InvalidPath);
        }
        if self.local_paths {
            let (start, relative) = match &rooted {
                Some(segments) => (&segments[..1], segments[1..].join("/")),
                None => (base, written.to_owned()),
            };
            return self
                .local_anchor(start, &relative)
                .map(|anchor| vec![anchor]);
        }
        let roots = self.roots.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(segments) = rooted {
            let mut selected = None;
            for (index, root) in roots.iter().enumerate() {
                for spelling in &root.spellings {
                    if within(&segments, spelling)
                        && selected.is_none_or(|(_, depth)| spelling.len() > depth)
                    {
                        selected = Some((index, spelling.len()));
                    }
                }
            }
            let (root, depth) = selected.ok_or(Error::OutsideRoots)?;
            return Ok(vec![anchor(&roots[root], segments[depth..].to_vec())?]);
        }
        let mut anchors = Vec::new();
        let inside = roots.iter().any(|r| within(base, &r.canonical));
        match relative_anchor(&roots, base, written) {
            Ok(anchor) => anchors.push(anchor),
            Err(error) if inside => return Err(error),
            Err(_) => {}
        }
        let mut seen = anchors
            .iter()
            .map(|a| a.key.clone())
            .collect::<BTreeSet<_>>();
        for root in roots.iter() {
            if let Ok(anchor) = relative_anchor(&roots, &root.canonical, written) {
                if seen.insert(anchor.key.clone()) {
                    anchors.push(anchor);
                }
            }
        }
        if anchors.is_empty() {
            Err(Error::OutsideRoots)
        } else {
            Ok(anchors)
        }
    }
    fn local_anchor(&self, start: &[String], written: &str) -> Result<Anchor, Error> {
        let mut resolved = start.to_vec();
        for segment in written.split('/') {
            match segment {
                "." => {}
                ".." if resolved.len() > 1 => {
                    resolved.pop();
                }
                ".." => {}
                value => resolved.push(value.into()),
            }
        }
        {
            let roots = self.roots.read().unwrap_or_else(PoisonError::into_inner);
            for root in roots.iter() {
                for spelling in &root.spellings {
                    if within(&resolved, spelling) {
                        return anchor(root, resolved[spelling.len()..].to_vec());
                    }
                }
            }
        }
        // Outside the caller-selected directories, a declaration is walked
        // from its own filesystem root: `/`, a drive or a network share (D15),
        // with the same link checks.
        let root_path = path_text(&resolved[..1]);
        if !Path::new(&root_path).is_absolute() {
            return Err(Error::InvalidPath);
        }
        let root = selected_root(&root_path)?;
        let remainder = resolved[1..].to_vec();
        if remainder.is_empty() {
            return Err(Error::NotRegularFile);
        }
        let mut key = root.canonical.clone();
        key.extend_from_slice(&remainder);
        Ok(Anchor {
            descriptor: root.descriptor,
            #[cfg(windows)]
            canonical: root.canonical,
            key,
            remainder,
        })
    }
    fn open_at(&self, base: &[String], written: &str) -> Result<Opened, Failure> {
        for anchor in self.anchors(base, written).map_err(refusal)? {
            match open_anchor(anchor) {
                // A genuinely absent entry permits fallback; other observed
                // failures are terminal.
                Err(Failure::Missing) => continue,
                Err(failure) => return Err(failure),
                Ok(opened) => return Ok(opened),
            }
        }
        Err(Failure::Missing)
    }
}

impl Resolver for FileResolver {
    /// Inspect walks to the file and Capture reads it; both answer with its
    /// canonical path as reference and location. Any other name reports the
    /// kind of the entry that stops the walk. Publication goes through
    /// [`FileResolver::publication_target`], not through requests.
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        let base =
            declaring_directory(request.declaring).ok_or(Failure::Path(PathRefusal::Invalid))?;
        match request.operation {
            Operation::Inspect => {
                let opened = self.open_at(&base, request.written)?;
                let key = path_text(&opened.key);
                if request.reference.is_some_and(|held| held != key) {
                    return Err(Failure::Changed);
                }
                Ok(Reply {
                    location: Some(key.clone()),
                    physical: Some(opened.physical),
                    ..Reply::reference(key)
                })
            }
            Operation::Capture => {
                let (opened, bytes) = capture(
                    || self.open_at(&base, request.written),
                    request.limit.unwrap_or(usize::MAX),
                    request.reference,
                )?;
                let key = path_text(&opened.key);
                Ok(Reply {
                    bytes: Some(bytes),
                    location: Some(key.clone()),
                    physical: Some(opened.physical),
                    ..Reply::reference(key)
                })
            }
            Operation::Target | Operation::Stage | Operation::Commit | Operation::Abort => {
                Err(Failure::Rejected)
            }
        }
    }
}

/// Read the file `open` reaches, refusing it before reading when it holds
/// more than `limit` bytes, and open and read it again to confirm one
/// snapshot: the second read is compared chunk by chunk, never held whole. A
/// held reference the first open no longer reaches has changed.
fn capture(
    open: impl Fn() -> Result<Opened, Failure>,
    limit: usize,
    held: Option<&str>,
) -> Result<(Opened, Arc<[u8]>), Failure> {
    let mut opened = open()?;
    if held.is_some_and(|held| held != path_text(&opened.key)) {
        return Err(Failure::Changed);
    }
    let content = read_bounded(&mut opened.file, limit)?;
    let mut current = open().map_err(|_| Failure::Changed)?;
    if current.key != opened.key
        || !holds(&mut current.file, &content).map_err(|_| Failure::Changed)?
    {
        return Err(Failure::Changed);
    }
    Ok((opened, content.into()))
}

/// An anchor failure as the protocol reports it.
fn refusal(error: Error) -> Failure {
    match error {
        Error::WrittenForm(failure) => Failure::Path(PathRefusal::WrittenForm(failure)),
        Error::InvalidPath => Failure::Path(PathRefusal::Invalid),
        Error::OutsideRoots => Failure::Path(PathRefusal::OutsideRoots),
        Error::InvalidRoot | Error::InvalidBase => Failure::Path(PathRefusal::Root),
        // An anchor with nothing below its root names that directory.
        Error::NotRegularFile => Failure::Kind(ResourceKind::Directory),
        Error::Symlink => Failure::Kind(ResourceKind::SymbolicLink),
        Error::Unreadable => Failure::Unreadable,
        Error::Missing => Failure::Missing,
        Error::Changed => Failure::Changed,
        Error::Limit(limit) => Failure::Limit(limit.actual.and_then(|n| u64::try_from(n).ok())),
        Error::Host(failure) => failure,
    }
}

fn relative_anchor(roots: &[Root], start: &[String], written: &str) -> Result<Anchor, Error> {
    let mut resolved = start.to_vec();
    for segment in written.split('/') {
        match segment {
            "." => {}
            ".." if resolved.len() > 1 => {
                resolved.pop();
            }
            ".." => return Err(Error::OutsideRoots),
            value => resolved.push(value.into()),
        }
    }
    let mut selected = None;
    for (index, root) in roots.iter().enumerate() {
        if within(&resolved, &root.canonical)
            && selected.is_none_or(|(_, depth)| root.canonical.len() > depth)
        {
            selected = Some((index, root.canonical.len()));
        }
    }
    let (root, depth) = selected.ok_or(Error::OutsideRoots)?;
    anchor(&roots[root], resolved[depth..].to_vec())
}
fn anchor(root: &Root, remainder: Segments) -> Result<Anchor, Error> {
    if remainder.is_empty() {
        return Err(Error::NotRegularFile);
    }
    let mut key = root.canonical.clone();
    key.extend_from_slice(&remainder);
    Ok(Anchor {
        descriptor: root.descriptor.try_clone().map_err(|_| Error::Unreadable)?,
        #[cfg(windows)]
        canonical: root.canonical.clone(),
        key,
        remainder,
    })
}

/// The kind a walk reports for an entry of this type.
#[cfg(unix)]
fn reported(kind: FileType) -> ResourceKind {
    match kind {
        FileType::RegularFile => ResourceKind::File,
        FileType::Directory => ResourceKind::Directory,
        FileType::Symlink => ResourceKind::SymbolicLink,
        _ => ResourceKind::Other,
    }
}
/// The kind a walk reports for an entry with this status.
#[cfg(windows)]
fn reported(status: &crate::windows_file::Status) -> ResourceKind {
    match status.kind {
        FileType::RegularFile => ResourceKind::File,
        FileType::Directory => ResourceKind::Directory,
        FileType::Symlink => reparse_kind(status.reparse_tag),
        FileType::Other => ResourceKind::Other,
    }
}
/// The kind of a Windows reparse point, from its tag. FILE-8 refuses all of
/// them; the kind only names which one a name reached.
#[cfg_attr(not(windows), allow(dead_code))]
fn reparse_kind(tag: u32) -> ResourceKind {
    // winnt.h: IO_REPARSE_TAG_MOUNT_POINT and IO_REPARSE_TAG_SYMLINK.
    const MOUNT_POINT: u32 = 0xA000_0003;
    const SYMLINK: u32 = 0xA000_000C;
    match tag {
        SYMLINK => ResourceKind::SymbolicLink,
        MOUNT_POINT => ResourceKind::Junction,
        _ => ResourceKind::ReparsePoint,
    }
}

/// Walk the anchor's remainder one component at a time: every component but
/// the last is a directory and the last is a regular file. The first entry
/// that is anything else stops the walk with its kind.
#[cfg(unix)]
fn open_anchor(anchor: Anchor) -> Result<Opened, Failure> {
    let root = &anchor.descriptor;
    let mut directories = Vec::new();
    let mut links = Vec::new();
    let mut file = None;
    for (index, name) in anchor.remainder.iter().enumerate() {
        let parent = directories.last().unwrap_or(root);
        let initial =
            fs::statat(parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
                if matches!(error, rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR) {
                    Failure::Missing
                } else {
                    Failure::Unreadable
                }
            })?;
        let directory = index + 1 != anchor.remainder.len();
        let found = reported(kind(&initial));
        let required = if directory {
            ResourceKind::Directory
        } else {
            ResourceKind::File
        };
        if found != required {
            return Err(Failure::Kind(found));
        }
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if directory {
                OFlags::DIRECTORY
            } else {
                OFlags::NONBLOCK
            };
        let descriptor =
            fs::openat(parent, name.as_str(), flags, Mode::empty()).map_err(
                |_| match fs::statat(parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
                    Ok(current) if identity(&current) == identity(&initial) => Failure::Unreadable,
                    _ => Failure::Changed,
                },
            )?;
        let status = fs::fstat(&descriptor).map_err(|_| Failure::Unreadable)?;
        if identity(&initial) != identity(&status) {
            return Err(Failure::Changed);
        }
        links.push(identity(&status));
        if directory {
            directories.push(descriptor);
        } else {
            file = Some((File::from(descriptor), physical(identity(&status))));
        }
    }
    // Verify the complete retained parent chain, including the file entry.
    for (index, expected) in links.iter().enumerate() {
        let parent = if index == 0 {
            root
        } else {
            &directories[index - 1]
        };
        let current = fs::statat(
            parent,
            anchor.remainder[index].as_str(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|_| Failure::Changed)?;
        if identity(&current) != *expected {
            return Err(Failure::Changed);
        }
    }
    let (file, physical) = file.ok_or(Failure::Kind(ResourceKind::Directory))?;
    Ok(Opened {
        file,
        key: anchor.key,
        physical,
    })
}
#[cfg(windows)]
fn open_anchor(anchor: Anchor) -> Result<Opened, Failure> {
    let root = &anchor.descriptor;
    let mut directories = Vec::new();
    let mut witnesses = Vec::new();
    let mut canonical = anchor.canonical.clone();
    let mut file = None;
    for (index, name) in anchor.remainder.iter().enumerate() {
        let parent = directories.last().unwrap_or(root);
        let initial = parent.inspect(name).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Failure::Missing
            } else {
                Failure::Unreadable
            }
        })?;
        let directory = index + 1 != anchor.remainder.len();
        let found = reported(&initial);
        let required = if directory {
            ResourceKind::Directory
        } else {
            ResourceKind::File
        };
        if found != required {
            return Err(Failure::Kind(found));
        }
        let opened_identity;
        if directory {
            let opened = parent
                .directory(name)
                .map_err(|_| match parent.inspect(name) {
                    Ok(current) if identity(&current) == identity(&initial) => Failure::Unreadable,
                    _ => Failure::Changed,
                })?;
            let physical = opened.identity().map_err(|_| Failure::Unreadable)?;
            opened_identity = (
                i128::from(physical.volume),
                i128::from_le_bytes(physical.file),
                FileType::Directory,
            );
            canonical.push(opened.name().map_err(|_| Failure::Unreadable)?);
            directories.push(opened);
        } else {
            let opened = parent
                .read_file(name)
                .map_err(|_| match parent.inspect(name) {
                    Ok(current) if identity(&current) == identity(&initial) => Failure::Unreadable,
                    _ => Failure::Changed,
                })?;
            opened_identity =
                identity(&crate::windows_file::status(&opened).map_err(|_| Failure::Unreadable)?);
            canonical
                .push(crate::windows_file::file_name(&opened).map_err(|_| Failure::Unreadable)?);
            file = Some((opened, physical(opened_identity)));
        }
        if opened_identity != identity(&initial) {
            return Err(Failure::Changed);
        }
        witnesses.push(opened_identity);
    }
    for (index, expected) in witnesses.iter().enumerate() {
        let parent = if index == 0 {
            root
        } else {
            &directories[index - 1]
        };
        let current = parent
            .inspect(&anchor.remainder[index])
            .map_err(|_| Failure::Changed)?;
        if identity(&current) != *expected {
            return Err(Failure::Changed);
        }
    }
    let (file, physical) = file.ok_or(Failure::Kind(ResourceKind::Directory))?;
    Ok(Opened {
        file,
        key: canonical,
        physical,
    })
}

#[cfg(unix)]
fn selected_root(written: &str) -> Result<Root, Error> {
    let path = crate::canonical_path::canonicalize(written).map_err(|_| Error::InvalidRoot)?;
    let canonical = directory_segments(&path).ok_or(Error::InvalidRoot)?;
    let before = fs::lstat(&path).map_err(|_| Error::InvalidRoot)?;
    let descriptor = fs::open(
        &path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| Error::InvalidRoot)?;
    let after = fs::fstat(&descriptor).map_err(|_| Error::InvalidRoot)?;
    if kind(&after) != FileType::Directory || identity(&before) != identity(&after) {
        return Err(Error::InvalidRoot);
    }
    let mut spellings = vec![canonical.clone()];
    if let Some(spelling) = directory_segments(Path::new(written)) {
        if !spellings.contains(&spelling) {
            spellings.push(spelling);
        }
    }
    Ok(Root {
        descriptor,
        canonical,
        spellings,
    })
}

/// A local drive, a mapped drive or a network share (D15): canonicalizing a
/// mapped drive reaches its share, and either opens as a held directory.
#[cfg(windows)]
fn selected_root(written: &str) -> Result<Root, Error> {
    let path = crate::canonical_path::canonicalize(written).map_err(|_| Error::InvalidRoot)?;
    let canonical = directory_segments(&path).ok_or(Error::InvalidRoot)?;
    let descriptor = OwnedFd::open(&path).map_err(|_| Error::InvalidRoot)?;
    let expected = descriptor.identity().map_err(|_| Error::InvalidRoot)?;
    let current = OwnedFd::open(&path).map_err(|_| Error::InvalidRoot)?;
    if current.identity().map_err(|_| Error::InvalidRoot)? != expected {
        return Err(Error::InvalidRoot);
    }
    let mut spellings = vec![canonical.clone()];
    if let Some(spelling) = directory_segments(Path::new(written)) {
        if !spellings.contains(&spelling) {
            spellings.push(spelling);
        }
    }
    Ok(Root {
        descriptor,
        canonical,
        spellings,
    })
}

#[cfg(unix)]
fn kind(status: &fs::Stat) -> FileType {
    FileType::from_raw_mode(status.st_mode)
}
#[cfg(unix)]
fn identity(status: &fs::Stat) -> Identity {
    (
        i128::from(status.st_dev),
        i128::from(status.st_ino),
        kind(status),
    )
}
#[cfg(windows)]
fn identity(status: &crate::windows_file::Status) -> Identity {
    (
        i128::from(status.identity.volume),
        i128::from_le_bytes(status.identity.file),
        status.kind,
    )
}
fn directory_segments(path: &Path) -> Option<Segments> {
    // Match host directory selection: normalized root aliases retain their authority.
    let normalized: std::path::PathBuf = path.components().collect();
    #[cfg(unix)]
    {
        rooted_segments(normalized.to_str()?)
    }
    #[cfg(windows)]
    {
        // Identities never carry the verbatim prefix, whatever their length:
        // every open goes through a held directory handle, never this text.
        let text = normalized.to_str()?;
        let text =
            crate::canonical_path::without_verbatim_prefix(text).unwrap_or_else(|| text.to_owned());
        rooted_segments(&text.replace('\\', "/"))
    }
}
/// Convert an absolute caller-owned native path to the resource spelling. YAML
/// paths still enter `anchors` unchanged and keep their strict separator rules.
pub fn caller_path(path: &Path) -> Result<String, Error> {
    #[cfg(unix)]
    let text = path.to_str().ok_or(Error::InvalidPath)?.to_owned();
    #[cfg(windows)]
    let text = path_text(&directory_segments(path).ok_or(Error::InvalidPath)?);
    Ok(text)
}
fn rooted_segments(path: &str) -> Option<Segments> {
    split_rooted(path, cfg!(windows))
}
/// A network-share root `//server/share` and what follows it, when `shares`
/// admits shares and `path` starts with one: a nonempty server and share,
/// neither `.`, `..` nor `?`, so device (`//./`) and verbatim (`//?/`)
/// namespaces are not shares.
fn share_root(path: &str, shares: bool) -> Option<(&str, &str)> {
    if !shares {
        return None;
    }
    let (server, after) = path.strip_prefix("//")?.split_once('/')?;
    let share = after.split('/').next().unwrap_or_default();
    if [server, share]
        .iter()
        .any(|s| s.is_empty() || matches!(*s, "." | ".." | "?"))
    {
        return None;
    }
    Some(path.split_at(2 + server.len() + 1 + share.len()))
}
/// The root marker and segments of a rooted path in the engine's spelling:
/// `/` (marker ``), a drive (`C:`) or, when `shares` admits them, a network
/// share (`//server/share`, D15). A root with nothing below it has no
/// segments. Any other spelling is not rooted.
pub(crate) fn split_rooted(path: &str, shares: bool) -> Option<Segments> {
    if path.starts_with("//") && shares {
        let (root, rest) = share_root(path, true)?;
        let mut result = vec![root.to_owned()];
        if let Some(rest) = rest.strip_prefix('/').filter(|rest| !rest.is_empty()) {
            result.extend(rest.split('/').map(str::to_owned));
        }
        return Some(result);
    }
    let (marker, rest) = if let Some(rest) = path.strip_prefix('/') {
        ("", rest)
    } else if path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes().get(2) == Some(&b'/')
    {
        (&path[..2], &path[3..])
    } else {
        return None;
    };
    let mut result = vec![marker.into()];
    if !rest.is_empty() {
        result.extend(rest.split('/').map(str::to_owned));
    }
    Some(result)
}
/// The physical file a walk identity names: device and inode on Unix, volume
/// serial number and file ID on Windows.
fn physical((volume, file, _): Identity) -> PhysicalFile {
    PhysicalFile { volume, file }
}
/// Whether `file` holds exactly `content`, from its current position: read
/// in bounded chunks and compared as they arrive, so confirming a capture
/// never holds a second copy of the file.
fn holds(file: &mut File, content: &[u8]) -> std::io::Result<bool> {
    let mut buffer = [0u8; 65_536];
    let mut offset = 0;
    loop {
        // One byte past the expected end shows a file that grew.
        let wanted = (content.len() - offset).saturating_add(1).min(buffer.len());
        let count = file.read(&mut buffer[..wanted])?;
        if count == 0 {
            return Ok(offset == content.len());
        }
        if count > content.len() - offset || buffer[..count] != content[offset..offset + count] {
            return Ok(false);
        }
        offset += count;
    }
}
fn read_bounded(file: &mut File, maximum: usize) -> Result<Vec<u8>, Failure> {
    let length = file.metadata().map_err(|_| Failure::Unreadable)?.len();
    if length > maximum as u64 {
        return Err(Failure::Limit(Some(length)));
    }
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let remaining = maximum
            .saturating_sub(bytes.len())
            .saturating_add(1)
            .min(buffer.len());
        let count = file
            .read(&mut buffer[..remaining])
            .map_err(|_| Failure::Unreadable)?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > maximum.saturating_sub(bytes.len()) {
            return Err(Failure::Limit(Some(bytes.len() as u64 + count as u64)));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn within(path: &[String], root: &[String]) -> bool {
    if path.len() < root.len() {
        return false;
    }
    path.iter().zip(root).all(|(left, right)| {
        #[cfg(unix)]
        {
            left == right
        }
        #[cfg(windows)]
        {
            crate::windows_file::equal_name(left, right)
        }
    })
}

fn path_text(segments: &[String]) -> String {
    format!("{}/{}", segments[0], segments[1..].join("/"))
}
/// The identity of the directory `segments`, from which undeclared requests
/// resolve: its path with a trailing separator, naming no file in it.
fn directory_identity(segments: &[String]) -> String {
    let text = path_text(segments);
    if text.ends_with('/') {
        text
    } else {
        format!("{text}/")
    }
}
/// The directory a declaring identity's relative paths resolve from: the
/// parent of a canonical file, or the directory a base identity names.
fn declaring_directory(identity: &str) -> Option<Segments> {
    let mut segments = rooted_segments(identity)?;
    if segments.len() > 1 {
        segments.pop();
    }
    (!segments[1..]
        .iter()
        .any(|s| s.is_empty() || s == "." || s == ".."))
    .then_some(segments)
}
/// The identity a walk gives the file `name` in the canonical directory
/// `parent`: its rooted segments in the engine's spelling. Opens nothing.
pub(crate) fn file_identity(parent: &Path, name: &str) -> Option<String> {
    let mut segments = directory_segments(parent)?;
    segments.push(name.to_owned());
    Some(path_text(&segments))
}
/// The identity of the canonical directory `path`, from which undeclared
/// requests resolve: its rooted segments in the engine's spelling with a
/// trailing separator, so it starts the identity a walk gives every file
/// beneath it. Opens nothing.
pub(crate) fn canonical_directory_identity(path: &Path) -> Option<String> {
    directory_segments(path).map(|segments| directory_identity(&segments))
}

#[cfg(test)]
mod classification_tests {
    use super::*;

    #[test]
    fn rooted_spellings_split_into_one_root_and_their_segments_on_every_host() {
        let split = |path, shares| split_rooted(path, shares);
        for shares in [false, true] {
            assert_eq!(split("/", shares), Some(vec![String::new()]));
            assert_eq!(
                split("/study/spec.yaml", shares),
                Some(vec!["".into(), "study".into(), "spec.yaml".into()])
            );
            assert_eq!(split("C:/", shares), Some(vec!["C:".into()]));
            assert_eq!(
                split("C:/study/spec.yaml", shares),
                Some(vec!["C:".into(), "study".into(), "spec.yaml".into()])
            );
            assert_eq!(split("study/spec.yaml", shares), None);
            assert_eq!(split("C:study", shares), None);
        }
        // A network share is one root, with or without a trailing separator.
        assert_eq!(
            split("//server/share", true),
            Some(vec!["//server/share".into()])
        );
        assert_eq!(
            split("//server/share/", true),
            Some(vec!["//server/share".into()])
        );
        assert_eq!(
            split("//server/share/study/spec.yaml", true),
            Some(vec![
                "//server/share".into(),
                "study".into(),
                "spec.yaml".into()
            ])
        );
        // Without shares, a doubled separator is an empty segment below `/`.
        assert_eq!(
            split("//server/share", false),
            Some(vec!["".into(), "".into(), "server".into(), "share".into()])
        );
        // Device and verbatim namespaces, and incomplete shares, are not roots.
        for path in [
            "//",
            "//server",
            "//server/",
            "///share",
            "//./pipe/name",
            "//?/C:/study",
            "//?/UNC/server/share",
            "//../share",
            "//server/..",
            "//server/./study",
        ] {
            assert_eq!(split(path, true), None, "{path}");
        }
    }

    #[test]
    fn share_roots_are_judged_like_a_leading_separator() {
        assert_eq!(
            share_root("//server/share/study/spec.yaml", true),
            Some(("//server/share", "/study/spec.yaml"))
        );
        assert_eq!(
            share_root("//server/share", true),
            Some(("//server/share", ""))
        );
        assert_eq!(share_root("//server/share", false), None);
        assert_eq!(share_root("/server/share", true), None);
        assert_eq!(share_root("//./pipe/name", true), None);
        // What follows the share keeps the written-form rules.
        let (_, rest) = share_root("//server/share/a//b", true).unwrap();
        assert!(yamaa_core::resource::written_path_failure(rest).is_some());
        let (_, rest) = share_root("//server/share/study/spec.yaml", true).unwrap();
        assert_eq!(yamaa_core::resource::written_path_failure(rest), None);
    }

    #[test]
    fn declaring_identities_name_a_file_or_a_selected_directory() {
        let directory = declaring_directory;
        assert_eq!(
            directory("/study/spec.yaml"),
            Some(vec!["".into(), "study".into()])
        );
        assert_eq!(directory("/study/"), Some(vec!["".into(), "study".into()]));
        assert_eq!(directory("/spec.yaml"), Some(vec![String::new()]));
        assert_eq!(directory("/"), Some(vec![String::new()]));
        assert_eq!(directory("C:/"), Some(vec!["C:".into()]));
        assert_eq!(directory("C:/spec.yaml"), Some(vec!["C:".into()]));
        assert_eq!(directory("relative/spec.yaml"), None);
        assert_eq!(directory("/a//spec.yaml"), None);
        assert_eq!(directory("/a/../spec.yaml"), None);
        for segments in [
            vec![String::new()],
            vec!["".into(), "study".into()],
            vec!["C:".into()],
            vec!["C:".into(), "study".into()],
        ] {
            assert_eq!(directory(&directory_identity(&segments)), Some(segments));
        }
    }

    #[test]
    fn reparse_tags_name_the_link_kind_and_every_one_is_refused() {
        assert_eq!(reparse_kind(0xA000_000C), ResourceKind::SymbolicLink);
        assert_eq!(reparse_kind(0xA000_0003), ResourceKind::Junction);
        // Deduplication, cloud placeholders and any unknown tag.
        for tag in [0x8000_0013, 0x9000_001A, 0, u32::MAX] {
            assert_eq!(reparse_kind(tag), ResourceKind::ReparsePoint);
        }
        for tag in [0xA000_000C, 0xA000_0003, 0x8000_0013] {
            assert!(reparse_kind(tag).is_link());
        }
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;
    use crate::host_resources::Kind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-file-resolver-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(crate::canonical_path::canonicalize(path).unwrap())
        }
        fn identity(&self, name: &str) -> String {
            file_identity(&self.0, name).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ask(
        resolver: &FileResolver,
        base: &str,
        operation: Operation,
        written: &str,
        limit: Option<usize>,
    ) -> Result<Reply, Failure> {
        resolver.request(Request {
            operation,
            kind: Kind::Input,
            declaring: base,
            written,
            reference: None,
            bytes: None,
            limit,
        })
    }

    #[test]
    fn walks_report_the_kind_of_the_entry_that_stops_them() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("data.csv"), b"ID\n1\n").unwrap();
        std::fs::create_dir(fixture.0.join("folder")).unwrap();
        std::fs::write(fixture.0.join("folder/inner.csv"), b"ID\n2\n").unwrap();
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut refused = vec![
            ("folder", ResourceKind::Directory),
            ("data.csv/inner.csv", ResourceKind::File),
        ];
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("data.csv", fixture.0.join("link.csv")).unwrap();
            std::os::unix::fs::symlink("folder", fixture.0.join("linked")).unwrap();
            let _socket = std::os::unix::net::UnixListener::bind(fixture.0.join("socket")).unwrap();
            refused.extend([
                ("link.csv", ResourceKind::SymbolicLink),
                ("linked/inner.csv", ResourceKind::SymbolicLink),
                ("socket", ResourceKind::Other),
            ]);
        }
        let (resolver, base) = FileResolver::for_entry(fixture.0.to_str().unwrap()).unwrap();
        for operation in [Operation::Inspect, Operation::Capture] {
            for (written, identity) in [
                ("data.csv", "data.csv"),
                ("./folder/../data.csv", "data.csv"),
                ("folder/inner.csv", "folder/inner.csv"),
            ] {
                let reply = ask(&resolver, &base, operation, written, None).unwrap();
                assert_eq!(reply.reference, fixture.identity(identity), "{written}");
                assert_eq!(reply.location.as_deref(), Some(reply.reference.as_str()));
                assert_eq!(reply.bytes.is_some(), operation == Operation::Capture);
            }
            for (written, kind) in &refused {
                assert_eq!(
                    ask(&resolver, &base, operation, written, None).unwrap_err(),
                    Failure::Kind(*kind),
                    "{written}"
                );
            }
            assert_eq!(
                ask(&resolver, &base, operation, "absent.csv", None).unwrap_err(),
                Failure::Missing
            );
            assert_eq!(
                ask(&resolver, &base, operation, "a//b.csv", None).unwrap_err(),
                Failure::Path(PathRefusal::WrittenForm(
                    yamaa_core::resource::ResourceFailure::EmptySegment
                ))
            );
        }
        // A capture refuses a file over its limit before reading it.
        assert_eq!(
            ask(&resolver, &base, Operation::Capture, "data.csv", Some(4)).unwrap_err(),
            Failure::Limit(Some(5))
        );
        // A held reference the walk no longer reaches has changed.
        let changed = resolver.request(Request {
            operation: Operation::Capture,
            kind: Kind::Input,
            declaring: &base,
            written: "data.csv",
            reference: Some("/elsewhere/data.csv"),
            bytes: None,
            limit: None,
        });
        assert_eq!(changed.unwrap_err(), Failure::Changed);
        // Publication goes through anchored targets, never through requests.
        assert_eq!(
            ask(&resolver, &base, Operation::Target, "out.csv", None).unwrap_err(),
            Failure::Rejected
        );
    }

    #[test]
    fn walks_report_the_physical_file_they_reach() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("data.csv"), b"ID\n1\n").unwrap();
        std::fs::hard_link(fixture.0.join("data.csv"), fixture.0.join("link.csv")).unwrap();
        std::fs::write(fixture.0.join("copy.csv"), b"ID\n1\n").unwrap();
        let (resolver, base) = FileResolver::for_entry(fixture.0.to_str().unwrap()).unwrap();
        for operation in [Operation::Inspect, Operation::Capture] {
            let physical = |written| {
                ask(&resolver, &base, operation, written, None)
                    .unwrap()
                    .physical
                    .unwrap()
            };
            // Two hard links are one physical file under two references.
            assert_eq!(physical("data.csv"), physical("link.csv"));
            assert_eq!(physical("data.csv"), physical("./data.csv"));
            assert_ne!(physical("data.csv"), physical("copy.csv"));
        }
    }

    #[test]
    fn a_confirming_read_compares_every_byte_and_the_length() {
        let fixture = Fixture::new();
        let content = (0..200_000_u32)
            .map(|n| (n % 251) as u8)
            .collect::<Vec<_>>();
        let path = fixture.0.join("data.bin");
        std::fs::write(&path, &content).unwrap();
        let open = || File::open(&path).unwrap();
        assert!(holds(&mut open(), &content).unwrap());
        let mut other = content.clone();
        other[150_000] ^= 1;
        assert!(!holds(&mut open(), &other).unwrap());
        // A file longer or shorter than the held bytes holds other content.
        assert!(!holds(&mut open(), &content[..content.len() - 1]).unwrap());
        assert!(!holds(&mut open(), &[content.as_slice(), b"x"].concat()).unwrap());
        std::fs::write(&path, b"").unwrap();
        assert!(holds(&mut open(), b"").unwrap());
        assert!(!holds(&mut open(), b"x").unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn walks_report_junctions_and_reparse_points_as_links() {
        use std::os::windows::process::CommandExt;
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.0.join("folder")).unwrap();
        std::fs::write(fixture.0.join("folder/inner.csv"), b"ID\n2\n").unwrap();
        let link = fixture.0.join("joined");
        let output = std::process::Command::new("cmd")
            .arg("/d")
            .arg("/c")
            .raw_arg(format!(
                "mklink /J \"{}\" \"{}\"",
                link.display(),
                fixture.0.join("folder").display()
            ))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        // Reparse points other than a junction: symbolic links to a directory
        // and to a file, each refused on the way to a name and as the name.
        std::os::windows::fs::symlink_dir(fixture.0.join("folder"), fixture.0.join("linked"))
            .unwrap();
        std::os::windows::fs::symlink_file(
            fixture.0.join("folder/inner.csv"),
            fixture.0.join("link.csv"),
        )
        .unwrap();
        let (resolver, base) = FileResolver::for_entry(fixture.0.to_str().unwrap()).unwrap();
        for operation in [Operation::Inspect, Operation::Capture] {
            for (written, kind) in [
                ("joined/inner.csv", ResourceKind::Junction),
                ("joined", ResourceKind::Junction),
                ("linked/inner.csv", ResourceKind::SymbolicLink),
                ("linked", ResourceKind::SymbolicLink),
                ("link.csv", ResourceKind::SymbolicLink),
            ] {
                assert_eq!(
                    ask(&resolver, &base, operation, written, None).unwrap_err(),
                    Failure::Kind(kind),
                    "{written}"
                );
            }
            assert!(ask(&resolver, &base, operation, "folder/inner.csv", None).is_ok());
        }
        std::fs::remove_dir(link).unwrap();
        std::fs::remove_dir(fixture.0.join("linked")).unwrap();
        std::fs::remove_file(fixture.0.join("link.csv")).unwrap();
    }
}
