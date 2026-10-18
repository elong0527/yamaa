//! Independently authored native diagnostic contexts.
#[allow(dead_code)]
mod support;
mod diagnostics {
    mod diagnostic {
        mod aggregate {
            use crate::support::diagnostic::aggregate::*;
            #[test]

            fn invalid_causes_refuse_and_large_group_values_stay_complete() {
                for types in [
                    &[ValueType::Int, ValueType::Float][..],
                    &[ValueType::Str, ValueType::Str][..],
                    &[][..],
                ] {
                    assert_eq!(
                        Finding::IncomparableSources {
                            path: "aggregate",
                            expression: "MAX(V)",
                            source: "V",
                            types
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                for count in [0, 1] {
                    assert_eq!(
                        Finding::MultipleRecords {
                            path: "aggregate",
                            expression: "ONLY(V)",
                            phase: AggregatePhase::Derivation,
                            count,
                            row: None,
                            group: &[]
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                let huge = "x".repeat(1_000_000);
                let group = vec![("D.ID".into(), Value::Str(huge.clone()))];
                let address = huge.as_ptr();
                let diagnostic = Finding::MultipleRecords {
                    path: "aggregate",
                    expression: "ONLY(D.V)",
                    phase: AggregatePhase::RowConstruction,
                    count: 2,
                    row: Some("total"),
                    group: &group,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.spec_paths, ["aggregate"]);
                assert_eq!(diagnostic.code, ConditionCode::AggregateRowMultipleRecords);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("expr".into(), text("ONLY(D.V)")),
                        ("reducer".into(), text("ONLY")),
                        ("record_count".into(), V::from(2)),
                        ("row".into(), text("total")),
                        (
                            "group".into(),
                            V::Object([("D.ID".into(), text(&huge))].into_iter().collect())
                        ),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                assert_eq!(huge.as_ptr(), address);
                assert_eq!(group[0].1, Value::Str(huge));
            }
            #[test]
            #[cfg(target_pointer_width = "64")]
            fn record_counts_keep_the_complete_unsigned_range() {
                let diagnostic = Finding::MultipleRecords {
                    path: "aggregate",
                    expression: "ONLY(V)",
                    phase: AggregatePhase::Derivation,
                    count: usize::MAX,
                    row: None,
                    group: &[],
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["record_count"], V::from(usize::MAX));
            }
        }
        mod binding {
            use crate::support::diagnostic::binding::*;
            #[test]

            fn malformed_dependency_metadata_has_no_projection_or_panic() {
                let analysis =
                    column_dependencies::analyze(&[Some(vec![1]), Some(vec![0])], &[], false)
                        .unwrap();
                for finding in analysis.diagnostics {
                    assert!(finding.specification_diagnostic(&[], &[]).is_none());
                }
            }
        }
        mod dataset {
            use crate::support::diagnostic::dataset::*;
            #[test]

            fn temporal_spelling_and_nonnumeric_reduction_types_remain_complete() {
                let date = Date::new(1, 1, 1, Precision::Year).unwrap();
                let datetime =
                    Value::DateTime(DateTime::new(date, 1, 2, 3, Precision::Day).unwrap());
                let diagnostic = F::BaselineAmbiguity {
                    path: "baseline",
                    column: "B",
                    date: &datetime,
                    count: 2,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["date"], text("0001-01-01T01:02:03"));
                for (actual, spelling) in [
                    (ValueType::Str, "str"),
                    (ValueType::Bool, "bool"),
                    (ValueType::Date, "date"),
                    (ValueType::DateTime, "datetime"),
                ] {
                    let diagnostic = F::ReductionInputType {
                        path: "aggregate",
                        expression: "SUM(A)",
                        reducer: NumericReducer::Sum,
                        source: "A",
                        actual,
                    }
                    .diagnostic()
                    .unwrap();
                    assert_eq!(diagnostic.context["actual"], text(spelling));
                }
            }
            #[test]

            fn large_context_and_invalid_causes_preserve_borrowed_facts() {
                let huge = "x".repeat(3_000_000);
                let address = huge.as_ptr();
                let cause = F::MultipleMatches {
                    path: "source",
                    dataset: "DM",
                    intermediate: &huge,
                    count: 2,
                };
                let diagnostic = cause.diagnostic().unwrap();
                assert_eq!(diagnostic.code, ConditionCode::DatasetMultipleMatches);
                assert_eq!(diagnostic.spec_paths, ["source"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("dataset".into(), text("DM")),
                        ("intermediate".into(), text(&huge)),
                        ("match_count".into(), V::from(2)),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
                assert_eq!(huge.as_ptr(), address);
                assert_eq!(huge.len(), 3_000_000);
                for count in [0, 1] {
                    assert_eq!(
                        F::MultipleValues {
                            path: "p",
                            identifier: "A",
                            count
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                    assert_eq!(
                        F::MultipleMatches {
                            path: "p",
                            dataset: "DM",
                            intermediate: "I",
                            count
                        }
                        .diagnostic(),
                        Err(ProjectionError::InvalidContext)
                    );
                }
                let wrong = Value::Str("2024-01-01".into());
                assert_eq!(
                    F::BaselineAmbiguity {
                        path: "p",
                        column: "A",
                        date: &wrong,
                        count: 2
                    }
                    .diagnostic(),
                    Err(ProjectionError::InvalidContext)
                );
                assert_eq!(
                    F::ReductionInputType {
                        path: "p",
                        expression: "SUM(A)",
                        reducer: NumericReducer::Sum,
                        source: "A",
                        actual: ValueType::Int
                    }
                    .diagnostic(),
                    Err(ProjectionError::InvalidContext)
                );
            }
            #[test]
            #[cfg(target_pointer_width = "64")]
            fn counts_above_i64_remain_mathematical_diagnostic_integers() {
                let diagnostic = F::MultipleValues {
                    path: "p",
                    identifier: "A",
                    count: usize::MAX,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["value_count"], V::from(usize::MAX));
                let diagnostic = F::MultipleMatches {
                    path: "p",
                    dataset: "DM",
                    intermediate: "I",
                    count: usize::MAX,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.context["match_count"], V::from(usize::MAX));
            }
        }
        mod grammar {
            use crate::support::diagnostic::grammar::*;
            #[test]

            fn arithmetic_is_a_comparison_operand_and_refusals_keep_two_fields() {
                use yamaa_core::json::Value as V;
                // Arithmetic in a comparison operand is admitted, with its names.
                for (text, names) in [
                    ("SBPSTD - SBPSUP <= -20", vec!["SBPSTD", "SBPSUP"]),
                    ("VS.SBPSTD - VS.SBPSUP < 0", vec!["VS.SBPSTD", "VS.SBPSUP"]),
                    ("ASTDT <= TRTEDT + 30", vec!["ASTDT", "TRTEDT"]),
                ] {
                    let compiled = compile(
                        text,
                        Field::Predicate,
                        "columns.OHYPO.derivation.case[0].when",
                    )
                    .unwrap();
                    assert_eq!(compiled.identifiers().collect::<Vec<_>>(), names, "{text}");
                }
                // A refused predicate reports its text and position, and nothing else.
                for (text, position) in [("AGE >> 65", 5), ("A >", 3)] {
                    let Err(CompileError::Invalid {
                        position: actual,
                        failure,
                        ..
                    }) = compile(text, Field::Predicate, "filter")
                    else {
                        panic!("predicate must fail: {text}")
                    };
                    assert_eq!(actual.character, position, "{text}");
                    let d = failure.diagnostic(Field::Predicate, "filter", text, position);
                    assert_eq!(d.definition().condition, "invalid_expression");
                    assert_eq!(d.definition().requirement, Some("REQ-0188"));
                    assert_eq!(
                        d.context,
                        [
                            ("predicate".into(), V::String(text.into())),
                            ("position".into(), V::from(position)),
                        ]
                        .into(),
                        "{text}"
                    );
                }
            }
            #[test]

            fn a_function_finding_carries_its_own_written_name() {
                for (field, written, function) in [
                    (Field::Compute, "  foo(1)", "foo"),
                    (Field::Aggregate, "  sumx(SRC.ID)", "sumx"),
                ] {
                    let Err(CompileError::Invalid { failure, .. }) =
                        compile(written, field, "columns.VALUE")
                    else {
                        panic!("prohibited function");
                    };
                    let d =
                        failure.diagnostic(field, "columns.VALUE", "\u{03b2}\u{03b2}\u{03b2}", 0);
                    assert_eq!(d.definition().condition, "prohibited_function");
                    assert_eq!(d.context["function"], text(function));
                }
            }
        }
        mod inheritance {
            use crate::support::diagnostic::inheritance::*;
            #[test]

            fn contradictory_cycle_carriers_refuse_without_relabeling_the_cause() {
                for path in [
                    vec![],
                    vec!["entry".into()],
                    vec!["entry".into(), "other".into()],
                ] {
                    assert_eq!(
                        F::Cycle {
                            path: &path,
                            returns_to_entry: true
                        }
                        .diagnostic(),
                        Err(yamaa_core::diagnostic::ProjectionError::InvalidContext)
                    );
                }
                let path = vec!["entry".into(), "parent".into(), "entry".into()];
                let diagnostic = F::Cycle {
                    path: &path,
                    returns_to_entry: true,
                }
                .diagnostic()
                .unwrap();
                assert_eq!(diagnostic.code, ConditionCode::InheritanceCycle);
                assert_eq!(diagnostic.spec_paths, ["parent"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        (
                            "reason".into(),
                            JsonValue::String("parent_chain_returns_to_entry".into())
                        ),
                        ("source".into(), JsonValue::String("parent".into())),
                        (
                            "cycle".into(),
                            JsonValue::Array(
                                ["entry", "parent", "entry"]
                                    .map(|name| JsonValue::String(name.into()))
                                    .into()
                            )
                        ),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
            }
        }
        mod mapping_cut {
            use crate::support::diagnostic::mapping_cut::*;
            #[test]

            fn failures_have_complete_independently_authored_geometry() {
                // A missing source no longer fails mapping or cut (COLUMN-10).
                assert_eq!(reached().len(), 5);
            }
        }
        mod odm {
            use crate::support::diagnostic::odm::*;
            #[test]

            fn complete_odm_projection_preserves_nested_values_and_escaped_metadata() {
                let large = "\"".repeat(400);
                let names = [large.clone()];
                let finding = F::AmbiguousField {
                    dataset: "D",
                    field: "ItemOID",
                    stored: &names,
                };
                let mut reached = BTreeSet::new();
                check(
                    finding,
                    "validation",
                    "odm_schema_field_ambiguous",
                    "REQ-1275",
                    "input.D.path",
                    vec![
                        ("dataset", text("D")),
                        ("field", text("ItemOID")),
                        ("stored", V::Array(vec![text(&large)])),
                    ],
                    &mut reached,
                );
                let value = "\\".repeat(400);
                let differ = [("FormOID".into(), vec![Value::Str(value.clone())])];
                let diagnostic = F::NotUnique {
                    path: "odm",
                    item: "D.Item",
                    row: None,
                    phase: AggregatePhase::Derivation,
                    scope: &[],
                    differ: &differ,
                    records: 2,
                }
                .diagnostic();
                assert_eq!(diagnostic.code, ConditionCode::OdmNotUnique);
                assert_eq!(diagnostic.spec_paths, ["odm"]);
                assert_eq!(
                    diagnostic.context,
                    [
                        ("item".into(), text("D.Item")),
                        ("row".into(), V::Null),
                        ("scope".into(), V::Object(Default::default())),
                        ("records".into(), V::from(2)),
                        (
                            "differ".into(),
                            V::Object(
                                [("FormOID".into(), V::Array(vec![text(&value)]))]
                                    .into_iter()
                                    .collect()
                            )
                        ),
                        ("repeated".into(), V::Bool(false)),
                    ]
                    .into()
                );
                assert_eq!(diagnostic.source_span, None);
                assert_eq!(diagnostic.operand_route, None);
            }
        }
        mod parquet {
            use crate::support::diagnostic::parquet::*;
            #[test]

            fn alignment_and_early_profile_failure_do_not_inspect_later_codec_metadata() {
                let mut inspected = 0;
                let fields = ["I", ""].into_iter().map(|name| {
                    inspected += 1;
                    field(name)
                });
                assert_eq!(p::columns(fields, 1).unwrap_err(), p::Error::Invalid);
                assert_eq!(inspected, 0);
                let mut inspected = 0;
                let fields = ["", "I"].into_iter().map(|name| {
                    inspected += 1;
                    field(name)
                });
                assert_eq!(
                    p::columns(fields, 2).unwrap_err(),
                    p::Error::EmptyName { field: 1 }
                );
                assert_eq!(inspected, 1);
            }
        }
        mod predicate_evaluation {
            use crate::support::diagnostic::predicate_evaluation::*;
            #[test]

            fn original_context_projection_admits_all_fields_and_retains_rich_parser_facts() {
                for (expression, requirement, character, detail) in [
                    ("STR_CONTAINS(ID, '(')", "REQ-1244", 17, "pattern_byte"),
                    ("ID = DATE '2026-02-30'", "REQ-0188", 10, "temporal_error"),
                ] {
                    let error = compile(
                        expression,
                        Context::Predicate,
                        "columns.VALUE.derivation.row_number.window.filter",
                    )
                    .unwrap_err();
                    let yamaa_core::expression::CompileError::Invalid {
                        failure,
                        position,
                        message,
                    } = &error
                    else {
                        panic!("independent rejected original form")
                    };
                    assert_eq!(position.character, character);
                    let detail_before = failure.syntax_diagnostic();
                    assert!(detail_before.context.contains_key(detail));
                    let message_address = message.as_ptr();
                    let finding = failure.diagnostic(
                        Context::Predicate,
                        "columns.VALUE.derivation.row_number.window.filter",
                        expression,
                        position.character,
                    );
                    assert_eq!(finding.definition().requirement, Some(requirement));
                    assert_eq!(
                        finding.context,
                        [
                            ("predicate".into(), text(expression)),
                            ("position".into(), V::from(character)),
                        ]
                        .into()
                    );
                    assert_eq!(finding.source_span, None);
                    assert_eq!(finding.operand_route, None);
                    assert_eq!(failure.syntax_diagnostic(), detail_before);
                    assert_eq!(message.as_ptr(), message_address);
                }
                // ESCAPE owns REQ-0191 under the one syntax condition.
                let yamaa_core::expression::CompileError::Invalid {
                    failure: failure @ yamaa_core::expression::Failure::InvalidEscape,
                    position,
                    ..
                } = compile(
                    "ID LIKE '%' ESCAPE 'xx'",
                    Context::Predicate,
                    "rows[0].filter",
                )
                .unwrap_err()
                else {
                    panic!("invalid ESCAPE width")
                };
                let finding = failure.diagnostic(
                    Context::Predicate,
                    "rows[0].filter",
                    "ID LIKE '%' ESCAPE 'xx'",
                    position.character,
                );
                assert_eq!(finding.definition().requirement, Some("REQ-0191"));
                assert_eq!(
                    finding.context["position"],
                    V::Number(("19").parse().expect("canonical integer"))
                );
                let huge = "\0".repeat(1_400_000);
                let projected = yamaa_core::expression::Failure::Syntax.diagnostic(
                    Context::Predicate,
                    "rows[0].filter",
                    &huge,
                    usize::MAX,
                );
                assert_eq!(projected.spec_paths, ["rows[0].filter"]);
                assert_eq!(
                    projected.context,
                    [
                        ("predicate".into(), text(&huge)),
                        ("position".into(), V::from(usize::MAX)),
                    ]
                    .into()
                );
            }
            #[test]

            fn every_native_predicate_cause_retains_independent_context_and_structural_route() {
                assert_eq!(reached().len(), 5)
            }
            #[test]

            fn upper_resolution_outranks_lower_type_error_and_projection_never_retries() {
                let original = Box::new(41);
                let pointer = &*original as *const u8;
                let mut port = Port {
                    payload: Some(Payload { original }),
                    trace: vec![],
                };
                let p = plan("v BETWEEN low AND high");
                let Err(error) = p.evaluate_truth(&mut port) else {
                    panic!("opaque upper error")
                };
                assert_eq!(port.trace, ["v", "low", "v", "high"]);
                assert_eq!(error.route, [Route::Upper]);
                for _ in 0..2 {
                    let Err(P::Resolution(payload)) = error.diagnostic() else {
                        panic!("borrowed original")
                    };
                    assert_eq!(&*payload.original as *const u8, pointer);
                }
                assert_eq!(port.trace, ["v", "low", "v", "high"]);
                let ErrorKind::Resolution {
                    identifier,
                    error: payload,
                } = error.kind
                else {
                    panic!("still original")
                };
                assert_eq!(identifier, "high");
                assert_eq!(&*payload.original as *const u8, pointer);
            }
            #[test]

            fn complete_long_context_and_full_width_routes_are_preserved() {
                let mut error: EvaluationError<Infallible> = EvaluationError {
                    spec_path: "rows[0].filter".into(),
                    expression: "unchanged".into(),
                    route: vec![
                        Route::Child,
                        Route::Left,
                        Route::Right,
                        Route::Value,
                        Route::Item(usize::MAX),
                        Route::Lower,
                        Route::RepeatedValue,
                        Route::Upper,
                        Route::Pattern,
                        Route::Unary,
                        Route::LeftOperand,
                        Route::RightOperand,
                    ],
                    source_span: None,
                    kind: ErrorKind::Condition(Condition::DanglingEscape),
                };
                let d = error.diagnostic().unwrap();
                assert_eq!(
                    d.operand_route,
                    Some(vec![
                        "Child".into(),
                        "Left".into(),
                        "Right".into(),
                        "Value".into(),
                        format!("Item({})", usize::MAX),
                        "Lower".into(),
                        "RepeatedValue".into(),
                        "Upper".into(),
                        "Pattern".into(),
                        "unary".into(),
                        "left".into(),
                        "right".into()
                    ])
                );
                error.route = vec![Route::Left; 65_536];
                let projected = error.diagnostic().unwrap();
                assert_eq!(projected.operand_route, Some(vec!["Left".into(); 65_536]));
                assert_eq!(
                    projected.context,
                    [("reason".into(), text("LIKE pattern has a dangling escape"))].into()
                );
                error.route = vec![];
                let identifier = "\0".repeat(1_400_000);
                error.kind = ErrorKind::Condition(Condition::UnknownField {
                    identifier: identifier.clone(),
                });
                let projected = error.diagnostic().unwrap();
                assert_eq!(
                    projected.context,
                    [("identifier".into(), text(&identifier))].into()
                );
                assert_eq!(projected.operand_route, Some(vec![]));
                assert_eq!(error.expression, "unchanged");
            }
        }
        mod project_activation {
            use crate::support::diagnostic::project_activation::*;
            #[test]

            fn opaque_payloads_remain_borrowed_with_original_precedence() {
                struct Payload(Rc<()>); // no Clone, Display, Send or Sync requirement
                let payload = Payload(Rc::new(()));
                let identity = ProjectFunctionIdentity {
                    name: String::new(),
                    call: String::new(),
                };
                let failure = InvocationFailure {
                    identity: identity.clone(),
                    kind: FailureKind::CallFailed(&payload),
                };
                let Err(Error::Opaque(held)) =
                    projection::invocation("", &failure, None, &[], None)
                else {
                    panic!("opaque failure precedence")
                };
                assert!(std::ptr::eq(*held, &payload));
                assert_eq!(Rc::strong_count(&held.0), 1);
                let function = support::function("id");
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions.id",
                }];
                let failure = [BindingFailure {
                    function: 0,
                    error: &payload,
                }];
                let Err(Error::Opaque(held)) =
                    projection::binding_diagnostics(&definitions, &failure, &[None])
                else {
                    panic!("opaque binding")
                };
                assert!(std::ptr::eq(*held, &payload));
            }
            #[test]

            fn complete_host_details_are_borrowed_in_runtime_binding_and_nested_case_diagnostics() {
                let function = support::function("id");
                let definition = function.definition();
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions.id",
                }];
                let class = "Exception".repeat(2048);
                let message = "exact\0\u{e9}\u{1f642}".repeat(2048);
                let reason = "invalid_native_result".repeat(1024);
                let returned = "returned\0\u{e9}".repeat(2048);
                for (host, fields) in [
                    (
                        HostDetails::Exception {
                            class: &class,
                            message: &message,
                        },
                        [
                            ("host_error", class.as_str()),
                            ("host_message", message.as_str()),
                        ],
                    ),
                    (
                        HostDetails::Rejected {
                            reason: &reason,
                            returned: Some(&returned),
                        },
                        [("reason", reason.as_str()), ("returned", returned.as_str())],
                    ),
                ] {
                    let failure = InvocationFailure {
                        identity: ProjectFunctionIdentity {
                            name: "id".into(),
                            call: "project.id".into(),
                        },
                        kind: FailureKind::CallFailed(()),
                    };
                    let failures = [TestFailure::Invocation {
                        function: 0,
                        case: 0,
                        error: failure,
                    }];
                    let TestFailure::Invocation { error: failure, .. } = &failures[0] else {
                        unreachable!()
                    };
                    let context = [
                        ("function", Fact::Text("id")),
                        ("call", Fact::Text("project.id")),
                        (fields[0].0, Fact::Text(fields[0].1)),
                        (fields[1].0, Fact::Text(fields[1].1)),
                    ]
                    .into();
                    let diagnostic = projection::invocation(
                        "columns.X.derivation",
                        failure,
                        None,
                        &[],
                        Some(host),
                    )
                    .unwrap();
                    assert_eq!(
                        diagnostic,
                        Diagnostic {
                            code: ConditionCode::RuntimeFunctionCallFailed,
                            spec_path: "columns.X.derivation".into(),
                            context,
                        }
                    );
                    for (key, text) in fields {
                        let Fact::Text(held) = diagnostic.context[key] else {
                            panic!("borrowed host detail")
                        };
                        assert!(std::ptr::eq(held.as_ptr(), text.as_ptr()));
                    }
                    let binding = [BindingFailure {
                        function: 0,
                        error: (),
                    }];
                    assert_eq!(
                        projection::binding_diagnostics(&definitions, &binding, &[Some(host)])
                            .unwrap(),
                        [Diagnostic {
                            code: ConditionCode::ProjectEnvironmentInvalid,
                            spec_path: "functions.id.function".into(),
                            context: diagnostic.context.clone(),
                        }]
                    );
                    assert_eq!(
                        projection::test_diagnostics(&definitions, &failures, &[Some(host)])
                            .unwrap(),
                        [Diagnostic {
                            code: ConditionCode::FunctionConformanceFailed,
                            spec_path: "functions.id.tests[0]".into(),
                            context: [
                                ("function", Fact::Text("id")),
                                ("call", Fact::Text("project.id")),
                                ("case", Fact::Text("normal")),
                                (
                                    "args",
                                    Fact::Object(
                                        [("x", Fact::Scalar(&definition.tests[0].args[0].1))]
                                            .into()
                                    )
                                ),
                                ("expected", Fact::Scalar(&definition.tests[0].result)),
                                ("comparison_decimals", Fact::Integer(4)),
                                (
                                    "reason",
                                    Fact::Text("the case failed before it produced a result")
                                ),
                                (
                                    "failure",
                                    Fact::Object(
                                        [
                                            ("phase", Fact::Text("derivation")),
                                            ("condition", Fact::Text("function_call_failed")),
                                            ("requirement", Fact::Text("REQ-0701")),
                                            ("context", Fact::Object(diagnostic.context)),
                                        ]
                                        .into()
                                    )
                                ),
                            ]
                            .into(),
                        }]
                    );
                }
            }
            #[test]

            fn late_contradictions_return_no_partial_report() {
                let function = support::function("id");
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions.id",
                }];
                let actual = Value::Int(8);
                let expected = &function.definition().tests[0].result;
                let failures: [TestFailure<()>; 2] = [
                    TestFailure::Result {
                        function: 0,
                        case: 0,
                        actual: actual.clone(),
                        expected: expected.clone(),
                    },
                    TestFailure::Result {
                        function: 0,
                        case: 99,
                        actual: actual.clone(),
                        expected: expected.clone(),
                    },
                ];
                assert!(matches!(
                    projection::test_diagnostics(&definitions, &failures, &[None, None],),
                    Err(Error::InvalidContext)
                ));
                let wrong = Value::Int(9);
                let failures: [TestFailure<()>; 1] = [TestFailure::Result {
                    function: 0,
                    case: 0,
                    actual: actual.clone(),
                    expected: wrong.clone(),
                }];
                assert!(matches!(
                    projection::test_diagnostics(&definitions, &failures, &[None],),
                    Err(Error::InvalidContext)
                ));
            }
            #[test]

            fn nested_invocation_and_row_keys_preserve_borrowed_scalar_geometry() {
                let function = support::function("id");
                let identity = ProjectFunctionIdentity {
                    name: "id".into(),
                    call: function.definition().function.clone(),
                };
                let definitions = [DefinitionSource {
                    function: &function,
                    path: "functions.id",
                }];
                let failures: [TestFailure<()>; 1] = [TestFailure::Invocation {
                    function: 0,
                    case: 0,
                    error: InvocationFailure {
                        identity: identity.clone(),
                        kind: FailureKind::MissingRequired {
                            parameter: "x".into(),
                        },
                    },
                }];
                let diagnostics =
                    projection::test_diagnostics(&definitions, &failures, &[None]).unwrap();
                let Fact::Object(held) = &diagnostics[0].context["failure"] else {
                    panic!("nested borrowed diagnostic")
                };
                assert_eq!(held["requirement"], Fact::Text("REQ-0700"));
                let Fact::Object(context) = &held["context"] else {
                    panic!("nested invocation")
                };
                assert_eq!(context["parameter"], Fact::Text("x"));
                let row = [
                    Value::Int(i64::MIN),
                    Value::Int(i64::MAX),
                    Value::Float(FiniteFloat::new(-0.0).unwrap()),
                    Value::Str("\u{e9}\0\u{1f642}".into()),
                ];
                let row = RowIdentity {
                    position: 0,
                    values: row.to_vec(),
                };
                let failure = InvocationFailure::<(), ProjectFunctionIdentity> {
                    identity,
                    kind: FailureKind::BooleanResult,
                };
                let diagnostic = projection::invocation(
                    "columns.X.derivation",
                    &failure,
                    Some(&row),
                    &["MIN", "MAX", "ZERO", "TEXT"],
                    None,
                )
                .unwrap();
                let Fact::Sequence(keys) = &diagnostic.context["keys"] else {
                    panic!("row keys")
                };
                let [Fact::Object(keys)] = keys.as_slice() else {
                    panic!("one exact key observation")
                };
                for (name, value) in ["MIN", "MAX", "ZERO", "TEXT"].into_iter().zip(&row.values) {
                    let Fact::Scalar(held) = keys[name] else {
                        panic!("scalar key")
                    };
                    assert!(std::ptr::eq(held, value));
                }
            }
        }
        mod project_lock {
            use crate::support::diagnostic::project_lock::*;
            #[test]

            fn every_classified_reason_preserves_independent_complete_context_and_order() {
                assert_eq!(reached(), [ConditionCode::ProjectLockMismatch].into());
            }
            #[test]

            fn spelling_missing_and_empty_facts_are_preserved_without_version_comparison() {
                let records = lock_diagnostics(
                    Some(LockOrigins {
                        source: "é\0🙂/environment.yaml",
                        lock_source: "captured-lock",
                        written: "../declared/uv.lock",
                    }),
                    &[
                        finding(R::InvalidInstalledVersion, &["1.0"], None),
                        finding(R::InvalidLockedVersion, &[], None),
                        finding(R::VersionMismatch, &[" 1.0 "], Some(" 1.0 ")),
                        finding(R::VersionMismatch, &[""], Some("")),
                    ],
                )
                .unwrap();
                assert_eq!(records[0].context["actual"], V::Null);
                assert_eq!(records[1].context["expected"], V::Array(vec![]));
                assert_eq!(
                    records[2].context["expected"],
                    V::Array(vec![text(" 1.0 ")])
                );
                assert_eq!(records[2].context["actual"], text(" 1.0 "));
                assert_eq!(records[3].context["actual"], text(""));
                assert_eq!(records[0].context["source"], text("é\0🙂/environment.yaml"));
                assert_eq!(records[0].context["entry"], records[0].context["source"]);
                assert_eq!(records[0].context["lock"], text("../declared/uv.lock"));
            }
            #[test]

            fn contradictory_shapes_refuse_the_complete_report() {
                assert_eq!(lock_diagnostics(None, &[]).unwrap(), vec![]);
                let first = || finding(R::VersionMismatch, &["1.0"], Some("2.0"));
                assert_eq!(lock_diagnostics(None, &[first()]), Err(E::InvalidContext));
                for bad in [
                    finding(R::DistributionNotIdentified, &["1"], None),
                    finding(R::VersionNotLocked, &[], Some("1")),
                    finding(R::AmbiguousLockVersion, &["1"], None),
                    finding(R::InvalidLockedVersion, &["1", "2"], None),
                    finding(R::PackageNotInstalled, &["1"], Some("2")),
                    finding(R::InvalidInstalledVersion, &[], Some("2")),
                    finding(R::VersionMismatch, &["1"], None),
                    LockFinding {
                        package: "bad\0package".into(),
                        ..first()
                    },
                    LockFinding {
                        package: String::new(),
                        ..first()
                    },
                ] {
                    assert_eq!(
                        lock_diagnostics(Some(origins()), &[first(), bad]),
                        Err(E::InvalidContext)
                    );
                }
            }
            #[test]

            fn long_lock_facts_and_all_ordered_versions_remain_complete() {
                let source = "\0".repeat(65_537);
                let package = "p".repeat(2049);
                let actual = "é".repeat(1025);
                let records = [LockFinding {
                    package: package.clone(),
                    reason: R::VersionMismatch,
                    expected: vec!["1".into()],
                    actual: Some(actual.clone()),
                }];
                let report = lock_diagnostics(
                    Some(LockOrigins {
                        source: &source,
                        ..origins()
                    }),
                    &records,
                )
                .unwrap();
                assert_eq!(
                    report,
                    [yamaa_core::diagnostic::Diagnostic {
                        code: ConditionCode::ProjectLockMismatch,
                        spec_paths: vec!["lock".into()],
                        context: [
                            ("source".into(), text(&source)),
                            ("entry".into(), text(&source)),
                            ("lock_source".into(), text("held/uv.lock")),
                            ("lock".into(), text("../uv.lock")),
                            ("package".into(), text(&package)),
                            ("reason".into(), text("version_mismatch")),
                            ("expected".into(), V::Array(vec![text("1")])),
                            ("actual".into(), text(&actual)),
                        ]
                        .into(),
                        source_span: None,
                        operand_route: None,
                    }]
                );
                let many = (0..2050)
                    .map(|_| finding(R::VersionNotLocked, &[], None))
                    .collect::<Vec<_>>();
                let projected = lock_diagnostics(Some(origins()), &many).unwrap();
                assert_eq!(projected.len(), 2050);
                assert!(projected
                    .iter()
                    .all(|diagnostic| diagnostic.context["reason"] == text("version_not_locked")));
                let versions = [LockFinding {
                    package: "program".into(),
                    reason: R::AmbiguousLockVersion,
                    expected: vec!["1".into(); 65_537],
                    actual: None,
                }];
                let projected = lock_diagnostics(Some(origins()), &versions).unwrap();
                assert_eq!(
                    projected[0].context["expected"],
                    V::Array(vec![text("1"); 65_537])
                );
                assert_eq!(projected[0].context["actual"], V::Null);
            }
        }
        mod submission {
            use crate::support::diagnostic::submission::*;
            #[test]

            fn large_binding_values_are_retained_and_a_restating_verification_is_redundant() {
                // Held semantic metadata has no cumulative copy allowance. File loaders
                // enforce their own per-file boundary before producing these arenas.
                let long = "X".repeat(8_388_609);
                let lists = catalogue(false, Value::Str(long.clone()));
                let mut spec = specification();
                spec.set(
                    &["columns", "0", "submission", "codelist"],
                    Tree::text("LIST"),
                );
                let study = one(environment(), spec.clone(), &lists).unwrap();
                assert_eq!(study.codelists().len(), 1);
                assert_eq!(
                    study.codelists()[0].declaration.items.as_ref().unwrap()[0].value,
                    Value::Str(long.clone())
                );
                // A list of items states the column's value set; an equal
                // `allowed_values` beside it states it twice.
                spec.set(
                    &["columns", "0", "verifications"],
                    Tree::list(vec![Tree::map(vec![(
                        "allowed_values",
                        Tree::map(vec![("values", Tree::list(vec![Tree::text(&long)]))]),
                    )])]),
                );
                let findings = one(environment(), spec, &lists).unwrap_err();
                assert_eq!(findings.len(), 1);
                assert_eq!(
                    findings[0].code,
                    ConditionCode::CodelistAllowedValuesRedundant
                );
                assert_eq!(findings[0].definition().requirement, Some("REQ-0955"));
                assert_eq!(
                    findings[0].path,
                    "sdtm.specs[0].columns.VALUE.verifications[0].allowed_values"
                );
                assert_eq!(
                    findings[0].context["stated_by"],
                    JsonValue::String("sdtm.specs[0].columns.VALUE.submission.codelist".into())
                );
            }
            #[test]

            fn dataset_json_refuses_oversized_geometry_before_reading_cells() {
                use yamaa_core::limits::Limits;
                struct Oversized(TableSchema);
                impl TableAccess for Oversized {
                    type Error = Infallible;
                    fn schema(&self) -> &TableSchema {
                        &self.0
                    }
                    fn row_count(&self) -> usize {
                        Limits::TABLE_ROWS + 1
                    }
                    fn cell(
                        &self,
                        _: usize,
                        _: usize,
                    ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                        panic!("geometry must be admitted before table access")
                    }
                }
                let lists = ct::Catalogue::admit(vec![]).unwrap();
                let study = one(environment(), specification(), &lists).unwrap();
                let table = Oversized(
                    TableSchema::new(vec![Column {
                        name: "VALUE".into(),
                        kind: ColumnType::Str,
                    }])
                    .unwrap(),
                );
                let finding =
                    submission_json::admit(&study, &study.datasets()[0], &table).unwrap_err();
                assert_eq!(finding.code, ConditionCode::ApplicationRejected);
                assert_eq!(finding.path, "datasets.D0.dataset_json");
                let detail = finding.context["detail"].as_str().unwrap();
                assert!(detail.contains("table_rows"));
                assert!(detail.contains("10000001"));
            }
        }
        mod verification {
            use crate::support::diagnostic::verification::*;
            #[test]

            fn warning_display_is_bounded_but_all_keys_and_exact_scalars_remain_in_order() {
                let compiled = compiled();
                let rows: Vec<_> = (0..7)
                    .map(|n| {
                        Row(vec![if n == 0 {
                            Value::Int(i64::MAX)
                        } else {
                            Value::Int(n)
                        }])
                    })
                    .collect();
                let mut record = record(
                    ConditionCode::VerificationUniqueFailed,
                    "verifications[0].unique",
                    &rows,
                );
                record.evaluated_count = usize::MAX;
                record.output_rows = usize::MAX;
                // Unique failure count is repeated groups, rather than all group members.
                record.failed_count = 2;
                let records = [record];
                let projected =
                    project(&compiled, &records, Phase::Verification, "study.yaml").unwrap();
                assert!(projected.diagnostics.is_empty());
                let observation = object(&projected.observations[0]);
                assert_eq!(observation["severity"], Fact::Text("warning"));
                assert_eq!(observation["verification_id"], Fact::Text("u"));
                assert_eq!(observation["evaluated_count"], Fact::Unsigned(usize::MAX));
                let failure = object(&observation["failure"]);
                let context = object(&failure["context"]);
                assert_eq!(
                    context["columns"],
                    Fact::Sequence(vec![Fact::Text("VAL"), Fact::Text("ID")])
                );
                assert_eq!(context["failure_count"], Fact::Unsigned(2));
                let Fact::Sequence(display) = &context["keys"] else {
                    panic!("display keys")
                };
                assert_eq!(display.len(), 5);
                let Fact::Sequence(retained) = &failure["offending_keys"] else {
                    panic!("complete keys")
                };
                assert_eq!(retained.len(), 7);
                let Fact::Scalar(value) = object(&retained[0])["ID"] else {
                    panic!("borrowed key")
                };
                assert!(std::ptr::eq(value, &records[0].offending_rows[0].values[0]));
                let Fact::Sequence(logged) = &object(&failure["log_context"])["keys"] else {
                    panic!("log keys")
                };
                assert_eq!(logged.len(), 7);
            }
            #[test]

            fn output_missing_keys_are_not_rejected_or_lost_and_row_count_keeps_full_width() {
                let compiled = compiled();
                let rows = [Row(vec![Value::Missing])];
                let records = [record(
                    ConditionCode::OutputMissingKeyFailed,
                    "keys[0]",
                    &rows,
                )];
                let result = project(&compiled, &records, Phase::Output, "s.yaml").unwrap();
                let context = &result.diagnostics[0].context;
                assert_eq!(context["column"], Fact::Text("ID"));
                let Fact::Sequence(keys) = &context["keys"] else {
                    panic!("missing keys")
                };
                assert_eq!(object(&keys[0])["ID"], Fact::Scalar(&Value::Missing));
                let mut count = record(
                    ConditionCode::VerificationRowCountFailed,
                    "verifications[3].row_count",
                    &[],
                );
                count.output_rows = usize::MAX;
                count.evaluated_count = 1;
                let counts = [count];
                let result = project(&compiled, &counts, Phase::Verification, "s.yaml").unwrap();
                assert_eq!(
                    result.diagnostics[0].context["count"],
                    Fact::Unsigned(usize::MAX)
                );
                assert_eq!(
                    result.diagnostics[0].context["keys"],
                    Fact::Sequence(vec![Fact::Object(Default::default())])
                );
            }
            #[test]

            fn later_invalid_geometry_refuses_the_entire_projection() {
                let compiled = compiled();
                let good = [Row(vec![Value::Int(1)])];
                let bad = [Row(vec![Value::Int(1), Value::Int(2)])];
                let records = [
                    record(
                        ConditionCode::VerificationAssertFailed,
                        "verifications[2].assert",
                        &good,
                    ),
                    record(
                        ConditionCode::VerificationAllOrNoneFailed,
                        "verifications[1].all_or_none",
                        &bad,
                    ),
                ];
                assert!(matches!(
                    project(&compiled, &records, Phase::Verification, "s.yaml"),
                    Err(Error::InvalidObservation)
                ));
            }
            #[test]

            fn typed_wrong_family_or_phase_refuses_the_entire_projection() {
                let compiled = compiled();
                let rows = [Row(vec![Value::Int(1)])];
                for (code, path) in [
                    (ConditionCode::ConversionInput, "verifications[2].assert"),
                    (ConditionCode::OutputMissingKeyFailed, "keys[0]"),
                ] {
                    let records = [
                        record(
                            ConditionCode::VerificationAssertFailed,
                            "verifications[2].assert",
                            &rows,
                        ),
                        record(code, path, &rows),
                    ];
                    assert!(matches!(
                        project(&compiled, &records, Phase::Verification, "s.yaml"),
                        Err(Error::UnsupportedOutcome)
                    ));
                }
            }
        }
        mod yaml_source {
            use crate::support::diagnostic::yaml_source::*;
            #[test]

            fn reason_and_identity_borrow_original_facts_and_unicode_order_is_complete() {
                let reason = String::from("retained parser reason");
                let identity = String::from("source.yaml");
                let result = diagnostics(
                    Failure::InvalidYaml {
                        line: 1,
                        column: 2,
                        reason: &reason,
                    },
                    Some(&identity),
                );
                let Fact::Text(held) = result[0].context["reason"] else {
                    panic!("borrowed reason")
                };
                assert!(std::ptr::eq(held.as_ptr(), reason.as_ptr()));
                let result = diagnostics(Failure::NonAscii { line: 1, column: 2 }, Some(&identity));
                let Fact::Text(held) = result[0].context["path"] else {
                    panic!("borrowed source identity")
                };
                assert!(std::ptr::eq(held.as_ptr(), identity.as_ptr()));
                let result = diagnostics(Failure::NonAscii { line: 1, column: 2 }, None);
                assert_eq!(result[0].context.len(), 2);
                let issues = [
                    UnicodeIssue {
                        path: "$.first".into(),
                        code_point: 0xD800,
                        offset: 0,
                    },
                    UnicodeIssue {
                        path: "$.second[2]".into(),
                        code_point: 0xDFFF,
                        offset: usize::MAX,
                    },
                ];
                let result = diagnostics(Failure::InvalidText(&issues), Some(&identity));
                assert_eq!(
                    result
                        .iter()
                        .map(|d| d.spec_path.as_str())
                        .collect::<Vec<_>>(),
                    ["$.first", "$.second[2]"]
                );
                assert_eq!(
                    result[0].context["code_point"],
                    Fact::OwnedText("U+D800".into())
                );
                assert_eq!(result[1].context["offset"], Fact::Unsigned(usize::MAX));
            }
        }
    }
}
