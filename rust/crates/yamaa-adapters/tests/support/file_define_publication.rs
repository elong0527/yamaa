use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
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
