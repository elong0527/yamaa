//! Unix publication through retained descriptors: the filesystem calls of
//! [`super`] on Unix.
use super::{next_number, staging_name, with_cleanup, Entry, Error, Publisher};
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    fs::File,
    io::Write,
    os::fd::OwnedFd,
    path::{Path, PathBuf},
};
use yamaa_core::resource::ResourceFailure;

/// The retained handle of a directory: its descriptor.
pub(super) type Handle = OwnedFd;
/// An entry's device, inode and file type.
pub(super) type Identity = (i128, i128, FileType);
/// The characters an anchored target's component may not hold.
pub(super) const SEPARATORS: [char; 2] = ['/', '\0'];

impl From<rustix::io::Errno> for Error {
    fn from(error: rustix::io::Errno) -> Self {
        Self::Io(error.into())
    }
}
/// Unix admits every final name the separators leave.
pub(super) fn admit_name(_name: &str) -> Result<(), Error> {
    Ok(())
}
/// The entry `name` beneath `parent`, without following a link.
pub(super) fn inspect(parent: &OwnedFd, name: &str) -> std::io::Result<(Entry, Identity)> {
    let status = fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)?;
    let entry = match kind(&status) {
        FileType::Directory => Entry::Directory,
        FileType::Symlink => Entry::Link,
        _ => Entry::Other,
    };
    Ok((entry, identity(&status)))
}
/// Open the directory `name` beneath `parent`, without following a link.
pub(super) fn directory(parent: &OwnedFd, name: &str) -> std::io::Result<OwnedFd> {
    Ok(fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
}
/// The identity of the directory `handle` holds.
pub(super) fn held_identity(handle: &OwnedFd) -> std::io::Result<Identity> {
    Ok(identity(&fs::fstat(handle)?))
}
impl Publisher {
    /// Compare entries through retained physical parents, including selected-root
    /// aliases. No target is opened or written. Unproven macOS Unicode name
    /// distinctions refuse the whole set before publication.
    pub fn same_target(&self, other: &Self) -> Result<bool, Error> {
        if held_identity(&self.parent)? != held_identity(&other.parent)? {
            return Ok(false);
        }
        #[cfg(target_os = "macos")]
        {
            // APFS also ignores Unicode normalization on case-sensitive volumes.
            // No bounded native Unicode comparer is present, so do not claim
            // distinct absent non-ASCII names in a shared physical directory.
            if !self.name.is_ascii() || !other.name.is_ascii() {
                return Err(Error::InvalidTarget);
            }
            if parent_case_sensitive(&self.parent)? {
                Ok(self.name == other.name)
            } else {
                Ok(self.name.eq_ignore_ascii_case(&other.name))
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(self.name == other.name)
        }
    }
    /// The caller names the absolute target; declarations grant no write authority.
    pub fn new(declared: &str, target: &str) -> Result<Self, Error> {
        if declared.is_empty() || declared.contains('\0') || target.contains('\0') {
            return Err(Error::InvalidTarget);
        }
        // Path components discard trailing slashes and dots; retain file intent.
        if matches!(target.rsplit('/').next(), None | Some("" | "." | "..")) {
            return Err(Error::InvalidTarget);
        }
        let target = Path::new(target);
        if !target.is_absolute() {
            return Err(Error::InvalidTarget);
        }
        let name = target
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Error::InvalidTarget)?
            .to_owned();
        let parent_path = target.parent().ok_or(Error::InvalidTarget)?;
        let parent_path =
            crate::canonical_path::canonicalize(parent_path).map_err(|_| Error::InvalidTarget)?;
        let initial = fs::lstat(&parent_path).map_err(|_| Error::InvalidTarget)?;
        let parent = fs::open(
            &parent_path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| Error::InvalidTarget)?;
        let opened = fs::fstat(&parent).map_err(|_| Error::InvalidTarget)?;
        if kind(&opened) != FileType::Directory || identity(&initial) != identity(&opened) {
            return Err(Error::InvalidTarget);
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
    /// Admit an absent target or a regular file, returning a present
    /// target's permission bits; a link or another entry is unreachable.
    pub(super) fn check_target(&self) -> Result<Option<Mode>, Error> {
        match fs::statat(&self.parent, self.name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(status) if kind(&status) == FileType::RegularFile => Ok(Some(
                Mode::from_raw_mode(status.st_mode) & (Mode::RWXU | Mode::RWXG | Mode::RWXO),
            )),
            Err(rustix::io::Errno::NOENT) => Ok(None),
            Ok(status) if kind(&status) == FileType::Symlink => {
                Err(Error::Unreachable(ResourceFailure::Symlink))
            }
            Ok(_) => Err(Error::Unreachable(ResourceFailure::NotRegularFile)),
            Err(error) => Err(error.into()),
        }
    }
    /// Open the staged file of `declared`: an empty temporary file in a
    /// private directory beside the target, which [`Writing::append`] fills
    /// piece by piece, such as one partition of an output after another
    /// (D264), and [`Writing::finish`] stages, without making it visible. The
    /// target's directory stays locked in `locks` until the save ends
    /// (OUTPUT-8). The file then has the mode it is published with: a present
    /// target's permission bits, or else 0666 less the umask (D92).
    pub fn open(&self, locks: &mut Locks, declared: &str) -> Result<Writing<'_>, Error> {
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        locks.hold(&self.parent)?;
        let replaced = self.check_target()?;
        let (temporary, descriptor) = self.temporary()?;
        Ok(Writing {
            temporary,
            file: File::from(descriptor),
            replaced,
            written: 0,
        })
    }
    /// Replace the target with the staged file in one step.
    pub fn commit(&self, staged: Staged) -> Result<(), Error> {
        self.commit_with_checkpoint(staged, |_| Ok(()))
    }
    fn commit_with_checkpoint(
        &self,
        staged: Staged,
        checkpoint: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut temporary = self.reopen(&staged)?;
        let replaced = (|| {
            let observed = fs::statat(
                &temporary.directory,
                "candidate.part",
                AtFlags::SYMLINK_NOFOLLOW,
            )?;
            if temporary.candidate != Some(identity(&observed)) {
                return Err(Error::Changed);
            }
            self.check_target()?;
            checkpoint(&temporary.name)?;
            fs::renameat(
                &temporary.directory,
                "candidate.part",
                &self.parent,
                self.name.as_str(),
            )?;
            Ok(())
        })();
        match replaced {
            Ok(()) => {
                // Replacement committed: cleanup must not report a failed save.
                let _ = temporary.cleanup();
                Ok(())
            }
            Err(error) => Err(temporary.failure(error)),
        }
    }
    /// Remove the staged file; the target stays as it was. A removal that
    /// fails names the staging location it left behind.
    pub fn abort(&self, staged: Staged) -> Result<(), (PathBuf, std::io::Error)> {
        let staging = self.parent_path.join(&staged.name);
        let mut temporary = self.reopen(&staged).map_err(|_| {
            (
                staging.clone(),
                std::io::Error::other("temporary directory identity unconfirmed or changed"),
            )
        })?;
        temporary.cleanup().map_err(|cleanup| (staging, cleanup))
    }
    /// The staging directory of `staged` again, through this publisher's held
    /// parent, confirmed to be the directory it staged into.
    fn reopen(&self, staged: &Staged) -> Result<Pending<'_>, Error> {
        let unconfirmed = || Error::Cleanup {
            operation: Box::new(Error::Changed),
            staging: self.parent_path.join(&staged.name),
            cleanup: std::io::Error::other("temporary directory identity unconfirmed or changed"),
        };
        let directory = directory(&self.parent, &staged.name).map_err(|_| unconfirmed())?;
        if held_identity(&directory)? != staged.directory {
            return Err(unconfirmed());
        }
        Ok(Pending {
            parent: &self.parent,
            parent_path: &self.parent_path,
            name: staged.name.clone(),
            directory,
            identity: staged.directory,
            candidate: Some(staged.candidate),
            cleaned: false,
        })
    }
    #[cfg(test)]
    fn publish_with_checkpoint(
        &mut self,
        declared: &str,
        content: &[u8],
        checkpoint: impl FnOnce(&str) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut locks = Locks::default();
        let staged = self.stage(&mut locks, declared, content)?;
        self.commit_with_checkpoint(staged, checkpoint)
    }
    fn temporary(&self) -> Result<(Pending<'_>, OwnedFd), Error> {
        self.temporary_with_numbers(next_number)
    }
    fn temporary_with_numbers(
        &self,
        mut next: impl FnMut() -> u64,
    ) -> Result<(Pending<'_>, OwnedFd), Error> {
        for _ in 0..64 {
            let name = staging_name(next());
            if name == self.name {
                continue;
            }
            match fs::mkdirat(
                &self.parent,
                name.as_str(),
                Mode::RUSR | Mode::WUSR | Mode::XUSR,
            ) {
                Ok(()) => (),
                Err(rustix::io::Errno::EXIST) => continue,
                Err(error) => return Err(error.into()),
            }
            let mut selected = None;
            let opened = (|| {
                let initial = fs::statat(&self.parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)?;
                if kind(&initial) != FileType::Directory
                    || initial.st_uid != rustix::process::geteuid().as_raw()
                {
                    return Err(Error::Changed);
                }
                selected = Some(identity(&initial));
                if initial.st_mode & 0o777 != 0o700 {
                    return Err(Error::Changed);
                }
                let directory = directory(&self.parent, &name)?;
                if held_identity(&directory)? != identity(&initial) {
                    return Err(Error::Changed);
                }
                Ok(Pending {
                    parent: &self.parent,
                    parent_path: &self.parent_path,
                    name: name.clone(),
                    directory,
                    identity: identity(&initial),
                    candidate: None,
                    cleaned: false,
                })
            })();
            let mut temporary = match opened {
                Ok(temporary) => temporary,
                Err(error) => {
                    return Err(with_cleanup(
                        error,
                        self.parent_path.join(&name),
                        cleanup_directory(&self.parent, &name, selected),
                    ));
                }
            };
            // The private directory, not the file's mode, keeps the candidate
            // from other users: the umask narrows 0666 as it does for any
            // file `open(2)` creates.
            let file = match fs::openat(
                &temporary.directory,
                "candidate.part",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::WGRP | Mode::ROTH | Mode::WOTH,
            ) {
                Ok(file) => file,
                Err(error) => return Err(temporary.failure(error.into())),
            };
            let status = match fs::fstat(&file) {
                Ok(status) => status,
                Err(error) => return Err(temporary.failure(error.into())),
            };
            temporary.candidate = Some(identity(&status));
            return Ok((temporary, file));
        }
        Err(Error::StagingCollision)
    }
}
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn parent_case_sensitive(parent: &OwnedFd) -> Result<bool, Error> {
    use std::{
        ffi::{c_int, c_long},
        os::fd::AsRawFd,
    };
    unsafe extern "C" {
        fn fpathconf(fd: c_int, name: c_int) -> c_long;
    }
    // _PC_CASE_SENSITIVE is 11 in the public macOS sys/unistd.h SDK.
    // SAFETY: fpathconf only queries this live owned directory descriptor.
    match unsafe { fpathconf(parent.as_raw_fd(), 11) } {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::InvalidTarget),
    }
}
/// One staged file: its complete bytes in a temporary file in a private
/// directory beside the target, not yet visible there. It holds no
/// descriptor, so a save may stage any number of files: the publisher that
/// staged it reopens its private directory to commit or abort it, and
/// confirms the directory is the one it staged into. A staged file neither
/// committed nor aborted stays behind.
pub struct Staged {
    name: String,
    directory: Identity,
    candidate: Identity,
}
/// A staged file being written: the temporary file [`Publisher::open`]
/// made, which nothing at the target shows. Dropping it before
/// [`Writing::finish`] removes it, as a failed or cancelled save does.
pub struct Writing<'a> {
    temporary: Pending<'a>,
    file: File,
    replaced: Option<Mode>,
    written: usize,
}
impl Writing<'_> {
    /// Append `bytes`, checking the saved-file bound before the file grows.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        yamaa_core::limits::Limits::output_extension(self.written, bytes.len())
            .map_err(Error::Limit)?;
        self.file.write_all(bytes)?;
        self.written += bytes.len();
        Ok(())
    }
    /// Flush and sync the complete file, with its published mode, and keep
    /// it for a later commit; a file that cannot be staged is removed.
    pub fn finish(self) -> Result<Staged, Error> {
        let Self {
            mut temporary,
            mut file,
            replaced,
            ..
        } = self;
        let written = (|| {
            file.flush()?;
            // Before the sync, so the sync covers the mode too.
            if let Some(mode) = replaced {
                fs::fchmod(&file, mode)?;
            }
            file.sync_all()?;
            let held = fs::fstat(&file)?;
            drop(file);
            let observed = fs::statat(
                &temporary.directory,
                "candidate.part",
                AtFlags::SYMLINK_NOFOLLOW,
            )?;
            if kind(&held) != FileType::RegularFile || identity(&held) != identity(&observed) {
                return Err(Error::Changed);
            }
            Ok(())
        })();
        match written {
            Ok(()) => Ok(temporary.keep()),
            Err(error) => Err(temporary.failure(error)),
        }
    }
    /// Stop writing for `error`: remove the file, keeping with the error
    /// what a removal that fails leaves behind.
    pub fn fail(mut self, error: Error) -> Error {
        self.temporary.failure(error)
    }
}
struct Pending<'a> {
    parent: &'a OwnedFd,
    parent_path: &'a Path,
    name: String,
    directory: OwnedFd,
    identity: Identity,
    candidate: Option<Identity>,
    cleaned: bool,
}
impl Pending<'_> {
    /// Keep the written candidate for a later commit, closing its directory.
    fn keep(mut self) -> Staged {
        self.cleaned = true;
        Staged {
            name: std::mem::take(&mut self.name),
            directory: self.identity,
            candidate: self.candidate.expect("a staged candidate"),
        }
    }
    fn cleanup(&mut self) -> std::io::Result<()> {
        if self.cleaned {
            return Ok(());
        }
        match fs::statat(&self.directory, "candidate.part", AtFlags::SYMLINK_NOFOLLOW) {
            Ok(current) if Some(identity(&current)) == self.candidate => {
                fs::unlinkat(&self.directory, "candidate.part", AtFlags::empty())?;
            }
            Err(rustix::io::Errno::NOENT) => (),
            Ok(_) => {
                return Err(std::io::Error::other(
                    "temporary candidate identity changed",
                ))
            }
            Err(error) => return Err(error.into()),
        }
        cleanup_directory(self.parent, &self.name, Some(self.identity))?;
        self.cleaned = true;
        Ok(())
    }
    fn failure(&mut self, error: Error) -> Error {
        with_cleanup(error, self.parent_path.join(&self.name), self.cleanup())
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        // Fallible operation paths clean explicitly; Drop also covers unwinding.
        let _ = self.cleanup();
    }
}
fn cleanup_directory(
    parent: &OwnedFd,
    name: &str,
    selected: Option<Identity>,
) -> std::io::Result<()> {
    match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(current) if Some(identity(&current)) == selected => {
            fs::unlinkat(parent, name, AtFlags::REMOVEDIR)?;
            Ok(())
        }
        Err(rustix::io::Errno::NOENT) => Err(std::io::Error::other(
            "temporary directory entry disappeared before cleanup",
        )),
        Ok(_) => Err(std::io::Error::other(
            "temporary directory identity unconfirmed or changed",
        )),
        Err(error) => Err(error.into()),
    }
}
/// The directory locks one save holds: one exclusive lock on each physical
/// directory it publishes into, taken before the first file there is staged
/// and released when the save ends, after its last commit (OUTPUT-8).
#[derive(Default)]
pub struct Locks(Vec<DirectoryLock>);
impl Locks {
    fn hold(&mut self, parent: &OwnedFd) -> Result<(), Error> {
        let directory = held_identity(parent)?;
        if !self.0.iter().any(|lock| lock.directory == directory) {
            self.0.push(DirectoryLock::acquire(parent, directory)?);
        }
        Ok(())
    }
}
struct DirectoryLock {
    directory: Identity,
    /// A duplicate of the publisher's descriptor: the lock belongs to the
    /// open directory description they share, and outlives the publisher.
    held: OwnedFd,
}
impl DirectoryLock {
    fn acquire(parent: &OwnedFd, directory: Identity) -> Result<Self, Error> {
        let held = parent.try_clone()?;
        fs::flock(&held, fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self { directory, held })
    }
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = fs::flock(&self.held, fs::FlockOperation::Unlock);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_authored_declarations_publish_complete_bytes_without_a_path_copy_quota() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-long-declaration-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let target = path.join("result.csv");
        std::fs::write(&target, b"old").unwrap();
        let declared = "a".repeat(65_537);
        let mut publisher = Publisher::new(&declared, target.to_str().unwrap()).unwrap();
        publisher.publish(&declared, b"complete").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"complete");
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn the_target_name_is_never_used_as_a_staging_directory() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-collision-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let target = path.join(format!(".yamaa-output-{}-0.stage", std::process::id()));
        let publisher = Publisher::new("result.csv", target.to_str().unwrap()).unwrap();
        assert!(matches!(
            publisher.temporary_with_numbers(|| 0),
            Err(Error::StagingCollision)
        ));
        assert!(!target.exists());
        let mut number = 0;
        let (temporary, file) = publisher
            .temporary_with_numbers(|| {
                let selected = number;
                number += 1;
                selected
            })
            .unwrap();
        let untouched = !target.exists();
        drop(file);
        drop(temporary);
        std::fs::remove_dir_all(path).unwrap();
        assert!(untouched, "staging must never create the target directory");
    }
    #[test]
    fn cleanup_without_confirmed_ownership_preserves_the_entry() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-unconfirmed-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let publisher =
            Publisher::new("result.csv", path.join("result.csv").to_str().unwrap()).unwrap();
        std::fs::create_dir(path.join("foreign")).unwrap();
        assert!(cleanup_directory(&publisher.parent, "foreign", None).is_err());
        assert!(path.join("foreign").is_dir());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn failed_cleanup_preserves_the_operation_error_and_foreign_entries() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-failed-swap-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let path = crate::canonical_path::canonicalize(path).unwrap();
        let target = path.join("result.csv");
        std::fs::write(&target, b"old").unwrap();
        let mut publisher = Publisher::new("result.csv", target.to_str().unwrap()).unwrap();
        let mut staging = PathBuf::new();
        let failure = publisher
            .publish_with_checkpoint("result.csv", b"checked", |name| {
                staging = path.join(name);
                std::fs::rename(&staging, path.join("moved")).unwrap();
                std::fs::create_dir(&staging).unwrap();
                std::fs::write(staging.join("foreign.part"), b"foreign").unwrap();
                Err(Error::Changed)
            })
            .unwrap_err();
        assert!(
            matches!(&failure, Error::Cleanup { operation, staging: reported, .. }
            if matches!(**operation, Error::Changed) && *reported == staging),
            "{failure:?}"
        );
        assert!(failure
            .message()
            .starts_with("publication target changed; temporary cleanup failed"));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert_eq!(
            std::fs::read(staging.join("foreign.part")).unwrap(),
            b"foreign"
        );
        assert!(!path.join("moved/candidate.part").exists());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn cleanup_permission_failure_is_reported_before_commit() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-cleanup-permission-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let path = crate::canonical_path::canonicalize(path).unwrap();
        let target = path.join("result.csv");
        std::fs::write(&target, b"old").unwrap();
        let mut publisher = Publisher::new("result.csv", target.to_str().unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).unwrap();
        let enforced = std::fs::write(path.join("permission-probe"), b"").is_err();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        if !enforced {
            std::fs::remove_dir_all(path).unwrap();
            eprintln!("cleanup permission probe unavailable for privileged publishing user");
            return;
        }
        let mut staging = PathBuf::new();
        let failure = publisher.publish_with_checkpoint("result.csv", b"checked", |name| {
            staging = path.join(name);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).unwrap();
            Err(Error::Changed)
        });
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let failure = failure.unwrap_err();
        assert!(
            matches!(&failure, Error::Cleanup { operation, staging: reported, cleanup }
            if matches!(**operation, Error::Changed) && *reported == staging
                && cleanup.kind() == std::io::ErrorKind::PermissionDenied),
            "{failure:?}"
        );
        assert!(failure.message().contains(&staging.display().to_string()));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        assert!(staging.is_dir());
        assert!(!staging.join("candidate.part").exists());
        std::fs::remove_dir_all(path).unwrap();
    }
    /// A save may stage any number of files: a staged file holds no
    /// descriptor until it is committed, so only the publishers' parents and
    /// the save's directory lock stay open.
    #[cfg(target_os = "linux")]
    #[test]
    fn staged_files_hold_no_descriptor_until_they_are_committed() {
        let path = std::env::temp_dir().join(format!(
            "yamaa-publication-many-staged-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        let path = crate::canonical_path::canonicalize(path).unwrap();
        // Descriptors this process holds beneath the directory, as staging
        // directories and candidates would be.
        let beneath = || {
            std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
                .filter(|target| target.starts_with(&path) && target != &path)
                .count()
        };
        let names = (0..64)
            .map(|index| format!("f{index}.csv"))
            .collect::<Vec<_>>();
        let publishers = names
            .iter()
            .map(|name| Publisher::new(name, path.join(name).to_str().unwrap()).unwrap())
            .collect::<Vec<_>>();
        let mut locks = Locks::default();
        let staged = publishers
            .iter()
            .zip(&names)
            .map(|(publisher, name)| publisher.stage(&mut locks, name, name.as_bytes()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(beneath(), 0);
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 64);
        for (publisher, staged) in publishers.iter().zip(staged) {
            publisher.commit(staged).unwrap();
        }
        drop(locks);
        for name in &names {
            assert_eq!(std::fs::read(path.join(name)).unwrap(), name.as_bytes());
        }
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 64);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn replacing_the_parent_temporary_entry_does_not_replace_checked_bytes() {
        let path =
            std::env::temp_dir().join(format!("yamaa-publication-race-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let target = path.join("result.csv");
        std::fs::write(&target, b"old").unwrap();
        let mut publisher = Publisher::new("result.csv", target.to_str().unwrap()).unwrap();
        publisher
            .publish_with_checkpoint("result.csv", b"checked", |name| {
                let entry = path.join(name);
                if entry.is_dir() {
                    use std::os::unix::fs::MetadataExt;
                    let metadata = std::fs::metadata(&entry).unwrap();
                    assert_eq!(metadata.mode() & 0o777, 0o700);
                    assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
                    std::fs::rename(&entry, path.join("moved")).unwrap();
                    std::fs::create_dir(&entry).unwrap();
                    std::fs::write(entry.join("candidate.part"), b"replacement").unwrap();
                } else {
                    std::fs::remove_file(&entry).unwrap();
                    std::fs::write(&entry, b"replacement").unwrap();
                }
                Ok(())
            })
            .unwrap();
        let actual = std::fs::read(target).unwrap();
        std::fs::remove_dir_all(path).unwrap();
        assert_eq!(actual, b"checked");
    }

    /// The umasks the mode tests run under, each with the mode a new file
    /// takes under it.
    const UMASKS: [(u32, u32); 2] = [(0o022, 0o644), (0o077, 0o600)];
    const UMASK_CHILD: &str = "YAMAA_PUBLICATION_TEST_UMASK";
    /// The umask belongs to the whole process, so a mode test runs again in
    /// one child process per umask, which runs that test alone and sets the
    /// umask first. In such a child this sets the umask and returns it with
    /// the mode a new file takes; elsewhere it runs the children and returns
    /// `None`.
    fn under_each_umask(test: &str) -> Option<(u32, u32)> {
        if let Ok(umask) = std::env::var(UMASK_CHILD) {
            let umask = u32::from_str_radix(&umask, 8).unwrap();
            let _ = rustix::process::umask(Mode::from_raw_mode(umask as _));
            let (_, new_file) = UMASKS
                .into_iter()
                .find(|&(each, _)| each == umask)
                .expect("a listed umask");
            return Some((umask, new_file));
        }
        for (umask, _) in UMASKS {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test.split_once("::").unwrap().1])
                .arg("--test-threads=1")
                .env(UMASK_CHILD, format!("{umask:03o}"))
                .output()
                .unwrap();
            assert!(output.status.success(), "umask {umask:03o}: {output:?}");
            // The child ran the test rather than matching nothing.
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("1 passed"), "umask {umask:03o}: {stdout}");
        }
        None
    }
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }
    fn sandbox(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("yamaa-publication-{label}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        crate::canonical_path::canonicalize(path).unwrap()
    }
    #[test]
    fn a_new_file_takes_0666_less_the_umask() {
        let Some((umask, new_file)) = under_each_umask(concat!(
            module_path!(),
            "::a_new_file_takes_0666_less_the_umask"
        )) else {
            return;
        };
        let path = sandbox("new-file-mode");
        let target = path.join("result.csv");
        let mut publisher = Publisher::new("result.csv", target.to_str().unwrap()).unwrap();
        publisher.publish("result.csv", b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(mode(&target), new_file, "umask {umask:03o}");
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn a_replaced_file_keeps_its_permission_bits() {
        use std::os::unix::fs::PermissionsExt;
        let Some((umask, _)) = under_each_umask(concat!(
            module_path!(),
            "::a_replaced_file_keeps_its_permission_bits"
        )) else {
            return;
        };
        let path = sandbox("replaced-file-mode");
        // Neither umask gives a new file either mode. The permission bits are
        // kept; set-user-ID is not.
        for (name, before, after) in [("shared.csv", 0o640, 0o640), ("setuid.csv", 0o4750, 0o750)] {
            let target = path.join(name);
            std::fs::write(&target, b"old").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(before)).unwrap();
            assert_eq!(mode(&target), before, "{name}");
            let mut publisher = Publisher::new(name, target.to_str().unwrap()).unwrap();
            publisher.publish(name, b"new").unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"new");
            assert_eq!(mode(&target), after, "{name} under umask {umask:03o}");
        }
        std::fs::remove_dir_all(path).unwrap();
    }
    /// The candidate has its published mode before the rename makes it
    /// visible, and the directory that holds it stays 0700.
    #[test]
    fn the_staging_directory_stays_private_under_any_umask() {
        use std::os::unix::fs::PermissionsExt;
        let Some((umask, new_file)) = under_each_umask(concat!(
            module_path!(),
            "::the_staging_directory_stays_private_under_any_umask"
        )) else {
            return;
        };
        let path = sandbox("staging-mode");
        let replaced = path.join("replaced.csv");
        std::fs::write(&replaced, b"old").unwrap();
        std::fs::set_permissions(&replaced, std::fs::Permissions::from_mode(0o640)).unwrap();
        for (name, published) in [("new.csv", new_file), ("replaced.csv", 0o640)] {
            let target = path.join(name);
            let mut publisher = Publisher::new(name, target.to_str().unwrap()).unwrap();
            let mut staged = None;
            publisher
                .publish_with_checkpoint(name, b"new", |staging| {
                    let staging = path.join(staging);
                    // A set-group-ID parent passes that bit to the directory.
                    let directory = mode(&staging) & 0o777;
                    staged = Some((directory, mode(&staging.join("candidate.part"))));
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                staged,
                Some((0o700, published)),
                "{name} under umask {umask:03o}"
            );
            assert_eq!(mode(&target), published, "{name} under umask {umask:03o}");
        }
        // Each staging directory went with its commit.
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 2);
        std::fs::remove_dir_all(path).unwrap();
    }
}
