#![cfg(unix)]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::file_publication::{Error, Publisher};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-native-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn target(&self) -> PathBuf {
        self.0.join("output.csv")
    }
    fn publisher(&self) -> Publisher {
        Publisher::new("declared.csv", self.target().to_str().unwrap()).unwrap()
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn explicit_saves_replace_complete_bytes_and_leave_no_temporary_files() {
    let directory = Directory::new();
    fs::write(directory.target(), b"old").unwrap();
    fs::hard_link(directory.target(), directory.0.join("old.csv")).unwrap();
    let mut publisher = directory.publisher();
    for content in [
        b"complete\n".as_slice(),
        b"".as_slice(),
        b"UTF-8: \xc3\xa9\n".as_slice(),
    ] {
        publisher.publish("declared.csv", content).unwrap();
        assert_eq!(fs::read(directory.target()).unwrap(), content);
        assert_eq!(fs::read(directory.0.join("old.csv")).unwrap(), b"old");
        assert_eq!(directory.names(), vec!["old.csv", "output.csv"]);
    }
}
#[test]
fn paths_and_capacity_are_checked_before_any_target_change() {
    let directory = Directory::new();
    fs::write(directory.target(), b"old").unwrap();
    let mut publisher = directory.publisher();
    assert!(matches!(
        publisher.publish("different.csv", b"new"),
        Err(Error::PathMismatch)
    ));
    assert!(matches!(
        publisher.publish("declared.csv", &vec![0; 67_108_865]),
        Err(Error::Limit)
    ));
    assert_eq!(fs::read(directory.target()).unwrap(), b"old");
    assert_eq!(directory.names(), vec!["output.csv"]);
    assert!(matches!(
        Publisher::new("declared.csv", "relative.csv"),
        Err(Error::InvalidTarget)
    ));
    assert!(matches!(
        Publisher::new("declared.csv", directory.0.to_str().unwrap()),
        Err(Error::InvalidTarget)
    ));
}
#[test]
fn directory_suffixes_are_not_normalized_into_file_targets() {
    let directory = Directory::new();
    fs::write(directory.target(), b"old").unwrap();
    for name in ["new.csv", "output.csv"] {
        for suffix in ["/", "/.", "/.."] {
            let target = format!("{}{suffix}", directory.0.join(name).display());
            assert!(
                matches!(
                    Publisher::new("declared.csv", &target),
                    Err(Error::InvalidTarget)
                ),
                "directory spelling was accepted: {target}"
            );
            assert_eq!(fs::read(directory.target()).unwrap(), b"old");
            assert_eq!(directory.names(), vec!["output.csv"]);
        }
    }
}
#[test]
fn a_new_link_or_directory_is_refused_without_following_or_replacing_it() {
    let directory = Directory::new();
    let mut publisher = directory.publisher();
    let other = directory.0.join("other");
    fs::write(&other, b"retained").unwrap();
    std::os::unix::fs::symlink(&other, directory.target()).unwrap();
    assert!(matches!(
        publisher.publish("declared.csv", b"new"),
        Err(Error::InvalidTarget)
    ));
    assert_eq!(fs::read(&other).unwrap(), b"retained");
    assert!(fs::symlink_metadata(directory.target())
        .unwrap()
        .file_type()
        .is_symlink());
    fs::remove_file(directory.target()).unwrap();
    fs::create_dir(directory.target()).unwrap();
    assert!(matches!(
        publisher.publish("declared.csv", b"new"),
        Err(Error::InvalidTarget)
    ));
    assert_eq!(directory.names().len(), 2);
}
#[test]
fn selected_parent_descriptor_retains_authority_after_renaming() {
    let directory = Directory::new();
    let parent = directory.0.join("selected");
    fs::create_dir(&parent).unwrap();
    let mut publisher =
        Publisher::new("declared.csv", parent.join("output.csv").to_str().unwrap()).unwrap();
    let moved = directory.0.join("moved");
    fs::rename(&parent, &moved).unwrap();
    let other = directory.0.join("other");
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, &parent).unwrap();
    publisher.publish("declared.csv", b"selected").unwrap();
    assert_eq!(fs::read(moved.join("output.csv")).unwrap(), b"selected");
    assert!(!other.join("output.csv").exists());
}
#[test]
fn permission_failure_preserves_the_prior_artifact_and_allows_explicit_retry() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Directory::new();
    fs::write(directory.target(), b"old").unwrap();
    let mut publisher = directory.publisher();
    let permissions = fs::metadata(&directory.0).unwrap().permissions();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o500)).unwrap();
    let probe = directory.0.join("dac-probe");
    if let Ok(file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        drop(file);
        fs::set_permissions(&directory.0, permissions).unwrap();
        fs::remove_file(probe).unwrap();
        eprintln!("process bypasses DAC; publication permission case not exercised");
        return;
    }
    let result = publisher.publish("declared.csv", b"new");
    fs::set_permissions(&directory.0, permissions).unwrap();
    assert!(matches!(result, Err(Error::Io(_))));
    assert_eq!(fs::read(directory.target()).unwrap(), b"old");
    assert_eq!(directory.names(), vec!["output.csv"]);
    publisher.publish("declared.csv", b"new").unwrap();
    assert_eq!(fs::read(directory.target()).unwrap(), b"new");
}
