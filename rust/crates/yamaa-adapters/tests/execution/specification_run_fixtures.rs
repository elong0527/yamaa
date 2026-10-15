//! Original-document compiler and source-port integration contracts.
use std::{path::Path, sync::Arc};
use yamaa_adapters::specification_source::Source;

use super::specification_run::*;

#[test]
fn classified_capture_failures_retain_complete_reports_without_publication() {
    use serde_json::json;
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PortError, PreparedRun, SourcePort},
    };
    use yamaa_core::resource::ResourceFailure;
    use yamaa_core::specification::SourceDeclaration;
    struct Payload {
        cause: Option<ResourceFailure>,
        identity: Arc<()>,
    }
    struct Port {
        bytes: Arc<[u8]>,
        calls: Vec<String>,
        fail_at: usize,
        reads: usize,
        cached: bool,
        cause: Option<ResourceFailure>,
        identity: Arc<()>,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        type Error = Payload;
        fn resource_failure(&self, error: &Payload) -> Option<ResourceFailure> {
            error.cause
        }
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            limit: usize,
        ) -> Result<Arc<[u8]>, Payload> {
            let index = self.calls.len();
            self.calls.push(source.name.clone());
            if index == self.fail_at {
                return Err(Payload {
                    cause: self.cause,
                    identity: Arc::clone(&self.identity),
                });
            }
            assert_eq!(source.name, "DM");
            assert!(self.bytes.len() <= limit);
            self.reads += usize::from(!self.cached);
            Ok(Arc::clone(&self.bytes))
        }
    }
    struct NoPublication;
    impl specification_report::ArtifactPort for NoPublication {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed build published")
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let case = root.join("benchmarks/schema-lookup");
    let schema = schema(&root.join("yaml"));
    let run = PreparedRun::prepare(prepare(
        &schema,
        &std::fs::read(case.join("spec.yaml")).unwrap(),
    ))
    .unwrap();
    let bytes: Arc<[u8]> = std::fs::read(case.join("input/dm.csv")).unwrap().into();
    let metadata = || Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example: "schema-lookup",
        specification: "spec.yaml",
        base_directory: ".",
    };
    for (cause, condition) in [
        (ResourceFailure::Missing, "resource_path_missing"),
        (
            ResourceFailure::NotRegularFile,
            "resource_path_not_regular_file",
        ),
    ] {
        for fail_at in [0, 1] {
            for cached in [false, true] {
                let identity = Arc::new(());
                let mut port = Port {
                    bytes: Arc::clone(&bytes),
                    calls: vec![],
                    fail_at,
                    reads: 0,
                    cached,
                    cause: Some(cause),
                    identity: Arc::clone(&identity),
                };
                let mut attempt = run.execute_with_port(&mut port);
                assert_eq!(
                    port.calls,
                    if fail_at == 0 {
                        vec!["DM"]
                    } else {
                        vec!["DM", "AE"]
                    }
                );
                let Err(PortError::Capture(error)) = &attempt.result else {
                    panic!("capture failure missing")
                };
                assert!(Arc::ptr_eq(&identity, &error.identity));
                if fail_at == 1 {
                    assert!(attempt.sources[0].table.is_some());
                }
                let (dataset, path) = if fail_at == 0 {
                    ("DM", "input/dm.csv")
                } else {
                    ("AE", "input/ae.csv")
                };
                let diagnostics = json!([{"phase":"validation","condition":condition,"requirement":"REQ-0785","spec_paths":[format!("input.{dataset}.path")],"context":{"dataset":dataset,"path":path}}]);
                let mut reads = vec![];
                if fail_at == 1 {
                    reads.push(json!({"base_directory":".","path":"input/dm.csv","outcome":"captured","condition":null,"snapshots_created":usize::from(!cached)}));
                }
                reads.push(json!({"base_directory":".","path":path,"outcome":"failure","condition":condition,"snapshots_created":0}));
                let expected = json!({"report_version":"0.3.0-draft","runtime":"python","backend":"rust","runtime_version":"fixture-runtime","engine_version":"fixture-engine","example":"schema-lookup","outcome":"failure","artifacts":[],"diagnostics":diagnostics,"unsupported":[],"handler_counts":[],"nodes":[{"specification":"spec.yaml","outcome":"failure","diagnostics":diagnostics,"unsupported":[],"handler_counts":[]}],"tables":[],"verifications":[],"callbacks":[],"source_reads":reads,"error":null});
                let kind = match cause {
                    ResourceFailure::Missing => "missing",
                    ResourceFailure::NotRegularFile => "not_regular_file",
                    ResourceFailure::UriScheme
                    | ResourceFailure::Backslash
                    | ResourceFailure::EmptySegment
                    | ResourceFailure::ContentChanged
                    | ResourceFailure::Symlink => {
                        unreachable!("this retained fixture selects missing and nonregular causes")
                    }
                };
                let row = include_str!("../fixtures/source_capture.tsv")
                    .lines()
                    .skip(1)
                    .map(|line| line.split('\t').collect::<Vec<_>>())
                    .find(|row| {
                        row[0] == kind
                            && row[1] == fail_at.to_string()
                            && row[2] == usize::from(cached).to_string()
                    })
                    .expect("independent capture truth missing");
                assert_eq!(
                    expected,
                    serde_json::from_str::<serde_json::Value>(row[3]).unwrap()
                );
                let result =
                    specification_report::build_result(&run, &attempt, metadata()).unwrap();
                assert_eq!(result.observations(), expected);
                assert!(result.output().is_none());
                assert!(matches!(
                    result.save(&mut NoPublication),
                    Err(yamaa_core::specification_output::SaveError::FailedBuild)
                ));
                // Failed accounting and opaque host errors cannot borrow a known
                // finding left in the source ledger to become semantic failures.
                attempt.sources.last_mut().unwrap().read.snapshots_created = None;
                assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
                attempt.sources.last_mut().unwrap().read.snapshots_created = Some(0);
                attempt.sources.last_mut().unwrap().read.failure = None;
                assert!(specification_report::failure(&run, &attempt, metadata()).is_err());
            }
        }
    }
    let identity = Arc::new(());
    let mut port = Port {
        bytes,
        calls: vec![],
        fail_at: 0,
        reads: 0,
        cached: false,
        cause: None,
        identity: Arc::clone(&identity),
    };
    let attempt = run.execute_with_port(&mut port);
    let Err(PortError::Capture(error)) = &attempt.result else {
        panic!("opaque failure missing")
    };
    assert!(Arc::ptr_eq(&identity, &error.identity));
    assert_eq!(attempt.sources[0].read.failure, None);
    assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
}

#[test]
fn resource_findings_use_written_inherited_paths_not_rebased_locations() {
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
        specification_source::{InheritancePort, Source},
    };
    use yamaa_core::resource::ResourceFailure;
    use yamaa_core::{
        inheritance::{Source as FileIdentity, SourceError},
        specification::SourceDeclaration,
    };
    struct Files {
        parent: Vec<u8>,
    }
    impl InheritancePort for Files {
        type Error = ();
        fn canonicalize(
            &mut self,
            _: &str,
            written: &str,
        ) -> Result<FileIdentity, SourceError<()>> {
            assert_eq!(written, "parent.yaml");
            Ok(FileIdentity {
                identity: written.into(),
                display_path: written.into(),
            })
        }
        fn capture(&mut self, _: &FileIdentity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
            Ok(self.parent.clone())
        }
        fn rebase(
            &mut self,
            layer: &FileIdentity,
            _: &FileIdentity,
            written: &str,
            _: usize,
        ) -> Result<String, ()> {
            Ok(if layer.identity == "parent.yaml" {
                format!("internal-location/{written}")
            } else {
                written.into()
            })
        }
    }
    struct Missing;
    impl SourcePort for Missing {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        type Error = ();
        fn resource_failure(&self, _: &()) -> Option<ResourceFailure> {
            Some(ResourceFailure::Missing)
        }
        fn capture_reads(&self) -> usize {
            0
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            Err(())
        }
    }
    for declaration in [
        "SRC: nested/data.csv",
        "SRC: {path: nested/data.csv, types: {ID: int}}",
    ] {
        for child_path in [None, Some("child.csv")] {
            let mut entry = String::from("schema_version: '1.0'\nparents: parent.yaml\ndomain: TEST\nkeys: [ID]\noutput: {path: result.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: {source: SRC.ID}}]\n");
            if let Some(path) = child_path {
                entry.push_str(&format!("input: {{SRC: {{path: {path}}}}}\n"));
            }
            let parent = format!("schema_version: '1.0'\ninput: {{{declaration}}}\n");
            let document = yamaa_adapters::shipped_schema::prepare(
                Source {
                    identity: "entry.yaml".into(),
                    bytes: entry.into_bytes(),
                },
                "entry.yaml".into(),
                &mut Files {
                    parent: parent.into_bytes(),
                },
            )
            .unwrap();
            let written = child_path.unwrap_or("nested/data.csv");
            assert_eq!(document.written_source_path("SRC"), Some(written));
            assert_eq!(document.written_source_path("ABSENT"), None);
            let run = PreparedRun::prepare(document).unwrap();
            if child_path.is_none() {
                assert_eq!(run.source().path, "internal-location/nested/data.csv");
            }
            let attempt = run.execute_with_port(&mut Missing);
            let report = specification_report::failure(
                &run,
                &attempt,
                Identity {
                    runtime: "r",
                    runtime_version: "test",
                    engine_version: "test",
                    example: "inheritance",
                    specification: "entry.yaml",
                    base_directory: ".",
                },
            )
            .unwrap();
            assert_eq!(
                report["diagnostics"],
                serde_json::json!([{"phase":"validation","condition":"resource_path_missing","requirement":"REQ-0785","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":written}}])
            );
        }
    }
}

#[test]
fn original_column_literals_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(
        include_str!("../fixtures/column_literals.json"),
        "literal",
        4,
    );
}

#[test]
fn original_conversion_handlers_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(
        include_str!("../fixtures/original_conversion_handlers.json"),
        "handler",
        5,
    );
}

#[test]
fn original_row_conversion_handlers_match_complete_reference_reports_and_csv() {
    independent_scalar_reports(
        include_str!("../fixtures/original_row_conversion_handlers.json"),
        "row-handler",
        8,
    );
}

fn independent_scalar_reports(fixture: &str, prefix: &str, expected_cases: usize) {
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        reads: usize,
        saves: usize,
        expected: Vec<u8>,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            assert!(maximum >= 5);
            self.reads += 1;
            Ok(Arc::from(&b"ID\n1\n"[..]))
        }
    }
    impl ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), ()> {
            assert!(!self.expected.is_empty(), "failed literal build published");
            assert_eq!(path, "result.csv");
            assert_eq!(content, self.expected);
            self.saves += 1;
            Ok(())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut cases = 0;
    for row in fixture_rows(fixture) {
        let mut port = Port {
            reads: 0,
            saves: 0,
            expected: fixture_bytes(&row["output"]),
        };
        let run =
            PreparedRun::prepare(prepare(&schema, row["source"].as_str().unwrap().as_bytes()))
                .unwrap();
        let attempt = run.execute_with_port(&mut port);
        let example = format!("{prefix}-{}", row["case"].as_str().unwrap());
        let result = specification_report::build_result(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: &example,
                specification: "spec.yaml",
                base_directory: ".",
            },
        )
        .unwrap();
        drop(run);
        drop(attempt);
        let expected: serde_json::Value = row["expected"].clone();
        let mut unsaved = expected.clone();
        unsaved["artifacts"] = serde_json::json!([]);
        assert_eq!(
            result.observations(),
            unsaved,
            "{}",
            row["case"].as_str().unwrap()
        );
        if port.expected.is_empty() {
            assert!(result.output().is_none());
            for _ in 0..2 {
                assert!(matches!(
                    result.save(&mut port),
                    Err(yamaa_core::specification_output::SaveError::FailedBuild)
                ));
            }
            assert_eq!(port.saves, 0);
        } else {
            assert!(result.output().is_some());
            for _ in 0..2 {
                assert_eq!(
                    result.save(&mut port).unwrap().observations_value(false),
                    expected
                );
            }
            assert_eq!(port.saves, 2);
        }
        assert_eq!(result.observations(), unsaved);
        assert_eq!(port.reads, 1);
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}

#[test]
fn metadata_inspection_reports_all_known_failures_before_any_study_read() {
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PortError, PreparedRun, SourcePort},
    };
    use yamaa_core::resource::ResourceFailure;
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        failures: serde_json::Value,
        inspected: Vec<String>,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        type Error = ResourceFailure;
        fn resource_failure(&self, error: &ResourceFailure) -> Option<ResourceFailure> {
            Some(*error)
        }
        fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), ResourceFailure> {
            self.inspected.push(source.name.clone());
            match self.failures[&source.name].as_str() {
                Some("missing") => Err(ResourceFailure::Missing),
                Some("not_regular_file") => Err(ResourceFailure::NotRegularFile),
                None => Ok(()),
                _ => panic!("unknown independent inspection cause"),
            }
        }
        fn capture_reads(&self) -> usize {
            0
        }
        fn capture(
            &mut self,
            _: &SourceDeclaration,
            _: usize,
        ) -> Result<Arc<[u8]>, ResourceFailure> {
            panic!("metadata failure reached study capture");
        }
    }
    struct Publisher;
    impl specification_report::ArtifactPort for Publisher {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("inspection failure published");
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let schema = schema(&root.join("yaml"));
    let run = PreparedRun::prepare(prepare(
        &schema,
        &std::fs::read(root.join("benchmarks/schema-lookup/spec.yaml")).unwrap(),
    ))
    .unwrap();
    let metadata = || Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example: "schema-lookup",
        specification: "spec.yaml",
        base_directory: ".",
    };
    for line in include_str!("../fixtures/source_inspection.tsv")
        .lines()
        .skip(1)
    {
        let fields = line.split('\t').collect::<Vec<_>>();
        let mut port = Port {
            failures: serde_json::from_str(fields[1]).unwrap(),
            inspected: vec![],
        };
        let mut attempt = run.execute_with_port(&mut port);
        assert_eq!(port.inspected, ["DM", "AE", "MEDDRA"]);
        assert!(attempt.sources.is_empty());
        let expected = serde_json::from_str::<serde_json::Value>(fields[2]).unwrap();
        let result = specification_report::build_result(&run, &attempt, metadata()).unwrap();
        assert_eq!(result.observations(), expected, "{}", fields[0]);
        assert!(result.output().is_none());
        assert!(matches!(
            result.save(&mut Publisher),
            Err(yamaa_core::specification_output::SaveError::FailedBuild)
        ));
        let Err(PortError::Inspect(ref mut failures)) = attempt.result else {
            panic!("inspection findings");
        };
        // A cause cannot be attached to a forged declaration, repeated source or
        // opaque host failure merely because the rest of the ledger is valid.
        let original = failures[0].source.clone();
        failures[0].source.path = "forged.csv".into();
        assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
        let Err(PortError::Inspect(ref mut failures)) = attempt.result else {
            unreachable!();
        };
        failures[0].source = original;
        if failures.len() > 1 {
            failures.swap(0, 1);
            assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
            let Err(PortError::Inspect(ref mut failures)) = attempt.result else {
                unreachable!();
            };
            failures.swap(0, 1);
            failures[1].source = failures[0].source.clone();
            assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
        }
        let Err(PortError::Inspect(ref mut failures)) = attempt.result else {
            unreachable!();
        };
        failures[0].failure = None;
        assert!(specification_report::build_result(&run, &attempt, metadata()).is_err());
    }
}

#[test]
fn original_source_filters_match_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/source_filters.json"),
        "source-filter",
        13,
    );
}
#[test]
fn original_first_available_matches_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/first_available.json"),
        "first-available",
        11,
    );
}
#[test]
fn original_assertions_match_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/original_assertions.json"),
        "assert",
        17,
    );
}
#[test]
fn original_all_or_none_matches_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/original_all_or_none.json"),
        "all-or-none",
        11,
    );
}
#[test]
fn original_column_not_missing_matches_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/original_not_missing.json"),
        "column-check",
        10,
    );
}
#[test]
fn row_filter_carrier_failures_match_complete_normative_reports() {
    replay_source_selection_truth(
        include_str!("../fixtures/row_filter_admission.json"),
        "predicate-carrier",
        2,
    );
}

#[test]
fn exact_wide_count_bounds_preserve_empty_counts_and_completed_check_prefix() {
    use serde_json::json;
    use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Host {
        bytes: Arc<[u8]>,
        reads: usize,
    }
    impl SourcePort for Host {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, ()> {
            self.reads += 1;
            Ok(Arc::clone(&self.bytes))
        }
    }
    impl ArtifactPort for Host {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed checks published")
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let prefix = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: source.csv}\noutput: {path: result.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    for input in [b"ID\n".as_slice(), b"ID\n1\n2\n".as_slice()] {
        for bound in [
            "min: 999999999999999999999999999999",
            "max: -999999999999999999999999999999",
        ] {
            let source = format!("{prefix}verifications:\n  - row_count: {{id: first, min: -999999999999999999999999999999, max: 999999999999999999999999999999}}\n  - row_count: {{{bound}}}\n");
            let run = PreparedRun::prepare(prepare(&schema, source.as_bytes())).unwrap();
            let mut host = Host {
                bytes: input.into(),
                reads: 0,
            };
            let attempt = run.execute_with_port(&mut host);
            let result = specification_report::build_result(
                &run,
                &attempt,
                Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: "wide-count",
                    specification: "spec.yaml",
                    base_directory: ".",
                },
            )
            .unwrap();
            let report = result.observations();
            assert_eq!(host.reads, 1);
            assert_eq!(report["outcome"], "failure");
            assert_eq!(report["artifacts"], json!([]));
            assert_eq!(report["verifications"].as_array().unwrap().len(), 2);
            assert_eq!(report["verifications"][0]["verification_id"], "first");
            assert_eq!(report["verifications"][0]["failure"], json!(null));
            let count = if input.len() == 3 { 0 } else { 2 };
            assert_eq!(
                report["diagnostics"],
                json!([{"phase":"verification", "condition":"row_count_failed", "requirement":"REQ-0385", "spec_paths":["verifications[1].row_count"], "context":{"count":count,"failure_count":1,"keys":[{}]}}])
            );
            assert_eq!(
                report["verifications"][1]["failure"]["log_context"]["count"],
                count
            );
        }
    }
}

#[test]
fn row_filters_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/row_filters.json"),
        "row-filter",
        27,
    );
}
#[test]
fn row_reductions_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/row_reductions.json"),
        "row-reduction",
        25,
    );
}
#[test]
fn aggregate_relations_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/correlated_aggregates.json"),
        "correlated-aggregate",
        16,
    );
}
#[test]
fn odm_reads_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(include_str!("../fixtures/odm_reads.json"), "odm", 14);
}

#[test]
fn deferred_odm_defaults_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/odm_deferred.json"),
        "odm-deferred",
        8,
    );
}

#[test]
fn odm_preflight_matches_complete_independent_findings_before_source_authority() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in fixture_rows(include_str!("../fixtures/odm_preflight.json")) {
        let raw = fixture_bytes(&row["source"]);
        let error = yamaa_adapters::specification_run::PreparedRun::prepare(prepare(&schema, &raw))
            .unwrap_err();
        let actual: serde_json::Value = serde_json::from_str(
            &yamaa_adapters::specification_diagnostics::failure(&error, None),
        )
        .unwrap();
        let expected: serde_json::Value = row["expected"].clone();
        assert_eq!(actual, expected, "{}", row["case"].as_str().unwrap());
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn row_column_checks_match_complete_independent_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/row_column_checks.json"),
        "row-column",
        48,
    );
}
#[test]
fn original_column_values_match_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/original_column_values.json"),
        "column-value",
        20,
    );
}
#[test]
fn original_column_matches_match_complete_reports_and_exact_csv() {
    replay_source_selection_truth(
        include_str!("../fixtures/original_column_matches.json"),
        "column-matches",
        16,
    );
}
#[test]
fn deferred_column_findings_stop_later_payload_compilation_but_keep_vocabulary_admission() {
    use yamaa_adapters::specification_run::{Error, PreparedRun};
    use yamaa_core::specification::PrepareError;
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let raw = |first: &str, severity: &str| {
        format!(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: unread.csv}}\noutput: {{path: result.csv, columns: [ID, V]}}\ncolumns:\n  - {{name: ID, type: str, label: ID, derivation: SRC.ID}}\n  - name: V\n    type: str\n    label: V\n    derivation: SRC.V\n    verifications:\n      - {first}\n      - matches: {{pattern: 'a{{1000001}}', severity: {severity}}}\n"
    )
    };
    for first in ["matches: {pattern: '('}", "allowed_values: {values: []}"] {
        let document = prepare(&schema, raw(first, "error").as_bytes());
        PreparedRun::prepare(document).expect("the earlier finding remains deferred");
        let document = prepare(&schema, raw(first, "warning").as_bytes());
        assert!(matches!(PreparedRun::prepare(document),
            Err(Error::Prepare(PrepareError::Invalid(items)))
                if items.iter().any(|item| item.diagnostic().definition().condition == "missing_warning_log")));
        let with_log = raw(first, "warning").replace(
            "columns: [ID, V]}",
            "columns: [ID, V], warning_log: warnings.csv}",
        );
        PreparedRun::prepare(prepare(&schema, with_log.as_bytes()))
            .expect("later large pattern is still not compiled after the earlier deferred finding");
    }
    for (first, later_severity, index) in [
        (
            "matches: {pattern: 'a{1000001}', severity: warning}",
            "error",
            0,
        ),
        ("matches: {pattern: 'a{1000001}'}", "warning", 1),
    ] {
        let document = prepare(&schema, raw(first, later_severity).as_bytes());
        let expected = format!("columns.V.verifications[{index}].matches");
        // Without an earlier deferred finding, the first large payload reaches
        // its own regex budget and still grants no study authority.
        assert!(
            matches!(
                PreparedRun::prepare(document),
                Err(Error::Prepare(PrepareError::Limit(_)))
            ),
            "{expected}"
        );
    }
}
#[test]
fn static_verification_checks_match_complete_issues_without_study_authority() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let rows = fixture_rows(include_str!("../fixtures/static_verification_checks.json"));
    assert_eq!(rows.len(), 15);
    for row in &rows {
        let document = schema
            .prepare_standalone(Source {
                identity: "spec.yaml".into(),
                bytes: fixture_bytes(&row["source"]),
            })
            .unwrap();
        let run = yamaa_adapters::specification_run::PreparedRun::prepare(document).unwrap();
        for _ in 0..2 {
            assert_eq!(
                yamaa_adapters::specification_check::issues(&run).unwrap(),
                serde_json::to_string(&row["expected"]).unwrap(),
                "{}",
                row["case"].as_str().unwrap()
            );
        }
    }
}
fn replay_source_selection_truth(truth: &str, prefix: &str, cases: usize) {
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        content: Arc<[u8]>,
        reads: usize,
        expected: Vec<u8>,
        saves: usize,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.reads
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            assert!(self.content.len() <= maximum);
            self.reads += 1;
            Ok(Arc::clone(&self.content))
        }
    }
    impl specification_report::ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), ()> {
            assert!(!self.expected.is_empty(), "failed filter result published");
            assert_eq!(path, "result.csv");
            assert_eq!(bytes, self.expected);
            self.saves += 1;
            Ok(())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in fixture_rows(truth) {
        let case = row["case"].as_str().unwrap();
        let raw = fixture_bytes(&row["source"]);
        let run = PreparedRun::prepare(prepare(&schema, &raw)).unwrap();
        let mut port = Port {
            content: fixture_bytes(&row["input"]).into(),
            reads: 0,
            expected: fixture_bytes(&row["artifact"]),
            saves: 0,
        };
        let attempt = run.execute_with_port(&mut port);
        let example = format!("{prefix}-{}", case);
        let result = specification_report::build_result(
            &run,
            &attempt,
            Identity {
                runtime: "python",
                runtime_version: "fixture-runtime",
                engine_version: "fixture-engine",
                example: &example,
                specification: "spec.yaml",
                base_directory: ".",
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "{}: {error:?}; execution: {:?}; outcome: {:?}",
                case,
                attempt.result.as_ref().err(),
                attempt
                    .result
                    .as_ref()
                    .ok()
                    .map(|response| &response.outcome)
            )
        });
        let expected: serde_json::Value = row["expected"].clone();
        // Require the captured row-filter citations without changing any finding.
        if prefix == "row-filter"
            && matches!(
                case,
                "grouped_source_phase"
                    | "grouped_unknown_phase"
                    | "record_unknown_phase"
                    | "group_and_filter_order"
            )
        {
            for diagnostics in ["diagnostics", "nodes"] {
                let values = if diagnostics == "nodes" {
                    &expected["nodes"][0]["diagnostics"]
                } else {
                    &expected["diagnostics"]
                };
                let mut phase_boundaries = 0;
                for finding in values.as_array().unwrap() {
                    if finding["condition"] == "phase_boundary" {
                        assert_eq!(finding["requirement"], serde_json::json!("REQ-0068"));
                        assert_eq!(finding["spec_paths"], serde_json::json!(["rows[0].filter"]));
                        phase_boundaries += 1;
                    }
                }
                assert_eq!(phase_boundaries, 1);
            }
        }
        let mut unsaved = expected.clone();
        unsaved["artifacts"] = serde_json::json!([]);
        assert_eq!(result.observations(), unsaved, "{}", case);
        if port.expected.is_empty() {
            assert!(result.output().is_none());
            assert!(matches!(
                result.save(&mut port),
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
            ));
        } else {
            assert!(result.output().is_some());
            for _ in 0..2 {
                assert_eq!(
                    result.save(&mut port).unwrap().observations_value(false),
                    expected
                );
            }
            assert_eq!(port.saves, 2);
        }
        assert_eq!(port.reads, 1);
        count += 1;
    }
    assert_eq!(count, cases);
}

#[test]
fn original_source_bindings_match_complete_reports_and_exact_repeated_csv() {
    use std::collections::BTreeMap;
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        inputs: BTreeMap<String, Arc<[u8]>>,
        calls: Vec<String>,
        expected: Vec<u8>,
        saves: usize,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.calls.len()
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!(source.path, format!("{}.csv", source.name.to_lowercase()));
            self.calls.push(source.name.clone());
            let input = &self.inputs[&source.name];
            assert!(input.len() <= maximum);
            Ok(Arc::clone(input))
        }
    }
    impl specification_report::ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), ()> {
            assert!(!self.expected.is_empty(), "failed source binding published");
            assert_eq!(path, "result.csv");
            assert_eq!(content, self.expected);
            self.saves += 1;
            Ok(())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in fixture_rows(include_str!("../fixtures/original_source_bindings.json")) {
        let case = row["case"].as_str().unwrap();
        let run = PreparedRun::prepare(prepare(&schema, &fixture_bytes(&row["source"]))).unwrap();
        let inputs: BTreeMap<String, String> =
            serde_json::from_value(row["inputs"].clone()).unwrap();
        let expected: serde_json::Value = row["expected"].clone();
        // A second attempt must neither inherit choices nor accumulate handler counts.
        for _ in 0..2 {
            let mut port = Port {
                inputs: inputs
                    .iter()
                    .map(|(n, h)| (n.clone(), h.as_bytes().into()))
                    .collect(),
                calls: vec![],
                expected: fixture_bytes(&row["artifact"]),
                saves: 0,
            };
            let attempt = run.execute_with_port(&mut port);
            let example = format!("source-binding-{}", case);
            let result = specification_report::build_result(
                &run,
                &attempt,
                Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: &example,
                    specification: "spec.yaml",
                    base_directory: ".",
                },
            )
            .unwrap();
            let mut unsaved = expected.clone();
            unsaved["artifacts"] = serde_json::json!([]);
            assert_eq!(result.observations(), unsaved, "{}", case);
            if port.expected.is_empty() {
                assert!(result.output().is_none());
                assert!(matches!(
                    result.save(&mut port),
                    Err(yamaa_core::specification_output::SaveError::FailedBuild)
                ));
            } else {
                for _ in 0..2 {
                    assert_eq!(
                        result.save(&mut port).unwrap().observations_value(false),
                        expected,
                        "{}",
                        case
                    );
                }
                assert_eq!(port.saves, 2);
            }
            assert_eq!(
                port.calls,
                if inputs.contains_key("DONOR") {
                    vec!["SRC", "DONOR"]
                } else {
                    vec!["SRC"]
                }
            );
        }
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn original_mapping_cut_match_complete_reports_and_exact_repeated_csv() {
    use std::collections::BTreeMap;
    use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    use yamaa_core::specification::SourceDeclaration;
    struct Port {
        inputs: BTreeMap<String, Arc<[u8]>>,
        calls: Vec<String>,
        expected: Vec<u8>,
        saves: usize,
    }
    impl SourcePort for Port {
        fn capture_observed(
            &mut self,
            source: &SourceDeclaration,
            byte_limit: usize,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, Self::Error> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            <Self as yamaa_core::specification_run::SourcePort>::capture(self, source, byte_limit)
        }
        fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
            Ok(())
        }
        fn resource_failure(
            &self,
            _error: &Self::Error,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            None
        }
        type Error = ();
        fn capture_reads(&self) -> usize {
            self.calls.len()
        }
        fn capture(&mut self, source: &SourceDeclaration, maximum: usize) -> Result<Arc<[u8]>, ()> {
            assert_eq!(source.path, format!("{}.csv", source.name.to_lowercase()));
            self.calls.push(source.name.clone());
            let input = &self.inputs[&source.name];
            assert!(input.len() <= maximum);
            Ok(Arc::clone(input))
        }
    }
    impl specification_report::ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), ()> {
            assert!(!self.expected.is_empty(), "failed mapping/cut published");
            assert_eq!(path, "result.csv");
            assert_eq!(content, self.expected);
            self.saves += 1;
            Ok(())
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut count = 0;
    for row in fixture_rows(include_str!("../fixtures/original_mapping_cut.json")) {
        let case = row["case"].as_str().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fixture_bytes(&row["expected_wire"]))
                .unwrap(),
            row["expected"]
        );
        let run = PreparedRun::prepare(prepare(&schema, &fixture_bytes(&row["source"]))).unwrap();
        let inputs: BTreeMap<String, String> =
            serde_json::from_value(row["inputs"].clone()).unwrap();
        let expected: serde_json::Value = row["expected"].clone();
        // A second attempt must neither inherit choices nor accumulate handler counts.
        for _ in 0..2 {
            let mut port = Port {
                inputs: inputs
                    .iter()
                    .map(|(n, h)| (n.clone(), h.as_bytes().into()))
                    .collect(),
                calls: vec![],
                expected: fixture_bytes(&row["artifact"]),
                saves: 0,
            };
            let attempt = run.execute_with_port(&mut port);
            let example = format!("mapping-cut-{}", case);
            let result = specification_report::build_result(
                &run,
                &attempt,
                Identity {
                    runtime: "python",
                    runtime_version: "fixture-runtime",
                    engine_version: "fixture-engine",
                    example: &example,
                    specification: "spec.yaml",
                    base_directory: ".",
                },
            )
            .unwrap();
            let mut unsaved = expected.clone();
            unsaved["artifacts"] = serde_json::json!([]);
            assert_eq!(result.observations(), unsaved, "{}", case);
            if port.expected.is_empty() {
                assert!(result.output().is_none());
                assert!(matches!(
                    result.save(&mut port),
                    Err(yamaa_core::specification_output::SaveError::FailedBuild)
                ));
            } else {
                for _ in 0..2 {
                    assert_eq!(
                        result.save(&mut port).unwrap().observations_value(false),
                        expected,
                        "{}",
                        case
                    );
                }
                assert_eq!(port.saves, 2);
            }
            assert_eq!(
                port.calls,
                if inputs.contains_key("DONOR") {
                    vec!["SRC", "DONOR"]
                } else {
                    vec!["SRC"]
                }
            );
        }
        count += 1;
    }
    assert_eq!(count, 21);
}

#[test]
fn source_condition_stays_unknown_under_the_normative_grammar() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let raw=b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: src.csv}\noutput: {path: result.csv, columns: [ID]}\ncolumns:\n  - name: ID\n    type: str\n    label: ID\n    derivation:\n      source: {variable: SRC.ID, condition: TRUE}\n";
    let error = schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: raw.to_vec(),
        })
        .unwrap_err();
    let finding: serde_json::Value = serde_json::from_str(
        &yamaa_adapters::specification_diagnostics::capture_failure(&error),
    )
    .unwrap();
    assert_eq!(finding["outcome"]["status"], "invalid");
    assert!(finding["outcome"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["condition"] == "unknown_field"
            && issue["spec_paths"]
                == serde_json::json!(["columns.ID.derivation.source.condition"])));
}

#[test]
fn original_mapping_cut_admission_has_complete_findings_before_study_ports() {
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    for row in fixture_rows(include_str!("../fixtures/mapping_cut_admission.json")) {
        let bytes = fixture_bytes(&row["source"]);
        let error =
            yamaa_adapters::specification_run::PreparedRun::prepare(prepare(&schema, &bytes))
                .unwrap_err();
        let expected: serde_json::Value = row["expected"].clone();
        assert_eq!(
            spec_findings(&error, None).unwrap(),
            expected["outcome"]["diagnostics"]
                .as_array()
                .unwrap()
                .clone(),
            "{}",
            row["case"].as_str().unwrap()
        );
    }
}

#[test]
fn numeric_policy_reply_preserves_the_applicable_parser_and_resolution_bounds() {
    use yamaa_adapters::{specification_diagnostics, specification_run::Error};
    use yamaa_core::{
        numeric_compiler::CompileError,
        numeric_parser::{ParseError, ParseResource, SourcePosition},
        specification::PrepareError,
    };
    for (error, resource, bound, actual) in [
        (
            CompileError::ResolutionLimit {
                limit: 7,
                required: 8,
            },
            "resolutions",
            7,
            Some(8),
        ),
        (
            CompileError::Parse(ParseError::Limit {
                position: SourcePosition {
                    byte: 1,
                    character: 1,
                },
                resource: ParseResource::Nodes,
                limit: 3,
            }),
            "nodes",
            3,
            None,
        ),
    ] {
        let error = Error::Prepare(PrepareError::Numeric {
            path: "columns.VALUE.derivation.compute.expr".into(),
            error,
        });
        let value: serde_json::Value =
            serde_json::from_str(&specification_diagnostics::failure(&error, None)).unwrap();
        let outcome = &value["outcome"];
        assert_eq!(outcome["code"], "numeric_policy");
        assert_eq!(outcome["stage"], "prepare");
        assert_eq!(outcome["resource"], resource);
        assert_eq!(outcome["limit"], bound);
        assert_eq!(outcome["actual"].as_u64(), actual);
        assert_eq!(
            outcome["spec_paths"],
            serde_json::json!(["columns.VALUE.derivation.compute.expr"])
        );
        assert_eq!(
            outcome.as_object().unwrap().contains_key("actual"),
            actual.is_some()
        );
    }
}
