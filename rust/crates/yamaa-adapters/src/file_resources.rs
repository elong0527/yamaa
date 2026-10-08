//! Descriptor-anchored bytes under caller-selected approved roots.
//! Captured equality compares retained bytes. This module interprets no study data.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    os::fd::OwnedFd,
    path::Path,
    sync::Arc,
};

const MAX_PATH_BYTES: usize = 65_536;
const MAX_ROOTS: usize = 64;
const MAX_SNAPSHOTS: usize = 1024;
const MAX_CAPTURED_BYTES: usize = 67_108_864;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Missing,
    Unreadable,
    NotRegularFile,
    InvalidRoot,
    InvalidBase,
    InvalidPath,
    OutsideRoots,
    Symlink,
    Changed,
    Limit,
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Missing => "resource path missing",
            Self::Unreadable => "resource path is not readable",
            Self::NotRegularFile => "resource path is not a regular file",
            Self::InvalidRoot => "invalid approved resource root",
            Self::InvalidBase => "invalid resource base directory",
            Self::InvalidPath => "invalid resource path spelling",
            Self::OutsideRoots => "resource path outside approved roots",
            Self::Symlink => "resource path contains a symbolic link",
            Self::Changed => "captured resource content changed",
            Self::Limit => "resource capture limit",
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
    root: usize,
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
    roots: Vec<Root>,
    base: Segments,
    snapshots: Vec<Snapshot>,
    by_path: BTreeMap<Segments, usize>,
    by_identity: BTreeMap<(i128, i128), usize>,
    captured_bytes: usize,
    aliases: usize,
    reads: usize,
}
impl Resources {
    /// The first root is the project root; later roots retain fallback order.
    pub fn new(root: &str, base: &str, data_roots: &[String]) -> Result<Self, Error> {
        if data_roots.len() >= MAX_ROOTS {
            return Err(Error::Limit);
        }
        let mut roots = Vec::new();
        for written in std::iter::once(root).chain(data_roots.iter().map(String::as_str)) {
            bounded(written)?;
            let path = std::fs::canonicalize(written).map_err(|_| Error::InvalidRoot)?;
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
            roots.push(Root {
                descriptor,
                canonical,
                spellings,
            });
        }
        bounded(base)?;
        let path = std::fs::canonicalize(base).map_err(|_| Error::InvalidBase)?;
        if !path.is_dir() {
            return Err(Error::InvalidBase);
        }
        let base = directory_segments(&path).ok_or(Error::InvalidBase)?;
        Ok(Self {
            roots,
            base,
            snapshots: Vec::new(),
            by_path: BTreeMap::new(),
            by_identity: BTreeMap::new(),
            captured_bytes: 0,
            aliases: 0,
            reads: 0,
        })
    }
    pub fn capture_reads(&self) -> usize {
        self.reads
    }
    /// Metadata only: no bytes read and no snapshot created.
    pub fn inspect(&self, written: &str) -> Result<(), Error> {
        self.open(written).map(drop)
    }
    /// Capture bounded immutable content, verify it and retain its authority witnesses.
    pub fn capture(&mut self, written: &str, maximum: usize) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_at(self.base.clone(), written, maximum)
    }
    /// A declaring identity comes from this reader's canonical regular-file resolution.
    pub fn resolve_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        let base = file_base(declaring)?;
        let opened = self.open_at(&base, written)?;
        Ok(path_text(&opened.key))
    }
    pub fn resolve(&self, written: &str) -> Result<String, Error> {
        Ok(path_text(&self.open(written)?.key))
    }
    pub fn capture_from(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        self.capture_at(file_base(declaring)?, written, maximum)
    }
    /// Keep the same selected roots and captures for data declared by this entry.
    pub fn select_entry_base(&mut self, identity: &str) -> Result<(), Error> {
        let base = file_base(identity)?;
        self.open(identity)?;
        self.base = base;
        Ok(())
    }
    fn capture_at(
        &mut self,
        base: Segments,
        written: &str,
        maximum: usize,
    ) -> Result<(Arc<[u8]>, bool), Error> {
        let mut opened = self.open_at(&base, written)?;
        let length = opened.file.metadata().map_err(|_| Error::Unreadable)?.len();
        if !self.by_path.contains_key(&opened.key) && length > maximum as u64 {
            return Err(Error::Limit);
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
        if !self.by_path.contains_key(&opened.key) && self.by_path.len() >= MAX_SNAPSHOTS {
            return Err(Error::Limit);
        }
        if let Some(index) = accepted {
            let known_alias = self.snapshots[index]
                .paths
                .iter()
                .any(|(key, path, from)| key == &opened.key && path == written && from == &base);
            if !known_alias && self.aliases >= MAX_SNAPSHOTS {
                return Err(Error::Limit);
            }
            if self.snapshots[index].bytes.len() > maximum {
                return Err(Error::Limit);
            }
            let bytes = Arc::clone(&self.snapshots[index].bytes);
            if read_bounded(&mut opened.file, bytes.len())
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
                self.aliases += 1;
            }
            self.by_path.insert(opened.key, index);
            return Ok((bytes, false));
        }
        if self.snapshots.len() >= MAX_SNAPSHOTS || self.aliases >= MAX_SNAPSHOTS {
            return Err(Error::Limit);
        }
        let available = MAX_CAPTURED_BYTES
            .checked_sub(self.captured_bytes)
            .ok_or(Error::Limit)?;
        let content = read_bounded(&mut opened.file, maximum.min(available))?;
        let mut current = self.open_at(&base, written).map_err(|_| Error::Changed)?;
        if current.key != opened.key
            || read_bounded(&mut current.file, content.len())
                .map_err(|_| Error::Changed)?
                .as_slice()
                != content.as_slice()
        {
            return Err(Error::Changed);
        }
        self.captured_bytes += content.len();
        self.reads += 1;
        self.aliases += 1;
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
        let snapshot = &self.snapshots[index];
        for (key, written, base) in &snapshot.paths {
            let mut opened = self.open_at(base, written).map_err(|_| Error::Changed)?;
            if &opened.key != key
                || read_bounded(&mut opened.file, snapshot.bytes.len())
                    .map_err(|_| Error::Changed)?
                    .as_slice()
                    != snapshot.bytes.as_ref()
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    fn physical_witness(&self, index: usize, physical: (i128, i128)) -> bool {
        self.snapshots[index]
            .paths
            .iter()
            .any(|(key, written, base)| {
                self.open_at(base, written).is_ok_and(|opened| {
                    opened.key == *key && (opened.identity.0, opened.identity.1) == physical
                })
            })
    }
    fn anchors(&self, base: &[String], written: &str) -> Result<Vec<Anchor>, Error> {
        bounded(written)?;
        if written.contains('\\') || written.contains('\0') {
            return Err(Error::InvalidPath);
        }
        let rooted = rooted_segments(written);
        if rooted.is_none()
            && written.split_once(':').is_some_and(|(head, _)| {
                head.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                    && head
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
            })
        {
            return Err(Error::InvalidPath);
        }
        if rooted.as_ref().map_or_else(
            || written.split('/').any(str::is_empty),
            |s| s.len() == 1 || s[1..].iter().any(|s| s.is_empty() || s == "." || s == ".."),
        ) {
            return Err(Error::InvalidPath);
        }
        if let Some(segments) = rooted {
            let mut selected = None;
            for (index, root) in self.roots.iter().enumerate() {
                for spelling in &root.spellings {
                    if segments.starts_with(spelling)
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
        let inside = self.roots.iter().any(|r| base.starts_with(&r.canonical));
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
            if resolved.starts_with(&root.canonical)
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
            root,
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
    fn open_anchor(&self, anchor: Anchor) -> Result<Opened, WalkError> {
        let root = &self.roots[anchor.root].descriptor;
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
}

impl yamaa_engine::specification_run::SourcePort for Resources {
    type Error = Error;
    fn resource_failure(&self, error: &Error) -> Option<yamaa_core::resource::ResourceFailure> {
        use yamaa_core::resource::ResourceFailure;
        match error {
            Error::Missing => Some(ResourceFailure::Missing),
            Error::NotRegularFile => Some(ResourceFailure::NotRegularFile),
            _ => None,
        }
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
        maximum: usize,
    ) -> Result<Arc<[u8]>, Error> {
        Resources::capture(self, &source.path, maximum).map(|(bytes, _)| bytes)
    }
}

fn bounded(text: &str) -> Result<(), Error> {
    if text.len() > MAX_PATH_BYTES {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn kind(status: &fs::Stat) -> FileType {
    FileType::from_raw_mode(status.st_mode)
}
fn identity(status: &fs::Stat) -> Identity {
    (
        i128::from(status.st_dev),
        i128::from(status.st_ino),
        kind(status),
    )
}
fn directory_segments(path: &Path) -> Option<Segments> {
    // Match host directory selection: normalized root aliases retain their authority.
    let normalized: std::path::PathBuf = path.components().collect();
    rooted_segments(normalized.to_str()?)
}
fn rooted_segments(path: &str) -> Option<Segments> {
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
fn read_bounded(file: &mut File, maximum: usize) -> Result<Vec<u8>, Error> {
    if file.metadata().map_err(|_| Error::Unreadable)?.len() > maximum as u64 {
        return Err(Error::Limit);
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
            return Err(Error::Limit);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn path_text(segments: &[String]) -> String {
    format!("{}/{}", segments[0], segments[1..].join("/"))
}
fn file_base(identity: &str) -> Result<Segments, Error> {
    bounded(identity)?;
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
        resources.capture("a", 32).unwrap();
        let opened = resources.open("c").unwrap();
        // Deterministically model a reused device/inode cache key. Equality of
        // bytes at a surviving old spelling cannot establish this new identity.
        resources
            .by_identity
            .insert((opened.identity.0, opened.identity.1), 0);
        assert!(resources.capture("c", 32).unwrap().1);
        assert_eq!(resources.capture_reads(), 2);
        assert!(!resources.capture("a", 32).unwrap().1);
    }
}
