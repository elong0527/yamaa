#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{
    file_configuration,
    file_preparation::FileSpecification,
    file_resources::{Error, Resources},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-local-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for name in ["entry", "shared", "outputs"] {
            fs::create_dir(path.join(name)).unwrap();
        }
        Self(fs::canonicalize(path).unwrap())
    }
    fn text(&self, name: &str) -> String {
        self.0
            .join(name)
            .to_str()
            .unwrap()
            .replace('\\', "/")
            .trim_start_matches("//?/")
            .to_owned()
    }
    fn resources(&self) -> Resources {
        Resources::for_entry(&self.text("entry")).unwrap()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn public_uri_and_symlink_sources_keep_their_normative_cause_and_authored_path() {
    use yamaa_adapters::{file_application, specification_report::Identity};
    use yamaa_engine::domain_entry::Request;
    let study = Study::new();
    fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("../shared/data.csv", study.0.join("entry/link.csv")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file("../shared/data.csv", study.0.join("entry/link.csv"))
        .unwrap();
    for (written, condition, requirement) in [
        (
            "https://example.invalid/source.csv",
            "resource_path_uri_scheme",
            "REQ-0775",
        ),
        ("link.csv", "resource_path_symlink", "REQ-0783"),
    ] {
        let spec = format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: '{written}'}}\ncolumns: [{{name: ID, type: int, derivation: SRC.ID}}]\noutput: {{path: output.csv, columns: [ID]}}\n");
        fs::write(study.0.join("entry/spec.yaml"), spec).unwrap();
        let path = study.text("entry/spec.yaml");
        let issues = file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: "test",
                example: "test",
                specification: &path,
                base_directory: ".",
            },
        )
        .map_or_else(
            |issues| issues,
            |mut result| {
                assert!(result.output().is_none());
                assert!(result.save().is_err());
                result.issues()
            },
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].phase, "validation");
        assert_eq!(issues[0].condition, condition);
        assert_eq!(issues[0].requirement.as_deref(), Some(requirement));
        assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&issues[0].context).unwrap(),
            serde_json::json!({"dataset":"SRC","path":written})
        );
        assert!(!study.0.join("entry/output.csv").exists());
    }
}

#[test]
fn inherited_parent_keeps_its_own_input_and_output_locations_without_configuration() {
    let study = Study::new();
    let parent = "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\noutput: {path: ../outputs/out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let child = "schema_version: '1.0'\nparents: ../shared/base.yaml\ndomain: CHILD\n";
    fs::write(study.0.join("entry/child.yaml"), child).unwrap();
    fs::write(study.0.join("shared/base.yaml"), parent).unwrap();
    fs::write(study.0.join("entry/data.csv"), b"ID\n999\n").unwrap();
    fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
    fs::write(
        study.0.join("yamaa-project.yaml"),
        b"[invalid configuration",
    )
    .unwrap();
    let (resources, entry) =
        file_configuration::resources(&study.text("entry/child.yaml")).unwrap();
    assert_eq!(resources.capture_reads(), 0);
    let mut prepared = FileSpecification::prepare(resources, &entry).unwrap();
    let origin = prepared.run().document().written_output_origin().unwrap();
    assert_eq!(origin.declaring_source, study.text("shared/base.yaml"));
    assert_eq!(origin.written, "../outputs/out.csv");
    assert_eq!(prepared.run().source().path, "../shared/data.csv");
    assert_eq!(
        prepared.run().compiled().output_path(),
        "../outputs/out.csv"
    );
    assert_eq!(prepared.capture_reads(), 2);
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 3);
    assert!(!study.0.join("outputs/out.csv").exists());
}

#[test]
fn parent_and_absolute_dot_aliases_share_bytes_and_detect_later_changes() {
    let study = Study::new();
    fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
    fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
    let mut resources = study.resources();
    let declaring = resources.resolve("root.yaml").unwrap();
    let relative = "../shared/data.csv";
    let absolute = study.text("shared/../shared/./data.csv");
    let (first, fresh) = resources.capture_from(&declaring, relative, 32).unwrap();
    assert!(fresh);
    let (second, fresh) = resources.capture_from(&declaring, &absolute, 32).unwrap();
    assert!(!fresh);
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(resources.capture_reads(), 1);
    assert_eq!(
        resources.resolve_from(&declaring, relative).unwrap(),
        study.text("shared/data.csv")
    );
    fs::write(study.0.join("shared/data.csv"), b"ID\n8\n").unwrap();
    assert_eq!(
        resources.capture_from(&declaring, relative, 32).err(),
        Some(Error::Changed)
    );
    assert_eq!(first.as_ref(), b"ID\n7\n");
    assert_eq!(resources.capture_reads(), 1);
}

#[test]
fn absent_artifact_location_has_no_fallback_and_nonregular_input_is_terminal() {
    let study = Study::new();
    fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
    fs::write(study.0.join("data.csv"), b"unrelated parent file").unwrap();
    let resources = study.resources();
    let declaring = resources.resolve("root.yaml").unwrap();
    assert_eq!(
        resources.inspect_from(&declaring, "data.csv"),
        Err(Error::Missing)
    );
    assert_eq!(
        resources
            .location_from(&declaring, "../outputs/held.csv")
            .unwrap(),
        study.text("outputs/held.csv")
    );
    assert!(!study.0.join("outputs/held.csv").exists());
    assert_eq!(
        resources.inspect_from(&declaring, "../shared"),
        Err(Error::NotRegularFile)
    );
    assert_eq!(resources.capture_reads(), 0);
}

#[test]
fn outside_parent_and_file_links_remain_terminal() {
    let study = Study::new();
    fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
    fs::write(study.0.join("shared/data.csv"), b"ID\n7\n").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("shared", study.0.join("linked-directory")).unwrap();
        std::os::unix::fs::symlink("shared/data.csv", study.0.join("linked-file.csv")).unwrap();
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir("shared", study.0.join("linked-directory")).unwrap();
        std::os::windows::fs::symlink_file("shared/data.csv", study.0.join("linked-file.csv"))
            .unwrap();
    }
    let resources = study.resources();
    let declaring = resources.resolve("root.yaml").unwrap();
    for written in ["../linked-directory/data.csv", "../linked-file.csv"] {
        assert_eq!(
            resources.inspect_from(&declaring, written),
            Err(Error::Symlink)
        );
    }
    assert_eq!(resources.capture_reads(), 0);
}

#[test]
fn local_path_admission_still_rejects_uri_backslash_and_empty_segments() {
    let study = Study::new();
    fs::write(study.0.join("entry/root.yaml"), b"declaring file").unwrap();
    let resources = study.resources();
    let declaring = resources.resolve("root.yaml").unwrap();
    for written in [
        "",
        "file:/data.csv",
        "https://example.org/data.csv",
        "../shared\\data.csv",
        "../shared//data.csv",
        "../shared/data.csv/",
    ] {
        assert_eq!(
            resources.inspect_from(&declaring, written),
            Err(Error::InvalidPath),
            "{written}"
        );
    }
    assert_eq!(resources.capture_reads(), 0);
}
