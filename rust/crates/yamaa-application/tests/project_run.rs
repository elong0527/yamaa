#![cfg(feature = "yaml")]
//! Project contracts: project report and project run.
mod project {
    mod project_report {
        use crate::support::ports::NoFunctions;
        use std::{collections::BTreeMap, convert::Infallible, path::PathBuf, sync::Arc};
        use yamaa_adapters::{
            document::Closures,
            project_source::{self, OwnedEnvironment},
            specification_source::Source,
        };
        use yamaa_application::{specification_report::Identity, specification_run::PreparedRun};
        use yamaa_core::application_observer::Scope;
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{Captured, Failure, Located, Name, Resolver};
        fn source(identity: &str, bytes: &[u8]) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.into(),
            }
        }
        struct NoCapture;
        impl Resolver for NoCapture {
            type Error = Infallible;
            fn inspect(&mut self, _: Name<'_>) -> Result<Located, Infallible> {
                panic!("inline project has no resource authority")
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Infallible> {
                panic!("inline project has no resource authority")
            }
            fn failure(&self, _: Name<'_>, error: &Infallible) -> Option<Failure> {
                match *error {}
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Infallible> {
                panic!("inline project has no resource authority")
            }
        }
        /// A project with inline study metadata; these runs call no project function.
        fn environment(extra: &str) -> OwnedEnvironment {
            project_source::prepare(
                Closures::shipped().unwrap(),
                source(
                    "project.yaml",
                    format!("yamaa: '1.0'\ntype: project\n{extra}").as_bytes(),
                ),
                Language::Python,
                &mut NoCapture,
            )
            .unwrap()
            .into_owned()
        }
        struct Study {
            root: PathBuf,
            cached: BTreeMap<String, Arc<[u8]>>,
        }
        impl Study {
            /// How many files the run read.
            fn reads(&self) -> usize {
                self.cached.len()
            }
        }
        use yamaa_core::resolver;
        impl resolver::Resolver for Study {
            type Error = Infallible;
            fn inspect(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Located, Self::Error> {
                let resolver::Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                Ok(resolver::Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Captured, Self::Error> {
                let resolver::Name::Input(declaration) = name else {
                    panic!("unexpected request {name:?}")
                };
                let before = self.cached.len();
                let bytes = self
                    .cached
                    .entry(declaration.path.clone())
                    .or_insert_with(|| {
                        std::fs::read(self.root.join(&declaration.path))
                            .unwrap()
                            .into()
                    });
                Ok(resolver::Captured::resource(
                    Arc::clone(bytes),
                    self.cached.len() != before,
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
            // Seam: already data: each benchmark against its authored report under
            // tests/fixtures/specs, its observations and saved bytes included.
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
                // Each benchmark in the language it is written in (#3422).
                let document = crate::support::specification_run::prepare(
                    &std::fs::read(case.join("spec.yaml")).unwrap(),
                );
                let run = PreparedRun::prepare_with_environment(document, environment("")).unwrap();
                let mut study = Study {
                    root: case.clone(),
                    cached: BTreeMap::new(),
                };
                let attempt =
                    run.execute_with_ports(&mut NoFunctions, &mut study, Scope::default());
                let result = yamaa_application::specification_report::build_result(
                    &run,
                    &attempt,
                    id(example),
                    &[],
                )
                .unwrap();
                let reads = study.reads();
                let mut observed = result.observations();
                assert_eq!(
                    observed
                        .as_object_mut()
                        .unwrap()
                        .remove("activation")
                        .unwrap(),
                    yamaa_adapters::json!({"lock":"not_requested","bindings":[],"tests":[]})
                );
                let expected: yamaa_core::json::Value = yamaa_adapters::json_wire::decode_slice(
                    &std::fs::read(truth.join(format!("{example}.json"))).unwrap(),
                )
                .unwrap();
                if expected["outcome"] == "success" {
                    observed["artifacts"] =
                        result.saved_observations().unwrap()["artifacts"].clone();
                    let files = result.files();
                    assert_eq!(files.len(), 1);
                    assert_eq!(
                        files[0].1,
                        std::fs::read(case.join("expected").join(files[0].0)).unwrap(),
                        "{example}"
                    );
                } else {
                    assert!(result.files().is_empty());
                    assert_eq!(result.output(), None);
                }
                assert_eq!(observed, expected, "{example}");
                assert_eq!(study.reads(), reads);
            }
        }

        #[test]
        fn full_codelist_values_and_keys_survive_failed_output_and_repeated_reporting() {
            // Seam: the verification record of the report, built twice from
            // one attempt, its absent output and the study's reads. The issue
            // is project_run.json's
            // a_codelist_failure_keeps_every_value_and_key_and_fails_the_output.
            let raw="yamaa: '1.0'\ntype: spec\nmodel: ADaM\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX, origins: [{type: Assigned}]}}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
            let document =
                crate::support::specification_run::prepare_as("spec.yaml", raw.as_bytes());
            let run=PreparedRun::prepare_with_environment(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
            let mut study = Study {
                root: PathBuf::new(),
                cached: BTreeMap::from([(
                    "input.csv".into(),
                    Arc::from(
                        b"ID,CODE\n9223372036854775807,X\n-9223372036854775808,Z\n".as_slice(),
                    ),
                )]),
            };
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut study, Scope::default());
            let result = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                id("codelist"),
                &[],
            )
            .unwrap();
            assert_eq!(result.output(), None);
            let context = yamaa_core::json::Value::Object(result.issues()[0].context.clone());
            let observed = result.observations();
            assert_eq!(observed["verifications"].as_array().unwrap().len(), 1);
            assert_eq!(
                observed["verifications"][0]["failure"]["log_context"],
                context
            );
            assert_eq!(
                yamaa_application::specification_report::build_result(
                    &run,
                    &attempt,
                    id("codelist"),
                    &[]
                )
                .unwrap()
                .observations(),
                observed
            );
            assert!(result.files().is_empty());
            assert_eq!(study.cached.len(), 1);
        }

        #[test]
        fn original_inspection_and_capture_failures_keep_complete_findings_and_counters() {
            // Seam: a resolver double whose inspection or capture fails, classified or not, its
            // payload by address and its call counts.
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
            use yamaa_core::resolver;
            impl resolver::Resolver for Resources {
                type Error = Failure;
                fn inspect(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Located, Self::Error> {
                    let resolver::Name::Input(source) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    self.inspections += 1;
                    if self.inspect {
                        Err(Failure {
                            original: std::rc::Rc::clone(&self.original),
                            classified: self.classified,
                        })
                    } else {
                        Ok(resolver::Located {
                            identity: source.path.clone(),
                            display_path: source.path.clone(),
                        })
                    }
                }
                fn capture(
                    &mut self,
                    name: resolver::Name<'_>,
                ) -> Result<resolver::Captured, Self::Error> {
                    let resolver::Name::Input(_) = name else {
                        panic!("unexpected request {name:?}")
                    };
                    Err(Failure {
                        original: std::rc::Rc::clone(&self.original),
                        classified: self.classified,
                    })
                }
                fn failure(
                    &self,
                    _: resolver::Name<'_>,
                    failure: &Self::Error,
                ) -> Option<resolver::Failure> {
                    failure
                        .classified
                        .then_some(yamaa_core::resource::ResourceFailure::Missing)
                        .map(resolver::Failure::Resource)
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
            let raw="yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: absent.csv}\ncolumns: [{name: ID, type: str, derivation: SRC.ID}]\noutput: {path: output.csv, columns: [ID]}\n";
            let document =
                crate::support::specification_run::prepare_as("spec.yaml", raw.as_bytes());
            let run = PreparedRun::prepare_with_environment(document, environment("")).unwrap();
            for inspect in [false, true] {
                for classified in [false, true] {
                    let mut resources = Resources {
                        inspect,
                        classified,
                        original: std::rc::Rc::new(()),
                        captures: 0,
                        inspections: 0,
                    };
                    let attempt =
                        run.execute_with_ports(&mut NoFunctions, &mut resources, Scope::default());
                    let candidate = yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        id("failed-resource"),
                        &[],
                    );
                    if classified {
                        let result = candidate.unwrap();
                        assert_eq!(result.issues().len(), 1);
                        let report = result.observations();
                        assert_eq!(
                            report["diagnostics"],
                            yamaa_adapters::json!([{"phase":"validation","condition":"resource_path_missing","rule":"FILE-7","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":"absent.csv"}}])
                        );
                        assert_eq!(
                            report["source_reads"],
                            if inspect {
                                yamaa_adapters::json!([])
                            } else {
                                yamaa_adapters::json!([{"base_directory":".","path":"absent.csv","outcome":"failure","condition":"resource_path_missing","snapshots_created":0}])
                            }
                        );
                        assert_eq!(result.output(), None);
                        assert!(result.files().is_empty());
                    } else {
                        // An unclassified failure refuses the result without
                        // a finding; the attempt keeps the original payload.
                        assert!(matches!(
                            candidate.unwrap_err(),
                            yamaa_application::specification_report::Refusal::Report(_)
                        ));
                        let payload = match attempt.dataset.result.as_ref().unwrap_err() {
                            yamaa_core::specification_run::PortError::Capture(error) => error,
                            yamaa_core::specification_run::PortError::Inspect(failures) => {
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
            // Seam: the verification record of the report and its absent
            // output. The issue is project_run.json's
            // a_completed_codelist_check_leaves_a_later_verification_declaration_s_failure_alone.
            let raw="yamaa: '1.0'\ntype: spec\nmodel: ADaM\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX, origins: [{type: Assigned}]}}\n  - {name: LATE, type: str, derivation: SRC.CODE, verifications: [{matches: {pattern: '('}}]}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
            let document =
                crate::support::specification_run::prepare_as("spec.yaml", raw.as_bytes());
            let run=PreparedRun::prepare_with_environment(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
            let mut study = Study {
                root: PathBuf::new(),
                cached: BTreeMap::from([(
                    "input.csv".into(),
                    Arc::from(
                        b"ID,CODE\n9223372036854775807,F\n-9223372036854775808,M\n".as_slice(),
                    ),
                )]),
            };
            let attempt = run.execute_with_ports(&mut NoFunctions, &mut study, Scope::default());
            let result = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                id("codelist-prefix"),
                &[],
            )
            .unwrap();
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
                yamaa_core::json::Value::Null
            );
            assert_eq!(result.output(), None);
            assert!(result.files().is_empty());
        }
    }

    mod project_run {
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_adapters::{
            document::Closures,
            project_source::{self, Kind, OwnedEnvironment},
            specification_source::{PreparedDocument, Source},
        };
        use yamaa_application::specification_run::{BoundaryFailure, PreparedRun};
        use yamaa_core::application_observer::Scope;
        use yamaa_core::resolver::{Captured, File, Located, Name, Resolver};
        use yamaa_core::{
            dataset::ExecutionError,
            function_invocation::{Argument, FailureKind, HostError},
            host_functions::{HostFunctions, Lock},
            project_activation::{Failure, LockObservation, TestOutcome},
            specification_run::PortError,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_function::Language,
            table::{TableAccess, ValueRef},
            value::Value,
        };

        fn source(identity: &str, bytes: &[u8]) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.to_vec(),
            }
        }
        /// The project lists three function files and holds the Python lock,
        /// each path relative to the project file (D8).
        const PROJECT: &[u8] = b"yamaa: '1.0'\ntype: project\nfunctions: [functions/unused.yaml, functions/id.yaml, functions/pair.yaml]\nenvironments: {python: {lock: ../uv.lock}}\n";
        /// The function files the project lists, by the path it writes: each
        /// defines one function with its Python callable (FUNC-4, FUNC-8).
        const FUNCTIONS: [(&str, &str); 3] = [
            (
                "functions/unused.yaml",
                r#"yamaa: '1.0'
type: function
name: unused
description: Uncalled definition.
params: []
returns: int
implementations: {python: project.unused}
tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]
"#,
            ),
            (
                "functions/id.yaml",
                r#"yamaa: '1.0'
type: function
name: id
description: Integer identity.
params: [{name: x, type: int}]
returns: int
implementations: {python: project.id}
tests:
  - {id: ordinary, covers: [normal], args: {x: 7}, result: 7}
  - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775808}
  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}
"#,
            ),
            (
                "functions/pair.yaml",
                r#"yamaa: '1.0'
type: function
name: pair
description: Integer sum of a weight and an own value.
params: [{name: weight, type: int}, {name: own, type: int}]
returns: int
implementations: {python: project.pair}
tests:
  - {id: ordinary, covers: [normal], args: {weight: 7, own: 3}, result: 10}
  - {id: boundary, covers: [boundary], args: {weight: 0, own: 0}, result: 0}
  - {id: missing-weight, covers: ['short-circuit-missing:weight'], args: {weight: null, own: 1}, result: null}
  - {id: missing-own, covers: ['short-circuit-missing:own'], args: {weight: 1, own: null}, result: null}
"#,
            ),
        ];
        #[derive(Default)]
        struct EnvironmentCapture {
            held: Vec<u8>,
        }
        impl EnvironmentCapture {
            /// The held source the double serves for the file a project names:
            /// a listed function file, or the lock. Every path resolves from
            /// the project file (D8).
            fn answer(
                &mut self,
                kind: Kind,
                declaring: &str,
                written: &str,
            ) -> Result<Source, std::convert::Infallible> {
                assert_eq!(declaring, "study/project.yaml");
                match kind {
                    Kind::Function => {
                        let (_, text) = FUNCTIONS
                            .iter()
                            .find(|(path, _)| *path == written)
                            .unwrap_or_else(|| panic!("unexpected function file {written}"));
                        Ok(source(&format!("study/{written}"), text.as_bytes()))
                    }
                    Kind::Lock => {
                        assert_eq!(written, "../uv.lock");
                        Ok(source(
                            "held/uv.lock",
                            crate::support::ports::UV_LOCK.as_bytes(),
                        ))
                    }
                    kind => panic!("unexpected {kind:?} capture"),
                }
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for EnvironmentCapture {
            type Error = std::convert::Infallible;
            fn inspect(&mut self, name: Name<'_>) -> Result<Located, Self::Error> {
                let Name::Written {
                    file: File::Project(kind),
                    declaring,
                    written,
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
                let source = self.answer(kind, declaring, written)?;
                self.held = source.bytes;
                Ok(Located {
                    display_path: source.identity.clone(),
                    identity: source.identity,
                })
            }
            fn capture(&mut self, _: Name<'_>) -> Result<Captured, Self::Error> {
                Ok(Captured::resource(
                    std::mem::take(&mut self.held).into(),
                    true,
                ))
            }
            fn failure(&self, _: Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
                None
            }
            fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Self::Error> {
                panic!("a project file is never rebased")
            }
        }
        fn environment() -> OwnedEnvironment {
            project_source::prepare(
                Closures::shipped().unwrap(),
                source("study/project.yaml", PROJECT),
                Language::Python,
                &mut EnvironmentCapture::default(),
            )
            .unwrap()
            .into_owned()
        }
        fn document(function: Option<&str>) -> PreparedDocument {
            let derivation = function.map_or_else(
                || "SRC.ID".into(),
                |name| format!("{{function: {{name: {name}, args: {{x: SRC.ID}}}}}}"),
            );
            let bytes = format!(
        "yamaa: '1.0'\ntype: spec\ndomain: TEST\ninput: {{SRC: {{path: input.csv, types: {{ID: int}}}}}}\nkeys: [ID]\ncolumns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: VALUE, type: int, derivation: {derivation}}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n"
    );
            crate::support::specification_run::prepare_as("study/domain.yaml", bytes.as_bytes())
        }

        #[derive(Debug, PartialEq)]
        struct Payload {
            original: Rc<()>,
            interrupt: bool,
        }
        type Trace = Rc<RefCell<Vec<&'static str>>>;
        #[derive(Clone, Copy)]
        enum Mode {
            Pass,
            LockFailure,
            BindFailure,
            CaseFailure,
            CaseMismatch,
            CaseInterrupt,
            CaseUnwind,
            LiveFailure,
        }
        struct Activation {
            trace: Trace,
            original: Rc<()>,
            unwind: Arc<()>,
            mode: Mode,
        }
        impl Activation {
            fn payload(&self, interrupt: bool) -> Payload {
                Payload {
                    original: Rc::clone(&self.original),
                    interrupt,
                }
            }
        }
        impl HostFunctions for Activation {
            type Handle = ();
            type Error = Payload;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: Lock<'_>,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.reference.written, "../uv.lock");
                assert_eq!(functions.len(), 1);
                assert_eq!(functions[0].name, "id");
                self.trace.borrow_mut().push("lock");
                if matches!(self.mode, Mode::LockFailure) {
                    Err(self.payload(false))
                } else {
                    Ok(())
                }
            }
            fn bind(
                &mut self,
                identity: &ProjectFunctionIdentity,
                signature: &LogicalSignature,
            ) -> Result<(), Payload> {
                assert_eq!(identity.call, "project.id");
                let names = signature
                    .parameters()
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(names, ["x"]);
                self.trace.borrow_mut().push("bind");
                if matches!(self.mode, Mode::BindFailure) {
                    Err(self.payload(false))
                } else {
                    Ok(())
                }
            }
            fn is_interrupt(&self, error: &Payload) -> bool {
                error.interrupt
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Payload>> {
                let [Argument {
                    name: "x",
                    value: ValueRef::Int(value),
                }] = args
                else {
                    panic!("an exact integer argument; missing cases are engine-short-circuited")
                };
                self.trace
                    .borrow_mut()
                    .push(if *value == 1 { "live" } else { "case" });
                match (self.mode, *value) {
                    (Mode::CaseMismatch, 7) => Ok(Value::Int(8)),
                    (Mode::CaseFailure, i64::MIN) | (Mode::LiveFailure, 1) => {
                        Err(HostError::Raised(self.payload(false)))
                    }
                    (Mode::CaseInterrupt, i64::MIN) => Err(HostError::Raised(self.payload(true))),
                    (Mode::CaseUnwind, i64::MIN) => std::panic::panic_any(Arc::clone(&self.unwind)),
                    _ => Ok(Value::Int(*value)),
                }
            }
        }
        struct Study {
            trace: Trace,
            bytes: Arc<[u8]>,
            captured: bool,
            inspections: usize,
        }
        impl Study {
            /// How many files the run read.
            fn reads(&self) -> usize {
                usize::from(self.captured)
            }
        }
        use yamaa_core::resolver;
        impl resolver::Resolver for Study {
            type Error = std::convert::Infallible;
            fn inspect(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Located, Self::Error> {
                let resolver::Name::Input(source) = name else {
                    panic!("unexpected request {name:?}")
                };
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                self.inspections += 1;
                Ok(resolver::Located {
                    identity: source.path.clone(),
                    display_path: source.path.clone(),
                })
            }
            fn capture(
                &mut self,
                name: resolver::Name<'_>,
            ) -> Result<resolver::Captured, Self::Error> {
                let resolver::Name::Input(_) = name else {
                    panic!("unexpected request {name:?}")
                };
                let before = usize::from(self.captured);
                self.trace.borrow_mut().push("capture");
                self.captured = true;
                Ok(resolver::Captured::resource(
                    Arc::clone(&self.bytes),
                    usize::from(self.captured) != before,
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
        fn ports() -> (Activation, Study) {
            let trace = Rc::new(RefCell::new(vec![]));
            (
                Activation {
                    trace: Rc::clone(&trace),
                    original: Rc::new(()),
                    unwind: Arc::new(()),
                    mode: Mode::Pass,
                },
                Study {
                    trace,
                    bytes: Arc::from(b"ID\n1\n".as_slice()),
                    captured: false,
                    inspections: 0,
                },
            )
        }

        #[test]
        fn prepared_and_rejected_runs_retain_original_documents_lock_and_metadata_allocations() {
            // Seam: the allocations a prepared or rejected run holds, by address.
            for name in ["id", "unknown"] {
                let document = document(Some(name));
                let environment = environment();
                let document_bytes = document.source().bytes.as_ptr();
                let root_bytes = environment.root().source().bytes.as_ptr();
                // The function file that defines `id`, at `functions[1]`.
                let file_bytes = environment.captures()[1].document.source().bytes.as_ptr();
                let lock_bytes = environment.lock().unwrap().source.bytes.as_ptr();
                let definition = environment.environment().functions()[1]
                    .definition()
                    .tests
                    .as_ptr();
                let prepared = PreparedRun::prepare_with_environment(document, environment);
                let (document, captured, environment) = match &prepared {
                    Ok(run) => {
                        assert!(run.check_diagnostics().is_empty());
                        (
                            run.document(),
                            run.captured_environment(),
                            run.environment(),
                        )
                    }
                    Err(run) => (&run.document, &run.captured, &run.rejection.environment),
                };
                assert_eq!(prepared.is_ok(), name == "id");
                assert_eq!(document.source().bytes.as_ptr(), document_bytes);
                assert_eq!(captured.root().source().bytes.as_ptr(), root_bytes);
                assert_eq!(
                    captured.captures()[1].document.source().bytes.as_ptr(),
                    file_bytes
                );
                assert_eq!(captured.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
                assert_eq!(
                    environment.functions()[1].definition().tests.as_ptr(),
                    definition
                );
                assert_eq!(captured.origins().functions, ["unused", "id", "pair"]);
            }
        }

        #[test]
        fn every_build_reactivates_only_called_definitions_on_exact_cached_study_bytes() {
            // Seam: one prepared run built twice, its activation observations and the
            // snapshot both builds share, by identity.
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            assert_eq!(run.compiled().called_functions(), [1]);
            assert!(activation.trace.borrow().is_empty());
            for reads in [Some(1), Some(0)] {
                let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
                assert!(attempt.boundary.is_ok());
                assert_eq!(attempt.activation.lock, LockObservation::Verified);
                assert_eq!(attempt.activation.tests.len(), 3);
                assert!(attempt
                    .activation
                    .tests
                    .iter()
                    .all(|case| case.outcome == TestOutcome::Passed));
                assert!(!attempt.activation.tests[2].invoked);
                assert_eq!(attempt.activation.tests[2].actual, Some(Value::Missing));
                let trace = activation.trace.borrow().clone();
                let observed = yamaa_application::project_activation_observations::activation(
                    &run,
                    &attempt.activation,
                )
                .unwrap();
                assert_eq!(
                    observed["tests"][1]["args"]["x"],
                    yamaa_adapters::json!({"int":"-9223372036854775808"})
                );
                assert_eq!(
                    observed["tests"][1]["actual"],
                    yamaa_adapters::json!({"int":"-9223372036854775808"})
                );
                assert_eq!(
                    observed["tests"][2]["actual"],
                    yamaa_adapters::json!({"missing":null})
                );
                assert_eq!(observed["tests"][2]["actual_retained"], true);
                assert_eq!(observed["tests"][2]["invoked"], false);
                assert_eq!(observed["tests"][0]["source"], "study/functions/id.yaml");
                assert_eq!(observed["tests"][0]["function"], "id");
                assert_eq!(observed["tests"][0]["call"], "project.id");
                assert_eq!(*activation.trace.borrow(), trace);
                assert!(yamaa_application::project_attempt::execution(&attempt).is_some());
                let captured = &attempt.dataset.sources[0];
                assert_eq!(captured.read.snapshots_created, reads);
                assert!(Arc::ptr_eq(
                    captured.snapshot.as_ref().unwrap(),
                    &study.bytes
                ));
                assert_eq!(
                    captured.table.as_ref().unwrap().cell(0, 0).unwrap(),
                    ValueRef::Int(1)
                );
                assert_eq!(
                    attempt
                        .dataset
                        .result
                        .unwrap()
                        .result
                        .unwrap()
                        .dataset
                        .rows(),
                    &[vec![Value::Int(1), Value::Int(1)]]
                );
            }
            let once = ["lock", "bind", "case", "case", "inspect", "capture", "live"];
            assert_eq!(
                &*activation.trace.borrow(),
                &once.into_iter().chain(once).collect::<Vec<_>>()
            );
            assert_eq!(study.reads(), 1);
        }

        #[test]
        fn rejected_activation_preserves_original_payloads_and_has_zero_study_authority() {
            // Seam: the payload a failed activation holds, by identity, and the attempt's
            // empty sources.
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            for mode in [
                Mode::LockFailure,
                Mode::BindFailure,
                Mode::CaseFailure,
                Mode::CaseInterrupt,
            ] {
                let (mut activation, mut study) = ports();
                activation.mode = mode;
                let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
                let Err(BoundaryFailure::Activation(error)) = &attempt.boundary else {
                    panic!("activation failure")
                };
                let payload = match error {
                    Failure::Lock(payload) => payload,
                    Failure::Bindings(failures) => &failures[0].error,
                    Failure::Tests(failures) => {
                        let yamaa_core::project_activation::TestFailure::Invocation {
                            error, ..
                        } = &failures[0]
                        else {
                            panic!("invocation failure")
                        };
                        let FailureKind::CallFailed(payload) = &error.kind else {
                            panic!("original case error")
                        };
                        payload
                    }
                    Failure::InterruptedInvocation { error, .. } => {
                        let FailureKind::CallFailed(payload) = &error.kind else {
                            panic!("original interrupt")
                        };
                        payload
                    }
                    _ => panic!("exact ordinary or interrupt stage"),
                };
                let mut visited = 0;
                yamaa_application::project_attempt::visit_host_failures(&attempt, |_, original| {
                    assert!(std::ptr::eq(original, payload));
                    visited += 1;
                });
                assert_eq!(visited, 1);
                assert!(yamaa_application::project_attempt::execution(&attempt).is_none());
                assert!(Rc::ptr_eq(&payload.original, &activation.original));
                assert_eq!(payload.interrupt, matches!(mode, Mode::CaseInterrupt));
                assert!(attempt.dataset.sources.is_empty());
                assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
                assert_eq!(study.inspections, 0);
                assert_eq!(study.reads(), 0);
                if matches!(mode, Mode::CaseFailure | Mode::CaseInterrupt) {
                    assert_eq!(attempt.activation.tests[0].actual, Some(Value::Int(7)));
                    assert_eq!(attempt.activation.tests[0].outcome, TestOutcome::Passed);
                }
            }
        }

        #[test]
        fn unwound_activation_keeps_original_boundary_payload_and_completed_case_evidence() {
            // Seam: an unwound activation's payload, by identity, and the case evidence its
            // attempt keeps.
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::CaseUnwind;
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            let Err(BoundaryFailure::Unwind(payload)) = &attempt.boundary else {
                panic!("original unwind")
            };
            assert!(Arc::ptr_eq(
                payload.downcast_ref::<Arc<()>>().unwrap(),
                &activation.unwind
            ));
            assert_eq!(attempt.activation.tests[0].actual, Some(Value::Int(7)));
            assert_eq!(attempt.activation.tests[0].outcome, TestOutcome::Passed);
            assert_eq!(attempt.activation.tests[1].outcome, TestOutcome::Attempted);
            assert!(attempt.activation.tests[1].invoked);
            let observed = yamaa_application::project_activation_observations::activation(
                &run,
                &attempt.activation,
            )
            .unwrap();
            assert_eq!(observed["tests"].as_array().unwrap().len(), 2);
            assert_eq!(
                observed["tests"][0]["actual"],
                yamaa_adapters::json!({"int":"7"})
            );
            assert_eq!(observed["tests"][1]["outcome"], "attempted");
            assert_eq!(observed["tests"][1]["actual_retained"], false);
            assert!(observed["tests"][1]["actual"].is_null());
            assert!(attempt.dataset.sources.is_empty());
            assert_eq!(study.inspections, 0);
            activation.mode = Mode::Pass;
            let next = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            assert!(next.boundary.is_ok());
            assert_eq!(next.activation.tests.len(), 3);
            assert!(next.dataset.result.unwrap().result.is_ok());
            // The held failed attempt remains intact while a fresh attempt succeeds.
            assert_eq!(attempt.activation.tests[1].outcome, TestOutcome::Attempted);
        }

        #[test]
        fn live_host_failure_retains_exact_study_snapshot_and_typed_activation_success() {
            // Seam: a live call failure's payload and study snapshot, by identity, and its
            // typed execution error.
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::LiveFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            assert!(attempt.boundary.is_ok());
            assert!(attempt
                .activation
                .tests
                .iter()
                .all(|case| case.outcome == TestOutcome::Passed));
            let ExecutionError::ProjectFunction {
                path,
                error,
                identity,
            } = attempt
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap_err()
                .as_ref()
            else {
                panic!("retained versionless live error")
            };
            assert_eq!(path, "columns.VALUE.derivation.function");
            assert_eq!(identity.as_ref().unwrap().values, [Value::Int(1)]);
            let FailureKind::CallFailed(payload) = &error.kind else {
                panic!("original payload")
            };
            assert!(Rc::ptr_eq(&payload.original, &activation.original));
            assert!(Arc::ptr_eq(
                attempt.dataset.sources[0].snapshot.as_ref().unwrap(),
                &study.bytes
            ));
            let mut visited = 0;
            yamaa_application::project_attempt::visit_host_failures(&attempt, |stage, original| {
                assert_eq!(stage, "derivation");
                assert!(std::ptr::eq(original, payload));
                visited += 1;
            });
            assert_eq!(visited, 1);
            assert!(yamaa_application::project_attempt::execution(&attempt).is_none());
            assert_eq!(study.reads(), 1);
            assert_eq!(
                &*activation.trace.borrow(),
                &["lock", "bind", "case", "case", "inspect", "capture", "live"]
            );
        }

        #[test]
        fn activation_projection_validates_forged_case_order_without_effects() {
            // Seam: activation observations forged in the attempt, which no input can
            // write.
            use yamaa_application::project_activation_observations::{activation, Error};
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut host, mut study) = ports();
            let mut attempt = run.execute_with_ports(&mut host, &mut study, Scope::default());
            let trace = host.trace.borrow().clone();
            let reads = study.reads();
            assert!(activation(&run, &attempt.activation).is_ok());
            attempt.activation.tests.swap(0, 1);
            assert_eq!(
                activation(&run, &attempt.activation),
                Err(Error::InvalidObservation)
            );
            assert_eq!(*host.trace.borrow(), trace);
            assert_eq!(study.reads(), reads);
        }

        #[test]
        fn repeated_forged_activation_facts_cannot_gain_report_authority() {
            // Seam: activation observations forged past their limits in the attempt, which
            // no input can write.
            use yamaa_application::project_activation_observations as activation;
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut host, mut study) = ports();
            let mut attempt = run.execute_with_ports(&mut host, &mut study, Scope::default());
            let trace = host.trace.borrow().clone();
            let reads = study.reads();
            let bindings = attempt.activation.bindings.clone();
            for (binding_observations, actual) in [(true, 1025), (false, 65_537)] {
                if binding_observations {
                    attempt.activation.bindings.resize(actual, bindings[0]);
                } else {
                    attempt.activation.bindings = bindings.clone();
                    attempt.activation.tests.resize_with(actual, || {
                        yamaa_core::project_activation::TestObservation {
                            function: 0,
                            case: 0,
                            outcome: TestOutcome::Passed,
                            invoked: true,
                            actual: None,
                        }
                    });
                }
                assert_eq!(
                    activation::activation(&run, &attempt.activation),
                    Err(activation::Error::InvalidObservation)
                );
                assert!(matches!(
                    yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &[]
                    ),
                    Err(yamaa_application::specification_report::Refusal::Report(
                        yamaa_application::specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert_eq!(*host.trace.borrow(), trace);
            assert_eq!(study.reads(), reads);
        }

        fn report_identity() -> yamaa_application::specification_report::Identity<'static> {
            yamaa_application::specification_report::Identity {
                runtime: "test",
                runtime_version: "test-1",
                engine_version: "0.1.0",
                example: "owned-project",
                specification: "study/domain.yaml",
                base_directory: "study",
            }
        }

        #[test]
        fn complete_project_observations_borrow_the_exact_attempt_without_reactivation() {
            // Seam: the observation record of a completed project report and
            // the snapshot its attempt holds, which no case states. Its
            // issues, output, saved bytes and calls are
            // `project_functions.json::a_saved_project_run_publishes_its_output_without_activating_again`.
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            let trace = activation.trace.borrow().clone();
            let snapshot = attempt.dataset.sources[0]
                .snapshot
                .as_ref()
                .unwrap()
                .as_ptr();
            let result = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[],
            )
            .unwrap();
            let report = result.observations();
            assert_eq!(report["outcome"], "success");
            assert_eq!(report["artifacts"], yamaa_adapters::json!([]));
            assert_eq!(report["diagnostics"], yamaa_adapters::json!([]));
            assert_eq!(
                report["source_reads"],
                yamaa_adapters::json!([{"base_directory":"study","path":"input.csv","outcome":"captured","condition":null,"snapshots_created":1}])
            );
            assert_eq!(
                report["tables"],
                yamaa_adapters::json!([
                    {"specification":"study/domain.yaml","stage":"source","name":"SRC","columns":["ID"],"types":["int"],"rows":[[{"type":"int","value":"1"}]]},
                    {"specification":"study/domain.yaml","stage":"derived","name":"output","columns":["ID","VALUE"],"types":["int","int"],"rows":[[{"type":"int","value":"1"},{"type":"int","value":"1"}]]}
                ])
            );
            assert_eq!(report["activation"]["lock"], "verified");
            assert_eq!(report["activation"]["tests"].as_array().unwrap().len(), 3);
            assert_eq!(
                report["activation"]["tests"][2]["actual"],
                yamaa_adapters::json!({"missing":null})
            );
            let saved = result.saved_observations().unwrap();
            assert_eq!(saved["artifacts"][0]["content"], "ID,VALUE\n1,1\n");
            assert_eq!(saved["artifacts"][0]["row_count"], 1);
            assert_eq!(result.observations(), report);
            assert_eq!(*activation.trace.borrow(), trace);
            assert_eq!(study.reads(), 1);
            assert_eq!(
                attempt.dataset.sources[0]
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .as_ptr(),
                snapshot
            );
        }

        #[test]
        fn project_reports_hold_original_opaque_and_boundary_failures_and_their_observations() {
            // Seam: the payload a refused report holds, by identity, and the
            // observation record of a classified failure's report, which no
            // case states. The issues, saved files and calls of each failure
            // are `project_functions.json`'s binding, case, live call,
            // interrupt and panic cases.
            use yamaa_application::{
                project_function_diagnostics::HostDetails, project_report::Classified,
                specification_report::Refusal as Error,
            };
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            for mode in [
                Mode::BindFailure,
                Mode::CaseFailure,
                Mode::LiveFailure,
                Mode::CaseInterrupt,
                Mode::CaseUnwind,
            ] {
                let (mut activation, mut study) = ports();
                activation.mode = mode;
                let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
                let trace = activation.trace.borrow().clone();
                let unclassified = yamaa_application::specification_report::build_result(
                    &run,
                    &attempt,
                    report_identity(),
                    &[],
                );
                if matches!(mode, Mode::CaseInterrupt | Mode::CaseUnwind) {
                    let Error::Boundary(held) = unclassified.unwrap_err() else {
                        panic!("original boundary")
                    };
                    assert!(std::ptr::eq(held, attempt.boundary.as_ref().unwrap_err()));
                } else {
                    let Error::Opaque(held) = unclassified.unwrap_err() else {
                        panic!("unclassified original payload")
                    };
                    assert!(Rc::ptr_eq(&held.original, &activation.original));
                    let facts = [Classified::Host {
                        failure: held,
                        details: HostDetails::Exception {
                            class: "OriginalFailure",
                            message: "é failure",
                        },
                    }];
                    let result = yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &facts,
                    )
                    .unwrap();
                    let observed = result.observations();
                    assert_eq!(observed["outcome"], "failure");
                    assert_eq!(observed["artifacts"], yamaa_adapters::json!([]));
                    assert_eq!(result.output(), None);
                    if matches!(mode, Mode::LiveFailure) {
                        assert_eq!(observed["source_reads"].as_array().unwrap().len(), 1);
                        assert_eq!(observed["activation"]["tests"].as_array().unwrap().len(), 3);
                    } else {
                        assert_eq!(observed["source_reads"], yamaa_adapters::json!([]));
                    }
                    assert!(Rc::ptr_eq(&held.original, &activation.original));
                }
                assert_eq!(*activation.trace.borrow(), trace);
                assert_eq!(
                    study.reads(),
                    usize::from(matches!(mode, Mode::LiveFailure))
                );
            }
        }

        #[test]
        fn report_classifications_cannot_target_a_different_failure_or_supply_duplicates() {
            // Seam: classification facts naming another payload, or one twice, which no
            // host's classification can give.
            use yamaa_application::{
                project_function_diagnostics::HostDetails, project_report::Classified,
                specification_report::Refusal as Error,
            };
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::BindFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            let Err(BoundaryFailure::Activation(Failure::Bindings(failures))) = &attempt.boundary
            else {
                panic!("binding failure")
            };
            let original = &failures[0].error;
            let other = Payload {
                original: Rc::clone(&original.original),
                interrupt: false,
            };
            let details = HostDetails::Exception {
                class: "Failure",
                message: "same text",
            };
            for facts in [
                vec![Classified::Host {
                    failure: &other,
                    details,
                }],
                vec![
                    Classified::Host {
                        failure: original,
                        details,
                    },
                    Classified::Host {
                        failure: original,
                        details,
                    },
                ],
            ] {
                assert!(matches!(
                    yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &facts
                    ),
                    Err(Error::Report(
                        yamaa_application::specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert_eq!(study.reads(), 0);
            assert_eq!(*activation.trace.borrow(), ["lock", "bind"]);
        }

        #[test]
        fn complete_reports_refuse_missing_or_contradictory_activation_success() {
            // Seam: activation observations removed or contradicted in the attempt, which
            // no input can write.
            use yamaa_application::specification_report::{self, Refusal as Error};
            use yamaa_core::project_activation::BindingOutcome;
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            for change in 0..5 {
                let (mut activation, mut study) = ports();
                let mut attempt =
                    run.execute_with_ports(&mut activation, &mut study, Scope::default());
                let trace = activation.trace.borrow().clone();
                match change {
                    0 => {
                        attempt.activation.tests.pop();
                    }
                    1 => attempt.activation.tests[0].actual = None,
                    2 => attempt.activation.tests[0].actual = Some(Value::Int(99)),
                    3 => attempt.activation.lock = LockObservation::Rejected,
                    _ => attempt.activation.bindings[0].outcome = BindingOutcome::Rejected,
                }
                assert!(matches!(
                    yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &[]
                    ),
                    Err(Error::Report(
                        specification_report::Error::InvalidObservation
                    ))
                ));
                assert_eq!(*activation.trace.borrow(), trace);
                assert_eq!(study.reads(), 1);
                assert!(attempt.dataset.result.as_ref().unwrap().result.is_ok());
            }
        }

        #[test]
        fn lock_and_conformance_reports_hold_the_original_lock_payload_and_activation_record() {
            // Seam: the lock payload a refused report holds, by identity, and
            // the observation record of a lock or conformance report, which no
            // case states. Their issues and calls are
            // `project_functions.json::lock_findings_report_against_the_project_file_that_holds_the_lock`,
            // `a_host_exception_while_verifying_the_lock_refuses_the_build_opaquely`,
            // `a_lock_failure_classified_with_no_finding_refuses_the_report` and
            // `a_case_result_that_differs_fails_conformance_in_the_function_file_that_defines_it`.
            use yamaa_adapters::project_lock::{Finding, Reason};
            use yamaa_application::{
                project_report::Classified, specification_report::Refusal as Error,
            };
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::LockFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            let Err(BoundaryFailure::Activation(Failure::Lock(original))) = &attempt.boundary
            else {
                panic!("original lock payload")
            };
            let findings = [Finding {
                package: "project".into(),
                reason: Reason::DistributionNotIdentified,
                expected: vec![],
                actual: None,
            }];
            let result = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[Classified::Lock {
                    failure: original,
                    findings: &findings,
                }],
            )
            .unwrap();
            // A host exception while verifying the lock names no lock
            // finding: the lock failure stays the original opaque payload.
            let host = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[Classified::Host {
                    failure: original,
                    details:
                        yamaa_application::project_function_diagnostics::HostDetails::Exception {
                            class: "OSError",
                            message: "lock unreadable",
                        },
                }],
            );
            let Err(Error::Opaque(held)) = host else {
                panic!("a host fact on the lock failure stays opaque")
            };
            assert!(std::ptr::eq(held, original));
            assert_eq!(
                result.observations()["source_reads"],
                yamaa_adapters::json!([])
            );
            assert_eq!(result.observations()["activation"]["lock"], "rejected");
            assert_eq!(*activation.trace.borrow(), ["lock"]);
            assert_eq!(study.reads(), 0);
            assert!(Rc::ptr_eq(&original.original, &activation.original));
            let (mut activation, mut study) = ports();
            activation.mode = Mode::CaseMismatch;
            let attempt = run.execute_with_ports(&mut activation, &mut study, Scope::default());
            let result = yamaa_application::specification_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[],
            )
            .unwrap();
            assert_eq!(
                result.observations()["activation"]["tests"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            assert_eq!(
                result.observations()["activation"]["tests"][1]["outcome"],
                "passed"
            );
            assert_eq!(result.output(), None);
            assert!(result.files().is_empty());
            assert_eq!(study.reads(), 0);
            assert_eq!(*activation.trace.borrow(), ["lock", "bind", "case", "case"]);
        }

        #[test]
        fn zero_sized_host_failures_remain_opaque_and_never_accept_address_classification() {
            // Seam: classification facts naming a zero-sized payload by address, which no
            // host's classification can give.
            use yamaa_application::{
                project_function_diagnostics::HostDetails, project_report::Classified,
                specification_report, specification_report::Refusal as Error,
            };
            struct EmptyFailure;
            struct RejectLock;
            impl HostFunctions for RejectLock {
                type Handle = ();
                type Error = EmptyFailure;
                fn verify_lock(
                    &mut self,
                    _: Language,
                    _: Lock<'_>,
                    _: &[ProjectFunctionIdentity],
                ) -> Result<(), EmptyFailure> {
                    Err(EmptyFailure)
                }
                fn bind(
                    &mut self,
                    _: &ProjectFunctionIdentity,
                    _: &LogicalSignature,
                ) -> Result<(), EmptyFailure> {
                    panic!("rejected lock grants no binding authority")
                }
                fn is_interrupt(&self, _: &EmptyFailure) -> bool {
                    false
                }
                fn invoke(
                    &mut self,
                    _: &(),
                    _: &[Argument<'_>],
                ) -> Result<Value, HostError<EmptyFailure>> {
                    panic!("rejected lock grants no invocation authority")
                }
            }
            let run =
                PreparedRun::prepare_with_environment(document(Some("id")), environment()).unwrap();
            let (_, mut study) = ports();
            let attempt = run.execute_with_ports(&mut RejectLock, &mut study, Scope::default());
            let Err(BoundaryFailure::Activation(Failure::Lock(original))) = &attempt.boundary
            else {
                panic!("held original zero-sized lock failure")
            };
            assert!(matches!(
                yamaa_application::specification_report::build_result(
                    &run,
                    &attempt,
                    report_identity(),
                    &[]
                ),
                Err(Error::Opaque(_))
            ));
            let foreign = EmptyFailure;
            for facts in [
                vec![Classified::Lock {
                    failure: original,
                    findings: &[],
                }],
                vec![Classified::Lock {
                    failure: &foreign,
                    findings: &[],
                }],
                vec![Classified::Host {
                    failure: original,
                    details: HostDetails::Exception {
                        class: "Error",
                        message: "foreign facts",
                    },
                }],
            ] {
                assert!(matches!(
                    yamaa_application::specification_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &facts
                    ),
                    Err(Error::Report(
                        specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert!(matches!(
                attempt.boundary,
                Err(BoundaryFailure::Activation(Failure::Lock(_)))
            ));
            assert_eq!(study.reads(), 0);
        }
    }
}
