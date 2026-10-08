//! Atomic byte publication to one explicit caller-selected file target.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    fs::File,
    io::Write,
    os::fd::OwnedFd,
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
impl From<rustix::io::Errno> for Error {
    fn from(error: rustix::io::Errno) -> Self {
        Self::Io(error.into())
    }
}

pub struct Publisher {
    parent: OwnedFd,
    parent_path: PathBuf,
    name: String,
    declared: String,
}
impl Publisher {
    /// The caller names the absolute target; declarations grant no write authority.
    pub fn new(declared: &str, target: &str) -> Result<Self, Error> {
        if declared.len() > MAX_PATH_BYTES || target.len() > MAX_PATH_BYTES {
            return Err(Error::Limit);
        }
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
        let parent_path = std::fs::canonicalize(parent_path).map_err(|_| Error::InvalidTarget)?;
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
    fn check_target(&self) -> Result<(), Error> {
        match fs::statat(&self.parent, self.name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(status) if kind(&status) == FileType::RegularFile => Ok(()),
            Err(rustix::io::Errno::NOENT) => Ok(()),
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
        let (mut temporary, descriptor) = self.temporary()?;
        let publication = (|| {
            let mut file = File::from(descriptor);
            file.write_all(content)?;
            file.flush()?;
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
        match publication {
            Ok(()) => {
                // Replacement committed: cleanup must not report a failed save.
                let _ = temporary.cleanup();
                Ok(())
            }
            Err(error) => Err(temporary.failure(error)),
        }
    }
    fn temporary(&self) -> Result<(Pending<'_>, OwnedFd), Error> {
        self.temporary_with_numbers(|| NEXT.fetch_add(1, Ordering::Relaxed))
    }
    fn temporary_with_numbers(
        &self,
        mut next: impl FnMut() -> u64,
    ) -> Result<(Pending<'_>, OwnedFd), Error> {
        for _ in 0..64 {
            let number = next();
            let name = format!(".yamaa-output-{}-{number}.stage", std::process::id());
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
                let directory = fs::openat(
                    &self.parent,
                    name.as_str(),
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?;
                if identity(&fs::fstat(&directory)?) != identity(&initial) {
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
            let file = match fs::openat(
                &temporary.directory,
                "candidate.part",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
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
        Err(Error::Limit)
    }
}
struct Pending<'a> {
    parent: &'a OwnedFd,
    parent_path: &'a Path,
    name: String,
    directory: OwnedFd,
    identity: (i128, i128, FileType),
    candidate: Option<(i128, i128, FileType)>,
    cleaned: bool,
}
impl Pending<'_> {
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
    selected: Option<(i128, i128, FileType)>,
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
struct DirectoryLock<'a>(&'a OwnedFd);
impl<'a> DirectoryLock<'a> {
    fn acquire(parent: &'a OwnedFd) -> Result<Self, Error> {
        fs::flock(parent, fs::FlockOperation::NonBlockingLockExclusive)?;
        Ok(Self(parent))
    }
}
impl Drop for DirectoryLock<'_> {
    fn drop(&mut self) {
        let _ = fs::flock(self.0, fs::FlockOperation::Unlock);
    }
}
impl yamaa_engine::specification_output::ArtifactPort for Publisher {
    type Error = Error;
    fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Error> {
        Publisher::publish(self, path, content)
    }
}
fn kind(status: &fs::Stat) -> FileType {
    FileType::from_raw_mode(status.st_mode)
}
fn identity(status: &fs::Stat) -> (i128, i128, FileType) {
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
            Err(Error::Limit)
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
        let path = std::fs::canonicalize(path).unwrap();
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
        let path = std::fs::canonicalize(path).unwrap();
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
}
