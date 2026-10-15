//! An empty child equals its parent (REQ-0627): for these negative
//! benchmarks the child's first issue must match the parent's in phase,
//! condition and requirement, even though the layered path resolves column
//! dependencies before final validation runs (#2292).
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() == "expected" {
                continue;
            }
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The first issue of a build as `phase | condition | requirement`.
fn first_issue(spec: &Path) -> String {
    let path = spec.to_str().unwrap();
    let identity = Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "empty-child-condition",
        specification: path,
        base_directory: ".",
    };
    let failure = match file_application::domain(
        Request {
            specification: path,
            environment: None,
        },
        identity,
    ) {
        Ok(domain) => domain.issues(),
        Err(failure) => failure,
    };
    let issue = failure.first().expect("benchmark must fail");
    format!(
        "{} | {} | {:?}",
        issue.phase, issue.condition, issue.requirement
    )
}

fn assert_empty_child_matches_parent(benchmark: &str, expected_first: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    let dir = std::env::temp_dir().join(format!(
        "yamaa-empty-child-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let case = dir.join("case");
    copy_dir(&root.join(benchmark), &case);
    fs::write(
        case.join("__child.yaml"),
        "schema_version: \"1.0\"\nparents: spec.yaml\n",
    )
    .unwrap();
    let parent = first_issue(&case.join("spec.yaml"));
    let child = first_issue(&case.join("__child.yaml"));
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(parent, expected_first, "{benchmark} parent first issue");
    assert_eq!(child, parent, "{benchmark} empty child first issue");
}

#[test]
fn empty_child_of_negative_odm_context_reports_invalid_odm_context() {
    assert_empty_child_matches_parent(
        "negative-odm-context",
        "validation | invalid_odm_context | Some(\"REQ-1277\")",
    );
}

#[test]
fn empty_child_of_negative_review_unknown_date_reports_unknown_field() {
    assert_empty_child_matches_parent(
        "negative-review-unknown-date",
        "validation | unknown_field | Some(\"REQ-0189\")",
    );
}

#[test]
fn empty_child_of_negative_lookup_mismatched_reports_unknown_field() {
    assert_empty_child_matches_parent(
        "negative-lookup-mismatched",
        "validation | unknown_field | Some(\"REQ-0117\")",
    );
}
