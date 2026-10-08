//! Atomic Windows publication through retained root, parent and source handles.
use crate::windows_file::{self, Directory, DirectoryLock, Identity, Kind};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const MAX_PATH_BYTES: usize = 65_536;
const MAX_OUTPUT_BYTES: usize = 67_108_864;
static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Debug)]
pub enum Error {
    InvalidTarget,
    PathMismatch,
    Limit,
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
            Self::InvalidTarget => "invalid explicit publication target",
            Self::PathMismatch => "publication path does not match explicit target",
            Self::Limit => "publication byte or path limit",
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
                )
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
pub struct Publisher {
    parent: Directory,
    parent_path: PathBuf,
    name: String,
    declared: String,
}
pub(crate) struct AnchoredTarget {
    root: Directory,
    remainder: Vec<String>,
    target: String,
}
impl AnchoredTarget {
    pub(crate) fn new(root: Directory, remainder: Vec<String>, target: String) -> Self {
        Self {
            root,
            remainder,
            target,
        }
    }
    pub(crate) fn publisher(&self, declared: &str) -> Result<Publisher, Error> {
        if declared.len() > MAX_PATH_BYTES || self.target.len() > MAX_PATH_BYTES {
            return Err(Error::Limit);
        }
        if declared.is_empty() || declared.contains('\0') || self.remainder.is_empty() {
            return Err(Error::InvalidTarget);
        }
        let (name, parents) = self.remainder.split_last().ok_or(Error::InvalidTarget)?;
        if self
            .remainder
            .iter()
            .any(|s| s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\', ':', '\0']))
        {
            return Err(Error::InvalidTarget);
        }
        let mut directories = Vec::new();
        let mut witnesses = Vec::new();
        for name in parents {
            let parent = directories.last().unwrap_or(&self.root);
            let initial = parent.inspect(name).map_err(|_| Error::InvalidTarget)?;
            if initial.kind != Kind::Directory {
                return Err(Error::InvalidTarget);
            }
            let opened = parent.directory(name).map_err(|_| Error::InvalidTarget)?;
            if opened.identity()? != initial.identity {
                return Err(Error::Changed);
            }
            witnesses.push(initial.identity);
            directories.push(opened);
        }
        for (index, expected) in witnesses.iter().enumerate() {
            let parent = if index == 0 {
                &self.root
            } else {
                &directories[index - 1]
            };
            if parent
                .inspect(&parents[index])
                .map_err(|_| Error::Changed)?
                .identity
                != *expected
            {
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
    /// Explicit caller-selected target; authored declarations grant no write authority.
    pub fn new(declared: &str, target: &str) -> Result<Self, Error> {
        if declared.len() > MAX_PATH_BYTES || target.len() > MAX_PATH_BYTES {
            return Err(Error::Limit);
        }
        if declared.is_empty()
            || declared.contains('\0')
            || target.contains('\0')
            || matches!(
                target.rsplit(['/', '\\']).next(),
                None | Some("" | "." | "..")
            )
        {
            return Err(Error::InvalidTarget);
        }
        let target = Path::new(target);
        if !target.is_absolute() {
            return Err(Error::InvalidTarget);
        }
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::InvalidTarget)?
            .to_owned();
        let parent_path = std::fs::canonicalize(target.parent().ok_or(Error::InvalidTarget)?)
            .map_err(|_| Error::InvalidTarget)?;
        let parent = Directory::open(&parent_path).map_err(|_| Error::InvalidTarget)?;
        if Directory::open(&parent_path)
            .map_err(|_| Error::InvalidTarget)?
            .identity()?
            != parent.identity()?
        {
            return Err(Error::Changed);
        }
        let publisher = Self {
            parent,
            parent_path,
            name,
            declared: declared.into(),
        };
        publisher.check_target()?;
        Ok(publisher)
    }
    fn check_target(&self) -> Result<(), Error> {
        match self.parent.inspect(&self.name) {
            Ok(status) if status.kind == Kind::RegularFile => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(Error::InvalidTarget),
            Err(error) => Err(error.into()),
        }
    }
    pub fn publish(&mut self, declared: &str, content: &[u8]) -> Result<(), Error> {
        self.publish_with_checkpoint(declared, content, |_| Ok(()))
    }
    fn publish_with_checkpoint(
        &mut self,
        declared: &str,
        content: &[u8],
        checkpoint: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        if content.len() > MAX_OUTPUT_BYTES {
            return Err(Error::Limit);
        }
        let _lock = DirectoryLock::acquire(&self.parent)?;
        self.check_target()?;
        let mut temporary = self.temporary()?;
        let publication = (|| {
            let file = temporary.candidate.as_mut().ok_or(Error::Changed)?;
            file.write_all(content)?;
            file.flush()?;
            file.sync_all()?;
            let held = windows_file::status(file)?;
            let observed = temporary.directory.inspect("candidate.part")?;
            if held.kind != Kind::RegularFile || held.identity != observed.identity {
                return Err(Error::Changed);
            }
            self.check_target()?;
            checkpoint(&temporary.name)?;
            self.parent.replace(file, &self.name)?;
            temporary.candidate.take();
            Ok(())
        })();
        match publication {
            Ok(()) => {
                let _ = temporary.cleanup();
                Ok(())
            }
            Err(error) => Err(temporary.failure(error)),
        }
    }
    fn temporary(&self) -> Result<Pending<'_>, Error> {
        self.temporary_with_numbers(|| NEXT.fetch_add(1, Ordering::Relaxed))
    }
    fn temporary_with_numbers(&self, mut next: impl FnMut() -> u64) -> Result<Pending<'_>, Error> {
        for _ in 0..64 {
            let name = format!(".yamaa-output-{}-{}.stage", std::process::id(), next());
            if name == self.name {
                continue;
            }
            let directory = match self.parent.create_private_directory(&name) {
                Ok(directory) => directory,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            };
            let identity = match directory.identity() {
                Ok(identity) => identity,
                Err(operation) => {
                    return Err(match directory.remove() {
                        Ok(()) => Error::Io(operation),
                        Err(cleanup) => Error::Cleanup {
                            operation: Box::new(Error::Io(operation)),
                            staging: self.parent_path.join(&name),
                            cleanup,
                        },
                    });
                }
            };
            let mut temporary = Pending {
                parent: &self.parent,
                parent_path: &self.parent_path,
                name,
                directory,
                identity,
                candidate: None,
                cleaned: false,
            };
            match temporary.directory.create_file("candidate.part") {
                Ok(file) => temporary.candidate = Some(file),
                Err(error) => return Err(temporary.failure(error.into())),
            }
            return Ok(temporary);
        }
        Err(Error::Limit)
    }
}
struct Pending<'a> {
    parent: &'a Directory,
    parent_path: &'a Path,
    name: String,
    directory: Directory,
    identity: Identity,
    candidate: Option<File>,
    cleaned: bool,
}
impl Pending<'_> {
    fn cleanup(&mut self) -> std::io::Result<()> {
        if self.cleaned {
            return Ok(());
        }
        match self.directory.inspect("candidate.part") {
            Ok(current) => {
                let candidate = self
                    .candidate
                    .as_ref()
                    .ok_or_else(|| std::io::Error::other("temporary candidate identity changed"))?;
                if current.identity != windows_file::status(candidate)?.identity {
                    return Err(std::io::Error::other(
                        "temporary candidate identity changed",
                    ));
                }
                windows_file::mark_delete(candidate)?;
                self.candidate.take();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.candidate.take();
            }
            Err(error) => return Err(error),
        }
        let current = self.parent.inspect(&self.name)?;
        if current.kind != Kind::Directory || current.identity != self.identity {
            return Err(std::io::Error::other(
                "temporary directory identity unconfirmed or changed",
            ));
        }
        self.directory.remove()?;
        self.cleaned = true;
        Ok(())
    }
    fn failure(&mut self, error: Error) -> Error {
        match self.cleanup() {
            Ok(()) => error,
            Err(cleanup) => Error::Cleanup {
                operation: Box::new(error),
                staging: self.parent_path.join(&self.name),
                cleanup,
            },
        }
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
impl yamaa_engine::specification_output::ArtifactPort for Publisher {
    type Error = Error;
    fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Error> {
        Publisher::publish(self, path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);
    struct Study(PathBuf);
    impl Study {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-windows-publish-{}-{}",
                std::process::id(),
                NEXT_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(std::fs::canonicalize(path).unwrap())
        }
        fn publisher(&self, target: &str) -> Publisher {
            Publisher::new("output.csv", self.0.join(target).to_str().unwrap()).unwrap()
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
    fn complete_publication_preserves_old_hardlinks_and_removes_staging() {
        let study = Study::new();
        let target = study.0.join("output.csv");
        std::fs::write(&target, b"old").unwrap();
        std::fs::hard_link(&target, study.0.join("old.csv")).unwrap();
        let mut publisher = study.publisher("output.csv");
        for content in [b"checked\n".as_slice(), b"", b"Unicode: \xc3\xa9\n"] {
            publisher.publish("output.csv", content).unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), content);
            assert_eq!(std::fs::read(study.0.join("old.csv")).unwrap(), b"old");
            assert_eq!(study.names(), ["old.csv", "output.csv"]);
        }
    }
    #[test]
    fn refused_paths_limits_and_directory_targets_preserve_existing_bytes() {
        let study = Study::new();
        std::fs::write(study.0.join("output.csv"), b"old").unwrap();
        let mut publisher = study.publisher("output.csv");
        assert!(matches!(
            publisher.publish("different.csv", b"new"),
            Err(Error::PathMismatch)
        ));
        assert!(matches!(
            publisher.publish("output.csv", &vec![0; MAX_OUTPUT_BYTES + 1]),
            Err(Error::Limit)
        ));
        for target in [
            "output.csv/",
            "output.csv/.",
            "output.csv/..",
            "output.csv:stream",
        ] {
            assert!(Publisher::new("output.csv", study.0.join(target).to_str().unwrap()).is_err());
        }
        assert_eq!(std::fs::read(study.0.join("output.csv")).unwrap(), b"old");
        assert_eq!(study.names(), ["output.csv"]);
    }
    #[test]
    fn target_is_never_created_as_a_staging_directory() {
        let study = Study::new();
        let name = format!(".yamaa-output-{}-0.stage", std::process::id());
        let publisher = study.publisher(&name);
        assert!(matches!(
            publisher.temporary_with_numbers(|| 0),
            Err(Error::Limit)
        ));
        assert!(study.names().is_empty());
    }
    #[test]
    fn owned_parent_handle_keeps_publication_beneath_the_selected_physical_root() {
        let study = Study::new();
        let selected = study.0.join("selected");
        let moved = study.0.join("moved");
        std::fs::create_dir(&selected).unwrap();
        let mut publisher = study.publisher("selected/output.csv");
        std::fs::rename(&selected, &moved).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("output.csv"), b"replacement").unwrap();
        publisher.publish("output.csv", b"checked").unwrap();
        assert_eq!(std::fs::read(moved.join("output.csv")).unwrap(), b"checked");
        assert_eq!(
            std::fs::read(selected.join("output.csv")).unwrap(),
            b"replacement"
        );
    }
    #[test]
    fn operation_failure_with_foreign_staging_substitution_preserves_foreign_entries() {
        let study = Study::new();
        std::fs::write(study.0.join("output.csv"), b"old").unwrap();
        let mut publisher = study.publisher("output.csv");
        let result = publisher.publish_with_checkpoint("output.csv", b"checked", |name| {
            std::fs::rename(study.0.join(name), study.0.join("moved-stage")).unwrap();
            std::fs::create_dir(study.0.join(name)).unwrap();
            std::fs::write(study.0.join(name).join("foreign"), b"retained").unwrap();
            Err(Error::InvalidTarget)
        });
        let Err(Error::Cleanup {
            operation, staging, ..
        }) = result
        else {
            panic!("cleanup must preserve the original failure");
        };
        assert!(matches!(*operation, Error::InvalidTarget));
        assert_eq!(std::fs::read(staging.join("foreign")).unwrap(), b"retained");
        assert_eq!(std::fs::read(study.0.join("output.csv")).unwrap(), b"old");
    }
}
