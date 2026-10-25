use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[test]
fn target_admission_keeps_typed_publication_fault_and_original_error_in_both_hosts() {
    use file_publication::Error;

    for host in [Language::Python, Language::R] {
        for (error, code) in [
            (Error::InvalidTarget, "invalid_target"),
            (Error::Changed, "target_changed"),
            (
                Error::Host(crate::host_resources::Failure::Cancelled),
                "publication_cancelled",
            ),
            (
                Error::Host(crate::host_resources::Failure::Cleanup),
                "cleanup_incomplete",
            ),
            (
                Error::Io(std::io::Error::other("invalid explicit publication target")),
                "publication_failed",
            ),
        ] {
            let cause = error.message();
            let mut result = Define::prepare("", Submission::Sdtm, None, "0.4.0", Some(host), None);
            result.refuse(Failure::TargetAdmission {
                index: usize::MAX,
                error,
            });
            assert_eq!(result.issues().len(), 1);
            let finding = &result.issues()[0];
            assert_eq!(finding.phase(), "engine");
            assert_eq!(finding.condition(), "engine_rejected");
            assert_eq!(finding.rule(), None);
            assert_eq!(finding.spec_paths, ["define"]);
            assert_eq!(
                yamaa_core::json::Value::Object(finding.context.clone()),
                crate::json!({
                    "step": "publication_target_admission",
                    "code": code,
                    "artifact": usize::MAX,
                    "cause": cause,
                })
            );
            let Some(Failure::TargetAdmission { index, error }) = result.failure() else {
                panic!("lost original target admission error")
            };
            assert_eq!(*index, usize::MAX);
            match error {
                Error::InvalidTarget => assert_eq!(code, "invalid_target"),
                Error::Changed => assert_eq!(code, "target_changed"),
                Error::Host(crate::host_resources::Failure::Cancelled) => {
                    assert_eq!(code, "publication_cancelled");
                }
                Error::Host(crate::host_resources::Failure::Cleanup) => {
                    assert_eq!(code, "cleanup_incomplete");
                }
                Error::Io(error) => {
                    assert_eq!(code, "publication_failed");
                    assert_eq!(error.to_string(), "invalid explicit publication target");
                }
                other => panic!("original target admission error was replaced: {other:?}"),
            }
        }
    }
}

#[test]
fn public_define_issue_prefix_keeps_complete_raw_metadata_and_diagnostics_in_both_hosts() {
    use yamaa_core::{application_issue, limits::Limits};
    for host in [Language::Python, Language::R] {
        for length in [Limits::FINDINGS, Limits::FINDINGS + 1] {
            let diagnostics = (0..length)
                .map(|index| {
                    let mut finding = application_issue::rejected(
                        application_issue::Step::Define,
                        application_issue::Fault::Internal,
                    );
                    finding.spec_paths = vec![format!("datasets.D{index}.artifact")];
                    finding
                })
                .collect::<Vec<_>>();
            let metadata = diagnostics
                .iter()
                .map(|finding| Finding {
                    code: finding.code,
                    path: finding.spec_paths[0].clone(),
                    context: finding.context.clone(),
                })
                .collect();
            for failure in [
                Failure::Metadata(metadata),
                Failure::Diagnostic(diagnostics),
            ] {
                let mut result =
                    Define::prepare("", Submission::Sdtm, None, "0.4.0", Some(host), None);
                result.refuse(failure);
                assert_eq!(result.issues().len(), Limits::FINDINGS);
                for (index, issue) in result.issues().iter().enumerate() {
                    assert_eq!(issue.spec_paths, [format!("datasets.D{index}.artifact")]);
                }
                let held = match result.failure().unwrap() {
                    Failure::Metadata(findings) => findings.len(),
                    Failure::Diagnostic(findings) => findings.len(),
                    _ => panic!("original metadata or diagnostic owner must remain held"),
                };
                assert_eq!(held, length);
            }
        }
    }
}

#[test]
fn actual_changed_define_resource_keeps_authored_cause_path_and_snapshot() {
    let directory = std::env::temp_dir().join(format!(
        "yamaa-define-resource-change-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir(&directory).unwrap();
    let directory = crate::canonical_path::canonicalize(&directory).unwrap();
    fs::write(directory.join("dm.csv"), b"ID\n1\n").unwrap();
    let text = crate::file_resolver::caller_path(&directory).unwrap();
    let mut resources = Resources::new(&text, &text, &[]).unwrap();
    let original = resources.capture("dm.csv").unwrap().0;
    fs::write(directory.join("dm.csv"), b"ID\n2\n").unwrap();
    let error = resources.capture("dm.csv").unwrap_err();
    assert_eq!(error, file_resources::Error::Changed);
    let error = resources.capture("dm.csv").unwrap_err();
    let findings = issues(&resource("datasets.DM.artifact", "dm.csv", error), "");
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.phase(), "ingest");
    assert_eq!(finding.condition(), "resource_path_content_changed");
    assert_eq!(finding.rule(), Some("FILE-10"));
    assert_eq!(finding.spec_paths, ["datasets.DM.artifact"]);
    assert_eq!(
        yamaa_core::json::Value::Object(finding.context.clone()),
        crate::json!({"path":"dm.csv"})
    );
    assert_eq!(resources.capture_reads(), 1);
    assert_eq!(original.as_ref(), b"ID\n1\n");
    drop(resources);
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn changed_define_metadata_uses_original_project_environment_spec_parent_and_dictionary_occurrences(
) {
    const SPEC: &str = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: str, derivation: SRC.ID}]\n";
    for changed in [
        "project.yaml",
        "python/uv.lock",
        "spec.yaml",
        "base.yaml",
        "dict.yaml",
    ] {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-define-metadata-change-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        let directory = crate::canonical_path::canonicalize(&directory).unwrap();
        fs::write(
            directory.join("project.yaml"),
            b"yamaa: '1.0'\ntype: project\nenvironments: {python: {lock: python/uv.lock}}\n",
        )
        .unwrap();
        fs::create_dir(directory.join("python")).unwrap();
        fs::write(
            directory.join("python/uv.lock"),
            b"version = 1\npackage = []\n",
        )
        .unwrap();
        let spec = match changed {
            "base.yaml" => {
                fs::write(directory.join("base.yaml"), SPEC).unwrap();
                "yamaa: '1.0'\ntype: spec\nparent: base.yaml\n".to_owned()
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
            crate::file_resolver::caller_path(&directory.join("project.yaml")).unwrap();
        let mut reads = Reads {
            resources: None,
            resolver: None,
            host: Some(Language::Python),
            environment: None,
            environment_written: String::new(),
            specifications: Vec::new(),
        };
        let environment = reads.capture_environment(&environment_written).unwrap();
        let specification = reads
            .capture_specification(
                "sdtm.specs[0]",
                &environment.root().source().identity,
                "spec.yaml",
            )
            .unwrap();
        let original_environment = environment.root().source().bytes.clone();
        let captured = reads.resources().capture_reads();
        fs::write(directory.join(changed), b"invalid replacement").unwrap();
        let failure = reads
            .verify_metadata("sdtm", &environment.root().source().identity)
            .unwrap_err();
        assert!(matches!(
            &failure,
            Failure::ClassifiedResource {
                error: file_resources::Error::Changed,
                ..
            }
        ));
        // A `parent` also names the layer that writes it, the specification.
        let layer = specification.source().identity.as_str();
        let (field, written, source) = match changed {
            "project.yaml" => ("environment", environment_written.as_str(), None),
            "python/uv.lock" => ("environments.python.lock", "python/uv.lock", None),
            "spec.yaml" => ("sdtm.specs[0]", "spec.yaml", None),
            "base.yaml" => ("parent", "base.yaml", Some(layer)),
            "dict.yaml" => ("columns.ID.derivation.mapping.dict", "dict.yaml", None),
            _ => unreachable!(),
        };
        let findings = issues(&failure, "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].phase(), "ingest");
        assert_eq!(findings[0].condition(), "resource_path_content_changed");
        assert_eq!(findings[0].rule(), Some("FILE-10"));
        assert_eq!(findings[0].spec_paths, [field]);
        assert_eq!(
            yamaa_core::json::Value::Object(findings[0].context.clone()),
            match source {
                Some(source) => crate::json!({"path":written,"source":source}),
                None => crate::json!({"path":written}),
            }
        );
        assert_eq!(reads.resources().capture_reads(), captured);
        assert_eq!(specification.source().bytes, spec.as_bytes());
        assert_eq!(environment.root().source().bytes, original_environment);
        assert!(!directory.join("never-read.csv").exists());
        assert!(!directory.join("never-written.csv").exists());
        drop(reads);
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
    let directory = crate::canonical_path::canonicalize(&directory).unwrap();
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/sdtm-dm-metadata");
    let project = fs::read_to_string(original.join("project.yaml"))
        .unwrap()
        .replace("  - spec.yaml", "  - spec.yaml\n  - second.yaml");
    let spec = fs::read_to_string(original.join("spec.yaml")).unwrap();
    fs::write(directory.join("project.yaml"), project).unwrap();
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
        directory.join("project.yaml").to_str().unwrap(),
        Submission::Sdtm,
        Some("2026-09-11T00:00:00"),
        "0.3.0",
        Some(Language::Python),
        None,
    );
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let held = result.artifacts().next().unwrap().1.to_vec();
    let reads = result.capture_reads();
    // Each transport file is named for its dataset beside the document. Once
    // the first is committed, the second's target becomes a directory.
    let later_target = directory.join("dm2.json");
    assert!(result
        .save_with(|index| {
            if index == 1 {
                fs::create_dir(&later_target).unwrap();
            }
            Ok(())
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
    assert!(matches!(
        error,
        file_publication::Error::Unreachable(yamaa_core::resource::ResourceFailure::NotRegularFile)
    ));
    // A target that became a directory is refused as the walk refuses it, at
    // the section's `define`, naming the file as written (#3030 D189).
    assert_eq!(result.issues().len(), 1);
    assert_eq!(result.issues()[0].phase(), "output");
    assert_eq!(
        result.issues()[0].condition(),
        "resource_path_not_regular_file"
    );
    assert_eq!(result.issues()[0].rule(), Some("FILE-7"));
    assert_eq!(result.issues()[0].spec_paths, ["sdtm.define"]);
    assert_eq!(
        yamaa_core::json::Value::Object(result.issues()[0].context.clone()),
        crate::json!({"path":"dm2.json"})
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

#[test]
fn define_caps_only_the_public_issue_prefix_and_retains_every_original_finding() {
    use yamaa_core::diagnostic::ConditionCode;
    for host in [Language::Python, Language::R] {
        for diagnostic in [false, true] {
            let missing = std::env::temp_dir().join(format!(
                "yamaa-missing-define-environment-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            let missing = crate::file_resolver::caller_path(&missing).unwrap();
            let mut result =
                Define::prepare(&missing, Submission::Sdtm, None, "0.4.0", Some(host), None);
            let failure = if diagnostic {
                Failure::Diagnostic(
                    (0..10_001)
                        .map(|index| {
                            define::resource_diagnostic(
                                &format!("findings[{index}]"),
                                "missing.csv",
                                define::ResourceObservation::Missing,
                            )
                        })
                        .collect(),
                )
            } else {
                Failure::Metadata(
                    (0..10_001)
                        .map(|index| {
                            Finding::cause(
                                ConditionCode::DefineRequiredDeclaration,
                                &format!("findings[{index}]"),
                                "authored metadata refusal",
                            )
                        })
                        .collect(),
                )
            };
            result.refuse(failure);
            assert_eq!(result.issues().len(), 10_000);
            for (index, issue) in result.issues().iter().enumerate() {
                assert_eq!(issue.spec_paths, [format!("findings[{index}]")]);
            }
            match result.failure().unwrap() {
                Failure::Metadata(findings) => {
                    assert_eq!(findings.len(), 10_001);
                    assert_eq!(findings.last().unwrap().path, "findings[10000]");
                }
                Failure::Diagnostic(findings) => {
                    assert_eq!(findings.len(), 10_001);
                    assert_eq!(findings.last().unwrap().spec_paths, ["findings[10000]"]);
                }
                other => panic!("original finding owner was replaced: {other:?}"),
            }
        }
    }
}
