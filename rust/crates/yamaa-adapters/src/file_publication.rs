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
/// The entry that holds a save's lock on a directory it writes into on
/// Windows (OUTPUT-8). Its name, in any ASCII case, is refused as a target
/// and as a directory on the way to one on every platform, so a save never
/// writes, replaces or walks through it, and saves behave alike everywhere.
pub(crate) const LOCK_NAME: &str = ".yamaa-output.lock";

/// Whether `name` is [`LOCK_NAME`] in some case. Its letters have no
/// non-ASCII case alias, so ASCII folding covers Windows' comparison.
fn reserved(name: &str) -> bool {
    name.eq_ignore_ascii_case(LOCK_NAME)
}
/// Admit the final name of a target: not the reserved lock name, and one
/// the platform admits.
fn admit_target_name(name: &str) -> Result<(), Error> {
    if reserved(name) {
        return Err(Error::InvalidTarget);
    }
    platform::admit_name(name)
}

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
        admit_target_name(name)?;
        if self.remainder.iter().any(|s| {
            s.is_empty() || s == "." || s == ".." || s.contains(platform::SEPARATORS) || reserved(s)
        }) {
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
#[cfg(test)]
mod tests {
    use super::*;
    /// A scratch directory holding one prior file, removed when dropped.
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-publication-reserved-{}-{}",
                std::process::id(),
                next_number()
            ));
            std::fs::create_dir(&path).unwrap();
            let path = crate::canonical_path::canonicalize(path).unwrap();
            std::fs::write(path.join("output.csv"), b"old").unwrap();
            Self(path)
        }
        fn root(&self) -> platform::Handle {
            #[cfg(unix)]
            {
                use rustix::fs::{Mode, OFlags};
                rustix::fs::open(
                    &self.0,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .unwrap()
            }
            #[cfg(windows)]
            {
                crate::windows_file::Directory::open(&self.0).unwrap()
            }
        }
        fn names(&self) -> Vec<String> {
            let mut names: Vec<_> = std::fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_str().unwrap().into())
                .collect();
            names.sort();
            names
        }
    }
    impl Drop for Study {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn the_lock_name_is_refused_as_a_target_and_as_a_directory_on_the_way_in_any_case() {
        // Seam: target admission on every platform, before any directory is
        // walked or locked; the hosts' save fixtures cannot name a target
        // the publisher's own lock entry would collide with on Windows.
        let study = Study::new();
        for name in [LOCK_NAME, ".YAMAA-OUTPUT.LOCK", ".Yamaa-Output.Lock"] {
            let written = study.0.join(name);
            let explicit = Publisher::new("output.csv", written.to_str().unwrap()).map(drop);
            let path = |segments: &[&str]| {
                segments
                    .iter()
                    .fold(study.0.clone(), |path, s| path.join(s))
            };
            let anchored = |segments: &[&str]| {
                AnchoredTarget::new(
                    study.root(),
                    segments.iter().map(|s| (*s).to_owned()).collect(),
                    path(segments).to_str().unwrap().replace('\\', "/"),
                )
                .publisher("output.csv")
                .map(drop)
            };
            let observed = [explicit, anchored(&[name]), anchored(&[name, "output.csv"])]
                .map(|result| matches!(result, Err(Error::InvalidTarget)));
            assert_eq!(
                (observed, study.names()),
                ([true; 3], vec!["output.csv".to_owned()]),
                "{name:?} as an explicit target, an anchored target and an anchored directory"
            );
        }
        assert!(
            Publisher::new(
                "output.csv",
                study.0.join(".yamaa-output.lock.csv").to_str().unwrap()
            )
            .is_ok(),
            "a name that only begins with the lock name is admitted"
        );
    }
}
