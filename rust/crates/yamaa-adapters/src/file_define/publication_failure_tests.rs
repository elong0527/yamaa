use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[test]
fn actual_changed_define_resource_keeps_authored_cause_path_and_snapshot_in_both_hosts() {
    let directory = std::env::temp_dir().join(format!(
        "yamaa-define-resource-change-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    fs::write(directory.join("dm.csv"), b"ID\n1\n").unwrap();
    let text = file_resources::caller_path(&directory).unwrap();
    let mut resources = Resources::new(&text, &text, &[]).unwrap();
    let original = resources.capture("dm.csv", 16).unwrap().0;
    fs::write(directory.join("dm.csv"), b"ID\n2\n").unwrap();
    let error = resources.capture("dm.csv", 16).unwrap_err();
    assert_eq!(error, file_resources::Error::Changed);
    for host in [Language::Python, Language::R] {
        let error = resources.capture("dm.csv", 16).unwrap_err();
        let findings = issues(&resource("datasets.DM.artifact", "dm.csv", error), host);
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.phase, "ingest");
        assert_eq!(finding.condition, "resource_path_content_changed");
        assert_eq!(finding.requirement.as_deref(), Some("REQ-0788"));
        assert_eq!(finding.spec_paths, ["datasets.DM.artifact"]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&finding.context).unwrap(),
            serde_json::json!({"path":"dm.csv"})
        );
    }
    assert_eq!(resources.capture_reads(), 1);
    assert_eq!(original.as_ref(), b"ID\n1\n");
    drop(resources);
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn changed_define_metadata_uses_original_root_spec_parent_and_dictionary_occurrences() {
    const SPEC: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: SRC.ID}]\n";
    for changed in ["environment.yaml", "spec.yaml", "base.yaml", "dict.yaml"] {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-define-metadata-change-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        fs::write(
            directory.join("environment.yaml"),
            b"schema_version: '1.0'\nlanguage: python\n",
        )
        .unwrap();
        let spec = match changed {
            "base.yaml" => {
                fs::write(directory.join("base.yaml"), SPEC).unwrap();
                "schema_version: '1.0'\nparents: [base.yaml]\n".to_owned()
            }
            "dict.yaml" => {
                fs::write(directory.join("dict.yaml"), b"A: answer\n").unwrap();
                SPEC.replace(
                    "derivation: SRC.ID",
                    "derivation: {mapping: {source: SRC.ID, dict: dict.yaml}}",
                )
            }
            _ => SPEC.to_owned(),
        };
        fs::write(directory.join("spec.yaml"), &spec).unwrap();
        let environment_written =
            file_resources::caller_path(&directory.join("environment.yaml")).unwrap();
        let mut ports = NativePorts {
            resources: None,
            host: Language::Python,
            environment: None,
            environment_written: String::new(),
            specifications: Vec::new(),
        };
        let environment = ports
            .capture_environment(&environment_written, service::METADATA_LIMITS)
            .unwrap();
        let specification = ports
            .capture_specification_at(
                "sdtm.specs[0]",
                &environment.root().source().identity,
                "spec.yaml",
            )
            .unwrap();
        let original_environment = environment.root().source().bytes.clone();
        let reads = ports.resources().capture_reads();
        fs::write(directory.join(changed), b"invalid replacement").unwrap();
        let failure = ports
            .verify_metadata("sdtm", &environment.root().source().identity)
            .unwrap_err();
        assert!(matches!(
            &failure,
            Failure::ClassifiedResource {
                error: file_resources::Error::Changed,
                ..
            }
        ));
        let (field, written) = match changed {
            "environment.yaml" => ("environment", environment_written.as_str()),
            "spec.yaml" => ("sdtm.specs[0]", "spec.yaml"),
            "base.yaml" => ("parents[0]", "base.yaml"),
            "dict.yaml" => ("columns.ID.derivation.mapping.dict", "dict.yaml"),
            _ => unreachable!(),
        };
        for host in [Language::Python, Language::R] {
            let findings = issues(&failure, host);
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].phase, "ingest");
            assert_eq!(findings[0].condition, "resource_path_content_changed");
            assert_eq!(findings[0].requirement.as_deref(), Some("REQ-0788"));
            assert_eq!(findings[0].spec_paths, [field]);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&findings[0].context).unwrap(),
                serde_json::json!({"path":written})
            );
        }
        assert_eq!(ports.resources().capture_reads(), reads);
        assert_eq!(specification.source().bytes, spec.as_bytes());
        assert_eq!(environment.root().source().bytes, original_environment);
        assert!(!directory.join("never-read.csv").exists());
        assert!(!directory.join("never-written.csv").exists());
        drop(ports);
        fs::remove_dir_all(directory).unwrap();
    }
}
#[test]
fn later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder() {
    let directory = std::env::temp_dir().join(format!(
        "yamaa-define-failure-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    let environment = fs::read_to_string(original.join("environment.yaml"))
        .unwrap()
        .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml");
    let spec = fs::read_to_string(original.join("spec.yaml")).unwrap();
    fs::write(directory.join("environment.yaml"), environment).unwrap();
    fs::write(directory.join("spec.yaml"), &spec).unwrap();
    fs::write(
        directory.join("second.yaml"),
        spec.replace("domain: DM", "domain: DM2")
            .replace("path: dm.csv", "path: second.csv"),
    )
    .unwrap();
    for name in ["dm.csv", "second.csv"] {
        fs::copy(original.join("expected/dm.csv"), directory.join(name)).unwrap();
    }
    fs::write(directory.join("define.xml"), b"prior document").unwrap();
    let mut result = Define::prepare(
        directory.join("environment.yaml").to_str().unwrap(),
        "sdtm",
        Some("2026-09-11T00:00:00"),
        "0.3.0",
        Language::Python,
    );
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let held = result.artifacts().next().unwrap().1.to_vec();
    let reads = result.capture_reads();
    assert!(result
        .save_with_checkpoint(|index| {
            if index == 0 {
                fs::create_dir(directory.join("second.json")).unwrap();
            }
        })
        .is_err());
    assert_eq!(fs::read(directory.join("dm.json")).unwrap(), held);
    assert_eq!(
        fs::read(directory.join("define.xml")).unwrap(),
        b"prior document"
    );
    assert_eq!(
        result
            .artifacts()
            .map(|(_, _, state)| state)
            .collect::<Vec<_>>(),
        [
            PublicationState::Published,
            PublicationState::Failed,
            PublicationState::Stopped
        ]
    );
    let Some(Failure::Publication { index, error }) = result.failure() else {
        panic!("lost original publication failure")
    };
    assert_eq!(*index, 1);
    assert!(matches!(error, file_publication::Error::InvalidTarget));
    assert_eq!(result.issues()[0].phase, "output");
    assert_eq!(result.issues()[0].condition, "publication_failed");
    assert_eq!(result.issues()[0].requirement.as_deref(), Some("REQ-0764"));
    assert_eq!(result.issues()[0].spec_paths, ["define"]);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.issues()[0].context).unwrap(),
        serde_json::json!({"artifact":1,"cause":error.message()})
    );
    assert_eq!(result.capture_reads(), reads);
    assert!(result.save().is_err());
    assert_eq!(
        fs::read_dir(&directory)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".yamaa-"))
            .count(),
        0
    );
    fs::remove_dir_all(directory).unwrap();
}
