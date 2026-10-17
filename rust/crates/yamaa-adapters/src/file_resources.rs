//! Descriptor-anchored bytes with retained local-path and snapshot authority.
//! Captured equality compares retained bytes. This module interprets no study data.
#[cfg(windows)]
use crate::windows_file::{Directory as OwnedFd, Kind as FileType};
#[cfg(unix)]
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    sync::Arc,
};

use yamaa_core::limits::{LimitExceeded, Limits};

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Host(crate::host_resources::Failure),
    Missing,
    Unreadable,
    NotRegularFile,
    InvalidRoot,
    InvalidBase,
    InvalidPath,
    WrittenForm(yamaa_core::resource::ResourceFailure),
    InvalidLock,
    OutsideRoots,
    Symlink,
    Changed,
    Limit(LimitExceeded),
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Host(_) => "host resource request failed",
            Self::Missing => "resource path missing",
            Self::Unreadable => "resource path is not readable",
            Self::NotRegularFile => "resource path is not a regular file",
            Self::InvalidRoot => "invalid approved resource root",
            Self::InvalidBase => "invalid resource base directory",
            Self::InvalidLock => "invalid packaging lock syntax",
            Self::InvalidPath => "invalid resource path spelling",
            Self::WrittenForm(_) => "invalid resource path spelling",
            Self::OutsideRoots => "resource path outside approved roots",
            Self::Symlink => "resource path contains a symbolic link",
            Self::Changed => "captured resource content changed",
            Self::Limit(_) => "resource capture limit",
        }
    }
}
// A genuinely absent entry permits fallback; other observed IO failures are terminal.
enum WalkError {
    NoEntry,
    Failure(Error),
}
impl From<Error> for WalkError {
    fn from(error: Error) -> Self {
        Self::Failure(error)
    }
}
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
    identity: Identity,
}
struct Snapshot {
    bytes: Arc<[u8]>,
    paths: Vec<(Segments, String, Segments)>,
}

pub struct Resources {
    host: Option<crate::host_resources::Resources>,
    host_sources: RetainedSources,
    roots: Vec<Root>,
    local_paths: bool,
    base: Segments,
    snapshots: Vec<Snapshot>,
    by_path: BTreeMap<Segments, usize>,
    by_identity: BTreeMap<(i128, i128), usize>,
    reads: usize,
}

pub type RetainedSources = Vec<(String, Arc<[u8]>)>;
impl Resources {
    pub fn with_resolver(resolver: Arc<dyn crate::host_resources::Resolver>, base: &str) -> Self {
        Self {
            host_sources: vec![],
            host: Some(crate::host_resources::Resources::new(resolver, base.into())),
            roots: vec![],
            local_paths: false,
            base: vec![],
            snapshots: vec![],
            by_path: BTreeMap::new(),
            by_identity: BTreeMap::new(),
            reads: 0,
        }
    }

    /// Public entry paths select their existing parent once. Document paths
    /// resolve at their declaring file without configuration or fallback roots.
    /// The retained parent admits caller-owned aliases; every declared component
    /// outside it is walked from its filesystem root with the same link checks.
    pub fn for_entry(base: &str) -> Result<Self, Error> {
        let mut resources = Self::new(base, base, &[])?;
        resources.local_paths = true;
        Ok(resources)
    }
    /// The first root is the project root; later roots retain fallback order.
    pub fn new(root: &str, base: &str, data_roots: &[String]) -> Result<Self, Error> {
        let mut roots = Vec::new();
        for written in std::iter::once(root).chain(data_roots.iter().map(String::as_str)) {
            roots.push(selected_root(written)?);
        }
        let path = crate::canonical_path::canonicalize(base).map_err(|_| Error::InvalidBase)?;
        if !path.is_dir() {
            return Err(Error::InvalidBase);
        }
        let base = directory_segments(&path).ok_or(Error::InvalidBase)?;
        Ok(Self {
            host: None,
            host_sources: vec![],
            roots,
            local_paths: false,
            base,
            snapshots: Vec::new(),
            by_path: BTreeMap::new(),
            by_identity: BTreeMap::new(),
            reads: 0,
        })
    }
    pub fn capture_reads(&self) -> usize {
        self.reads
    }
    pub(crate) fn snapshot_count(&self) -> usize {
        self.host
            .as_ref()
            .map_or(self.snapshots.len(), crate::host_resources::Resources::len)
    }
    /// Immutable raw prefix evidence survives failed decoding and inheritance.
    /// This view grants no resource, execution or publication port.
    pub(crate) fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.snapshots
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .paths
                    .iter()
                    .map(|(key, _, _)| (path_text(key), snapshot.bytes.as_ref()))
            })
            .chain(
                self.host_sources
                    .iter()
                    .map(|(name, bytes)| (name.clone(), bytes.as_ref())),
            )
    }
    pub(crate) fn verify_captured(&self) -> Result<(), (String, Error)> {
        self.verify_prefix(self.snapshot_count())
    }
    pub(crate) fn verify_prefix(&self, count: usize) -> Result<(), (String, Error)> {
        if let Some(host) = &self.host {
            return host
                .verify(count)
                .map_err(|(path, error)| (path, Error::Host(error)));
        }
        if count > self.snapshots.len() {
            return Err((String::new(), Error::InvalidPath));
        }
        for index in 0..count {
            self.verify_paths(index)?;
        }
        Ok(())
    }
    /// Own immutable raw evidence without copying content or reopening files.
    pub(crate) fn retain_sources(&self) -> RetainedSources {
        if let Some(host) = &self.host {
            return host.sources();
        }
        self.snapshots
            .iter()
            .flat_map(|snapshot| {
                snapshot
                    .paths
                    .iter()
                    .map(|(key, _, _)| (path_text(key), Arc::clone(&snapshot.bytes)))
            })
            .collect()
    }
    /// Metadata only: no bytes read and no snapshot created.
    pub fn inspect(&self, written: &str) -> Result<(), Error> {
        if let Some(host) = &self.host {
            return host
                .resolve(&host.base, written, crate::host_resources::Kind::Input)
                .map(drop)
                .map_err(Error::Host);
        }
        self.open(written).map(drop)
    }
    /// Inspect relative to a canonical declaring entry without changing another
    /// node's resource base, reading bytes or creating a snapshot.
    pub fn inspect_from(&self, declaring: &str, written: &str) -> Result<(), Error> {
        if let Some(host) = &self.host {
            return host
                .resolve(declaring, written, crate::host_resources::Kind::Input)
                .map(drop)
                .map_err(Error::Host);
        }
        self.open_at(&file_base(declaring)?, written).map(drop)
    }
    /// Capture source bytes under the fixed per-file source boundary.
    pub fn capture(&mut self, written: &str) -> Result<(Arc<[u8]>, bool), Error> {
        if let Some(host) = &self.host {
            let result = host
                .capture(
                    &host.base,
                    written,
                    crate::host_resources::Kind::Input,
                    Limits::SOURCE_BYTES,
                )
                .map_err(Error::Host)?;
            self.reads += usize::from(result.1);
            self.host_sources = host.sources();
            return Ok(result);
        }
        self.capture_at(
            self.base.clone(),
            written,
            "source_bytes",
            Limits::SOURCE_BYTES,
        )
    }
    /// Capture specification bytes under the fixed per-file metadata boundary.
    pub fn capture_metadata(&mut self, written: &str) -> Result<(Arc<[u8]>, bool), Error> {
        if let Some(host) = &self.host {
            let result = host
                .capture(
                    &host.base,
                    written,
                    crate::host_resources::Kind::Metadata,
                    Limits::SPEC_BYTES,
                )
                .map_err(Error::Host)?;
            self.reads += usize::from(result.1);
            self.host_sources = host.sources();
            return Ok(result);
        }
        self.capture_at(self.base.clone(), written, "spec_bytes", Limits::SPEC_BYTES)
    }
    /// A declaring identity comes from this reader's canonical regular-file resolution.
    pub fn resolve_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        if let Some(host) = &self.host {
            return host
                .resolve(declaring, written, crate::host_resources::Kind::Metadata)
                .map_err(Error::Host);
        }
        let base = file_base(declaring)?;
        let opened = self.open_at(&base, written)?;
        Ok(path_text(&opened.key))
    }
    pub fn resolve(&self, written: &str) -> Result<String, Error> {
        if let Some(host) = &self.host {
            return host
                .resolve(&host.base, written, crate::host_resources::Kind::Metadata)
                .map_err(Error::Host);
        }
        Ok(path_text(&self.open(written)?.key))
    }
    /// A lexical declaration location under the selected resolution policy.
    /// The target may be absent. This fact opens no file and grants no byte or
    /// publication authority; declaring identities come from this reader.
    pub fn location_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        if self.host.is_some() {
            return crate::host_resources::Resources::location(declaring, written)
                .map_err(Error::Host);
        }
        let anchor = self.declaration_anchor(&file_base(declaring)?, written)?;
        Ok(path_text(&anchor.key))
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
    /// Publication holds the declaration location, including absent targets.
    /// Retain its physical descriptor; the parent chain is checked at save.
    /// Retain the original declaring-layer location without rebasing through
    /// another entry. The declaring identity comes from this reader.
    pub(crate) fn publication_target_from(
        &self,
        declaring: &str,
        written: &str,
    ) -> Result<crate::resource_publication::AnchoredTarget, Error> {
        if let Some(host) = &self.host {
            return Ok(crate::resource_publication::AnchoredTarget::Host {
                resolver: Arc::clone(&host.resolver),
                declaring: declaring.into(),
                written: written.into(),
                reference: host.target(declaring, written).map_err(Error::Host)?,
            });
        }
        let anchor = self.declaration_anchor(&file_base(declaring)?, written)?;
        Ok(crate::resource_publication::AnchoredTarget::File(
            crate::file_publication::AnchoredTarget::new(
                anchor.descriptor,
                anchor.remainder,
                path_text(&anchor.key),
            ),
        ))
    }
    pub fn capture_from(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        if let Some(host) = &self.host {
            let result = host
                .capture(
                    declaring,
                    written,
                    crate::host_resources::Kind::Input,
                    Limits::SOURCE_BYTES,
                )
                .map_err(Error::Host)?;
            self.reads += usize::from(result.1);
            self.host_sources = host.sources();
            return Ok(result);
        }
        self.capture_at(
            file_base(declaring)?,
            written,
            "source_bytes",
            Limits::SOURCE_BYTES,
        )
    }
    pub fn capture_metadata_from(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        if let Some(host) = &self.host {
            let result = host
                .capture(
                    declaring,
                    written,
                    crate::host_resources::Kind::Metadata,
                    Limits::SPEC_BYTES,
                )
                .map_err(Error::Host)?;
            self.reads += usize::from(result.1);
            self.host_sources = host.sources();
            return Ok(result);
        }
        self.capture_at(
            file_base(declaring)?,
            written,
            "spec_bytes",
            Limits::SPEC_BYTES,
        )
    }
    /// Keep the same path authority and captures for data declared by this entry.
    pub fn select_entry_base(&mut self, identity: &str) -> Result<(), Error> {
        if let Some(host) = &mut self.host {
            host.base = identity.into();
            return Ok(());
        }
        let base = file_base(identity)?;
        // This is a canonical identity returned by the reader, not an authored
        // path. Selected physical root names can contain a literal backslash on
        // Unix. Keep the canonical identity and descriptor/link checks intact.
        let mut segments = rooted_segments(identity).ok_or(Error::InvalidPath)?;
        let marker = segments.remove(0);
        let anchor = if self.local_paths {
            self.local_anchor(&[marker], &segments.join("/"))?
        } else {
            self.relative_anchor(&[marker], &segments.join("/"))?
        };
        self.open_anchor(anchor).map_err(|error| match error {
            WalkError::NoEntry => Error::Missing,
            WalkError::Failure(error) => error,
        })?;
        self.base = base;
        Ok(())
    }
    fn capture_at(
        &mut self,
        base: Segments,
        written: &str,
        resource: &'static str,
        maximum: usize,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        let refusal = |actual| {
            Error::Limit(LimitExceeded {
                resource,
                limit: maximum,
                actual: usize::try_from(actual).ok(),
            })
        };
        let mut opened = self.open_at(&base, written)?;
        let length = opened.file.metadata().map_err(|_| Error::Unreadable)?.len();
        if !self.by_path.contains_key(&opened.key) && length > maximum as u64 {
            return Err(refusal(length));
        }
        let physical = (opened.identity.0, opened.identity.1);
        let accepted = if let Some(&index) = self.by_path.get(&opened.key) {
            Some(index)
        } else if let Some(&index) = self.by_identity.get(&physical) {
            // Numeric inode identities may be reused after unlink/replacement.
            // Only a current physical witness and fully valid old aliases allow
            // reuse. Previously captured path keys always retain their snapshot.
            if length == self.snapshots[index].bytes.len() as u64
                && self.physical_witness(index, physical)
                && self.verify(index).is_ok()
            {
                Some(index)
            } else {
                self.by_identity.remove(&physical);
                None
            }
        } else {
            None
        };
        if let Some(index) = accepted {
            let known_alias = self.snapshots[index]
                .paths
                .iter()
                .any(|(key, path, from)| key == &opened.key && path == written && from == &base);
            if self.snapshots[index].bytes.len() > maximum {
                return Err(refusal(self.snapshots[index].bytes.len() as u64));
            }
            let bytes = Arc::clone(&self.snapshots[index].bytes);
            if read_bounded(&mut opened.file, bytes.len(), |_| Error::Changed)
                .map_err(|_| Error::Changed)?
                .as_slice()
                != bytes.as_ref()
            {
                return Err(Error::Changed);
            }
            self.verify(index)?;
            if !known_alias {
                self.snapshots[index]
                    .paths
                    .push((opened.key.clone(), written.into(), base));
            }
            self.by_path.insert(opened.key, index);
            return Ok((bytes, false));
        }
        let content = read_bounded(&mut opened.file, maximum, refusal)?;
        let mut current = self.open_at(&base, written).map_err(|_| Error::Changed)?;
        if current.key != opened.key
            || read_bounded(&mut current.file, content.len(), |_| Error::Changed)
                .map_err(|_| Error::Changed)?
                .as_slice()
                != content.as_slice()
        {
            return Err(Error::Changed);
        }
        self.reads += 1;
        let bytes: Arc<[u8]> = content.into();
        let index = self.snapshots.len();
        self.snapshots.push(Snapshot {
            bytes: Arc::clone(&bytes),
            paths: vec![(opened.key.clone(), written.into(), base)],
        });
        self.by_identity.insert(physical, index);
        self.by_path.insert(opened.key, index);
        Ok((bytes, true))
    }
    fn verify(&self, index: usize) -> Result<(), Error> {
        self.verify_paths(index).map_err(|(_, error)| error)
    }
    fn verify_paths(&self, index: usize) -> Result<(), (String, Error)> {
        let snapshot = &self.snapshots[index];
        for (key, written, base) in &snapshot.paths {
            let result = (|| {
                let mut opened = self.open_at(base, written).map_err(|_| Error::Changed)?;
                if &opened.key != key
                    || read_bounded(&mut opened.file, snapshot.bytes.len(), |_| Error::Changed)
                        .map_err(|_| Error::Changed)?
                        .as_slice()
                        != snapshot.bytes.as_ref()
                {
                    return Err(Error::Changed);
                }
                Ok(())
            })();
            result.map_err(|error| (path_text(key), error))?;
        }
        Ok(())
    }
    fn physical_witness(&self, index: usize, physical: (i128, i128)) -> bool {
        for (key, written, base) in &self.snapshots[index].paths {
            match self.open_at(base, written) {
                Ok(opened)
                    if opened.key == *key && (opened.identity.0, opened.identity.1) == physical =>
                {
                    return true
                }
                _ => {}
            }
        }
        false
    }
    fn anchors(&self, base: &[String], written: &str) -> Result<Vec<Anchor>, Error> {
        if let Some(failure) = yamaa_core::resource::written_path_failure(written) {
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
        if let Some(segments) = rooted {
            let mut selected = None;
            for (index, root) in self.roots.iter().enumerate() {
                for spelling in &root.spellings {
                    if within(&segments, spelling)
                        && selected.is_none_or(|(_, depth)| spelling.len() > depth)
                    {
                        selected = Some((index, spelling.len()));
                    }
                }
            }
            let (root, depth) = selected.ok_or(Error::OutsideRoots)?;
            return Ok(vec![self.anchor(root, segments[depth..].to_vec())?]);
        }
        let mut anchors = Vec::new();
        let inside = self.roots.iter().any(|r| within(base, &r.canonical));
        match self.relative_anchor(base, written) {
            Ok(anchor) => anchors.push(anchor),
            Err(error) if inside => return Err(error),
            Err(_) => {}
        }
        let mut seen = anchors
            .iter()
            .map(|a| a.key.clone())
            .collect::<BTreeSet<_>>();
        for root in &self.roots {
            if let Ok(anchor) = self.relative_anchor(&root.canonical, written) {
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
        for (index, root) in self.roots.iter().enumerate() {
            for spelling in &root.spellings {
                if within(&resolved, spelling) {
                    return self.anchor(index, resolved[spelling.len()..].to_vec());
                }
            }
        }
        // UNC resources outside the caller-selected directory are not a new
        // network-resource capability under the local-path decision.
        if resolved[0].starts_with("//") {
            return Err(Error::InvalidPath);
        }
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
    fn relative_anchor(&self, start: &[String], written: &str) -> Result<Anchor, Error> {
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
        for (index, root) in self.roots.iter().enumerate() {
            if within(&resolved, &root.canonical)
                && selected.is_none_or(|(_, depth)| root.canonical.len() > depth)
            {
                selected = Some((index, root.canonical.len()));
            }
        }
        let (root, depth) = selected.ok_or(Error::OutsideRoots)?;
        self.anchor(root, resolved[depth..].to_vec())
    }
    fn anchor(&self, root: usize, remainder: Segments) -> Result<Anchor, Error> {
        if remainder.is_empty() {
            return Err(Error::NotRegularFile);
        }
        let mut key = self.roots[root].canonical.clone();
        key.extend_from_slice(&remainder);
        Ok(Anchor {
            descriptor: self.roots[root]
                .descriptor
                .try_clone()
                .map_err(|_| Error::Unreadable)?,
            #[cfg(windows)]
            canonical: self.roots[root].canonical.clone(),
            key,
            remainder,
        })
    }
    fn open(&self, written: &str) -> Result<Opened, Error> {
        self.open_at(&self.base, written)
    }
    fn open_at(&self, base: &[String], written: &str) -> Result<Opened, Error> {
        for anchor in self.anchors(base, written)? {
            match self.open_anchor(anchor) {
                Err(WalkError::NoEntry) => continue,
                Err(WalkError::Failure(error)) => return Err(error),
                Ok(opened) => return Ok(opened),
            }
        }
        Err(Error::Missing)
    }
    #[cfg(unix)]
    fn open_anchor(&self, anchor: Anchor) -> Result<Opened, WalkError> {
        let root = &anchor.descriptor;
        let mut directories = Vec::new();
        let mut links = Vec::new();
        let mut file = None;
        for (index, name) in anchor.remainder.iter().enumerate() {
            let parent = directories.last().unwrap_or(root);
            let initial =
                fs::statat(parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
                    if matches!(error, rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR) {
                        WalkError::NoEntry
                    } else {
                        WalkError::Failure(Error::Unreadable)
                    }
                })?;
            if kind(&initial) == FileType::Symlink {
                return Err(Error::Symlink.into());
            }
            let directory = index + 1 != anchor.remainder.len();
            if kind(&initial)
                != if directory {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                }
            {
                return Err(Error::NotRegularFile.into());
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
                fs::openat(parent, name.as_str(), flags, Mode::empty()).map_err(|_| {
                    match fs::statat(parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
                        Ok(current) if identity(&current) == identity(&initial) => {
                            Error::Unreadable
                        }
                        _ => Error::Changed,
                    }
                })?;
            let status = fs::fstat(&descriptor).map_err(|_| Error::Unreadable)?;
            if identity(&initial) != identity(&status) {
                return Err(Error::Changed.into());
            }
            links.push(identity(&status));
            if directory {
                directories.push(descriptor);
            } else {
                file = Some((File::from(descriptor), identity(&status)));
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
            .map_err(|_| Error::Changed)?;
            if identity(&current) != *expected {
                return Err(Error::Changed.into());
            }
        }
        let (file, identity) = file.ok_or(Error::NotRegularFile)?;
        Ok(Opened {
            file,
            identity,
            key: anchor.key,
        })
    }
    #[cfg(windows)]
    fn open_anchor(&self, anchor: Anchor) -> Result<Opened, WalkError> {
        let root = &anchor.descriptor;
        let mut directories = Vec::new();
        let mut witnesses = Vec::new();
        let mut canonical = anchor.canonical.clone();
        let mut file = None;
        for (index, name) in anchor.remainder.iter().enumerate() {
            let parent = directories.last().unwrap_or(root);
            let initial = parent.inspect(name).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    WalkError::NoEntry
                } else {
                    WalkError::Failure(Error::Unreadable)
                }
            })?;
            if initial.kind == FileType::Symlink {
                return Err(Error::Symlink.into());
            }
            let directory = index + 1 != anchor.remainder.len();
            let required = if directory {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            if initial.kind != required {
                return Err(Error::NotRegularFile.into());
            }
            let opened_identity;
            if directory {
                let opened = parent
                    .directory(name)
                    .map_err(|_| match parent.inspect(name) {
                        Ok(current) if identity(&current) == identity(&initial) => {
                            Error::Unreadable
                        }
                        _ => Error::Changed,
                    })?;
                let physical = opened.identity().map_err(|_| Error::Unreadable)?;
                opened_identity = (
                    i128::from(physical.volume),
                    i128::from_le_bytes(physical.file),
                    FileType::Directory,
                );
                canonical.push(opened.name().map_err(|_| Error::Unreadable)?);
                directories.push(opened);
            } else {
                let opened = parent
                    .read_file(name)
                    .map_err(|_| match parent.inspect(name) {
                        Ok(current) if identity(&current) == identity(&initial) => {
                            Error::Unreadable
                        }
                        _ => Error::Changed,
                    })?;
                opened_identity =
                    identity(&crate::windows_file::status(&opened).map_err(|_| Error::Unreadable)?);
                canonical
                    .push(crate::windows_file::file_name(&opened).map_err(|_| Error::Unreadable)?);
                file = Some((opened, opened_identity));
            }
            if opened_identity != identity(&initial) {
                return Err(Error::Changed.into());
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
                .map_err(|_| Error::Changed)?;
            if identity(&current) != *expected {
                return Err(Error::Changed.into());
            }
        }
        let (file, identity) = file.ok_or(Error::NotRegularFile)?;
        Ok(Opened {
            file,
            identity,
            key: canonical,
        })
    }
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

pub(crate) fn resource_failure(error: &Error) -> Option<yamaa_core::resource::ResourceFailure> {
    use yamaa_core::resource::ResourceFailure;
    match error {
        Error::Host(crate::host_resources::Failure::Missing) => {
            Some(yamaa_core::resource::ResourceFailure::Missing)
        }
        Error::Host(crate::host_resources::Failure::Changed) => {
            Some(yamaa_core::resource::ResourceFailure::ContentChanged)
        }
        Error::Host(_) => None,
        Error::Missing => Some(ResourceFailure::Missing),
        Error::NotRegularFile => Some(ResourceFailure::NotRegularFile),
        Error::Symlink => Some(ResourceFailure::Symlink),
        Error::WrittenForm(failure) => Some(*failure),
        Error::Changed => Some(ResourceFailure::ContentChanged),
        Error::Limit(bound) => Some(ResourceFailure::Boundary(*bound)),
        _ => None,
    }
}

#[cfg(test)]
mod classification_tests {
    use super::*;
    use yamaa_core::resource::ResourceFailure;

    #[test]
    fn explicit_physical_facts_are_classified_and_other_failures_stay_opaque() {
        let boundary = Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err();
        for (error, expected) in [
            (Error::Missing, ResourceFailure::Missing),
            (Error::NotRegularFile, ResourceFailure::NotRegularFile),
            (Error::Symlink, ResourceFailure::Symlink),
            (
                Error::WrittenForm(ResourceFailure::Backslash),
                ResourceFailure::Backslash,
            ),
            (Error::Changed, ResourceFailure::ContentChanged),
            (Error::Limit(boundary), ResourceFailure::Boundary(boundary)),
        ] {
            assert_eq!(resource_failure(&error), Some(expected));
        }
        for error in [
            Error::Unreadable,
            Error::InvalidRoot,
            Error::InvalidBase,
            Error::InvalidPath,
            Error::InvalidLock,
            Error::OutsideRoots,
        ] {
            assert_eq!(resource_failure(&error), None);
        }
    }
}

impl yamaa_core::specification_run::SpecificationSourcePort for Resources {
    type Error = Error;
    fn owned_table(
        &self,
        source: &yamaa_core::specification::SourceDeclaration,
    ) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        self.host
            .as_ref()
            .and_then(|host| host.frame(&host.base, &source.path))
            .map(|table| table as Arc<dyn std::any::Any + Send + Sync>)
    }
    fn resource_failure(&self, error: &Error) -> Option<yamaa_core::resource::ResourceFailure> {
        resource_failure(error)
    }
    fn inspect(
        &mut self,
        source: &yamaa_core::specification::SourceDeclaration,
    ) -> Result<(), Error> {
        Resources::inspect(self, &source.path)
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(
        &mut self,
        source: &yamaa_core::specification::SourceDeclaration,
        origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
    ) -> Result<Arc<[u8]>, Error> {
        *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
        Resources::capture(self, &source.path).map(|(bytes, _)| bytes)
    }
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
pub(crate) fn caller_path(path: &Path) -> Result<String, Error> {
    #[cfg(unix)]
    let text = path.to_str().ok_or(Error::InvalidPath)?.to_owned();
    #[cfg(windows)]
    let text = path_text(&directory_segments(path).ok_or(Error::InvalidPath)?);
    Ok(text)
}
fn rooted_segments(path: &str) -> Option<Segments> {
    #[cfg(windows)]
    if let Some(unc) = path.strip_prefix("//") {
        let mut parts = unc.split('/');
        let server = parts.next()?;
        let share = parts.next()?;
        if [server, share]
            .iter()
            .any(|s| s.is_empty() || matches!(*s, "." | ".." | "?"))
        {
            return None;
        }
        let mut result = vec![format!("//{server}/{share}")];
        result.extend(parts.map(str::to_owned));
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
fn read_bounded(
    file: &mut File,
    maximum: usize,
    refusal: impl Fn(u64) -> Error,
) -> Result<Vec<u8>, Error> {
    let length = file.metadata().map_err(|_| Error::Unreadable)?.len();
    if length > maximum as u64 {
        return Err(refusal(length));
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
            .map_err(|_| Error::Unreadable)?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > maximum.saturating_sub(bytes.len()) {
            return Err(refusal(bytes.len() as u64 + count as u64));
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
fn file_base(identity: &str) -> Result<Segments, Error> {
    let mut segments = rooted_segments(identity).ok_or(Error::InvalidPath)?;
    if segments.len() <= 1
        || segments[1..]
            .iter()
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(Error::InvalidPath);
    }
    segments.pop();
    Ok(segments)
}

#[cfg(test)]
mod identity_cache_tests {
    use super::*;
    #[test]
    fn reused_numeric_identity_without_a_current_physical_witness_is_a_fresh_capture() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let dir = Directory(
            std::env::temp_dir().join(format!("yamaa-stale-inode-{}", std::process::id())),
        );
        std::fs::create_dir(&dir.0).unwrap();
        std::fs::write(dir.0.join("a"), b"same bytes").unwrap();
        std::fs::write(dir.0.join("c"), b"same bytes").unwrap();
        let root = dir.0.to_str().unwrap();
        let mut resources = Resources::new(root, root, &[]).unwrap();
        resources.capture("a").unwrap();
        let opened = resources.open("c").unwrap();
        // Deterministically model a reused device/inode cache key. Equality of
        // bytes at a surviving old spelling cannot establish this new identity.
        resources
            .by_identity
            .insert((opened.identity.0, opened.identity.1), 0);
        assert!(resources.capture("c").unwrap().1);
        assert_eq!(resources.capture_reads(), 2);
        assert!(!resources.capture("a").unwrap().1);
    }
}

#[cfg(test)]
mod physical_capture_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-physical-capture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(crate::canonical_path::canonicalize(path).unwrap())
        }
        fn resources(&self) -> Resources {
            let root = self.0.to_str().unwrap();
            Resources::new(root, root, &[]).unwrap()
        }
        fn identity(&self, name: &str) -> String {
            let path = self.0.join(name).to_str().unwrap().to_owned();
            #[cfg(windows)]
            let path = path.replace('\\', "/");
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn metadata_boundary_is_per_file_and_the_held_evidence_keeps_its_bytes() {
        let fixture = Fixture::new();
        let file = File::create(fixture.0.join("first.yaml")).unwrap();
        file.set_len(Limits::SPEC_BYTES as u64).unwrap();
        let file = File::create(fixture.0.join("second.yaml")).unwrap();
        file.set_len(Limits::SPEC_BYTES as u64).unwrap();
        let mut resources = fixture.resources();
        let (first, new) = resources.capture_metadata("first.yaml").unwrap();
        assert!(new);
        let (second, new) = resources.capture_metadata("second.yaml").unwrap();
        assert!(new);
        assert_eq!(first.len(), Limits::SPEC_BYTES);
        assert_eq!(second.len(), Limits::SPEC_BYTES);
        let retained = resources.retain_sources();
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0].0, fixture.identity("first.yaml"));
        assert_eq!(retained[1].0, fixture.identity("second.yaml"));
        assert!(Arc::ptr_eq(&retained[0].1, &first));
        assert!(Arc::ptr_eq(&retained[1].1, &second));
        std::fs::OpenOptions::new()
            .write(true)
            .open(fixture.0.join("first.yaml"))
            .unwrap()
            .set_len((Limits::SPEC_BYTES + 1) as u64)
            .unwrap();
        assert_eq!(
            resources.capture_metadata("first.yaml"),
            Err(Error::Changed)
        );
        std::fs::remove_file(fixture.0.join("first.yaml")).unwrap();
        std::fs::remove_file(fixture.0.join("second.yaml")).unwrap();
        assert!(retained
            .iter()
            .all(|(_, bytes)| bytes.iter().all(|b| *b == 0)));
        assert_eq!(resources.capture_reads(), 2);
    }

    #[test]
    fn source_and_metadata_boundaries_refuse_oversized_files_before_snapshot_creation() {
        let fixture = Fixture::new();
        let file = File::create(fixture.0.join("large.yaml")).unwrap();
        file.set_len((Limits::SPEC_BYTES + 1) as u64).unwrap();
        let mut resources = fixture.resources();
        assert_eq!(
            resources.capture_metadata("large.yaml"),
            Err(Error::Limit(
                Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err()
            ))
        );
        assert_eq!(resources.capture_reads(), 0);
        assert_eq!(resources.snapshot_count(), 0);
        let (source, new) = resources.capture("large.yaml").unwrap();
        assert!(new);
        assert_eq!(source.len(), Limits::SPEC_BYTES + 1);
        assert_eq!(
            resources.capture_metadata("large.yaml"),
            Err(Error::Limit(
                Limits::spec_bytes(Limits::SPEC_BYTES + 1).unwrap_err()
            ))
        );
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(resources.snapshot_count(), 1);
        #[cfg(unix)]
        {
            // Unix truncation creates a sparse extent; refuse its logical size
            // before reading or allocating the ten-gigabyte source contents.
            file.set_len((Limits::SOURCE_BYTES + 1) as u64).unwrap();
            assert_eq!(resources.capture("large.yaml"), Err(Error::Changed));
            let mut uncaptured = fixture.resources();
            assert_eq!(
                uncaptured.capture("large.yaml"),
                Err(Error::Limit(
                    Limits::source_bytes(Limits::SOURCE_BYTES + 1).unwrap_err()
                ))
            );
            assert_eq!(uncaptured.capture_reads(), 0);
            assert_eq!(uncaptured.snapshot_count(), 0);
        }
    }

    #[test]
    fn repeated_alias_verification_keeps_original_bytes_and_checks_every_witness() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("source.yaml"), b"held original bytes").unwrap();
        let mut resources = fixture.resources();
        let (before, new) = resources.capture_metadata("source.yaml").unwrap();
        assert!(new);
        let (alias, new) = resources.capture_metadata("./source.yaml").unwrap();
        assert!(!new);
        assert!(Arc::ptr_eq(&before, &alias));
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
        std::fs::hard_link(fixture.0.join("source.yaml"), fixture.0.join("other.yaml")).unwrap();
        assert!(!resources.capture_metadata("other.yaml").unwrap().1);
        std::fs::remove_file(fixture.0.join("other.yaml")).unwrap();
        std::fs::write(fixture.0.join("other.yaml"), b"changed alias").unwrap();
        assert_eq!(
            resources.verify_captured().unwrap_err(),
            (fixture.identity("other.yaml"), Error::Changed)
        );
        std::fs::write(fixture.0.join("source.yaml"), b"changed bytes").unwrap();
        assert_eq!(
            resources.verify_captured().unwrap_err(),
            (fixture.identity("source.yaml"), Error::Changed)
        );
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
    }
}

#[cfg(test)]
mod declaring_publication_tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-layer-publication-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            for name in ["entry/out", "shared/out"] {
                fs::create_dir_all(path.join(name)).unwrap();
            }
            fs::write(path.join("entry/root.yaml"), b"entry").unwrap();
            fs::write(path.join("shared/base.yaml"), b"parent").unwrap();
            Self(crate::canonical_path::canonicalize(path).unwrap())
        }
        fn text(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().replace('\\', "/")
        }
        fn resources(&self) -> Resources {
            Resources::for_entry(&self.text("entry")).unwrap()
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn identical_output_spellings_publish_at_each_original_layer_without_metadata_rereads() {
        let study = Study::new();
        let resources = study.resources();
        let entry = resources.resolve("root.yaml").unwrap();
        let parent = resources
            .resolve_from(&entry, "../shared/base.yaml")
            .unwrap();
        let entry_target = resources
            .publication_target_from(&entry, "out/held.csv")
            .unwrap();
        let parent_target = resources
            .publication_target_from(&parent, "out/held.csv")
            .unwrap();
        fs::remove_file(study.0.join("entry/root.yaml")).unwrap();
        fs::remove_file(study.0.join("shared/base.yaml")).unwrap();
        assert!(!study.0.join("entry/out/held.csv").exists());
        assert!(!study.0.join("shared/out/held.csv").exists());
        parent_target
            .publisher("out/held.csv")
            .unwrap()
            .publish("out/held.csv", b"ID\n7\n")
            .unwrap();
        entry_target
            .publisher("out/held.csv")
            .unwrap()
            .publish("out/held.csv", b"ID\n8\n")
            .unwrap();
        assert_eq!(
            fs::read(study.0.join("shared/out/held.csv")).unwrap(),
            b"ID\n7\n"
        );
        assert_eq!(
            fs::read(study.0.join("entry/out/held.csv")).unwrap(),
            b"ID\n8\n"
        );
        assert_eq!(resources.capture_reads(), 0);
    }

    #[test]
    fn outside_layer_publication_refuses_linked_parent_substitution() {
        let study = Study::new();
        let resources = study.resources();
        let parent = resources.resolve("../shared/base.yaml").unwrap();
        let target = resources
            .publication_target_from(&parent, "out/held.csv")
            .unwrap();
        fs::rename(study.0.join("shared/out"), study.0.join("shared/moved")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../entry/out", study.0.join("shared/out")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir("../entry/out", study.0.join("shared/out")).unwrap();
        assert!(matches!(
            target.publisher("out/held.csv"),
            Err(crate::file_publication::Error::InvalidTarget)
        ));
        assert!(!study.0.join("entry/out/held.csv").exists());
        assert!(!study.0.join("shared/moved/held.csv").exists());
        assert_eq!(resources.capture_reads(), 0);
    }
}
