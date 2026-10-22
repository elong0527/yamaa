#![cfg(feature = "yaml")]
//! Project contracts: project environment projection, project function
//! diagnostics, project lock and project lock diagnostics.
mod project {
    mod project_environment_projection {
        //! Complete retained-attempt witnesses; expected reports are authored independently.
        use crate::support::project_schemas::candidate_schemas;
        use std::{cell::Cell, rc::Rc};
        use yamaa_adapters::json;
        use yamaa_adapters::{
            project_environment_diagnostics::{self as diagnostic, Error as ProjectionError},
            project_source::{
                self, CaptureFailure, Failure, Kind, Origin, ProjectCapturePort, Reply, Request,
            },
            specification_source::Source,
        };
        use yamaa_core::json::Value;
        use yamaa_core::{project_environment::LockKind, project_function::Language};
        /// The project names its Python environment file, which declares the lock
        /// and the external function file.
        const ENVIRONMENT: &str =
            "schema_version: '1.0'\nlock: uv.lock\nfunctions: {first: function.yaml}\n";
        // Deliberately neither Clone, Debug nor Send; the exact held error remains owned.
        struct Opaque {
            allocation: Rc<Cell<usize>>,
            text: String,
        }
        struct Port {
            error: Option<Opaque>,
            trace: Vec<(Kind, String)>,
            environment: String,
            lock_kind: LockKind,
            wide: bool,
            same_identity: bool,
        }
        impl Port {
            fn new(error: Opaque) -> Self {
                Self {
                    error: Some(error),
                    trace: vec![],
                    environment: ENVIRONMENT.into(),
                    lock_kind: LockKind::Uv,
                    wide: false,
                    same_identity: false,
                }
            }
        }
        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        impl ProjectCapturePort for Port {
            type Error = Opaque;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                self.trace.push((request.kind, request.written.into()));
                if request.written == "missing.yaml" {
                    return Err(self.error.take().unwrap());
                }
                Ok(match request.kind {
                    Kind::Environment => {
                        Reply::Document(source("held/environment.yaml", &self.environment))
                    }
                    Kind::Lock => Reply::Lock {
                        source: source("held/uv.lock", "version = 1\n"),
                        kind: self.lock_kind,
                    },
                    Kind::Function => Reply::Document(source(
                        // The same spelling as the declaring environment file
                        // still names the separately captured function file.
                        if self.same_identity {
                            "held/environment.yaml"
                        } else {
                            "held/function.yaml"
                        },
                        if self.wide {
                            "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 123456789012345678901234567890}]\n"
                        } else {
                            "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n"
                        },
                    )),
                    Kind::Codelist => Reply::Document(source(
                        "held/ct.yaml",
                        if self.wide {
                            "codelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n"
                        } else {
                            "codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n"
                        },
                    )),
                })
            }
        }
        fn rejected(
            wide: bool,
            same_identity: bool,
        ) -> (
            Box<project_source::RejectedEnvironment<Opaque>>,
            Port,
            Rc<Cell<usize>>,
            *const u8,
        ) {
            let allocation = Rc::new(Cell::new(19));
            let error = Opaque {
                allocation: Rc::clone(&allocation),
                text: "original capture allocation".into(),
            };
            let pointer = error.text.as_ptr();
            let mut port = Port::new(error);
            port.wide = wide;
            port.same_identity = same_identity;
            let body = if wide {
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [missing.yaml, ct.yaml]\n"
            } else {
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [missing.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n"
            };
            let rejected = match project_source::prepare(
                candidate_schemas(),
                source("held/project.yaml", body),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::Rejected(rejected)) => rejected,
                _ => panic!("complete retained rejection"),
            };
            assert_eq!(
                port.trace,
                [
                    (Kind::Environment, "environment.yaml".into()),
                    (Kind::Lock, "uv.lock".into()),
                    (Kind::Function, "function.yaml".into()),
                    (Kind::Codelist, "missing.yaml".into()),
                    (Kind::Codelist, "ct.yaml".into())
                ]
            );
            (rejected, port, allocation, pointer)
        }
        fn expected(condition: &str, rule: &str, path: &str, context: Value) -> Value {
            yamaa_adapters::json!({"phase": "validation", "condition": condition, "rule": rule, "spec_paths": [path], "context": context})
        }
        fn opaque(
            rejected: &project_source::RejectedEnvironment<Opaque>,
            allocation: &Rc<Cell<usize>>,
            pointer: *const u8,
        ) {
            let original = rejected
                .sources
                .iter()
                .find_map(|source| match &source.error {
                    CaptureFailure::Port(error) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert!(Rc::ptr_eq(&original.allocation, allocation));
            assert_eq!(original.text.as_ptr(), pointer);
            assert_eq!(original.text, "original capture allocation");
            assert_eq!(original.allocation.get(), 19);
        }
        #[test]
        fn complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes(
        ) {
            for same_identity in [false, true] {
                let (rejected, port, allocation, pointer) = rejected(false, same_identity);
                assert_eq!(rejected.origins.codelists, [1, 2]);
                let function_source = if same_identity {
                    "held/environment.yaml"
                } else {
                    "held/function.yaml"
                };
                let mut want = vec![expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "tests[0].result",
                    json!({"function":"first","case":"ordinary","value":true,"reason":"invalid_case_result","source":function_source,"entry":"held/project.yaml","environment_path":"functions.first"}),
                )];
                for obligation in ["boundary", "normal"] {
                    want.push(expected("project_environment_invalid", "FUNC-2", "tests", json!({"function":"first","obligation":obligation,"reason":"missing_coverage_obligation","source":function_source,"entry":"held/project.yaml","environment_path":"functions.first"})));
                }
                want.push(expected("codelist_shape_invalid", "CT-4", "codelists[0].items[0].value", json!({"codelist":"SEX","value":true,"source":"held/ct.yaml","entry":"held/project.yaml","environment_path":"codelists[1]"})));
                want.push(expected("codelist_shape_invalid", "CT-4", "codelists[2].codelists[0].items[0].value", json!({"codelist":"OTHER","value":true,"source":"held/project.yaml","entry":"held/project.yaml","environment_path":"codelists[2]"})));
                let result = diagnostic::admission(&rejected).unwrap();
                assert_eq!(crate::support::rows(&result), want);
                assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
                assert_eq!(port.trace.len(), 5);
                opaque(&rejected, &allocation, pointer);
            }
        }
        #[test]
        fn complete_external_scalar_reports_preserve_unbounded_integers_and_non_debug_originals() {
            let (rejected, port, allocation, pointer) = rejected(true, false);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), []);
            let wide: Value =
                yamaa_adapters::json_wire::decode("123456789012345678901234567890").unwrap();
            let want = [
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "tests[0].result",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/function.yaml","entry":"held/project.yaml","environment_path":"functions.first"}),
                ),
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "codelists[0].items[0].value",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/ct.yaml","entry":"held/project.yaml","environment_path":"codelists[1]"}),
                ),
            ];
            let result = diagnostic::scalar_sources(&rejected).unwrap();
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::scalar_sources(&rejected).unwrap(), result);
            assert_eq!(port.trace.len(), 5);
            opaque(&rejected, &allocation, pointer);
        }
        #[test]
        fn complete_static_report_matches_all_independent_single_finding_projections() {
            let (mut rejected, port, allocation, pointer) = rejected(false, false);
            let all = rejected.admission.take().unwrap();
            let mut independent = vec![];
            for finding in &all {
                rejected.admission = Some(vec![finding.clone()]);
                independent.extend(diagnostic::admission(&rejected).unwrap());
            }
            rejected.admission = Some(all);
            let complete = diagnostic::admission(&rejected).unwrap();
            assert_eq!(complete.len(), 5);
            assert_eq!(complete, independent);
            assert_eq!(port.trace.len(), 5);
            opaque(&rejected, &allocation, pointer);
        }

        #[test]
        fn wrong_origin_shape_duplicate_origins_and_equal_text_wrong_schema_refuse_without_replacement_errors(
        ) {
            let (mut rejected, port, allocation, pointer) = rejected(false, false);
            rejected.origins.codelists[1] = 1;
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.origins.codelists[1] = 2;
            rejected.captures[0].origin = Origin::Codelist(2);
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.captures[0].origin = Origin::Function("first".into());
            let complete = diagnostic::admission(&rejected).unwrap();
            // Equal text through an equal but separately admitted root is not the
            // retained schema: the function file shares the environment file's
            // schema, and the codelist file shares the project's schema.
            let replacement = candidate_schemas();
            let function = std::mem::replace(
                &mut rejected.captures[0].document,
                replacement.environment.prepare_named_class(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n"), "function_definition_class").unwrap(),
            );
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.captures[0].document = function;
            assert_eq!(rejected.captures[1].origin, Origin::Codelist(1));
            rejected.captures[1].document = replacement
                .project
                .prepare_named_class(
                    source(
                        "held/ct.yaml",
                        "codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n",
                    ),
                    "codelist_source_class",
                )
                .unwrap();
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            assert_eq!(complete.len(), 5);
            assert_eq!(port.trace.len(), 5);
            opaque(&rejected, &allocation, pointer);
        }
        #[test]
        fn invalid_origins_refuse_without_replacing_original_capture_errors() {
            let (mut rejected, port, allocation, pointer) = rejected(false, false);
            rejected.origins.functions.clear();
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.origins.functions.push("first".into());
            assert_eq!(diagnostic::admission(&rejected).unwrap().len(), 5);
            // Function findings name the linked environment file that declares them.
            let environment = rejected.environment_file.take();
            assert!(environment.is_some());
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            assert_eq!(port.trace.len(), 5);
            opaque(&rejected, &allocation, pointer);
        }

        #[test]
        fn retained_structural_limit_keeps_exact_facts_and_refuses_earlier_missing_rows() {
            use yamaa_adapters::{
                file_project,
                file_resources::Error as ResourceError,
                project_application::{self, Error as ApplicationError},
                specification_source::Error as SourceError,
            };
            use yamaa_core::limits::{LimitExceeded, Limits};

            struct RefusingPort {
                reads: usize,
            }
            impl ProjectCapturePort for RefusingPort {
                type Error = ResourceError;
                fn capture(&mut self, _: Request<'_>) -> Result<Reply, Self::Error> {
                    self.reads += 1;
                    Err(ResourceError::Missing)
                }
            }
            let mut port = RefusingPort { reads: 0 };
            let root = source(
                "held/project.yaml",
                "schema_version: '1.0'\ncodelists: [missing.yaml, ct.yaml]\n",
            );
            let Failure::Rejected(mut rejected) =
                project_source::prepare(candidate_schemas(), root, Language::Python, &mut port)
                    .unwrap_err()
            else {
                panic!("retained metadata capture owner")
            };
            let expected = LimitExceeded {
                resource: "spec_bytes",
                limit: Limits::SPEC_BYTES,
                actual: Some(Limits::SPEC_BYTES + 1),
            };
            // Supply the public retained codec-refusal shape directly: native
            // fixed byte limits normally stop this at the resource reader.
            rejected.sources[1].error = CaptureFailure::Structural(SourceError::Limit(expected));
            let error = ApplicationError::Project(file_project::Error::Environment(
                Failure::Rejected(rejected),
            ));
            let ApplicationError::Project(file_project::Error::Environment(Failure::Rejected(
                owner,
            ))) = &error
            else {
                unreachable!()
            };
            assert_eq!(owner.sources.len(), 2);
            assert!(matches!(
                owner.sources[0].error,
                CaptureFailure::Port(ResourceError::Missing)
            ));
            assert!(matches!(
                owner.sources[1].error,
                CaptureFailure::Structural(SourceError::Limit(limit)) if limit == expected
            ));
            let held_root = owner.root.source().bytes.as_ptr();
            let held_sources = owner.sources.as_ptr();
            for _ in 0..2 {
                let rows = project_application::failure(&error);
                assert_eq!(
                    crate::support::rows(&rows),
                    [
                        yamaa_adapters::json!({"phase": "engine", "condition": "engine_rejected", "rule": null, "spec_paths": ["codelists[1]"], "context": json!({
                            "step": "environment", "code": "resource_limit",
                            "resource": "spec_bytes", "limit": Limits::SPEC_BYTES,
                            "actual": Limits::SPEC_BYTES + 1,
                        })})
                    ],
                );
                assert_eq!(owner.root.source().bytes.as_ptr(), held_root);
                assert_eq!(owner.sources.as_ptr(), held_sources);
                assert_eq!(port.reads, 2);
            }
        }

        #[test]
        fn missing_environment_file_reports_its_link_and_written_path_beside_other_sources() {
            use yamaa_adapters::{
                file_project,
                file_resources::Error as ResourceError,
                project_application::{self, Error as ApplicationError},
            };
            struct MissingPort {
                trace: Vec<(Kind, String, String)>,
            }
            impl ProjectCapturePort for MissingPort {
                type Error = ResourceError;
                fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                    self.trace.push((
                        request.kind,
                        request.declaring_source.into(),
                        request.written.into(),
                    ));
                    Err(ResourceError::Missing)
                }
            }
            let mut port = MissingPort { trace: vec![] };
            let root = source(
                "held/project.yaml",
                "schema_version: '1.0'\nenvironments: {python: python/environment.yaml, r: r/environment.yaml}\ncodelists: [ct.yaml]\n",
            );
            let Failure::Rejected(rejected) =
                project_source::prepare(candidate_schemas(), root, Language::Python, &mut port)
                    .unwrap_err()
            else {
                panic!("retained environment file capture owner")
            };
            // The R link is never opened; both reads resolve from the project file.
            assert_eq!(
                port.trace,
                [
                    (
                        Kind::Environment,
                        "held/project.yaml".into(),
                        "python/environment.yaml".into()
                    ),
                    (Kind::Codelist, "held/project.yaml".into(), "ct.yaml".into()),
                ]
            );
            assert_eq!(
                rejected
                    .sources
                    .iter()
                    .map(|source| source.origin.clone())
                    .collect::<Vec<_>>(),
                [Origin::Environment(Language::Python), Origin::Codelist(0)]
            );
            assert!(rejected.environment_file.is_none());
            let error = ApplicationError::Project(file_project::Error::Environment(
                Failure::Rejected(rejected),
            ));
            let want = [
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "environments.python",
                    json!({"entry":"held/project.yaml","path":"python/environment.yaml","environment_path":"environments.python","cause":"missing"}),
                ),
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "codelists[0]",
                    json!({"entry":"held/project.yaml","path":"ct.yaml","environment_path":"codelists[0]","cause":"missing"}),
                ),
            ];
            for _ in 0..2 {
                assert_eq!(
                    crate::support::rows(&project_application::failure(&error)),
                    want
                );
            }
            assert_eq!(port.trace.len(), 2);
        }

        #[test]
        fn original_root_wide_codelist_scalars_report_against_the_project_without_capture_effects()
        {
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port::new(Opaque {
                allocation,
                text: "never consumed".into(),
            });
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists:\n  - {codelists: [{id: WIDE, name: Wide, data_type: integer, items: [{value: 123456789012345678901234567890, rank: -123456789012345678901234567890}, {value: 9223372036854775808}]}]}\n";
            let (root, findings) = match project_source::prepare(
                candidate_schemas(),
                source("held/project.yaml", body),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::RootScalar { root, findings }) => (root, findings),
                _ => panic!("retained original root scalars"),
            };
            let number = |text: &str| -> Value { yamaa_adapters::json_wire::decode(text).unwrap() };
            let want = [
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "codelists[0].codelists[0].items[0].value",
                    json!({"reason":"integer_out_of_range","value":number("123456789012345678901234567890"),"source":"held/project.yaml","entry":"held/project.yaml","environment_path":"$"}),
                ),
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "codelists[0].codelists[0].items[0].rank",
                    json!({"reason":"integer_out_of_range","value":number("-123456789012345678901234567890"),"source":"held/project.yaml","entry":"held/project.yaml","environment_path":"$"}),
                ),
                expected(
                    "project_environment_invalid",
                    "FUNC-2",
                    "codelists[0].codelists[0].items[1].value",
                    json!({"reason":"integer_out_of_range","value":number("9223372036854775808"),"source":"held/project.yaml","entry":"held/project.yaml","environment_path":"$"}),
                ),
            ];
            let result = diagnostic::scalar_root(&root, &findings).unwrap();
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::scalar_root(&root, &findings).unwrap(), result);
            // A failed project scalar stops before its environment link is read.
            assert_eq!(port.trace, []);
            assert!(port.error.is_some());
        }

        #[test]
        fn environment_file_temporal_and_wide_scalars_report_against_its_link_without_later_reads()
        {
            for (host, key) in [(Language::Python, "python"), (Language::R, "r")] {
                let allocation = Rc::new(Cell::new(19));
                let mut port = Port::new(Opaque {
                    allocation,
                    text: "never consumed".into(),
                });
                port.environment = "schema_version: '1.0'\nlock: uv.lock\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n".into();
                let rejected = match project_source::prepare(
                    candidate_schemas(),
                    source(
                        "held/project.yaml",
                        &format!(
                            "schema_version: '1.0'\nenvironments: {{{key}: environment.yaml}}\n"
                        ),
                    ),
                    host,
                    &mut port,
                ) {
                    Err(Failure::Rejected(rejected)) => rejected,
                    _ => panic!("retained environment file scalars"),
                };
                assert_eq!(rejected.sources[0].origin, Origin::Environment(host));
                let link = format!("environments.{key}");
                let wide: Value =
                    yamaa_adapters::json_wire::decode("123456789012345678901234567890").unwrap();
                let want = [
                    expected(
                        "project_environment_invalid",
                        "FUNC-2",
                        "functions.wide.tests[0].result",
                        json!({"reason":"integer_out_of_range","value":wide,"source":"held/environment.yaml","entry":"held/project.yaml","environment_path":link}),
                    ),
                    expected(
                        "project_environment_invalid",
                        "FUNC-2",
                        "functions.bad_date.tests[0].result",
                        json!({"reason":"invalid_date","value":"2024-02-30","source":"held/environment.yaml","entry":"held/project.yaml","environment_path":link}),
                    ),
                    expected(
                        "project_environment_invalid",
                        "FUNC-2",
                        "functions.bad_time.tests[0].result",
                        json!({"reason":"invalid_datetime","value":"2024-01-01T25:00:00","source":"held/environment.yaml","entry":"held/project.yaml","environment_path":link}),
                    ),
                ];
                let result = diagnostic::scalar_sources(&rejected).unwrap();
                assert_eq!(crate::support::rows(&result), want);
                assert_eq!(diagnostic::scalar_sources(&rejected).unwrap(), result);
                assert_eq!(diagnostic::admission(&rejected).unwrap(), []);
                assert_eq!(diagnostic::schema_sources(&rejected).unwrap(), []);
                // The failed environment file declares the lock; nothing it writes is read.
                assert_eq!(port.trace, [(Kind::Environment, "environment.yaml".into())]);
                assert!(rejected.environment_file.is_none());
                assert!(port.error.is_some());
            }
        }

        #[test]
        fn lock_kind_mismatch_reports_against_the_environment_file_that_declares_the_lock() {
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port::new(Opaque {
                allocation,
                text: "never consumed".into(),
            });
            port.environment = "schema_version: '1.0'\nlock: uv.lock\n".into();
            port.lock_kind = LockKind::Renv;
            let rejected = match project_source::prepare(
                candidate_schemas(),
                source(
                    "held/project.yaml",
                    "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                ),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::Rejected(rejected)) => rejected,
                _ => panic!("retained lock kind rejection"),
            };
            let want = [expected(
                "project_environment_invalid",
                "FUNC-2",
                "lock",
                json!({"reason":"lock_kind_mismatch","language":"python","actual_lock_kind":"renv","source":"held/environment.yaml","entry":"held/project.yaml","environment_path":"lock"}),
            )];
            let result = diagnostic::admission(&rejected).unwrap();
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
            assert_eq!(
                port.trace,
                [
                    (Kind::Environment, "environment.yaml".into()),
                    (Kind::Lock, "uv.lock".into())
                ]
            );
            // The lock belongs to the environment file; without it there is no owner.
            let mut rejected = rejected;
            rejected.environment_file = None;
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            assert!(port.error.is_some());
        }

        #[test]
        fn actual_captured_two_hundred_inline_functions_keep_the_full_six_hundred_record_report() {
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port::new(Opaque {
                allocation,
                text: "never consumed".into(),
            });
            let mut body = String::from("schema_version: '1.0'\nlock: uv.lock\nfunctions:\n");
            for index in 0..200 {
                body.push_str(&format!("  function_{index}: {{function: project.constant, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: true}}]}}\n"));
            }
            port.environment = body;
            let rejected = match project_source::prepare(
                candidate_schemas(),
                source(
                    "held/project.yaml",
                    "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                ),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::Rejected(rejected)) => rejected,
                _ => panic!("actual captured static rejection"),
            };
            let Some(findings) = &rejected.admission else {
                panic!("complete independent findings")
            };
            assert_eq!(findings.len(), 600);
            let result = diagnostic::admission(&rejected).unwrap();
            let mut want = vec![];
            for index in 0..200 {
                let name = format!("function_{index}");
                let declaration = format!("functions.{name}");
                want.push(expected("project_environment_invalid", "FUNC-2", &format!("{declaration}.tests[0].result"), json!({"function":name,"case":"ordinary","value":true,"reason":"invalid_case_result","source":"held/environment.yaml","entry":"held/project.yaml","environment_path":declaration})));
                for obligation in ["boundary", "normal"] {
                    want.push(expected("project_environment_invalid", "FUNC-2", &format!("{declaration}.tests"), json!({"function":name,"obligation":obligation,"reason":"missing_coverage_obligation","source":"held/environment.yaml","entry":"held/project.yaml","environment_path":declaration})));
                }
            }
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
            assert_eq!(
                port.trace,
                [
                    (Kind::Environment, "environment.yaml".into()),
                    (Kind::Lock, "uv.lock".into())
                ]
            );
            assert!(port.error.is_some());
        }
    }
    mod project_function_diagnostics {
        use std::rc::Rc;
        use yamaa_adapters::project_function_diagnostics::{invocation, Error, HostDetails};
        use yamaa_core::{
            dataset::RowIdentity,
            function_invocation::{FailureKind, InvocationFailure},
        };
        use yamaa_core::{
            function_signature::ProjectFunctionIdentity,
            value::{ColumnType, Value, ValueType},
        };

        fn make_failure(
            kind: FailureKind<Rc<str>>,
        ) -> InvocationFailure<Rc<str>, ProjectFunctionIdentity> {
            InvocationFailure {
                identity: ProjectFunctionIdentity {
                    name: "bmi".into(),
                    call: "projectbmi.bmi".into(),
                },
                kind,
            }
        }
        #[test]
        fn every_runtime_failure_keeps_versionless_identity_and_authored_provenance() {
            let cases = [
                (
                    FailureKind::UnknownArguments(vec!["extra".into()]),
                    "invalid_function_argument",
                    "FUNC-5",
                ),
                (
                    FailureKind::MissingRequired {
                        parameter: "weight".into(),
                    },
                    "invalid_function_argument",
                    "FUNC-5",
                ),
                (
                    FailureKind::ArgumentType {
                        parameter: "weight".into(),
                        expected: ValueType::Float,
                        actual: ValueType::Int,
                    },
                    "invalid_function_argument",
                    "FUNC-5",
                ),
                (
                    FailureKind::BooleanResult,
                    "invalid_function_result",
                    "FUNC-9",
                ),
                (
                    FailureKind::UndeclaredMissing,
                    "invalid_function_result",
                    "FUNC-9",
                ),
                (
                    FailureKind::ResultType {
                        expected: ColumnType::Float,
                        actual: ValueType::Str,
                    },
                    "invalid_function_result",
                    "FUNC-9",
                ),
            ];
            let row = RowIdentity {
                position: 3,
                values: vec![
                    Value::Int(i64::MIN),
                    Value::Int(i64::MAX),
                    Value::Str("é\0🙂".into()),
                ],
            };
            for (kind, condition, rule) in cases {
                let failure = make_failure(kind);
                let issue = invocation(
                    "rows[2].derivation.BMI.function",
                    &failure,
                    Some(&row),
                    &["MIN", "MAX", "TEXT"],
                    None,
                )
                .unwrap();
                assert_eq!(issue.phase(), "derivation");
                assert_eq!(issue.condition(), condition);
                assert_eq!(issue.rule(), Some(rule));
                assert_eq!(issue.spec_paths, ["rows[2].derivation.BMI.function"]);
                let context = yamaa_core::json::Value::Object(issue.context.clone());
                assert_eq!(context["function"], "bmi");
                assert_eq!(context["call"], "projectbmi.bmi");
                assert_eq!(
                    context["keys"],
                    yamaa_adapters::json!([{"MIN":i64::MIN,"MAX":i64::MAX,"TEXT":"é\0🙂"}])
                );
                assert!(context.get("contract_version").is_none());
                assert!(context.get("implementation_version").is_none());
            }
        }
        #[test]
        fn opaque_payload_is_borrowed_unchanged_and_complete_ordinary_facts_remain_exact() {
            let payload: Rc<str> = Rc::from("original interrupt or boundary");
            for kind in [
                FailureKind::CallFailed(payload.clone()),
                FailureKind::InvalidHostResult(payload.clone()),
            ] {
                let failure = make_failure(kind);
                let Err(Error::Opaque(held)) =
                    invocation("columns.BMI.derivation.function", &failure, None, &[], None)
                else {
                    panic!("opaque payload flattened")
                };
                assert!(Rc::ptr_eq(held, &payload));
                let issue = invocation(
                    "columns.BMI.derivation.function",
                    &failure,
                    None,
                    &[],
                    Some(HostDetails::Exception {
                        class: "ValueError",
                        message: "é\0🙂",
                    }),
                )
                .unwrap();
                let context = yamaa_core::json::Value::Object(issue.context.clone());
                assert_eq!(context["host_error"], "ValueError");
                assert_eq!(context["host_message"], "é\0🙂");
                assert!(context.get("host_details_truncated").is_none());
                assert!(Rc::strong_count(&payload) >= 2);
                let oversized = "x".repeat(8193);
                let issue = invocation(
                    "columns.BMI.derivation.function",
                    &failure,
                    None,
                    &[],
                    Some(HostDetails::Rejected {
                        reason: &oversized,
                        returned: None,
                    }),
                )
                .unwrap();
                let context = yamaa_core::json::Value::Object(issue.context.clone());
                assert_eq!(context["reason"], oversized);
            }
            let failure = make_failure(FailureKind::InvalidHostResult(payload));
            let issue = invocation(
                "columns.BMI.derivation.function",
                &failure,
                None,
                &[],
                Some(HostDetails::Rejected {
                    reason: "not a scalar",
                    returned: Some("list"),
                }),
            )
            .unwrap();
            assert_eq!(issue.condition(), "invalid_function_result");
            assert_eq!(issue.context.clone()["returned"], "list");
        }
        #[test]
        fn malformed_identity_keys_refuse_and_escaped_arguments_remain_complete() {
            let failure = make_failure(FailureKind::BooleanResult);
            let row = RowIdentity {
                position: 0,
                values: vec![Value::Int(1), Value::Int(2)],
            };
            for names in [&["ID"][..], &["ID", "ID"][..], &["", "ID"][..]] {
                assert!(matches!(
                    invocation(
                        "columns.BMI.derivation.function",
                        &failure,
                        Some(&row),
                        names,
                        None
                    ),
                    Err(Error::InvalidContext)
                ));
            }
            assert!(matches!(
                invocation("", &failure, None, &[], None),
                Err(Error::InvalidContext)
            ));
            assert!(matches!(
                invocation(
                    "columns.BMI.derivation.function",
                    &failure,
                    None,
                    &[],
                    Some(HostDetails::Rejected {
                        reason: "invented",
                        returned: None
                    })
                ),
                Err(Error::InvalidContext)
            ));
            let oversized = "\0".repeat(1_400_000);
            let failure = make_failure(FailureKind::UnknownArguments(vec![oversized.clone()]));
            let issue =
                invocation("columns.BMI.derivation.function", &failure, None, &[], None).unwrap();
            let context = yamaa_core::json::Value::Object(issue.context.clone());
            assert_eq!(context["unknown"], yamaa_adapters::json!([oversized]));
        }

        #[test]
        fn actual_engine_failures_project_without_a_second_callback_or_payload_consumption() {
            use yamaa_core::function_invocation::{
                invoke_project, Argument, FunctionPort, HostError,
            };
            use yamaa_core::function_signature::ProjectInvocationPlan;
            struct Port {
                calls: usize,
                result: Option<Result<Value, HostError<Rc<str>>>>,
            }
            impl FunctionPort for Port {
                type Error = Rc<str>;
                fn call(
                    &mut self,
                    _slot: usize,
                    arguments: &[Argument<'_>],
                ) -> Result<Value, HostError<Self::Error>> {
                    assert!(arguments.is_empty());
                    self.calls += 1;
                    self.result.take().expect("callback was retried")
                }
            }
            let plan = ProjectInvocationPlan::new(
                ProjectFunctionIdentity {
                    name: "identity".into(),
                    call: "project.identity".into(),
                },
                vec![],
                ColumnType::Int,
                false,
            )
            .unwrap();
            let payload: Rc<str> = Rc::from("original host condition");
            for (result, condition, rule, host_failure) in [
                (
                    Ok(Value::Bool(true)),
                    "invalid_function_result",
                    "FUNC-9",
                    false,
                ),
                (
                    Ok(Value::Missing),
                    "invalid_function_result",
                    "FUNC-9",
                    false,
                ),
                (
                    Ok(Value::float(7.0)),
                    "invalid_function_result",
                    "FUNC-9",
                    false,
                ),
                (
                    Err(HostError::Raised(payload.clone())),
                    "function_call_failed",
                    "FUNC-8",
                    true,
                ),
                (
                    Err(HostError::InvalidResult(payload.clone())),
                    "invalid_function_result",
                    "FUNC-9",
                    true,
                ),
            ] {
                let mut port = Port {
                    calls: 0,
                    result: Some(result),
                };
                let failure = invoke_project(&plan, &std::collections::BTreeMap::new(), &mut port)
                    .unwrap_err();
                let issue = if host_failure {
                    let Err(Error::Opaque(held)) = invocation(
                        "columns.VALUE.derivation.function",
                        &failure,
                        None,
                        &[],
                        None,
                    ) else {
                        panic!("opaque payload consumed")
                    };
                    assert!(Rc::ptr_eq(held, &payload));
                    invocation(
                        "columns.VALUE.derivation.function",
                        &failure,
                        None,
                        &[],
                        Some(HostDetails::Exception {
                            class: "OriginalCondition",
                            message: "ordinary classified failure",
                        }),
                    )
                    .unwrap()
                } else {
                    invocation(
                        "columns.VALUE.derivation.function",
                        &failure,
                        None,
                        &[],
                        None,
                    )
                    .unwrap()
                };
                assert_eq!(issue.condition(), condition);
                assert_eq!(issue.rule(), Some(rule));
                assert_eq!(port.calls, 1);
            }
        }
    }
    mod project_lock {
        use yamaa_adapters::project_lock::{decode_renv, Error, RenvLock};
        #[test]
        fn uv_classification_allows_unresolved_versions_but_validates_written_identities() {
            let dynamic =
                b"version = 1\n[[package]]\nname = 'program'\nsource = {editable = 'program'}\n";
            assert_eq!(
                yamaa_adapters::project_lock::kind(dynamic).unwrap(),
                yamaa_core::project_environment::LockKind::Uv
            );
            for invalid in [
                "version = 1\n[[package]]\nversion = '1.0'\n",
                "version = 1\n[[package]]\nname = 'program'\nversion = 1\n",
                "version = 1\n[[package]]\nname = 'program'\nversion = ''\n",
            ] {
                assert!(yamaa_adapters::project_lock::kind(invalid.as_bytes()).is_err());
            }
        }
        #[test]
        fn renv_metadata_preserves_exact_names_versions_and_ignores_unrelated_tool_metadata() {
            let raw=br#"{"R":{"Version":"4.6.1","Repositories":[{"Name":"CRAN","URL":"https://example.invalid"}]},"Packages":{"yamaa":{"Package":"yamaa","Version":"0.2.0","Source":"Local"},"projectbmi":{"Package":"projectbmi","Version":"1.2-0","Source":"Repository"}}}"#;
            assert_eq!(
                decode_renv(raw).unwrap(),
                RenvLock {
                    runtime_version: "4.6.1".into(),
                    packages: [
                        ("projectbmi".into(), "1.2-0".into()),
                        ("yamaa".into(), "0.2.0".into())
                    ]
                    .into()
                }
            );
        }
        #[test]
        fn duplicate_keys_are_rejected_including_nested_package_version_replacement() {
            for raw in [
                r#"{"R":{"Version":"4.6.1"},"R":{"Version":"4.5.0"},"Packages":{}}"#,
                r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2.0","Version":"9.0.0"}}}"#,
                r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2.0"},"yamaa":{"Version":"9.0.0"}}}"#,
            ] {
                assert!(matches!(
                    decode_renv(raw.as_bytes()),
                    Err(Error::Document(_))
                ));
            }
        }
        #[test]
        fn yaml_extensions_never_become_valid_renv_json() {
            for raw in [
                "R: {Version: '4.6.1'}\nPackages: {}",
                r#"{"R":{"Version":"4.6.1"},"Packages":{},}"#,
                r#"{"R":{"Version":NaN},"Packages":{}}"#,
            ] {
                assert!(matches!(decode_renv(raw.as_bytes()), Err(Error::Json(_))));
            }
        }
        #[test]
        fn wrong_shapes_and_package_identity_mismatches_never_produce_partial_metadata() {
            for raw in [
                r#"{"Packages":{}}"#,
                r#"{"R":{"Version":4.6},"Packages":{}}"#,
                r#"{"R":{"Version":"4.6.1"},"Packages":[]}"#,
                r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Package":"other","Version":"0.2.0"}}}"#,
                r#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":null}}}"#,
            ] {
                assert!(matches!(decode_renv(raw.as_bytes()), Err(Error::Shape(_))));
            }
        }
        #[test]
        fn complete_version_text_is_retained_and_nul_is_rejected_without_truncation() {
            let raw = format!(
        "{{\"R\":{{\"Version\":\"4.6.1\"}},\"Packages\":{{\"yamaa\":{{\"Version\":\"{}\"}}}}}}",
        "x".repeat(2049)
    );
            assert_eq!(
                decode_renv(raw.as_bytes()).unwrap().packages["yamaa"],
                "x".repeat(2049)
            );
            assert!(matches!(
                decode_renv(
                    br#"{"R":{"Version":"4.6.1"},"Packages":{"yamaa":{"Version":"0.2\u0000"}}}"#
                ),
                Err(Error::Shape("Version"))
            ));
        }
    }
    mod project_lock_diagnostics {
        use crate::support::project_schemas::candidate_schemas;
        use yamaa_adapters::{
            project_lock::{Finding, Reason},
            project_lock_diagnostics::{issues, Error},
            project_source::{self, CapturedEnvironment, Kind, ProjectCapturePort, Reply, Request},
            specification_source::Source,
        };
        use yamaa_core::{project_environment::LockKind, project_function::Language};

        struct Capture {
            environments: usize,
            locks: usize,
            lock: bool,
        }
        impl ProjectCapturePort for Capture {
            type Error = std::convert::Infallible;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                match request.kind {
                    Kind::Environment => {
                        assert_eq!(request.declaring_source, "study/project.yaml");
                        assert_eq!(request.written, "environment.yaml");
                        self.environments += 1;
                        Ok(Reply::Document(Source {
                            identity: "study/environment.yaml".into(),
                            bytes: if self.lock {
                                b"schema_version: '1.0'\nlock: ../uv.lock\n".to_vec()
                            } else {
                                b"schema_version: '1.0'\n".to_vec()
                            },
                        }))
                    }
                    Kind::Lock => {
                        // The environment file writes the lock path; it resolves there.
                        assert_eq!(request.declaring_source, "study/environment.yaml");
                        assert_eq!(request.written, "../uv.lock");
                        self.locks += 1;
                        Ok(Reply::Lock {
                            source: Source {
                                identity: "held/uv.lock".into(),
                                bytes: b"version = 1\n".to_vec(),
                            },
                            kind: LockKind::Uv,
                        })
                    }
                    kind => panic!("unexpected {kind:?} capture"),
                }
            }
        }
        fn prepare(project: &str, lock: bool) -> (CapturedEnvironment, Capture) {
            let mut capture = Capture {
                environments: 0,
                locks: 0,
                lock,
            };
            let source = Source {
                identity: "study/project.yaml".into(),
                bytes: project.as_bytes().to_vec(),
            };
            let prepared = project_source::prepare(
                candidate_schemas(),
                source,
                Language::Python,
                &mut capture,
            )
            .unwrap();
            let (_, provenance) = prepared.into_owned().into_parts();
            (provenance, capture)
        }
        /// The project links one environment file, which may declare a lock.
        fn environment(locked: bool) -> (CapturedEnvironment, Capture) {
            prepare(
                "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n",
                locked,
            )
        }
        /// The project names no environment file for the host.
        fn without_environment() -> (CapturedEnvironment, Capture) {
            prepare("schema_version: '1.0'\n", false)
        }
        fn finding(reason: Reason, expected: &[&str], actual: Option<&str>) -> Finding {
            Finding {
                package: "program".into(),
                reason,
                expected: expected.iter().map(|version| (*version).into()).collect(),
                actual: actual.map(str::to_owned),
            }
        }

        #[test]
        fn complete_classified_host_facts_preserve_order_and_held_lock_origins_without_reads() {
            let (environment, capture) = environment(true);
            let root = environment.root().source().bytes.as_ptr();
            let file = environment
                .environment_file()
                .unwrap()
                .source()
                .bytes
                .as_ptr();
            let lock = environment.lock().unwrap().source.bytes.as_ptr();
            let facts = [
                finding(Reason::DistributionNotIdentified, &[], None),
                finding(Reason::VersionNotLocked, &[], None),
                finding(Reason::AmbiguousLockVersion, &["1.0", "2.0"], None),
                finding(Reason::InvalidLockedVersion, &["invalid\0é"], None),
                finding(Reason::PackageNotInstalled, &["1.0"], None),
                finding(Reason::InvalidInstalledVersion, &["1.0"], Some("bad\0🙂")),
                finding(Reason::VersionMismatch, &["1.0"], Some("2.0")),
            ];
            let records = issues(&environment, Some(Language::Python), &facts).unwrap();
            assert_eq!(records.len(), 7);
            let expected = [
                (
                    "distribution_not_identified",
                    yamaa_adapters::json!([]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "version_not_locked",
                    yamaa_adapters::json!([]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "ambiguous_lock_version",
                    yamaa_adapters::json!(["1.0", "2.0"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "invalid_locked_version",
                    yamaa_adapters::json!(["invalid\0é"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "package_not_installed",
                    yamaa_adapters::json!(["1.0"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "invalid_installed_version",
                    yamaa_adapters::json!(["1.0"]),
                    yamaa_adapters::json!("bad\0🙂"),
                ),
                (
                    "version_mismatch",
                    yamaa_adapters::json!(["1.0"]),
                    yamaa_adapters::json!("2.0"),
                ),
            ];
            for (record, (reason, versions, actual)) in records.iter().zip(expected) {
                assert_eq!(record.phase(), "validation");
                assert_eq!(record.condition(), "runtime_artifact_mismatch");
                assert_eq!(record.rule(), Some("FUNC-3"));
                assert_eq!(record.spec_paths, ["lock"]);
                assert_eq!(
                    yamaa_core::json::Value::Object(record.context.clone()),
                    yamaa_adapters::json!({"source":"study/environment.yaml", "entry":"study/project.yaml",
                "lock_source":"held/uv.lock", "lock":"../uv.lock", "package":"program",
                "reason":reason, "expected":versions, "actual":actual})
                );
            }
            assert_eq!(
                issues(&environment, Some(Language::Python), &facts).unwrap(),
                records
            );
            assert_eq!((capture.environments, capture.locks), (1, 1));
            assert_eq!(environment.root().source().bytes.as_ptr(), root);
            assert_eq!(
                environment
                    .environment_file()
                    .unwrap()
                    .source()
                    .bytes
                    .as_ptr(),
                file
            );
            assert_eq!(environment.lock().unwrap().source.bytes.as_ptr(), lock);
        }

        #[test]
        fn projection_preserves_host_version_spelling_and_does_not_reimplement_comparison() {
            let (environment, _) = environment(true);
            let facts = [
                finding(Reason::InvalidInstalledVersion, &["1.0"], None),
                finding(Reason::InvalidLockedVersion, &[], None),
                finding(Reason::VersionMismatch, &["1.0"], Some("1.0.0")),
            ];
            let records = issues(&environment, Some(Language::Python), &facts).unwrap();
            assert_eq!(records.len(), 3);
            let context = yamaa_core::json::Value::Object(records[2].context.clone());
            assert_eq!(context["expected"], yamaa_adapters::json!(["1.0"]));
            assert_eq!(context["actual"], "1.0.0");
        }

        #[test]
        fn absent_lock_and_contradictory_facts_refuse_the_complete_projection() {
            for (unlocked, _) in [environment(false), without_environment()] {
                assert!(issues(&unlocked, Some(Language::Python), &[])
                    .unwrap()
                    .is_empty());
                let fact = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
                assert_eq!(
                    issues(&unlocked, Some(Language::Python), &[fact]),
                    Err(Error::InvalidContext)
                );
            }
            let (environment, _) = environment(true);
            for bad in [
                finding(Reason::VersionMismatch, &["1.0"], None),
                finding(Reason::VersionNotLocked, &["1.0"], None),
                finding(Reason::AmbiguousLockVersion, &["1.0"], None),
                finding(Reason::PackageNotInstalled, &["1.0"], Some("2.0")),
            ] {
                let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
                assert_eq!(
                    issues(&environment, Some(Language::Python), &[first, bad]),
                    Err(Error::InvalidContext)
                );
            }
            assert!(Reason::parse("unclassified runtime error").is_none());
        }

        #[test]
        fn complete_escaped_host_versions_and_all_given_findings_remain_exact() {
            let (environment, _) = environment(true);
            let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            assert_eq!(
                issues(&environment, Some(Language::Python), &[first])
                    .unwrap()
                    .len(),
                1
            );
            let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            let second = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            let result = issues(&environment, Some(Language::Python), &[first, second]).unwrap();
            assert_eq!(result.len(), 2);
            assert_eq!(result[0], result[1]);
            let wide = "é\0🙂".repeat(1_025);
            let result = issues(
                &environment,
                Some(Language::Python),
                &[finding(
                    Reason::InvalidInstalledVersion,
                    &["1.0"],
                    Some(&wide),
                )],
            )
            .unwrap();
            let context = yamaa_core::json::Value::Object(result[0].context.clone());
            assert_eq!(context["actual"], wide);
            let facts = (0..2_050)
                .map(|_| finding(Reason::VersionNotLocked, &[], None))
                .collect::<Vec<_>>();
            let result = issues(&environment, Some(Language::Python), &facts).unwrap();
            assert_eq!(result.len(), 2_050);
            assert_eq!(result[0], result[2_049]);
        }

        #[test]
        fn empty_unlocked_projection_preserves_success_without_lock_capture() {
            for ((environment, capture), environments) in
                [(environment(false), 1), (without_environment(), 0)]
            {
                assert_eq!(
                    issues(&environment, Some(Language::Python), &[]).unwrap(),
                    vec![]
                );
                assert_eq!((capture.environments, capture.locks), (environments, 0));
            }
        }
    }
}
