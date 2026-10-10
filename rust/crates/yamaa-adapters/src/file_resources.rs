//! Descriptor-anchored bytes under caller-selected approved roots.
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
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
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
    InvalidLock,
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
            Self::InvalidLock => "invalid packaging lock syntax",
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

/// A native metadata traversal shares these ceilings across captures, owned
/// preparation copies, aliases and rereads. Ordinary resource use is unchanged.
#[derive(Clone, Copy)]
pub(crate) struct MetadataLimits {
    pub bytes: usize,
    pub snapshots: usize,
    pub text: usize,
    pub work: usize,
}
struct MetadataBudget {
    limits: MetadataLimits,
    bytes: AtomicUsize,
    text: AtomicUsize,
    work: AtomicUsize,
}
fn reserve(counter: &AtomicUsize, amount: usize, maximum: usize) -> Result<(), Error> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
            used.checked_add(amount).filter(|&n| n <= maximum)
        })
        .map(|_| ())
        .map_err(|_| Error::Limit)
}
fn changed(error: Error) -> Error {
    if error == Error::Limit {
        error
    } else {
        Error::Changed
    }
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
    metadata: Option<MetadataBudget>,
}

pub type RetainedSources = Vec<(String, Arc<[u8]>)>;
#[derive(Debug)]
pub struct RetentionFailure {
    pub error: Error,
    pub retained: RetainedSources,
}
impl Resources {
    /// Only native entry configuration may add roots, before entry/model capture.
    /// Keep the selected primary descriptor and captured configuration immutable.
    pub(crate) fn with_configuration_roots(mut self, roots: &[String]) -> Result<Self, Error> {
        if self.roots.len() != 1
            || self.roots.len().saturating_add(roots.len()) > MAX_ROOTS
            || self
                .by_path
                .keys()
                .any(|path| path.last().map(String::as_str) != Some("yamaa-project.yaml"))
        {
            return Err(Error::InvalidRoot);
        }
        for written in roots {
            self.roots.push(selected_root(written)?);
        }
        Ok(self)
    }
    /// The first root is the project root; later roots retain fallback order.
    pub fn new(root: &str, base: &str, data_roots: &[String]) -> Result<Self, Error> {
        if data_roots.len() >= MAX_ROOTS {
            return Err(Error::Limit);
        }
        let mut roots = Vec::new();
        for written in std::iter::once(root).chain(data_roots.iter().map(String::as_str)) {
            roots.push(selected_root(written)?);
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
            metadata: None,
        })
    }
    pub fn capture_reads(&self) -> usize {
        self.reads
    }
    pub(crate) fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }
    /// End a traversal's cumulative scope without discarding descriptors, raw
    /// snapshots or alias witnesses. A later build admits its own fresh scope.
    pub(crate) fn finish_metadata_budget(&mut self) {
        self.metadata = None;
    }
    pub(crate) fn admit_metadata_budget(&mut self, limits: MetadataLimits) -> Result<(), Error> {
        if self.metadata.is_some()
            || self.captured_bytes > limits.bytes
            || self.snapshots.len() > limits.snapshots
            || self.aliases > limits.snapshots
        {
            return Err(Error::Limit);
        }
        let mut text = 0usize;
        for snapshot in &self.snapshots {
            for (key, written, base) in &snapshot.paths {
                for length in key
                    .iter()
                    .chain(base)
                    .map(String::len)
                    .chain([written.len()])
                {
                    text = text
                        .checked_add(length)
                        .filter(|&n| n <= limits.text)
                        .ok_or(Error::Limit)?;
                }
            }
        }
        for key in self.by_path.keys() {
            for part in key {
                text = text
                    .checked_add(part.len())
                    .filter(|&n| n <= limits.text)
                    .ok_or(Error::Limit)?;
            }
        }
        self.metadata = Some(MetadataBudget {
            limits,
            bytes: AtomicUsize::new(self.captured_bytes),
            text: AtomicUsize::new(text),
            work: AtomicUsize::new(0),
        });
        self.charge_work(text)
    }
    pub(crate) fn metadata_usage(&self) -> (usize, usize, usize) {
        self.metadata.as_ref().map_or((0, 0, 0), |budget| {
            (
                budget.bytes.load(Ordering::Relaxed),
                budget.text.load(Ordering::Relaxed),
                budget.work.load(Ordering::Relaxed),
            )
        })
    }
    pub(crate) fn charge_work(&self, amount: usize) -> Result<(), Error> {
        if let Some(budget) = &self.metadata {
            reserve(&budget.work, amount, budget.limits.work)?;
        }
        Ok(())
    }
    pub(crate) fn charge_metadata_text(&self, amount: usize) -> Result<(), Error> {
        if let Some(budget) = &self.metadata {
            reserve(&budget.text, amount, budget.limits.text)?;
        }
        self.charge_work(amount)
    }
    fn available_metadata_bytes(&self) -> usize {
        self.metadata.as_ref().map_or(usize::MAX, |budget| {
            budget
                .limits
                .bytes
                .saturating_sub(budget.bytes.load(Ordering::Relaxed))
        })
    }
    fn charge_metadata_bytes(&self, amount: usize) -> Result<(), Error> {
        if let Some(budget) = &self.metadata {
            reserve(&budget.bytes, amount, budget.limits.bytes)?;
        }
        Ok(())
    }
    pub(crate) fn preparation_copy(&self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        self.charge_metadata_bytes(bytes.len())?;
        self.charge_work(bytes.len())?;
        Ok(bytes.to_vec())
    }
    /// Immutable raw prefix evidence survives failed decoding and inheritance.
    /// This view grants no resource, execution or publication port.
    pub(crate) fn captured_sources(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.snapshots.iter().flat_map(|snapshot| {
            snapshot
                .paths
                .iter()
                .map(|(key, _, _)| (path_text(key), snapshot.bytes.as_ref()))
        })
    }
    pub(crate) fn verify_captured(&self) -> Result<(), (String, Error)> {
        self.verify_prefix(self.snapshots.len())
    }
    pub(crate) fn verify_prefix(&self, count: usize) -> Result<(), (String, Error)> {
        if count > self.snapshots.len() {
            return Err((String::new(), Error::InvalidPath));
        }
        for index in 0..count {
            self.verify_paths(index)?;
        }
        Ok(())
    }
    /// Own immutable raw evidence without copying content. Charge each path
    /// before allocating its spelling; quota failure keeps the admitted prefix.
    pub(crate) fn retain_sources(
        &self,
        aliases: usize,
        text: usize,
    ) -> Result<RetainedSources, RetentionFailure> {
        let mut retained = Vec::new();
        let mut used = 0usize;
        for snapshot in &self.snapshots {
            for (key, _, _) in &snapshot.paths {
                let amount = key
                    .iter()
                    .try_fold(key.len().saturating_add(4), |n, s| n.checked_add(s.len()));
                let next = amount.and_then(|n| used.checked_add(n));
                if retained.len() >= aliases || next.is_none_or(|n| n > text) {
                    return Err(RetentionFailure {
                        error: Error::Limit,
                        retained,
                    });
                }
                used = next.expect("retained path budget admitted");
                retained.push((path_text(key), Arc::clone(&snapshot.bytes)));
            }
        }
        Ok(retained)
    }
    fn charge_alias(
        &self,
        key: &[String],
        written: &str,
        base: &[String],
        new_path: bool,
    ) -> Result<(), Error> {
        let mut amount = written.len();
        for part in key
            .iter()
            .chain(base)
            .chain(if new_path { key } else { &[] })
        {
            amount = amount.checked_add(part.len()).ok_or(Error::Limit)?;
        }
        self.charge_metadata_text(amount)
    }
    fn read_bounded(
        &self,
        file: &mut File,
        maximum: usize,
        verifying: bool,
    ) -> Result<Vec<u8>, Error> {
        let observed = |error| if verifying { Error::Changed } else { error };
        let Some(budget) = &self.metadata else {
            return read_bounded(file, maximum).map_err(observed);
        };
        if file
            .metadata()
            .map_err(|_| observed(Error::Unreadable))?
            .len()
            > maximum as u64
        {
            return Err(observed(Error::Limit));
        }
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 65_536];
        loop {
            let available = budget
                .limits
                .work
                .saturating_sub(budget.work.load(Ordering::Relaxed));
            if available <= 1 {
                return Err(Error::Limit);
            }
            let count_limit = maximum
                .saturating_sub(bytes.len())
                .saturating_add(1)
                .min(buffer.len())
                .min(available - 1);
            // Reserve before the read. Unused reservation is returned after IO;
            // every read operation and every byte, including EOF probes, count.
            self.charge_work(count_limit + 1)?;
            let count = file
                .read(&mut buffer[..count_limit])
                .map_err(|_| observed(Error::Unreadable))?;
            budget
                .work
                .fetch_sub(count_limit - count, Ordering::Relaxed);
            if count == 0 {
                return Ok(bytes);
            }
            if count > maximum.saturating_sub(bytes.len()) {
                return Err(observed(Error::Limit));
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
    }
    /// Metadata only: no bytes read and no snapshot created.
    pub fn inspect(&self, written: &str) -> Result<(), Error> {
        self.open(written).map(drop)
    }
    /// Inspect relative to a canonical declaring entry without changing another
    /// node's resource base, reading bytes or creating a snapshot.
    pub fn inspect_from(&self, declaring: &str, written: &str) -> Result<(), Error> {
        self.open_at(&file_base(declaring)?, written).map(drop)
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
    /// A lexical declaration location under the approved first-anchor policy.
    /// The target may be absent. This fact opens no file and grants no byte or
    /// publication authority; declaring identities come from this reader.
    pub fn location_from(&self, declaring: &str, written: &str) -> Result<String, Error> {
        let anchor = self.declaration_anchor(&file_base(declaring)?, written)?;
        Ok(path_text(&anchor.key))
    }
    fn declaration_anchor(&self, base: &[String], written: &str) -> Result<Anchor, Error> {
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
    /// Publication names the declaration's first anchor, including absent targets.
    /// Retain its selected physical root; parent traversal is checked at save.
    pub(crate) fn publication_target(
        &self,
        written: &str,
    ) -> Result<crate::file_publication::AnchoredTarget, Error> {
        let anchor = self.declaration_anchor(&self.base, written)?;
        let descriptor = self.roots[anchor.root]
            .descriptor
            .try_clone()
            .map_err(|_| Error::Unreadable)?;
        Ok(crate::file_publication::AnchoredTarget::new(
            descriptor,
            anchor.remainder,
            path_text(&anchor.key),
        ))
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
        // This is a canonical identity returned by the reader, not an authored
        // path. Selected physical root names can contain a literal backslash on
        // Unix. Keep canonical containment and descriptor/link checks intact.
        let mut segments = rooted_segments(identity).ok_or(Error::InvalidPath)?;
        let marker = segments.remove(0);
        let anchor = self.relative_anchor(&[marker], &segments.join("/"))?;
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
                && self.physical_witness(index, physical)?
                && match self.verify(index) {
                    Err(Error::Limit) => return Err(Error::Limit),
                    result => result.is_ok(),
                }
            {
                Some(index)
            } else {
                self.by_identity.remove(&physical);
                None
            }
        } else {
            None
        };
        let snapshots_limit = self
            .metadata
            .as_ref()
            .map_or(MAX_SNAPSHOTS, |b| b.limits.snapshots.min(MAX_SNAPSHOTS));
        if !self.by_path.contains_key(&opened.key) && self.by_path.len() >= snapshots_limit {
            return Err(Error::Limit);
        }
        if let Some(index) = accepted {
            self.charge_work(
                self.snapshots[index]
                    .paths
                    .iter()
                    .try_fold(0usize, |n, (key, path, from)| {
                        key.iter()
                            .chain(from)
                            .map(String::len)
                            .chain([path.len()])
                            .try_fold(n, usize::checked_add)
                    })
                    .ok_or(Error::Limit)?,
            )?;
            let known_alias = self.snapshots[index]
                .paths
                .iter()
                .any(|(key, path, from)| key == &opened.key && path == written && from == &base);
            if !known_alias && self.aliases >= snapshots_limit {
                return Err(Error::Limit);
            }
            if self.snapshots[index].bytes.len() > maximum {
                return Err(Error::Limit);
            }
            let bytes = Arc::clone(&self.snapshots[index].bytes);
            self.charge_work(bytes.len())?;
            if !known_alias {
                self.charge_alias(
                    &opened.key,
                    written,
                    &base,
                    !self.by_path.contains_key(&opened.key),
                )?;
            }
            if self
                .read_bounded(&mut opened.file, bytes.len(), true)
                .map_err(changed)?
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
        if self.snapshots.len() >= snapshots_limit || self.aliases >= snapshots_limit {
            return Err(Error::Limit);
        }
        let available = MAX_CAPTURED_BYTES
            .checked_sub(self.captured_bytes)
            .ok_or(Error::Limit)?;
        self.charge_alias(&opened.key, written, &base, true)?;
        let content = self.read_bounded(
            &mut opened.file,
            maximum.min(available).min(self.available_metadata_bytes()),
            false,
        )?;
        self.charge_work(content.len())?;
        let mut current = self.open_at(&base, written).map_err(changed)?;
        if current.key != opened.key
            || self
                .read_bounded(&mut current.file, content.len(), true)
                .map_err(changed)?
                .as_slice()
                != content.as_slice()
        {
            return Err(Error::Changed);
        }
        self.charge_metadata_bytes(content.len())?;
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
        self.verify_paths(index).map_err(|(_, error)| error)
    }
    fn verify_paths(&self, index: usize) -> Result<(), (String, Error)> {
        let snapshot = &self.snapshots[index];
        for (key, written, base) in &snapshot.paths {
            let result = (|| {
                self.charge_work(snapshot.bytes.len())?;
                let mut opened = self.open_at(base, written).map_err(changed)?;
                if &opened.key != key
                    || self
                        .read_bounded(&mut opened.file, snapshot.bytes.len(), true)?
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
    fn physical_witness(&self, index: usize, physical: (i128, i128)) -> Result<bool, Error> {
        for (key, written, base) in &self.snapshots[index].paths {
            match self.open_at(base, written) {
                Ok(opened)
                    if opened.key == *key && (opened.identity.0, opened.identity.1) == physical =>
                {
                    return Ok(true)
                }
                Err(Error::Limit) => return Err(Error::Limit),
                _ => {}
            }
        }
        Ok(false)
    }
    fn anchors(&self, base: &[String], written: &str) -> Result<Vec<Anchor>, Error> {
        bounded(written)?;
        let size = base
            .iter()
            .try_fold(written.len(), |n, part| n.checked_add(part.len()))
            .ok_or(Error::Limit)?;
        self.charge_work(size.checked_mul(self.roots.len() + 1).ok_or(Error::Limit)?)?;
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
    #[cfg(unix)]
    fn open_anchor(&self, anchor: Anchor) -> Result<Opened, WalkError> {
        self.charge_work(anchor.remainder.len().checked_mul(4).ok_or(Error::Limit)?)?;
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
    #[cfg(windows)]
    fn open_anchor(&self, anchor: Anchor) -> Result<Opened, WalkError> {
        self.charge_work(anchor.remainder.len().checked_mul(4).ok_or(Error::Limit)?)?;
        let root = &self.roots[anchor.root].descriptor;
        let mut directories = Vec::new();
        let mut witnesses = Vec::new();
        let mut canonical = self.roots[anchor.root].canonical.clone();
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
    Ok(Root {
        descriptor,
        canonical,
        spellings,
    })
}

#[cfg(windows)]
fn selected_root(written: &str) -> Result<Root, Error> {
    bounded(written)?;
    let path = std::fs::canonicalize(written).map_err(|_| Error::InvalidRoot)?;
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
        let text = normalized.to_str()?.replace('\\', "/");
        let text = text.strip_prefix("//?/").unwrap_or(&text);
        if let Some(unc) = text.strip_prefix("UNC/") {
            rooted_segments(&format!("//{unc}"))
        } else {
            rooted_segments(text)
        }
    }
}
/// Convert an absolute caller-owned native path to the resource spelling. YAML
/// paths still enter `anchors` unchanged and keep their strict separator rules.
pub(crate) fn caller_path(path: &Path) -> Result<String, Error> {
    #[cfg(unix)]
    let text = path.to_str().ok_or(Error::InvalidPath)?.to_owned();
    #[cfg(windows)]
    let text = path_text(&directory_segments(path).ok_or(Error::InvalidPath)?);
    bounded(&text)?;
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

#[cfg(test)]
mod metadata_budget_tests {
    use super::*;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-metadata-budget-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
        fn resources(&self, snapshots: usize) -> Resources {
            let root = self.0.to_str().unwrap();
            let mut resources = Resources::new(root, root, &[]).unwrap();
            resources
                .admit_metadata_budget(MetadataLimits {
                    bytes: 65_536,
                    snapshots,
                    text: 65_536,
                    work: 1_048_576,
                })
                .unwrap();
            resources
        }
        fn identity(&self, name: &str) -> String {
            let path = self.0.join(name).to_str().unwrap().to_owned();
            #[cfg(windows)]
            let path = path
                .strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('\\', "/");
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn repeated_alias_verification_spends_work_without_claiming_a_new_capture() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("source.yaml"), b"held original bytes").unwrap();
        let mut resources = fixture.resources(4);
        let (before, new) = resources.capture("source.yaml", 1024).unwrap();
        assert!(new);
        let (bytes, text, work) = resources.metadata_usage();
        let (alias, new) = resources.capture("./source.yaml", 1024).unwrap();
        assert!(!new);
        assert!(Arc::ptr_eq(&before, &alias));
        let (after_bytes, after_text, after_work) = resources.metadata_usage();
        assert_eq!(after_bytes, bytes);
        assert!(after_text > text);
        assert!(after_work > work + before.len() * 2);
        assert_eq!(resources.capture_reads(), 1);
        resources.metadata.as_mut().unwrap().limits.work = after_work;
        assert_eq!(
            resources.capture("./source.yaml", 1024).unwrap_err(),
            Error::Limit
        );
        assert_eq!(resources.capture_reads(), 1);
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
        resources.metadata.as_mut().unwrap().limits.work = 1_048_576;
        std::fs::hard_link(fixture.0.join("source.yaml"), fixture.0.join("other.yaml")).unwrap();
        assert!(!resources.capture("other.yaml", 1024).unwrap().1);
        std::fs::remove_file(fixture.0.join("other.yaml")).unwrap();
        std::fs::write(fixture.0.join("other.yaml"), b"changed alias").unwrap();
        assert_eq!(
            resources.verify_captured().unwrap_err(),
            (fixture.identity("other.yaml"), Error::Changed,)
        );
        std::fs::write(fixture.0.join("source.yaml"), b"changed bytes").unwrap();
        assert_eq!(
            resources.verify_captured().unwrap_err(),
            (fixture.identity("source.yaml"), Error::Changed,)
        );
        assert_eq!(
            resources.captured_sources().next().unwrap().1,
            before.as_ref()
        );
    }

    #[test]
    fn alias_and_copy_ownership_are_bounded_before_new_authority() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("source.yaml"), b"abc").unwrap();
        let mut resources = fixture.resources(1);
        let (bytes, _) = resources.capture("source.yaml", 3).unwrap();
        assert_eq!(
            resources.capture("./source.yaml", 3).unwrap_err(),
            Error::Limit
        );
        assert_eq!(resources.aliases, 1);
        assert_eq!(resources.by_path.len(), 1);
        resources.metadata.as_mut().unwrap().limits.bytes = 5;
        assert_eq!(
            resources.preparation_copy(&bytes).unwrap_err(),
            Error::Limit
        );
        assert_eq!(resources.metadata_usage().0, 3);
        assert_eq!(resources.captured_sources().next().unwrap().1, b"abc");
    }
}
