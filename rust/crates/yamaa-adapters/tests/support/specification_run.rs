//! Original-document compiler and source-port integration contracts.
// Reexports share the authored fixture types across automatically discovered feature tests.
#![allow(unused_imports)]
pub(crate) use std::{path::Path, sync::Arc};
pub(crate) use yamaa_adapters::{
    csv_source,
    specification_source::{CapturedSchema, PreparedDocument, Source},
    yaml_decode::decode_yaml,
};
pub(crate) use yamaa_core::{
    evaluation::EvaluationErrorKind,
    schema::{Document, DocumentNode as N, SpecificationDocument},
    table::{TableAccess, TableSchema},
    value::Value,
};

pub(crate) use yamaa_core::dataset::{self, DatasetPlan};

/// Rows of a converted readable JSON fixture (formerly hex-encoded TSV).
pub(crate) fn fixture_rows(truth: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(truth).unwrap()
}

/// Exact bytes of a fixture text cell; the two non-UTF-8 cells in the whole
/// fixture set are tagged as {"hex": "..."} objects instead of strings.
pub(crate) fn fixture_bytes(value: &serde_json::Value) -> Vec<u8> {
    if let Some(text) = value.as_str() {
        return text.as_bytes().to_vec();
    }
    let hex = value["hex"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

pub(crate) fn spec_findings(
    error: &yamaa_adapters::specification_run::Error,
    source: Option<&yamaa_core::specification::SourceDeclaration>,
) -> Option<Vec<serde_json::Value>> {
    yamaa_adapters::specification_diagnostics::findings(error, source)
        .expect("valid authored source finding context")
}

pub(crate) fn text(d: &Document, id: usize) -> &str {
    let N::Text(value) = &d.nodes()[id] else {
        panic!("expected text")
    };
    value
}
pub(crate) fn sequence(d: &Document, id: usize) -> &[usize] {
    let N::Sequence(value) = &d.nodes()[id] else {
        panic!("expected sequence")
    };
    value
}
pub(crate) fn schema(root: &Path) -> Arc<CapturedSchema> {
    pub(crate) fn visit(root: &Path, name: &str, modules: &mut Vec<Source>) {
        if modules.iter().any(|m| m.identity == name) {
            return;
        }
        let d = decode_yaml(&std::fs::read(root.join(name)).unwrap())
            .unwrap()
            .document;
        let children = d
            .field(d.root(), "includes")
            .map(|id| {
                sequence(&d, id)
                    .iter()
                    .map(|&id| text(&d, id).to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        modules.push(Source {
            identity: name.into(),
            bytes: std::fs::read(root.join(name)).unwrap(),
        });
        for child in children {
            visit(root, &child, modules);
        }
    }
    let mut modules = Vec::new();
    visit(root, "schema.yaml", &mut modules);
    CapturedSchema::admit(modules, 0).unwrap()
}
pub(crate) fn prepare(schema: &Arc<CapturedSchema>, source: &[u8]) -> PreparedDocument {
    schema
        .prepare_standalone(Source {
            identity: "spec.yaml".into(),
            bytes: source.to_vec(),
        })
        .unwrap()
}
pub(crate) fn lower(spec: &SpecificationDocument, source: &TableSchema) -> DatasetPlan {
    yamaa_core::specification::PreparedSpecification::prepare(spec)
        .unwrap()
        .bind(source)
        .unwrap()
}

pub(crate) fn independent_failed_reports(
    fixture: &str,
    prefix: &str,
    expected_counts: (usize, usize),
    content: &[u8],
) {
    pub(crate) use serde_json::Value;
    pub(crate) use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port(usize, Arc<[u8]>);
    impl SourcePort for Port {
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
            self.0
        }
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, ()> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            self.0 += 1;
            Ok(Arc::clone(&self.1))
        }
    }
    impl ArtifactPort for Port {
        type Error = ();
        fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
            panic!("failed output published")
        }
    }
    let schema = yamaa_adapters::shipped_schema::capture().unwrap();
    let mut cases = 0;
    let mut findings = 0;
    // Converted fixtures are JSON rows; the remaining readable TSV fixtures
    // keep their original three/four-column shape. Both normalize to
    // (case, source, expected, optional exact source bytes).
    let rows: Vec<(String, String, Value, Option<Vec<u8>>)> =
        if fixture.trim_start().starts_with('[') {
            fixture_rows(fixture)
                .iter()
                .map(|row| {
                    (
                        row["case"].as_str().unwrap().to_owned(),
                        row["source"].as_str().unwrap().to_owned(),
                        row["expected"].clone(),
                        row.get("source_bytes").map(fixture_bytes),
                    )
                })
                .collect()
        } else {
            fixture
                .lines()
                .skip(1)
                .map(|line| {
                    let fields = line.split('\t').collect::<Vec<_>>();
                    assert!(matches!(fields.len(), 3 | 4));
                    (
                        fields[0].to_owned(),
                        fields[1].to_owned(),
                        serde_json::from_str(fields[2]).unwrap(),
                        (fields.len() == 4)
                            .then(|| fixture_bytes(&serde_json::json!({"hex": fields[3]}))),
                    )
                })
                .collect()
        };
    for (case, source, expected, source_bytes) in &rows {
        let run = PreparedRun::prepare(prepare(&schema, source.as_bytes())).unwrap();
        let content = source_bytes.as_deref().unwrap_or(content);
        let mut port = Port(0, Arc::from(content));
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
        .unwrap();
        drop(run);
        drop(attempt);
        assert_eq!(result.observations(), *expected, "{}", case);
        assert!(result.output().is_none());
        for _ in 0..2 {
            assert!(matches!(
                result.save(&mut port),
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
            ));
            assert_eq!(result.observations(), *expected);
        }
        assert_eq!(port.0, expected["source_reads"].as_array().unwrap().len());
        findings += expected["diagnostics"].as_array().unwrap().len();
        cases += 1;
    }
    assert_eq!((cases, findings), expected_counts);
}

pub(crate) fn independent_scalar_reports(fixture: &str, prefix: &str, expected_cases: usize) {
    pub(crate) use yamaa_adapters::{
        specification_report::{self, ArtifactPort, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port {
        pub(crate) reads: usize,
        pub(crate) saves: usize,
        pub(crate) expected: Vec<u8>,
    }
    impl SourcePort for Port {
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
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, ()> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
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
pub(crate) fn replay_source_selection_truth(truth: &str, prefix: &str, cases: usize) {
    pub(crate) use yamaa_adapters::{
        specification_report::{self, Identity},
        specification_run::{PreparedRun, SourcePort},
    };
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port {
        pub(crate) content: Arc<[u8]>,
        pub(crate) reads: usize,
        pub(crate) expected: Vec<u8>,
        pub(crate) saves: usize,
    }
    impl SourcePort for Port {
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
        fn capture(
            &mut self,
            source: &SourceDeclaration,
            origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
        ) -> Result<Arc<[u8]>, ()> {
            *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
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
