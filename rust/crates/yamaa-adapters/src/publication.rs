//! One save: every file a save writes, across every member of an outcome,
//! opened for writing, staged beside its target, then committed in order.
//!
//! A target is a physical file under the local file resolver, or an object a
//! caller's resolver names. [`stage`] opens every target first, so a target
//! that cannot be written, or two files that name one target, stop the save
//! before anything is staged; it then stages every file completely. A run's
//! output encoded a partition at a time (D264) is staged so too: the file
//! publisher opens its staged file once and appends each partition, and
//! [`stage_with`] is asked between two partitions, where a save polls its
//! cancellation.
//! [`Staged::commit`] replaces each target in order. The first file that
//! cannot replace its target stops the save there: the files before it stay
//! replaced, and every staged file after it is removed. Between staging and
//! the first commit nothing is visible at any target, so a save stopped there
//! writes nothing (D74).
//!
//! This is the engine's one publisher port (#3030 section 2.10): target,
//! stage, commit and abort, implemented by the file publisher (Unix and
//! Windows) and by a caller's resolver (a memory resolver or an embedder's
//! bridge). Every save writes through it, as does a saved table, and a
//! submission composition admits its targets through `AnchoredTarget`
//! before it is held. The files come encoded: a run's result and a
//! composition hold their bytes, and a save never encodes again.
use crate::{
    file_publication as file,
    host_resources::{Failure, Kind, Operation, Request, Resolver},
};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::Arc,
};
pub(crate) enum AnchoredTarget {
    File(file::AnchoredTarget),
    Host {
        resolver: Arc<dyn Resolver>,
        declaring: String,
        written: String,
        reference: String,
    },
}
pub enum Publisher {
    File(file::Publisher),
    Host {
        resolver: Arc<dyn Resolver>,
        declaring: String,
        written: String,
        reference: String,
    },
}
impl AnchoredTarget {
    pub(crate) fn publisher(&self, declared: &str) -> Result<Publisher, file::Error> {
        match self {
            Self::File(target) => target.publisher(declared).map(Publisher::File),
            Self::Host {
                resolver,
                declaring,
                written,
                reference,
            } => {
                let reply = resolver
                    .request(Request {
                        operation: Operation::Target,
                        kind: Kind::Output,
                        declaring,
                        written,
                        reference: Some(reference),
                        bytes: None,
                        limit: None,
                    })
                    .map_err(target_refusal)?;
                if reply.reference != *reference || (reply.bytes.is_some() || reply.frame.is_some())
                {
                    return Err(file::Error::Host(Failure::Changed));
                }
                Ok(Publisher::Host {
                    resolver: Arc::clone(resolver),
                    declaring: declaring.clone(),
                    written: declared.into(),
                    reference: reference.clone(),
                })
            }
        }
    }
    /// The path a save reports for this target: a file's absolute path with
    /// forward slashes, or the reference a caller's resolver gave its object.
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::File(target) => target.path(),
            Self::Host { reference, .. } => reference,
        }
    }
}
/// A caller's resolver's refusal of a target: a kind it reports is the walk's
/// refusal, as the local file resolver's is (FILE-7, FILE-8), and any other
/// failure stays the resolver's own.
fn target_refusal(failure: Failure) -> file::Error {
    failure
        .resource_kind()
        .and_then(|kind| {
            crate::file_resources::resource_failure(&crate::file_resources::Error::from_kind(kind))
        })
        .map_or(file::Error::Host(failure), file::Error::Unreachable)
}
/// A staged file: the file publisher's private temporary file, or the staging
/// token a caller's resolver returned.
enum Staging {
    File(file::Staged),
    Host(String),
}
/// What a staged file left behind when it could not be removed.
enum Leftover {
    File(PathBuf, std::io::Error),
    Host,
}
impl Publisher {
    pub fn same_target(&self, other: &Self) -> Result<bool, file::Error> {
        match (self, other) {
            (Self::File(left), Self::File(right)) => left.same_target(right),
            (
                Self::Host {
                    resolver: left,
                    reference: l,
                    ..
                },
                Self::Host {
                    resolver: right,
                    reference: r,
                    ..
                },
            ) => Ok(Arc::ptr_eq(left, right) && l == r),
            _ => Ok(false),
        }
    }
    /// Stage and commit one file on its own, as a save of that one file.
    pub fn publish(&mut self, declared: &str, bytes: &[u8]) -> Result<(), file::Error> {
        let mut locks = file::Locks::default();
        let staging = match self.stage(&mut locks, declared, bytes, &[], &mut |_| Ok(())) {
            Ok(staging) => staging,
            Err(Interruption::Failed(error)) => return Err(error),
            Err(Interruption::Cancelled) => unreachable!("a whole file is never interrupted"),
        };
        self.commit(staging)
    }
    fn request<'a>(
        &'a self,
        operation: Operation,
        token: &'a str,
        bytes: Option<&'a [u8]>,
    ) -> Option<(&'a Arc<dyn Resolver>, Request<'a>)> {
        let Self::Host {
            resolver,
            declaring,
            written,
            ..
        } = self
        else {
            return None;
        };
        Some((
            resolver,
            Request {
                operation,
                kind: Kind::Output,
                declaring,
                written,
                reference: Some(token),
                bytes,
                limit: None,
            },
        ))
    }
    /// Stage `bytes` one partition at a time, where `ends` marks each
    /// partition's end (D264), calling `between` with each partition after
    /// the first before it is written. The file publisher appends each
    /// partition to the one staged file it opened, checking the saved-file
    /// bound as the file grows; a caller's resolver stages complete bytes, so
    /// it is asked once every partition has passed. A cancellation or a
    /// failure between two partitions removes the partial staged file.
    fn stage(
        &self,
        locks: &mut file::Locks,
        declared: &str,
        bytes: &[u8],
        ends: &[usize],
        between: &mut dyn FnMut(usize) -> Result<(), Interruption>,
    ) -> Result<Staging, Interruption> {
        let failed = Interruption::Failed;
        let pieces = pieces(bytes, ends);
        let (written, reference) = match self {
            Self::File(publisher) if pieces.len() == 1 => {
                return publisher
                    .stage(locks, declared, bytes)
                    .map(Staging::File)
                    .map_err(failed);
            }
            Self::File(publisher) => {
                yamaa_core::limits::Limits::output_bytes(bytes.len())
                    .map_err(|error| failed(file::Error::Limit(error)))?;
                let mut writing = publisher.open(locks, declared).map_err(failed)?;
                for (partition, piece) in pieces.into_iter().enumerate() {
                    if partition > 0 {
                        match between(partition) {
                            Ok(()) => {}
                            // Dropped unfinished, the staged file is removed.
                            Err(Interruption::Cancelled) => return Err(Interruption::Cancelled),
                            Err(Interruption::Failed(error)) => {
                                return Err(failed(writing.fail(error)))
                            }
                        }
                    }
                    if let Err(error) = writing.append(piece) {
                        return Err(failed(writing.fail(error)));
                    }
                }
                return writing.finish().map(Staging::File).map_err(failed);
            }
            Self::Host {
                written, reference, ..
            } => (written, reference),
        };
        if written != declared {
            return Err(failed(file::Error::PathMismatch));
        }
        yamaa_core::limits::Limits::output_bytes(bytes.len())
            .map_err(|error| failed(file::Error::Limit(error)))?;
        for partition in 1..pieces.len() {
            between(partition)?;
        }
        let (resolver, request) = self
            .request(Operation::Stage, reference, Some(bytes))
            .expect("a host publisher");
        let reply = resolver
            .request(request)
            .map_err(|failure| failed(file::Error::Host(failure)))?;
        // Stage is atomic by contract: a failed Stage leaves nothing to abort,
        // and a reply without a staging token staged nothing.
        if reply.reference.is_empty() || reply.bytes.is_some() || reply.frame.is_some() {
            return Err(failed(file::Error::Host(Failure::Rejected)));
        }
        Ok(Staging::Host(reply.reference))
    }
    fn commit(&self, staging: Staging) -> Result<(), file::Error> {
        let token = match (self, staging) {
            (Self::File(publisher), Staging::File(staged)) => return publisher.commit(staged),
            (Self::Host { .. }, Staging::Host(token)) => token,
            _ => unreachable!("a staged file belongs to the publisher that staged it"),
        };
        let Self::Host { reference, .. } = self else {
            unreachable!("a host staging token belongs to a host publisher")
        };
        let (resolver, request) = self
            .request(Operation::Commit, &token, None)
            .expect("a host publisher");
        let committed = resolver.request(request).and_then(|reply| {
            if reply.reference == *reference && reply.bytes.is_none() && reply.frame.is_none() {
                Ok(())
            } else {
                Err(Failure::Publication)
            }
        });
        // Attempt-owned cleanup uses the returned staging token, never the
        // target reference.
        committed.map_err(|failure| match self.abort(Staging::Host(token)) {
            Ok(()) => file::Error::Host(failure),
            Err(_) => file::Error::Host(Failure::Cleanup),
        })
    }
    fn abort(&self, staging: Staging) -> Result<(), Leftover> {
        match (self, staging) {
            (Self::File(publisher), Staging::File(staged)) => publisher
                .abort(staged)
                .map_err(|(staging, cleanup)| Leftover::File(staging, cleanup)),
            (_, Staging::File(_)) => unreachable!("a staged file belongs to its file publisher"),
            (_, Staging::Host(token)) => {
                let (resolver, request) = self
                    .request(Operation::Abort, &token, None)
                    .expect("a host staging token belongs to a host publisher");
                resolver
                    .request(request)
                    .map(drop)
                    .map_err(|_| Leftover::Host)
            }
        }
    }
}
/// `error`, keeping what a staged file left behind beside it (OUTPUT-8).
fn keeping(error: file::Error, leftover: Leftover) -> file::Error {
    match leftover {
        Leftover::File(staging, cleanup) => file::Error::Cleanup {
            operation: Box::new(error),
            staging,
            cleanup,
        },
        Leftover::Host => file::Error::Host(Failure::Cleanup),
    }
}

/// One file a save writes: its target, the path its specification or project
/// file wrote, its complete bytes, and where each partition of them ends,
/// the last at their length, for a file published a partition at a time
/// (D264); none for a file published whole.
pub struct File<'a> {
    pub(crate) target: &'a AnchoredTarget,
    pub(crate) written: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) partitions: &'a [usize],
}

/// `bytes` cut where `ends` marks each partition's end, in order; whole when
/// `ends` marks no partition, or anything but consecutive pieces of `bytes`.
fn pieces<'b>(bytes: &'b [u8], ends: &[usize]) -> Vec<&'b [u8]> {
    if ends.last() != Some(&bytes.len()) || ends.windows(2).any(|pair| pair[0] > pair[1]) {
        return vec![bytes];
    }
    let mut start = 0;
    ends.iter()
        .map(|&end| {
            let piece = &bytes[start..end];
            start = end;
            piece
        })
        .collect()
}

/// What a save's call between two partitions of a file it stages asks
/// ([`stage_with`]).
pub enum Interruption {
    /// The save was cancelled.
    Cancelled,
    /// The file cannot be staged, as a test injects a failure.
    Failed(file::Error),
}
impl File<'_> {
    /// The path the save reports for the file once written.
    pub fn path(&self) -> &str {
        self.target.path()
    }
}

/// Where a save stopped: the position of the file in save order, how many
/// files before it replaced their targets, and why.
#[derive(Debug)]
pub struct Stop {
    pub file: usize,
    pub committed: usize,
    pub cause: Cause,
}
pub enum Cause {
    /// The file's target cannot be written.
    Target(file::Error),
    /// The file names the target of the earlier file at this position.
    Collision { earlier: usize },
    /// The file could not be staged.
    Stage(file::Error),
    /// The save was cancelled between two partitions of the file (D264).
    Cancelled,
    /// The file could not replace its target.
    Commit(file::Error),
    /// A caller's resolver unwound while staging or committing the file; the
    /// original payload is kept.
    Unwind(Box<dyn Any + Send>),
}
impl std::fmt::Debug for Cause {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Target(error) => formatter.debug_tuple("Target").field(error).finish(),
            Self::Collision { earlier } => formatter
                .debug_struct("Collision")
                .field("earlier", earlier)
                .finish(),
            Self::Stage(error) => formatter.debug_tuple("Stage").field(error).finish(),
            Self::Cancelled => formatter.write_str("Cancelled"),
            Self::Commit(error) => formatter.debug_tuple("Commit").field(error).finish(),
            Self::Unwind(_) => formatter.write_str("Unwind"),
        }
    }
}

/// One staged file with the publisher that staged it. Dropping it before it is
/// committed removes it.
struct StagedFile {
    publisher: Publisher,
    staging: Option<Staging>,
    path: String,
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = catch_unwind(AssertUnwindSafe(|| self.publisher.abort(staging)));
        }
    }
}
/// Every file of one save, staged beside its target in save order and not yet
/// visible. Dropping it removes every staged file; the save's directory locks
/// are released after the last file is committed or removed.
pub struct Staged {
    files: Vec<StagedFile>,
    locks: file::Locks,
}

/// Open every file's target, then stage every file, in save order. A target
/// that cannot be written, two files that name one target, or a file that
/// cannot be staged stop the save before any target is replaced, and every
/// file already staged is removed.
pub fn stage(files: &[File<'_>]) -> Result<Staged, Stop> {
    stage_with(files, |_, _| Ok(()))
}

/// [`stage`], calling `between` before each partition of a file after its
/// first, with the file's position in save order and the partition's
/// (D264): a save polls its cancellation there, and a test injects a
/// failure. A cancellation or a failure stops the save there, before any
/// target is replaced: the file's partial staged file and every file
/// already staged are removed.
pub fn stage_with(
    files: &[File<'_>],
    mut between: impl FnMut(usize, usize) -> Result<(), Interruption>,
) -> Result<Staged, Stop> {
    let mut publishers: Vec<Publisher> = Vec::with_capacity(files.len());
    for (index, file) in files.iter().enumerate() {
        let stop = |cause| Stop {
            file: index,
            committed: 0,
            cause,
        };
        let publisher = file
            .target
            .publisher(file.written)
            .map_err(|error| stop(Cause::Target(error)))?;
        for (earlier, prior) in publishers.iter().enumerate() {
            match publisher.same_target(prior) {
                Ok(false) => {}
                Ok(true) => return Err(stop(Cause::Collision { earlier })),
                Err(error) => return Err(stop(Cause::Target(error))),
            }
        }
        publishers.push(publisher);
    }
    let mut staged = Staged {
        files: Vec::with_capacity(files.len()),
        locks: file::Locks::default(),
    };
    for (index, (publisher, file)) in publishers.into_iter().zip(files).enumerate() {
        let locks = &mut staged.locks;
        let between = &mut between;
        let cause = match catch_unwind(AssertUnwindSafe(|| {
            publisher.stage(
                locks,
                file.written,
                file.bytes,
                file.partitions,
                &mut |partition| between(index, partition),
            )
        })) {
            Ok(Ok(staging)) => {
                staged.files.push(StagedFile {
                    publisher,
                    staging: Some(staging),
                    path: file.path().to_owned(),
                });
                continue;
            }
            Ok(Err(Interruption::Failed(error))) => Cause::Stage(error),
            Ok(Err(Interruption::Cancelled)) => Cause::Cancelled,
            Err(payload) => Cause::Unwind(payload),
        };
        return Err(staged.stop(index, 0, cause));
    }
    Ok(staged)
}

impl Staged {
    /// Replace each target in save order and return every file's path. The
    /// first file that cannot replace its target stops the save: the files
    /// before it stay replaced, and it and every later file are removed.
    pub fn commit(self) -> Result<Vec<String>, Stop> {
        self.commit_with(|_| Ok(()))
    }
    /// [`Self::commit`], first calling `before` with each file's position;
    /// an error it returns fails that file's commit, as a test injects one.
    pub fn commit_with(
        mut self,
        mut before: impl FnMut(usize) -> Result<(), file::Error>,
    ) -> Result<Vec<String>, Stop> {
        let mut pending = std::mem::take(&mut self.files);
        pending.reverse();
        let mut paths = Vec::with_capacity(pending.len());
        while let Some(mut file) = pending.pop() {
            let index = paths.len();
            let staging = file.staging.take().expect("a staged file");
            let publisher = &file.publisher;
            let cause = match catch_unwind(AssertUnwindSafe(|| before(index))) {
                Ok(Ok(())) => match catch_unwind(AssertUnwindSafe(|| publisher.commit(staging))) {
                    Ok(Ok(())) => {
                        paths.push(std::mem::take(&mut file.path));
                        continue;
                    }
                    Ok(Err(error)) => Cause::Commit(error),
                    Err(payload) => Cause::Unwind(payload),
                },
                Ok(Err(error)) => Cause::Commit(match publisher.abort(staging) {
                    Ok(()) => error,
                    Err(leftover) => keeping(error, leftover),
                }),
                Err(payload) => {
                    let _ = publisher.abort(staging);
                    Cause::Unwind(payload)
                }
            };
            pending.reverse();
            self.files = pending;
            return Err(self.stop(index, index, cause));
        }
        Ok(paths)
    }
    /// Remove every staged file; no target is touched.
    pub fn abort(mut self) {
        self.remove();
    }
    /// Remove every file still staged, returning the first removal that left
    /// something behind.
    fn remove(&mut self) -> Option<Leftover> {
        let mut first = None;
        for mut file in std::mem::take(&mut self.files) {
            let Some(staging) = file.staging.take() else {
                continue;
            };
            let removed = catch_unwind(AssertUnwindSafe(|| file.publisher.abort(staging)));
            if let Ok(Err(leftover)) = removed {
                first.get_or_insert(leftover);
            }
        }
        first
    }
    /// Stop at `file`, after `committed` files replaced their targets: remove
    /// every file still staged, and keep what a removal left behind with the
    /// cause's error.
    fn stop(mut self, file: usize, committed: usize, cause: Cause) -> Stop {
        let cause = match (self.remove(), cause) {
            (Some(leftover), Cause::Stage(error)) => Cause::Stage(keeping(error, leftover)),
            (Some(leftover), Cause::Commit(error)) => Cause::Commit(keeping(error, leftover)),
            (_, cause) => cause,
        };
        Stop {
            file,
            committed,
            cause,
        }
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        self.remove();
        // The locks are released after every staged file is gone.
        drop(std::mem::take(&mut self.locks));
    }
}

/// The finding of a file whose target an earlier file of another member of
/// the same save also writes.
pub fn shared_target() -> Vec<yamaa_core::diagnostic::Diagnostic> {
    crate::file_application::publication_failure(&file::Error::InvalidTarget)
}
