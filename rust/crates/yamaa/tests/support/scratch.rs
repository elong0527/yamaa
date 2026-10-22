//! Owned temporary directories for facade tests; each value cleans up on drop.
//!
//! Every family shares one test process (#3556), so a name carries the
//! process, a counter shared by every scratch directory and the caller's label.
//! Removal ignores errors: a `Drop` that panicked while a failing test unwinds
//! would abort the whole binary.
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(label: &str) -> Self {
        Self::under(&std::env::temp_dir(), label)
    }

    pub fn under(parent: &Path, label: &str) -> Self {
        let path = parent.join(format!(
            ".yamaa-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
    }

    /// Write `bytes` to `name` inside the directory, creating its parent
    /// directories, and return its path.
    pub fn write(&self, name: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub fn path(&self) -> &Path {
        &self.0
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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
