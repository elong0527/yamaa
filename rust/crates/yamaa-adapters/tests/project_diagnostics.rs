#![cfg(feature = "yaml")]
//! Project contracts: producer links, project activation diagnostics,
//! project codelist diagnostics and project environment diagnostics.
mod project {
    mod producer_links {
        use crate::support::Edit as _;
        use std::sync::Arc;
        use yamaa_adapters::{
            shipped_schema,
            specification_source::{
                link_metadata, InheritancePort, LinkedSpecification, PreparedDocument, Source,
            },
        };
        use yamaa_core::inheritance::{Source as FileIdentity, SourceError};
        use yamaa_core::{
            producer_contract::Links, specification::PrepareError, value::ColumnType,
        };
        const PRODUCER:&str="schema_version: '1.0'\ndomain: PROD\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: RAW.VALUE}\noutput: {path: './produced.csv', columns: [ID, VALUE]}\n";
        const CONSUMER:&str="schema_version: '1.0'\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './produced.csv', schema: nested/producer.yaml}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
        fn standalone(identity: &str, text: &str) -> Arc<PreparedDocument> {
            Arc::new(
                shipped_schema::capture()
                    .unwrap()
                    .prepare_standalone(Source {
                        identity: identity.into(),
                        bytes: text.as_bytes().to_vec(),
                    })
                    .unwrap(),
            )
        }
        fn linked(document: Arc<PreparedDocument>) -> LinkedSpecification {
            LinkedSpecification {
                dataset: "SRC".into(),
                identity: document.source().identity.clone(),
                document: Some(document),
            }
        }
        fn check(
            consumer: &PreparedDocument,
            links: &[LinkedSpecification],
        ) -> Result<yamaa_core::domain::CheckedSpecification, PrepareError> {
            let metadata = link_metadata(consumer, links);
            yamaa_core::domain::check_linked(
                consumer.model(),
                None,
                Links {
                    consumer: &consumer.source().identity,
                    links: &metadata,
                },
            )
        }
        #[test]
        fn linked_documents_supply_only_their_output_contract_without_study_authority() {
            let consumer = standalone("/consumer.yaml", CONSUMER);
            let producer = standalone("/producer.yaml", PRODUCER);
            let links = [linked(Arc::clone(&producer))];
            let checked = check(&consumer, &links).unwrap();
            let compiled = checked.compiled();
            assert_eq!(compiled.sources()[0].path, "./produced.csv");
            assert_eq!(
                compiled.sources()[0].types,
                [
                    ("ID".to_owned(), ColumnType::Int),
                    ("VALUE".to_owned(), ColumnType::Float)
                ]
            );
            assert_eq!(
                compiled
                    .source_contract(0)
                    .unwrap()
                    .fields()
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "VALUE"]
            );
            drop(links);
            drop(producer);
            assert_eq!(compiled.source_contract(0).unwrap().fields().len(), 2);
            // Without the linked document the declaration has no type authority.
            let Err(PrepareError::Invalid(findings)) = check(&consumer, &[]) else {
                panic!("missing link metadata")
            };
            let finding = findings[0].diagnostic();
            assert_eq!(finding.definition().condition, "invalid_producer_contract");
            assert_eq!(finding.spec_paths, ["input.SRC.schema"]);
            assert_eq!(
                finding.context["reason"],
                yamaa_core::json::Value::String("missing_metadata".into())
            );
        }
        struct Parent {
            text: Vec<u8>,
            calls: Vec<String>,
        }
        impl InheritancePort for Parent {
            type Error = ();
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<FileIdentity, SourceError<()>> {
                self.calls
                    .push(format!("canonicalize:{declaring}:{written}"));
                assert_eq!(written, "parent.yaml");
                Ok(FileIdentity {
                    identity: if declaring == "/producer.yaml" {
                        "/producer-parent.yaml"
                    } else {
                        "/parent.yaml"
                    }
                    .into(),
                    display_path: "parent.yaml".into(),
                })
            }
            fn capture(&mut self, source: &FileIdentity) -> Result<Vec<u8>, SourceError<()>> {
                self.calls.push(format!("capture:{}", source.identity));
                Ok(self.text.clone())
            }
            fn rebase(
                &mut self,
                layer: &FileIdentity,
                _: &FileIdentity,
                written: &str,
            ) -> Result<String, ()> {
                self.calls
                    .push(format!("rebase:{}:{written}", layer.identity));
                Ok(if layer.identity.ends_with("parent.yaml") {
                    format!("internal/parent/{written}")
                } else {
                    written.into()
                })
            }
        }
        fn inherited(identity: &str, entry: &str, parent: &str) -> (Arc<PreparedDocument>, Parent) {
            let mut port = Parent {
                text: parent.as_bytes().to_vec(),
                calls: vec![],
            };
            let document = shipped_schema::prepare(
                Source {
                    identity: identity.into(),
                    bytes: entry.as_bytes().to_vec(),
                },
                identity.into(),
                &mut port,
            )
            .unwrap();
            (Arc::new(document), port)
        }
        #[test]
        fn inherited_link_and_artifact_paths_keep_independent_authored_layers() {
            let entry="schema_version: '1.0'\nparent: parent.yaml\ndomain: CONS\nkeys: [ID]\ninput: {SRC: './child.csv'}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
            let parent =
        "schema_version: '1.0'\ninput: {SRC: {path: parent.csv, schema: '../producer.yaml'}}\n";
            let (consumer, consumer_port) = inherited("/consumer.yaml", entry, parent);
            let producer_entry = PRODUCER
                .edit(
                    "schema_version: '1.0'",
                    "schema_version: '1.0'\nparent: parent.yaml",
                )
                .edit("output: {path: './produced.csv', columns: [ID, VALUE]}", "");
            let producer_parent =
                "schema_version: '1.0'\noutput: {path: '../stored.csv', columns: [ID, VALUE]}\n";
            let (producer, producer_port) =
                inherited("/producer.yaml", &producer_entry, producer_parent);
            let before = (consumer_port.calls.clone(), producer_port.calls.clone());
            let schema = consumer.written_input_origin("SRC", "schema").unwrap();
            assert_eq!(schema.written, "../producer.yaml");
            assert_eq!(schema.declaring_source, "/parent.yaml");
            let path = consumer.written_input_origin("SRC", "path").unwrap();
            assert_eq!(path.written, "./child.csv");
            assert_eq!(path.declaring_source, "/consumer.yaml");
            let checked = check(&consumer, &[linked(Arc::clone(&producer))]).unwrap();
            // The consumer reads its own path; the producer's inherited output
            // path is never consulted.
            assert_eq!(checked.compiled().sources()[0].path, "./child.csv");
            assert_eq!(checked.compiled().sources()[0].types.len(), 2);
            assert_eq!(consumer.parents()[0].source().bytes, parent.as_bytes());
            assert_eq!(
                producer.parents()[0].source().bytes,
                producer_parent.as_bytes()
            );
            assert_eq!(before, (consumer_port.calls, producer_port.calls));
        }
        #[test]
        fn explicit_null_schema_is_an_absent_link() {
            // Null clears (LAYER-9): an explicit-null schema is absent, so the
            // input is an ordinary schema-less source.
            let null = CONSUMER.edit("schema: nested/producer.yaml", "schema: null");
            assert!(shipped_schema::capture()
                .unwrap()
                .prepare_standalone(Source {
                    identity: "/consumer.yaml".into(),
                    bytes: null.into_bytes(),
                })
                .is_ok());
        }
    }

    mod project_activation_diagnostics {
        use crate::support::project_call_compiler as support;
        use crate::support::roots::{Case, Definition};
        use std::rc::Rc;
        use yamaa_adapters::project_activation_diagnostics::{
            binding_issues, test_issues, DefinitionSource, Error, HostDetails,
        };
        use yamaa_core::{
            function_invocation::{Argument, FailureKind, HostError, InvocationFailure},
            project_activation::{self, ActivationPort, BindingFailure, Failure, TestFailure},
        };
        use yamaa_core::{
            function_signature::{LogicalSignature, ProjectFunctionIdentity},
            project_environment::{LockKind, LockReference},
            project_function::{Function, Language},
            value::{ColumnType, FiniteFloat, Value},
        };

        struct Port {
            payload: Rc<str>,
            bindings_fail: bool,
            calls_fail: bool,
            calls: usize,
            bindings: usize,
        }
        impl ActivationPort for Port {
            type Handle = ();
            type Error = Rc<str>;
            fn verify_lock(
                &mut self,
                _: Language,
                _: &LockReference,
                _: &[ProjectFunctionIdentity],
            ) -> Result<(), Rc<str>> {
                Ok(())
            }
            fn bind(
                &mut self,
                _: &ProjectFunctionIdentity,
                _: &LogicalSignature,
            ) -> Result<(), Rc<str>> {
                self.bindings += 1;
                if self.bindings_fail {
                    Err(self.payload.clone())
                } else {
                    Ok(())
                }
            }
            fn invoke(
                &mut self,
                _: &(),
                args: &[Argument<'_>],
            ) -> Result<Value, HostError<Rc<str>>> {
                self.calls += 1;
                if self.calls_fail {
                    return Err(HostError::Raised(self.payload.clone()));
                }
                let [Argument {
                    value: yamaa_core::table::ValueRef::Int(value),
                    ..
                }] = args
                else {
                    panic!("exact argument")
                };
                Ok(Value::Int(if *value == 7 { 8 } else { *value }))
            }
        }
        fn port() -> Port {
            Port {
                payload: Rc::from("é\0🙂 original"),
                bindings_fail: false,
                calls_fail: false,
                calls: 0,
                bindings: 0,
            }
        }
        fn lock() -> LockReference {
            LockReference {
                written: "uv.lock".into(),
                kind: LockKind::Uv,
            }
        }
        fn sources(functions: &[Function; 2]) -> [DefinitionSource<'_>; 2] {
            [
                DefinitionSource {
                    function: &functions[0],
                    path: "functions.first",
                },
                DefinitionSource {
                    function: &functions[1],
                    path: "functions.second",
                },
            ]
        }
        #[test]
        fn real_activation_mismatches_keep_every_case_in_selected_order_without_retry() {
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("actual case mismatch")
            };
            assert_eq!((port.bindings, port.calls), (2, 4));
            assert_eq!(failures.len(), 2);
            let issues = test_issues(&sources(&functions), &failures, &[None, None]).unwrap();
            assert_eq!(issues.len(), 2);
            for (index, issue) in issues.iter().enumerate() {
                assert_eq!(
                    (issue.phase(), issue.condition(), issue.rule()),
                    ("validation", "function_conformance_failed", Some("FUNC-12"))
                );
                assert_eq!(
                    issue.spec_paths,
                    [format!("{}.tests[0]", sources(&functions)[index].path)]
                );
                let context = yamaa_core::json::Value::Object(issue.context.clone());
                assert_eq!(context["function"], functions[index].name());
                assert_eq!(context["case"], "normal");
                assert_eq!(context["args"]["x"], 7);
                assert_eq!(context["expected"], 7);
                assert_eq!(context["actual"], 8);
                assert!(context.get("contract_version").is_none());
                assert!(context.get("implementation_version").is_none());
            }
            assert_eq!(
                test_issues(&sources(&functions), &failures, &[None, None]).unwrap(),
                issues
            );
            assert_eq!((port.bindings, port.calls), (2, 4));
        }
        #[test]
        fn actual_binding_and_invocation_errors_remain_original_non_send_payloads() {
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            port.bindings_fail = true;
            let Failure::Bindings(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("binding failures")
            };
            assert_eq!((port.bindings, port.calls), (2, 0));
            let Err(Error::Opaque(held)) =
                binding_issues(&sources(&functions), &failures, &[None, None])
            else {
                panic!("opaque binding flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
            let host = Some(HostDetails::Exception {
                class: "ValueError",
                message: "é\0🙂",
            });
            let issues = binding_issues(&sources(&functions), &failures, &[host, host]).unwrap();
            assert_eq!(issues.len(), 2);
            assert_eq!(issues[0].spec_paths, ["functions.first.function"]);
            assert_eq!(issues[0].rule(), Some("FUNC-2"));
            port.bindings_fail = false;
            port.calls_fail = true;
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, &lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("call failures")
            };
            assert_eq!(failures.len(), 4);
            let details = [None, None, None, None];
            let Err(Error::Opaque(held)) = test_issues(&sources(&functions), &failures, &details)
            else {
                panic!("opaque test flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
            let issues =
                test_issues(&sources(&functions), &failures, &[host, host, host, host]).unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["failure"]["condition"], "function_call_failed");
            assert_eq!(context["failure"]["rule"], "FUNC-8");
            assert_eq!(context["failure"]["context"]["host_message"], "é\0🙂");
            assert_eq!(port.calls, 4);
        }
        #[test]
        fn contradictory_indices_identity_result_and_duplicate_observations_return_no_issues() {
            let functions = [support::function("first"), support::function("second")];
            let selected = sources(&functions);
            for (function, case, actual, expected) in
                [(2, 0, 8, 7), (0, 9, 8, 7), (0, 0, 8, 9), (0, 0, 7, 7)]
            {
                let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                    function,
                    case,
                    actual: Value::Int(actual),
                    expected: Value::Int(expected),
                }];
                assert!(matches!(
                    test_issues(&selected, &failures, &[None]),
                    Err(Error::InvalidContext)
                ));
            }
            let failure: TestFailure<()> = TestFailure::Invocation {
                function: 0,
                case: 0,
                error: InvocationFailure {
                    identity: ProjectFunctionIdentity {
                        name: "second".into(),
                        call: "project.second".into(),
                    },
                    kind: FailureKind::BooleanResult,
                },
            };
            assert!(matches!(
                test_issues(&selected, &[failure], &[None]),
                Err(Error::InvalidContext)
            ));
            let failures = [
                BindingFailure {
                    function: 0,
                    error: (),
                },
                BindingFailure {
                    function: 0,
                    error: (),
                },
            ];
            let host = Some(HostDetails::Rejected {
                reason: "binding rejected",
                returned: None,
            });
            assert!(matches!(
                binding_issues(&selected, &failures, &[host, host]),
                Err(Error::InvalidContext)
            ));
        }
        #[test]
        fn complete_projection_keeps_long_escaped_paths_and_host_details() {
            let functions = [support::function("first"), support::function("second")];
            let selected = sources(&functions);
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: Value::Int(8),
                expected: Value::Int(7),
            }];
            assert_eq!(test_issues(&selected, &failures, &[None]).unwrap().len(), 1);
            let escaped = "\0".repeat(1_400_000);
            let huge = [DefinitionSource {
                function: &functions[0],
                path: &escaped,
            }];
            let issues = test_issues(&huge, &failures, &[None]).unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].spec_paths, [format!("{escaped}.tests[0]")]);
            let duplicate = vec![
                TestFailure::<()>::Result {
                    function: 0,
                    case: 0,
                    actual: Value::Int(8),
                    expected: Value::Int(7),
                },
                TestFailure::Result {
                    function: 0,
                    case: 0,
                    actual: Value::Int(8),
                    expected: Value::Int(7),
                },
            ];
            assert!(matches!(
                test_issues(&selected, &duplicate, &[None, None]),
                Err(Error::InvalidContext)
            ));
            let text = "é\0🙂".repeat(8193);
            let issues = binding_issues(
                &selected,
                &[BindingFailure {
                    function: 0,
                    error: (),
                }],
                &[Some(HostDetails::Exception {
                    class: "ValueError",
                    message: &text,
                })],
            )
            .unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["host_error"], "ValueError");
            assert_eq!(context["host_message"], text);
            assert!(context.get("host_details_truncated").is_none());
        }

        #[test]
        fn authored_float_bits_and_complete_scalar_values_remain_exact() {
            let float = |value| Value::Float(FiniteFloat::new(value).unwrap());
            let function = Function::admit(
                Language::Python,
                Definition {
                    name: "zero".into(),
                    function: "project.zero".into(),
                    description: "Zero".into(),
                    params: vec![],
                    returns: ColumnType::Float,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "zero".into(),
                        covers: vec![
                            "normal".into(),
                            "boundary".into(),
                            "numeric-comparison".into(),
                        ],
                        args: vec![],
                        result: float(0.0),
                    }],
                }
                .root(Language::Python),
            )
            .unwrap();
            let selected = [DefinitionSource {
                function: &function,
                path: "functions.zero",
            }];
            let wrong: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: float(1.0),
                expected: float(-0.0),
            }];
            assert!(matches!(
                test_issues(&selected, &wrong, &[None]),
                Err(Error::InvalidContext)
            ));
            let correct: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: float(1.0),
                expected: float(0.0),
            }];
            let issues = test_issues(&selected, &correct, &[None]).unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(
                context["expected"].as_f64().unwrap().to_bits(),
                0.0_f64.to_bits()
            );
            assert_eq!(function.definition().tests[0].result, float(0.0));

            let integer = support::function("integer");
            let selected = [DefinitionSource {
                function: &integer,
                path: "functions.integer",
            }];
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 1,
                actual: Value::Int(i64::MAX),
                expected: Value::Int(i64::MIN),
            }];
            let issues = test_issues(&selected, &failures, &[None]).unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["expected"].as_i64(), Some(i64::MIN));
            assert_eq!(context["args"]["x"].as_i64(), Some(i64::MIN));
            assert_eq!(context["actual"].as_i64(), Some(i64::MAX));

            let text = Function::admit(
                Language::Python,
                Definition {
                    name: "text".into(),
                    function: "project.text".into(),
                    description: "Text".into(),
                    params: vec![],
                    returns: ColumnType::Str,
                    may_return_missing: false,
                    comparison_decimals: 4,
                    tests: vec![Case {
                        id: "text".into(),
                        covers: vec!["normal".into(), "boundary".into()],
                        args: vec![],
                        result: Value::Str("authored é\0🙂".into()),
                    }],
                }
                .root(Language::Python),
            )
            .unwrap();
            let selected = [DefinitionSource {
                function: &text,
                path: "functions.text",
            }];
            let failures: Vec<TestFailure<()>> = vec![TestFailure::Result {
                function: 0,
                case: 0,
                actual: Value::Str("actual é\0🙂".into()),
                expected: Value::Str("authored é\0🙂".into()),
            }];
            let issues = test_issues(&selected, &failures, &[None]).unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["expected"], "authored é\0🙂");
            assert_eq!(context["actual"], "actual é\0🙂");
        }
    }

    mod project_codelist_diagnostics {
        use crate::support::project_codelist_compiler as support;
        use std::{cell::Cell, convert::Infallible};
        use support::{environment, schema, spec};
        use yamaa_adapters::project_codelist_diagnostics::{codelist_issues, Error};
        use yamaa_core::dataset::{CheckRecord, CodelistObservation, ExecutionError, RowIdentity};
        use yamaa_core::table::{CellError, TableAccess, TableSchema, ValueRef};
        use yamaa_core::{specification::PreparedSpecification, value::Value};
        struct Source {
            schema: TableSchema,
            reads: Cell<usize>,
        }
        impl TableAccess for Source {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                1
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                assert_eq!(row, 0);
                self.reads.set(self.reads.get() + 1);
                Ok(match column {
                    0 => ValueRef::Int(i64::MAX),
                    1 => ValueRef::Str("é\0🙂"),
                    _ => panic!("later source read"),
                })
            }
        }
        /// The 1.0 specification, prepared against its project as the 1.0
        /// front end lowers it.
        fn compiled() -> PreparedSpecification {
            let document = spec("SEX", "str", None, false, None);
            let tree = yamaa_core::front_end::v1::spec::lower(&document, None).into_spec();
            PreparedSpecification::prepare_tree_linked(
                tree,
                &document,
                Some(&environment(false)),
                Default::default(),
            )
            .unwrap()
        }
        fn record(value: Value) -> CheckRecord {
            CheckRecord {
                path: "columns.CODE.submission.codelist".into(),
                code: yamaa_core::diagnostic::ConditionCode::VerificationCodelistFailed,
                evaluated_count: 2,
                failed_count: 1,
                output_rows: 2,
                offending_rows: vec![RowIdentity {
                    position: 1,
                    values: vec![Value::Int(i64::MAX)],
                }],
                inventory: None,

                row_count: None,
                codelist: Some(CodelistObservation {
                    id: "SEX".into(),
                    values: vec![value],
                }),
                entries: None,
            }
        }
        #[test]
        fn complete_retained_values_and_full_width_keys_are_projected_without_output_or_sources() {
            let compiled = compiled();
            let source = Source {
                schema: schema(),
                reads: Cell::new(0),
            };
            let plan = compiled.bind(&source.schema).unwrap();
            let ExecutionError::VerificationFailures(records) = *plan
                .execute(
                    &source,
                    &[],
                    &mut yamaa_core::function_invocation::NoFunctions::new(),
                    None,
                    &|| false,
                )
                .result
                .unwrap_err()
            else {
                panic!("actual fixed-list failure")
            };
            let reads = source.reads.get();
            let before = records[0].codelist.as_ref().unwrap().values.as_ptr();
            let issues = codelist_issues(&compiled, &records).unwrap();
            assert_eq!(issues.len(), 1);
            let issue = &issues[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("verification", "allowed_values_failed", Some("CT-7"))
            );
            assert_eq!(issue.spec_paths, ["columns.CODE.submission.codelist"]);
            let context = yamaa_core::json::Value::Object(issue.context.clone());
            assert_eq!(context["column"], "CODE");
            assert_eq!(context["codelist"], "SEX");
            assert_eq!(context["keys"][0]["ID"].as_i64(), Some(i64::MAX));
            assert_eq!(context["values"], yamaa_adapters::json!(["é\0🙂"]));
            assert_eq!(codelist_issues(&compiled, &records).unwrap(), issues);
            assert_eq!(
                before,
                records[0].codelist.as_ref().unwrap().values.as_ptr()
            );
            assert_eq!(source.reads.get(), reads);
        }
        #[test]
        fn contradictory_observation_geometry_never_returns_partial_or_invented_issues() {
            let compiled = compiled();
            let original = record(Value::Str("bad".into()));
            for case in 0..6 {
                let mut wrong = original.clone();
                match case {
                    0 => wrong.codelist = None,
                    1 => wrong.failed_count = 2,
                    2 => wrong.codelist.as_mut().unwrap().id = "OTHER".into(),
                    3 => wrong.offending_rows[0].values.clear(),
                    4 => wrong.codelist.as_mut().unwrap().values[0] = Value::Missing,
                    _ => {
                        wrong.code =
                            yamaa_core::diagnostic::ConditionCode::VerificationAllowedValuesFailed
                    }
                }
                assert_eq!(
                    codelist_issues(&compiled, &[wrong]),
                    Err(Error::InvalidObservation)
                );
            }
            let mut passing = original;
            passing.failed_count = 0;
            passing.offending_rows.clear();
            passing.codelist.as_mut().unwrap().values.clear();
            assert!(codelist_issues(&compiled, &[passing]).unwrap().is_empty());
        }
        #[test]
        fn complete_escaped_values_and_large_retained_observations_are_projected() {
            let compiled = compiled();
            let text = "\0".repeat(1_500_000);
            let records = vec![record(Value::Str(text.clone()))];
            let issues = codelist_issues(&compiled, &records).unwrap();
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["values"], yamaa_adapters::json!([text]));
            let mut many = record(Value::Str("bad".into()));
            many.failed_count = 65_537;
            many.evaluated_count = 65_537;
            many.output_rows = 65_537;
            many.codelist.as_mut().unwrap().values = vec![Value::Int(1); 65_537];
            many.offending_rows = (0..65_537)
                .map(|position| RowIdentity {
                    position,
                    values: vec![Value::Int(i64::MAX)],
                })
                .collect();
            let issues = codelist_issues(&compiled, &[many]).unwrap();
            assert_eq!(issues.len(), 1);
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["failure_count"], 65_537);
            assert_eq!(context["values"].as_array().unwrap().len(), 65_537);
            assert_eq!(context["keys"].as_array().unwrap().len(), 65_537);
            assert_eq!(context["keys"][65_536]["ID"].as_i64(), Some(i64::MAX));
        }
    }

    mod project_environment_diagnostics {
        use std::rc::Rc;
        use yamaa_adapters::{
            project_environment_diagnostics::{admission, Error as ProjectionError},
            project_source::{
                self, CaptureFailure, Failure, Kind, Origin, ProjectCapturePort, Reply, Request,
            },
            specification_source::Source,
        };
        use yamaa_core::json::Value;
        use yamaa_core::{project_environment::LockKind, project_function::Language};
        /// The caller-supplied project file and the environment file it links.
        const PROJECT: &str = "study/project.yaml";
        const ENVIRONMENT: &str = "study/environment.yaml";
        /// A project that links only a Python environment file.
        const LINKED: &str = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\n";
        /// An environment file declaring a lock and one external function.
        const CODE: &str =
            "schema_version: '1.0'\nlock: uv.lock\nfunctions: {first: function.yaml}\n";
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        /// The frozen candidate project and environment roots.
        fn schemas() -> project_source::ProjectSchemas {
            crate::support::project_schemas::candidate_schemas()
        }
        struct Port {
            payload: Rc<()>,
            /// The bytes of the environment file the project links.
            environment: String,
            same_identity: bool,
            reads: usize,
            wide: bool,
            shape: bool,
        }
        fn port(environment: &str) -> Port {
            Port {
                payload: Rc::new(()),
                environment: environment.into(),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: false,
            }
        }
        impl ProjectCapturePort for Port {
            type Error = Rc<()>;
            fn capture(&mut self, request: Request<'_>) -> Result<Reply, Rc<()>> {
                self.reads += 1;
                // Project paths resolve from the project file; the lock and
                // function paths an environment file writes resolve from it.
                let declaring = match request.kind {
                    Kind::Environment | Kind::Codelist => PROJECT,
                    Kind::Lock | Kind::Function => ENVIRONMENT,
                };
                assert_eq!(request.declaring_source, declaring, "{:?}", request.kind);
                if request.written == "bad.yaml" {
                    return Err(Rc::clone(&self.payload));
                }
                Ok(match request.kind {
            Kind::Environment => Reply::Document(source(ENVIRONMENT, &self.environment)),
            Kind::Lock => Reply::Lock { source:source("held/uv.lock","version = 1\n"),kind:LockKind::Uv },
            Kind::Function if self.shape => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: []\ncontract_version: '1'\nbinding: {call: project.constant}\n")),
            Kind::Codelist if self.shape => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, items: [{value: F}]}]\nlegacy: true\n")),
            Kind::Function if self.wide => Reply::Document(source("held/function.yaml", "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: 123456789012345678901234567890}]\n")),
            Kind::Codelist if self.wide => Reply::Document(source("held/ct.yaml", "codelists: [{id: SEX, name: Sex, data_type: integer, items: [{value: 123456789012345678901234567890}]}]\n")),
            Kind::Function => Reply::Document(source(if self.same_identity { ENVIRONMENT } else { "held/function.yaml" }, "function: project.constant\ndescription: Constant\nparams: []\nreturns: int\ntests: [{id: ordinary, covers: [normal, boundary], args: {}, result: true}]\n")),
            Kind::Codelist => Reply::Document(source("held/ct.yaml","codelists: [{id: SEX, name: Sex, items: [{value: true}]}]\n")),
        })
            }
        }
        fn rejected(
            same_identity: bool,
        ) -> (Box<project_source::RejectedEnvironment<Rc<()>>>, Rc<()>) {
            let mut port = port(CODE);
            port.same_identity = same_identity;
            let payload = Rc::clone(&port.payload);
            let root=source(PROJECT,"schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [bad.yaml, ct.yaml, {codelists: [{id: OTHER, name: Other, items: [{value: true}]}]}]\n");
            let Failure::Rejected(rejected) =
                project_source::prepare(schemas(), root, Language::Python, &mut port).unwrap_err()
            else {
                panic!("retained rejection")
            };
            // The environment file, its lock and function, then both codelist files.
            assert_eq!(port.reads, 5);
            (rejected, payload)
        }
        fn context(issue: &yamaa_core::diagnostic::Diagnostic) -> Value {
            Value::Object(issue.context.clone())
        }
        #[test]
        fn static_admission_restores_external_and_inline_origins_after_an_earlier_capture_failure()
        {
            let (rejected, payload) = rejected(false);
            assert_eq!(rejected.origins.codelists, [1, 2]);
            let issues = admission(&rejected).unwrap();
            for issue in &issues {
                assert_eq!(context(issue)["entry"], PROJECT);
            }
            let function = issues
                .iter()
                .find(|i| context(i)["reason"] == "invalid_case_result")
                .unwrap();
            assert_eq!(function.spec_paths, ["tests[0].result"]);
            assert_eq!(context(function)["source"], "held/function.yaml");
            assert_eq!(context(function)["environment_path"], "functions.first");
            assert_eq!(context(function)["value"], true);
            let lists = issues
                .iter()
                .filter(|i| i.rule() == Some("CT-4"))
                .collect::<Vec<_>>();
            assert_eq!(lists.len(), 2);
            assert_eq!(lists[0].spec_paths, ["codelists[0].items[0].value"]);
            assert_eq!(context(lists[0])["source"], "held/ct.yaml");
            assert_eq!(context(lists[0])["environment_path"], "codelists[1]");
            assert_eq!(
                lists[1].spec_paths,
                ["codelists[2].codelists[0].items[0].value"]
            );
            // An inline codelist is declared by the project itself.
            assert_eq!(context(lists[1])["source"], PROJECT);
            assert_eq!(context(lists[1])["environment_path"], "codelists[2]");
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!("held host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
            let again = admission(&rejected).unwrap();
            assert_eq!(again, issues);
        }
        #[test]
        fn captured_external_document_ownership_does_not_depend_on_identity_text_equality() {
            // The function file reports the identity of the environment file
            // that declares it; it is still an external capture.
            let (rejected, _) = rejected(true);
            let issues = admission(&rejected).unwrap();
            let function = issues
                .iter()
                .find(|i| context(i)["reason"] == "invalid_case_result")
                .unwrap();
            assert_eq!(function.spec_paths, ["tests[0].result"]);
            assert_eq!(context(function)["source"], ENVIRONMENT);
            assert_eq!(context(function)["entry"], PROJECT);
            assert_eq!(context(function)["environment_path"], "functions.first");
        }
        #[test]
        fn invalid_origins_refuse_while_retaining_original_rejection() {
            let (mut rejected, payload) = rejected(false);
            assert_eq!(admission(&rejected).unwrap().len(), 5);
            rejected.origins.functions.clear();
            assert_eq!(admission(&rejected), Err(ProjectionError::InvalidOrigin));
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!("held host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
        }

        /// The linked environment file fails as one captured source of scalar
        /// findings; its issues name it and the project that links it.
        fn environment_scalars(
            environment: &str,
        ) -> (Box<project_source::RejectedEnvironment<Rc<()>>>, usize) {
            let mut port = port(environment);
            let Failure::Rejected(rejected) = project_source::prepare(
                schemas(),
                source(PROJECT, LINKED),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("retained environment scalar findings")
            };
            let [failure] = &rejected.sources[..] else {
                panic!("one failed environment file")
            };
            assert_eq!(failure.origin, Origin::Environment(Language::Python));
            assert!(matches!(failure.error, CaptureFailure::Scalar { .. }));
            assert!(rejected.environment_file.is_none());
            (rejected, port.reads)
        }
        fn assert_environment_provenance(issue: &yamaa_core::diagnostic::Diagnostic) {
            assert_eq!(context(issue)["source"], ENVIRONMENT);
            assert_eq!(context(issue)["entry"], PROJECT);
            assert_eq!(context(issue)["environment_path"], "environments.python");
        }

        #[test]
        fn scalar_decode_diagnostics_keep_every_case_and_the_complete_function_name() {
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            let name = "f".repeat(100);
            let cases = (0..40)
        .map(|n| {
            format!("{{id: case-{n}, covers: [normal], args: {{}}, result: 9223372036854775808}}")
        })
        .collect::<Vec<_>>()
        .join(",");
            let body = format!("schema_version: '1.0'\nfunctions:\n  ? {name}\n  : {{function: project.constant, description: Constant, params: [], returns: int, tests: [{cases}]}}\n");
            let (rejected, reads) = environment_scalars(&body);
            let CaptureFailure::Scalar { findings, .. } = &rejected.sources[0].error else {
                unreachable!()
            };
            assert_eq!(findings.len(), 40);
            let issues = scalar_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 40);
            for (index, issue) in issues.iter().enumerate() {
                assert_eq!(
                    issue.spec_paths,
                    [format!("functions.{name}.tests[{index}].result")]
                );
                assert_eq!(
                    yamaa_adapters::json_wire::encode(&context(issue)["value"]),
                    "9223372036854775808"
                );
                assert_eq!(context(issue)["reason"], "integer_out_of_range");
                assert_environment_provenance(issue);
            }
            assert_eq!(reads, 1);
        }

        #[test]
        fn environment_scalar_issues_keep_arbitrary_integers_and_invalid_temporal_text_without_later_reads(
        ) {
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            let body = "schema_version: '1.0'\nlock: uv.lock\nfunctions:\n  wide: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}\n  bad_date: {function: project.constant, description: Constant, params: [], returns: date, tests: [{id: ordinary, covers: [normal], args: {}, result: {date: '2024-02-30'}}]}\n  bad_time: {function: project.constant, description: Constant, params: [], returns: datetime, tests: [{id: ordinary, covers: [normal], args: {}, result: {datetime: '2024-01-01T25:00:00'}}]}\n";
            let (rejected, reads) = environment_scalars(body);
            let issues = scalar_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 3);
            assert_eq!(issues[0].spec_paths, ["functions.wide.tests[0].result"]);
            assert_eq!(
                yamaa_adapters::json_wire::encode(&context(&issues[0])["value"]),
                "123456789012345678901234567890"
            );
            assert_eq!(context(&issues[1])["value"], "2024-02-30");
            assert_eq!(context(&issues[2])["value"], "2024-01-01T25:00:00");
            for issue in &issues {
                assert_eq!(
                    (issue.phase(), issue.condition(), issue.rule()),
                    ("validation", "project_environment_invalid", Some("FUNC-2"))
                );
                assert_environment_provenance(issue);
            }
            assert_eq!(scalar_sources(&rejected).unwrap(), issues);
            // A failed environment file declares nothing: its lock is never read.
            assert!(rejected.lock.is_none());
            assert_eq!(reads, 1);
        }

        #[test]
        fn root_scalar_issues_keep_arbitrary_integers_without_dependency_reads() {
            use yamaa_adapters::project_environment_diagnostics::scalar_root;
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [{codelists: [{id: WIDE, name: Wide, data_type: integer, items: [{value: 123456789012345678901234567890}]}]}]\n";
            let mut port = port(CODE);
            let Failure::RootScalar { root, findings } = project_source::prepare(
                schemas(),
                source(PROJECT, body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("complete root scalars")
            };
            let issues = scalar_root(&root, &findings).unwrap();
            assert_eq!(issues.len(), 1);
            let issue = &issues[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "project_environment_invalid", Some("FUNC-2"))
            );
            assert_eq!(
                issue.spec_paths,
                ["codelists[0].codelists[0].items[0].value"]
            );
            assert_eq!(context(issue)["reason"], "integer_out_of_range");
            assert_eq!(
                yamaa_adapters::json_wire::encode(&context(issue)["value"]),
                "123456789012345678901234567890"
            );
            assert_eq!(context(issue)["source"], PROJECT);
            assert_eq!(context(issue)["entry"], PROJECT);
            assert_eq!(context(issue)["environment_path"], "$");
            assert_eq!(scalar_root(&root, &findings).unwrap(), issues);
            // A failed project root links nothing: its environment is never read.
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn failed_external_scalar_issues_keep_each_document_origin_and_original_other_failures() {
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            let mut port = port(CODE);
            port.wide = true;
            let payload = Rc::clone(&port.payload);
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
            let Failure::Rejected(rejected) = project_source::prepare(
                schemas(),
                source(PROJECT, body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("independent source scalars")
            };
            let issues = scalar_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 2);
            assert_eq!(issues[0].spec_paths, ["tests[0].result"]);
            assert_eq!(context(&issues[0])["source"], "held/function.yaml");
            assert_eq!(context(&issues[0])["environment_path"], "functions.first");
            assert_eq!(issues[1].spec_paths, ["codelists[0].items[0].value"]);
            assert_eq!(context(&issues[1])["source"], "held/ct.yaml");
            assert_eq!(context(&issues[1])["environment_path"], "codelists[1]");
            for issue in &issues {
                assert_eq!(
                    yamaa_adapters::json_wire::encode(&context(issue)["value"]),
                    "123456789012345678901234567890"
                );
                assert_eq!(context(issue)["entry"], PROJECT);
            }
            let CaptureFailure::Port(original) = &rejected.sources[1].error else {
                panic!("held original failure")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(port.reads, 5);
            assert_eq!(scalar_sources(&rejected).unwrap(), issues);
            assert_eq!(port.reads, 5);
        }

        #[test]
        fn root_schema_issues_keep_actual_closed_fields_and_source_without_dependency_reads() {
            use yamaa_adapters::{
                project_environment_diagnostics::schema_root,
                specification_source::Error as SourceError,
            };
            let mut port = port(CODE);
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\nparent: base.yaml\nruntime: {}\nversion: '1'\n";
            let Failure::Root(SourceError::Findings(captured)) = project_source::prepare(
                schemas(),
                source(PROJECT, body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("retained root schema findings")
            };
            let issues = schema_root(&captured).unwrap();
            assert_eq!(issues.len(), 3);
            for (issue, field) in issues.iter().zip(["parent", "runtime", "version"]) {
                assert_eq!(issue.spec_paths, [field]);
                assert_eq!(issue.condition(), "unknown_field");
                assert_eq!(issue.rule(), Some("SCHEMA-13"));
                assert_eq!(context(issue)["field"], field);
                assert_eq!(context(issue)["class"], "project_class");
                assert_eq!(context(issue)["source"], PROJECT);
                assert_eq!(context(issue)["entry"], PROJECT);
                assert_eq!(context(issue)["environment_path"], "$");
            }
            assert_eq!(schema_root(&captured).unwrap(), issues);
            // A failed project root links nothing: its environment is never read.
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn environment_file_schema_issues_name_its_project_link_and_read_nothing_it_declares() {
            use yamaa_adapters::{
                project_environment_diagnostics::schema_sources,
                specification_source::Error as SourceError,
            };
            // The project key states the language; an environment file that
            // still declares one fails its closed root.
            let mut port = port(
                "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {first: function.yaml}\n",
            );
            let Failure::Rejected(rejected) = project_source::prepare(
                schemas(),
                source(PROJECT, LINKED),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("retained environment schema findings")
            };
            let [failure] = &rejected.sources[..] else {
                panic!("one failed environment file")
            };
            assert_eq!(failure.origin, Origin::Environment(Language::Python));
            assert_eq!(failure.written, "environment.yaml");
            assert!(matches!(
                failure.error,
                CaptureFailure::Structural(SourceError::Findings(_))
            ));
            let issues = schema_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 1);
            let issue = &issues[0];
            assert_eq!(issue.spec_paths, ["language"]);
            assert_eq!(issue.condition(), "unknown_field");
            assert_eq!(issue.rule(), Some("SCHEMA-13"));
            assert_eq!(context(issue)["field"], "language");
            assert_eq!(context(issue)["class"], "environment_class");
            assert_environment_provenance(issue);
            assert_eq!(schema_sources(&rejected).unwrap(), issues);
            // Its lock and function are never read.
            assert_eq!(port.reads, 1);
        }

        #[test]
        fn failed_external_schema_issues_use_held_arenas_and_keep_original_other_failures() {
            use yamaa_adapters::project_environment_diagnostics::schema_sources;
            let mut port = port(CODE);
            port.shape = true;
            let payload = Rc::clone(&port.payload);
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\ncodelists: [bad.yaml, ct.yaml]\n";
            let Failure::Rejected(rejected) = project_source::prepare(
                schemas(),
                source(PROJECT, body),
                Language::Python,
                &mut port,
            )
            .unwrap_err() else {
                panic!("independent captured schema failures")
            };
            let issues = schema_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 3);
            for (issue, field, class, source, declaration) in [
                (
                    &issues[0],
                    "contract_version",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions.first",
                ),
                (
                    &issues[1],
                    "binding",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions.first",
                ),
                (
                    &issues[2],
                    "legacy",
                    "codelist_source_class",
                    "held/ct.yaml",
                    "codelists[1]",
                ),
            ] {
                assert_eq!(issue.spec_paths, [format!("<normalization>.{field}")]);
                assert_eq!(issue.condition(), "unknown_field");
                assert_eq!(issue.rule(), Some("SCHEMA-13"));
                assert_eq!(context(issue)["field"], field);
                assert_eq!(context(issue)["class"], class);
                assert_eq!(context(issue)["source"], source);
                assert_eq!(context(issue)["entry"], PROJECT);
                assert_eq!(context(issue)["environment_path"], declaration);
            }
            let CaptureFailure::Port(original) = &rejected.sources[1].error else {
                panic!("held original host failure")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(schema_sources(&rejected).unwrap(), issues);
            assert_eq!(port.reads, 5);
        }

        #[test]
        fn inline_scalars_report_alone_without_study_or_language_findings_and_unlinked_hosts_read_nothing(
        ) {
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            // A section without study admits for runs, and the environments key
            // states the language: neither yields a finding of its own.
            let body = "schema_version: '1.0'\nenvironments: {python: environment.yaml}\nsdtm: {standard: {name: SDTMIG, version: '3.4', status: Final}, specs: [], define: define.xml}\n";
            let environment = "schema_version: '1.0'\nfunctions: {bad: {function: project.constant, description: Constant, params: [], returns: int, tests: [{id: ordinary, covers: [normal], args: {}, result: 123456789012345678901234567890}]}}\n";
            let mut python = port(environment);
            let Failure::Rejected(rejected) = project_source::prepare(
                schemas(),
                source(PROJECT, body),
                Language::Python,
                &mut python,
            )
            .unwrap_err() else {
                panic!("held environment scalars")
            };
            assert!(rejected.admission.is_none());
            assert!(admission(&rejected).unwrap().is_empty());
            let issues = scalar_sources(&rejected).unwrap();
            assert_eq!(
                issues
                    .iter()
                    .map(|i| i.spec_paths[0].as_str())
                    .collect::<Vec<_>>(),
                ["functions.bad.tests[0].result"]
            );
            assert_environment_provenance(&issues[0]);
            assert_eq!(scalar_sources(&rejected).unwrap(), issues);
            assert_eq!(python.reads, 1);
            // Each host reads only the entry its own key names.
            let mut r = port(environment);
            let prepared =
                project_source::prepare(schemas(), source(PROJECT, body), Language::R, &mut r)
                    .unwrap();
            assert!(prepared.environment_file.is_none());
            assert!(prepared.captures.is_empty() && prepared.lock.is_none());
            assert_eq!(r.reads, 0);
        }

        #[test]
        fn large_capture_counts_preserve_complete_reports_and_original_origins() {
            use yamaa_adapters::project_environment_diagnostics::{scalar_sources, schema_sources};
            for written in ["bad.yaml", "ct.yaml"] {
                let mut port = port("schema_version: '1.0'\nlock: uv.lock\n");
                let payload = Rc::clone(&port.payload);
                let declarations = vec![written; 129].join(", ");
                let body = format!(
            "schema_version: '1.0'\nenvironments: {{python: environment.yaml}}\ncodelists: [{declarations}]\n"
        );
                let Failure::Rejected(rejected) = project_source::prepare(
                    schemas(),
                    source(PROJECT, &body),
                    Language::Python,
                    &mut port,
                )
                .unwrap_err() else {
                    panic!("retained valid capture records")
                };
                // The environment file, its lock and every codelist file.
                assert_eq!(port.reads, 131);
                if written == "bad.yaml" {
                    assert_eq!(rejected.sources.len(), 129);
                    assert!(scalar_sources(&rejected).unwrap().is_empty());
                    assert!(schema_sources(&rejected).unwrap().is_empty());
                    let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                        panic!("original payload")
                    };
                    assert!(Rc::ptr_eq(original, &payload));
                } else {
                    assert_eq!(rejected.captures.len(), 129);
                    let issues = admission(&rejected).unwrap();
                    let mut wanted = vec![];
                    for index in 0..129 {
                        if index > 0 {
                            wanted.push((
                                index,
                                "duplicate_define_identifier",
                                Some("id"),
                                "codelists[0].id",
                            ));
                            wanted.push((
                                index,
                                "duplicate_define_identifier",
                                Some("name"),
                                "codelists[0].name",
                            ));
                        }
                        wanted.push((
                            index,
                            "codelist_shape_invalid",
                            None,
                            "codelists[0].items[0].value",
                        ));
                    }
                    assert_eq!(issues.len(), 129 + 2 * 128);
                    for (issue, (index, condition, field, path)) in issues.iter().zip(wanted) {
                        let context = context(issue);
                        assert_eq!(issue.condition(), condition);
                        assert_eq!(issue.spec_paths, [path]);
                        assert_eq!(context["field"].as_str(), field);
                        assert_eq!(context["environment_path"], format!("codelists[{index}]"));
                        assert_eq!(context["source"], "held/ct.yaml");
                        assert_eq!(context["entry"], PROJECT);
                        if field.is_none() {
                            assert_eq!(context["value"], true);
                        }
                    }
                }
                assert_eq!(port.reads, 131);
            }
        }
    }
}
