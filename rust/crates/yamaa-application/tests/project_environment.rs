#![cfg(feature = "yaml")]
//! Project contracts: project environment projection, project function
//! diagnostics, project lock and project lock diagnostics.
mod project {
    mod project_environment_projection {
        //! Complete retained-attempt witnesses; expected reports are authored independently.
        use std::{cell::Cell, rc::Rc};
        use yamaa_adapters::json;
        use yamaa_adapters::{
            document::{self, Closures},
            project_environment_diagnostics::{self as diagnostic, Error as ProjectionError},
            project_source::{self, CaptureFailure, Failure, Kind, Origin},
            specification_source::Source,
        };
        use yamaa_core::json::Value;
        use yamaa_core::language::{FileType, LANGUAGE_VERSION};
        use yamaa_core::resolver::{self, Captured, File, Located, Name, Resolver};
        use yamaa_core::{project_environment::LockKind, project_function::Language};
        /// The project lists one function file and holds the Python lock, each
        /// relative to itself (D8).
        const LISTING: &str =
            "yamaa: '1.0'\ntype: project\nfunctions: [function.yaml]\nenvironments: {python: {lock: uv.lock}}\n";
        // Deliberately neither Clone, Debug nor Send; the exact held error remains owned.
        struct Opaque {
            allocation: Rc<Cell<usize>>,
            text: String,
        }
        struct Port {
            error: Option<Opaque>,
            trace: Vec<(Kind, String)>,
            lock_kind: LockKind,
            wide: bool,
            same_identity: bool,
            /// The bytes of the file last located.
            held: Vec<u8>,
        }
        impl Port {
            fn new(error: Opaque) -> Self {
                Self {
                    error: Some(error),
                    trace: vec![],
                    lock_kind: LockKind::Uv,
                    wide: false,
                    same_identity: false,
                    held: vec![],
                }
            }
        }
        fn source(identity: &str, text: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        /// A function file defining `name`, whose one case returns `result`.
        fn function_file(name: &str, result: &str) -> String {
            format!("yamaa: '1.0'\ntype: function\nname: {name}\ndescription: Constant\nparams: []\nreturns: int\nimplementations: {{python: project.constant}}\ntests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: {result}}}]\n")
        }
        impl Port {
            /// The held source the double serves for the file a project names:
            /// `function.yaml` defines `first`, and `functions/<name>.yaml`
            /// defines `<name>`.
            fn answer(
                &mut self,
                kind: Kind,
                _declaring: &str,
                written: &str,
            ) -> Result<Source, Opaque> {
                self.trace.push((kind, written.into()));
                if written == "missing.yaml" {
                    return Err(self.error.take().unwrap());
                }
                let result = if self.wide {
                    "123456789012345678901234567890"
                } else {
                    "true"
                };
                Ok(match kind {
                    Kind::Lock => {
                        source("held/uv.lock", crate::support::ports::lock(self.lock_kind))
                    }
                    Kind::Function => match written.strip_prefix("functions/") {
                        Some(file) => source(
                            &format!("held/{written}"),
                            &function_file(file.trim_end_matches(".yaml"), result),
                        ),
                        None => source(
                            // The same spelling as the project file still
                            // names the separately captured function file.
                            if self.same_identity {
                                "held/project.yaml"
                            } else {
                                "held/function.yaml"
                            },
                            &function_file("first", result),
                        ),
                    },
                    Kind::Codelist => source(
                        "held/ct.yaml",
                        if self.wide {
                            "yamaa: '1.0'\ntype: codelists\ncodelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n"
                        } else {
                            "yamaa: '1.0'\ntype: codelists\ncodelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n"
                        },
                    ),
                })
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for Port {
            type Error = Opaque;
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
        fn prepare(
            port: &mut Port,
            body: &str,
        ) -> Result<project_source::PreparedEnvironment, Failure<Opaque>> {
            project_source::prepare(
                Closures::shipped().unwrap(),
                source("held/project.yaml", body),
                Language::Python,
                port,
            )
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
                format!("{LISTING}codelists: [missing.yaml, ct.yaml]\n")
            } else {
                format!("{LISTING}codelists: [missing.yaml, ct.yaml, {{codelists: [{{id: OTHER, name: Other, items: [{{value: true}}]}}]}}]\n")
            };
            let rejected = match prepare(&mut port, &body) {
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
        /// The bounds of `int` beside a literal outside it (VALUE-5).
        /// A literal outside int's range: its value as a number and as
        /// written, and the range (VALUE-5).
        fn overflow(actual: &str) -> [(&'static str, Value); 4] {
            let number = |text: &str| -> Value { yamaa_adapters::json_wire::decode(text).unwrap() };
            [
                ("actual", number(actual)),
                ("value", Value::String(actual.into())),
                ("minimum", number("-9223372036854775808")),
                ("maximum", number("9223372036854775807")),
            ]
        }
        fn with(fields: &[(&'static str, Value)], provenance: Value) -> Value {
            let Value::Object(mut context) = provenance else {
                unreachable!()
            };
            context.extend(
                fields
                    .iter()
                    .map(|(key, value)| ((*key).into(), value.clone())),
            );
            Value::Object(context)
        }
        #[test]
        fn complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes(
        ) {
            // Seam: a resolver double's held copies, one under the project file's own identity, and
            // the opaque capture failure the rejection keeps by address beside the findings.
            for same_identity in [false, true] {
                let (rejected, port, allocation, pointer) = rejected(false, same_identity);
                assert_eq!(rejected.origins.codelists, [1, 2]);
                let function_source = if same_identity {
                    "held/project.yaml"
                } else {
                    "held/function.yaml"
                };
                // The function file holds its definition alone; the project
                // lists it at `functions[0]` (FUNC-2, D8).
                let mut want = vec![expected(
                    "function_file_invalid",
                    "FUNC-2",
                    "tests[0].result",
                    json!({"function":"first","case":"ordinary","value":true,"reason":"invalid_case_result","source":function_source,"entry":"held/project.yaml","environment_path":"functions[0]"}),
                )];
                for obligation in ["boundary", "normal"] {
                    want.push(expected("function_file_invalid", "FUNC-2", "tests", json!({"function":"first","obligation":obligation,"reason":"missing_coverage_obligation","source":function_source,"entry":"held/project.yaml","environment_path":"functions[0]"})));
                }
                want.push(expected("codelist_shape_invalid", "CT-4", "codelists[0].items[0].value", json!({"codelist":"SEX","value":true,"source":"held/ct.yaml","entry":"held/project.yaml","environment_path":"codelists[1]"})));
                want.push(expected("codelist_shape_invalid", "CT-4", "codelists[2].codelists[0].items[0].value", json!({"codelist":"OTHER","value":true,"source":"held/project.yaml","entry":"held/project.yaml","environment_path":"codelists[2]"})));
                let result = diagnostic::admission(&rejected).unwrap();
                assert_eq!(crate::support::rows(&result), want);
                assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
                assert_eq!(port.trace.len(), 4);
                opaque(&rejected, &allocation, pointer);
            }
        }
        #[test]
        fn complete_external_scalar_reports_preserve_unbounded_integers_and_non_debug_originals() {
            // Seam: the opaque capture failure the rejection keeps by address beside the scalar
            // findings, which a case's files cannot raise.
            let (rejected, port, allocation, pointer) = rejected(true, false);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), []);
            let wide: Value =
                yamaa_adapters::json_wire::decode("123456789012345678901234567890").unwrap();
            // A function file's value fails as its file does (FUNC-2), and a
            // codelist file's integer outside the `int` range as any runtime
            // literal does (VALUE-5).
            let want = [
                expected(
                    "function_file_invalid",
                    "FUNC-2",
                    "tests[0].result",
                    json!({"reason":"integer_out_of_range","value":wide,"source":"held/function.yaml","entry":"held/project.yaml","environment_path":"functions[0]"}),
                ),
                expected(
                    "integer_overflow",
                    "VALUE-5",
                    "codelists[0].items[0].value",
                    with(
                        &overflow("123456789012345678901234567890"),
                        json!({"source":"held/ct.yaml","entry":"held/project.yaml","environment_path":"codelists[1]"}),
                    ),
                ),
            ];
            let result = diagnostic::scalar_sources(&rejected).unwrap();
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::scalar_sources(&rejected).unwrap(), result);
            assert_eq!(port.trace.len(), 4);
            opaque(&rejected, &allocation, pointer);
        }
        #[test]
        fn complete_static_report_matches_all_independent_single_finding_projections() {
            // Seam: each finding of a held rejection projected alone, against the whole.
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
            // Seam: forged origins and swapped captures of a held rejection, which no preparation
            // produces.
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
            rejected.captures[0].origin = Origin::FunctionFile(0);
            let complete = diagnostic::admission(&rejected).unwrap();
            // Equal text admitted against another type's closure is not the
            // schema a listed file of its type is held with: each listed file
            // is admitted against the closure of its own type.
            let closures = Closures::shipped().unwrap();
            let codelists = |identity: &str| {
                document::decode_referenced(
                    source(
                        identity,
                        "yamaa: '1.0'\ntype: codelists\ncodelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n",
                    ),
                    FileType::Codelists,
                    LANGUAGE_VERSION,
                    &closures,
                )
                .unwrap()
                .admit()
                .unwrap()
                .into_captured()
            };
            let function = std::mem::replace(
                &mut rejected.captures[0].document,
                codelists("held/function.yaml"),
            );
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.captures[0].document = function;
            assert_eq!(rejected.captures[1].origin, Origin::Codelist(1));
            let codelist = std::mem::replace(&mut rejected.captures[1].document, {
                let function = document::decode_referenced(
                    source("held/ct.yaml", &function_file("first", "true")),
                    FileType::Function,
                    LANGUAGE_VERSION,
                    &closures,
                )
                .unwrap();
                function.admit().unwrap().into_captured()
            });
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.captures[1].document = codelist;
            assert_eq!(diagnostic::admission(&rejected).unwrap(), complete);
            assert_eq!(complete.len(), 5);
            assert_eq!(port.trace.len(), 4);
            opaque(&rejected, &allocation, pointer);
        }
        #[test]
        fn invalid_origins_refuse_without_replacing_original_capture_errors() {
            // Seam: forged origins of a held rejection, which no preparation produces.
            let (mut rejected, port, allocation, pointer) = rejected(false, false);
            rejected.origins.functions.clear();
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            rejected.origins.functions.push("first".into());
            assert_eq!(diagnostic::admission(&rejected).unwrap().len(), 5);
            // Function findings name the function file the project lists.
            rejected.origins.function_files.clear();
            assert_eq!(
                diagnostic::admission(&rejected),
                Err(ProjectionError::InvalidOrigin)
            );
            assert_eq!(port.trace.len(), 4);
            opaque(&rejected, &allocation, pointer);
        }

        #[test]
        fn retained_structural_limit_keeps_exact_facts_and_refuses_earlier_missing_rows() {
            // Seam: injected limits, which no case can set.
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
            impl Resolver for RefusingPort {
                type Error = ResourceError;
                fn inspect(&mut self, _: Name<'_>) -> Result<Located, Self::Error> {
                    self.reads += 1;
                    Err(ResourceError::Missing)
                }
                fn capture(&mut self, _: Name<'_>) -> Result<Captured, Self::Error> {
                    unreachable!("a file no inspection located is never captured")
                }
                fn failure(&self, _: Name<'_>, _: &Self::Error) -> Option<resolver::Failure> {
                    None
                }
                fn rebase(
                    &mut self,
                    _: &Located,
                    _: &Located,
                    _: &str,
                ) -> Result<String, Self::Error> {
                    unreachable!("a project file is never rebased")
                }
            }
            let mut port = RefusingPort { reads: 0 };
            let root = source(
                "held/project.yaml",
                "yamaa: '1.0'\ntype: project\ncodelists: [missing.yaml, ct.yaml]\n",
            );
            let Failure::Rejected(mut rejected) = project_source::prepare(
                Closures::shipped().unwrap(),
                root,
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
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
        fn actual_captured_two_hundred_function_files_keep_the_full_six_hundred_record_report() {
            // Seam: two hundred generated function files and the six hundred findings of one
            // projection.
            let allocation = Rc::new(Cell::new(19));
            let mut port = Port::new(Opaque {
                allocation,
                text: "never consumed".into(),
            });
            let files = (0..200)
                .map(|index| format!("functions/function_{index}.yaml"))
                .collect::<Vec<_>>();
            let body = format!(
                "yamaa: '1.0'\ntype: project\nfunctions: [{}]\nenvironments: {{python: {{lock: uv.lock}}}}\n",
                files.join(", ")
            );
            let rejected = match prepare(&mut port, &body) {
                Err(Failure::Rejected(rejected)) => rejected,
                _ => panic!("actual captured static rejection"),
            };
            let Some(findings) = &rejected.admission else {
                panic!("complete independent findings")
            };
            assert_eq!(findings.len(), 600);
            let result = diagnostic::admission(&rejected).unwrap();
            let mut want = vec![];
            for (index, file) in files.iter().enumerate() {
                let name = format!("function_{index}");
                let source = format!("held/{file}");
                let declaration = format!("functions[{index}]");
                want.push(expected("function_file_invalid", "FUNC-2", "tests[0].result", json!({"function":name,"case":"ordinary","value":true,"reason":"invalid_case_result","source":source,"entry":"held/project.yaml","environment_path":declaration})));
                for obligation in ["boundary", "normal"] {
                    want.push(expected("function_file_invalid", "FUNC-2", "tests", json!({"function":name,"obligation":obligation,"reason":"missing_coverage_obligation","source":source,"entry":"held/project.yaml","environment_path":declaration})));
                }
            }
            assert_eq!(crate::support::rows(&result), want);
            assert_eq!(diagnostic::admission(&rejected).unwrap(), result);
            let mut trace = vec![(Kind::Lock, "uv.lock".to_string())];
            trace.extend(files.iter().map(|file| (Kind::Function, file.clone())));
            assert_eq!(port.trace, trace);
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
            // Seam: forged invocation failures, among them argument failures a valid specification
            // never makes the engine raise.
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
            // Seam: the original payload of an opaque invocation failure, by address, and forged
            // host details.
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
            // Seam: forged identity keys and arguments, which no run produces.
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
            // Seam: the original payload the invocation holds, which no
            // replay compares, and the one call it made. The issue each
            // failure reports, classified, is project_functions.json's
            // a_classified_live_call_failure_fails_the_call_after_the_input_is_read
            // and a_live_call_whose_result_its_host_rejects_fails_its_result,
            // beside the results the engine refuses (FUNC-9).
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
            for result in [
                HostError::Raised(payload.clone()),
                HostError::InvalidResult(payload.clone()),
            ] {
                let mut port = Port {
                    calls: 0,
                    result: Some(Err(result)),
                };
                let failure = invoke_project(&plan, &std::collections::BTreeMap::new(), &mut port)
                    .unwrap_err();
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
                assert_eq!(port.calls, 1);
            }
        }
    }
    mod project_lock {
        use yamaa_adapters::project_lock::{decode_renv, Error, RenvLock};
        #[test]
        fn uv_classification_allows_unresolved_versions_but_validates_written_identities() {
            // Seam: the lock codec, on lock bytes alone.
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
            // Seam: the renv lock codec, on lock bytes alone.
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
            // Seam: the renv lock codec, on lock bytes alone.
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
            // Seam: the renv lock codec, on lock bytes alone.
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
            // Seam: the renv lock codec, on lock bytes alone.
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
            // Seam: the renv lock codec, on lock bytes alone.
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
        use yamaa_adapters::{
            document::Closures,
            project_lock::{Finding, Reason},
            project_lock_diagnostics::{issues, Error},
            project_source::{self, CapturedEnvironment, Kind},
            specification_source::Source,
        };
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{self, Captured, File, Located, Name, Resolver};

        struct Capture {
            locks: usize,
            /// The bytes of the file last located.
            held: Vec<u8>,
        }
        impl Capture {
            /// The held source the double serves for the file a project names.
            fn answer(
                &mut self,
                kind: Kind,
                declaring: &str,
                written: &str,
            ) -> Result<Source, std::convert::Infallible> {
                match kind {
                    Kind::Lock => {
                        // The project file writes the lock path; it resolves
                        // there (D8).
                        assert_eq!(declaring, "study/project.yaml");
                        assert_eq!(written, "../uv.lock");
                        self.locks += 1;
                        Ok(Source {
                            identity: "held/uv.lock".into(),
                            bytes: crate::support::ports::UV_LOCK.as_bytes().to_vec(),
                        })
                    }
                    kind => panic!("unexpected {kind:?} capture"),
                }
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for Capture {
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
        fn prepare(project: &str) -> (CapturedEnvironment, Capture) {
            let mut capture = Capture {
                locks: 0,
                held: vec![],
            };
            let source = Source {
                identity: "study/project.yaml".into(),
                bytes: project.as_bytes().to_vec(),
            };
            let prepared = project_source::prepare(
                Closures::shipped().unwrap(),
                source,
                Language::Python,
                &mut capture,
            )
            .unwrap();
            let (_, provenance) = prepared.into_owned().into_parts();
            (provenance, capture)
        }
        /// The project holds the Python lock, or only the R one, which the
        /// Python host does not read (PROJECT-2).
        fn environment(locked: bool) -> (CapturedEnvironment, Capture) {
            prepare(if locked {
                "yamaa: '1.0'\ntype: project\nenvironments: {python: {lock: ../uv.lock}}\n"
            } else {
                "yamaa: '1.0'\ntype: project\nenvironments: {r: {lock: r/renv.lock}}\n"
            })
        }
        /// The project declares no environment.
        fn without_environment() -> (CapturedEnvironment, Capture) {
            prepare("yamaa: '1.0'\ntype: project\n")
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
        fn classified_host_facts_project_from_the_held_lock_without_reading_it() {
            // Seam: the projection's reads and the held bytes it leaves in
            // place, which a build's issues do not show. Each finding's row is
            // project_functions.json's
            // lock_findings_report_against_the_project_file_that_holds_the_lock,
            // and the host's version spelling its
            // a_lock_finding_keeps_the_host_s_version_spelling_without_comparing_versions.
            let (environment, capture) = environment(true);
            let root = environment.root().source().bytes.as_ptr();
            let lock = environment.lock().unwrap().source.bytes.as_ptr();
            let facts = [finding(Reason::VersionMismatch, &["1.0"], Some("2.0"))];
            let records = issues(&environment, Some(Language::Python), &facts).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(
                issues(&environment, Some(Language::Python), &facts).unwrap(),
                records
            );
            assert_eq!(capture.locks, 1);
            assert_eq!(environment.root().source().bytes.as_ptr(), root);
            assert_eq!(environment.lock().unwrap().source.bytes.as_ptr(), lock);
        }

        #[test]
        fn absent_lock_and_contradictory_facts_refuse_the_complete_projection() {
            // Seam: lock facts no host classification gives: findings for a project without the
            // host's lock, and contradictory findings.
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
            // Seam: equal findings, a 1,025-fold version text and 2,050 findings, each kept whole
            // by the projection.
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
    }
}
