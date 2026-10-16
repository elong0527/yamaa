#![cfg(feature = "yaml")]
//! Project contracts.
#[allow(dead_code)]
mod support;
mod project {
    mod project_environment_projection {
        //! Complete retained-attempt witnesses; expected reports are authored independently.
        use std::{cell::Cell, rc::Rc, sync::Arc};
        use yamaa_adapters::{
            issue_rows::Issue,
            project_environment_diagnostics::{self as diagnostic, Error as ProjectionError},
            project_source::{
                self, CaptureFailure, CapturePort, Failure, Kind, Origin, Reply, Request,
            },
            specification_source::{CapturedSchema, Source},
        };
        use yamaa_core::json::{json, Value};
        use yamaa_core::{project_environment::LockKind, project_function::Language};
        // Deliberately neither Clone, Debug nor Send; the exact held error remains owned.
        struct Opaque {
            allocation: Rc<Cell<usize>>,
            text: String,
        }
        struct Port {
            error: Option<Opaque>,
            trace: Vec<(Kind, String)>,
            wide: bool,
            same_identity: bool,
        }
        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        fn schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit_root(
                vec![
                    source(
                        "schema_environment.yaml",
                        include_str!("fixtures/environment-candidate/schema_environment.yaml"),
                    ),
                    source(
                        "schema_shared.yaml",
                        include_str!("fixtures/environment-candidate/schema_shared.yaml"),
                    ),
                ],
                0,
                "environment_class",
            )
            .unwrap()
        }
        impl CapturePort for Port {
            type Error = Opaque;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                self.trace.push((request.kind, request.written.into()));
                if request.written == "missing.yaml" {
                    return Err(self.error.take().unwrap());
                }
                Ok(match request.kind {
                    Kind::Lock => Reply::Lock {
                        source: source("held/uv.lock", "version = 1\n"),
                        kind: LockKind::Uv,
                    },
                    Kind::Function => Reply::Document(source(
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
            let mut port = Port {
                error: Some(error),
                trace: vec![],
                wide,
                same_identity,
            };
            let body = if wide {
                "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [missing.yaml, ct.yaml]\n"
            } else {
                "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\ncodelists: [missing.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n"
            };
            let rejected = match project_source::prepare(
                schema(),
                source("held/environment.yaml", body),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::Rejected(rejected)) => rejected,
                _ => panic!("complete retained rejection"),
            };
            assert_eq!(
                port.trace,
                [
                    (Kind::Lock, "uv.lock".into()),
                    (Kind::Function, "function.yaml".into()),
                    (Kind::Codelist, "missing.yaml".into()),
                    (Kind::Codelist, "ct.yaml".into())
                ]
            );
            (rejected, port, allocation, pointer)
        }
        fn expected(condition: &str, requirement: &str, path: &str, context: Value) -> Issue {
            Issue {
                phase: "validation".into(),
                condition: condition.into(),
                requirement: Some(requirement.into()),
                spec_paths: vec![path.into()],
                context: context.to_string(),
            }
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
                    "REQ-0695",
                    "tests[0].result",
                    json!({"function":"first","case":"ordinary","value":true,"reason":"invalid_case_result","source":function_source,"entry":"held/environment.yaml","environment_path":"functions.first"}),
                )];
                for obligation in ["boundary", "normal"] {
                    want.push(expected("project_environment_invalid", "REQ-0695", "tests", json!({"function":"first","obligation":obligation,"reason":"missing_coverage_obligation","source":function_source,"entry":"held/environment.yaml","environment_path":"functions.first"})));
                }
                want.push(expected("codelist_shape_invalid", "REQ-0950", "codelists[0].items[0].value", json!({"codelist":"SEX","value":true,"source":"held/ct.yaml","entry":"held/environment.yaml","environment_path":"codelists[1]"})));
                want.push(expected("codelist_shape_invalid", "REQ-0950", "codelists[2].codelists[0].items[0].value", json!({"codelist":"OTHER","value":true,"source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"codelists[2]"})));
                let result = diagnostic::admission(&rejected).unwrap();
                assert_eq!(result, want);
                assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
                assert_eq!(port.trace.len(), 4);
                opaque(&rejected, &allocation, pointer);
            }
        }
        #[test]
        fn complete_external_scalar_reports_preserve_unbounded_integers_and_non_debug_originals() {
            let (rejected, port, allocation, pointer) = rejected(true, false);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), []);
            let wide: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
            let want = [
                expected(
                    "project_environment_invalid",
                    "REQ-0695",
                    "tests[0].result",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/function.yaml","entry":"held/environment.yaml","environment_path":"functions.first"}),
                ),
                expected(
                    "project_environment_invalid",
                    "REQ-0695",
                    "codelists[0].items[0].value",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/ct.yaml","entry":"held/environment.yaml","environment_path":"codelists[1]"}),
                ),
            ];
            let result = diagnostic::scalar_sources(&rejected).unwrap();
            assert_eq!(result, want);
            assert_eq!(diagnostic::scalar_sources(&rejected).unwrap(), result);
            assert_eq!(port.trace.len(), 4);
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
            assert_eq!(port.trace.len(), 4);
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
            let replacement_schema = schema();
            rejected.captures[0].document = replacement_schema.prepare_named_class(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n"), "function_definition_class").unwrap();
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            assert_eq!(port.trace.len(), 4);
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
            assert_eq!(port.trace.len(), 4);
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
            impl CapturePort for RefusingPort {
                type Error = ResourceError;
                fn capture(&mut self, _: Request<'_>) -> Result<Reply, Self::Error> {
                    self.reads += 1;
                    Err(ResourceError::Missing)
                }
            }
            let mut port = RefusingPort { reads: 0 };
            let root = source(
                "held/environment.yaml",
                "schema_version: '1.0'\ncodelists: [missing.yaml, ct.yaml]\n",
            );
            let Failure::Rejected(mut rejected) =
                project_source::prepare(schema(), root, Language::Python, &mut port).unwrap_err()
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
                let rows = project_application::failure(&error, Language::Python);
                assert_eq!(
                    rows,
                    [Issue {
                        phase: "engine".into(),
                        condition: "engine_rejected".into(),
                        requirement: None,
                        spec_paths: vec!["codelists[1]".into()],
                        context: json!({
                            "stage": "environment", "code": "resource_limit",
                            "resource": "spec_bytes", "limit": Limits::SPEC_BYTES,
                            "actual": Limits::SPEC_BYTES + 1,
                        })
                        .to_string(),
                    }],
                );
                assert_eq!(owner.root.source().bytes.as_ptr(), held_root);
                assert_eq!(owner.sources.as_ptr(), held_sources);
                assert_eq!(port.reads, 2);
            }
        }

        #[test]
        fn original_root_temporal_and_wide_scalar_report_keeps_independent_headers_without_capture_effects(
        ) {
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port {
                error: Some(Opaque {
                    allocation,
                    text: "never consumed".into(),
                }),
                trace: vec![],
                wide: false,
                same_identity: false,
            };
            let body = "schema_version: '1.0'\nlanguage: r\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n";
            let (root, findings) = match project_source::prepare(
                schema(),
                source("held/environment.yaml", body),
                Language::Python,
                &mut port,
            ) {
                Err(Failure::RootScalar { root, findings }) => (root, findings),
                _ => panic!("retained original root scalars"),
            };
            let wide: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
            let want = [
                expected(
                    "runner_language_mismatch",
                    "REQ-0696",
                    "language",
                    json!({"declared":"r","runner":"python","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
                ),
                expected(
                    "project_environment_invalid",
                    "REQ-0695",
                    "functions.wide.tests[0].result",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
                ),
                expected(
                    "project_environment_invalid",
                    "REQ-0695",
                    "functions.bad_date.tests[0].result",
                    json!({"reason":"invalid_date","value":"2024-02-30","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
                ),
                expected(
                    "project_environment_invalid",
                    "REQ-0695",
                    "functions.bad_time.tests[0].result",
                    json!({"reason":"invalid_datetime","value":"2024-01-01T25:00:00","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":"$"}),
                ),
            ];
            let result =
                diagnostic::scalar_root_with_host(&root, &findings, Language::Python).unwrap();
            assert_eq!(result, want);
            assert_eq!(
                diagnostic::scalar_root_with_host(&root, &findings, Language::Python).unwrap(),
                result
            );
            assert_eq!(port.trace, []);
            assert!(port.error.is_some());
        }

        #[test]
        fn actual_captured_two_hundred_inline_functions_keep_the_full_six_hundred_record_report() {
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port {
                error: Some(Opaque {
                    allocation,
                    text: "never consumed".into(),
                }),
                trace: vec![],
                wide: false,
                same_identity: false,
            };
            let mut body = String::from(
                "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n",
            );
            for index in 0..200 {
                body.push_str(&format!("  function_{index}: {{function: project.constant, description: Constant, params: [], returns: int, tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: true}}]}}\n"));
            }
            let rejected = match project_source::prepare(
                schema(),
                source("held/environment.yaml", &body),
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
                want.push(expected("project_environment_invalid", "REQ-0695", &format!("{declaration}.tests[0].result"), json!({"function":name,"case":"ordinary","value":true,"reason":"invalid_case_result","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":declaration})));
                for obligation in ["boundary", "normal"] {
                    want.push(expected("project_environment_invalid", "REQ-0695", &format!("{declaration}.tests"), json!({"function":name,"obligation":obligation,"reason":"missing_coverage_obligation","source":"held/environment.yaml","entry":"held/environment.yaml","environment_path":declaration})));
                }
            }
            assert_eq!(result, want);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
            assert_eq!(port.trace, [(Kind::Lock, "uv.lock".into())]);
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
                    "REQ-0700",
                ),
                (
                    FailureKind::MissingRequired {
                        parameter: "weight".into(),
                    },
                    "invalid_function_argument",
                    "REQ-0700",
                ),
                (
                    FailureKind::ArgumentType {
                        parameter: "weight".into(),
                        expected: ValueType::Float,
                        actual: ValueType::Int,
                    },
                    "invalid_function_argument",
                    "REQ-0700",
                ),
                (
                    FailureKind::BooleanResult,
                    "invalid_function_result",
                    "REQ-0702",
                ),
                (
                    FailureKind::UndeclaredMissing,
                    "invalid_function_result",
                    "REQ-0702",
                ),
                (
                    FailureKind::ResultType {
                        expected: ColumnType::Float,
                        actual: ValueType::Str,
                    },
                    "invalid_function_result",
                    "REQ-0702",
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
            for (kind, condition, requirement) in cases {
                let failure = make_failure(kind);
                let issue = invocation(
                    "rows[2].derivation.BMI.function",
                    &failure,
                    Some(&row),
                    &["MIN", "MAX", "TEXT"],
                    None,
                )
                .unwrap();
                assert_eq!(issue.phase, "derivation");
                assert_eq!(issue.condition, condition);
                assert_eq!(issue.requirement.as_deref(), Some(requirement));
                assert_eq!(issue.spec_paths, ["rows[2].derivation.BMI.function"]);
                let context: yamaa_core::json::Value =
                    serde_json::from_str(&issue.context).unwrap();
                assert_eq!(context["function"], "bmi");
                assert_eq!(context["call"], "projectbmi.bmi");
                assert_eq!(
                    context["keys"],
                    yamaa_core::json::json!([{"MIN":i64::MIN,"MAX":i64::MAX,"TEXT":"é\0🙂"}])
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
                let context: yamaa_core::json::Value =
                    serde_json::from_str(&issue.context).unwrap();
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
                let context: yamaa_core::json::Value =
                    serde_json::from_str(&issue.context).unwrap();
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
            assert_eq!(issue.condition, "invalid_function_result");
            assert_eq!(
                serde_json::from_str::<yamaa_core::json::Value>(&issue.context).unwrap()
                    ["returned"],
                "list"
            );
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
            let context: yamaa_core::json::Value = serde_json::from_str(&issue.context).unwrap();
            assert_eq!(context["unknown"], yamaa_core::json::json!([oversized]));
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
            for (result, condition, requirement, host_failure) in [
                (
                    Ok(Value::Bool(true)),
                    "invalid_function_result",
                    "REQ-0702",
                    false,
                ),
                (
                    Ok(Value::Missing),
                    "invalid_function_result",
                    "REQ-0702",
                    false,
                ),
                (
                    Ok(Value::float(7.0)),
                    "invalid_function_result",
                    "REQ-0702",
                    false,
                ),
                (
                    Err(HostError::Raised(payload.clone())),
                    "function_call_failed",
                    "REQ-0701",
                    true,
                ),
                (
                    Err(HostError::InvalidResult(payload.clone())),
                    "invalid_function_result",
                    "REQ-0702",
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
                assert_eq!(issue.condition, condition);
                assert_eq!(issue.requirement.as_deref(), Some(requirement));
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
        use std::sync::Arc;
        use yamaa_adapters::{
            project_lock::{Finding, Reason},
            project_lock_diagnostics::{issues, Error},
            project_source::{self, CapturePort, CapturedEnvironment, Kind, Reply, Request},
            specification_source::{CapturedSchema, Source},
        };
        use yamaa_core::{project_environment::LockKind, project_function::Language};

        struct Capture {
            calls: usize,
        }
        impl CapturePort for Capture {
            type Error = std::convert::Infallible;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                assert_eq!(request.kind, Kind::Lock);
                assert_eq!(request.written, "../uv.lock");
                self.calls += 1;
                Ok(Reply::Lock {
                    source: Source {
                        identity: "held/uv.lock".into(),
                        bytes: b"version = 1\n".to_vec(),
                    },
                    kind: LockKind::Uv,
                })
            }
        }
        fn environment(locked: bool) -> (CapturedEnvironment, Capture) {
            let schema = CapturedSchema::admit_root(
                vec![
                    Source {
                        identity: "schema_environment.yaml".into(),
                        bytes: include_bytes!(
                            "fixtures/environment-candidate/schema_environment.yaml"
                        )
                        .to_vec(),
                    },
                    Source {
                        identity: "schema_shared.yaml".into(),
                        bytes: include_bytes!("fixtures/environment-candidate/schema_shared.yaml")
                            .to_vec(),
                    },
                ],
                0,
                "environment_class",
            )
            .unwrap();
            let mut capture = Capture { calls: 0 };
            let source = Source {
                identity: "study/environment.yaml".into(),
                bytes: if locked {
                    b"schema_version: '1.0'\nlanguage: python\nlock: ../uv.lock\n".to_vec()
                } else {
                    b"schema_version: '1.0'\n".to_vec()
                },
            };
            let prepared = project_source::prepare(
                Arc::clone(&schema),
                source,
                Language::Python,
                &mut capture,
            )
            .unwrap();
            let (_, provenance) = prepared.into_owned().into_parts();
            (provenance, capture)
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
            let records = issues(&environment, &facts).unwrap();
            assert_eq!(records.len(), 7);
            let expected = [
                (
                    "distribution_not_identified",
                    yamaa_core::json::json!([]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "version_not_locked",
                    yamaa_core::json::json!([]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "ambiguous_lock_version",
                    yamaa_core::json::json!(["1.0", "2.0"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "invalid_locked_version",
                    yamaa_core::json::json!(["invalid\0é"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "package_not_installed",
                    yamaa_core::json::json!(["1.0"]),
                    yamaa_core::json::Value::Null,
                ),
                (
                    "invalid_installed_version",
                    yamaa_core::json::json!(["1.0"]),
                    yamaa_core::json::json!("bad\0🙂"),
                ),
                (
                    "version_mismatch",
                    yamaa_core::json::json!(["1.0"]),
                    yamaa_core::json::json!("2.0"),
                ),
            ];
            for (record, (reason, versions, actual)) in records.iter().zip(expected) {
                assert_eq!(record.phase, "validation");
                assert_eq!(record.condition, "runtime_artifact_mismatch");
                assert_eq!(record.requirement.as_deref(), Some("REQ-0697"));
                assert_eq!(record.spec_paths, ["lock"]);
                assert_eq!(
                    serde_json::from_str::<yamaa_core::json::Value>(&record.context).unwrap(),
                    yamaa_core::json::json!({"source":"study/environment.yaml", "entry":"study/environment.yaml",
                "lock_source":"held/uv.lock", "lock":"../uv.lock", "package":"program",
                "reason":reason, "expected":versions, "actual":actual})
                );
            }
            assert_eq!(issues(&environment, &facts).unwrap(), records);
            assert_eq!(capture.calls, 1);
            assert_eq!(environment.root().source().bytes.as_ptr(), root);
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
            let records = issues(&environment, &facts).unwrap();
            assert_eq!(records.len(), 3);
            let context: yamaa_core::json::Value =
                serde_json::from_str(&records[2].context).unwrap();
            assert_eq!(context["expected"], yamaa_core::json::json!(["1.0"]));
            assert_eq!(context["actual"], "1.0.0");
        }

        #[test]
        fn absent_lock_and_contradictory_facts_refuse_the_complete_projection() {
            let (unlocked, _) = environment(false);
            assert!(issues(&unlocked, &[]).unwrap().is_empty());
            let fact = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            assert_eq!(issues(&unlocked, &[fact]), Err(Error::InvalidContext));
            let (environment, _) = environment(true);
            for bad in [
                finding(Reason::VersionMismatch, &["1.0"], None),
                finding(Reason::VersionNotLocked, &["1.0"], None),
                finding(Reason::AmbiguousLockVersion, &["1.0"], None),
                finding(Reason::PackageNotInstalled, &["1.0"], Some("2.0")),
            ] {
                let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
                assert_eq!(
                    issues(&environment, &[first, bad]),
                    Err(Error::InvalidContext)
                );
            }
            assert!(Reason::parse("unclassified runtime error").is_none());
        }

        #[test]
        fn complete_escaped_host_versions_and_all_given_findings_remain_exact() {
            let (environment, _) = environment(true);
            let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            assert_eq!(issues(&environment, &[first]).unwrap().len(), 1);
            let first = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            let second = finding(Reason::VersionMismatch, &["1.0"], Some("2.0"));
            let result = issues(&environment, &[first, second]).unwrap();
            assert_eq!(result.len(), 2);
            assert_eq!(result[0], result[1]);
            let wide = "é\0🙂".repeat(1_025);
            let result = issues(
                &environment,
                &[finding(
                    Reason::InvalidInstalledVersion,
                    &["1.0"],
                    Some(&wide),
                )],
            )
            .unwrap();
            let context: yamaa_core::json::Value =
                serde_json::from_str(&result[0].context).unwrap();
            assert_eq!(context["actual"], wide);
            let facts = (0..2_050)
                .map(|_| finding(Reason::VersionNotLocked, &[], None))
                .collect::<Vec<_>>();
            let result = issues(&environment, &facts).unwrap();
            assert_eq!(result.len(), 2_050);
            assert_eq!(result[0], result[2_049]);
        }

        #[test]
        fn empty_unlocked_projection_preserves_success_without_capture() {
            let (environment, capture) = environment(false);
            assert_eq!(issues(&environment, &[]).unwrap(), vec![]);
            assert_eq!(capture.calls, 0);
        }
    }
    mod project_report {
        use crate::support::project_schemas as schemas;
        use std::{collections::BTreeMap, convert::Infallible, path::PathBuf, sync::Arc};
        use yamaa_adapters::{
            project_report,
            project_run::PreparedRun,
            project_source::{self, CapturePort, OwnedEnvironment, Reply, Request},
            specification_report::Identity,
            specification_source::Source,
        };
        use yamaa_core::{
            function_invocation::{Argument, HostError},
            project_activation::ActivationPort,
            specification_run::SourcePort,
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::LockReference,
            project_function::Language,
            specification::SourceDeclaration,
            value::Value,
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
            fn invoke(
                &mut self,
                _: &(),
                _: &[Argument<'_>],
            ) -> Result<Value, HostError<Infallible>> {
                panic!("no selected functions")
            }
        }
        struct Study {
            root: PathBuf,
            cached: BTreeMap<String, Arc<[u8]>>,
        }
        impl SourcePort for Study {
            fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = Infallible;
            fn capture_reads(&self) -> usize {
                self.cached.len()
            }
            fn capture(
                &mut self,
                declaration: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Infallible> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                let bytes = self
                    .cached
                    .entry(declaration.path.clone())
                    .or_insert_with(|| {
                        std::fs::read(self.root.join(&declaration.path))
                            .unwrap()
                            .into()
                    });
                Ok(Arc::clone(bytes))
            }
        }
        #[derive(Default)]
        struct Publisher(Vec<(String, Vec<u8>)>);
        impl yamaa_core::specification_output::ArtifactPort for Publisher {
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
                let result =
                    project_report::build_result(&run, &attempt, id(example), &[]).unwrap();
                let reads = study.capture_reads();
                let mut observed = result.observations();
                assert_eq!(
                    observed
                        .as_object_mut()
                        .unwrap()
                        .remove("activation")
                        .unwrap(),
                    yamaa_core::json::json!({"lock":"not_requested","bindings":[],"tests":[]})
                );
                let expected: yamaa_core::json::Value = serde_json::from_slice(
                    &std::fs::read(truth.join(format!("{example}.json"))).unwrap(),
                )
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
                        Err(yamaa_core::specification_output::SaveError::FailedBuild)
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
            let raw="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX, origin: {type: Assigned}}}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("spec.yaml", raw.as_bytes()))
                .unwrap();
            let run=PreparedRun::prepare(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
            let mut study = Study {
                root: PathBuf::new(),
                cached: BTreeMap::from([(
                    "input.csv".into(),
                    Arc::from(
                        b"ID,CODE\n9223372036854775807,X\n-9223372036854775808,Z\n".as_slice(),
                    ),
                )]),
            };
            let attempt = run.execute_with_ports(&mut NoActivation, &mut study);
            let result = project_report::build_result(&run, &attempt, id("codelist"), &[]).unwrap();
            assert_eq!(result.output(), None);
            assert_eq!(result.issues().len(), 1);
            let context: yamaa_core::json::Value =
                serde_json::from_str(&result.issues()[0].context).unwrap();
            assert_eq!(
                context,
                yamaa_core::json::json!({"column":"CODE","codelist":"SEX","failure_count":2,"keys":[{"ID":i64::MAX},{"ID":i64::MIN}],"values":["X","Z"]})
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
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
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
                fn capture(
                    &mut self,
                    _: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, Failure> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
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
                            yamaa_core::json::json!([{"phase":"validation","condition":"resource_path_missing","requirement":"REQ-0785","spec_paths":["input.SRC.path"],"context":{"dataset":"SRC","path":"absent.csv"}}])
                        );
                        assert_eq!(
                            report["source_reads"],
                            if inspect {
                                yamaa_core::json::json!([])
                            } else {
                                yamaa_core::json::json!([{"base_directory":".","path":"absent.csv","outcome":"failure","condition":"resource_path_missing","snapshots_created":0}])
                            }
                        );
                        assert_eq!(result.output(), None);
                        let mut publisher = Publisher::default();
                        assert!(matches!(
                            result.save(&mut publisher),
                            Err(yamaa_core::specification_output::SaveError::FailedBuild)
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
            let raw="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: str, derivation: SRC.CODE, submission: {codelist: SEX, origin: {type: Assigned}}}\n  - {name: LATE, type: str, derivation: SRC.CODE, verifications: [{matches: {pattern: '('}}]}\noutput: {path: output.csv, columns: [ID, CODE]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("spec.yaml", raw.as_bytes()))
                .unwrap();
            let run=PreparedRun::prepare(document,environment("codelists:\n  - codelists: [{id: SEX, name: Sex, data_type: text, items: [{value: F}, {value: M}]}]\n")).unwrap();
            let mut study = Study {
                root: PathBuf::new(),
                cached: BTreeMap::from([(
                    "input.csv".into(),
                    Arc::from(
                        b"ID,CODE\n9223372036854775807,F\n-9223372036854775808,M\n".as_slice(),
                    ),
                )]),
            };
            let attempt = run.execute_with_ports(&mut NoActivation, &mut study);
            let result =
                project_report::build_result(&run, &attempt, id("codelist-prefix"), &[]).unwrap();
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
            assert_eq!(
                observed["diagnostics"],
                yamaa_core::json::json!([{"phase":"validation","condition":"invalid_regex","requirement":"REQ-0827","spec_paths":["columns.LATE.verifications[0].matches.pattern"],"context":{"pattern":"("}}])
            );
            assert_eq!(result.output(), None);
            let mut publisher = Publisher::default();
            assert!(matches!(
                result.save(&mut publisher),
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
            ));
            assert!(publisher.0.is_empty());
        }
    }
    mod project_run {
        use crate::support::project_schemas as schemas;
        use std::{cell::RefCell, rc::Rc, sync::Arc};
        use yamaa_adapters::{
            project_run::{BoundaryFailure, PreparedRun},
            project_source::{self, CapturePort, Kind, OwnedEnvironment, Reply, Request},
            specification_source::{PreparedDocument, Source},
        };
        use yamaa_core::{
            dataset::ExecutionError,
            function_invocation::{Argument, FailureKind, HostError},
            project_activation::{ActivationPort, Failure, LockObservation, TestOutcome},
            specification_run::{PortError, SourcePort},
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::Language,
            specification::SourceDeclaration,
            table::{TableAccess, ValueRef},
            value::Value,
        };

        fn source(identity: &str, bytes: &[u8]) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.to_vec(),
            }
        }
        struct EnvironmentCapture;
        impl CapturePort for EnvironmentCapture {
            type Error = std::convert::Infallible;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
                assert_eq!(request.kind, Kind::Lock);
                assert_eq!(request.written, "../uv.lock");
                Ok(Reply::Lock {
                    source: source("held/uv.lock", b"version = 1\n"),
                    kind: LockKind::Uv,
                })
            }
        }
        fn environment() -> OwnedEnvironment {
            let schema = schemas::environment_schema();
            project_source::prepare(
        schema,
        source(
            "study/environment.yaml",
            br#"schema_version: '1.0'
language: python
lock: ../uv.lock
functions:
  unused:
    function: project.unused
    description: Uncalled definition.
    params: []
    returns: int
    tests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 7}]
  id:
    function: project.id
    description: Integer identity.
    params: [{name: x, type: int}]
    returns: int
    tests:
      - {id: ordinary, covers: [normal], args: {x: 7}, result: 7}
      - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775808}
      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}
"#,
        ),
        Language::Python,
        &mut EnvironmentCapture)
    .unwrap()
    .into_owned()
        }
        fn document(function: Option<&str>) -> PreparedDocument {
            let schema = schemas::domain_schema();
            let derivation = function.map_or_else(
                || "SRC.ID".into(),
                |name| format!("{{function: {{name: {name}, args: {{x: SRC.ID}}}}}}"),
            );
            let bytes = format!(
        "schema_version: '1.0'\ndomain: TEST\ninput: {{SRC: {{path: input.csv, types: {{ID: int}}}}}}\nkeys: [ID]\ncolumns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: VALUE, type: int, derivation: {derivation}}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n"
    );
            schema
                .prepare_standalone(source("study/domain.yaml", bytes.as_bytes()))
                .unwrap()
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
            values: Vec<i64>,
        }
        impl Activation {
            fn payload(&self, interrupt: bool) -> Payload {
                Payload {
                    original: Rc::clone(&self.original),
                    interrupt,
                }
            }
        }
        impl ActivationPort for Activation {
            type Handle = ();
            type Error = Payload;
            fn verify_lock(
                &mut self,
                language: Language,
                lock: &LockReference,
                functions: &[ProjectFunctionIdentity],
            ) -> Result<(), Payload> {
                assert_eq!(language, Language::Python);
                assert_eq!(lock.written, "../uv.lock");
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
                assert_eq!(signature.parameters()[0].name, "x");
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
                self.values.push(*value);
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
        impl SourcePort for Study {
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = std::convert::Infallible;
            fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), Self::Error> {
                assert_eq!(source.path, "input.csv");
                self.trace.borrow_mut().push("inspect");
                self.inspections += 1;
                Ok(())
            }
            fn capture_reads(&self) -> usize {
                usize::from(self.captured)
            }
            fn capture(
                &mut self,
                _: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Self::Error> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                self.trace.borrow_mut().push("capture");
                self.captured = true;
                Ok(Arc::clone(&self.bytes))
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
                    values: Vec::new(),
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
            for name in ["id", "unknown"] {
                let document = document(Some(name));
                let environment = environment();
                let document_bytes = document.source().bytes.as_ptr();
                let root_bytes = environment.root().source().bytes.as_ptr();
                let lock_bytes = environment.lock().unwrap().source.bytes.as_ptr();
                let definition = environment.environment().functions()[1]
                    .definition()
                    .tests
                    .as_ptr();
                let prepared = PreparedRun::prepare(document, environment);
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
                assert_eq!(captured.lock().unwrap().source.bytes.as_ptr(), lock_bytes);
                assert_eq!(
                    environment.functions()[1].definition().tests.as_ptr(),
                    definition
                );
                assert_eq!(captured.origins().functions, ["unused", "id"]);
            }
        }

        #[test]
        fn every_build_reactivates_only_called_definitions_on_exact_cached_study_bytes() {
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            assert_eq!(run.compiled().called_functions(), [1]);
            assert!(activation.trace.borrow().is_empty());
            for reads in [Some(1), Some(0)] {
                let attempt = run.execute_with_ports(&mut activation, &mut study);
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
                let observed = yamaa_adapters::project_activation_observations::activation(
                    &run,
                    &attempt.activation,
                )
                .unwrap();
                assert_eq!(
                    observed["tests"][1]["args"]["x"],
                    yamaa_core::json::json!({"int":"-9223372036854775808"})
                );
                assert_eq!(
                    observed["tests"][1]["actual"],
                    yamaa_core::json::json!({"int":"-9223372036854775808"})
                );
                assert_eq!(
                    observed["tests"][2]["actual"],
                    yamaa_core::json::json!({"missing":null})
                );
                assert_eq!(observed["tests"][2]["actual_retained"], true);
                assert_eq!(observed["tests"][2]["invoked"], false);
                assert_eq!(observed["tests"][0]["source"], "study/environment.yaml");
                assert_eq!(observed["tests"][0]["function"], "id");
                assert_eq!(observed["tests"][0]["call"], "project.id");
                assert_eq!(*activation.trace.borrow(), trace);
                assert!(yamaa_adapters::project_attempt::execution(&attempt).is_some());
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
            assert_eq!(study.capture_reads(), 1);
        }

        #[test]
        fn aggregate_derive_callbacks_activate_before_data_and_receive_each_donor_or_prior_binding()
        {
            for grouped_rows in [false, true] {
                for argument in ["SRC.ID", "BASE"] {
                    let rows = if grouped_rows {
                        "rows: [{id: all, group_by: [SRC.ID], derivations: {ID: SRC.ID}}]\n"
                    } else {
                        ""
                    };
                    let raw = format!(
                "schema_version: '1.0'\ndomain: TEST\ninput: {{SRC: {{path: input.csv, types: {{ID: int}}}}}}\nkeys: [ID]\n{rows}columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - name: VALUE\n    type: int\n    derivation:\n      aggregate:\n        derive:\n          - {{name: BASE, type: int, derivation: SRC.ID}}\n          - {{name: V, type: int, derivation: {{function: {{name: id, args: {{x: {argument}}}}}}}}}\n        expr: SUM(V)\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n"
            );
                    let document = schemas::domain_schema()
                        .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                        .unwrap();
                    let run = PreparedRun::prepare(document, environment())
                        .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
                    assert_eq!(run.compiled().called_functions(), [1]);
                    let (mut activation, mut study) = ports();
                    study.bytes = Arc::from(b"ID\n1\n1\n2\n".as_slice());
                    let attempt = run.execute_with_ports(&mut activation, &mut study);
                    assert!(attempt.boundary.is_ok());
                    let trace = activation.trace.borrow();
                    assert_eq!(&trace[..4], &["lock", "bind", "case", "case"]);
                    assert!(trace.iter().position(|event| *event == "capture").unwrap() > 3);
                    assert_eq!(activation.values, [7, i64::MIN, 1, 1, 2]);
                    assert_eq!(
                        attempt
                            .dataset
                            .result
                            .unwrap()
                            .result
                            .unwrap()
                            .dataset
                            .rows(),
                        &[
                            vec![Value::Int(1), Value::Int(2)],
                            vec![Value::Int(2), Value::Int(2)]
                        ]
                    );
                }
            }
        }

        #[test]
        fn aggregate_callback_reads_the_kept_current_row_instead_of_each_donor_record() {
            let raw = "schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int, N: int}}}\nkeys: [ID]\nintermediates:\n  - {id: CAP, dataset: SRC, key: {ID: ID}, order_by: [SRC.N], keep: first}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: int\n    derivation:\n      aggregate:\n        derive:\n          - {name: BASE, type: int, derivation: SRC.N}\n          - {name: V, type: int, derivation: {function: {name: id, args: {x: CAP.N}}}}\n        expr: SUM(V)\noutput: {path: output.csv, columns: [ID, VALUE]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                .unwrap();
            let run = PreparedRun::prepare(document, environment())
                .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
            let (mut activation, mut study) = ports();
            study.bytes = Arc::from(b"ID,N\n1,10\n1,30\n2,40\n".as_slice());
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            assert_eq!(activation.values, [7, i64::MIN, 10, 10, 40]);
            assert_eq!(
                attempt
                    .dataset
                    .result
                    .unwrap()
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                &[
                    vec![Value::Int(1), Value::Int(20)],
                    vec![Value::Int(2), Value::Int(40)]
                ]
            );
            let document = schemas::domain_schema()
                .prepare_standalone(source(
                    "study/domain.yaml",
                    raw.replace(", order_by: [SRC.N], keep: first", "")
                        .as_bytes(),
                ))
                .unwrap();
            let Err(rejected) = PreparedRun::prepare(document, environment()) else {
                panic!("kept scalar read required");
            };
            let issues = yamaa_adapters::project_check::rejected(&rejected).unwrap();
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.condition == "invalid_derive_step"
                        && issue.requirement.as_deref() == Some("REQ-1242")),
                "{issues:?}"
            );
        }

        #[test]
        fn aggregate_callback_uses_original_current_source_types_when_donor_types_differ() {
            let raw = "schema_version: '1.0'\ndomain: TEST\ninput:\n  SRC: {path: input.csv, types: {ID: int, N: int}}\n  B: {path: input.csv, types: {ID: int, N: str}}\nbase: SRC\nkeys: [ID, N]\nintermediates:\n  - {id: CAP, dataset: SRC, key: {N: SRC.N}, order_by: [SRC.ID], keep: first}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: N, type: int, derivation: SRC.N}\n  - name: VALUE\n    type: int\n    derivation:\n      aggregate:\n        key: {ID: ID}\n        derive:\n          - {name: BASE, type: int, derivation: B.N}\n          - {name: V, type: int, derivation: {function: {name: id, args: {x: CAP.N}}}}\n        expr: SUM(V)\noutput: {path: output.csv, columns: [ID, N, VALUE]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                .unwrap();
            let run = PreparedRun::prepare(document, environment())
                .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
            let (mut activation, mut study) = ports();
            study.bytes = Arc::from(b"ID,N\n1,10\n1,30\n2,40\n".as_slice());
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            assert!(
                attempt
                    .dataset
                    .result
                    .as_ref()
                    .is_ok_and(|entered| entered.result.is_ok()),
                "{:?}",
                attempt
                    .dataset
                    .result
                    .as_ref()
                    .err()
                    .map(|error| format!("{error:?}"))
                    .or_else(|| attempt
                        .dataset
                        .result
                        .as_ref()
                        .ok()
                        .and_then(|entered| entered
                            .result
                            .as_ref()
                            .err()
                            .map(|error| format!("{error:?}"))))
            );
            assert_eq!(activation.values, [7, i64::MIN, 10, 10, 30, 30, 40]);
            assert_eq!(
                attempt
                    .dataset
                    .result
                    .unwrap()
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                &[
                    vec![Value::Int(1), Value::Int(10), Value::Int(20)],
                    vec![Value::Int(1), Value::Int(30), Value::Int(60)],
                    vec![Value::Int(2), Value::Int(40), Value::Int(40)]
                ]
            );
        }

        #[test]
        fn rejected_activation_preserves_original_payloads_and_has_zero_study_authority() {
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            for mode in [
                Mode::LockFailure,
                Mode::BindFailure,
                Mode::CaseFailure,
                Mode::CaseInterrupt,
            ] {
                let (mut activation, mut study) = ports();
                activation.mode = mode;
                let attempt = run.execute_with_ports(&mut activation, &mut study);
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
                yamaa_adapters::project_attempt::visit_host_failures(&attempt, |_, original| {
                    assert!(std::ptr::eq(original, payload));
                    visited += 1;
                });
                assert_eq!(visited, 1);
                assert!(yamaa_adapters::project_attempt::execution(&attempt).is_none());
                assert!(Rc::ptr_eq(&payload.original, &activation.original));
                assert_eq!(payload.interrupt, matches!(mode, Mode::CaseInterrupt));
                assert!(attempt.dataset.sources.is_empty());
                assert!(matches!(attempt.dataset.result, Err(PortError::Incomplete)));
                assert_eq!(study.inspections, 0);
                assert_eq!(study.capture_reads(), 0);
                if matches!(mode, Mode::CaseFailure | Mode::CaseInterrupt) {
                    assert_eq!(attempt.activation.tests[0].actual, Some(Value::Int(7)));
                    assert_eq!(attempt.activation.tests[0].outcome, TestOutcome::Passed);
                }
            }
        }

        #[test]
        fn unwound_activation_keeps_original_boundary_payload_and_completed_case_evidence() {
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::CaseUnwind;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
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
            let observed = yamaa_adapters::project_activation_observations::activation(
                &run,
                &attempt.activation,
            )
            .unwrap();
            assert_eq!(observed["tests"].as_array().unwrap().len(), 2);
            assert_eq!(
                observed["tests"][0]["actual"],
                yamaa_core::json::json!({"int":"7"})
            );
            assert_eq!(observed["tests"][1]["outcome"], "attempted");
            assert_eq!(observed["tests"][1]["actual_retained"], false);
            assert!(observed["tests"][1]["actual"].is_null());
            assert!(attempt.dataset.sources.is_empty());
            assert_eq!(study.inspections, 0);
            activation.mode = Mode::Pass;
            let next = run.execute_with_ports(&mut activation, &mut study);
            assert!(next.boundary.is_ok());
            assert_eq!(next.activation.tests.len(), 3);
            assert!(next.dataset.result.unwrap().result.is_ok());
            // The held failed attempt remains intact while a fresh attempt succeeds.
            assert_eq!(attempt.activation.tests[1].outcome, TestOutcome::Attempted);
        }

        #[test]
        fn live_host_failure_retains_exact_study_snapshot_and_typed_activation_success() {
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::LiveFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
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
            yamaa_adapters::project_attempt::visit_host_failures(&attempt, |stage, original| {
                assert_eq!(stage, "derivation");
                assert!(std::ptr::eq(original, payload));
                visited += 1;
            });
            assert_eq!(visited, 1);
            assert!(yamaa_adapters::project_attempt::execution(&attempt).is_none());
            assert_eq!(study.capture_reads(), 1);
            assert_eq!(
                &*activation.trace.borrow(),
                &["lock", "bind", "case", "case", "inspect", "capture", "live"]
            );
        }

        #[test]
        fn unused_environment_does_not_request_any_host_activation() {
            let run = PreparedRun::prepare(document(None), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::LockFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            assert!(attempt.boundary.is_ok());
            assert_eq!(attempt.activation.lock, LockObservation::NotRequested);
            assert!(attempt.activation.bindings.is_empty());
            assert!(attempt.activation.tests.is_empty());
            assert!(attempt.dataset.result.unwrap().result.is_ok());
            assert_eq!(&*activation.trace.borrow(), &["inspect", "capture"]);
        }

        #[test]
        fn activation_projection_validates_forged_case_order_without_effects() {
            use yamaa_adapters::project_activation_observations::{activation, Error};
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut host, mut study) = ports();
            let mut attempt = run.execute_with_ports(&mut host, &mut study);
            let trace = host.trace.borrow().clone();
            let reads = study.capture_reads();
            assert!(activation(&run, &attempt.activation).is_ok());
            attempt.activation.tests.swap(0, 1);
            assert_eq!(
                activation(&run, &attempt.activation),
                Err(Error::InvalidObservation)
            );
            assert_eq!(*host.trace.borrow(), trace);
            assert_eq!(study.capture_reads(), reads);
        }

        #[test]
        fn repeated_forged_activation_facts_cannot_gain_report_authority() {
            use yamaa_adapters::{project_activation_observations as activation, project_report};
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut host, mut study) = ports();
            let mut attempt = run.execute_with_ports(&mut host, &mut study);
            let trace = host.trace.borrow().clone();
            let reads = study.capture_reads();
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
                    project_report::build_result(&run, &attempt, report_identity(), &[]),
                    Err(project_report::Error::Report(
                        yamaa_adapters::specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert_eq!(*host.trace.borrow(), trace);
            assert_eq!(study.capture_reads(), reads);
        }

        fn report_identity() -> yamaa_adapters::specification_report::Identity<'static> {
            yamaa_adapters::specification_report::Identity {
                runtime: "test",
                runtime_version: "test-1",
                engine_version: "0.1.0",
                example: "owned-project",
                specification: "study/domain.yaml",
                base_directory: "study",
            }
        }
        #[derive(Default)]
        struct Publication {
            calls: Vec<(String, Vec<u8>)>,
            reject: bool,
        }
        impl yamaa_core::specification_output::ArtifactPort for Publication {
            type Error = Rc<()>;
            fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), Self::Error> {
                self.calls.push((path.into(), bytes.into()));
                if self.reject {
                    Err(Rc::new(()))
                } else {
                    Ok(())
                }
            }
        }

        #[test]
        fn complete_project_output_and_save_borrow_the_exact_attempt_without_reactivation() {
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            let trace = activation.trace.borrow().clone();
            let snapshot = attempt.dataset.sources[0]
                .snapshot
                .as_ref()
                .unwrap()
                .as_ptr();
            let result = yamaa_adapters::project_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[],
            )
            .unwrap();
            let report = result.observations();
            assert_eq!(report["outcome"], "success");
            assert_eq!(report["artifacts"], yamaa_core::json::json!([]));
            assert_eq!(report["diagnostics"], yamaa_core::json::json!([]));
            assert_eq!(
                report["source_reads"],
                yamaa_core::json::json!([{"base_directory":"study","path":"input.csv","outcome":"captured","condition":null,"snapshots_created":1}])
            );
            assert_eq!(
                report["tables"],
                yamaa_core::json::json!([
                    {"specification":"study/domain.yaml","stage":"source","name":"SRC","columns":["ID"],"types":["int"],"rows":[[{"type":"int","value":"1"}]]},
                    {"specification":"study/domain.yaml","stage":"derived","name":"output","columns":["ID","VALUE"],"types":["int","int"],"rows":[[{"type":"int","value":"1"},{"type":"int","value":"1"}]]}
                ])
            );
            assert_eq!(report["activation"]["lock"], "verified");
            assert_eq!(report["activation"]["tests"].as_array().unwrap().len(), 3);
            assert_eq!(
                report["activation"]["tests"][2]["actual"],
                yamaa_core::json::json!({"missing":null})
            );
            assert!(result.output().is_some());
            assert!(result.issues().is_empty());
            let mut publisher = Publication {
                reject: true,
                ..Default::default()
            };
            assert!(matches!(
                result.save(&mut publisher),
                Err(yamaa_core::specification_output::SaveError::Publish(_))
            ));
            publisher.reject = false;
            let saved = result.save(&mut publisher).unwrap();
            assert_eq!(
                publisher.calls,
                [
                    ("output.csv".into(), b"ID,VALUE\n1,1\n".to_vec()),
                    ("output.csv".into(), b"ID,VALUE\n1,1\n".to_vec())
                ]
            );
            assert_eq!(saved["artifacts"][0]["content"], "ID,VALUE\n1,1\n");
            assert_eq!(saved["artifacts"][0]["row_count"], 1);
            assert_eq!(result.observations(), report);
            assert_eq!(*activation.trace.borrow(), trace);
            assert_eq!(study.capture_reads(), 1);
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
        fn project_reports_keep_opaque_failures_and_interrupts_original_and_failed_saves_inert() {
            use yamaa_adapters::{
                project_function_diagnostics::HostDetails,
                project_report::{Classified, Error},
            };
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            for mode in [
                Mode::BindFailure,
                Mode::CaseFailure,
                Mode::LiveFailure,
                Mode::CaseInterrupt,
                Mode::CaseUnwind,
            ] {
                let (mut activation, mut study) = ports();
                activation.mode = mode;
                let attempt = run.execute_with_ports(&mut activation, &mut study);
                let trace = activation.trace.borrow().clone();
                let unclassified = yamaa_adapters::project_report::build_result(
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
                    let result = yamaa_adapters::project_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &facts,
                    )
                    .unwrap();
                    let observed = result.observations();
                    assert_eq!(observed["outcome"], "failure");
                    assert_eq!(observed["artifacts"], yamaa_core::json::json!([]));
                    assert_eq!(result.output(), None);
                    assert_eq!(result.issues().len(), 1);
                    let expected = if matches!(mode, Mode::BindFailure) {
                        "project_environment_invalid"
                    } else if matches!(mode, Mode::CaseFailure) {
                        "function_conformance_failed"
                    } else {
                        "function_call_failed"
                    };
                    assert_eq!(result.issues()[0].condition, expected);
                    if matches!(mode, Mode::LiveFailure) {
                        assert_eq!(observed["source_reads"].as_array().unwrap().len(), 1);
                        assert_eq!(observed["activation"]["tests"].as_array().unwrap().len(), 3);
                    } else {
                        assert_eq!(observed["source_reads"], yamaa_core::json::json!([]));
                    }
                    let mut publisher = Publication::default();
                    assert!(matches!(
                        result.save(&mut publisher),
                        Err(yamaa_core::specification_output::SaveError::FailedBuild)
                    ));
                    assert!(publisher.calls.is_empty());
                    assert!(Rc::ptr_eq(&held.original, &activation.original));
                }
                assert_eq!(*activation.trace.borrow(), trace);
                assert_eq!(
                    study.capture_reads(),
                    usize::from(matches!(mode, Mode::LiveFailure))
                );
            }
        }

        #[test]
        fn report_classifications_cannot_target_a_different_failure_or_supply_duplicates() {
            use yamaa_adapters::{
                project_function_diagnostics::HostDetails,
                project_report::{Classified, Error},
            };
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::BindFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
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
                    yamaa_adapters::project_report::build_result(
                        &run,
                        &attempt,
                        report_identity(),
                        &facts
                    ),
                    Err(Error::Report(
                        yamaa_adapters::specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert_eq!(study.capture_reads(), 0);
            assert_eq!(*activation.trace.borrow(), ["lock", "bind"]);
        }

        #[test]
        fn complete_reports_refuse_missing_or_contradictory_activation_success() {
            use yamaa_adapters::{project_report::Error, specification_report};
            use yamaa_core::project_activation::BindingOutcome;
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            for change in 0..5 {
                let (mut activation, mut study) = ports();
                let mut attempt = run.execute_with_ports(&mut activation, &mut study);
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
                    yamaa_adapters::project_report::build_result(
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
                assert_eq!(study.capture_reads(), 1);
                assert!(attempt.dataset.result.as_ref().unwrap().result.is_ok());
            }
        }

        #[test]
        fn lock_and_actual_conformance_mismatches_keep_complete_zero_read_reports() {
            use yamaa_adapters::{
                project_lock::{Finding, Reason},
                project_report::{Classified, Error},
            };
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (mut activation, mut study) = ports();
            activation.mode = Mode::LockFailure;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
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
            let result = yamaa_adapters::project_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[Classified::Lock {
                    failure: original,
                    findings: &findings,
                }],
            )
            .unwrap();
            assert_eq!(result.issues()[0].condition, "runtime_artifact_mismatch");
            assert_eq!(
                serde_json::from_str::<yamaa_core::json::Value>(&result.issues()[0].context)
                    .unwrap(),
                yamaa_core::json::json!({"source":"study/environment.yaml","entry":"study/environment.yaml","lock_source":"held/uv.lock","lock":"../uv.lock","package":"project","reason":"distribution_not_identified","expected":[],"actual":null})
            );
            assert_eq!(
                result.observations()["source_reads"],
                yamaa_core::json::json!([])
            );
            assert_eq!(result.observations()["activation"]["lock"], "rejected");
            assert_eq!(*activation.trace.borrow(), ["lock"]);
            assert_eq!(study.capture_reads(), 0);
            assert!(Rc::ptr_eq(&original.original, &activation.original));
            assert!(matches!(
                yamaa_adapters::project_report::build_result(
                    &run,
                    &attempt,
                    report_identity(),
                    &[Classified::Lock {
                        failure: original,
                        findings: &[]
                    }]
                ),
                Err(Error::Report(_))
            ));
            let (mut activation, mut study) = ports();
            activation.mode = Mode::CaseMismatch;
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            let result = yamaa_adapters::project_report::build_result(
                &run,
                &attempt,
                report_identity(),
                &[],
            )
            .unwrap();
            assert_eq!(result.issues()[0].condition, "function_conformance_failed");
            assert_eq!(result.issues()[0].spec_paths, ["functions.id.tests[0]"]);
            let context: yamaa_core::json::Value =
                serde_json::from_str(&result.issues()[0].context).unwrap();
            assert_eq!(context["expected"], 7);
            assert_eq!(context["actual"], 8);
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
            let mut publisher = Publication::default();
            assert!(matches!(
                result.save(&mut publisher),
                Err(yamaa_core::specification_output::SaveError::FailedBuild)
            ));
            assert!(publisher.calls.is_empty());
            assert_eq!(study.capture_reads(), 0);
            assert_eq!(*activation.trace.borrow(), ["lock", "bind", "case", "case"]);
        }

        #[test]
        fn zero_sized_host_failures_remain_opaque_and_never_accept_address_classification() {
            use yamaa_adapters::{
                project_function_diagnostics::HostDetails,
                project_report::{self, Classified, Error},
                specification_report,
            };
            struct EmptyFailure;
            struct RejectLock;
            impl ActivationPort for RejectLock {
                type Handle = ();
                type Error = EmptyFailure;
                fn verify_lock(
                    &mut self,
                    _: Language,
                    _: &LockReference,
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
            let run = PreparedRun::prepare(document(Some("id")), environment()).unwrap();
            let (_, mut study) = ports();
            let attempt = run.execute_with_ports(&mut RejectLock, &mut study);
            let Err(BoundaryFailure::Activation(Failure::Lock(original))) = &attempt.boundary
            else {
                panic!("held original zero-sized lock failure")
            };
            assert!(matches!(
                project_report::build_result(&run, &attempt, report_identity(), &[]),
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
                    project_report::build_result(&run, &attempt, report_identity(), &facts),
                    Err(Error::Report(
                        specification_report::Error::InvalidObservation
                    ))
                ));
            }
            assert!(matches!(
                attempt.boundary,
                Err(BoundaryFailure::Activation(Failure::Lock(_)))
            ));
            assert_eq!(study.capture_reads(), 0);
        }

        #[test]
        fn row_callbacks_join_primary_or_secondary_records_by_completed_keys() {
            struct TwoSources {
                trace: Trace,
                reads: usize,
            }
            impl SourcePort for TwoSources {
                fn resource_failure(
                    &self,
                    _error: &Self::Error,
                ) -> Option<yamaa_core::resource::ResourceFailure> {
                    None
                }
                type Error = std::convert::Infallible;
                fn inspect(&mut self, _: &SourceDeclaration) -> Result<(), Self::Error> {
                    self.trace.borrow_mut().push("inspect");
                    Ok(())
                }
                fn capture_reads(&self) -> usize {
                    self.reads
                }
                fn capture(
                    &mut self,
                    source: &SourceDeclaration,
                    origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
                ) -> Result<Arc<[u8]>, Self::Error> {
                    *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                    self.reads += 1;
                    self.trace.borrow_mut().push("capture");
                    Ok(Arc::from(match source.name.as_str() {
                        "SRC" => b"ID,N\n1,10\n2,20\n3,30\n".as_slice(),
                        "B" => b"ID,N\n2,40\n1,50\n".as_slice(),
                        _ => panic!("known source"),
                    }))
                }
            }
            for (driver, other, expected, live) in [
                (
                    "SRC",
                    "B",
                    vec![
                        vec![Value::Int(1), Value::Int(50)],
                        vec![Value::Int(2), Value::Int(40)],
                        vec![Value::Int(3), Value::Missing],
                    ],
                    vec![50, 40],
                ),
                (
                    "B",
                    "SRC",
                    vec![
                        vec![Value::Int(2), Value::Int(20)],
                        vec![Value::Int(1), Value::Int(10)],
                    ],
                    vec![20, 10],
                ),
            ] {
                let raw = format!("schema_version: '1.0'\ndomain: TEST\ninput:\n  SRC: {{path: a.csv, types: {{ID: int, N: int}}}}\n  B: {{path: b.csv, types: {{ID: int, N: int}}}}\nbase: SRC\nkeys: [ID]\ncolumns:\n  - {{name: ID, type: int}}\n  - {{name: VALUE, type: int}}\nrows:\n  - id: records\n    dataset: {driver}\n    derivations:\n      ID: {driver}.ID\n      VALUE: {{function: {{name: id, args: {{x: {other}.N}}}}}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n");
                let document = schemas::domain_schema()
                    .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                    .unwrap();
                let run = PreparedRun::prepare(document, environment())
                    .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
                let (mut activation, _) = ports();
                let mut study = TwoSources {
                    trace: Rc::clone(&activation.trace),
                    reads: 0,
                };
                let attempt = run.execute_with_ports(&mut activation, &mut study);
                let entered = attempt
                    .dataset
                    .result
                    .unwrap_or_else(|error| panic!("{error:?}"));
                let result = entered.result.unwrap_or_else(|error| panic!("{error:?}"));
                assert_eq!(result.dataset.rows(), expected.as_slice());
                let mut values = vec![7, i64::MIN];
                values.extend(live);
                assert_eq!(activation.values, values);
                assert_eq!(
                    &activation.trace.borrow()[..4],
                    &["lock", "bind", "case", "case"]
                );
            }
        }

        #[test]
        fn intermediate_derive_callbacks_activate_before_data_and_receive_donor_or_prior_values() {
            for grouped_rows in [false, true] {
                for argument in ["SRC.ID", "BASE"] {
                    let rows = if grouped_rows {
                        "rows: [{id: all, group_by: [SRC.ID], derivations: {ID: SRC.ID}}]\n"
                    } else {
                        ""
                    };
                    let raw = format!("schema_version: '1.0'\ndomain: TEST\ninput: {{SRC: {{path: input.csv, types: {{ID: int}}}}}}\nkeys: [ID]\n{rows}intermediates:\n  - id: DON\n    dataset: SRC\n    key: [ID]\n    order_by: [SRC.ID]\n    keep: first\n    derivations:\n      BASE: SRC.ID\n      V: {{function: {{name: id, args: {{x: {argument}}}}}}}\ncolumns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n  - {{name: VALUE, type: int, derivation: DON.V}}\noutput: {{path: output.csv, columns: [ID, VALUE]}}\n");
                    let document = schemas::domain_schema()
                        .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                        .unwrap();
                    let run = PreparedRun::prepare(document, environment())
                        .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
                    assert_eq!(run.compiled().called_functions(), [1]);
                    let (mut activation, mut study) = ports();
                    study.bytes = Arc::from(b"ID\n1\n1\n2\n".as_slice());
                    let attempt = run.execute_with_ports(&mut activation, &mut study);
                    assert!(attempt.boundary.is_ok());
                    let trace = activation.trace.borrow();
                    assert_eq!(&trace[..4], &["lock", "bind", "case", "case"]);
                    assert!(trace.iter().position(|event| *event == "capture").unwrap() > 3);
                    assert_eq!(activation.values, [7, i64::MIN, 1, 1, 2]);
                    let entered = attempt
                        .dataset
                        .result
                        .unwrap_or_else(|error| panic!("{error:?}"));
                    let result = entered.result.unwrap_or_else(|error| panic!("{error:?}"));
                    assert_eq!(
                        result.dataset.rows(),
                        &[
                            vec![Value::Int(1), Value::Int(1)],
                            vec![Value::Int(2), Value::Int(2)]
                        ]
                    );
                }
            }
        }

        #[test]
        fn intermediate_callback_selects_for_donor_record_and_keeps_selected_field_types() {
            let raw = "schema_version: '1.0'\ndomain: TEST\ninput:\n  SRC: {path: input.csv, types: {ID: int, N: int}}\n  B: {path: input.csv, types: {ID: int, N: str}}\nbase: SRC\nkeys: [ID, N]\nintermediates:\n  - {id: CAP, dataset: SRC, key: [ID], order_by: [SRC.N], keep: first}\n  - id: DON\n    dataset: B\n    key: {ID: ID}\n    order_by: [B.ID]\n    keep: first\n    derivations:\n      V: {function: {name: id, args: {x: CAP.N}}}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: N, type: int, derivation: SRC.N}\n  - {name: VALUE, type: int, derivation: DON.V}\noutput: {path: output.csv, columns: [ID, N, VALUE]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                .unwrap();
            let run = PreparedRun::prepare(document, environment())
                .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
            let (mut activation, mut study) = ports();
            study.bytes = Arc::from(b"ID,N\n1,10\n1,30\n2,40\n".as_slice());
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            assert!(attempt.boundary.is_ok());
            let entered = attempt
                .dataset
                .result
                .unwrap_or_else(|error| panic!("{error:?}"));
            let result = entered.result.unwrap_or_else(|error| panic!("{error:?}"));
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![Value::Int(1), Value::Int(10), Value::Int(10)],
                    vec![Value::Int(1), Value::Int(30), Value::Int(10)],
                    vec![Value::Int(2), Value::Int(40), Value::Int(40)]
                ]
            );
            assert_eq!(activation.values, [7, i64::MIN, 10, 10, 40]);
        }

        #[test]
        fn callback_derived_fields_are_not_exposed_as_inferred_row_driver_types() {
            let raw = "schema_version: '1.0'\ndomain: TEST\ninput: {SRC: {path: input.csv, types: {ID: int}}}\nkeys: [ID]\nintermediates:\n  - id: DON\n    dataset: SRC\n    derivations:\n      V: {function: {name: id, args: {x: SRC.ID}}}\ncolumns:\n  - {name: ID, type: int}\nrows:\n  - id: records\n    dataset: DON\n    derivations: {ID: DON.ID}\noutput: {path: output.csv, columns: [ID]}\n";
            let document = schemas::domain_schema()
                .prepare_standalone(source("study/domain.yaml", raw.as_bytes()))
                .unwrap();
            let run = PreparedRun::prepare(document, environment())
                .unwrap_or_else(|run| panic!("{:?}", run.rejection.error));
            let (mut activation, mut study) = ports();
            study.bytes = Arc::from(b"ID\n1\n".as_slice());
            let attempt = run.execute_with_ports(&mut activation, &mut study);
            let error = attempt.dataset.result.unwrap_err();
            let PortError::Run(yamaa_core::specification_run::RunError::Bind(
                yamaa_core::specification::BindError::Invalid(findings),
            )) = error
            else {
                panic!("{error:?}")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|finding| finding.diagnostics(run.compiled().source()).unwrap())
                .collect::<Vec<_>>();
            assert!(
                diagnostics.iter().any(|d| d.definition().condition
                    == "unknown_intermediate_driver_type"
                    && d.definition().requirement == Some("REQ-1262")
                    && d.spec_paths == ["rows[0].dataset"]),
                "{diagnostics:?}"
            );
            assert_eq!(activation.values, [7, i64::MIN]);
        }
    }
}
