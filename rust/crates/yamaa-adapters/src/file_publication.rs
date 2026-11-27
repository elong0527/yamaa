//! Atomic byte publication to one explicit caller-selected file target.
//!
//! This module holds what publication decides on every platform: its
//! errors, the walk to an anchored target and the staging of one whole
//! file. The platform module holds the filesystem calls: `unix` through
//! retained descriptors, `windows` through retained root, parent and source
//! handles.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use yamaa_core::resource::ResourceFailure;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

pub use platform::{Locks, Staged, Writing};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum Error {
    Host(crate::host_resources::Failure),
    InvalidTarget,
    /// The walk to the target stopped at an entry FILE-7 or FILE-8 refuses:
    /// a directory on its way that is missing (`Missing`), is not a
    /// directory (`NotRegularFile`) or is a link or a Windows reparse point
    /// (`Symlink`), or a target that is a link, a reparse point or another
    /// entry that is not a regular file.
    Unreachable(ResourceFailure),
    PathMismatch,
    Limit(yamaa_core::limits::LimitExceeded),
    StagingCollision,
    Changed,
    Io(std::io::Error),
    Cleanup {
        operation: Box<Error>,
        staging: PathBuf,
        cleanup: std::io::Error,
    },
}
impl Error {
    pub fn message(&self) -> String {
        match self {
            Self::Host(_) => "host resource publication failed",
            Self::InvalidTarget => "invalid explicit publication target",
            Self::Unreachable(_) => "publication target cannot be reached",
            Self::PathMismatch => "publication path does not match explicit target",
            Self::Limit(_) => "publication output byte limit",
            Self::StagingCollision => "publication temporary names already exist",
            Self::Changed => "publication target changed",
            Self::Io(_) => "artifact publication failed",
            Self::Cleanup {
                operation,
                staging,
                cleanup,
            } => {
                return format!(
                    "{}; temporary cleanup failed beneath selected parent at {}: {cleanup}",
                    operation.message(),
                    staging.display()
                );
            }
        }
        .into()
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// An entry on the walk to a target, as FILE-7 and FILE-8 sort it.
enum Entry {
    Directory,
    /// A link, or a Windows reparse point.
    Link,
    Other,
}

pub struct Publisher {
    parent: platform::Handle,
    parent_path: PathBuf,
    name: String,
    declared: String,
}
/// Retain the selected physical root; an authored output cannot add authority.
pub struct AnchoredTarget {
    root: platform::Handle,
    remainder: Vec<String>,
    target: String,
}
impl AnchoredTarget {
    pub(crate) fn new(root: platform::Handle, remainder: Vec<String>, target: String) -> Self {
        Self {
            root,
            remainder,
            target,
        }
    }
    /// The target's absolute path in the engine's spelling, with forward
    /// slashes.
    pub(crate) fn path(&self) -> &str {
        &self.target
    }
    pub(crate) fn publisher(&self, declared: &str) -> Result<Publisher, Error> {
        if declared.is_empty() || declared.contains('\0') || self.remainder.is_empty() {
            return Err(Error::InvalidTarget);
        }
        let (name, parents) = self.remainder.split_last().ok_or(Error::InvalidTarget)?;
        platform::admit_name(name)?;
        if self
            .remainder
            .iter()
            .any(|s| s.is_empty() || s == "." || s == ".." || s.contains(platform::SEPARATORS))
        {
            return Err(Error::InvalidTarget);
        }
        let mut directories = Vec::new();
        let mut witnesses = Vec::new();
        for name in parents {
            let parent = directories.last().unwrap_or(&self.root);
            let (entry, initial) = platform::inspect(parent, name).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Error::Unreachable(ResourceFailure::Missing)
                } else {
                    Error::InvalidTarget
                }
            })?;
            match entry {
                Entry::Directory => {}
                Entry::Link => return Err(Error::Unreachable(ResourceFailure::Symlink)),
                Entry::Other => return Err(Error::Unreachable(ResourceFailure::NotRegularFile)),
            }
            let opened = platform::directory(parent, name).map_err(|_| Error::InvalidTarget)?;
            if platform::held_identity(&opened)? != initial {
                return Err(Error::Changed);
            }
            witnesses.push(initial);
            directories.push(opened);
        }
        // Check the complete parent chain before handing the final handle
        // to publication. Replacement names never redirect this held authority.
        for (index, expected) in witnesses.iter().enumerate() {
            let parent = if index == 0 {
                &self.root
            } else {
                &directories[index - 1]
            };
            let (_, observed) =
                platform::inspect(parent, &parents[index]).map_err(|_| Error::Changed)?;
            if observed != *expected {
                return Err(Error::Changed);
            }
        }
        let parent = directories
            .pop()
            .map(Ok)
            .unwrap_or_else(|| self.root.try_clone())?;
        let publisher = Publisher {
            parent,
            parent_path: Path::new(&self.target)
                .parent()
                .ok_or(Error::InvalidTarget)?
                .into(),
            name: name.clone(),
            declared: declared.into(),
        };
        publisher.check_target()?;
        Ok(publisher)
    }
}
impl Publisher {
    /// Stage and commit one file on its own, as a save of that one file.
    pub fn publish(&mut self, declared: &str, content: &[u8]) -> Result<(), Error> {
        let mut locks = Locks::default();
        let staged = self.stage(&mut locks, declared, content)?;
        self.commit(staged)
    }
    /// Write `content` completely into a temporary file in a private
    /// directory beside the target, flushed and synced, without making it
    /// visible: [`Self::open`], one [`Writing::append`] and
    /// [`Writing::finish`]. The saved-file bound is checked before anything
    /// is staged.
    pub fn stage(
        &self,
        locks: &mut Locks,
        declared: &str,
        content: &[u8],
    ) -> Result<Staged, Error> {
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        yamaa_core::limits::Limits::output_bytes(content.len()).map_err(Error::Limit)?;
        let mut writing = self.open(locks, declared)?;
        match writing.append(content) {
            Ok(()) => writing.finish(),
            Err(error) => Err(writing.fail(error)),
        }
    }
}
/// The next number of a staging directory's name in this process.
fn next_number() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
/// The name of the private staging directory numbered `number`.
fn staging_name(number: u64) -> String {
    format!(".yamaa-output-{}-{number}.stage", std::process::id())
}
/// `error`, or, when its cleanup failed, `error` with the cleanup's cause
/// and the staging location it left behind.
fn with_cleanup(error: Error, staging: PathBuf, cleanup: std::io::Result<()>) -> Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup) => Error::Cleanup {
            operation: Box::new(error),
            staging,
            cleanup,
        },
    }
}
