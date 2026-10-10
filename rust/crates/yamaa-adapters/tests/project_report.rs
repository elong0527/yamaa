#[path = "support/project_schemas.rs"]
mod schemas;
use std::{collections::BTreeMap, convert::Infallible, path::PathBuf, sync::Arc};
use yamaa_adapters::{
    project_report,
    project_run::PreparedRun,
    project_source::{self, CapturePort, OwnedEnvironment, Reply, Request},
    specification_report::Identity,
    specification_source::Source,
};
use yamaa_core::{
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_environment::LockReference,
    project_function::Language,
    specification::SourceDeclaration,
    value::Value,
};
use yamaa_engine::{
    function_invocation::{Argument, HostError},
    project_activation::ActivationPort,
    specification_run::SourcePort,
};
fn source(identity: &str, bytes: &[u8]) -> Source {
    Source {
        identity: identity.into(),
        bytes: bytes.into(),
    }
}
struct NoCapture;
impl CapturePort for NoCapture {
    type Error = Infallible;
    fn capture(&mut self, _: Request<'_>) -> Result<Reply, Infallible> {
        panic!("inline environment has no resource authority")
    }
}
fn environment(extra: &str) -> OwnedEnvironment {
    project_source::prepare(
        schemas::environment_schema(),
        source(
            "environment.yaml",
            format!("schema_version: '1.0'\nlanguage: python\n{extra}").as_bytes(),
        ),
        Language::Python,
        &mut NoCapture,
        Default::default(),
    )
    .unwrap()
    .into_owned()
}
struct NoActivation;
impl ActivationPort for NoActivation {
    type Error = Infallible;
    type Handle = ();
    fn verify_lock(
        &mut self,
        _: Language,
        _: &LockReference,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), Infallible> {
        panic!("no selected functions")
    }
    fn bind(
        &mut self,
        _: &ProjectFunctionIdentity,
        _: &LogicalSignature,
    ) -> Result<(), Infallible> {
        panic!("no selected functions")
    }
    fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Infallible>> {
        panic!("no selected functions")
    }
}
struct Study {
    root: PathBuf,
    cached: BTreeMap<String, Arc<[u8]>>,
}
impl SourcePort for Study {
    type Error = Infallible;
    fn capture_reads(&self) -> usize {
        self.cached.len()
    }
    fn capture(
        &mut self,
        declaration: &SourceDeclaration,
        maximum: usize,
    ) -> Result<Arc<[u8]>, Infallible> {
        let bytes = self
            .cached
            .entry(declaration.path.clone())
            .or_insert_with(|| {
                std::fs::read(self.root.join(&declaration.path))
                    .unwrap()
                    .into()
            });
        assert!(bytes.len() <= maximum);
        Ok(Arc::clone(bytes))
    }
}
#[derive(Default)]
struct Publisher(Vec<(String, Vec<u8>)>);
impl yamaa_engine::specification_output::ArtifactPort for Publisher {
    type Error = Infallible;
    fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), Infallible> {
        self.0.push((path.into(), bytes.into()));
        Ok(())
    }
}
fn id(example: &str) -> Identity<'_> {
    Identity {
        runtime: "python",
        runtime_version: "fixture-runtime",
        engine_version: "fixture-engine",
        example,
        specification: "spec.yaml",
        base_directory: ".",
    }
}

#[test]
fn whole_project_reports_match_independent_original_document_truth_and_exact_saved_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let truth = root.join("tests/fixtures/specs");
    let benchmarks = root.join("../../..").join("benchmarks");
    for example in [
        "adam-adlb-ordered-sum",
        "schema-lookup",
        "schema-window-functions",
        "negative-zero-division",
        "negative-integer-overflow",
        "negative-implausible-age",
        "negative-invalid-sex",
        "negative-not-missing-age",
        "negative-sex-code",
        "negative-matches-bad-pattern",
        "negative-paired-dates",
        "negative-source-missing-field",
        "negative-source-trivial-filter",
        "negative-formula-flag",
        "negative-row-aggregate",
        "negative-row-no-prior",
    ] {
        let case = benchmarks.join(example);
        let document = schemas::domain_schema()
            .prepare_standalone(source(
                "spec.yaml",
                &std::fs::read(case.join("spec.yaml")).unwrap(),
            ))
            .unwrap();
        let run = PreparedRun::prepare(document, environment("")).unwrap();
        let mut study = Study {
            root: case.clone(),
            cached: BTreeMap::new(),
        };
        let attempt = run.execute_with_ports(&mut NoActivation, &mut study);
        let result = project_report::build_result(&run, &attempt, id(example), &[]).unwrap();
        let reads = study.capture_reads();
        let mut observed = result.observations();
        assert_eq!(
            observed
                .as_object_mut()
                .unwrap()
                .remove("activation")
                .unwrap(),
            serde_json::json!({"lock":"not_requested","bindings":[],"tests":[]})
        );
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(truth.join(format!("{example}.json"))).unwrap())
                .unwrap();
        let mut publisher = Publisher::default();
        if expected["outcome"] == "success" {
            let saved = result.save(&mut publisher).unwrap();
            observed["artifacts"] = saved["artifacts"].clone();
            assert_eq!(publisher.0.len(), 1);
            assert_eq!(
                publisher.0[0].1,
                std::fs::read(case.join("expected").join(&publisher.0[0].0)).unwrap(),
                "{example}"
            );
        } else {
            assert!(matches!(
                result.save(&mut publisher),
                Err(yamaa_engine::specification_output::SaveError::FailedBuild)
            ));
            assert!(publisher.0.is_empty());
            assert_eq!(result.output(), None);
        }
        assert_eq!(observed, expected, "{example}");
        assert_eq!(study.capture_reads(), reads);
    }
}

#[test]
fn full_codelist_values_and_keys_survive_failed_output_and_repeated_reporting() {
    let raw="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX}}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
    let document = schemas::domain_schema()
        .prepare_standalone(source("spec.yaml", raw.as_bytes()))
        .unwrap();
    let run=PreparedRun::prepare(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
    let mut study = Study {
        root: PathBuf::new(),
        cached: BTreeMap::from([(
            "input.csv".into(),
            Arc::from(b"ID,CODE\n9223372036854775807,X\n-9223372036854775808,Z\n".as_slice()),
        )]),
    };
    let attempt = run.execute_with_ports(&mut NoActivation, &mut study);
    let result = project_report::build_result(&run, &attempt, id("codelist"), &[]).unwrap();
    assert_eq!(result.output(), None);
    assert_eq!(result.issues().len(), 1);
    let context: serde_json::Value = serde_json::from_str(&result.issues()[0].context).unwrap();
    assert_eq!(
        context,
        serde_json::json!({"column":"CODE","codelist":"SEX","failure_count":2,"keys":[{"ID":i64::MAX},{"ID":i64::MIN}],"values":["X","Z"]})
    );
    let observed = result.observations();
    assert_eq!(observed["verifications"].as_array().unwrap().len(), 1);
    assert_eq!(
        observed["verifications"][0]["failure"]["log_context"],
        context
    );
    assert_eq!(
        project_report::build_result(&run, &attempt, id("codelist"), &[])
            .unwrap()
            .observations(),
        observed
    );
    let mut publisher = Publisher::default();
    assert!(matches!(
        result.save(&mut publisher),
        Err(yamaa_engine::specification_output::SaveError::FailedBuild)
    ));
    assert!(publisher.0.is_empty());
    assert_eq!(study.cached.len(), 1);
}

#[test]
fn original_inspection_and_capture_failures_keep_complete_findings_and_counters() {
    #[derive(Debug)]
    struct Failure {
        original: std::rc::Rc<()>,
        classified: bool,
    }
    struct Resources {
        inspect: bool,
        classified: bool,
        original: std::rc::Rc<()>,
        captures: usize,
        inspections: usize,
    }
    impl SourcePort for Resources {
        type Error = Failure;
        fn inspect(&mut self, _: &SourceDeclaration) -> Result<(), Failure> {
            self.inspections += 1;
            if self.inspect {
                Err(Failure {
                    original: std::rc::Rc::clone(&self.original),
                    classified: self.classified,
                })
            } else {
                Ok(())
            }
        }
        fn resource_failure(
            &self,
            failure: &Failure,
        ) -> Option<yamaa_core::resource::ResourceFailure> {
            failure
                .classified
                .then_some(yamaa_core::resource::ResourceFailure::Missing)
        }
        fn capture_reads(&self) -> usize {
            self.captures
        }
        fn capture(&mut self, _: &SourceDeclaration, _: usize) -> Result<Arc<[u8]>, Failure> {
            Err(Failure {
                original: std::rc::Rc::clone(&self.original),
                classified: self.classified,
            })
        }
    }
    let raw="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: absent.csv}\ncolumns: [{name: ID, type: str, derivation: SRC.ID}]\noutput: {path: output.csv, columns: [ID]}\n";
    let document = schemas::domain_schema()
        .prepare_standalone(source("spec.yaml", raw.as_bytes()))
        .unwrap();
    let run = PreparedRun::prepare(document, environment("")).unwrap();
    for inspect in [false, true] {
        for classified in [false, true] {
            let mut resources = Resources {
                inspect,
                classified,
                original: std::rc::Rc::new(()),
                captures: 0,
                inspections: 0,
            };
            let attempt = run.execute_with_ports(&mut NoActivation, &mut resources);
            let candidate =
                project_report::build_result(&run, &attempt, id("failed-resource"), &[]);
            if classified {
                let result = candidate.unwrap();
                assert_eq!(result.issues().len(), 1);
                let report = result.observations();
                assert_eq!(
                    report["diagnostics"],
                    serde_json::json!([{"phase":"validation","condition":"resource_path_missing","requirement":"REQ-0785","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":"absent.csv"}}])
                );
                assert_eq!(
                    report["source_reads"],
                    if inspect {
                        serde_json::json!([])
                    } else {
                        serde_json::json!([{"base_directory":".","path":"absent.csv","outcome":"failure","condition":"resource_path_missing","snapshots_created":0}])
                    }
                );
                assert_eq!(result.output(), None);
                let mut publisher = Publisher::default();
                assert!(matches!(
                    result.save(&mut publisher),
                    Err(yamaa_engine::specification_output::SaveError::FailedBuild)
                ));
                assert!(publisher.0.is_empty());
            } else {
                let project_report::Error::Source(held) = candidate.unwrap_err() else {
                    panic!("original source boundary")
                };
                assert!(std::ptr::eq(
                    held,
                    attempt.dataset.result.as_ref().unwrap_err()
                ));
                let payload = match held {
                    yamaa_engine::specification_run::PortError::Capture(error) => error,
                    yamaa_engine::specification_run::PortError::Inspect(failures) => {
                        &failures[0].error
                    }
                    _ => panic!("source failure"),
                };
                assert!(std::rc::Rc::ptr_eq(&payload.original, &resources.original));
            }
            assert_eq!(resources.captures, 0);
            assert_eq!(resources.inspections, 1);
        }
    }
}

#[test]
fn completed_codelist_check_survives_a_later_verification_declaration_failure() {
    let raw="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX}}\n  - {name: LATE, type: str, derivation: SRC.CODE, verifications: [{matches: {pattern: '('}}]}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
    let document = schemas::domain_schema()
        .prepare_standalone(source("spec.yaml", raw.as_bytes()))
        .unwrap();
    let run=PreparedRun::prepare(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
    let mut study = Study {
        root: PathBuf::new(),
        cached: BTreeMap::from([(
            "input.csv".into(),
            Arc::from(b"ID,CODE\n9223372036854775807,F\n-9223372036854775808,M\n".as_slice()),
        )]),
    };
    let attempt = run.execute_with_ports(&mut NoActivation, &mut study);
    let result = project_report::build_result(&run, &attempt, id("codelist-prefix"), &[]).unwrap();
    let observed = result.observations();
    assert_eq!(observed["outcome"], "failure");
    assert_eq!(observed["verifications"].as_array().unwrap().len(), 1);
    assert_eq!(
        observed["verifications"][0]["spec_path"],
        "columns.CODE.submission.codelist"
    );
    assert_eq!(observed["verifications"][0]["evaluated_count"], 2);
    assert_eq!(
        observed["verifications"][0]["failure"],
        serde_json::Value::Null
    );
    assert_eq!(
        observed["diagnostics"],
        serde_json::json!([{"phase":"validation","condition":"invalid_regex","requirement":"REQ-0827","spec_paths":["columns.LATE.verifications[0].matches.pattern"],"context":{"pattern":"("}}])
    );
    assert_eq!(result.output(), None);
    let mut publisher = Publisher::default();
    assert!(matches!(
        result.save(&mut publisher),
        Err(yamaa_engine::specification_output::SaveError::FailedBuild)
    ));
    assert!(publisher.0.is_empty());
}
