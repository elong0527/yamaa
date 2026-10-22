//! Scratch directories for tests that write files.
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

/// A fresh directory under the system temporary directory, removed on drop.
///
/// Every family of the crate shares one test process (#3556), so the name
/// carries the process, a counter shared by every scratch directory, and the
/// caller's label. Removal ignores errors: a `Drop` that panicked while a
/// failing test unwinds would abort the whole binary.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(label: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "yamaa-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    /// A scratch directory named by its canonical path, as the engine spells
    /// the paths it reports: no symbolic link and no Windows verbatim prefix.
    pub fn canonical(label: &str) -> Self {
        Self::new(label).resolved(yamaa_adapters::canonical_path::canonicalize)
    }

    /// Take ownership of an existing directory, such as one inside another
    /// scratch directory, and remove it on drop.
    pub fn adopt(path: PathBuf) -> Self {
        Self(path)
    }

    /// The same directory, named through `resolve`, such as
    /// `std::fs::canonicalize`.
    pub fn resolved(mut self, resolve: impl FnOnce(PathBuf) -> std::io::Result<PathBuf>) -> Self {
        self.0 = resolve(std::mem::take(&mut self.0)).unwrap();
        self
    }

    /// Write `bytes` to `name` inside the directory, creating its parent
    /// directories, and return its path.
    pub fn write(&self, name: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The identity the engine gives the file `name` in the directory, as a
    /// finding's `source` and `entry` name it: the directory canonical, with
    /// forward slashes and no Windows verbatim prefix, derived by the engine's
    /// own `directory_identity`. It differs from the path as created where the
    /// temporary directory's spelling is not canonical: macOS's `/var` alias
    /// of `/private/var`, or a Windows 8.3 short name such as `RUNNER~1`.
    pub fn identity(&self, name: &str) -> String {
        let directory = yamaa_adapters::file_configuration::directory_identity(&self.0).unwrap();
        format!("{directory}{name}")
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
