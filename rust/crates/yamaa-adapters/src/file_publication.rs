//! Atomic byte publication to one explicit caller-selected file target.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
use std::{
    fs::File,
    io::Write,
    os::fd::OwnedFd,
    path::Path,
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
}
impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidTarget => "invalid explicit publication target",
            Self::PathMismatch => "publication path does not match explicit target",
            Self::Limit => "publication byte or path limit",
            Self::Changed => "publication target changed",
            Self::Io(_) => "artifact publication failed",
        }
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
        self.publish_with_checkpoint(declared, content, |_| {})
    }
    fn publish_with_checkpoint(
        &mut self,
        declared: &str,
        content: &[u8],
        checkpoint: impl FnOnce(&str),
    ) -> Result<(), Error> {
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        if content.len() > MAX_OUTPUT_BYTES {
            return Err(Error::Limit);
        }
        let _lock = DirectoryLock::acquire(&self.parent)?;
        self.check_target()?;
        let (temporary, descriptor) = self.temporary()?;
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
        checkpoint(&temporary.name);
        fs::renameat(
            &temporary.directory,
            "candidate.part",
            &self.parent,
            self.name.as_str(),
        )?;
        Ok(())
    }
    fn temporary(&self) -> Result<(Pending<'_>, OwnedFd), Error> {
        for _ in 0..64 {
            let number = NEXT.fetch_add(1, Ordering::Relaxed);
            let name = format!(".yamaa-output-{}-{number}.stage", std::process::id());
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
                    name: name.clone(),
                    directory,
                    identity: identity(&initial),
                })
            })();
            let temporary = match opened {
                Ok(temporary) => temporary,
                Err(error) => {
                    if selected.is_some_and(|selected| {
                        fs::statat(&self.parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                            .is_ok_and(|current| identity(&current) == selected)
                    }) {
                        let _ = fs::unlinkat(&self.parent, name.as_str(), AtFlags::REMOVEDIR);
                    }
                    return Err(error);
                }
            };
            let file = fs::openat(
                &temporary.directory,
                "candidate.part",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?;
            return Ok((temporary, file));
        }
        Err(Error::Limit)
    }
}
struct Pending<'a> {
    parent: &'a OwnedFd,
    name: String,
    directory: OwnedFd,
    identity: (i128, i128, FileType),
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let _ = fs::unlinkat(&self.directory, "candidate.part", AtFlags::empty());
        if fs::statat(self.parent, self.name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .is_ok_and(|current| identity(&current) == self.identity)
        {
            let _ = fs::unlinkat(self.parent, self.name.as_str(), AtFlags::REMOVEDIR);
        }
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
            })
            .unwrap();
        let actual = std::fs::read(target).unwrap();
        std::fs::remove_dir_all(path).unwrap();
        assert_eq!(actual, b"checked");
    }
}
