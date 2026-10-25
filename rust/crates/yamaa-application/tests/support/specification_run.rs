//! Original-document compiler and source-port integration contracts.
// Reexports share the authored fixture types across automatically discovered feature tests.
#![allow(unused_imports)]
use super::ports::NoFunctions;
pub(crate) use std::{path::Path, sync::Arc};
pub(crate) use yamaa_adapters::{
    csv_source,
    specification_source::{PreparedDocument, Source},
    yaml_decode::decode_yaml,
};
use yamaa_core::application_observer::Scope;
pub(crate) use yamaa_core::{
    expression::ErrorKind as EvaluationErrorKind,
    schema::{Document, DocumentNode as N},
    table::{TableAccess, TableSchema},
    value::Value,
};

pub(crate) use yamaa_core::dataset::{self, ResolvedPlan};

/// Rows of a converted readable JSON fixture (formerly hex-encoded TSV).
pub(crate) fn fixture_rows(truth: &str) -> Vec<yamaa_core::json::Value> {
    match yamaa_adapters::json_wire::decode(truth).unwrap() {
        yamaa_core::json::Value::Array(rows) => rows,
        _ => panic!("fixture rows are a JSON array"),
    }
}

/// Exact bytes of a fixture text cell; the two non-UTF-8 cells in the whole
/// fixture set are tagged as {"hex": "..."} objects instead of strings.
pub(crate) fn fixture_bytes(value: &yamaa_core::json::Value) -> Vec<u8> {
    if let Some(text) = value.as_str() {
        return text.as_bytes().to_vec();
    }
    let hex = value["hex"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

/// The findings of a refused specification as the run report renders them.
pub(crate) fn spec_findings(
    error: &yamaa_application::specification_run::Error,
    source: Option<&yamaa_core::specification::SourceDeclaration>,
) -> Option<Vec<yamaa_core::json::Value>> {
    yamaa_adapters::specification_diagnostics::findings(error, source)
        .expect("valid authored source finding context")
        .map(|findings| {
            findings
                .iter()
                .map(yamaa_adapters::report_storage::diagnostic_value)
                .collect()
        })
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
/// A port that holds only the entry: every parent and dictionary it names
/// is unavailable, and every path keeps the spelling its layer wrote.
pub(crate) struct EntryOnly;
impl yamaa_core::resolver::Resolver for EntryOnly {
    type Error = ();
    fn inspect(
        &mut self,
        _: yamaa_core::resolver::Name<'_>,
    ) -> Result<yamaa_core::resolver::Located, ()> {
        Err(())
    }
    fn capture(
        &mut self,
        _: yamaa_core::resolver::Name<'_>,
    ) -> Result<yamaa_core::resolver::Captured, ()> {
        Err(())
    }
    fn failure(
        &self,
        _: yamaa_core::resolver::Name<'_>,
        _: &(),
    ) -> Option<yamaa_core::resolver::Failure> {
        Some(yamaa_core::resolver::Failure::Resource(
            yamaa_core::resource::ResourceFailure::Missing,
        ))
    }
    fn rebase(
        &mut self,
        _: &yamaa_core::resolver::Located,
        _: &yamaa_core::resolver::Located,
        written: &str,
    ) -> Result<String, ()> {
        Ok(written.into())
    }
}
/// A specification without parents, prepared by the 1.0 closures the
/// package ships (#3030 D76).
pub(crate) fn prepare(source: &[u8]) -> PreparedDocument {
    try_prepare(source).unwrap_or_else(|error| panic!("{error}"))
}
/// [`prepare`], or why the specification is refused.
pub(crate) fn try_prepare(source: &[u8]) -> Result<PreparedDocument, String> {
    let source = Source {
        identity: "spec.yaml".into(),
        bytes: source.to_vec(),
    };
    yamaa_adapters::shipped_schema::prepare(source, "spec.yaml".into(), &mut EntryOnly)
        .map_err(|error| refusal(&error))
}
/// The findings a refused preparation reports, or the refusal itself when it
/// reports none.
fn refusal(error: &yamaa_adapters::specification_source::InheritanceError<()>) -> String {
    match yamaa_adapters::specification_diagnostics::inheritance_failure_ref(error) {
        Ok(rejection) => format!("{:?}", rejection.into_issues()),
        Err(_) => format!("{error:?}"),
    }
}
/// Compile a prepared specification without an environment or linked
/// producers, from the tree its front end lowered (#3030 D76).
pub(crate) fn compile(
    document: &PreparedDocument,
) -> Result<yamaa_core::specification::PreparedSpecification, yamaa_core::specification::PrepareError>
{
    yamaa_core::specification::PreparedSpecification::prepare_tree_linked(
        document.tree().clone(),
        document.document(),
        None,
        Default::default(),
    )
}
pub(crate) fn lower(document: &PreparedDocument, source: &TableSchema) -> ResolvedPlan {
    compile(document).unwrap().bind(source).unwrap()
}

pub(crate) fn independent_failed_reports(
    fixture: &str,
    prefix: &str,
    expected_counts: (usize, usize),
    content: &[u8],
) {
    pub(crate) use yamaa_application::{
        specification_report::{self, Identity},
        specification_run::PreparedRun,
    };
    pub(crate) use yamaa_core::json::Value;
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port(usize, Arc<[u8]>);
    use yamaa_core::resolver;
    impl resolver::Resolver for Port {
        type Error = ();
        fn inspect(&mut self, name: resolver::Name<'_>) -> Result<resolver::Located, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            Ok(resolver::Located {
                identity: source.path.clone(),
                display_path: source.path.clone(),
            })
        }
        fn capture(&mut self, name: resolver::Name<'_>) -> Result<resolver::Captured, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            self.0 += 1;
            Ok(resolver::Captured::resource(Arc::clone(&self.1), true))
        }
        fn failure(&self, _: resolver::Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
            None
        }
        fn rebase(
            &mut self,
            _: &resolver::Located,
            _: &resolver::Located,
            _: &str,
        ) -> Result<String, Self::Error> {
            panic!("an input is never rebased")
        }
    }
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
                        yamaa_adapters::json_wire::decode(fields[2]).unwrap(),
                        (fields.len() == 4)
                            .then(|| fixture_bytes(&yamaa_adapters::json!({"hex": fields[3]}))),
                    )
                })
                .collect()
        };
    for (case, source, expected, source_bytes) in &rows {
        let run = PreparedRun::prepare(prepare(source.as_bytes())).unwrap();
        let content = source_bytes.as_deref().unwrap_or(content);
        let mut port = Port(0, Arc::from(content));
        let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
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
            &[],
        )
        .unwrap();
        drop(run);
        drop(attempt);
        assert_eq!(result.observations(), *expected, "{}", case);
        assert!(result.output().is_none());
        // A failed build declares no report file, so a save writes nothing.
        assert!(result.files().is_empty(), "{}", case);
        assert_eq!(port.0, expected["source_reads"].as_array().unwrap().len());
        findings += expected["diagnostics"].as_array().unwrap().len();
        cases += 1;
    }
    assert_eq!((cases, findings), expected_counts);
}

pub(crate) fn independent_scalar_reports(fixture: &str, prefix: &str, expected_cases: usize) {
    pub(crate) use yamaa_application::{
        specification_report::{self, Identity},
        specification_run::PreparedRun,
    };
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port {
        pub(crate) reads: usize,
        pub(crate) expected: Vec<u8>,
    }
    use yamaa_core::resolver;
    impl resolver::Resolver for Port {
        type Error = ();
        fn inspect(&mut self, name: resolver::Name<'_>) -> Result<resolver::Located, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            Ok(resolver::Located {
                identity: source.path.clone(),
                display_path: source.path.clone(),
            })
        }
        fn capture(&mut self, name: resolver::Name<'_>) -> Result<resolver::Captured, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            self.reads += 1;
            Ok(resolver::Captured::resource(
                Arc::from(&b"ID\n1\n"[..]),
                true,
            ))
        }
        fn failure(&self, _: resolver::Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
            None
        }
        fn rebase(
            &mut self,
            _: &resolver::Located,
            _: &resolver::Located,
            _: &str,
        ) -> Result<String, Self::Error> {
            panic!("an input is never rebased")
        }
    }
    let mut cases = 0;
    for row in fixture_rows(fixture) {
        let mut port = Port {
            reads: 0,
            expected: fixture_bytes(&row["output"]),
        };
        let run =
            PreparedRun::prepare(prepare(row["source"].as_str().unwrap().as_bytes())).unwrap();
        let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
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
            &[],
        )
        .unwrap();
        drop(run);
        drop(attempt);
        let expected: yamaa_core::json::Value = row["expected"].clone();
        let mut unsaved = expected.clone();
        unsaved["artifacts"] = yamaa_adapters::json!([]);
        assert_eq!(
            result.observations(),
            unsaved,
            "{}",
            row["case"].as_str().unwrap()
        );
        if port.expected.is_empty() {
            // A failed literal build has no file to save.
            assert!(result.output().is_none());
            assert!(result.files().is_empty());
        } else {
            assert!(result.output().is_some());
            assert_eq!(result.files(), [("result.csv", &port.expected[..])]);
            assert_eq!(result.saved_observations().unwrap(), expected);
        }
        assert_eq!(result.observations(), unsaved);
        assert_eq!(port.reads, 1);
        cases += 1;
    }
    assert_eq!(cases, expected_cases);
}
pub(crate) fn replay_source_selection_truth(truth: &str, prefix: &str, cases: usize) {
    pub(crate) use yamaa_application::{
        specification_report::{self, Identity},
        specification_run::PreparedRun,
    };
    pub(crate) use yamaa_core::specification::SourceDeclaration;
    pub(crate) struct Port {
        pub(crate) content: Arc<[u8]>,
        pub(crate) reads: usize,
        pub(crate) expected: Vec<u8>,
    }
    use yamaa_core::resolver;
    impl resolver::Resolver for Port {
        type Error = ();
        fn inspect(&mut self, name: resolver::Name<'_>) -> Result<resolver::Located, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            Ok(resolver::Located {
                identity: source.path.clone(),
                display_path: source.path.clone(),
            })
        }
        fn capture(&mut self, name: resolver::Name<'_>) -> Result<resolver::Captured, Self::Error> {
            let resolver::Name::Input(source) = name else {
                panic!("unexpected request {name:?}")
            };
            assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
            self.reads += 1;
            Ok(resolver::Captured::resource(
                Arc::clone(&self.content),
                true,
            ))
        }
        fn failure(&self, _: resolver::Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
            None
        }
        fn rebase(
            &mut self,
            _: &resolver::Located,
            _: &resolver::Located,
            _: &str,
        ) -> Result<String, Self::Error> {
            panic!("an input is never rebased")
        }
    }
    let mut count = 0;
    for row in fixture_rows(truth) {
        let case = row["case"].as_str().unwrap();
        let raw = fixture_bytes(&row["source"]);
        let run = PreparedRun::prepare(prepare(&raw)).unwrap();
        let mut port = Port {
            content: fixture_bytes(&row["input"]).into(),
            reads: 0,
            expected: fixture_bytes(&row["artifact"]),
        };
        let attempt = run.execute_with_ports(&mut NoFunctions, &mut port, Scope::default());
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
            &[],
        )
        .unwrap_or_else(|error| {
            panic!(
                "{}: {error:?}; execution: {:?}; outcome: {:?}",
                case,
                attempt.dataset.result.as_ref().err(),
                attempt
                    .dataset
                    .result
                    .as_ref()
                    .ok()
                    .map(|execution| execution.result.as_ref().err())
            )
        });
        let expected: yamaa_core::json::Value = row["expected"].clone();
        // Require the captured row-filter citations without changing any finding.
        let cited = match (prefix, case) {
            (
                "row-filter",
                "grouped_source_phase" | "grouped_unknown_phase" | "group_and_filter_order",
            ) => Some("rows.grouped.filter"),
            ("row-filter", "record_unknown_phase") => Some("rows.records0.filter"),
            _ => None,
        };
        if let Some(cited) = cited {
            for diagnostics in ["diagnostics", "nodes"] {
                let values = if diagnostics == "nodes" {
                    &expected["nodes"][0]["diagnostics"]
                } else {
                    &expected["diagnostics"]
                };
                let mut phase_boundaries = 0;
                for finding in values.as_array().unwrap() {
                    if finding["condition"] == "phase_boundary" {
                        assert_eq!(finding["rule"], yamaa_adapters::json!("ROW-6"));
                        assert_eq!(finding["spec_paths"], yamaa_adapters::json!([cited]));
                        phase_boundaries += 1;
                    }
                }
                assert_eq!(phase_boundaries, 1);
            }
        }
        let mut unsaved = expected.clone();
        unsaved["artifacts"] = yamaa_adapters::json!([]);
        assert_eq!(result.observations(), unsaved, "{}", case);
        if port.expected.is_empty() {
            // A failed filter result has no file to save.
            assert!(result.output().is_none());
            assert!(result.files().is_empty(), "{}", case);
        } else {
            assert!(result.output().is_some());
            assert_eq!(
                result.files(),
                [("result.csv", &port.expected[..])],
                "{}",
                case
            );
            assert_eq!(result.saved_observations().unwrap(), expected, "{}", case);
        }
        assert_eq!(port.reads, 1);
        count += 1;
    }
    assert_eq!(count, cases);
}
/// [`prepare`], for a specification the port names `identity`.
pub(crate) fn prepare_as(identity: &str, source: &[u8]) -> PreparedDocument {
    let source = Source {
        identity: identity.into(),
        bytes: source.to_vec(),
    };
    yamaa_adapters::shipped_schema::prepare(source, identity.into(), &mut EntryOnly)
        .unwrap_or_else(|error| panic!("{}", refusal(&error)))
}
