//! Windows publication through retained root, parent and source handles:
//! the filesystem calls of [`super`] on Windows.
use super::{next_number, staging_name, with_cleanup, Entry, Error, Publisher};
use crate::windows_file::{self, Directory, DirectoryLock, Identity, Kind};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use yamaa_core::resource::ResourceFailure;

/// The retained handle of a directory.
pub(super) type Handle = Directory;
/// The characters an anchored target's component may not hold.
pub(super) const SEPARATORS: [char; 4] = ['/', '\\', ':', '\0'];

/// Refuse a final name Win32 would alter or reserves.
pub(super) fn admit_name(name: &str) -> Result<(), Error> {
    windows_file::validate_publication_name(name).map_err(|_| Error::InvalidTarget)
}
/// The entry `name` beneath `parent`, without following a reparse point.
pub(super) fn inspect(parent: &Directory, name: &str) -> std::io::Result<(Entry, Identity)> {
    let status = parent.inspect(name)?;
    let entry = match status.kind {
        Kind::Directory => Entry::Directory,
        Kind::Symlink => Entry::Link,
        _ => Entry::Other,
    };
    Ok((entry, status.identity))
}
/// Open the directory `name` beneath `parent`, without following a reparse
/// point.
pub(super) fn directory(parent: &Directory, name: &str) -> std::io::Result<Directory> {
    parent.directory(name)
}
/// The identity of the directory `handle` holds.
pub(super) fn held_identity(handle: &Directory) -> std::io::Result<Identity> {
    handle.identity()
}
impl Publisher {
    /// Compare entries through retained physical parents, including selected-root
    /// aliases and absent ordinal case aliases. No target is opened or written.
    pub fn same_target(&self, other: &Self) -> Result<bool, Error> {
        Ok(self.parent.identity()? == other.parent.identity()?
            && crate::file_resources::equal_ignoring_case(
                &self.name,
                &other.name,
                crate::file_resources::Units::Utf16,
            ))
    }
    /// Explicit caller-selected target; authored declarations grant no write authority.
    pub fn new(declared: &str, target: &str) -> Result<Self, Error> {
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
        super::admit_target_name(&name)?;
        let parent_path =
            crate::canonical_path::canonicalize(target.parent().ok_or(Error::InvalidTarget)?)
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
    /// Admit an absent target or a regular file; a reparse point or another
    /// entry is unreachable.
    pub(super) fn check_target(&self) -> Result<(), Error> {
        match self.parent.inspect(&self.name) {
            Ok(status) if status.kind == Kind::RegularFile => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(status) if status.kind == Kind::Symlink => {
                Err(Error::Unreachable(ResourceFailure::Symlink))
            }
            Ok(_) => Err(Error::Unreachable(ResourceFailure::NotRegularFile)),
            Err(error) => Err(error.into()),
        }
    }
    /// Open the staged file of `declared`: an empty private temporary file
    /// beside the target, which [`Writing::append`] fills piece by piece,
    /// such as one partition of an output after another (D264), and
    /// [`Writing::finish`] stages, without making it visible. The target's
    /// directory stays locked in `locks` until the save ends (OUTPUT-8).
    pub fn open(&self, locks: &mut Locks, declared: &str) -> Result<Writing, Error> {
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        locks.hold(&self.parent)?;
        self.check_target()?;
        let temporary = self.temporary()?;
        Ok(Writing {
            temporary,
            written: 0,
        })
    }
    /// Replace the target with the staged file in one step.
    pub fn commit(&self, staged: Staged) -> Result<(), Error> {
        self.commit_with_checkpoint(staged, |_| Ok(()))
    }
    /// Remove the staged file; the target stays as it was. A removal that
    /// fails names the staging location it left behind.
    pub fn abort(&self, staged: Staged) -> Result<(), (PathBuf, std::io::Error)> {
        staged.abort()
    }
    fn commit_with_checkpoint(
        &self,
        staged: Staged,
        checkpoint: impl FnOnce(&mut Pending) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut temporary = staged.0;
        let replaced = (|| {
            let candidate = temporary.candidate.as_ref().ok_or(Error::Changed)?;
            let held = windows_file::status(candidate)?;
            let observed = temporary.directory.inspect("candidate.part")?;
            if held.identity != observed.identity {
                return Err(Error::Changed);
            }
            self.check_target()?;
            checkpoint(&mut temporary)?;
            self.parent.replace(
                temporary.candidate.as_ref().ok_or(Error::Changed)?,
                &self.name,
            )?;
            temporary.candidate.take();
            Ok(())
        })();
        match replaced {
            Ok(()) => {
                let _ = temporary.cleanup();
                Ok(())
            }
            Err(error) => Err(temporary.failure(error)),
        }
    }
    #[cfg(test)]
    fn publish_with_checkpoint(
        &mut self,
        declared: &str,
        content: &[u8],
        checkpoint: impl FnOnce(&mut Pending) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut locks = Locks::default();
        let staged = self.stage(&mut locks, declared, content)?;
        self.commit_with_checkpoint(staged, checkpoint)
    }
    fn temporary(&self) -> Result<Pending, Error> {
        self.temporary_with_numbers(next_number)
    }
    fn temporary_with_numbers(&self, mut next: impl FnMut() -> u64) -> Result<Pending, Error> {
        let mut parent = Some(self.parent.try_clone()?);
        for _ in 0..64 {
            let name = staging_name(next());
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
                    return Err(with_cleanup(
                        Error::Io(operation),
                        self.parent_path.join(&name),
                        directory.remove(),
                    ));
                }
            };
            let mut temporary = Pending {
                parent: parent.take().ok_or(Error::Changed)?,
                parent_path: self.parent_path.clone(),
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
        Err(Error::StagingCollision)
    }
}
/// One staged file: its complete bytes in a private temporary file beside the
/// target, not yet visible there. Dropping it removes the temporary file.
pub struct Staged(Pending);
impl Staged {
    /// Remove the staged file; the target stays as it was. A removal that
    /// fails names the staging location it left behind.
    pub fn abort(self) -> Result<(), (PathBuf, std::io::Error)> {
        let mut temporary = self.0;
        temporary
            .cleanup()
            .map_err(|cleanup| (temporary.parent_path.join(&temporary.name), cleanup))
    }
}
/// A staged file being written: the temporary file [`Publisher::open`]
/// made, which nothing at the target shows, written through the handle that
/// created it. Dropping it before [`Writing::finish`] removes it through that
/// handle, as a failed or cancelled save does.
pub struct Writing {
    temporary: Pending,
    written: usize,
}
impl Writing {
    /// Append `bytes`, checking the saved-file bound before the file grows.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        yamaa_core::limits::Limits::output_extension(self.written, bytes.len())
            .map_err(Error::Limit)?;
        let file = self.temporary.candidate.as_mut().ok_or(Error::Changed)?;
        file.write_all(bytes)?;
        self.written += bytes.len();
        Ok(())
    }
    /// Flush and sync the complete file and keep it for a later commit; a
    /// file that cannot be staged is removed.
    pub fn finish(self) -> Result<Staged, Error> {
        let Self { mut temporary, .. } = self;
        let written = (|| {
            let file = temporary.candidate.as_mut().ok_or(Error::Changed)?;
            file.flush()?;
            file.sync_all()?;
            let held = windows_file::status(file)?;
            let observed = temporary.directory.inspect("candidate.part")?;
            if held.kind != Kind::RegularFile || held.identity != observed.identity {
                return Err(Error::Changed);
            }
            Ok(())
        })();
        match written {
            Ok(()) => Ok(Staged(temporary)),
            Err(error) => Err(temporary.failure(error)),
        }
    }
    /// Stop writing for `error`: remove the file, keeping with the error
    /// what a removal that fails leaves behind.
    pub fn fail(mut self, error: Error) -> Error {
        self.temporary.failure(error)
    }
}
/// The directory locks one save holds: one exclusive lock on each physical
/// directory it publishes into, taken before the first file there is staged
/// and released when the save ends, after its last commit (OUTPUT-8).
#[derive(Default)]
pub struct Locks(Vec<(Identity, DirectoryLock)>);
impl Locks {
    fn hold(&mut self, parent: &Directory) -> Result<(), Error> {
        let directory = parent.identity()?;
        if !self.0.iter().any(|(held, _)| *held == directory) {
            let lock = DirectoryLock::acquire(parent)?;
            self.0.push((directory, lock));
        }
        Ok(())
    }
}
struct Pending {
    parent: Directory,
    parent_path: PathBuf,
    name: String,
    directory: Directory,
    identity: Identity,
    candidate: Option<File>,
    cleaned: bool,
}
impl Pending {
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
        with_cleanup(error, self.parent_path.join(&self.name), self.cleanup())
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_publication::AnchoredTarget;
    use std::sync::atomic::{AtomicU64, Ordering};
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
            Self(crate::canonical_path::canonicalize(path).unwrap())
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
        let declared = "a".repeat(65_537);
        Publisher::new(&declared, study.0.join("output.csv").to_str().unwrap())
            .unwrap()
            .publish(&declared, b"complete")
            .unwrap();
        assert_eq!(
            std::fs::read(study.0.join("output.csv")).unwrap(),
            b"complete"
        );
        std::fs::write(study.0.join("output.csv"), b"old").unwrap();
        for target in [
            "output.csv/",
            "output.csv/.",
            "output.csv/..",
            "output.csv:stream",
        ] {
            // PathBuf::join normalizes dot components under a verbatim Windows root.
            // Preserve the caller spelling so this actually probes refused file intent.
            let written = format!("{}\\{target}", study.0.display());
            assert!(Publisher::new("output.csv", &written).is_err(), "{written}");
        }
        assert_eq!(std::fs::read(study.0.join("output.csv")).unwrap(), b"old");
        assert_eq!(study.names(), ["output.csv"]);
    }
    #[test]
    fn explicit_and_anchored_publication_refuse_win32_names_before_staging() {
        let study = Study::new();
        std::fs::write(study.0.join("output.csv"), b"old").unwrap();
        for name in [
            "output.csv.",
            "output.csv ",
            "CON.csv",
            "nul",
            "com9.txt",
            "LPT².csv",
            "bad|name",
        ] {
            let written = format!("{}\\{name}", study.0.display());
            assert!(
                matches!(
                    Publisher::new("output.csv", &written),
                    Err(Error::InvalidTarget)
                ),
                "{name:?}"
            );
            let anchored = AnchoredTarget::new(
                Directory::open(&study.0).unwrap(),
                vec![name.into()],
                written,
            );
            assert!(
                matches!(anchored.publisher("output.csv"), Err(Error::InvalidTarget)),
                "{name:?}"
            );
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
            Err(Error::StagingCollision)
        ));
        assert!(study.names().is_empty());
    }
    #[test]
    fn physical_target_comparison_keeps_root_aliases_and_absent_case_aliases() {
        let study = Study::new();
        let selected = study.0.join("selected");
        let moved = study.0.join("moved");
        std::fs::create_dir(&selected).unwrap();
        let first = study.publisher("selected/output.csv");
        std::fs::rename(&selected, &moved).unwrap();
        let alias = study.publisher("moved/OUTPUT.csv");
        assert!(first.same_target(&alias).unwrap());
        let other = study.publisher("moved/other.csv");
        assert!(!first.same_target(&other).unwrap());
        std::fs::create_dir(&selected).unwrap();
        let replacement = study.publisher("selected/output.csv");
        assert!(!first.same_target(&replacement).unwrap());
        assert!(std::fs::read_dir(&selected).unwrap().next().is_none());
        assert!(std::fs::read_dir(&moved).unwrap().next().is_none());
        std::fs::write(moved.join("output.csv"), b"prior").unwrap();
        std::fs::hard_link(moved.join("output.csv"), moved.join("other.csv")).unwrap();
        assert!(!first.same_target(&other).unwrap());
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
        let result = publisher.publish_with_checkpoint("output.csv", b"checked", |temporary| {
            // Windows refuses renaming a directory while a descendant file is open.
            // Inject an operation failure after closing the candidate, then substitute
            // its staging name to exercise the remaining held-directory cleanup guard.
            temporary.candidate.take();
            let name = &temporary.name;
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
