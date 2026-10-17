//! Numeric contracts.
#[allow(dead_code)]
mod support;
mod numeric {
    mod aggregate_parser {
        use yamaa_core::aggregate_parser::{
            parse_aggregate, GrammarFailure, ParseError, ParseResource, ParsedKind, Reducer,
        };

        /// Read the structured grammar outcome without relying on diagnostic prose.
        fn failure(text: &str) -> (usize, usize, GrammarFailure) {
            match parse_aggregate(text).unwrap_err() {
                ParseError::Grammar { position, failure } => {
                    (position.byte, position.character, failure)
                }
                error => panic!("unexpected limit: {error:?}"),
            }
        }

        /// Distinguish fieldless record dependencies and ungrouped fields in written order.
        #[test]
        fn references_and_spans_preserve_written_context() {
            let text = "\u{2003}NULLIF(sUm( D.A + D.B ), COUNT(D.*)) + D.A + X + COUNT(E.*) + X";
            let parsed = parse_aggregate(text).unwrap();
            assert_eq!(parsed.expression(), text);
            assert_eq!(
                parsed.identifiers().collect::<Vec<_>>(),
                ["D.A", "D.B", "X"]
            );
            assert_eq!(
                parsed.ungrouped_identifiers().collect::<Vec<_>>(),
                ["D.A", "X"]
            );
            assert_eq!(parsed.star_datasets().collect::<Vec<_>>(), ["D", "E"]);
            let mut reductions = Vec::new();
            for (id, node) in parsed.nodes().iter().enumerate() {
                let mut children = Vec::new();
                match &node.kind {
                    ParsedKind::Reduction {
                        reducer,
                        name,
                        operand,
                    } => {
                        reductions.push((
                            *reducer,
                            &text[node.span.start..node.span.end],
                            &text[name.start..name.end],
                        ));
                        children.push(*operand);
                    }
                    ParsedKind::Group { operand } | ParsedKind::Unary { operand, .. } => {
                        children.push(*operand)
                    }
                    ParsedKind::Binary { left, right, .. } => children.extend([*left, *right]),
                    ParsedKind::Call { arguments, .. } => children.extend(arguments),
                    _ => {}
                }
                assert!(children.into_iter().all(|child| child < id));
            }
            assert_eq!(
                reductions,
                [
                    (Reducer::Sum, "sUm( D.A + D.B )", "sUm"),
                    (Reducer::Count, "COUNT(D.*)", "COUNT"),
                    (Reducer::Count, "COUNT(E.*)", "COUNT")
                ]
            );
        }

        /// Closing syntax and complete lexing retain priority over semantic call checks.
        #[test]
        fn diagnostics_have_aggregate_requirements_and_exact_priority() {
            for (text, byte, condition, requirement) in [
                ("SUM(A", 5, "invalid_aggregate_expression", "REQ-0499"),
                ("SUM(A,B)", 5, "invalid_aggregate_expression", "REQ-0499"),
                ("SUM(D.*)", 4, "invalid_aggregate_expression", "REQ-0499"),
                ("COUNT(*)", 6, "invalid_aggregate_expression", "REQ-0499"),
                (
                    "COUNT((D.*))",
                    7,
                    "invalid_aggregate_expression",
                    "REQ-0499",
                ),
                (
                    "COUNT(D.*,A)",
                    9,
                    "invalid_aggregate_expression",
                    "REQ-0499",
                ),
                ("D.*", 0, "invalid_aggregate_expression", "REQ-0499"),
                ("SUM(MIN(A))", 0, "nested_reduction", "REQ-0502"),
                ("SUM(MIN(A)", 10, "invalid_aggregate_expression", "REQ-0499"),
                ("SUM(MIN(A)) > X", 12, "prohibited_construct", "REQ-0512"),
                ("SUM(ABS())", 4, "prohibited_function", "REQ-0500"),
                ("--A AND B", 4, "prohibited_construct", "REQ-0512"),
                ("AVG(A)", 0, "prohibited_function", "REQ-0500"),
                ("AVG(A", 5, "invalid_aggregate_expression", "REQ-0499"),
                ("COUNT(D .*)", 8, "invalid_aggregate_expression", "REQ-0499"),
            ] {
                let (actual, _, failure) = failure(text);
                assert_eq!(
                    (actual, failure.condition(), failure.requirement()),
                    (byte, condition, requirement),
                    "{text}"
                );
            }
            assert_eq!(
                failure("\u{2003}SUM(NULLIF(MIN(A), MAX(B)))"),
                (
                    3,
                    1,
                    GrammarFailure::NestedReduction {
                        outer: Reducer::Sum,
                        inner: Reducer::Min
                    }
                )
            );
            assert_eq!(
                failure("SUM(MAX(MIN(A)))").2,
                GrammarFailure::NestedReduction {
                    outer: Reducer::Max,
                    inner: Reducer::Min
                }
            );
            let (_, _, error) = failure("abS(1,2)");
            assert!(matches!(
                error,
                GrammarFailure::ProhibitedFunction {
                    argument_count: Some(2),
                    ..
                }
            ));
        }

        /// Literal range and missingness are evaluation concerns, never parser checks.
        #[test]
        fn literal_spelling_and_identifier_vocabulary_are_not_evaluated() {
            for text in [
                "SUM(0009223372036854775808)",
                "SUM(1e9999)",
                "NULL",
                "SUM + COUNT",
                "COUNT(TRUE.*)",
                "SUM(A.NULL)",
                "ONLY(NULL)",
            ] {
                assert!(parse_aggregate(text).is_ok(), "{text}");
            }
            let text = format!("SUM({})", "9".repeat(5000));
            let parsed = parse_aggregate(&text).unwrap();
            let literal = &parsed.nodes()[0];
            assert_eq!(literal.kind, ParsedKind::Number { fractional: false });
            assert_eq!(
                &text[literal.span.start..literal.span.end],
                "9".repeat(5000)
            );
        }

        /// Fixed depth protects recursive descent and retained expression trees.
        #[test]
        fn fixed_semantic_depth_guards_aggregate_trees_without_a_breadth_counter() {
            use yamaa_core::limits::Limits;
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                let text = vec!["SUM(A)"; depth - 1].join("+");
                let result = parse_aggregate(&text);
                if depth <= Limits::SEMANTIC_DEPTH {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(
                        result,
                        Err(ParseError::Limit {
                            resource: ParseResource::Depth,
                            limit: Limits::SEMANTIC_DEPTH,
                            ..
                        })
                    ));
                }
            }
            // Breadth has no counter: every argument parses before the closed
            // arity rejects the call.
            let text = format!("NULLIF({})", vec!["SUM(A)"; 3000].join(","));
            assert!(matches!(
                parse_aggregate(&text),
                Err(ParseError::Grammar {
                    failure: GrammarFailure::ProhibitedFunction {
                        argument_count: Some(3000),
                        ..
                    },
                    ..
                })
            ));
            let text = format!("{}A{}", "(".repeat(10000), ")".repeat(10000));
            assert!(matches!(
                parse_aggregate(&text),
                Err(ParseError::Limit {
                    resource: ParseResource::Depth,
                    ..
                })
            ));
        }
    }
    mod bound_expression {
        use std::convert::Infallible;
        use yamaa_core::{
            bound_expression::{
                Binding, BindingError, BoundNumeric, BoundPredicate, Read, ScopeError,
            },
            evaluation::NumericResolver,
            numeric_compiler::compile_numeric,
            predicate::{Node, Plan, Scalar},
            value::{Selection, Value},
        };

        fn binding(name: &str, read: Read) -> Binding {
            Binding {
                name: name.into(),
                read,
            }
        }

        fn numeric(text: &str, bindings: Vec<Binding>) -> Result<BoundNumeric, BindingError> {
            BoundNumeric::new(
                compile_numeric(text, "columns.RESULT.compute").unwrap(),
                bindings,
            )
        }

        fn predicate(path: &str, bindings: Vec<Binding>) -> Result<BoundPredicate, BindingError> {
            BoundPredicate::new(
                Plan::new(
                    vec![
                        Node::Boolean(false),
                        Node::IsNull {
                            value: Scalar::Identifier("A".into()),
                            negated: false,
                        },
                        Node::And(0, 1),
                    ],
                    2,
                    path.into(),
                    "FALSE AND A IS NULL".into(),
                )
                .unwrap(),
                bindings,
            )
        }

        #[test]
        fn numeric_names_are_complete_unique_and_sorted_before_scope_admission() {
            let admitted = numeric(
                "B + A + B",
                vec![binding("B", Read::Column(1)), binding("A", Read::Source(0))],
            )
            .unwrap();
            assert_eq!(
                admitted.bindings(),
                &[binding("A", Read::Source(0)), binding("B", Read::Column(1))]
            );
            assert_eq!(admitted.validate(1, &[false, true], false), Ok(()));
            assert!(admitted.reads_source());
            assert_eq!(
                numeric("A + B", vec![binding("A", Read::Source(0))]),
                Err(BindingError::MissingName)
            );
            assert_eq!(
                numeric(
                    "A",
                    vec![binding("A", Read::Source(0)), binding("A", Read::Column(0))]
                ),
                Err(BindingError::DuplicateName)
            );
            // Existing admission checks each sorted name before looking for absent names.
            assert_eq!(
                numeric(
                    "B",
                    vec![binding("A", Read::Source(0)), binding("A", Read::Source(0))]
                ),
                Err(BindingError::UnusedName)
            );
            assert_eq!(
                numeric("A + B", vec![binding("Z", Read::Source(0))]),
                Err(BindingError::UnusedName)
            );
        }

        #[test]
        fn predicate_short_circuiting_does_not_skip_static_binding() {
            assert_eq!(
                predicate("rows[0].filter", vec![]),
                Err(BindingError::MissingName)
            );
            assert_eq!(predicate("", vec![]), Err(BindingError::EmptyPath));
            let admitted =
                predicate("rows[0].filter", vec![binding("A", Read::Column(0))]).unwrap();
            assert_eq!(admitted.validate(0, &[true], true), Ok(()));
            assert_eq!(
                admitted.validate(0, &[false], false),
                Err(BindingError::UnavailableColumn)
            );
            assert_eq!(
                admitted.validate(0, &[], false),
                Err(BindingError::UnavailableColumn)
            );
            assert_eq!(admitted.plan().spec_path(), "rows[0].filter");
        }

        #[test]
        fn scope_validation_preserves_group_and_sorted_name_error_precedence() {
            let admitted = numeric(
                "A + B",
                vec![binding("B", Read::Column(0)), binding("A", Read::Source(2))],
            )
            .unwrap();
            assert_eq!(
                admitted.validate(1, &[], true),
                Err(ScopeError::GroupedSource)
            );
            assert_eq!(
                admitted.validate(1, &[], false),
                Err(ScopeError::InvalidSource)
            );
            assert_eq!(
                admitted.validate(3, &[], false),
                Err(ScopeError::UnavailableColumn)
            );
            assert_eq!(admitted.validate(3, &[true], false), Ok(()));
            let source = predicate("filter", vec![binding("A", Read::Source(2))]).unwrap();
            assert_eq!(
                source.validate(1, &[], true),
                Err(BindingError::GroupedSource)
            );
            assert_eq!(
                source.validate(1, &[], false),
                Err(BindingError::InvalidSource)
            );
            let output = numeric("A", vec![binding("A", Read::Column(0))]).unwrap();
            assert!(!output.reads_source());
            assert_eq!(output.validate(0, &[true], true), Ok(()));
        }

        #[derive(Default)]
        struct Reads(Vec<String>);
        impl NumericResolver for Reads {
            type Error = Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.0.push(name.into());
                Ok(Selection::Present(Value::Int(1)))
            }
        }

        #[test]
        fn immutable_admission_preserves_deferred_failures_and_occurrences() {
            for text in ["A + A + 1 / 0", "A + A + 9223372036854775808"] {
                let admitted = numeric(text, vec![binding("A", Read::Source(0))]).unwrap();
                for _ in 0..2 {
                    let mut reads = Reads::default();
                    let error = admitted.expression().evaluate(&mut reads).unwrap_err();
                    assert_eq!(reads.0, ["A", "A"]);
                    assert_eq!(error.evaluation.location.expression, text);
                    assert_eq!(admitted.expression().spec_path(), "columns.RESULT.compute");
                }
            }
        }

        #[test]
        fn contextual_case_names_do_not_expand_generic_filter_authority() {
            let original = predicate("case.when", vec![binding("A", Read::Source(0))]).unwrap();
            let collected = BoundPredicate::new_contextual(
                original.plan().clone(),
                original.bindings().to_vec(),
                vec!["A".into()],
            )
            .unwrap();
            assert_eq!(collected.collected(), ["A"]);
            assert!(collected.collects("A"));
            assert!(!collected.collects("B"));
            assert_eq!(
                collected.validate(1, &[], false),
                Err(BindingError::InvalidSource)
            );
            assert_eq!(
                BoundPredicate::new_contextual(
                    original.plan().clone(),
                    vec![binding("A", Read::Column(0))],
                    vec!["A".into()]
                ),
                Err(BindingError::InvalidSource)
            );
            assert_eq!(
                BoundPredicate::new_contextual(
                    original.plan().clone(),
                    original.bindings().to_vec(),
                    vec!["A".into(), "A".into()]
                ),
                Err(BindingError::DuplicateName)
            );
            let kept = BoundPredicate::new_contextual(
                original.plan().clone(),
                vec![binding(
                    "A",
                    Read::Intermediate {
                        index: 0,
                        column: 0,
                    },
                )],
                vec![],
            )
            .unwrap();
            assert_eq!(
                kept.validate(0, &[], false),
                Err(BindingError::InvalidSource)
            );
            assert_eq!(
                BoundPredicate::new(original.plan().clone(), kept.bindings().to_vec()),
                Err(BindingError::InvalidSource)
            );
            assert_eq!(
                BoundPredicate::new_contextual(
                    original.plan().clone(),
                    vec![binding("A", Read::Source(0)); 65_537],
                    vec![]
                ),
                Err(BindingError::DuplicateName)
            );
        }

        #[test]
        fn contextual_case_collection_cannot_borrow_a_foreign_drivers_same_width_coordinates() {
            use yamaa_core::{
                dataset::{
                    Assignment, CaseBranch, DatasetPlan, Expression, PlanError, RowMode,
                    RowTemplate, SecondarySource,
                },
                table::{Column, TableSchema},
                value::ColumnType,
            };
            let schema = TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "V".into(),
                    kind: ColumnType::Int,
                },
            ])
            .unwrap();
            let original = predicate(
                "columns.V.case[0].when",
                vec![binding("A", Read::Source(1))],
            )
            .unwrap();
            let when = BoundPredicate::new_contextual(
                original.plan().clone(),
                original.bindings().to_vec(),
                vec!["A".into()],
            )
            .unwrap();
            let result = DatasetPlan::new_with_sources(
                schema.clone(),
                vec![SecondarySource {
                    name: "ALT".into(),
                    schema: schema.clone(),
                }],
                schema,
                vec![RowTemplate {
                    mode: RowMode::Driver {
                        source: 1,
                        mode: Box::new(RowMode::Records),
                    },
                    assignments: vec![Assignment {
                        column: 0,
                        expression: Expression::Source(0),
                        path: "rows[0].ID".into(),
                    }],
                    filter: None,
                }],
                vec![Assignment {
                    column: 1,
                    path: "columns.V.case".into(),
                    expression: Expression::Case(vec![CaseBranch {
                        when: Some(when),
                        assignment: Box::new(Assignment {
                            column: 1,
                            expression: Expression::Literal(Value::Int(7)),
                            path: "columns.V.case[0].then".into(),
                        }),
                    }]),
                }],
                vec![0],
                vec![],
            );
            assert_eq!(result.unwrap_err(), PlanError::InvalidKeyMode);
        }
    }
    mod function_signature {
        use yamaa_core::{
            function_signature::{
                FunctionIdentity, InvocationPlan, Parameter, PlanError, Presence,
            },
            value::{ColumnType, Value, ValueType},
        };

        fn identity() -> FunctionIdentity {
            FunctionIdentity {
                name: "example".into(),
                contract_version: "1".into(),
                implementation_version: "2".into(),
                call: "artifact.example".into(),
            }
        }
        /// No accepted plan can carry duplicate names, invalid defaults or empty identity.
        #[test]
        fn plans_reject_invalid_normalized_signatures() {
            let base = vec![Parameter {
                name: "x".into(),
                host_name: "host_x".into(),
                kind: ValueType::Int,
                accepts_missing: false,
                presence: Presence::Required,
            }];
            let build = |p| InvocationPlan::new(identity(), p, ColumnType::Int, false);
            for name in ["", "1x", "bad-name", "snow\u{96ea}"] {
                let mut p = base.clone();
                p[0].name = name.into();
                assert_eq!(build(p), Err(PlanError::InvalidName { parameter: 0 }));
            }
            assert_eq!(
                build(vec![base[0].clone(), base[0].clone()]),
                Err(PlanError::DuplicateName { parameter: 1 })
            );
            let mut p = vec![base[0].clone(), base[0].clone()];
            p[1].name = "y".into();
            assert_eq!(build(p), Err(PlanError::DuplicateHostName { parameter: 1 }));
            let mut p = base.clone();
            p[0].host_name.clear();
            assert_eq!(build(p), Err(PlanError::EmptyHostName { parameter: 0 }));
            for default in [Value::Missing, Value::float(1.0), Value::Bool(true)] {
                let mut p = base.clone();
                p[0].presence = Presence::Optional(default);
                assert_eq!(build(p), Err(PlanError::InvalidDefault { parameter: 0 }));
            }
            for slot in 0..4 {
                let mut id = identity();
                [
                    &mut id.name,
                    &mut id.contract_version,
                    &mut id.implementation_version,
                    &mut id.call,
                ][slot]
                    .clear();
                assert_eq!(
                    InvocationPlan::new(id, base.clone(), ColumnType::Int, false),
                    Err(PlanError::EmptyIdentity)
                );
            }
        }

        #[test]
        fn signature_preserves_authored_order_and_missing_defaults() {
            let parameters = vec![
                Parameter {
                    name: "z".into(),
                    host_name: "host_z".into(),
                    kind: ValueType::Int,
                    accepts_missing: true,
                    presence: Presence::Optional(Value::Missing),
                },
                Parameter {
                    name: "a".into(),
                    host_name: "host_a".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
            ];
            let admitted =
                InvocationPlan::new(identity(), parameters.clone(), ColumnType::Date, true)
                    .unwrap();
            assert_eq!(admitted.parameters(), parameters);
            assert_eq!(admitted.identity(), &identity());
            assert_eq!(admitted.returns(), ColumnType::Date);
            assert!(admitted.may_return_missing());
        }

        #[test]
        fn versionless_signature_reuses_exact_defaults_and_refuses_host_renaming() {
            use yamaa_core::function_signature::{ProjectFunctionIdentity, ProjectInvocationPlan};
            let id = ProjectFunctionIdentity {
                name: "identity".into(),
                call: "program.identity".into(),
            };
            let parameter = Parameter {
                name: "x".into(),
                host_name: "x".into(),
                kind: ValueType::Int,
                accepts_missing: true,
                presence: Presence::Optional(Value::Missing),
            };
            let plan = ProjectInvocationPlan::new(
                id.clone(),
                vec![parameter.clone()],
                ColumnType::Int,
                false,
            )
            .unwrap();
            assert_eq!(plan.identity(), &id);
            assert_eq!(
                plan.signature().parameters(),
                std::slice::from_ref(&parameter)
            );
            let mut renamed = parameter.clone();
            renamed.host_name = "host_x".into();
            assert_eq!(
                ProjectInvocationPlan::new(id.clone(), vec![renamed], ColumnType::Int, false),
                Err(PlanError::InvalidHostMapping)
            );
            let mut wrong = parameter;
            wrong.presence = Presence::Optional(Value::float(1.0));
            assert_eq!(
                ProjectInvocationPlan::new(id, vec![wrong], ColumnType::Int, false),
                Err(PlanError::InvalidDefault { parameter: 0 })
            );
        }

        #[test]
        fn versionless_default_identity_keeps_zero_sign_without_changing_value_comparison() {
            use yamaa_core::function_signature::{ProjectFunctionIdentity, ProjectInvocationPlan};
            let make = |zero| {
                ProjectInvocationPlan::new(
                    ProjectFunctionIdentity {
                        name: "identity".into(),
                        call: "program.identity".into(),
                    },
                    vec![Parameter {
                        name: "x".into(),
                        host_name: "x".into(),
                        kind: ValueType::Float,
                        accepts_missing: false,
                        presence: Presence::Optional(Value::float(zero)),
                    }],
                    ColumnType::Float,
                    false,
                )
                .unwrap()
            };
            let negative = make(-0.0);
            assert_eq!(negative, negative.clone());
            assert_ne!(negative, make(0.0));
            assert_eq!(Value::float(-0.0), Value::float(0.0));
        }
    }
    mod math_policy {
        use std::convert::Infallible;
        use yamaa_core::evaluation::{
            EvaluationErrorKind, NumericCondition, NumericResolver, Operand,
        };
        use yamaa_core::numeric::{power, ArithmeticErrorKind, Number};
        use yamaa_core::numeric_compiler::compile_numeric;
        use yamaa_core::value::{Selection, Value};

        /// Record each resolution to verify compiler preflight and failure ordering.
        #[derive(Default)]
        struct Resolver(Vec<String>);
        impl NumericResolver for Resolver {
            type Error = Infallible;
            /// Bind predictable typed values while retaining absent versus missing.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.0.push(name.into());
                Ok(match name {
                    "A" => Selection::Present(Value::Int(2)),
                    "M" => Selection::Present(Value::Missing),
                    _ => Selection::Absent,
                })
            }
        }

        /// The canonical compiler admits portable math before any resolver effects.
        #[test]
        fn current_numeric_vocabulary_compiles_directly() {
            for text in ["EXP(A)", "LN(A)", "POWER(A, A)"] {
                let plan = compile_numeric(text, "spec").unwrap();
                let mut resolver = Resolver::default();
                assert!(matches!(plan.evaluate(&mut resolver), Ok(Number::Float(_))));
                assert_eq!(
                    resolver.0,
                    if text.starts_with("POWER") {
                        vec!["A", "A"]
                    } else {
                        vec!["A"]
                    }
                );
            }
        }

        /// Domain failures belong to calls; operand failures retain their exact inner span.
        #[test]
        fn math_failure_spans_and_order_are_preserved() {
            for (text, span, route, trace) in [
                ("M + (LN(0))", "LN(0)", vec![Operand::Right], vec!["M"]),
                ("(POWER(-2, 0.5))", "POWER(-2, 0.5)", vec![], vec![]),
                (
                    "EXP((UNKNOWN))",
                    "UNKNOWN",
                    vec![Operand::Unary],
                    vec!["UNKNOWN"],
                ),
                (
                    "POWER(M, UNKNOWN)",
                    "UNKNOWN",
                    vec![Operand::Right],
                    vec!["M", "UNKNOWN"],
                ),
                ("LN(0) + UNKNOWN", "LN(0)", vec![Operand::Left], vec![]),
            ] {
                let plan = compile_numeric(text, "spec").unwrap();
                let mut resolver = Resolver::default();
                let error = plan.evaluate(&mut resolver).unwrap_err();
                assert_eq!(&text[error.source_span.start..error.source_span.end], span);
                assert_eq!(error.evaluation.location.operands, route);
                assert_eq!(resolver.0, trace);
            }
        }

        /// Domain context preserves promoted arguments, including the sign of a zero base.
        #[test]
        fn invalid_power_retains_exact_promoted_diagnostic_arguments() {
            for (base, exponent) in [(-0.0, -1.0), (-2.0, 0.5)] {
                let error =
                    power(Number::float(base), Number::float(exponent), "POWER(A, B)").unwrap_err();
                assert_eq!(error.phase(), "derivation");
                assert_eq!(error.condition(), "invalid_power");
                assert_eq!(error.requirement(), "REQ-0433");
                assert_eq!(
                    error.kind,
                    ArithmeticErrorKind::InvalidPower {
                        base_bits: base.to_bits(),
                        exponent_bits: exponent.to_bits()
                    }
                );
            }
            let plan = compile_numeric("POWER(-0.0, -1)", "spec").unwrap();
            let error = plan.evaluate(&mut Resolver::default()).unwrap_err();
            assert_eq!(
                error.evaluation.kind,
                EvaluationErrorKind::Numeric(NumericCondition::Arithmetic(
                    ArithmeticErrorKind::InvalidPower {
                        base_bits: (-0.0_f64).to_bits(),
                        exponent_bits: (-1.0_f64).to_bits()
                    }
                ))
            );
        }

        /// Portable math cannot discard a resolver's opaque failure even after a missing operand.
        #[test]
        fn portable_math_retains_opaque_resolver_failures() {
            #[derive(Debug, PartialEq, Eq)]
            struct Failure(Box<i32>);
            struct FailedResolver;
            impl NumericResolver for FailedResolver {
                type Error = Failure;
                /// Return an owned non-Clone payload to prove no diagnostic reconstruction occurs.
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    assert_eq!(name, "BROKEN");
                    Err(Failure(Box::new(42)))
                }
            }
            for text in ["EXP(BROKEN)", "LN(BROKEN)", "POWER(NULL, BROKEN)"] {
                let plan = compile_numeric(text, "spec").unwrap();
                let error = plan.evaluate(&mut FailedResolver).unwrap_err();
                assert_eq!(
                    &text[error.source_span.start..error.source_span.end],
                    "BROKEN"
                );
                assert_eq!(
                    error.evaluation.kind,
                    EvaluationErrorKind::Resolution {
                        identifier: "BROKEN".into(),
                        error: Failure(Box::new(42))
                    }
                );
            }
        }
    }
    mod numeric_compiler {
        use std::convert::Infallible;

        use yamaa_core::evaluation::{
            EvaluationErrorKind, NumericCondition, NumericResolver, Operand,
        };
        use yamaa_core::numeric::Number;
        use yamaa_core::numeric_compiler::{compile_numeric, CompiledNumeric};
        use yamaa_core::numeric_parser::{GrammarFailure, ParseError};
        use yamaa_core::value::{Selection, Value};

        /// Compile a test expression through the public source-to-plan entry point.
        fn compile(text: &str) -> CompiledNumeric {
            compile_numeric(text, "columns.RESULT.derivation.compute").unwrap()
        }

        /// Record calls while changing A's value so plan reuse cannot hide memoization.
        #[derive(Default)]
        struct Resolver {
            trace: Vec<String>,
        }
        impl NumericResolver for Resolver {
            type Error = Infallible;
            /// Preserve present missing versus absent; each A occurrence has a new integer.
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.trace.push(name.into());
                Ok(match name {
                    "A" => Selection::Present(Value::Int(self.trace.len() as i64)),
                    "M" => Selection::Present(Value::Missing),
                    "B" => Selection::Present(Value::Int(2)),
                    _ => Selection::Absent,
                })
            }
        }

        /// Extract the numeric outcome from a resolver that cannot return a boundary error.
        fn numeric_condition(kind: EvaluationErrorKind<Infallible>) -> NumericCondition {
            let EvaluationErrorKind::Numeric(condition) = kind;
            condition
        }

        /// Replay exact types/bits and deferred literal diagnostics from independent truth.
        #[test]
        fn shared_literal_vectors_through_compilation() {
            for row in include_str!("fixtures/numeric_literals.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 3);
                let mut resolver = Resolver::default();
                let actual = match compile(fields[1]).evaluate(&mut resolver) {
                    Ok(Number::Missing) => "missing".into(),
                    Ok(Number::Int(value)) => format!("int:{value}"),
                    Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
                    Err(error) => {
                        assert_eq!(error.evaluation.location.expression, fields[1]);
                        let condition = numeric_condition(error.evaluation.kind);
                        assert_eq!(condition.phase(), "derivation");
                        assert_eq!(condition.condition(), "integer_overflow");
                        assert_eq!(condition.requirement(), "REQ-0434");
                        let NumericCondition::LiteralOverflow { value } = condition else {
                            panic!("literal failure required");
                        };
                        assert_eq!(
                            fields[1][error.source_span.start..error.source_span.end]
                                .trim_start_matches('0'),
                            value
                        );
                        format!("error:integer_overflow:REQ-0434:{value}")
                    }
                };
                assert_eq!(actual, fields[2], "{}", fields[0]);
                assert!(resolver.trace.is_empty());
            }
        }

        /// Literal failures happen at evaluation time and never hide earlier failures.
        #[test]
        fn oversized_literals_preserve_order_and_missing_does_not_short_circuit() {
            let digits = "9".repeat(5000);
            for (text, expected, trace) in [
                (format!("A + {digits} + B"), "integer_overflow", vec!["A"]),
                (format!("M + {digits} + B"), "integer_overflow", vec!["M"]),
                (format!("{digits} + A"), "integer_overflow", vec![]),
                (
                    format!("UNKNOWN + {digits}"),
                    "unknown_field",
                    vec!["UNKNOWN"],
                ),
                (format!("1 / 0 + {digits}"), "division_by_zero", vec![]),
                (
                    format!("NULLIF(A, {digits})"),
                    "integer_overflow",
                    vec!["A"],
                ),
            ] {
                let plan = compile(&text);
                let mut resolver = Resolver::default();
                let error = plan.evaluate(&mut resolver).unwrap_err();
                let condition = numeric_condition(error.evaluation.kind);
                assert_eq!(condition.condition(), expected);
                assert_eq!(resolver.trace, trace);
            }
        }

        /// Supported plans retain exact semantic spans even under redundant groups/Unicode.
        #[test]
        fn source_spans_identify_the_failed_node() {
            for (text, expected, route) in [
                ("\u{2003}M + ((UNKNOWN))", "UNKNOWN", vec![Operand::Right]),
                ("M + ((1 / 0))", "1 / 0", vec![Operand::Right]),
                (
                    "-((9223372036854775808))",
                    "9223372036854775808",
                    vec![Operand::Unary],
                ),
                ("ABS((1 / 0))", "1 / 0", vec![Operand::Unary]),
                ("SQRT((UNKNOWN))", "UNKNOWN", vec![Operand::Unary]),
                ("SQRT((1 / 0))", "1 / 0", vec![Operand::Unary]),
                ("(SQRT(-1))", "SQRT(-1)", vec![]),
                ("CEIL((UNKNOWN))", "UNKNOWN", vec![Operand::Unary]),
                ("FLOOR((1 / 0))", "1 / 0", vec![Operand::Unary]),
                (
                    "TRUNC((9223372036854775808))",
                    "9223372036854775808",
                    vec![Operand::Unary],
                ),
                ("MOD(A, (UNKNOWN))", "UNKNOWN", vec![Operand::Right]),
                ("(MOD(A, 0))", "MOD(A, 0)", vec![]),
            ] {
                let plan = compile(text);
                let error = plan.evaluate(&mut Resolver::default()).unwrap_err();
                assert_eq!(
                    &text[error.source_span.start..error.source_span.end],
                    expected
                );
                assert_eq!(error.evaluation.location.operands, route);
                assert_eq!(error.evaluation.location.spec_path, plan.spec_path());
                assert_eq!(error.evaluation.location.expression, plan.expression());
            }
        }

        /// Prohibited function spellings remain grammar errors with the current compiler.
        #[test]
        fn prohibited_functions_are_rejected_before_resolution() {
            for text in ["SQRT(A) + ROUND(B)", "A + ROUND(B)"] {
                assert!(matches!(
                    compile_numeric(text, "spec"),
                    Err(ParseError::Grammar {
                        failure: GrammarFailure::ProhibitedFunction { .. },
                        ..
                    })
                ));
            }
        }

        /// Plan reuse resolves each original occurrence again.
        #[test]
        fn plan_reuse_resolves_every_occurrence_again() {
            let plan = compile_numeric("A + A", "spec").unwrap();
            let mut resolver = Resolver::default();
            assert_eq!(plan.evaluate(&mut resolver).unwrap(), Number::Int(3));
            assert_eq!(plan.evaluate(&mut resolver).unwrap(), Number::Int(7));
            assert_eq!(resolver.trace, ["A", "A", "A", "A"]);
        }

        /// The compiled wrapper does not clone, stringify or reclassify resolver failures.
        #[test]
        fn opaque_non_clone_resolution_failure_keeps_span_and_payload() {
            #[derive(Debug, PartialEq)]
            struct BoundaryError(Box<u8>);
            struct FailedResolver;
            impl NumericResolver for FailedResolver {
                type Error = BoundaryError;
                /// Move the opaque boundary payload through exactly one failed resolution.
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    assert_eq!(name, "BROKEN");
                    Err(BoundaryError(Box::new(7)))
                }
            }
            for text in [
                "ABS((BROKEN)) + A",
                "NULLIF(1, (BROKEN)) + A",
                "CEIL((BROKEN)) + A",
                "FLOOR((BROKEN)) + A",
                "TRUNC((BROKEN)) + A",
                "SQRT((BROKEN)) + A",
            ] {
                let error = compile(text).evaluate(&mut FailedResolver).unwrap_err();
                assert_eq!(
                    &text[error.source_span.start..error.source_span.end],
                    "BROKEN"
                );
                assert_eq!(
                    error.evaluation.kind,
                    EvaluationErrorKind::Resolution {
                        identifier: "BROKEN".into(),
                        error: BoundaryError(Box::new(7))
                    }
                );
            }
        }

        /// Unary function calls count identifier occurrences and resolve anew on every execution.
        #[test]
        fn unary_functions_resolve_again_when_plans_are_reused() {
            for name in ["CEIL", "FLOOR", "TRUNC", "SQRT"] {
                let text = format!("{name}(A)");

                let plan = compile(&text);
                let mut resolver = Resolver::default();
                assert_eq!(plan.evaluate(&mut resolver), Ok(Number::float(1.0)));
                assert_eq!(
                    plan.evaluate(&mut resolver),
                    Ok(Number::float(if name == "SQRT" {
                        2.0_f64.sqrt()
                    } else {
                        2.0
                    }))
                );
                assert_eq!(resolver.trace, ["A", "A"]);
            }
        }
    }
    mod numeric_parser {
        use yamaa_core::numeric_parser::{
            parse_numeric, GrammarFailure, ParseError, ParseResource, ParsedKind, SourcePosition,
        };

        /// Read grammar errors independently of their human-readable formatting.
        fn failure(text: &str) -> (SourcePosition, GrammarFailure) {
            match parse_numeric(text).unwrap_err() {
                ParseError::Grammar { position, failure } => (position, failure),
                other => panic!("expected grammar error, got {other:?}"),
            }
        }

        /// Group spans must not overwrite the exact identifier or function-name extent.
        #[test]
        fn spans_and_identifier_order_preserve_original_text() {
            let text = "\u{2003}-(a + ABS(B.x)) + a";
            let parsed = parse_numeric(text).unwrap();
            assert_eq!(parsed.expression(), text);
            assert_eq!(parsed.identifiers().collect::<Vec<_>>(), ["a", "B.x"]);
            let mut slices = Vec::new();
            for (index, node) in parsed.nodes().iter().enumerate() {
                let source = &text[node.span.start..node.span.end];
                slices.push(source);
                match &node.kind {
                    ParsedKind::Group { operand } | ParsedKind::Unary { operand, .. } => {
                        assert!(*operand < index);
                    }
                    ParsedKind::Binary { left, right, .. } => {
                        assert!(*left < index && *right < index);
                    }
                    ParsedKind::Call {
                        name, arguments, ..
                    } => {
                        assert_eq!(&text[name.start..name.end], "ABS");
                        assert!(arguments.iter().all(|&child| child < index));
                    }
                    _ => {}
                }
            }
            assert_eq!(
                slices,
                [
                    "a",
                    "B.x",
                    "ABS(B.x)",
                    "a + ABS(B.x)",
                    "(a + ABS(B.x))",
                    "-(a + ABS(B.x))",
                    "a",
                    "-(a + ABS(B.x)) + a"
                ]
            );
            assert_eq!(parsed.root(), parsed.nodes().len() - 1);
        }

        /// Unicode scalar offsets match Python, while spans remain valid UTF-8 bytes.
        #[test]
        fn grammar_failures_preserve_context_and_both_coordinates() {
            let (position, kind) = failure("\u{2003}\u{a0}A + #");
            assert_eq!(
                position,
                SourcePosition {
                    byte: 9,
                    character: 6
                }
            );
            assert_eq!(kind.condition(), "invalid_numeric_expression");
            assert_eq!(kind.requirement(), "REQ-0439");
            let (position, kind) = failure("\u{2003}aNd");
            assert_eq!(
                position,
                SourcePosition {
                    byte: 3,
                    character: 1
                }
            );
            assert_eq!(
                kind,
                GrammarFailure::ProhibitedConstruct {
                    construct: "boolean"
                }
            );
            assert_eq!(kind.requirement(), "REQ-0441");
            for (text, count) in [("abS(1, 2)", Some(2)), ("foo(1)", None)] {
                let (_, kind) = failure(text);
                assert_eq!(kind.condition(), "prohibited_function");
                assert_eq!(kind.requirement(), "REQ-0440");
                match kind {
                    GrammarFailure::ProhibitedFunction {
                        name,
                        argument_count,
                    } => {
                        assert_eq!(&text[name.start..name.end], &text[..3]);
                        assert_eq!(argument_count, count);
                    }
                    _ => panic!("expected function context"),
                }
            }
        }

        /// Late lexical errors precede syntax; call closure precedes function validation.
        #[test]
        fn failure_priority_matches_reference() {
            for (text, condition, byte) in [
                ("--A > B", "prohibited_construct", 4),
                ("foo(1", "invalid_numeric_expression", 5),
                ("ABS(1,)", "invalid_numeric_expression", 6),
                ("foo(ABS())", "prohibited_function", 4),
                ("A.1", "invalid_numeric_expression", 0),
                ("A.B.C", "invalid_numeric_expression", 3),
                ("1e+", "invalid_numeric_expression", 1),
                ("--A", "invalid_numeric_expression", 1),
                ("NULL()", "prohibited_function", 0),
                (" ", "invalid_numeric_expression", 1),
            ] {
                let (position, kind) = failure(text);
                assert_eq!(
                    (kind.condition(), position.byte),
                    (condition, byte),
                    "{text}"
                );
            }
        }

        /// Range and nonfinite-value checks belong to later evaluation, not parsing.
        #[test]
        fn literals_remain_exact_text_without_host_integer_or_float_conversion() {
            for (text, fractional) in [
                ("0009223372036854775808", false),
                ("1e9999", true),
                ("01.00E+2", true),
                (&"9".repeat(5000), false),
            ] {
                let parsed = parse_numeric(text).unwrap();
                assert_eq!(
                    parsed.nodes()[parsed.root()].kind,
                    ParsedKind::Number { fractional }
                );
                assert_eq!(parsed.expression(), text);
            }
            for text in ["A.TRUE", "NULL.x", "_A._B", "A\u{1c}+\u{1f}B"] {
                assert!(parse_numeric(text).is_ok(), "{text:?}");
            }
        }

        /// Both recursion before node creation and left-associated tree depth are bounded.
        #[test]
        fn fixed_semantic_depth_guards_nesting_and_chains_without_a_breadth_counter() {
            use yamaa_core::limits::Limits;
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                for text in [
                    vec!["A"; depth].join("+"),
                    format!("{}A{}", "(".repeat(depth - 1), ")".repeat(depth - 1)),
                ] {
                    let result = parse_numeric(&text);
                    if depth <= Limits::SEMANTIC_DEPTH {
                        assert!(result.is_ok(), "{text}");
                    } else {
                        assert!(matches!(
                            result,
                            Err(ParseError::Limit {
                                resource: ParseResource::Depth,
                                limit: Limits::SEMANTIC_DEPTH,
                                ..
                            })
                        ));
                    }
                }
            }
            // Breadth has no counter: every argument parses before the closed
            // arity rejects the call.
            let call = format!("NULLIF({})", vec!["A"; 9000].join(","));
            assert!(matches!(
                parse_numeric(&call),
                Err(ParseError::Grammar {
                    failure: GrammarFailure::ProhibitedFunction {
                        argument_count: Some(9000),
                        ..
                    },
                    ..
                })
            ));
            assert!(matches!(
                parse_numeric(&"(".repeat(9000)),
                Err(ParseError::Limit {
                    resource: ParseResource::Depth,
                    ..
                })
            ));
            assert!(matches!(
                parse_numeric(&" ".repeat(65_537)),
                Err(ParseError::Grammar { .. })
            ));
        }
    }
    mod predicate_compiler {
        use std::{collections::BTreeMap, convert::Infallible};
        use yamaa_core::{
            predicate::{self, Resolver, Truth},
            predicate_compiler::{compile, Error},
            predicate_parser::{GrammarFailure, ParseError},
            value::{Selection, Value},
        };

        #[derive(Default)]
        struct Port {
            values: BTreeMap<String, Value>,
            reads: Vec<String>,
        }
        impl Resolver for Port {
            type Error = Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.reads.push(name.into());
                Ok(self
                    .values
                    .get(name)
                    .cloned()
                    .map_or(Selection::Absent, Selection::Present))
            }
        }

        #[test]
        fn original_predicate_forms_execute_with_independent_truth() {
            for (expression, truth) in [
                ("AE.AEOUT = 'FATAL'", Truth::True),
                ("AE.AEOUT <> 'RECOVERED'", Truth::True),
                ("NOT AE.AEOUT = 'FATAL' OR TRUE AND FALSE", Truth::False),
                ("NOT (AE.AEOUT <> 'FATAL' OR FALSE)", Truth::True),
                ("X = 9007199254740993", Truth::True),
                ("X <> 9007199254740992", Truth::True),
                ("-9223372036854775808 < +0001", Truth::True),
                ("-2.50e+3 <= -2500", Truth::True),
                ("1e309 = 0", Truth::Unknown),
                ("-1e9999 IS NULL", Truth::True),
                ("TRUE OR X = 1e309", Truth::True),
                ("3 > 2 AND 3 >= 3", Truth::True),
                ("M IS NULL AND X IS NOT NULL", Truth::True),
                ("AE.AEOUT IN ('RECOVERED', 'FATAL', NULL)", Truth::True),
                ("AE.AEOUT NOT IN ('RECOVERED', NULL)", Truth::Unknown),
                (
                    "X BETWEEN 9007199254740993 AND 9007199254740994",
                    Truth::True,
                ),
                ("X NOT BETWEEN 0 AND 5", Truth::True),
                ("AE.AEOUT LIKE 'F_T%'", Truth::True),
                ("'F_T' LIKE 'F!_T' ESCAPE '!'", Truth::True),
                ("AE.AEOUT NOT LIKE 'R%'", Truth::True),
                ("str_contains(AE.AEOUT, '^FA(T|R)AL$')", Truth::True),
                ("str_contains(M, '^$')", Truth::Unknown),
                ("DATE '2024-02-29' < DATE '2024-03-01'", Truth::True),
                (
                    "DATETIME '2024-02-29T12:34' = DATETIME '2024-02-29T12:34:00'",
                    Truth::True,
                ),
                ("'it''s' = 'it''s'", Truth::True),
            ] {
                let plan = compile(expression, "intermediates[0].filter").unwrap();
                let mut port = Port {
                    values: [
                        ("AE.AEOUT".into(), Value::Str("FATAL".into())),
                        ("X".into(), Value::Int(9007199254740993)),
                        ("M".into(), Value::Missing),
                    ]
                    .into(),
                    ..Default::default()
                };
                assert_eq!(plan.spec_path(), "intermediates[0].filter");
                assert_eq!(plan.evaluate(&mut port).unwrap(), truth, "{expression}");
            }
        }

        #[test]
        fn compilation_preserves_eager_order_and_repeated_occurrences() {
            for (text, truth, reads) in [
                ("FALSE AND X = 1", Truth::False, vec!["X"]),
                ("TRUE OR X = 1", Truth::True, vec!["X"]),
                ("X = 1 AND X = 1", Truth::True, vec!["X", "X"]),
                ("X BETWEEN 0 AND 2", Truth::True, vec!["X", "X"]),
            ] {
                let plan = compile(text, "site").unwrap();
                let mut port = Port {
                    values: [("X".into(), Value::Int(1))].into(),
                    ..Default::default()
                };
                assert_eq!(plan.evaluate(&mut port).unwrap(), truth);
                assert_eq!(port.reads, reads);
                port.reads.clear();
                assert_eq!(plan.evaluate(&mut port).unwrap(), truth);
                assert_eq!(port.reads, reads, "reusing a plan must repeat resolution");
            }
            for text in ["FALSE AND ABSENT = 1", "TRUE OR ABSENT = 1"] {
                let plan = compile(text, "site").unwrap();
                let mut port = Port::default();
                let error = plan.evaluate(&mut port).unwrap_err();
                assert_eq!(port.reads, ["ABSENT"]);
                assert_eq!(error.spec_path, "site");
                assert_eq!(error.expression, text);
                assert!(matches!(
                    error.kind,
                    predicate::ErrorKind::Condition(predicate::Condition::UnknownField { .. })
                ));
            }
        }

        #[test]
        fn whole_expression_grammar_and_integer_carrier_precede_execution() {
            assert!(matches!(
                compile("FALSE AND X =", "site"),
                Err(Error::Parse(ParseError::Grammar { .. }))
            ));
            for text in [
                "FALSE AND X = 9223372036854775808",
                "X = -9223372036854775809",
            ] {
                let Err(Error::Parse(ParseError::Grammar {
                    failure, position, ..
                })) = compile(text, "site")
                else {
                    panic!("expected literal rejection")
                };
                assert_eq!(failure, GrammarFailure::IntegerOverflow);
                assert_eq!(
                    position.character,
                    text.find("9223372036854775808")
                        .unwrap_or_else(|| text.find("-9223372036854775809").unwrap())
                );
                let finding = failure.original_diagnostic("site", text, position.character);
                assert_eq!(finding.definition().condition, "integer_overflow");
                assert_eq!(finding.definition().phase, "parse");
                assert_eq!(finding.definition().requirement, Some("REQ-0014"));
            }
        }
    }
    mod predicate_context {
        //! Owned and borrowed resolution preserve independent truth and occurrence effects.
        use std::{collections::BTreeMap, convert::Infallible, rc::Rc};
        use yamaa_core::{
            predicate::{ErrorKind, Resolved, Resolver, Truth},
            predicate_compiler,
            table::ValueRef,
            value::{Selection, Value},
        };
        struct Borrowed {
            values: BTreeMap<String, Value>,
            trace: Vec<String>,
        }
        impl Resolver for Borrowed {
            type Error = Infallible;
            fn resolve(&mut self, id: &str) -> Result<Selection, Self::Error> {
                unreachable!("borrowed port only: {id}")
            }
            fn resolve_value(&mut self, id: &str) -> Result<Resolved<'_>, Self::Error> {
                self.trace.push(id.into());
                Ok(self
                    .values
                    .get(id)
                    .map_or(Resolved::Absent, |v| Resolved::Borrowed(ValueRef::from(v))))
            }
        }
        struct Owned {
            values: BTreeMap<String, Value>,
            trace: Vec<String>,
            fail: Option<(Box<u8>, Rc<()>)>,
        }
        impl Resolver for Owned {
            type Error = (Box<u8>, Rc<()>);
            fn resolve(&mut self, id: &str) -> Result<Selection, Self::Error> {
                self.trace.push(id.into());
                if let Some(error) = self.fail.take() {
                    return Err(error);
                }
                Ok(self
                    .values
                    .get(id)
                    .cloned()
                    .map_or(Selection::Absent, Selection::Present))
            }
        }
        fn owned(values: &[(&str, Value)]) -> Owned {
            Owned {
                values: values
                    .iter()
                    .map(|(k, v)| ((*k).into(), v.clone()))
                    .collect(),
                trace: vec![],
                fail: None,
            }
        }
        #[test]
        fn owned_and_borrowed_ports_preserve_truth_and_written_occurrences() {
            for (expr, values, truth, names) in [
                (
                    "a = 0 AND b = 1",
                    vec![("a", Value::Int(2)), ("b", Value::Int(1))],
                    Truth::False,
                    vec!["a", "b"],
                ),
                (
                    "a = 2 OR b = 1",
                    vec![("a", Value::Int(2)), ("b", Value::Missing)],
                    Truth::True,
                    vec!["a", "b"],
                ),
                (
                    "a BETWEEN b AND c",
                    vec![
                        ("a", Value::Int(3)),
                        ("b", Value::Int(1)),
                        ("c", Value::Int(5)),
                    ],
                    Truth::True,
                    vec!["a", "b", "a", "c"],
                ),
                (
                    "str_contains(a, 'a.*')",
                    vec![("a", Value::Str("baaa".into()))],
                    Truth::True,
                    vec!["a"],
                ),
                (
                    "a LIKE 'a%'",
                    vec![("a", Value::Str("abc".into()))],
                    Truth::True,
                    vec!["a"],
                ),
            ] {
                let plan = predicate_compiler::compile(expr, "columns.F.flag.condition").unwrap();
                let mut new = owned(&values);
                let mut old = Borrowed {
                    values: new.values.clone(),
                    trace: vec![],
                };
                assert_eq!(plan.evaluate(&mut old).unwrap(), truth);
                assert_eq!(plan.evaluate(&mut new).unwrap(), truth);
                assert_eq!(old.trace, names);
                assert_eq!(new.trace, names);
            }
        }
        #[test]
        fn fixed_regex_steps_preserve_the_actual_resolution_prefix_and_fresh_retry() {
            let plan =
                predicate_compiler::compile("str_contains(a, 'z') AND b = 1", "flag.condition")
                    .unwrap();
            let mut port = owned(&[("a", Value::Str("a".repeat(500_000))), ("b", Value::Int(1))]);
            assert!(matches!(
                plan.evaluate(&mut port).unwrap_err().kind,
                ErrorKind::Regex(yamaa_core::regex::MatchError {
                    resource: yamaa_core::regex::Resource::Work,
                    limit: 1_000_000
                })
            ));
            assert_eq!(port.trace, ["a"]);
            port.values.insert("a".into(), Value::Str("z".into()));
            port.trace.clear();
            assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b"]);
        }
        #[test]
        fn data_driven_like_keeps_scalar_truth_and_eager_resolution() {
            let expression = "a LIKE b AND c = 1";
            let plan = predicate_compiler::compile(expression, "flag.condition").unwrap();
            let mut port = owned(&[
                ("a", Value::Str("a".repeat(100))),
                ("b", Value::Str("%a".repeat(100) + "b")),
                ("c", Value::Int(1)),
            ]);
            assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::False);
            assert_eq!(port.trace, ["a", "b", "c"]);
            port.values.insert("a".into(), Value::Str("\n😀".into()));
            port.values.insert("b".into(), Value::Str("_%".into()));
            port.trace.clear();
            assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b", "c"]);
        }
        #[test]
        fn like_preserves_full_scalar_truth_without_regex_syntax_or_line_rules() {
            let plan = predicate_compiler::compile("a LIKE b", "flag.condition").unwrap();
            let negated = predicate_compiler::compile("a NOT LIKE b", "flag.condition").unwrap();
            for (value, pattern, expected) in [
                ("", "", Truth::True),
                ("", "%", Truth::True),
                ("", "_", Truth::False),
                ("a", "", Truth::False),
                ("a", "__", Truth::False),
                ("ab", "_", Truth::False),
                ("ab", "a", Truth::False),
                ("ba", "a", Truth::False),
                ("ab", "a%", Truth::True),
                ("ba", "%a", Truth::True),
                ("abab", "%ab%ab", Truth::True),
                ("Ab", "a%", Truth::False),
                ("😀", "_", Truth::True),
                ("é", "__", Truth::False),
                ("e\u{301}", "__", Truth::True),
                ("e\u{301}", "é", Truth::False),
                ("\r\n", "__", Truth::True),
                ("\n😀\u{2028}", "_%_", Truth::True),
                (".^$[](){}*+?|\\", ".^$[](){}*+?|\\", Truth::True),
                ("a\\b", "a\\b", Truth::True),
                ("%_", "\\%\\_", Truth::False),
            ] {
                let values = [
                    ("a", Value::Str(value.into())),
                    ("b", Value::Str(pattern.into())),
                ];
                let mut port = owned(&values);
                assert_eq!(
                    plan.evaluate(&mut port).unwrap(),
                    expected,
                    "{value:?} LIKE {pattern:?}"
                );
                assert_eq!(port.trace, ["a", "b"]);
                let mut borrowed = Borrowed {
                    values: port.values,
                    trace: vec![],
                };
                assert_eq!(plan.evaluate(&mut borrowed).unwrap(), expected);
                assert_eq!(borrowed.trace, ["a", "b"]);
                assert_eq!(negated.evaluate(&mut borrowed).unwrap(), expected.negate());
            }
            for character in [
                '\0',
                '\n',
                '\r',
                '\u{b}',
                '\u{c}',
                '\u{85}',
                '\u{2028}',
                '\u{2029}',
                '\u{10ffff}',
            ] {
                let mut port = owned(&[
                    ("a", Value::Str(character.into())),
                    ("b", Value::Str("_".into())),
                ]);
                assert_eq!(
                    plan.evaluate(&mut port).unwrap(),
                    Truth::True,
                    "{character:?}"
                );
            }
            for (value, pattern) in [
                ("a".repeat(3000), "a".repeat(3000)),
                ("\n😀".repeat(1000), "%".repeat(10_000)),
            ] {
                let mut port = owned(&[("a", Value::Str(value)), ("b", Value::Str(pattern))]);
                assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::True);
            }
        }
        #[test]
        fn like_validates_complete_escape_spelling_and_preserves_missing_and_type_owners() {
            use yamaa_core::predicate::{Condition, Route};
            use yamaa_core::value::ValueType;

            for (expression, value, pattern) in [
                ("a LIKE b ESCAPE '!'", "%_!", "!%!_!!"),
                ("a LIKE b ESCAPE '😀'", "%_😀", "😀%😀_😀😀"),
                ("a LIKE b ESCAPE '%'", "_", "%_"),
                ("a LIKE b ESCAPE '_'", "%", "_%"),
            ] {
                let plan = predicate_compiler::compile(expression, "flag.condition").unwrap();
                let mut port = owned(&[
                    ("a", Value::Str(value.into())),
                    ("b", Value::Str(pattern.into())),
                ]);
                assert_eq!(
                    plan.evaluate(&mut port).unwrap(),
                    Truth::True,
                    "{expression}"
                );
            }
            let expression = "a LIKE b ESCAPE '!'";
            let plan = predicate_compiler::compile(expression, "flag.condition").unwrap();
            for (value, pattern) in [
                ("".into(), "!".into()),
                ("unrelated".into(), "z!".into()),
                ("a".repeat(100), "%a".repeat(100) + "!"),
            ] {
                let mut port = owned(&[("a", Value::Str(value)), ("b", Value::Str(pattern))]);
                let error = plan.evaluate(&mut port).unwrap_err();
                assert_eq!(error.kind, ErrorKind::Condition(Condition::DanglingEscape));
                assert_eq!(error.spec_path, "flag.condition");
                assert_eq!(error.expression, expression);
                assert!(error.route.is_empty());
                assert_eq!(port.trace, ["a", "b"]);
            }
            for (value, pattern) in [
                (Value::Missing, Value::Str("!".into())),
                (Value::Str("a".into()), Value::Missing),
                (Value::Missing, Value::Int(1)),
                (Value::Int(1), Value::Missing),
            ] {
                let mut port = owned(&[("a", value), ("b", pattern)]);
                assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::Unknown);
                assert_eq!(port.trace, ["a", "b"]);
                let negated =
                    predicate_compiler::compile("a NOT LIKE b ESCAPE '!'", "flag.condition")
                        .unwrap();
                assert_eq!(negated.evaluate(&mut port).unwrap(), Truth::Unknown);
            }
            for (value, pattern, actual) in [
                (Value::Int(1), Value::Str("%".into()), ValueType::Int),
                (Value::Str("a".into()), Value::Bool(true), ValueType::Bool),
            ] {
                let mut port = owned(&[("a", value), ("b", pattern)]);
                let error = plan.evaluate(&mut port).unwrap_err();
                assert_eq!(
                    error.kind,
                    ErrorKind::Condition(Condition::ExpectedText { actual })
                );
                assert_eq!(error.spec_path, "flag.condition");
                assert!(error.route.is_empty());
                assert_eq!(port.trace, ["a", "b"]);
            }
            let mut port = owned(&[("a", Value::Missing)]);
            let error = plan.evaluate(&mut port).unwrap_err();
            assert_eq!(error.route, [Route::Pattern]);
            assert_eq!(
                error.kind,
                ErrorKind::Condition(Condition::UnknownField {
                    identifier: "b".into()
                })
            );
            assert_eq!(port.trace, ["a", "b"]);
        }
        #[test]
        fn like_preserves_an_opaque_pattern_resolution_error_before_later_work() {
            struct Port {
                trace: Vec<String>,
                payload: Option<Box<u8>>,
                marker: Rc<()>,
            }
            impl Resolver for Port {
                type Error = (Box<u8>, Rc<()>);
                fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
                    self.trace.push(identifier.into());
                    match identifier {
                        "a" => Ok(Selection::Present(Value::Str("a".into()))),
                        "b" => Err((self.payload.take().unwrap(), self.marker.clone())),
                        _ => panic!("later operand was resolved"),
                    }
                }
            }
            let marker = Rc::new(());
            let payload = Box::new(7);
            let address = &*payload as *const u8;
            let mut port = Port {
                trace: vec![],
                payload: Some(payload),
                marker: marker.clone(),
            };
            let expression = "a LIKE b AND c = 1";
            let plan = predicate_compiler::compile(expression, "rows[0].filter").unwrap();
            let error = plan.evaluate(&mut port).unwrap_err();
            assert_eq!(error.spec_path, "rows[0].filter");
            assert_eq!(error.expression, expression);
            assert_eq!(
                error.route,
                [
                    yamaa_core::predicate::Route::Left,
                    yamaa_core::predicate::Route::Pattern
                ]
            );
            let ErrorKind::Resolution {
                identifier,
                error: (payload, retained),
            } = error.kind
            else {
                panic!("original resolution error")
            };
            assert_eq!(identifier, "b");
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&marker, &retained));
            assert_eq!(port.trace, ["a", "b"]);
        }
        #[test]
        fn opaque_error_moves_once_without_later_resolution_and_retains_site_route() {
            let plan = predicate_compiler::compile("a = 1 OR b = 2", "flag.condition").unwrap();
            let marker = Rc::new(());
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let mut ctx = owned(&[]);
            ctx.fail = Some((payload, marker.clone()));
            let error = plan.evaluate(&mut ctx).unwrap_err();
            assert_eq!(error.spec_path, "flag.condition");
            assert_eq!(error.expression, "a = 1 OR b = 2");
            let ErrorKind::Resolution {
                identifier,
                error: (payload, retained),
            } = error.kind
            else {
                panic!("opaque")
            };
            assert_eq!(identifier, "a");
            assert_eq!(&*payload as *const u8, address);
            assert!(Rc::ptr_eq(&marker, &retained));
            assert_eq!(ctx.trace, ["a"]);
        }
    }
    mod predicate_parser {
        use yamaa_core::predicate::Comparison;
        use yamaa_core::predicate_parser::{
            parse_predicate, GrammarFailure, ParseError, ParseResource, ParsedKind, TemporalKind,
        };
        use yamaa_core::temporal::TemporalError;

        #[test]
        fn integer_carrier_admission_preserves_exact_boundaries_and_whole_grammar_order() {
            for expression in [
                "X = -9223372036854775808",
                "X = +9223372036854775807",
                "X = 0009223372036854775807",
                "X IN (-9223372036854775808, 9223372036854775807)",
                "X = 9223372036854775808.0",
                "X = 1e309",
            ] {
                parse_predicate(expression).unwrap();
            }
            for expression in [
                "X = 9223372036854775808",
                "X = -9223372036854775809",
                "FALSE AND X IN (1, +9223372036854775808)",
                "'\u{1f600}' = -9223372036854775809",
                "X BETWEEN 0 AND 9999999999999999999999999999999",
            ] {
                let Err(ParseError::Grammar {
                    failure, position, ..
                }) = parse_predicate(expression)
                else {
                    panic!("overflowing literal admitted: {expression}")
                };
                assert_eq!(failure, GrammarFailure::IntegerOverflow);
                let suffix = &expression[position.byte..];
                assert!(
                    suffix.starts_with("9223372036854775808")
                        || suffix.starts_with("+9223372036854775808")
                        || suffix.starts_with("-9223372036854775809")
                        || suffix.starts_with("9999999999999999999999999999999")
                );
                assert_eq!(
                    position.character,
                    expression[..position.byte].chars().count()
                );
                let finding = failure.original_diagnostic("filter", expression, position.character);
                assert_eq!(finding.definition().phase, "parse");
                assert_eq!(finding.definition().condition, "integer_overflow");
                assert_eq!(finding.definition().requirement, Some("REQ-0014"));
                assert_eq!(finding.spec_paths, ["filter"]);
                assert_eq!(
                    finding.context["predicate"],
                    yamaa_core::json::Value::String(expression.into())
                );
                assert_eq!(
                    finding.context["position"],
                    yamaa_core::json::Value::from(position.character)
                );
            }
            assert!(matches!(
                parse_predicate("X = 9223372036854775808 AND"),
                Err(ParseError::Grammar {
                    failure: GrammarFailure::InvalidExpression,
                    ..
                })
            ));
        }

        /// Syntax association is independent of evaluator truth and preserves written names.
        #[test]
        fn precedence_spelling_and_names() {
            let p = parse_predicate("NOT A = +0001 OR B < -2.50e+3 AND (A IS NULL)").unwrap();
            assert_eq!(p.identifiers().collect::<Vec<_>>(), ["A", "B"]);
            let nodes = p.nodes();
            let ParsedKind::Or(left, right) = nodes[p.root()].kind else {
                panic!()
            };
            assert!(matches!(nodes[left].kind, ParsedKind::Not(_)));
            let ParsedKind::And(_, group) = nodes[right].kind else {
                panic!()
            };
            assert!(matches!(nodes[group].kind, ParsedKind::Group(_)));
            let numbers: Vec<_> = nodes
                .iter()
                .filter_map(|n| match n.kind {
                    ParsedKind::Number { fractional } => {
                        Some((&p.expression()[n.span.start..n.span.end], fractional))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(numbers, [("+0001", false), ("-2.50e+3", true)]);
            let p = parse_predicate("FALSE OR TRUE OR FALSE").unwrap();
            let ParsedKind::Or(left, _) = p.nodes()[p.root()].kind else {
                panic!()
            };
            assert!(matches!(p.nodes()[left].kind, ParsedKind::Or(..)));
        }

        /// Every closed comparison and compound production admits only operand syntax.
        #[test]
        fn closed_vocabulary() {
            for (op, expected) in [
                ("=", Comparison::Equal),
                ("<>", Comparison::NotEqual),
                ("<", Comparison::Less),
                ("<=", Comparison::LessEqual),
                (">", Comparison::Greater),
                (">=", Comparison::GreaterEqual),
            ] {
                let p = parse_predicate(&format!("A {op} NULL")).unwrap();
                assert!(
                    matches!(p.nodes()[p.root()].kind,ParsedKind::Compare{operator,..} if operator==expected)
                );
            }
            for text in [
                "A NOT IN (1, 2, NULL)",
                "A BETWEEN -1 AND 1 OR FALSE",
                "A NOT BETWEEN B AND C",
                "A NOT LIKE '%' ESCAPE '!'",
                "A IS NOT NULL",
                "str_contains = 'column'",
                "D.TRUE = 1",
                "STR_CONTAINS(NULL, '^$')",
            ] {
                parse_predicate(text).unwrap();
            }
            for text in [
                "",
                " ",
                "A",
                "NULL",
                "A != 1",
                "A == 1",
                "A IN ()",
                "A IS TRUE",
                "A IN ((1))",
                "A BETWEEN 1 OR 2",
                "A NOT = 1",
                "A LIKE '%' ESCAPE 1",
                "A+1 = 2",
                "TRUE()",
                "str_detect(A, 'a')",
                "str_contains(A, B)",
                "str_contains(A)",
                "str_contains(A, 'a', 'b')",
                "NOT NULL",
                "(A) = 1",
                "A. = 1",
                "A.B.C = 1",
                "A = 1.",
                "A = .1",
                "A = - 1",
                "A = 1e+",
                "A = 0x10",
                "A = 'open",
                "A = TRUE",
            ] {
                assert!(
                    matches!(parse_predicate(text), Err(ParseError::Grammar { .. })),
                    "{text}"
                );
            }
            for keyword in [
                "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE", "NOT",
                "NULL", "OR", "TRUE",
            ] {
                parse_predicate(&format!("D.{keyword} = 1")).unwrap();
            }
        }

        /// Doubled quotes decode exactly once; other Unicode, backslash and NUL are literal.
        #[test]
        fn text_temporal_and_unbounded_number_syntax() {
            let text = "'it''s\\\0\u{1f600}' = 'x'";
            let p = parse_predicate(text).unwrap();
            assert_eq!(
                p.nodes()[0].kind,
                ParsedKind::String("it's\\\0\u{1f600}".into())
            );
            for text in [
                "A = DATE '2024-02-29'",
                "A = DATETIME '2024-02-29T12:34'",
                "A = DATETIME '2024-02-29T12:34:56'",
                "A = 1e9999",
            ] {
                parse_predicate(text).unwrap();
            }
            assert!(matches!(
                parse_predicate("A = DATE '2025-02-30'"),
                Err(ParseError::Grammar {
                    failure: GrammarFailure::InvalidTemporal {
                        kind: TemporalKind::Date,
                        error: TemporalError::InvalidDate
                    },
                    ..
                })
            ));
            for text in [
                "A = DATE '2024'",
                "A = DATETIME '2024-02-29T12:34Z'",
                "A = DATETIME '2024-02-29T24:00'",
                "A = DATE '0000-01-01'",
            ] {
                assert!(matches!(
                    parse_predicate(text),
                    Err(ParseError::Grammar {
                        failure: GrammarFailure::InvalidTemporal { .. },
                        ..
                    })
                ));
            }
        }

        /// Regex admission uses portable grammar, including features rejected by the reference host.
        #[test]
        fn portable_regex_literals_are_checked_before_closing_token() {
            for pattern in [
                r"[]",
                r"[^]",
                r"\1(a)",
                r"(a)(?<=\1)",
                r"(?<\u0061>a)\k<a>",
                r"[\S]",
                r"(?<=a|b)c",
                r"(?<name>a)\k<name>",
            ] {
                let text = format!("str_contains(A, '{pattern}')");
                let p = parse_predicate(&text).unwrap();
                assert!(matches!(
                    p.nodes()[p.root()].kind,
                    ParsedKind::Contains { .. }
                ));
            }
            for pattern in [r"(?P<name>a)", r"(?i)a", r"\p{Letter}", r"\a", r"(?<=a+)b"] {
                let expression = format!("str_contains(A, '{pattern}')");
                let Err(ParseError::Grammar {
                    failure: GrammarFailure::InvalidRegex { .. },
                    ..
                }) = parse_predicate(&expression)
                else {
                    panic!("REQ-0826 syntax must be a registered finding: {pattern}")
                };
            }
            let text = "str_contains('\u{1f600}', '('";
            let Err(ParseError::Grammar {
                position, failure, ..
            }) = parse_predicate(text)
            else {
                panic!()
            };
            assert_eq!(position.byte, 21);
            assert_eq!(position.character, 18);
            assert!(matches!(
                failure,
                GrammarFailure::InvalidRegex { byte: 1, .. }
            ));
            assert_eq!(failure.requirement(), "REQ-1244");
            // Whole-input tokenization still precedes invalid regex admission.
            assert!(matches!(
                parse_predicate("str_contains(A, '(') @"),
                Err(ParseError::Grammar {
                    failure: GrammarFailure::InvalidExpression,
                    ..
                })
            ));
            parse_predicate("str_contains(A, 'a{1000001}')").unwrap();
        }

        /// Static ESCAPE defects have their owning requirement and original literal site.
        #[test]
        fn escape_admission_and_unicode_positions() {
            for text in [
                "A LIKE 'a!' ESCAPE '!'",
                "A LIKE 'a' ESCAPE ''",
                "A LIKE 'a' ESCAPE 'ab'",
            ] {
                let Err(ParseError::Grammar { failure, .. }) = parse_predicate(text) else {
                    panic!("{text}")
                };
                assert_eq!(failure, GrammarFailure::InvalidEscape);
                assert_eq!(failure.requirement(), "REQ-0191");
            }
            for text in [
                "A LIKE 'a!!' ESCAPE '!'",
                "A LIKE 'a!!!b' ESCAPE '!'",
                "A LIKE B ESCAPE '!'",
                "A LIKE 'a\u{1f600}%' ESCAPE '\u{1f600}'",
            ] {
                parse_predicate(text).unwrap();
            }
            let Err(ParseError::Grammar { position, .. }) = parse_predicate("'\u{1f600}' = @")
            else {
                panic!()
            };
            assert_eq!((position.byte, position.character), (9, 6));
        }

        /// The sole parser bound protects semantic recursion, independently on each call.
        #[test]
        fn fixed_depth_and_fresh_retry() {
            for text in [
                "TRUE AND ".repeat(140) + "TRUE",
                "NOT ".repeat(140) + "TRUE",
                format!("{}TRUE{}", "(".repeat(140), ")".repeat(140)),
            ] {
                assert!(matches!(
                    parse_predicate(&text),
                    Err(ParseError::Limit {
                        resource: ParseResource::Depth,
                        limit: 128,
                        ..
                    })
                ));
                parse_predicate("TRUE").unwrap();
            }
            let p = parse_predicate(&format!("A IN ({})", vec!["1"; 1000].join(","))).unwrap();
            assert_eq!(p.nodes().len(), 1002);
        }

        /// Repeated admitted regex literals do not share an invented request quota.
        #[test]
        fn repeated_regex_compilation_retains_each_admitted_pattern() {
            let pattern = format!("(a){}(?<=\\1)", "(a|aa)".repeat(8));
            let call = format!("str_contains(A, '{pattern}')");
            parse_predicate(&call).unwrap();
            let mut expression = call.clone();
            for _ in 0..9 {
                expression = format!("({expression} OR {expression})");
            }
            parse_predicate(&expression).unwrap();
            parse_predicate(&call).unwrap();
        }

        /// Reasons belong to Rust productions, so host planning never reparses a failed input.
        #[test]
        fn grammar_messages_preserve_production_reason() {
            for (text, expected) in [
                ("", "expected operand"),
                ("A >", "expected operand"),
                ("A = TRUE", "expected operand"),
                ("TRUE FALSE", "unexpected trailing token"),
                ("A = 'open", "unterminated string literal"),
                ("A. = 1", "invalid qualified identifier"),
                ("A @", "unexpected character"),
                ("A \u{200b}", "unexpected character"),
                ("(TRUE", "expected ')' to close predicate"),
                ("A IS 1", "expected NULL after IS"),
                ("A IN 1", "expected '(' after IN"),
                ("A IN (1 2)", "expected ')' after IN operands"),
                ("A BETWEEN 1 OR 2", "expected AND in BETWEEN predicate"),
                ("A LIKE 'x' ESCAPE 1", "ESCAPE requires a string literal"),
                (
                    "A LIKE 'x' ESCAPE ''",
                    "ESCAPE requires exactly one code point",
                ),
                (
                    "A LIKE 'x!' ESCAPE '!'",
                    "LIKE pattern has a dangling escape",
                ),
                ("A NOT = 1", "NOT must precede IN, BETWEEN, or LIKE"),
                ("A", "operand must be followed by a Boolean operator"),
                ("A = DATE 1", "DATE requires a string literal"),
                ("A = DATETIME 1", "DATETIME requires a string literal"),
                ("A = DATE '2025-02-30'", "invalid date literal"),
                ("A = DATETIME 'bad'", "invalid datetime literal"),
                (
                    "str_contains(A)",
                    "expected ',' between str_contains source and pattern",
                ),
                (
                    "str_contains(A, B)",
                    "str_contains pattern must be a string literal",
                ),
                ("str_contains(A, 'a'", "expected ')' to close str_contains"),
                (
                    "str_contains(A, '[')",
                    "invalid regex in str_contains pattern: unclosed character class",
                ),
            ] {
                let Err(ParseError::Grammar { message, .. }) = parse_predicate(text) else {
                    panic!("expected a grammar failure for {text}")
                };
                assert_eq!(message, expected, "{text}");
            }
        }
    }
    mod regex {
        use yamaa_core::regex::{CompileError, MatchError, Pattern, Resource};

        /// Each match owns its fixed step guard; successful earlier matches do not consume it.
        #[test]
        fn each_match_has_one_fresh_step_scope() {
            let compiled = pattern("a+");
            for _ in 0..3 {
                assert_eq!(
                    compiled.search("aaaa").unwrap().unwrap().groups,
                    [Some("aaaa")]
                );
            }
        }

        /// Unicode subjects retain exact capture text in both matching modes.
        #[test]
        fn unicode_subjects_keep_exact_scalar_captures() {
            let compiled = pattern(".");
            assert_eq!(compiled.search("😀").unwrap().unwrap().groups, [Some("😀")]);
            assert_eq!(
                compiled.full_match("😀").unwrap().unwrap().groups,
                [Some("😀")]
            );
            assert!(pattern("a").search("b").unwrap().is_none());
        }

        /// Compile a test pattern under production defaults.
        fn pattern(text: &str) -> Pattern {
            Pattern::compile(text).unwrap()
        }
        /// Observe borrowed capture text without converting missing groups to empty strings.
        fn captures<'a>(text: &str, subject: &'a str) -> Option<Vec<Option<&'a str>>> {
            pattern(text).search(subject).unwrap().map(|m| m.groups)
        }

        /// Written truth covers leftmost order, greedy/lazy branching and full-match continuation.
        #[test]
        fn backtracking_order_and_empty_matches() {
            assert_eq!(captures("a|ab", "ab"), Some(vec![Some("a")]));
            assert_eq!(
                pattern("a|ab").full_match("ab").unwrap().unwrap().groups,
                vec![Some("ab")]
            );
            assert_eq!(captures("a+?", "aaa"), Some(vec![Some("a")]));
            assert_eq!(captures("a+", "aaa"), Some(vec![Some("aaa")]));
            assert_eq!(captures("a*", "bbb"), Some(vec![Some("")]));
            assert_eq!(captures("(a?)*", ""), Some(vec![Some(""), None]));
            assert_eq!(captures("(a?)+", ""), Some(vec![Some(""), Some("")]));
            assert_eq!(captures("a{2,3}?a", "aaaa"), Some(vec![Some("aaa")]));
            assert_eq!(captures("a{2,3}a", "aaaa"), Some(vec![Some("aaaa")]));
        }

        /// Repetition clears descendant capture registers; unset and empty remain distinct.
        #[test]
        fn backreferences_and_capture_lifecycle() {
            assert_eq!(
                captures(r"(a|(b))+", "aba"),
                Some(vec![Some("aba"), Some("a"), None])
            );
            assert_eq!(
                captures(r"^(a|(b))\2c$", "ac"),
                Some(vec![Some("ac"), Some("a"), None])
            );
            assert_eq!(captures(r"^\1(a)$", "a"), Some(vec![Some("a"), Some("a")]));
            assert_eq!(captures(r"^(a\1)$", "a"), Some(vec![Some("a"), Some("a")]));
            assert_eq!(
                captures(r"^(?<x>a)\k<x>$", "aa"),
                Some(vec![Some("aa"), Some("a")])
            );
            assert_eq!(captures(r"^(a)?\1$", ""), Some(vec![Some(""), None]));
            assert_eq!(captures(r"^(a*)\1$", ""), Some(vec![Some(""), Some("")]));
        }

        /// Positive assertions are atomic; lookbehind traverses a fixed-width sequence backwards.
        #[test]
        fn lookaround_order_and_isolation() {
            assert_eq!(
                captures(r"(?=(a+))a*b\1", "baaabac"),
                Some(vec![Some("aba"), Some("a")])
            );
            assert_eq!(
                captures(r"(?<=(a)(b))c", "abc"),
                Some(vec![Some("c"), Some("a"), Some("b")])
            );
            assert_eq!(captures(r"(?<=a|b)c", "ac"), Some(vec![Some("c")]));
            assert_eq!(captures(r"(?<!a)b", "ab"), None);
            assert_eq!(captures(r"(?<!a)b", "cb"), Some(vec![Some("b")]));
            assert_eq!(captures(r"(?!(a))\1b", "b"), Some(vec![Some("b"), None]));
            assert_eq!(
                captures(r"(?<=((a)|(b)){2})c", "abc"),
                Some(vec![Some("c"), Some("a"), Some("a"), None])
            );
        }

        /// Width admission follows capture participation and reverse evaluation order.
        #[test]
        fn lookbehind_backreference_width_uses_capture_state() {
            for (source, subject, expected) in [
                (r"^(a)(?<=\1)b$", "ab", vec![Some("ab"), Some("a")]),
                (r"^(?<x>a)(?<=\k<x>)b$", "ab", vec![Some("ab"), Some("a")]),
                (r"(?<=\1(a))b", "aab", vec![Some("b"), Some("a")]),
                (r"(?<=(a)\1)b", "ab", vec![Some("b"), Some("a")]),
                (r"^(?<=\1)(a*)$", "aaa", vec![Some("aaa"), Some("aaa")]),
                (r"^()(?:a)(?<=\1+)b$", "ab", vec![Some("ab"), Some("")]),
                (r"^(?=(a))a(?<=\1)b$", "ab", vec![Some("ab"), Some("a")]),
                (r"^(?!(a))(?<=\1)b$", "b", vec![Some("b"), None]),
            ] {
                assert_eq!(captures(source, subject), Some(expected), "{source}");
            }
            // Alternative capture states must stay correlated: either pair totals three.
            assert_eq!(
                captures(r"^(?:(a)(bc)|(ab)(c))(?<=\1\2\3\4)d$", "abcd"),
                Some(vec![Some("abcd"), Some("a"), Some("bc"), None, None])
            );
        }

        /// Prior variable/optional captures can make an otherwise plain reference vary.
        #[test]
        fn lookbehind_backreference_width_rejects_proven_variation() {
            for source in [
                r"(a+)(?<=\1)b",
                r"(a)?(?<=\1)b",
                r"(?:a|(b))(?<=\1)c",
                r"(a)(?<=\1+)b",
            ] {
                assert!(
                    matches!(
                        Pattern::compile(source),
                        Err(CompileError::Invalid {
                            reason: "variable-length lookbehind",
                            ..
                        })
                    ),
                    "{source}"
                );
            }
        }

        /// Final repeat captures belong to one cleared iteration, in either direction.
        #[test]
        fn lookbehind_width_preserves_repeat_capture_lifecycle() {
            for (source, subject, expected) in [
                (
                    r"^(?:(a)|(b)){2}(?<=\1\2)c$",
                    "abc",
                    vec![Some("abc"), None, Some("b")],
                ),
                (r"^(?:(a)(?<=\1)){2}b$", "aab", vec![Some("aab"), Some("a")]),
                (r"(?<=(?:\1(a)){2})b", "aaaab", vec![Some("b"), Some("a")]),
                (r"(?<=(?:(a)\1){2})b", "aab", vec![Some("b"), Some("a")]),
                (
                    r"^((?:a|b){2})(?<=\1)c$",
                    "abc",
                    vec![Some("abc"), Some("ab")],
                ),
                (r"^(a){1,3}(?<=\1)b$", "aaab", vec![Some("aaab"), Some("a")]),
                (r"^(){0,3}(?<=\1)b$", "b", vec![Some("b"), None]),
                (r"^(){2,3}(?<=\1)b$", "b", vec![Some("b"), Some("")]),
                (r"^(a){0}(?<=\1)b$", "b", vec![Some("b"), None]),
            ] {
                assert_eq!(captures(source, subject), Some(expected), "{source}");
            }
            for source in [
                r"(a|bb){2}(?<=\1)c",
                r"((?:a|bb){2})(?<=\1)c",
                r"(a){0,2}(?<=\1)b",
                r"(a?){1,2}(?<=\1)b",
                r"(?<=(?:\1(a)){1,2})b",
            ] {
                assert!(
                    matches!(
                        Pattern::compile(source),
                        Err(CompileError::Invalid {
                            reason: "variable-length lookbehind",
                            ..
                        })
                    ),
                    "{source}"
                );
            }
        }

        /// References can compensate unequal literal alternatives in reverse traversal.
        #[test]
        fn lookbehind_width_preserves_reverse_alternative_correlation() {
            let source = r"(?<=\1\2(?:(a)bb|(aa)))c";
            assert_eq!(
                captures(source, "aabbc"),
                Some(vec![Some("c"), Some("a"), None])
            );
            assert_eq!(
                captures(source, "aaaac"),
                Some(vec![Some("c"), None, Some("aa")])
            );
            assert_eq!(captures(source, "aaac"), None);
            // A reference outside its own open capture is unset until that capture closes.
            assert_eq!(
                captures(r"^(\1a)(?<=\1)b$", "ab"),
                Some(vec![Some("ab"), Some("a")])
            );
            assert_eq!(
                captures(r"^(\2a)(\1b)(?<=\1\2)c$", "aabc"),
                Some(vec![Some("aabc"), Some("a"), Some("ab")])
            );
        }

        /// Assertion state commits only on positive assertions; syntax checks remain structural.
        #[test]
        fn lookbehind_width_validates_nested_and_unexecuted_assertions() {
            assert_eq!(
                captures(r"^(a)(?<!(?<=\1)b)c$", "ac"),
                Some(vec![Some("ac"), Some("a")])
            );
            assert_eq!(
                captures(r"^(?!(a+))(?<=\1)b$", "b"),
                Some(vec![Some("b"), None])
            );
            assert_eq!(
                captures(r"^(?=(a))(?<=(?=\1))a$", "a"),
                Some(vec![Some("a"), Some("a")])
            );
            for source in [
                r"(?:(?<=a+)){0}(a)\1",
                r"(?!(?<=a+))(a)\1",
                r"(?<=(?=a+)(?<=b+))(a)\1",
                r"(?=(a+))(?<=\1)b",
                r"(?:(a+)(?<=\1)){0}",
            ] {
                assert!(
                    matches!(
                        Pattern::compile(source),
                        Err(CompileError::Invalid {
                            reason: "variable-length lookbehind",
                            ..
                        })
                    ),
                    "{source}"
                );
            }
            assert_eq!(
                Pattern::compile("\u{e9}(a+)(?<=\\1)b").unwrap_err(),
                CompileError::Invalid {
                    byte: 6,
                    reason: "variable-length lookbehind"
                }
            );
        }

        /// Fixed repetition widths are calculated symbolically.
        #[test]
        fn fixed_lookbehind_counts_are_symbolic_without_artificial_width_ceilings() {
            for source in [
                r"(a)(?<=\1)b",
                r"(a){1000000}(?<=\1)",
                r"(a)(?<=\1{1000000})",
                r"(aa)(?<=\1{1000000})",
            ] {
                assert!(Pattern::compile(source).is_ok(), "{source}");
            }
            let flat = "a".repeat(3000) + r"(a)(?<=\1)";
            assert!(Pattern::compile(&flat).is_ok());
        }

        /// An unrelated reference must not make a statically fixed assertion need analysis.
        #[test]
        fn fixed_lookbehind_does_not_analyze_unrelated_capture_paths() {
            let choices = "(?:(a)|b)".repeat(16) + r"(?<=c)\1";
            for source in [&choices, r"(a{1000})\1{2000}(?<=b)", r"(a)(?<=(?=\1))a"] {
                assert!(Pattern::compile(source).is_ok(), "{source}");
            }
            // A fixed outer assertion must not hide a dependent nested assertion.
            assert!(matches!(
                Pattern::compile(r"(a+)(?<=(?<=\1))b"),
                Err(CompileError::Invalid {
                    reason: "variable-length lookbehind",
                    ..
                })
            ));
        }

        /// Integer count truth distinguishes a final capture from a whole repeated group.
        #[test]
        fn lookbehind_width_agrees_with_independent_count_truth() {
            let assert_fixed = |source: &str, fixed: bool| {
                let compiled = Pattern::compile(source);
                if fixed {
                    assert!(compiled.is_ok(), "{source}: {compiled:?}");
                } else {
                    assert!(
                        matches!(
                            compiled,
                            Err(CompileError::Invalid {
                                reason: "variable-length lookbehind",
                                ..
                            })
                        ),
                        "{source}: {compiled:?}"
                    );
                }
            };
            for width in 0..5 {
                for min in 0..5 {
                    for max in min..5 {
                        assert_fixed(
                            &format!(r"(a{{{width}}}){{{min},{max}}}(?<=\1)"),
                            width == 0 || min > 0 || max == 0,
                        );
                        assert_fixed(
                            &format!(r"((?:a{{{width}}}){{{min},{max}}})(?<=\1)"),
                            width == 0 || min == max,
                        );
                    }
                }
            }
            for left in 0..5 {
                for right in 0..5 {
                    for count in 0..4 {
                        let fixed = count == 0 || left == right;
                        assert_fixed(
                            &format!(r"(?:(a{{{left}}})|(a{{{right}}})){{{count}}}(?<=\1\2)"),
                            fixed,
                        );
                        assert_fixed(
                            &format!(r"(?<=(?:\1\2(?:(a{{{left}}})|(a{{{right}}}))){{{count}}})x"),
                            fixed,
                        );
                    }
                }
            }
        }

        /// Scalar classes have the repository's exact whitespace/ASCII rules in both polarities.
        #[test]
        fn unicode_classes_and_escape_grammar() {
            for c in [
                '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}',
                '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
            ] {
                let s = c.to_string();
                assert!(captures(r"^[\s]$", &s).is_some());
                assert!(captures(r"^[\S]$", &s).is_none());
                assert!(captures(r"^[^\S]$", &s).is_some());
            }
            assert!(captures(r"^\s$", "\u{85}").is_none());
            assert!(captures(r"^[\S]$", "\u{85}").is_some());
            assert!(captures(r"^\d$", "\u{665}").is_none());
            assert!(captures(r"^\w$", "\u{e9}").is_none());
            assert_eq!(
                captures(r"^\u{1D400}$", "\u{1d400}"),
                Some(vec![Some("\u{1d400}")])
            );
            assert_eq!(
                captures(r"^\uD835\uDC00$", "\u{1d400}"),
                Some(vec![Some("\u{1d400}")])
            );
            assert_eq!(captures(r"^\cA$", "\u{1}"), Some(vec![Some("\u{1}")]));
            assert_eq!(captures(r"^\0$", "\0"), Some(vec![Some("\0")]));
            assert_eq!(captures(r"[]", "x"), None);
            assert_eq!(captures(r"[^]", "\n"), Some(vec![Some("\n")]));
            assert_eq!(captures(r"\B", ""), Some(vec![Some("")]));
            assert_eq!(captures(r"a$", "a\n"), None);
        }

        /// Invalid grammar never becomes ordinary no-match or a silently selected host dialect.
        #[test]
        fn invalid_patterns() {
            for text in [
                r"\01",
                r"\a",
                r"\!",
                r"[\B]",
                r"[\d-a]",
                r"[z-a]",
                r"a{2,1}",
                r"(?P<x>a)",
                r"(?i)a",
                r"\p{L}",
                r"(?<=a+)b",
                r"(?<=a|bc)d",
                r"a++",
                r"a???",
                r"(?=a)*",
                r"(?<x>a)(?<x>b)",
                r"\2(a)",
                r"\k<absent>",
                "(",
                "[",
                r"\",
            ] {
                assert!(
                    matches!(Pattern::compile(text), Err(CompileError::Invalid { .. })),
                    "{text}"
                );
            }
        }

        /// Names use decoded ID properties, not host identifiers, XID or normalization.
        #[test]
        fn unicode_group_names_preserve_decoded_identity() {
            for name in [
                "x",
                "$",
                "_",
                "\u{e9}",
                "\u{37a}",
                "\u{1d400}",
                "a\u{301}",
                "a\u{200c}\u{200d}",
                "a\u{30fb}\u{ff65}",
                "a0",
            ] {
                let escaped: String = name
                    .chars()
                    .map(|c| format!(r"\u{{{:X}}}", c as u32))
                    .collect();
                for (definition, reference) in [(name, escaped.as_str()), (escaped.as_str(), name)]
                {
                    assert_eq!(
                        captures(&format!(r"^(?<{definition}>a)\k<{reference}>$"), "aa"),
                        Some(vec![Some("aa"), Some("a")])
                    );
                    assert_eq!(
                        captures(&format!(r"^\k<{reference}>(?<{definition}>a)$"), "a"),
                        Some(vec![Some("a"), Some("a")])
                    );
                }
            }
            assert_eq!(
                captures(r"^(?<\uD835\uDC00>a)\k<\u{1D400}>$", "aa"),
                Some(vec![Some("aa"), Some("a")])
            );
            assert_eq!(
                captures(
                    "^(?<\u{e9}>a)(?<e\u{301}>b)\\k<\u{e9}>\\k<e\u{301}>$",
                    "abab"
                ),
                Some(vec![Some("abab"), Some("a"), Some("b")])
            );
        }

        /// Invalid name scalars and escapes own syntax errors at their source positions.
        #[test]
        fn unicode_group_names_reject_invalid_positions_and_aliases() {
            for name in [
                "",
                "0x",
                "\u{301}a",
                "\u{200c}",
                "\u{1f600}",
                "a-",
                r"\x61",
                r"\cA",
                r"\uD800",
                r"\uDC00",
                r"\u{D800}",
                r"\u{110000}",
                r"\u{}",
                r"\u12",
                r"\u0030x",
            ] {
                for source in [format!("(?<{name}>a)"), format!(r"\k<{name}>(?<x>a)")] {
                    assert!(
                        matches!(Pattern::compile(&source), Err(CompileError::Invalid { .. })),
                        "{source}"
                    );
                }
            }
            for source in [r"(?<x>a)(?<\u0078>b)", "(?<\u{e9}>a)(?<\\u00E9>b)"] {
                assert!(matches!(
                    Pattern::compile(source),
                    Err(CompileError::Invalid {
                        reason: "duplicate group name",
                        ..
                    })
                ));
            }
            assert_eq!(
                Pattern::compile("(?<\u{e9}\\u0030->a)").unwrap_err(),
                CompileError::Invalid {
                    byte: 11,
                    reason: "invalid group name"
                }
            );
        }

        /// Compilation and matching enforce independent budgets; failure cannot poison the next run.
        #[test]
        fn fixed_depth_and_per_match_steps_refuse_only_their_real_boundaries() {
            for depth in [127, 128] {
                let source = "(".repeat(depth) + "a" + &")".repeat(depth);
                assert!(Pattern::compile(&source).is_ok());
            }
            let source = "(".repeat(129) + "a" + &")".repeat(129);
            assert!(matches!(
                Pattern::compile(&source),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    limit: 128
                })
            ));
            // Each literal candidate consumes one start and one interpretation step.
            let literal = pattern("z");
            assert_eq!(literal.search(&"a".repeat(499_998)), Ok(None));
            assert_eq!(literal.search(&"a".repeat(499_999)), Ok(None));
            assert_eq!(
                literal.search(&"a".repeat(500_000)),
                Err(MatchError {
                    resource: Resource::Work,
                    limit: 1_000_000
                })
            );
            let compiled = pattern("(a+)+$");
            assert_eq!(
                compiled.search(&("a".repeat(30) + "!")),
                Err(MatchError {
                    resource: Resource::Work,
                    limit: 1_000_000
                })
            );
            assert_eq!(
                compiled.search("a").unwrap().unwrap().groups,
                vec![Some("a"), Some("a")]
            );
        }

        /// Machine-width overflow is a capability refusal rather than a grammar finding.
        #[test]
        fn fixed_width_zero_repetition_and_addressable_range() {
            assert_eq!(captures(r"(?<=(?:a|bc){0})x", "x"), Some(vec![Some("x")]));
            assert!(Pattern::compile(r"(?:a{3}){3}").is_ok());
            let large = format!("(?:a{{{}}}){{2}}", usize::MAX);
            assert!(matches!(
                Pattern::compile(&large),
                Err(CompileError::Unsupported {
                    feature: "pattern width exceeds addressable range",
                    ..
                })
            ));
        }

        /// Flat patterns and nested assertions retain matching order without quota plumbing.
        #[test]
        fn large_flat_patterns_and_nested_assertions_keep_their_match_results() {
            let source = "a".repeat(3000);
            assert!(pattern(&source).full_match(&source).unwrap().is_some());
            let nested = "(?=".repeat(60) + "a" + &")".repeat(60) + "a";
            assert_eq!(captures(&nested, "a"), Some(vec![Some("a")]));
        }

        /// Matching steps count interpretation rather than the allocation of subject indexes.
        #[test]
        fn long_subjects_are_not_charged_as_matching_steps() {
            let subject = "a".repeat(1_048_577);
            assert_eq!(
                pattern("a").search(&subject).unwrap().unwrap().groups,
                vec![Some("a")]
            );
        }

        /// REQ-0825 expands braced escapes to REQ-0022 scalars, excluding surrogates.
        #[test]
        fn braced_code_point_expansion_requires_a_scalar() {
            for value in 0xd800..=0xdfff {
                for source in [format!(r"\u{{{value:X}}}"), format!(r"[\u{{{value:x}}}]")] {
                    assert!(
                        matches!(Pattern::compile(&source), Err(CompileError::Invalid { .. })),
                        "{source}"
                    );
                }
            }
            // The neighboring scalars, unassigned scalars and U+10FFFF stay valid.
            for value in [0xd7ff, 0xe000, 0x378, 0x10ffff] {
                let source = format!(r"^\u{{{value:X}}}$");
                let subject = char::from_u32(value).unwrap().to_string();
                assert_eq!(
                    captures(&source, &subject),
                    Some(vec![Some(subject.as_str())])
                );
            }
        }

        /// Known variable consumption stays invalid even when a backreference is also present.
        #[test]
        fn lookbehind_width_distinguishes_variable_from_backreference_dependent() {
            for source in [
                r"(a)(?<=a+\1)b",
                r"(a)(?<=\1a+)b",
                r"(a)(?<=a+|\1)b",
                r"(a)(?<=\1|a+)b",
                r"(a)(?<=a|\1|bb)b",
                r"(a)(?<=(?=\1)a+)b",
                r"(a)(?<=(?:a+\1){2})b",
                r"(a)(?<=(?:b\1)+)c",
            ] {
                assert!(
                    matches!(Pattern::compile(source), Err(CompileError::Invalid { .. })),
                    "{source}"
                );
            }
            // A backreference can be empty, so repetition alone does not prove variability.
            for source in [r"(a)(?<=\1)b", r"()(?<=\1+)b", r"()(?<=\1{1,2})b"] {
                assert!(Pattern::compile(source).is_ok(), "{source}");
            }
            // Zero repetitions and zero-width assertions do not consume their descendants.
            assert_eq!(
                captures(r"(a)(?<=(?:a+\1){0})b", "ab"),
                Some(vec![Some("ab"), Some("a")])
            );
            assert_eq!(
                captures(r"(?<=((?=a))*?)a", "a"),
                Some(vec![Some("a"), None])
            );
            assert_eq!(
                captures(r"(a)(?<=(?=\1))a", "aa"),
                Some(vec![Some("aa"), Some("a")])
            );
        }
    }
}
