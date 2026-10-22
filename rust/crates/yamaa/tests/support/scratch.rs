//! Owned temporary directories for facade tests; each value cleans up on drop.
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

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
