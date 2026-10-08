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
        if declared != self.declared {
            return Err(Error::PathMismatch);
        }
        if content.len() > MAX_OUTPUT_BYTES {
            return Err(Error::Limit);
        }
        self.check_target()?;
        let (name, descriptor) = self.temporary()?;
        let mut temporary = Pending {
            parent: &self.parent,
            name: Some(name),
        };
        let mut file = File::from(descriptor);
        file.write_all(content)?;
        file.flush()?;
        file.sync_all()?;
        let held = fs::fstat(&file)?;
        let name = temporary.name.as_deref().ok_or(Error::Changed)?;
        let observed = fs::statat(&self.parent, name, AtFlags::SYMLINK_NOFOLLOW)?;
        if kind(&held) != FileType::RegularFile || identity(&held) != identity(&observed) {
            return Err(Error::Changed);
        }
        self.check_target()?;
        fs::renameat(&self.parent, name, &self.parent, self.name.as_str())?;
        temporary.name = None;
        Ok(())
    }
    fn temporary(&self) -> Result<(String, OwnedFd), Error> {
        for _ in 0..64 {
            let number = NEXT.fetch_add(1, Ordering::Relaxed);
            let name = format!(".yamaa-output-{}-{number}.part", std::process::id());
            match fs::openat(
                &self.parent,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(file) => return Ok((name, file)),
                Err(rustix::io::Errno::EXIST) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(Error::Limit)
    }
}
struct Pending<'a> {
    parent: &'a OwnedFd,
    name: Option<String>,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if let Some(name) = &self.name {
            let _ = fs::unlinkat(self.parent, name.as_str(), AtFlags::empty());
        }
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
