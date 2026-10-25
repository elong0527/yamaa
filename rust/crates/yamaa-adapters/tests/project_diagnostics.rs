#![cfg(feature = "yaml")]
//! Project contracts: producer links, project activation diagnostics,
//! project codelist diagnostics and project environment diagnostics.
mod project {
    mod producer_links {
        use crate::support::Edit as _;
        use std::sync::Arc;
        use yamaa_adapters::{
            shipped_schema,
            specification_source::{link_metadata, LinkedSpecification, PreparedDocument, Source},
        };
        use yamaa_core::inheritance::Source as FileIdentity;
        use yamaa_core::resolver::{Captured, Failure, Name, Resolver};
        use yamaa_core::{
            producer_contract::Links, specification::PrepareError, value::ColumnType,
        };
        const PRODUCER:&str="yamaa: '1.0'\ntype: spec\ndomain: PROD\nkeys: [ID]\ninput: {RAW: never-read.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: float, label: Reported value, derivation: RAW.VALUE}\noutput: {path: './produced.csv', columns: [ID, VALUE]}\n";
        const CONSUMER:&str="yamaa: '1.0'\ntype: spec\ndomain: CONS\nkeys: [ID]\ninput: {SRC: {path: './produced.csv', schema: nested/producer.yaml}}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
        fn standalone(identity: &str, text: &str) -> Arc<PreparedDocument> {
            Arc::new(crate::support::specification_run::prepare_as(
                identity,
                text.as_bytes(),
            ))
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
            yamaa_core::domain::check_tree_linked(
                consumer.tree().clone(),
                consumer.document(),
                None,
                Links {
                    consumer: &consumer.source().identity,
                    links: &metadata,
                },
            )
        }
        #[test]
        fn linked_documents_supply_only_their_output_contract_without_study_authority() {
            // Seam: a consumer checked against prepared producer documents in memory, and the
            // compiled sources it holds.
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
        impl Resolver for Parent {
            type Error = ();
            fn inspect(&mut self, name: Name<'_>) -> Result<FileIdentity, ()> {
                let Name::Written {
                    declaring, written, ..
                } = name
                else {
                    panic!("unexpected request {name:?}")
                };
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
            fn capture(&mut self, name: Name<'_>) -> Result<Captured, ()> {
                let Name::Located { source, .. } = name else {
                    panic!("unexpected request {name:?}")
                };
                self.calls.push(format!("capture:{}", source.identity));
                Ok(Captured::resource(self.text.clone().into(), true))
            }
            fn failure(&self, _: Name<'_>, _: &()) -> Option<Failure> {
                None
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
            // Seam: inherited producer and consumer documents in memory, the resolver calls each
            // makes and the paths each layer keeps.
            let entry="yamaa: '1.0'\ntype: spec\nparent: parent.yaml\ndomain: CONS\nkeys: [ID]\ninput: {SRC: './child.csv'}\ncolumns: [{name: ID, type: int, label: ID, derivation: SRC.ID}]\noutput: {path: consumer.csv, columns: [ID]}\n";
            let parent =
        "yamaa: '1.0'\ntype: spec\ninput: {SRC: {path: parent.csv, schema: '../producer.yaml'}}\n";
            let (consumer, consumer_port) = inherited("/consumer.yaml", entry, parent);
            let producer_entry = PRODUCER
                .edit("type: spec", "type: spec\nparent: parent.yaml")
                .edit("output: {path: './produced.csv', columns: [ID, VALUE]}", "");
            let producer_parent =
                "yamaa: '1.0'\ntype: spec\noutput: {path: '../stored.csv', columns: [ID, VALUE]}\n";
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
            host_functions::{HostFunctions, Lock},
            project_activation::{self, BindingFailure, Failure, TestFailure},
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
        impl HostFunctions for Port {
            type Handle = ();
            type Error = Rc<str>;
            fn verify_lock(
                &mut self,
                _: Language,
                _: Lock<'_>,
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
        /// The declared uv lock each activation verifies, with no captured bytes.
        fn lock() -> Lock<'static> {
            static REFERENCE: std::sync::LazyLock<LockReference> =
                std::sync::LazyLock::new(|| LockReference {
                    written: "uv.lock".into(),
                    kind: LockKind::Uv,
                });
            Lock {
                reference: &REFERENCE,
                bytes: b"",
            }
        }
        fn sources(functions: &[Function; 2]) -> [DefinitionSource<'_>; 2] {
            [
                DefinitionSource {
                    function: &functions[0],
                    path: "functions[0]",
                },
                DefinitionSource {
                    function: &functions[1],
                    path: "functions[1]",
                },
            ]
        }
        #[test]
        fn held_case_failures_project_alike_twice_without_another_call() {
            // Seam: one set of held case failures projected twice, which no
            // build does. The issues and calls of the build are
            // project_activation.json's
            // every_case_of_each_called_function_runs_and_each_mismatch_reports_in_listing_order.
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("actual case mismatch")
            };
            let issues = test_issues(&sources(&functions), &failures, &[None, None]).unwrap();
            assert_eq!(
                test_issues(&sources(&functions), &failures, &[None, None]).unwrap(),
                issues
            );
            assert_eq!((port.bindings, port.calls), (2, 4));
        }
        #[test]
        fn actual_binding_and_invocation_errors_remain_original_non_send_payloads() {
            // Seam: the original payload each unclassified failure holds,
            // which no replay compares. The issues of the classified failures
            // are project_activation.json's
            // each_called_function_s_binding_failure_reports_at_its_python_callable
            // and each_case_a_host_exception_fails_reports_the_call_failure_it_held.
            let functions = [support::function("first"), support::function("second")];
            let mut port = port();
            port.bindings_fail = true;
            let Failure::Bindings(failures) =
                project_activation::activate(Language::Python, lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("binding failures")
            };
            let Err(Error::Opaque(held)) =
                binding_issues(&sources(&functions), &failures, &[None, None])
            else {
                panic!("opaque binding flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
            port.bindings_fail = false;
            port.calls_fail = true;
            let Failure::Tests(failures) =
                project_activation::activate(Language::Python, lock(), &functions, &mut port)
                    .unwrap_err()
            else {
                panic!("call failures")
            };
            let details = [None, None, None, None];
            let Err(Error::Opaque(held)) = test_issues(&sources(&functions), &failures, &details)
            else {
                panic!("opaque test flattened")
            };
            assert!(Rc::ptr_eq(held, &port.payload));
        }
        #[test]
        fn contradictory_indices_identity_result_and_duplicate_observations_return_no_issues() {
            // Seam: forged case and binding failures, which no activation produces.
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
            // Seam: a 1.4 MB definition path, an 8,193-fold host message and a duplicate case
            // failure, forged.
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
        fn authored_float_bits_remain_exact() {
            // Seam: a forged case result whose expected value is the authored
            // zero's other sign, and the bits of the zero the issue holds,
            // which a case's JSON numbers do not tell apart. The integer and
            // text values a mismatch keeps are project_activation.json's
            // a_conformance_failure_keeps_the_extreme_integers_it_compared and
            // a_conformance_failure_keeps_the_escaped_text_it_compared.
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
                path: "functions[0]",
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
                denominator: None,
            }
        }
        #[test]
        fn complete_retained_values_and_full_width_keys_are_projected_without_output_or_sources() {
            // Seam: codelist observations over an in-memory table, and the cells the projection
            // reads.
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
            // Seam: forged codelist observations, which no run produces.
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
            // Seam: codelist observations of 65,537 rows over an in-memory table.
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
            document::Closures,
            project_environment_diagnostics::{admission, Error as ProjectionError},
            project_source::{self, CaptureFailure, Failure, Kind},
            specification_source::Source,
        };
        use yamaa_core::json::Value;
        use yamaa_core::project_function::Language;
        use yamaa_core::resolver::{self, Captured, File, Located, Name, Resolver};
        /// The caller-supplied project file.
        const PROJECT: &str = "study/project.yaml";
        /// The header and lock of a project that lists one function file.
        const LISTING: &str =
            "yamaa: '1.0'\ntype: project\nfunctions: [function.yaml]\nenvironments: {python: {lock: uv.lock}}\n";
        fn source(identity: &str, bytes: &str) -> Source {
            Source {
                identity: identity.into(),
                bytes: bytes.as_bytes().to_vec(),
            }
        }
        struct Port {
            payload: Rc<()>,
            same_identity: bool,
            reads: usize,
            wide: bool,
            shape: bool,
            /// The bytes of the file last located.
            held: Vec<u8>,
        }
        fn port() -> Port {
            Port {
                payload: Rc::new(()),
                same_identity: false,
                reads: 0,
                wide: false,
                shape: false,
                held: vec![],
            }
        }
        impl Port {
            /// The held source the double serves for the file a project names.
            fn answer(
                &mut self,
                kind: Kind,
                declaring: &str,
                written: &str,
            ) -> Result<Source, Rc<()>> {
                self.reads += 1;
                // Every path a project writes resolves from the project file (D8).
                assert_eq!(declaring, PROJECT, "{kind:?}");
                if written == "bad.yaml" {
                    return Err(Rc::clone(&self.payload));
                }
                const FUNCTION: &str = "yamaa: '1.0'\ntype: function\nname: first\ndescription: Constant\nparams: []\nreturns: int\nimplementations: {python: project.constant}\n";
                const CODELISTS: &str = "yamaa: '1.0'\ntype: codelists\n";
                Ok(match kind {
            Kind::Lock => source("held/uv.lock", crate::support::ports::UV_LOCK),
            Kind::Function if self.shape => source("held/function.yaml", &format!("{FUNCTION}tests: []\ncontract_version: '1'\nbinding: {{call: project.constant}}\n")),
            Kind::Codelist if self.shape => source("held/ct.yaml", &format!("{CODELISTS}codelists: [{{id: SEX, name: Sex, items: [{{value: F}}]}}]\nlegacy: true\n")),
            Kind::Function if self.wide => source("held/function.yaml", &format!("{FUNCTION}tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: 123456789012345678901234567890}}]\n")),
            Kind::Codelist if self.wide => source("held/ct.yaml", &format!("{CODELISTS}codelists: [{{id: SEX, name: Sex, data_type: integer, items: [{{value: 123456789012345678901234567890}}]}}]\n")),
            Kind::Function => source(if self.same_identity { PROJECT } else { "held/function.yaml" }, &format!("{FUNCTION}tests: [{{id: ordinary, covers: [normal, boundary], args: {{}}, result: true}}]\n")),
            Kind::Codelist => source("held/ct.yaml", &format!("{CODELISTS}codelists: [{{id: SEX, name: Sex, items: [{{value: true}}]}}]\n")),
        })
            }
        }
        /// A file is served when it is located; its capture reads the bytes served.
        impl Resolver for Port {
            type Error = Rc<()>;
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
        ) -> Result<project_source::PreparedEnvironment, Failure<Rc<()>>> {
            project_source::prepare(
                Closures::shipped().unwrap(),
                source(PROJECT, body),
                Language::Python,
                port,
            )
        }
        fn rejected(
            same_identity: bool,
        ) -> (Box<project_source::RejectedEnvironment<Rc<()>>>, Rc<()>) {
            let mut port = port();
            port.same_identity = same_identity;
            let payload = Rc::clone(&port.payload);
            let body = format!("{LISTING}codelists: [bad.yaml, ct.yaml, {{codelists: [{{id: OTHER, name: Other, items: [{{value: true}}]}}]}}]\n");
            let Failure::Rejected(rejected) = prepare(&mut port, &body).unwrap_err() else {
                panic!("retained rejection")
            };
            // The lock and the function file, then both codelist files.
            assert_eq!(port.reads, 4);
            (rejected, payload)
        }
        fn context(issue: &yamaa_core::diagnostic::Diagnostic) -> Value {
            Value::Object(issue.context.clone())
        }
        #[test]
        fn static_admission_restores_external_and_inline_origins_after_an_earlier_capture_failure()
        {
            // Seam: a resolver double's opaque failure, kept by address beside the findings of the
            // files after it.
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
            assert_eq!(context(function)["environment_path"], "functions[0]");
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
            // Seam: a function file served under the project file's own identity, which a study's
            // files cannot have.
            // The function file reports the identity of the project file that
            // lists it; it is still an external capture.
            let (rejected, _) = rejected(true);
            let issues = admission(&rejected).unwrap();
            let function = issues
                .iter()
                .find(|i| context(i)["reason"] == "invalid_case_result")
                .unwrap();
            assert_eq!(function.spec_paths, ["tests[0].result"]);
            assert_eq!(context(function)["source"], PROJECT);
            assert_eq!(context(function)["entry"], PROJECT);
            assert_eq!(context(function)["environment_path"], "functions[0]");
        }
        #[test]
        fn invalid_origins_refuse_while_retaining_original_rejection() {
            // Seam: forged origins of a held rejection, which no preparation produces.
            let (mut rejected, payload) = rejected(false);
            assert_eq!(admission(&rejected).unwrap().len(), 5);
            rejected.origins.functions.clear();
            assert_eq!(admission(&rejected), Err(ProjectionError::InvalidOrigin));
            let CaptureFailure::Port(original) = &rejected.sources[0].error else {
                panic!("held host payload")
            };
            assert!(Rc::ptr_eq(original, &payload));
        }

        #[test]
        fn root_scalar_issues_keep_arbitrary_integers_without_dependency_reads() {
            // Seam: a held root's scalar findings projected twice, with no
            // file the root names read. The issue rows are project_files.json's
            // inline_codelist_integers_outside_the_int_range_fail_in_the_project_before_any_file_is_read.
            use yamaa_adapters::project_environment_diagnostics::scalar_root;
            let body = format!("{LISTING}codelists: [{{codelists: [{{id: WIDE, name: Wide, data_type: integer, items: [{{value: 123456789012345678901234567890}}]}}]}}]\n");
            let mut port = port();
            let Failure::RootScalar { root, findings } = prepare(&mut port, &body).unwrap_err()
            else {
                panic!("complete root scalars")
            };
            let issues = scalar_root(&root, &findings).unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(scalar_root(&root, &findings).unwrap(), issues);
            // A failed project root names nothing that is read.
            assert_eq!(port.reads, 0);
        }

        fn number(text: &str) -> Value {
            Value::Number(text.parse().expect("canonical integer"))
        }

        #[test]
        fn failed_external_scalar_issues_keep_each_document_origin_and_original_other_failures() {
            // Seam: one listed file's opaque failure, kept by address beside the others' scalar
            // findings, and the reads of a repeated projection.
            use yamaa_adapters::project_environment_diagnostics::scalar_sources;
            let mut port = port();
            port.wide = true;
            let payload = Rc::clone(&port.payload);
            let body = format!("{LISTING}codelists: [bad.yaml, ct.yaml]\n");
            let Failure::Rejected(rejected) = prepare(&mut port, &body).unwrap_err() else {
                panic!("independent source scalars")
            };
            let issues = scalar_sources(&rejected).unwrap();
            // A function file's value fails as its file does (FUNC-2), and a
            // codelist file's integer outside the `int` range as any runtime
            // literal does (VALUE-5).
            let wide = "123456789012345678901234567890";
            assert_eq!(
                crate::support::rows(&issues),
                [
                    yamaa_adapters::json!({"phase": "validation", "condition": "function_file_invalid",
                        "rule": "FUNC-2", "spec_paths": ["tests[0].result"],
                        "context": {"reason": "integer_out_of_range", "value": number(wide),
                            "source": "held/function.yaml", "entry": PROJECT,
                            "environment_path": "functions[0]"}}),
                    yamaa_adapters::json!({"phase": "validation", "condition": "integer_overflow",
                        "rule": "VALUE-5", "spec_paths": ["codelists[0].items[0].value"],
                        "context": {"actual": number(wide),
                            "value": wide,
                            "minimum": number("-9223372036854775808"),
                            "maximum": number("9223372036854775807"),
                            "source": "held/ct.yaml", "entry": PROJECT,
                            "environment_path": "codelists[1]"}}),
                ]
            );
            let CaptureFailure::Port(original) = &rejected.sources[1].error else {
                panic!("held original failure")
            };
            assert!(Rc::ptr_eq(original, &payload));
            assert_eq!(port.reads, 4);
            assert_eq!(scalar_sources(&rejected).unwrap(), issues);
            assert_eq!(port.reads, 4);
        }

        #[test]
        fn root_schema_issues_keep_actual_closed_fields_and_source_without_dependency_reads() {
            // Seam: a held root's schema findings projected twice, with no file the root names
            // read.
            use yamaa_adapters::{
                project_environment_diagnostics::schema_root,
                specification_source::Error as SourceError,
            };
            let mut port = port();
            let body = format!("{LISTING}parent: base.yaml\nruntime: {{}}\nversion: '1'\n");
            let Failure::Root(SourceError::Findings(captured)) =
                prepare(&mut port, &body).unwrap_err()
            else {
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
            // A failed project root names nothing that is read.
            assert_eq!(port.reads, 0);
        }

        #[test]
        fn failed_external_schema_issues_use_held_arenas_and_keep_original_other_failures() {
            // Seam: one listed file's opaque failure, kept by address beside the others' schema
            // findings.
            use yamaa_adapters::project_environment_diagnostics::schema_sources;
            let mut port = port();
            port.shape = true;
            let payload = Rc::clone(&port.payload);
            let body = format!("{LISTING}codelists: [bad.yaml, ct.yaml]\n");
            let Failure::Rejected(rejected) = prepare(&mut port, &body).unwrap_err() else {
                panic!("independent captured schema failures")
            };
            let issues = schema_sources(&rejected).unwrap();
            assert_eq!(issues.len(), 3);
            // Each listed file is admitted against the closure of its type, and
            // its findings write the file's own paths (SCHEMA-13, P-CLR-2).
            for (issue, field, class, source, declaration) in [
                (
                    &issues[0],
                    "contract_version",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions[0]",
                ),
                (
                    &issues[1],
                    "binding",
                    "function_definition_class",
                    "held/function.yaml",
                    "functions[0]",
                ),
                (
                    &issues[2],
                    "legacy",
                    "codelist_file_class",
                    "held/ct.yaml",
                    "codelists[1]",
                ),
            ] {
                assert_eq!(issue.spec_paths, [field]);
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
            assert_eq!(port.reads, 4);
        }

        #[test]
        fn large_capture_counts_preserve_complete_reports_and_original_origins() {
            // Seam: 129 generated listings of one file, with its opaque failure or its findings
            // each time.
            use yamaa_adapters::project_environment_diagnostics::{scalar_sources, schema_sources};
            for written in ["bad.yaml", "ct.yaml"] {
                let mut port = port();
                let payload = Rc::clone(&port.payload);
                let declarations = vec![written; 129].join(", ");
                let body = format!(
                    "yamaa: '1.0'\ntype: project\nenvironments: {{python: {{lock: uv.lock}}}}\ncodelists: [{declarations}]\n"
                );
                let Failure::Rejected(rejected) = prepare(&mut port, &body).unwrap_err() else {
                    panic!("retained valid capture records")
                };
                // The lock and every codelist file.
                assert_eq!(port.reads, 130);
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
                assert_eq!(port.reads, 130);
            }
        }
    }
}
