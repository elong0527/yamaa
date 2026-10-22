//! The helpers the command line's test families share: the binary run in a
//! directory, and a scratch directory of a test's own.
//!
//! Every family shares one test process (#3556), so a scratch directory's
//! name carries the process, a counter shared by every scratch directory and
//! the caller's label. Removal ignores errors: a `Drop` that panicked while a
//! failing test unwinds would abort the whole binary.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Run the binary with `arguments` in `directory`.
pub fn yamaa_in(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yamaa"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the yamaa binary starts")
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory of a test's own, canonical and without a Windows
/// verbatim prefix, so its files are spelled as the engine spells the files
/// it reads and writes (D38). It is removed when the value drops.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let canonical = std::fs::canonicalize(&directory).unwrap();
        let text = canonical.to_str().unwrap();
        Self(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text)))
    }

    /// Write `contents` to `name`, creating its parent directories.
    pub fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// The directory, or a file in it, as the engine spells it: absolute,
    /// with forward slashes.
    pub fn spelled(&self, name: &str) -> String {
        let directory = self.0.to_str().unwrap().replace('\\', "/");
        if name.is_empty() {
            directory
        } else {
            format!("{directory}/{name}")
        }
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
