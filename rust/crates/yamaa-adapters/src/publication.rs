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
//! Each file goes through the engine's one publisher port, the adapters'
//! `publisher` (#3030 section 2.10). Every save writes through it, as does a
//! saved table, and a submission composition admits its targets through
//! `AnchoredTarget` before it is held. The files come encoded: a run's result
//! and a composition hold their bytes, and a save never encodes again.
use crate::file_publication as file;
use crate::publisher::{keeping, AnchoredTarget, Interruption, Leftover, Publisher, Staging};
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};

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
