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

/// Replace a scratch root in native paths and diagnostic paths, which use
/// forward slashes on every platform. Leave suffixes and other text intact.
pub fn replace_scratch_root(text: String, root: &str, replacement: &str) -> String {
    [root.to_owned(), root.replace('\\', "/")]
        .iter()
        .fold(text, |text, spelling| text.replace(spelling, replacement))
}

#[test]
fn replacing_a_scratch_root_preserves_suffixes_and_unrelated_text() {
    // The normalization seam must cover Windows spellings on every test host.
    for (label, root, text, expected) in [
        (
            "Windows native and diagnostic paths",
            r"C:\tmp\study",
            r"C:\tmp\study\project.yaml | C:/tmp/study/raw/missing.csv | C:\other\file | \n",
            r"<study>\project.yaml | <study>/raw/missing.csv | C:\other\file | \n",
        ),
        (
            "POSIX paths",
            "/tmp/study",
            r"/tmp/study/project.yaml | /tmp/study/raw/missing.csv | /other/file | \n",
            r"<study>/project.yaml | <study>/raw/missing.csv | /other/file | \n",
        ),
    ] {
        assert_eq!(
            replace_scratch_root(text.to_owned(), root, "<study>"),
            expected,
            "{label}"
        );
    }
}

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
