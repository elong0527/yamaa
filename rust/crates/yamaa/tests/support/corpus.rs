//! The benchmark corpus as the partitioned families read it: the cases one
//! specification run reproduces, staged without what they state as truth.
use std::path::{Path, PathBuf};

pub fn benchmarks() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks")
}

/// The benchmarks one specification run reproduces: one `spec.yaml`, no
/// project file and no declared entries, in name order.
pub fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(benchmarks())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|case| case.join("benchmark.yaml").is_file() && case.join("spec.yaml").is_file())
        .filter(|case| {
            let metadata = std::fs::read_to_string(case.join("benchmark.yaml")).unwrap();
            let specifications = std::fs::read_dir(case)
                .unwrap()
                .filter(|entry| {
                    let name = entry.as_ref().unwrap().file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("spec") && name.ends_with(".yaml")
                })
                .count();
            specifications == 1
                && !case.join("project.yaml").exists()
                && !metadata.lines().any(|line| line.starts_with("entries:"))
        })
        .collect();
    cases.sort();
    cases
}

/// Copy the benchmark into `to` without what it states as truth, keeping
/// its symbolic links; false where this platform cannot make one.
pub fn stage(from: &Path, to: &Path) -> bool {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let target = to.join(&name);
        let kind = entry.file_type().unwrap();
        if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(entry.path()).unwrap(), &target).unwrap();
            #[cfg(not(unix))]
            return false;
        } else if kind.is_dir() {
            if name == "expected" {
                continue;
            }
            std::fs::create_dir_all(&target).unwrap();
            if !stage(&entry.path(), &target) {
                return false;
            }
        } else if name != "README.md" && name != "benchmark.yaml" {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
    true
}
