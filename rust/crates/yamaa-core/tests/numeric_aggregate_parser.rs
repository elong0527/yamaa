//! Expression contracts: one grammar in its three contexts, its evaluation,
//! and the function signatures and portable patterns it relies on.
mod numeric {
    mod aggregate_grammar {
        use yamaa_core::expression::{
            compile, CompileError, CompiledExpression, Context, Failure, NodeKind, Reducer,
            ReductionOperand, Resource,
        };
        use yamaa_core::limits::Limits;

        fn aggregate(text: &str) -> Result<CompiledExpression, CompileError> {
            compile(text, Context::Aggregate, "rows[0].aggregate.expr")
        }

        /// Read the structured finding without relying on diagnostic prose.
        fn failure(text: &str) -> (usize, usize, Failure) {
            match aggregate(text).unwrap_err() {
                CompileError::Invalid {
                    position, failure, ..
                } => (position.byte, position.character, failure),
                error => panic!("unexpected limit: {error:?}"),
            }
        }

        /// Distinguish fieldless record dependencies and ungrouped fields in written order.
        #[test]
        fn references_and_spans_preserve_written_context() {
            let text = "\u{2003}NULLIF(sUm( D.A + D.B ), COUNT(D.*)) + D.A + X + COUNT(E.*) + X";
            let compiled = aggregate(text).unwrap();
            let parsed = compiled.syntax();
            assert_eq!(compiled.expression(), text);
            assert_eq!(
                compiled.identifiers().collect::<Vec<_>>(),
                ["D.A", "D.B", "X"]
            );
            assert_eq!(
                compiled.ungrouped_identifiers().collect::<Vec<_>>(),
                ["D.A", "X"]
            );
            assert_eq!(compiled.record_relations().collect::<Vec<_>>(), ["D", "E"]);
            let mut reductions = Vec::new();
            for (id, node) in parsed.nodes().iter().enumerate() {
                let mut children = Vec::new();
                match &node.kind {
                    NodeKind::Reduction {
                        reducer,
                        name,
                        operand,
                    } => {
                        reductions.push((*reducer, parsed.text(id), parsed.slice(*name)));
                        children.push(*operand);
                    }
                    NodeKind::Group(operand) | NodeKind::Unary { operand, .. } => {
                        children.push(*operand)
                    }
                    NodeKind::Binary { left, right, .. } => children.extend([*left, *right]),
                    NodeKind::Call { arguments, .. } => children.extend(arguments),
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
            let operands: Vec<_> = compiled
                .reductions()
                .iter()
                .map(|reduction| (reduction.reference.as_str(), reduction.operand.clone()))
                .collect();
            assert_eq!(
                operands,
                [
                    ("sUm( D.A + D.B )", ReductionOperand::Numeric),
                    ("COUNT(D.*)", ReductionOperand::Records("D".into())),
                    ("COUNT(E.*)", ReductionOperand::Records("E".into()))
                ]
            );
        }

        /// Lexing and syntax precede the binder's construct, call and reduction checks.
        #[test]
        fn diagnostics_have_expression_requirements_and_exact_priority() {
            for (text, byte, condition, rule) in [
                ("SUM(A", 5, "invalid_expression", "FORMULA-1"),
                ("SUM(A,B)", 5, "invalid_expression", "FORMULA-1"),
                ("SUM(D.*)", 4, "invalid_expression", "FORMULA-1"),
                ("COUNT(*)", 6, "invalid_expression", "FORMULA-1"),
                ("COUNT((D.*))", 7, "invalid_expression", "FORMULA-1"),
                ("COUNT(D.*,A)", 9, "invalid_expression", "FORMULA-1"),
                ("D.*", 0, "invalid_expression", "FORMULA-1"),
                ("SUM(MIN(A))", 0, "nested_reduction", "FORMULA-21"),
                ("SUM(MIN(A)", 10, "invalid_expression", "FORMULA-1"),
                ("SUM(MIN(A)) > X", 12, "prohibited_construct", "FORMULA-27"),
                ("SUM(ABS())", 4, "prohibited_function", "FORMULA-13"),
                // The doubled sign is outside the grammar, so syntax is reported
                // before the context refuses the later AND.
                ("--A AND B", 1, "invalid_expression", "FORMULA-1"),
                ("AVG(A)", 0, "prohibited_function", "FORMULA-13"),
                ("AVG(A", 5, "invalid_expression", "FORMULA-1"),
                ("COUNT(D .*)", 8, "invalid_expression", "FORMULA-1"),
            ] {
                let (actual, _, failure) = failure(text);
                assert_eq!(
                    (actual, failure.condition(), failure.rule()),
                    (byte, condition, rule),
                    "{text}"
                );
            }
            assert_eq!(
                failure("\u{2003}SUM(NULLIF(MIN(A), MAX(B)))"),
                (
                    3,
                    1,
                    Failure::NestedReduction {
                        outer: Reducer::Sum,
                        inner: Reducer::Min
                    }
                )
            );
            assert_eq!(
                failure("SUM(MAX(MIN(A)))").2,
                Failure::NestedReduction {
                    outer: Reducer::Max,
                    inner: Reducer::Min
                }
            );
            let (_, _, error) = failure("abS(1,2)");
            assert!(matches!(
                error,
                Failure::ProhibitedFunction {
                    argument_count: Some(2),
                    ..
                }
            ));
        }

        /// The integer carrier is checked once the whole text parses; a float
        /// literal, missing and names are never evaluated here.
        #[test]
        fn integer_literals_are_checked_after_the_whole_grammar() {
            for text in [
                "SUM(1e9999)",
                "NULL",
                "SUM + COUNT",
                "COUNT(TRUE.*)",
                "SUM(A.NULL)",
                "ONLY(NULL)",
            ] {
                assert!(aggregate(text).is_ok(), "{text}");
            }
            let long = format!("SUM({})", "9".repeat(5000));
            for text in ["SUM(0009223372036854775808)", long.as_str()] {
                assert_eq!(failure(text), (4, 4, Failure::IntegerOverflow), "{text}");
            }
            assert_eq!(failure("SUM(9223372036854775808").2, Failure::Syntax);
        }

        /// Fixed depth protects recursive descent and retained expression trees.
        #[test]
        fn fixed_semantic_depth_guards_aggregate_trees_without_a_breadth_counter() {
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                let text = vec!["SUM(A)"; depth - 1].join("+");
                let result = aggregate(&text);
                if depth <= Limits::SEMANTIC_DEPTH {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(
                        result,
                        Err(CompileError::Limit {
                            resource: Resource::Depth,
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
                failure(&text).2,
                Failure::ProhibitedFunction {
                    argument_count: Some(3000),
                    ..
                }
            ));
            let text = format!("{}A{}", "(".repeat(10000), ")".repeat(10000));
            assert!(matches!(
                aggregate(&text),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    ..
                })
            ));
        }
    }
    mod bound_expression {
        use std::convert::Infallible;
        use yamaa_core::{
            bound_expression::{Binding, BindingError, BoundNumeric, BoundPredicate, Read},
            expression::{compile, CompileError, Context, Failure, Resolver},
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
                compile(text, Context::Derivation, "columns.RESULT").unwrap(),
                bindings,
            )
        }

        fn predicate(path: &str, bindings: Vec<Binding>) -> Result<BoundPredicate, BindingError> {
            BoundPredicate::new(
                compile("FALSE AND A IS NULL", Context::Predicate, path).unwrap(),
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
            assert_eq!(admitted.plan().spec_path(), "rows[0].filter");
        }

        #[derive(Default)]
        struct Reads(Vec<String>);
        impl Resolver for Reads {
            type Error = Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.0.push(name.into());
                Ok(Selection::Present(Value::Int(1)))
            }
        }

        #[test]
        fn immutable_admission_preserves_deferred_failures_and_occurrences() {
            for text in ["A + A + 1 / 0", "A + A + 9223372036854775807 * 2"] {
                let admitted = numeric(text, vec![binding("A", Read::Source(0))]).unwrap();
                for _ in 0..2 {
                    let mut reads = Reads::default();
                    let error = admitted
                        .expression()
                        .evaluate_number(&mut reads)
                        .unwrap_err();
                    assert_eq!(reads.0, ["A", "A"]);
                    assert_eq!(error.expression, text);
                    assert_eq!(admitted.expression().spec_path(), "columns.RESULT");
                }
            }
            // An integer literal outside int64 is refused before any binding.
            assert!(matches!(
                compile(
                    "A + A + 9223372036854775808",
                    Context::Derivation,
                    "columns.RESULT"
                ),
                Err(CompileError::Invalid {
                    failure: Failure::IntegerOverflow,
                    ..
                })
            ));
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
    }
    mod compute_evaluation {
        use std::convert::Infallible;

        use yamaa_core::expression::{
            compile, CompileError, CompiledExpression, Context, ErrorKind, Failure, Resolver, Step,
        };
        use yamaa_core::numeric::Number;
        use yamaa_core::value::{Selection, Value};

        const PATH: &str = "columns.RESULT.derivation";

        /// Compile a test expression through the public source-to-plan entry point.
        fn compute(text: &str) -> CompiledExpression {
            compile(text, Context::Derivation, PATH).unwrap()
        }

        /// Record calls while changing A's value so plan reuse cannot hide memoization.
        #[derive(Default)]
        struct Reads {
            trace: Vec<String>,
        }
        impl Resolver for Reads {
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

        /// Replay exact types and bits, and the integer carrier, from independent truth.
        #[test]
        fn shared_literal_vectors_through_compilation() {
            for row in include_str!("fixtures/numeric_literals.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 3);
                let actual = match compile(fields[1], Context::Derivation, PATH) {
                    Ok(plan) => {
                        let mut reads = Reads::default();
                        let actual = match plan.evaluate_number(&mut reads).unwrap() {
                            Number::Missing => "missing".into(),
                            Number::Int(value) => format!("int:{value}"),
                            Number::Float(value) => {
                                format!("float:{:016x}", value.get().to_bits())
                            }
                        };
                        assert!(reads.trace.is_empty());
                        actual
                    }
                    Err(CompileError::Invalid {
                        position, failure, ..
                    }) => {
                        // The refusal cites the literal, a written sign included.
                        assert_eq!(position.byte, 0, "{}", fields[0]);
                        assert_eq!(failure.diagnostic_code().definition().phase, "validation");
                        format!("error:{}:{}", failure.condition(), failure.rule())
                    }
                    Err(error) => panic!("{}: {error:?}", fields[0]),
                };
                assert_eq!(actual, fields[2], "{}", fields[0]);
            }
        }

        /// An integer literal outside int64 is refused before any name is resolved,
        /// wherever it is written, once the whole text parses.
        #[test]
        fn oversized_literals_are_refused_before_evaluation() {
            let digits = "9".repeat(5000);
            for text in [
                format!("A + {digits} + B"),
                format!("M + {digits} + B"),
                format!("{digits} + A"),
                format!("UNKNOWN + {digits}"),
                format!("1 / 0 + {digits}"),
                format!("NULLIF(A, {digits})"),
            ] {
                let Err(CompileError::Invalid {
                    position, failure, ..
                }) = compile(&text, Context::Derivation, PATH)
                else {
                    panic!("an overflowing literal must be refused: {text}")
                };
                assert_eq!(failure, Failure::IntegerOverflow);
                assert!(text[position.byte..].starts_with(&digits));
            }
            let Err(CompileError::Invalid { failure, .. }) =
                compile(&format!("{digits} +"), Context::Derivation, PATH)
            else {
                panic!("an incomplete text must be refused")
            };
            assert_eq!(failure, Failure::Syntax);
        }

        /// Supported plans retain exact semantic spans even under redundant groups/Unicode.
        #[test]
        fn source_spans_identify_the_failed_node() {
            for (text, expected, route) in [
                (
                    "\u{2003}M + ((UNKNOWN))",
                    "UNKNOWN",
                    vec![Step::RightOperand],
                ),
                ("M + ((1 / 0))", "1 / 0", vec![Step::RightOperand]),
                (
                    "-((9223372036854775807 * 2))",
                    "9223372036854775807 * 2",
                    vec![Step::Unary],
                ),
                ("ABS((1 / 0))", "1 / 0", vec![Step::Unary]),
                ("SQRT((UNKNOWN))", "UNKNOWN", vec![Step::Unary]),
                ("SQRT((1 / 0))", "1 / 0", vec![Step::Unary]),
                ("(SQRT(-1))", "SQRT(-1)", vec![]),
                ("CEIL((UNKNOWN))", "UNKNOWN", vec![Step::Unary]),
                ("FLOOR((1 / 0))", "1 / 0", vec![Step::Unary]),
                (
                    "TRUNC((9223372036854775807 + 1))",
                    "9223372036854775807 + 1",
                    vec![Step::Unary],
                ),
                ("MOD(A, (UNKNOWN))", "UNKNOWN", vec![Step::RightOperand]),
                ("(MOD(A, 0))", "MOD(A, 0)", vec![]),
            ] {
                let plan = compute(text);
                let error = plan.evaluate_number(&mut Reads::default()).unwrap_err();
                let site = error.source_span.expect("a numeric failure names its node");
                assert_eq!(&text[site.start..site.end], expected);
                assert_eq!(error.route, route);
                assert_eq!(error.spec_path, plan.spec_path());
                assert_eq!(error.expression, plan.expression());
            }
        }

        /// Prohibited function spellings are refused before resolution.
        #[test]
        fn prohibited_functions_are_rejected_before_resolution() {
            for text in ["SQRT(A) + ROUND(B)", "A + ROUND(B)"] {
                assert!(matches!(
                    compile(text, Context::Derivation, "spec"),
                    Err(CompileError::Invalid {
                        failure: Failure::ProhibitedFunction { .. },
                        ..
                    })
                ));
            }
        }

        /// Retired calls stay refused, with a replacement in the public finding.
        #[test]
        fn retired_formula_functions_name_their_replacement() {
            use yamaa_core::json::Value as V;

            for (call, function, hint) in [
                (
                    "cOaLeScE(A, B)",
                    "cOaLeScE",
                    "use the first_available expression",
                ),
                ("GREATEST(A, B)", "GREATEST", "use the greatest expression"),
                ("least(A, B)", "least", "use the least expression"),
                (
                    "ROUND(A, 2)",
                    "ROUND",
                    "use the round_half_away_from_zero operation",
                ),
                (
                    "round_half_away_from_zero(A, 2)",
                    "round_half_away_from_zero",
                    "use the round_half_away_from_zero operation",
                ),
            ] {
                for field in [Context::Derivation, Context::Aggregate, Context::Predicate] {
                    let expression = if field == Context::Predicate {
                        format!("{call} > 0")
                    } else {
                        format!("1 + {call}")
                    };
                    let Err(CompileError::Invalid {
                        position, failure, ..
                    }) = compile(&expression, field, "columns.V.derivation")
                    else {
                        panic!("retired call must fail before resolving A or B: {expression}")
                    };
                    let finding = failure.diagnostic(
                        field,
                        "columns.V.derivation",
                        &expression,
                        position.character,
                    );
                    assert_eq!(finding.definition().condition, "prohibited_function");
                    assert_eq!(finding.definition().rule, Some("FORMULA-13"));
                    assert_eq!(finding.spec_paths, ["columns.V.derivation"]);
                    assert_eq!(finding.context["function"], V::String(function.into()));
                    assert_eq!(finding.context["hint"], V::String(hint.into()));
                    assert_eq!(
                        failure.syntax_diagnostic().context["hint"],
                        V::String(hint.into())
                    );
                    assert_eq!(position.byte, expression.find(function).unwrap());
                    assert!(!finding.context.contains_key("argument_count"));
                }
            }
            for expression in ["mystery(A)", "ABS(A, B)", "SUM(A)"] {
                let Err(CompileError::Invalid {
                    position, failure, ..
                }) = compile(expression, Context::Derivation, "columns.V.derivation")
                else {
                    panic!("invalid call must still fail: {expression}")
                };
                let finding = failure.diagnostic(
                    Context::Derivation,
                    "columns.V.derivation",
                    expression,
                    position.character,
                );
                assert_eq!(finding.definition().condition, "prohibited_function");
                assert!(!finding.context.contains_key("hint"));
            }
        }

        /// Plan reuse resolves each original occurrence again.
        #[test]
        fn plan_reuse_resolves_every_occurrence_again() {
            let plan = compute("A + A");
            let mut reads = Reads::default();
            assert_eq!(plan.evaluate_number(&mut reads).unwrap(), Number::Int(3));
            assert_eq!(plan.evaluate_number(&mut reads).unwrap(), Number::Int(7));
            assert_eq!(reads.trace, ["A", "A", "A", "A"]);
        }

        /// The compiled plan does not clone, stringify or reclassify resolver failures.
        #[test]
        fn opaque_non_clone_resolution_failure_keeps_span_and_payload() {
            #[derive(Debug, PartialEq)]
            struct BoundaryError(Box<u8>);
            struct FailedResolver;
            impl Resolver for FailedResolver {
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
                let error = compute(text)
                    .evaluate_number(&mut FailedResolver)
                    .unwrap_err();
                let site = error.source_span.expect("a numeric failure names its node");
                assert_eq!(&text[site.start..site.end], "BROKEN");
                assert_eq!(
                    error.kind,
                    ErrorKind::Resolution {
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

                let plan = compute(&text);
                let mut reads = Reads::default();
                assert_eq!(plan.evaluate_number(&mut reads), Ok(Number::float(1.0)));
                assert_eq!(
                    plan.evaluate_number(&mut reads),
                    Ok(Number::float(if name == "SQRT" {
                        2.0_f64.sqrt()
                    } else {
                        2.0
                    }))
                );
                assert_eq!(reads.trace, ["A", "A"]);
            }
        }
    }
    mod compute_grammar {
        use yamaa_core::expression::{
            compile, CompileError, CompiledExpression, Construct, Context, Failure, NodeKind,
            Resource, SourcePosition,
        };
        use yamaa_core::limits::Limits;

        fn compute(text: &str) -> Result<CompiledExpression, CompileError> {
            compile(text, Context::Derivation, "columns.RESULT.derivation")
        }

        /// Read language findings independently of their human-readable formatting.
        fn failure(text: &str) -> (SourcePosition, Failure) {
            match compute(text).unwrap_err() {
                CompileError::Invalid {
                    position, failure, ..
                } => (position, failure),
                other => panic!("expected a language finding, got {other:?}"),
            }
        }

        /// Group spans must not overwrite the exact identifier or function-name extent.
        #[test]
        fn spans_and_identifier_order_preserve_original_text() {
            let text = "\u{2003}-(a + ABS(B.x)) + a";
            let compiled = compute(text).unwrap();
            let parsed = compiled.syntax();
            assert_eq!(parsed.expression(), text);
            assert_eq!(compiled.identifiers().collect::<Vec<_>>(), ["a", "B.x"]);
            let mut slices = Vec::new();
            for (index, node) in parsed.nodes().iter().enumerate() {
                slices.push(parsed.text(index));
                match &node.kind {
                    NodeKind::Group(operand) | NodeKind::Unary { operand, .. } => {
                        assert!(*operand < index);
                    }
                    NodeKind::Binary { left, right, .. } => {
                        assert!(*left < index && *right < index);
                    }
                    NodeKind::Call { name, arguments } => {
                        assert_eq!(parsed.slice(*name), "ABS");
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
            assert_eq!(kind.condition(), "invalid_expression");
            assert_eq!(kind.rule(), "FORMULA-1");
            // A reserved word is never a name, so a bare one is outside the grammar.
            let (position, kind) = failure("\u{2003}aNd");
            assert_eq!(
                position,
                SourcePosition {
                    byte: 3,
                    character: 1
                }
            );
            assert_eq!(kind, Failure::Syntax);
            // Boolean logic is grammatical; an expression field refuses it.
            let (position, kind) = failure("\u{2003}A aNd B");
            assert_eq!(
                position,
                SourcePosition {
                    byte: 5,
                    character: 3
                }
            );
            assert_eq!(
                kind,
                Failure::ProhibitedConstruct {
                    construct: Construct::Boolean
                }
            );
            assert_eq!(kind.rule(), "FORMULA-27");
            for (text, count) in [("abS(1, 2)", Some(2)), ("\u{2003}foo(1)", None)] {
                let (position, kind) = failure(text);
                assert_eq!(kind.condition(), "prohibited_function");
                assert_eq!(kind.rule(), "FORMULA-13");
                let Failure::ProhibitedFunction {
                    function,
                    argument_count,
                } = kind
                else {
                    panic!("expected function context")
                };
                // The written name starts at the finding's position.
                assert_eq!(function, text.trim_start().get(..3).unwrap());
                assert_eq!(
                    &text[position.byte..position.byte + function.len()],
                    function
                );
                assert_eq!(argument_count, count);
            }
        }

        /// Lexing precedes syntax, syntax precedes the binder, and call closure
        /// precedes function validation.
        #[test]
        fn failure_priority_matches_reference() {
            for (text, condition, byte) in [
                ("--A > B", "invalid_expression", 1),
                ("A > B", "prohibited_construct", 2),
                ("foo(1", "invalid_expression", 5),
                ("ABS(1,)", "invalid_expression", 6),
                ("foo(ABS())", "prohibited_function", 4),
                ("A.1", "invalid_expression", 0),
                ("A.B.C", "invalid_expression", 3),
                ("1e+", "invalid_expression", 1),
                ("--A", "invalid_expression", 1),
                // A keyword literal never names a function.
                ("NULL()", "invalid_expression", 4),
                (" ", "invalid_expression", 1),
            ] {
                let (position, kind) = failure(text);
                assert_eq!(
                    (kind.condition(), position.byte),
                    (condition, byte),
                    "{text}"
                );
            }
        }

        /// Literals keep their written text, a sign included. The integer carrier
        /// is checked once the text parses; a float converts only when evaluated.
        #[test]
        fn literals_keep_their_written_text() {
            for (text, fractional) in [
                ("1e9999", true),
                ("01.00E+2", true),
                ("-0009223372036854775808", false),
                ("+0009223372036854775807", false),
            ] {
                let compiled = compute(text).unwrap();
                let parsed = compiled.syntax();
                assert_eq!(
                    parsed.nodes()[parsed.root()].kind,
                    NodeKind::Number {
                        fractional,
                        text: text.into()
                    }
                );
                assert_eq!(parsed.expression(), text);
            }
            let long = "9".repeat(5000);
            for text in ["0009223372036854775808", long.as_str()] {
                assert_eq!(
                    failure(text),
                    (
                        SourcePosition {
                            byte: 0,
                            character: 0
                        },
                        Failure::IntegerOverflow
                    )
                );
            }
            for text in ["A.TRUE", "NULL.x", "_A._B", "A\u{1c}+\u{1f}B"] {
                assert!(compute(text).is_ok(), "{text:?}");
            }
        }

        /// Both recursion before node creation and left-associated tree depth are bounded.
        #[test]
        fn fixed_semantic_depth_guards_nesting_and_chains_without_a_breadth_counter() {
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                for text in [
                    vec!["A"; depth].join("+"),
                    format!("{}A{}", "(".repeat(depth - 1), ")".repeat(depth - 1)),
                ] {
                    let result = compute(&text);
                    if depth <= Limits::SEMANTIC_DEPTH {
                        assert!(result.is_ok(), "{text}");
                    } else {
                        assert!(matches!(
                            result,
                            Err(CompileError::Limit {
                                resource: Resource::Depth,
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
                failure(&call).1,
                Failure::ProhibitedFunction {
                    argument_count: Some(9000),
                    ..
                }
            ));
            assert!(matches!(
                compute(&"(".repeat(9000)),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    ..
                })
            ));
            assert!(matches!(
                compute(&" ".repeat(65_537)),
                Err(CompileError::Invalid { .. })
            ));
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
        use yamaa_core::expression::{
            compile, CompiledExpression, Condition, Context, ErrorKind, Resolver, Step,
        };
        use yamaa_core::numeric::{power, ArithmeticErrorKind, Number};
        use yamaa_core::value::{Selection, Value};

        fn compute(text: &str) -> CompiledExpression {
            compile(text, Context::Derivation, "spec").unwrap()
        }

        /// Record each resolution to verify compiler preflight and failure ordering.
        #[derive(Default)]
        struct Reads(Vec<String>);
        impl Resolver for Reads {
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

        /// The binder admits portable math before any resolver effects.
        #[test]
        fn current_numeric_vocabulary_compiles_directly() {
            for text in ["EXP(A)", "LN(A)", "POWER(A, A)"] {
                let plan = compute(text);
                let mut reads = Reads::default();
                assert!(matches!(
                    plan.evaluate_number(&mut reads),
                    Ok(Number::Float(_))
                ));
                assert_eq!(
                    reads.0,
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
                ("M + (LN(0))", "LN(0)", vec![Step::RightOperand], vec!["M"]),
                ("(POWER(-2, 0.5))", "POWER(-2, 0.5)", vec![], vec![]),
                (
                    "EXP((UNKNOWN))",
                    "UNKNOWN",
                    vec![Step::Unary],
                    vec!["UNKNOWN"],
                ),
                (
                    "POWER(M, UNKNOWN)",
                    "UNKNOWN",
                    vec![Step::RightOperand],
                    vec!["M", "UNKNOWN"],
                ),
                ("LN(0) + UNKNOWN", "LN(0)", vec![Step::LeftOperand], vec![]),
            ] {
                let plan = compute(text);
                let mut reads = Reads::default();
                let error = plan.evaluate_number(&mut reads).unwrap_err();
                let site = error.source_span.expect("a numeric failure names its node");
                assert_eq!(&text[site.start..site.end], span);
                assert_eq!(error.route, route);
                assert_eq!(reads.0, trace);
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
                assert_eq!(error.rule(), "FORMULA-15");
                assert_eq!(
                    error.kind,
                    ArithmeticErrorKind::InvalidPower {
                        base_bits: base.to_bits(),
                        exponent_bits: exponent.to_bits()
                    }
                );
            }
            let error = compute("POWER(-0.0, -1)")
                .evaluate_number(&mut Reads::default())
                .unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::Condition(Condition::Arithmetic(ArithmeticErrorKind::InvalidPower {
                    base_bits: (-0.0_f64).to_bits(),
                    exponent_bits: (-1.0_f64).to_bits()
                }))
            );
        }

        /// Portable math cannot discard a resolver's opaque failure even after a missing operand.
        #[test]
        fn portable_math_retains_opaque_resolver_failures() {
            #[derive(Debug, PartialEq, Eq)]
            struct Failure(Box<i32>);
            struct FailedResolver;
            impl Resolver for FailedResolver {
                type Error = Failure;
                /// Return an owned non-Clone payload to prove no diagnostic reconstruction occurs.
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    assert_eq!(name, "BROKEN");
                    Err(Failure(Box::new(42)))
                }
            }
            for text in ["EXP(BROKEN)", "LN(BROKEN)", "POWER(NULL, BROKEN)"] {
                let error = compute(text)
                    .evaluate_number(&mut FailedResolver)
                    .unwrap_err();
                let site = error.source_span.expect("a numeric failure names its node");
                assert_eq!(&text[site.start..site.end], "BROKEN");
                assert_eq!(
                    error.kind,
                    ErrorKind::Resolution {
                        identifier: "BROKEN".into(),
                        error: Failure(Box::new(42))
                    }
                );
            }
        }
    }
    mod predicate_context {
        //! Owned and borrowed resolution preserve independent truth and occurrence effects.
        use std::{collections::BTreeMap, convert::Infallible, rc::Rc};
        use yamaa_core::{
            expression::{
                compile, CompiledExpression, Condition, Context, ErrorKind, Resolved, Resolver,
                Step, Truth,
            },
            table::ValueRef,
            value::{Selection, Value, ValueType},
        };
        fn plan(text: &str, path: &str) -> CompiledExpression {
            compile(text, Context::Predicate, path).unwrap()
        }
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
                (
                    "a > 2 * b",
                    vec![("a", Value::Int(5)), ("b", Value::Int(2))],
                    Truth::True,
                    vec!["a", "b"],
                ),
            ] {
                let plan = plan(expr, "columns.F.flag.condition");
                let mut new = owned(&values);
                let mut old = Borrowed {
                    values: new.values.clone(),
                    trace: vec![],
                };
                assert_eq!(plan.evaluate_truth(&mut old).unwrap(), truth);
                assert_eq!(plan.evaluate_truth(&mut new).unwrap(), truth);
                assert_eq!(old.trace, names);
                assert_eq!(new.trace, names);
            }
        }
        #[test]
        fn long_subject_searches_complete_and_keep_eager_resolution() {
            let plan = plan("str_contains(a, 'z') AND b = 1", "flag.condition");
            // Half a million scalars, which the retired step budget refused.
            let mut port = owned(&[("a", Value::Str("a".repeat(500_000))), ("b", Value::Int(1))]);
            assert_eq!(plan.evaluate_truth(&mut port).unwrap(), Truth::False);
            assert_eq!(port.trace, ["a", "b"]);
            port.values.insert("a".into(), Value::Str("z".into()));
            port.trace.clear();
            assert_eq!(plan.evaluate_truth(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b"]);
        }
        #[test]
        fn data_driven_like_keeps_scalar_truth_and_eager_resolution() {
            let plan = plan("a LIKE b AND c = 1", "flag.condition");
            let mut port = owned(&[
                ("a", Value::Str("a".repeat(100))),
                ("b", Value::Str("%a".repeat(100) + "b")),
                ("c", Value::Int(1)),
            ]);
            assert_eq!(plan.evaluate_truth(&mut port).unwrap(), Truth::False);
            assert_eq!(port.trace, ["a", "b", "c"]);
            port.values.insert("a".into(), Value::Str("\n😀".into()));
            port.values.insert("b".into(), Value::Str("_%".into()));
            port.trace.clear();
            assert_eq!(plan.evaluate_truth(&mut port).unwrap(), Truth::True);
            assert_eq!(port.trace, ["a", "b", "c"]);
        }
        #[test]
        fn like_preserves_full_scalar_truth_without_regex_syntax_or_line_rules() {
            let positive = plan("a LIKE b", "flag.condition");
            let negated = plan("a NOT LIKE b", "flag.condition");
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
                    positive.evaluate_truth(&mut port).unwrap(),
                    expected,
                    "{value:?} LIKE {pattern:?}"
                );
                assert_eq!(port.trace, ["a", "b"]);
                let mut borrowed = Borrowed {
                    values: port.values,
                    trace: vec![],
                };
                assert_eq!(positive.evaluate_truth(&mut borrowed).unwrap(), expected);
                assert_eq!(borrowed.trace, ["a", "b"]);
                assert_eq!(
                    negated.evaluate_truth(&mut borrowed).unwrap(),
                    expected.negate()
                );
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
                    positive.evaluate_truth(&mut port).unwrap(),
                    Truth::True,
                    "{character:?}"
                );
            }
            for (value, pattern) in [
                ("a".repeat(3000), "a".repeat(3000)),
                ("\n😀".repeat(1000), "%".repeat(10_000)),
            ] {
                let mut port = owned(&[("a", Value::Str(value)), ("b", Value::Str(pattern))]);
                assert_eq!(positive.evaluate_truth(&mut port).unwrap(), Truth::True);
            }
        }
        #[test]
        fn like_validates_complete_escape_spelling_and_preserves_missing_and_type_owners() {
            for (expression, value, pattern) in [
                ("a LIKE b ESCAPE '!'", "%_!", "!%!_!!"),
                ("a LIKE b ESCAPE '😀'", "%_😀", "😀%😀_😀😀"),
                ("a LIKE b ESCAPE '%'", "_", "%_"),
                ("a LIKE b ESCAPE '_'", "%", "_%"),
            ] {
                let plan = plan(expression, "flag.condition");
                let mut port = owned(&[
                    ("a", Value::Str(value.into())),
                    ("b", Value::Str(pattern.into())),
                ]);
                assert_eq!(
                    plan.evaluate_truth(&mut port).unwrap(),
                    Truth::True,
                    "{expression}"
                );
            }
            let expression = "a LIKE b ESCAPE '!'";
            let escaped = plan(expression, "flag.condition");
            for (value, pattern) in [
                ("".into(), "!".into()),
                ("unrelated".into(), "z!".into()),
                ("a".repeat(100), "%a".repeat(100) + "!"),
            ] {
                let mut port = owned(&[("a", Value::Str(value)), ("b", Value::Str(pattern))]);
                let error = escaped.evaluate_truth(&mut port).unwrap_err();
                assert_eq!(error.kind, ErrorKind::Condition(Condition::DanglingEscape));
                assert_eq!(error.spec_path, "flag.condition");
                assert_eq!(error.expression, expression);
                assert!(error.route.is_empty());
                assert_eq!(port.trace, ["a", "b"]);
            }
            let negated = plan("a NOT LIKE b ESCAPE '!'", "flag.condition");
            for (value, pattern) in [
                (Value::Missing, Value::Str("!".into())),
                (Value::Str("a".into()), Value::Missing),
                (Value::Missing, Value::Int(1)),
                (Value::Int(1), Value::Missing),
            ] {
                let mut port = owned(&[("a", value), ("b", pattern)]);
                assert_eq!(escaped.evaluate_truth(&mut port).unwrap(), Truth::Unknown);
                assert_eq!(port.trace, ["a", "b"]);
                assert_eq!(negated.evaluate_truth(&mut port).unwrap(), Truth::Unknown);
            }
            for (value, pattern, actual) in [
                (Value::Int(1), Value::Str("%".into()), ValueType::Int),
                (Value::Str("a".into()), Value::Bool(true), ValueType::Bool),
            ] {
                let mut port = owned(&[("a", value), ("b", pattern)]);
                let error = escaped.evaluate_truth(&mut port).unwrap_err();
                assert_eq!(
                    error.kind,
                    ErrorKind::Condition(Condition::ExpectedText { actual })
                );
                assert_eq!(error.spec_path, "flag.condition");
                assert!(error.route.is_empty());
                assert_eq!(port.trace, ["a", "b"]);
            }
            let mut port = owned(&[("a", Value::Missing)]);
            let error = escaped.evaluate_truth(&mut port).unwrap_err();
            assert_eq!(error.route, [Step::Pattern]);
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
            let error = plan(expression, "rows[0].filter")
                .evaluate_truth(&mut port)
                .unwrap_err();
            assert_eq!(error.spec_path, "rows[0].filter");
            assert_eq!(error.expression, expression);
            assert_eq!(error.route, [Step::Left, Step::Pattern]);
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
            let plan = plan("a = 1 OR b = 2", "flag.condition");
            let marker = Rc::new(());
            let payload = Box::new(9);
            let address = &*payload as *const u8;
            let mut ctx = owned(&[]);
            ctx.fail = Some((payload, marker.clone()));
            let error = plan.evaluate_truth(&mut ctx).unwrap_err();
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
    mod predicate_evaluation {
        use std::{collections::BTreeMap, convert::Infallible};
        use yamaa_core::{
            expression::{
                compile, CompileError, CompiledExpression, Condition, Context, ErrorKind, Failure,
                Resolver, Step, Truth,
            },
            numeric::ArithmeticErrorKind,
            value::{Selection, Value, ValueType},
        };

        fn predicate(text: &str, path: &str) -> CompiledExpression {
            compile(text, Context::Predicate, path).unwrap()
        }

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
        fn port(values: &[(&str, Value)]) -> Port {
            Port {
                values: values
                    .iter()
                    .map(|(name, value)| ((*name).into(), value.clone()))
                    .collect(),
                ..Default::default()
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
                let plan = predicate(expression, "intermediates[0].filter");
                let mut port = port(&[
                    ("AE.AEOUT", Value::Str("FATAL".into())),
                    ("X", Value::Int(9007199254740993)),
                    ("M", Value::Missing),
                ]);
                assert_eq!(plan.spec_path(), "intermediates[0].filter");
                assert_eq!(
                    plan.evaluate_truth(&mut port).unwrap(),
                    truth,
                    "{expression}"
                );
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
                let plan = predicate(text, "site");
                let mut port = port(&[("X", Value::Int(1))]);
                assert_eq!(plan.evaluate_truth(&mut port).unwrap(), truth);
                assert_eq!(port.reads, reads);
                port.reads.clear();
                assert_eq!(plan.evaluate_truth(&mut port).unwrap(), truth);
                assert_eq!(port.reads, reads, "reusing a plan must repeat resolution");
            }
            for text in ["FALSE AND ABSENT = 1", "TRUE OR ABSENT = 1"] {
                let plan = predicate(text, "site");
                let mut port = Port::default();
                let error = plan.evaluate_truth(&mut port).unwrap_err();
                assert_eq!(port.reads, ["ABSENT"]);
                assert_eq!(error.spec_path, "site");
                assert_eq!(error.expression, text);
                assert!(matches!(
                    error.kind,
                    ErrorKind::Condition(Condition::UnknownField { .. })
                ));
            }
        }

        /// Arithmetic in a comparison operand has the numeric semantics of
        /// expression-field text: an int stays exact, missing propagates to
        /// unknown, and a non-numeric or absent name or an arithmetic fault
        /// fails at its node.
        #[test]
        fn arithmetic_operands_have_numeric_semantics() {
            let plan = predicate("ALT > 3 * ULN", "columns.F.flag.condition");
            for (alt, uln, truth) in [
                (Value::Int(10), Value::Int(3), Truth::True),
                (Value::Int(9), Value::Int(3), Truth::False),
                (Value::float(9.5), Value::Int(3), Truth::True),
                (Value::Int(10), Value::Missing, Truth::Unknown),
            ] {
                let mut port = port(&[("ALT", alt), ("ULN", uln)]);
                assert_eq!(plan.evaluate_truth(&mut port).unwrap(), truth);
                assert_eq!(port.reads, ["ALT", "ULN"]);
            }
            let exact = [("X", Value::Int(9007199254740993))];
            for (text, truth) in [
                ("X + 0 = 9007199254740993", Truth::True),
                ("X * 1.0 = 9007199254740993", Truth::False),
                ("-X < 0", Truth::True),
                ("X BETWEEN 2 * 4503599627370496 AND X + 1", Truth::True),
                ("X IN (1, X - 1 + 1)", Truth::True),
            ] {
                let mut port = port(&exact);
                assert_eq!(
                    predicate(text, "filter").evaluate_truth(&mut port).unwrap(),
                    truth,
                    "{text}"
                );
            }
            // A comparison operand keeps its type; an arithmetic one is numeric.
            let mut text = port(&[
                ("ALT", Value::Str("high".into())),
                ("ULN", Value::Str("x".into())),
            ]);
            let error = plan.evaluate_truth(&mut text).unwrap_err();
            assert_eq!(text.reads, ["ALT", "ULN"]);
            assert_eq!(
                error.kind,
                ErrorKind::Condition(Condition::NumericInput {
                    identifier: "ULN".into(),
                    actual: ValueType::Str
                })
            );
            assert_eq!(error.route, [Step::Right, Step::RightOperand]);
            let site = error
                .source_span
                .expect("an arithmetic operand names its node");
            assert_eq!(&plan.expression()[site.start..site.end], "ULN");
            let mut absent = port(&[("ALT", Value::Int(1))]);
            let error = plan.evaluate_truth(&mut absent).unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::Condition(Condition::NumericUnknownField {
                    identifier: "ULN".into()
                })
            );
            for (text, failed, route) in [
                ("1 / 0 = 1 OR TRUE", "1 / 0", vec![Step::Left, Step::Left]),
                ("X * 1024 > 0", "X * 1024", vec![Step::Left]),
            ] {
                let plan = predicate(text, "filter");
                let error = plan.evaluate_truth(&mut port(&exact)).unwrap_err();
                assert!(matches!(
                    error.kind,
                    ErrorKind::Condition(Condition::Arithmetic(
                        ArithmeticErrorKind::DivisionByZero
                            | ArithmeticErrorKind::IntegerOverflow { .. }
                    ))
                ));
                let site = error
                    .source_span
                    .expect("an arithmetic fault names its node");
                assert_eq!(&text[site.start..site.end], failed);
                assert_eq!(error.route, route);
            }
        }

        #[test]
        fn whole_expression_grammar_and_integer_carrier_precede_execution() {
            assert!(matches!(
                compile("FALSE AND X =", Context::Predicate, "site"),
                Err(CompileError::Invalid {
                    failure: Failure::Syntax,
                    ..
                })
            ));
            for text in [
                "FALSE AND X = 9223372036854775808",
                "X = -9223372036854775809",
            ] {
                let Err(CompileError::Invalid {
                    failure, position, ..
                }) = compile(text, Context::Predicate, "site")
                else {
                    panic!("expected literal rejection")
                };
                assert_eq!(failure, Failure::IntegerOverflow);
                assert_eq!(
                    position.character,
                    text.find("9223372036854775808")
                        .unwrap_or_else(|| text.find("-9223372036854775809").unwrap())
                );
                let finding =
                    failure.diagnostic(Context::Predicate, "site", text, position.character);
                assert_eq!(finding.definition().condition, "integer_overflow");
                assert_eq!(finding.definition().phase, "validation");
                assert_eq!(finding.definition().rule, Some("VALUE-5"));
            }
        }
    }
    mod predicate_grammar {
        use yamaa_core::expression::{
            compile, Comparison, CompileError, CompiledExpression, Context, Failure, NodeKind,
            Resource, TemporalKind,
        };
        use yamaa_core::temporal::TemporalError;

        fn predicate(text: &str) -> Result<CompiledExpression, CompileError> {
            compile(text, Context::Predicate, "filter")
        }

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
                predicate(expression).unwrap();
            }
            for expression in [
                "X = 9223372036854775808",
                "X = -9223372036854775809",
                "FALSE AND X IN (1, +9223372036854775808)",
                "'\u{1f600}' = -9223372036854775809",
                "X BETWEEN 0 AND 9999999999999999999999999999999",
            ] {
                let Err(CompileError::Invalid {
                    failure, position, ..
                }) = predicate(expression)
                else {
                    panic!("overflowing literal admitted: {expression}")
                };
                assert_eq!(failure, Failure::IntegerOverflow);
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
                let finding = failure.diagnostic(
                    Context::Predicate,
                    "filter",
                    expression,
                    position.character,
                );
                assert_eq!(finding.definition().phase, "validation");
                assert_eq!(finding.definition().condition, "integer_overflow");
                assert_eq!(finding.definition().rule, Some("VALUE-5"));
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
                predicate("X = 9223372036854775808 AND"),
                Err(CompileError::Invalid {
                    failure: Failure::Syntax,
                    ..
                })
            ));
        }

        /// Syntax association is independent of evaluator truth and preserves written names.
        #[test]
        fn precedence_spelling_and_names() {
            let compiled = predicate("NOT A = +0001 OR B < -2.50e+3 AND (A IS NULL)").unwrap();
            let p = compiled.syntax();
            assert_eq!(compiled.identifiers().collect::<Vec<_>>(), ["A", "B"]);
            let nodes = p.nodes();
            let NodeKind::Or(left, right) = nodes[p.root()].kind else {
                panic!()
            };
            assert!(matches!(nodes[left].kind, NodeKind::Not(_)));
            let NodeKind::And(_, group) = nodes[right].kind else {
                panic!()
            };
            assert!(matches!(nodes[group].kind, NodeKind::Group(_)));
            let numbers: Vec<_> = nodes
                .iter()
                .enumerate()
                .filter_map(|(id, n)| match &n.kind {
                    NodeKind::Number { fractional, text } => {
                        assert_eq!(p.text(id), text);
                        Some((p.text(id), *fractional))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(numbers, [("+0001", false), ("-2.50e+3", true)]);
            let compiled = predicate("FALSE OR TRUE OR FALSE").unwrap();
            let p = compiled.syntax();
            let NodeKind::Or(left, _) = p.nodes()[p.root()].kind else {
                panic!()
            };
            assert!(matches!(p.nodes()[left].kind, NodeKind::Or(..)));
            // Arithmetic binds tighter than a comparison, and the upper bound of
            // BETWEEN tighter than AND.
            let compiled = predicate("A BETWEEN 1 AND B + 1 AND C * 2 = -D").unwrap();
            let p = compiled.syntax();
            let NodeKind::And(range, comparison) = p.nodes()[p.root()].kind else {
                panic!()
            };
            assert_eq!(p.text(range), "A BETWEEN 1 AND B + 1");
            assert_eq!(p.text(comparison), "C * 2 = -D");
        }

        /// Every closed comparison and compound production admits only value operands.
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
                let compiled = predicate(&format!("A {op} NULL")).unwrap();
                let p = compiled.syntax();
                assert!(
                    matches!(p.nodes()[p.root()].kind,NodeKind::Compare{operator,..} if operator==expected)
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
                // Value operands may be arithmetic and grouped.
                "A+1 = 2",
                "(A) = 1",
                "A = - 1",
                "A IN ((1))",
                "ABS(A - B) <= 0.5",
                // DATE, DATETIME and ESCAPE are keywords only in place.
                "DATE = 1",
                "DATETIME IS NULL",
                "ESCAPE LIKE ESCAPE ESCAPE '!'",
            ] {
                predicate(text).unwrap();
            }
            for text in [
                "",
                " ",
                "A",
                "NULL",
                "A != 1",
                "A == 1",
                "A = B = C",
                "A IN ()",
                "A IS TRUE",
                "A BETWEEN 1 OR 2",
                "A NOT = 1",
                "A LIKE '%' ESCAPE 1",
                "TRUE()",
                "str_detect(A, 'a')",
                "str_contains(A, B)",
                "str_contains(A)",
                "str_contains(A, 'a', 'b')",
                "SUM(A) > 1",
                "NOT NULL",
                "A + (B = 1) = 2",
                "A. = 1",
                "A.B.C = 1",
                "A = 1.",
                "A = .1",
                "A = 1e+",
                "A = 0x10",
                "A = 'open",
                "A = TRUE",
            ] {
                assert!(
                    matches!(predicate(text), Err(CompileError::Invalid { .. })),
                    "{text}"
                );
            }
            for keyword in [
                "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE", "NOT",
                "NULL", "OR", "TRUE",
            ] {
                predicate(&format!("D.{keyword} = 1")).unwrap();
            }
        }

        /// Doubled quotes decode exactly once; other Unicode, backslash and NUL are literal.
        #[test]
        fn text_temporal_and_unbounded_number_syntax() {
            let text = "'it''s\\\0\u{1f600}' = 'x'";
            let compiled = predicate(text).unwrap();
            assert_eq!(
                compiled.syntax().nodes()[0].kind,
                NodeKind::String("it's\\\0\u{1f600}".into())
            );
            for text in [
                "A = DATE '2024-02-29'",
                "A = DATETIME '2024-02-29T12:34'",
                "A = DATETIME '2024-02-29T12:34:56'",
                "A = 1e9999",
            ] {
                predicate(text).unwrap();
            }
            assert!(matches!(
                predicate("A = DATE '2025-02-30'"),
                Err(CompileError::Invalid {
                    failure: Failure::InvalidTemporal {
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
                    predicate(text),
                    Err(CompileError::Invalid {
                        failure: Failure::InvalidTemporal { .. },
                        ..
                    })
                ));
            }
        }

        /// Regex admission uses portable grammar, including features rejected by the
        /// reference host, once the whole text parses.
        #[test]
        fn portable_regex_literals_are_checked_once_the_text_parses() {
            for pattern in [r"[]", r"[^]", r"(?<name>a)\w", r"[\S]", r"a|b(c)*?"] {
                let text = format!("str_contains(A, '{pattern}')");
                let compiled = predicate(&text).unwrap();
                let p = compiled.syntax();
                assert!(matches!(
                    &p.nodes()[p.root()].kind,
                    NodeKind::Call { name, .. } if p.slice(*name) == "str_contains"
                ));
            }
            for pattern in [
                r"(?P<name>a)",
                r"(?i)a",
                r"\p{Letter}",
                r"\a",
                r"(?<=a+)b",
                r"(?<=a|b)c",
                r"a(?=b)",
                r"\1(a)",
                r"(?<name>a)\k<name>",
                r"(?<\u0061>a)",
            ] {
                let expression = format!("str_contains(A, '{pattern}')");
                let Err(CompileError::Invalid {
                    failure: Failure::InvalidRegex { .. },
                    ..
                }) = predicate(&expression)
                else {
                    panic!("TEXT-6 syntax must be a registered finding: {pattern}")
                };
            }
            let text = "str_contains('\u{1f600}', '(')";
            let Err(CompileError::Invalid {
                position, failure, ..
            }) = predicate(text)
            else {
                panic!()
            };
            assert_eq!(position.byte, 21);
            assert_eq!(position.character, 18);
            assert!(matches!(failure, Failure::InvalidRegex { byte: 1, .. }));
            assert_eq!(failure.rule(), "FORMULA-8");
            // The whole text is lexed and parsed before a pattern is compiled.
            for text in ["str_contains('\u{1f600}', '('", "str_contains(A, '(') @"] {
                assert!(
                    matches!(
                        predicate(text),
                        Err(CompileError::Invalid {
                            failure: Failure::Syntax,
                            ..
                        })
                    ),
                    "{text}"
                );
            }
            // A million copies exceed the compiled size limit, which is a resource
            // refusal at the pattern literal rather than a language finding.
            let Err(CompileError::Limit {
                position,
                resource: Resource::Regex(yamaa_core::portable_regex::Resource::Size),
                limit: yamaa_core::limits::Limits::REGEX_SIZE,
            }) = predicate("str_contains(A, 'a{1000001}')")
            else {
                panic!("an oversized pattern must be refused")
            };
            assert_eq!(position.byte, 16);
        }

        /// Static ESCAPE defects have their owning rule and original literal site.
        #[test]
        fn escape_admission_and_unicode_positions() {
            for text in [
                "A LIKE 'a!' ESCAPE '!'",
                "A LIKE 'a' ESCAPE ''",
                "A LIKE 'a' ESCAPE 'ab'",
            ] {
                let Err(CompileError::Invalid { failure, .. }) = predicate(text) else {
                    panic!("{text}")
                };
                assert_eq!(failure, Failure::InvalidEscape);
                assert_eq!(failure.rule(), "FORMULA-7");
            }
            for text in [
                "A LIKE 'a!!' ESCAPE '!'",
                "A LIKE 'a!!!b' ESCAPE '!'",
                "A LIKE B ESCAPE '!'",
                "A LIKE 'a\u{1f600}%' ESCAPE '\u{1f600}'",
            ] {
                predicate(text).unwrap();
            }
            let Err(CompileError::Invalid { position, .. }) = predicate("'\u{1f600}' = @") else {
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
                    predicate(&text),
                    Err(CompileError::Limit {
                        resource: Resource::Depth,
                        limit: 128,
                        ..
                    })
                ));
                predicate("TRUE").unwrap();
            }
            let compiled = predicate(&format!("A IN ({})", vec!["1"; 1000].join(","))).unwrap();
            assert_eq!(compiled.syntax().nodes().len(), 1002);
        }

        /// Repeated admitted regex literals do not share an invented request quota.
        #[test]
        fn repeated_regex_compilation_retains_each_admitted_pattern() {
            let pattern = format!("(a){}b", "(a|aa)".repeat(8));
            let call = format!("str_contains(A, '{pattern}')");
            predicate(&call).unwrap();
            let mut expression = call.clone();
            for _ in 0..9 {
                expression = format!("({expression} OR {expression})");
            }
            predicate(&expression).unwrap();
            predicate(&call).unwrap();
        }

        /// Reasons belong to Rust productions, so host planning never reparses a failed input.
        #[test]
        fn grammar_messages_preserve_production_reason() {
            for (text, expected) in [
                ("", "expected an operand"),
                ("A >", "expected an operand"),
                ("A = TRUE", "a truth value is not an operand"),
                ("TRUE FALSE", "unexpected trailing token"),
                ("A = 'open", "unterminated string literal"),
                ("A. = 1", "invalid qualified identifier"),
                ("A @", "unexpected character"),
                ("A \u{200b}", "unexpected character"),
                ("(TRUE", "expected ')' to close a group"),
                ("A IS 1", "expected NULL after IS"),
                ("A IN 1", "expected '(' after IN"),
                ("A IN (1 2)", "expected ')' after IN operands"),
                ("A BETWEEN 1 OR 2", "expected AND in BETWEEN"),
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
                ("A = DATE 1", "unexpected trailing token"),
                ("A = DATE '2025-02-30'", "invalid date literal"),
                ("A = DATETIME 'bad'", "invalid datetime literal"),
                (
                    "str_contains(A)",
                    "the function takes another number of arguments",
                ),
                (
                    "str_contains(A, B)",
                    "str_contains pattern must be a string literal",
                ),
                ("str_contains(A, 'a'", "expected ')' to close a call"),
                (
                    "str_contains(A, '[')",
                    "invalid regex in str_contains pattern: unclosed character class",
                ),
                (
                    "X = 9223372036854775808",
                    "integer literal is outside the signed 64-bit range",
                ),
            ] {
                let Err(CompileError::Invalid { message, .. }) = predicate(text) else {
                    panic!("expected a language finding for {text}")
                };
                assert_eq!(message, expected, "{text}");
            }
        }
    }
    mod regex {
        use yamaa_core::{
            limits::Limits,
            portable_regex::{CompileError, Pattern, Resource},
        };

        /// A compiled pattern serves every later match unchanged.
        #[test]
        fn repeated_matches_reuse_the_compiled_pattern() {
            let compiled = pattern("a+");
            for _ in 0..3 {
                assert_eq!(compiled.search("aaaa").unwrap().groups, [Some("aaaa")]);
            }
        }

        /// Unicode subjects retain exact capture text in both matching modes.
        #[test]
        fn unicode_subjects_keep_exact_scalar_captures() {
            let compiled = pattern(".");
            assert_eq!(compiled.search("😀").unwrap().groups, [Some("😀")]);
            assert!(compiled.is_full_match("😀"));
            assert!(!pattern("..").is_full_match("😀"));
            assert!(pattern("a").search("b").is_none());
        }

        /// Compile a test pattern under production defaults.
        fn pattern(text: &str) -> Pattern {
            Pattern::compile(text).unwrap()
        }
        /// Observe borrowed capture text without converting missing groups to empty strings.
        fn captures<'a>(text: &str, subject: &'a str) -> Option<Vec<Option<&'a str>>> {
            pattern(text).search(subject).map(|m| m.groups)
        }

        /// Leftmost-first: earlier alternatives first, greedy (lazy) quantifiers
        /// prefer more (fewer) iterations; a full match continues past an
        /// alternative that stops early.
        #[test]
        fn leftmost_first_order_and_empty_matches() {
            assert_eq!(captures("a|ab", "ab"), Some(vec![Some("a")]));
            assert!(pattern("a|ab").is_full_match("ab"));
            assert_eq!(captures("a+?", "aaa"), Some(vec![Some("a")]));
            assert_eq!(captures("a+", "aaa"), Some(vec![Some("aaa")]));
            assert_eq!(captures("a*", "bbb"), Some(vec![Some("")]));
            assert_eq!(captures("b|a*", "aab"), Some(vec![Some("aa")]));
            assert_eq!(captures("a{2,3}?a", "aaaa"), Some(vec![Some("aaa")]));
            assert_eq!(captures("a{2,3}a", "aaaa"), Some(vec![Some("aaaa")]));
            // An iteration that matches empty ends a repetition, and it entered
            // its group: the capture is empty rather than unset.
            assert_eq!(captures("(a?)*", ""), Some(vec![Some(""), Some("")]));
            assert_eq!(captures("(a?)+", ""), Some(vec![Some(""), Some("")]));
            assert_eq!(captures("(|a)*", "aa"), Some(vec![Some(""), Some("")]));
            assert_eq!(
                captures("(a|){1,3}b", "ab"),
                Some(vec![Some("ab"), Some("")])
            );
        }

        /// A capture inside a repetition keeps the last iteration that entered it;
        /// unentered and empty captures stay distinct.
        #[test]
        fn repeated_captures_keep_their_last_entered_iteration() {
            assert_eq!(
                captures(r"(a|(b))+", "aba"),
                Some(vec![Some("aba"), Some("a"), Some("b")])
            );
            assert_eq!(
                captures(r"(?:(a)|b){2,3}", "abb"),
                Some(vec![Some("abb"), Some("a")])
            );
            assert_eq!(
                captures(r"^(?:(a)|(b))$", "b"),
                Some(vec![Some("b"), None, Some("b")])
            );
            assert_eq!(captures(r"^a(b)?$", "a"), Some(vec![Some("a"), None]));
            assert_eq!(captures(r"^(a*)$", ""), Some(vec![Some(""), Some("")]));
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
            // \b is an ASCII word boundary: é is not a word character.
            assert!(captures(r"\bé", "é").is_none());
            assert!(captures(r"a\b", "aé").is_some());
            assert_eq!(
                captures(r"^\u{1D400}$", "\u{1d400}"),
                Some(vec![Some("\u{1d400}")])
            );
            assert_eq!(captures(r"^𝐀$", "\u{1d400}"), Some(vec![Some("\u{1d400}")]));
            assert_eq!(captures(r"^\cA$", "\u{1}"), Some(vec![Some("\u{1}")]));
            assert_eq!(captures(r"^\0$", "\0"), Some(vec![Some("\0")]));
            assert_eq!(captures(r"[]", "x"), None);
            assert_eq!(captures(r"[^]", "\n"), Some(vec![Some("\n")]));
            assert_eq!(captures(r"\B", ""), Some(vec![Some("")]));
            assert_eq!(captures(r"a$", "a\n"), None);
            // An unpaired surrogate escape names no scalar and matches nothing.
            assert_eq!(captures(r"\uD800", "\u{d7ff}\u{e000}"), None);
            assert_eq!(
                captures(r"[\uD800-]", "\u{e000}"),
                Some(vec![Some("\u{e000}")])
            );
        }

        /// Syntax outside the subset is a finding, never a no-match or a host dialect.
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
                r"a++",
                r"a???",
                r"(?<x>a)(?<x>b)",
                "(",
                "[",
                r"\",
                "a{",
                "{1}",
                "]",
            ] {
                assert!(
                    matches!(Pattern::compile(text), Err(CompileError::Invalid { .. })),
                    "{text}"
                );
            }
        }

        /// Lookaround and backreferences are outside the subset, at their own byte.
        #[test]
        fn lookaround_and_backreferences_are_rejected_where_written() {
            for (text, byte, reason) in [
                ("a(?=b)", 1, "lookahead outside the portable subset"),
                ("a(?!b)", 1, "lookahead outside the portable subset"),
                ("(?=a)*", 0, "lookahead outside the portable subset"),
                ("é(?<=a)b", 2, "lookbehind outside the portable subset"),
                ("(?<!a)b", 0, "lookbehind outside the portable subset"),
                ("(?<=a+)b", 0, "lookbehind outside the portable subset"),
                (r"(a)\1", 3, "backreference outside the portable subset"),
                (r"\2(a)(b)", 0, "backreference outside the portable subset"),
                (
                    r"(?<x>a)\k<x>",
                    7,
                    "backreference outside the portable subset",
                ),
                (
                    r"\k<absent>",
                    0,
                    "backreference outside the portable subset",
                ),
            ] {
                assert_eq!(
                    Pattern::compile(text),
                    Err(CompileError::Invalid { byte, reason }),
                    "{text}"
                );
            }
        }

        /// Group names are ASCII identifiers: a letter or underscore, then
        /// letters, digits or underscores. Names only label numbered groups.
        #[test]
        fn group_names_are_ascii_identifiers() {
            for name in ["x", "_", "a0", "_9", "Serial_No"] {
                assert_eq!(
                    captures(&format!(r"^(?<{name}>a)(b)$"), "ab"),
                    Some(vec![Some("ab"), Some("a"), Some("b")]),
                    "{name}"
                );
            }
            for (name, offset) in [
                ("$", 0),
                ("\u{e9}", 0),
                ("\u{1d400}", 0),
                ("a\u{301}", 1),
                ("a-", 1),
                ("0x", 0),
                (r"\x61", 0),
                (r"\u0061", 0),
            ] {
                assert_eq!(
                    Pattern::compile(&format!("(?<{name}>a)")),
                    Err(CompileError::Invalid {
                        byte: 3 + offset,
                        reason: "invalid group name"
                    }),
                    "{name}"
                );
            }
            assert_eq!(
                Pattern::compile("(?<>a)"),
                Err(CompileError::Invalid {
                    byte: 3,
                    reason: "empty group name"
                })
            );
            assert!(matches!(
                Pattern::compile("(?<x>a)|(?<x>b)"),
                Err(CompileError::Invalid {
                    reason: "duplicate group name",
                    ..
                })
            ));
        }

        /// Compilation refuses only group nesting beyond the semantic depth and
        /// programs beyond the compiled size; matching has no step budget.
        #[test]
        fn fixed_depth_and_compiled_size_refuse_only_their_real_boundaries() {
            for depth in [127, 128] {
                let source = "(".repeat(depth) + "a" + &")".repeat(depth);
                assert!(Pattern::compile(&source).is_ok());
            }
            // Nesting counts the lowered syntax, not groups alone: each level here
            // nests a group, an alternation, a sequence and a repetition, so 32
            // levels reach the semantic depth and a 33rd is refused.
            let mut quantified = "a".to_owned();
            for _ in 0..32 {
                quantified = format!("(?:a|b{quantified}*c)");
            }
            assert!(Pattern::compile(&quantified).is_ok());
            assert_eq!(
                Pattern::compile(&format!("(?:a|b{quantified}*c)")),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    limit: 128
                })
            );
            let source = "(".repeat(129) + "a" + &")".repeat(129);
            assert_eq!(
                Pattern::compile(&source),
                Err(CompileError::Limit {
                    resource: Resource::Depth,
                    limit: 128
                })
            );
            for source in [
                "(?:a{1000}){1000}".to_owned(),
                "a{4294967295}".to_owned(),
                format!("(?:ab){{{}}}", usize::MAX),
                format!("a{{0,{}}}", u64::MAX),
            ] {
                assert_eq!(
                    Pattern::compile(&source),
                    Err(CompileError::Limit {
                        resource: Resource::Size,
                        limit: Limits::REGEX_SIZE
                    }),
                    "{source}"
                );
            }
            assert!(Pattern::compile("(?:a{3}){3}").is_ok());
            let literal = pattern("z");
            assert_eq!(literal.search(&"a".repeat(500_000)), None);
            let compiled = pattern("(a+)+$");
            assert_eq!(compiled.search(&("a".repeat(100_000) + "!")), None);
            assert_eq!(
                compiled.search("a").unwrap().groups,
                vec![Some("a"), Some("a")]
            );
        }

        /// Flat patterns and long subjects keep their match results.
        #[test]
        fn large_flat_patterns_and_long_subjects_match() {
            let source = "a".repeat(3000);
            assert!(pattern(&source).is_full_match(&source));
            let subject = "a".repeat(1_048_577);
            assert_eq!(
                pattern("a").search(&subject).unwrap().groups,
                vec![Some("a")]
            );
        }

        /// TEXT-8 expands braced escapes to VALUE-11 scalars, excluding surrogates.
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

        /// Every short pattern over the syntax alphabet either compiles or is
        /// refused with a finding: the lowering never hands the crate syntax it
        /// rejects.
        #[test]
        fn every_short_pattern_compiles_or_is_refused() {
            let alphabet = [
                "(", ")", "[", "]", "{", "}", "*", "+", "?", "|", "^", "$", ".", "-", ",", "\\",
                "a", "0", "1", "d", "w", "s", "b", "B", "k", "<", ">", "=", "!", ":", "u", "x",
                "c", "é", " ",
            ];
            let mut count = 0;
            for first in alphabet {
                for second in alphabet {
                    for third in alphabet {
                        let source = format!("{first}{second}{third}");
                        let _ = Pattern::compile(&source);
                        count += 1;
                    }
                }
            }
            assert_eq!(count, alphabet.len().pow(3));
        }
    }
}
