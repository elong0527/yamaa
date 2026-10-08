#![cfg(any(unix, windows))]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
#[cfg(unix)]
use yamaa_adapters::file_resources::Error;
use yamaa_adapters::{
    file_application, file_configuration, file_preparation::FileSpecification,
    specification_report::Identity,
};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n";
struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-file-application-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn text(&self, path: &str) -> String {
        self.0.join(path).to_str().unwrap().into()
    }
    fn build(&self) -> file_application::Domain {
        let path = self.text("spec.yaml");
        file_application::domain(
            Request {
                specification: &path,
                environment: None,
            },
            identity(&path),
        )
        .unwrap()
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn identity(specification: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "test",
        specification,
        base_directory: ".",
    }
}
#[test]
fn environment_and_unsupported_admission_precede_entry_and_study_reads() {
    let study = Study::new();
    let path = study.text("absent.yaml");
    let issues = file_application::check(
        Request {
            specification: &path,
            environment: Some("absent-environment.yaml"),
        },
        identity(&path),
    );
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].condition, "unsupported_operation");
    assert_eq!(issues[0].context, "{\"operation\":\"environment\"}");
    assert_eq!(issues[0].spec_paths, ["environment"]);
    assert_eq!(issues[0].requirement, None);
    fs::write(
        study.0.join("spec.yaml"),
        VALID.replace(
            "input: {SRC: input.csv}",
            "input: {SRC: {path: input.csv, ordinal: RECNO}}",
        ),
    )
    .unwrap();
    let path = study.text("spec.yaml");
    let issues = file_application::domain(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    )
    .err()
    .unwrap();
    assert!(!issues.is_empty());
    assert!(issues
        .iter()
        .all(|i| i.condition == "unsupported_operation"));
    assert!(!study.0.join("input.csv").exists());
}
#[test]
fn static_check_does_not_need_study_data_and_failed_build_cannot_save() {
    let study = Study::new();
    fs::write(study.0.join("spec.yaml"), VALID).unwrap();
    let path = study.text("spec.yaml");
    assert!(file_application::check(
        Request {
            specification: &path,
            environment: None
        },
        identity(&path)
    )
    .is_empty());
    let mut result = study.build();
    assert!(result.output().is_none());
    assert!(!result.issues().is_empty());
    assert!(result.save().is_err());
    assert_eq!(fs::read_dir(&study.0).unwrap().count(), 1);
}
#[test]
fn saved_bytes_use_retained_values_and_save_failure_can_be_retried() {
    let study = Study::new();
    fs::write(
        study.0.join("spec.yaml"),
        VALID.replace("path: output.csv", "path: absent/output.csv"),
    )
    .unwrap();
    fs::write(
        study.0.join("input.csv"),
        b"ID\n-9223372036854775808\n9223372036854775807\n",
    )
    .unwrap();
    let mut result = study.build();
    let before = result.output().unwrap().to_vec();
    let observations = result.observations();
    assert_eq!(observations["artifacts"], serde_json::json!([]));
    assert!(result.issues().is_empty());
    fs::write(study.0.join("input.csv"), b"changed").unwrap();
    fs::write(study.0.join("spec.yaml"), b"[changed").unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(result.observations(), observations);
    assert_eq!(result.issues().len(), 1);
    assert_eq!(
        result.issues()[0].context,
        "{\"code\":\"invalid_target\",\"stage\":\"output\"}"
    );
    assert_eq!(result.output().unwrap(), before);
    fs::create_dir(study.0.join("absent")).unwrap();
    for _ in 0..2 {
        assert!(result.save().unwrap());
        let published = result.observations();
        assert_eq!(
            published["artifacts"][0]["content"],
            "ID\n-9223372036854775808\n9223372036854775807\n"
        );
        let mut build = published;
        build["artifacts"] = serde_json::json!([]);
        assert_eq!(build, observations);
        assert!(result.issues().is_empty());
        assert_eq!(
            fs::read(study.0.join("absent/output.csv")).unwrap(),
            b"ID\n-9223372036854775808\n9223372036854775807\n"
        );
    }
    assert_eq!(fs::read_dir(study.0.join("absent")).unwrap().count(), 1);
    fs::remove_file(study.0.join("absent/output.csv")).unwrap();
    fs::remove_dir(study.0.join("absent")).unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(result.observations(), observations);
}
#[test]
fn project_configuration_keeps_current_root_fallback_and_captured_config() {
    let study = Study::new();
    fs::create_dir(study.0.join("entry")).unwrap();
    fs::create_dir(study.0.join("extra")).unwrap();
    fs::write(
        study.0.join("yamaa-project.yaml"),
        "version: '1.0'\ndata_roots: [extra]\n",
    )
    .unwrap();
    fs::write(study.0.join("entry/spec.yaml"), VALID).unwrap();
    fs::write(study.0.join("extra/input.csv"), b"ID\n1\n").unwrap();
    let (resources, entry) = file_configuration::resources(&study.text("entry/spec.yaml")).unwrap();
    assert_eq!(resources.capture_reads(), 1);
    let mut prepared = FileSpecification::prepare(resources, &entry).unwrap();
    assert_eq!(prepared.capture_reads(), 2);
    fs::write(study.0.join("yamaa-project.yaml"), b"[changed").unwrap();
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 3);
    assert!(prepared.build().result.is_ok());
    assert_eq!(prepared.capture_reads(), 3);
}
#[cfg(unix)]
#[test]
fn entry_link_and_outside_read_remain_refused() {
    let study = Study::new();
    fs::create_dir(study.0.join("entry")).unwrap();
    fs::write(study.0.join("outside.csv"), b"ID\n1\n").unwrap();
    fs::write(
        study.0.join("entry/spec.yaml"),
        VALID.replace("input.csv", "../outside.csv"),
    )
    .unwrap();
    let path = study.text("entry/spec.yaml");
    let issues = file_application::domain(
        Request {
            specification: &path,
            environment: None,
        },
        identity(&path),
    )
    .err()
    .unwrap();
    assert_eq!(
        issues[0].context,
        "{\"code\":\"resource_outside_roots\",\"stage\":\"capture\"}"
    );
    std::os::unix::fs::symlink("spec.yaml", study.0.join("entry/link.yaml")).unwrap();
    let (resources, entry) = file_configuration::resources(&study.text("entry/link.yaml")).unwrap();
    assert!(matches!(
        FileSpecification::prepare(resources, &entry),
        Err(yamaa_adapters::file_preparation::Error::Resource(
            Error::Symlink
        ))
    ));
}
#[test]
fn invalid_project_configuration_is_rejected_before_entry_capture() {
    let study = Study::new();
    for config in [
        "version: '99.0'\n",
        "version: '1.0'\nunknown: true\n",
        "version: '1.0'\ndata_roots: [null]\n",
    ] {
        fs::write(study.0.join("yamaa-project.yaml"), config).unwrap();
        assert!(matches!(
            file_configuration::resources(&study.text("absent.yaml")),
            Err(file_configuration::Error::Configuration)
        ));
    }
}

#[cfg(unix)]
#[test]
fn publication_refuses_linked_parents_for_existing_and_absent_targets() {
    for existing in [false, true] {
        let study = Study::new();
        let outside = Study::new();
        fs::write(
            study.0.join("spec.yaml"),
            VALID.replace("output.csv", "link/output.csv"),
        )
        .unwrap();
        fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
        if existing {
            fs::write(outside.0.join("output.csv"), b"retained").unwrap();
        }
        std::os::unix::fs::symlink(&outside.0, study.0.join("link")).unwrap();
        let mut result = study.build();
        assert!(result.output().is_some());
        assert!(!result.save().unwrap());
        assert_eq!(
            result.issues()[0].context,
            "{\"code\":\"invalid_target\",\"stage\":\"output\"}"
        );
        if existing {
            assert_eq!(fs::read(outside.0.join("output.csv")).unwrap(), b"retained");
        }
        assert_eq!(
            fs::read_dir(&outside.0).unwrap().count(),
            usize::from(existing)
        );
        fs::remove_file(study.0.join("link")).unwrap();
        fs::create_dir(study.0.join("link")).unwrap();
        assert!(result.save().unwrap());
        assert!(result.issues().is_empty());
        assert_eq!(
            fs::read(study.0.join("link/output.csv")).unwrap(),
            b"ID\n1\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn publication_refuses_parent_link_substitution_after_build() {
    let study = Study::new();
    let outside = Study::new();
    fs::create_dir(study.0.join("parent")).unwrap();
    fs::write(
        study.0.join("spec.yaml"),
        VALID.replace("output.csv", "parent/output.csv"),
    )
    .unwrap();
    fs::write(study.0.join("input.csv"), b"ID\n1\n").unwrap();
    let mut result = study.build();
    fs::rename(study.0.join("parent"), study.0.join("held")).unwrap();
    std::os::unix::fs::symlink(&outside.0, study.0.join("parent")).unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
    assert_eq!(fs::read_dir(study.0.join("held")).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn publication_retains_the_selected_physical_root_after_name_substitution() {
    let study = Study::new();
    let outside = Study::new();
    let root = study.0.join("selected");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("spec.yaml"), VALID).unwrap();
    fs::write(root.join("input.csv"), b"ID\n1\n").unwrap();
    let path = root.join("spec.yaml");
    let path = path.to_str().unwrap();
    let mut result = file_application::domain(
        Request {
            specification: path,
            environment: None,
        },
        identity(path),
    )
    .unwrap();
    fs::rename(&root, study.0.join("retained")).unwrap();
    std::os::unix::fs::symlink(&outside.0, &root).unwrap();
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.0.join("retained/output.csv")).unwrap(),
        b"ID\n1\n"
    );
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
}
