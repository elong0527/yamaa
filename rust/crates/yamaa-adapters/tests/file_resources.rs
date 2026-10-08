#![cfg(unix)]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::file_resources::{Error, Resources};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-native-file-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for child in ["project/spec", "project/data", "additional", "outside"] {
            fs::create_dir_all(path.join(child)).unwrap();
        }
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn text(&self, name: &str) -> String {
        self.path(name).to_str().unwrap().into()
    }
    fn resources(&self) -> Resources {
        Resources::new(
            &self.text("project"),
            &self.text("project/spec"),
            &[self.text("additional")],
        )
        .unwrap()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn metadata_and_bounded_capture_reuse_only_exact_retained_bytes() {
    let study = Study::new();
    fs::write(study.path("project/data/source.csv"), b"ID,V\n1,2\n").unwrap();
    let mut resources = study.resources();
    resources.inspect("../data/source.csv").unwrap();
    assert_eq!(resources.capture_reads(), 0);
    assert_eq!(
        resources.capture("../data/source.csv", 8),
        Err(Error::Limit)
    );
    assert_eq!(resources.capture_reads(), 0);
    let (first, created) = resources.capture("../data/source.csv", 9).unwrap();
    assert!(created);
    assert_eq!(first.as_ref(), b"ID,V\n1,2\n");
    let (again, created) = resources.capture("../data/source.csv", 9).unwrap();
    assert!(!created && std::sync::Arc::ptr_eq(&first, &again));
    assert_eq!(resources.capture_reads(), 1);
    assert_eq!(
        resources.capture("../data/source.csv", 8),
        Err(Error::Limit)
    );
    fs::write(study.path("project/data/source.csv"), b"ID,V\n1,3\n").unwrap();
    assert_eq!(
        resources.capture("../data/source.csv", 9),
        Err(Error::Changed)
    );
    assert_eq!(first.as_ref(), b"ID,V\n1,2\n");
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn cached_growth_and_shrink_are_changes_and_refusal_does_not_create_a_snapshot() {
    let study = Study::new();
    let name = study.path("project/spec/a");
    fs::write(&name, b"123").unwrap();
    let mut resources = study.resources();
    assert_eq!(resources.capture("a", 2), Err(Error::Limit));
    fs::write(&name, b"12").unwrap();
    assert!(resources.capture("a", 2).unwrap().1);
    fs::write(&name, b"123").unwrap();
    assert_eq!(resources.capture("a", 100), Err(Error::Changed));
    fs::write(&name, b"1").unwrap();
    assert_eq!(resources.capture("a", 100), Err(Error::Changed));
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn empty_file_obeys_zero_ceiling_and_later_attempts_share_the_capture() {
    let study = Study::new();
    fs::write(study.path("project/spec/empty"), b"").unwrap();
    let mut resources = study.resources();
    assert!(resources.capture("empty", 0).unwrap().1);
    assert!(!resources.capture("empty", 0).unwrap().1);
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn relative_fallback_order_is_base_then_project_then_declared_data_roots() {
    let study = Study::new();
    fs::write(study.path("project/spec/a"), b"base").unwrap();
    fs::write(study.path("project/a"), b"project").unwrap();
    fs::write(study.path("additional/a"), b"additional").unwrap();
    fs::write(study.path("project/b"), b"project").unwrap();
    fs::write(study.path("additional/b"), b"additional").unwrap();
    fs::write(study.path("additional/c"), b"additional").unwrap();
    let mut resources = study.resources();
    assert_eq!(resources.capture("a", 32).unwrap().0.as_ref(), b"base");
    assert_eq!(resources.capture("b", 32).unwrap().0.as_ref(), b"project");
    assert_eq!(
        resources.capture("c", 32).unwrap().0.as_ref(),
        b"additional"
    );
    fs::create_dir(study.path("project/spec/d")).unwrap();
    fs::write(study.path("project/d"), b"later").unwrap();
    assert_eq!(resources.capture("d", 32), Err(Error::NotRegularFile));
}

#[test]
fn links_are_terminal_and_paths_cannot_escape_the_selected_roots() {
    let study = Study::new();
    fs::write(study.path("outside/secret"), b"outside").unwrap();
    fs::write(study.path("project/link"), b"later").unwrap();
    std::os::unix::fs::symlink(
        study.path("outside/secret"),
        study.path("project/spec/link"),
    )
    .unwrap();
    std::os::unix::fs::symlink(study.path("outside"), study.path("project/spec/directory"))
        .unwrap();
    let mut resources = study.resources();
    assert_eq!(resources.inspect("link"), Err(Error::Symlink));
    assert_eq!(resources.capture("link", 32), Err(Error::Symlink));
    assert_eq!(
        resources.capture("directory/secret", 32),
        Err(Error::Symlink)
    );
    assert_eq!(
        resources.capture("../../outside/secret", 32),
        Err(Error::OutsideRoots)
    );
    assert_eq!(
        resources.capture(&study.text("outside/secret"), 32),
        Err(Error::OutsideRoots)
    );
    for malformed in [
        "",
        "a//b",
        "a\\b",
        "https://example.org/a",
        "/tmp/../a",
        "a:é",
        "a\0b",
    ] {
        assert_eq!(
            resources.inspect(malformed),
            Err(Error::InvalidPath),
            "{malformed:?}"
        );
    }
    assert_eq!(resources.capture_reads(), 0);
}

#[test]
fn hard_link_aliases_share_a_snapshot_and_every_accepted_alias_is_verified() {
    let study = Study::new();
    let first = study.path("project/spec/a");
    let second = study.path("additional/b");
    fs::write(&first, b"original").unwrap();
    fs::hard_link(&first, &second).unwrap();
    let mut resources = study.resources();
    let (a, _) = resources.capture("a", 16).unwrap();
    let (b, created) = resources.capture("b", 16).unwrap();
    assert!(!created && std::sync::Arc::ptr_eq(&a, &b));
    fs::remove_file(&first).unwrap();
    fs::write(&first, b"changed!").unwrap();
    assert_eq!(resources.capture("b", 16), Err(Error::Changed));
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn selected_root_descriptor_survives_renaming_without_following_a_replacement_link() {
    let study = Study::new();
    fs::write(study.path("project/spec/a"), b"accepted").unwrap();
    fs::write(study.path("outside/a"), b"outside").unwrap();
    let mut resources = study.resources();
    fs::rename(study.path("project"), study.path("moved")).unwrap();
    std::os::unix::fs::symlink(study.path("outside"), study.path("project")).unwrap();
    assert_eq!(resources.capture("a", 16).unwrap().0.as_ref(), b"accepted");
    assert_eq!(
        resources
            .capture(&study.text("project/spec/a"), 16)
            .unwrap()
            .0
            .as_ref(),
        b"accepted"
    );
}

#[test]
fn an_outside_declaring_directory_can_reach_an_approved_root_or_fall_back() {
    let study = Study::new();
    fs::write(study.path("project/a"), b"approved").unwrap();
    fs::write(study.path("outside/a"), b"outside").unwrap();
    let mut resources =
        Resources::new(&study.text("project"), &study.text("outside"), &[]).unwrap();
    assert_eq!(
        resources.capture("../project/a", 16).unwrap().0.as_ref(),
        b"approved"
    );
    assert_eq!(resources.capture("a", 16).unwrap().0.as_ref(), b"approved");
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn authored_absolute_root_aliases_and_nested_roots_keep_their_selected_authority() {
    let study = Study::new();
    fs::write(study.path("project/spec/a"), b"selected").unwrap();
    std::os::unix::fs::symlink(study.path("project"), study.path("alias")).unwrap();
    let mut resources = Resources::new(
        &format!("{}/./", study.text("alias")),
        &study.text("project/spec"),
        &[study.text("project/spec")],
    )
    .unwrap();
    assert_eq!(
        resources
            .capture(&study.text("alias/spec/a"), 16)
            .unwrap()
            .0
            .as_ref(),
        b"selected"
    );
    assert_eq!(resources.capture("a", 16).unwrap().0.as_ref(), b"selected");
    assert_eq!(resources.capture_reads(), 1);
    assert_eq!(
        resources.inspect(Path::new(".").to_str().unwrap()),
        Err(Error::NotRegularFile)
    );
}

#[test]
fn permission_failures_are_terminal_before_any_later_fallback_is_read() {
    use std::os::unix::fs::PermissionsExt;
    let study = Study::new();
    let earlier = study.path("project/spec/blocked");
    fs::write(&earlier, b"earlier").unwrap();
    fs::write(study.path("project/blocked"), b"later").unwrap();
    let mut resources = study.resources();
    let permissions = fs::metadata(&earlier).unwrap().permissions();
    fs::set_permissions(&earlier, fs::Permissions::from_mode(0o0)).unwrap();
    let inspected = resources.inspect("blocked");
    let captured = resources.capture("blocked", 32);
    fs::set_permissions(&earlier, permissions).unwrap();
    assert_eq!(inspected, Err(Error::Missing));
    assert_eq!(captured, Err(Error::Missing));
    assert_eq!(resources.capture_reads(), 0);
    assert_eq!(
        resources.capture("blocked", 32).unwrap().0.as_ref(),
        b"earlier"
    );
}

#[test]
fn unsearchable_directory_does_not_fall_back_to_a_different_file() {
    use std::os::unix::fs::PermissionsExt;
    let study = Study::new();
    let earlier = study.path("project/spec/private");
    fs::create_dir(&earlier).unwrap();
    fs::write(earlier.join("blocked"), b"earlier").unwrap();
    fs::create_dir(study.path("project/private")).unwrap();
    fs::write(study.path("project/private/blocked"), b"later").unwrap();
    let mut resources = study.resources();
    let permissions = fs::metadata(&earlier).unwrap().permissions();
    fs::set_permissions(&earlier, fs::Permissions::from_mode(0o400)).unwrap();
    let inspected = resources.inspect("private/blocked");
    let captured = resources.capture("private/blocked", 32);
    fs::set_permissions(&earlier, permissions).unwrap();
    assert_eq!(inspected, Err(Error::Missing));
    assert_eq!(captured, Err(Error::Missing));
    assert_eq!(resources.capture_reads(), 0);
}
