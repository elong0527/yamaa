//! Relations contracts: aggregate expressions, column dependencies, dataset
//! count, declarative flags, dependency analysis, intermediate match values,
//! intermediate reference and reference binding.
mod relations {
    mod aggregate_expressions {
        use yamaa_core::{
            expression::{
                compile, AggregateResult, CompileError, CompiledExpression, Context, Failure,
                ReductionOperand, Resolver as ExpressionResolver,
            },
            numeric::Number,
            value::{Selection, Value},
        };

        fn compile_aggregate(text: &str, path: &str) -> Result<CompiledExpression, CompileError> {
            compile(text, Context::Aggregate, path)
        }

        struct Resolver {
            calls: Vec<String>,
        }

        #[test]
        fn all_retained_aggregate_plans_keep_the_complete_expression_and_authored_path() {
            let name = format!("D.{}", "A".repeat(60));
            let mut terms = vec![format!("SUM({name} + 1)"); 512];
            while terms.len() > 1 {
                terms = terms
                    .chunks(2)
                    .map(|pair| format!("({} + {})", pair[0], pair[1]))
                    .collect();
            }
            let expression = terms.pop().unwrap();
            let path = "columns.VALUE.aggregate";
            let plan = compile_aggregate(&expression, path).unwrap();
            assert_eq!(plan.reductions().len(), 512);
            for reduction in plan.reductions() {
                assert_eq!(reduction.operand, ReductionOperand::Numeric);
            }
            assert_eq!(plan.expression(), expression);
            assert_eq!(plan.spec_path(), path);
            assert_eq!(expression.matches("SUM(").count(), 512);
        }

        #[test]
        fn a_bare_identifier_outside_reduction_uses_numeric_coercion() {
            let plan = compile_aggregate("A", "columns.VALUE.aggregate").unwrap();
            assert_eq!(plan.result(), AggregateResult::Numeric);
            assert_eq!(
                plan.evaluate_aggregate(&mut Resolver { calls: Vec::new() }, &[])
                    .unwrap(),
                Number::Int(7)
            );
            assert_eq!(plan.ungrouped_identifiers().collect::<Vec<_>>(), ["A"]);
        }
        impl ExpressionResolver for Resolver {
            type Error = std::convert::Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.calls.push(name.into());
                Ok(Selection::Present(Value::Int(match name {
                    "EX.DOSE" => 3,
                    "A" => 7,
                    _ => panic!("unbound aggregate resolver identity {name}"),
                })))
            }
        }

        /// Reductions reach the arithmetic as values, never as resolver names.
        #[test]
        fn outer_and_per_record_arithmetic_share_the_numeric_evaluator_without_rewriting() {
            let expression = "POWER(SUM(EX.DOSE * 2), 2) + MAX(EX.DOSE)";
            let plan = compile_aggregate(expression, "columns.VALUE.derivation.aggregate").unwrap();
            assert_eq!(plan.identifiers().collect::<Vec<_>>(), ["EX.DOSE"]);
            assert_eq!(plan.reductions()[0].operand, ReductionOperand::Numeric);
            assert_eq!(plan.reductions()[0].reference, "SUM(EX.DOSE * 2)");
            let mut resolver = Resolver { calls: Vec::new() };
            assert_eq!(
                plan.evaluate_operand(0, &mut resolver).unwrap(),
                Number::Int(6)
            );
            assert_eq!(plan.result(), AggregateResult::Numeric);
            assert_eq!(
                plan.evaluate_aggregate(&mut resolver, &[Value::Int(6), Value::Int(4)])
                    .unwrap(),
                Number::float(40.0)
            );
            assert_eq!(resolver.calls, ["EX.DOSE"]);
            assert_eq!(plan.expression(), expression);
        }

        #[test]
        fn bare_reducers_retain_typed_reads_and_record_stars() {
            let plan =
                compile_aggregate("(ONLY(EX.TEXT))", "rows[0].derivations.TEXT.aggregate").unwrap();
            assert_eq!(plan.result(), AggregateResult::Reduction(0));
            assert_eq!(
                plan.reductions()[0].operand,
                ReductionOperand::Value("EX.TEXT".into())
            );
            let count = compile_aggregate("COUNT(EX.*)", "columns.N.aggregate").unwrap();
            assert_eq!(count.record_relations().collect::<Vec<_>>(), ["EX"]);
            assert_eq!(
                count.reductions()[0].operand,
                ReductionOperand::Records("EX".into())
            );
            let group = compile_aggregate("KEY + SUM(EX.DOSE)", "columns.X.aggregate").unwrap();
            assert_eq!(group.ungrouped_identifiers().collect::<Vec<_>>(), ["KEY"]);
        }

        #[test]
        fn failures_preserve_full_authored_expression_and_inner_span() {
            let expression = "SUM(A / 0) + 1";
            let plan = compile_aggregate(expression, "columns.X.aggregate").unwrap();
            assert_eq!(plan.reductions()[0].operand, ReductionOperand::Numeric);
            let error = plan
                .evaluate_operand(0, &mut Resolver { calls: Vec::new() })
                .unwrap_err();
            assert_eq!(error.expression, expression);
            let span = error.source_span.unwrap();
            assert_eq!(&expression[span.start..span.end], "A / 0");
            assert_eq!(error.spec_path, "columns.X.aggregate");
            let complete = compile_aggregate("SUM(A + B) + SUM(C)", "columns.X.aggregate").unwrap();
            assert_eq!(complete.reductions().len(), 2);
            assert_eq!(complete.identifiers().collect::<Vec<_>>(), ["A", "B", "C"]);
            assert!(matches!(
                compile_aggregate("SUM(MEAN(A))", "columns.X.aggregate"),
                Err(CompileError::Invalid {
                    failure: Failure::NestedReduction { .. },
                    ..
                })
            ));
        }
    }
    mod column_dependencies {
        use yamaa_core::column_dependencies::{analyze, Analysis, Diagnostic, Error};
        use yamaa_core::dependency_analysis::Error as GraphError;

        /// Decode hand-authored fixture indices, independently of the transport parser.
        fn indices(text: &str) -> Vec<usize> {
            if text == "-" || text == "_" {
                vec![]
            } else {
                text.split(',').map(|n| n.parse().unwrap()).collect()
            }
        }

        /// Pin rule order, authored dependency/key order, and completed-phase scheduling.
        #[test]
        fn shared_column_dependency_truth() {
            for line in include_str!("fixtures/column_dependencies.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = line.split('\t').collect();
                let graph = if fields[1] == "~" {
                    vec![]
                } else {
                    fields[1]
                        .split(';')
                        .map(|edges| (edges != "-").then(|| indices(edges)))
                        .collect()
                };
                let diagnostics = if fields[5] == "-" {
                    vec![]
                } else {
                    fields[5]
                        .split(';')
                        .map(|entry| {
                            let (kind, columns) = entry.split_once(':').unwrap();
                            let columns = indices(columns);
                            match kind {
                                "cycle" => Diagnostic::Cycle { columns },
                                "forward" => Diagnostic::ForwardReference {
                                    column: columns[0],
                                    dependency: columns[1],
                                },
                                "missing" => {
                                    Diagnostic::MissingKeyDerivation { column: columns[0] }
                                }
                                "key" => Diagnostic::KeyDependency {
                                    column: columns[0],
                                    dependency: columns[1],
                                },
                                _ => panic!("unknown fixture diagnostic"),
                            }
                        })
                        .collect()
                };
                assert_eq!(
                    analyze(&graph, &indices(fields[2]), fields[3].parse().unwrap(),),
                    Ok(Analysis {
                        order: indices(fields[4]),
                        diagnostics
                    }),
                    "{}",
                    fields[0]
                );
            }
        }

        /// Malformed metadata never poisons the next call.
        #[test]
        fn metadata_admission_and_reuse() {
            let graph = [Some(vec![1, 1, 99]), None];
            for _ in 0..2 {
                assert_eq!(
                    analyze(&graph, &[99], false),
                    Err(Error::Graph(GraphError::InvalidDependency {
                        node: 0,
                        dependency: 99
                    }))
                );
                assert_eq!(
                    analyze(&[None], &[1], true),
                    Err(Error::InvalidKey { key: 1 })
                );
                assert_eq!(
                    analyze(&[None], &[0, 0], true),
                    Err(Error::DuplicateKey { key: 0 })
                );
                assert_eq!(
                    analyze(&[Some(vec![])], &[0], false),
                    Ok(Analysis {
                        order: vec![0],
                        diagnostics: vec![]
                    })
                );
            }
        }

        /// Host path selection is explicit even where two failures share one condition.
        #[test]
        fn portable_diagnostic_metadata() {
            let cases = [
                (
                    Diagnostic::Cycle {
                        columns: vec![1, 2, 1],
                    },
                    "dependency_cycle",
                    "COLUMN-6",
                    "operation",
                    vec![1, 2, 1],
                ),
                (
                    Diagnostic::ForwardReference {
                        column: 0,
                        dependency: 2,
                    },
                    "forward_reference",
                    "COLUMN-5",
                    "operation",
                    vec![0, 2],
                ),
                (
                    Diagnostic::MissingKeyDerivation { column: 2 },
                    "key_dependency",
                    "COLUMN-7",
                    "declaration",
                    vec![2],
                ),
                (
                    Diagnostic::KeyDependency {
                        column: 2,
                        dependency: 0,
                    },
                    "key_dependency",
                    "COLUMN-7",
                    "declaration",
                    vec![2, 0],
                ),
            ];
            for (diagnostic, condition, rule, location, columns) in cases {
                assert_eq!(
                    (
                        diagnostic.condition(),
                        diagnostic.rule(),
                        diagnostic.location(),
                        diagnostic.columns()
                    ),
                    (condition, rule, location, columns)
                );
            }
        }
    }
    mod dataset_count {
        use std::cmp::Ordering;
        use yamaa_core::dataset::{Check, CountBoundError, RowCountBound};

        fn bound(value: &str) -> RowCountBound {
            RowCountBound::new(value).unwrap()
        }

        #[test]
        fn exact_count_bounds_preserve_decimal_order_and_machine_count_width() {
            let values = [
                "-9999999999999999999999999999999999999",
                "-9223372036854775809",
                "-9223372036854775808",
                "-10",
                "-2",
                "-1",
                "0",
                "1",
                "2",
                "10",
                "9223372036854775807",
                "9223372036854775808",
                "18446744073709551615",
                "18446744073709551616",
                "9999999999999999999999999999999999999",
            ];
            for (i, a) in values.iter().enumerate() {
                for (j, b) in values.iter().enumerate() {
                    assert_eq!(bound(a).cmp(&bound(b)), i.cmp(&j));
                }
            }
            assert_eq!(bound("0").compare_count(0), Ordering::Equal);
            assert_eq!(bound("-1").compare_count(0), Ordering::Less);
            assert_eq!(bound("1").compare_count(0), Ordering::Greater);
            assert_eq!(
                bound(&usize::MAX.to_string()).compare_count(usize::MAX),
                Ordering::Equal
            );
            assert_eq!(
                bound("9999999999999999999999999999999999999").compare_count(usize::MAX),
                Ordering::Greater
            );
            let check = Check::WideRowCount {
                min: Some(bound("-9999999999999999999999999999999999999")),
                max: Some(bound("9999999999999999999999999999999999999")),
            };
            assert_eq!(check.row_count_accepts(0), Some(true));
            assert_eq!(check.row_count_accepts(usize::MAX), Some(true));
            for count in [0, 1, 2, usize::MAX] {
                assert_eq!(
                    Check::RowCount {
                        min: Some(-1),
                        max: Some(i64::MAX)
                    }
                    .row_count_accepts(count),
                    Check::WideRowCount {
                        min: Some(bound("-1")),
                        max: Some(bound(&i64::MAX.to_string()))
                    }
                    .row_count_accepts(count)
                );
            }
        }

        #[test]
        fn wide_bound_ownership_is_exact_and_noncanonical_spellings_are_rejected() {
            for invalid in ["", "-", "+1", "-0", "00", "01", "-01", " 1", "1.0", "١"] {
                assert_eq!(RowCountBound::new(invalid), Err(CountBoundError::Invalid));
            }
            let boundary = "9".repeat(16_385);
            assert_eq!(bound(&boundary).decimal(), boundary);
            assert_eq!(
                bound(&(boundary.clone() + "9")).decimal(),
                boundary.clone() + "9"
            );
            assert_eq!(
                bound(&boundary).compare_count(usize::MAX),
                Ordering::Greater
            );
        }
    }
    mod declarative_flags {
        use crate::support::diagnostic::declarative_flags::*;
        use crate::support::resolved;
        use yamaa_core::dataset::resolved::{Op, OperandRole};
        #[test]

        fn raw_truth_literals_presence_null_and_complete_name_map() {
            let plan = compile("ID = 1", Context::Predicate, "flag.condition").unwrap();
            let bindings = vec![FlagBinding {
                name: "ID".into(),
                read: FlagRead::Read(Read::Column(0)),
            }];
            let flag = Flag::new(
                plan.clone(),
                bindings.clone(),
                Value::Str("bad".into()),
                Some(Value::Int(2)),
                Some(Value::Missing),
            )
            .unwrap();
            assert_eq!(flag.value(Truth::True), ValueRef::Str("bad"));
            assert_eq!(flag.value(Truth::False), ValueRef::Int(2));
            assert_eq!(flag.value(Truth::Unknown), ValueRef::Missing);
            assert_eq!(
                Flag::new(
                    plan.clone(),
                    bindings.clone(),
                    Value::Int(1),
                    Some(Value::Missing),
                    None
                )
                .unwrap_err(),
                FlagError::MissingValueRequired
            );
            assert!(matches!(
                Flag::new(plan, vec![], Value::Missing, None, None),
                Err(FlagError::Binding(_))
            ));
        }
        #[test]

        fn semantic_flag_declarations_are_deferred_and_interleaved_before_name_binding() {
            let f = diagnostics(vec![
                ("A", "str", flag("ABSENT = 1", Some(Tree::text("N")), None)),
                ("B", "str", flag("ID >> 1", Some(Tree::text("N")), None)),
            ]);
            assert_eq!(
                f.iter().map(|d| d.code).collect::<Vec<_>>(),
                [
                    C::FlagMissingValueRequired,
                    C::PredicateUnknownField,
                    C::ExpressionInvalid,
                    C::FlagMissingValueRequired
                ]
            );
            assert_eq!(f[0].spec_paths, ["columns.A.derivation.flag.missing"]);
            assert_eq!(f[1].spec_paths, ["columns.A.derivation.flag.condition"]);
            let f = diagnostics(vec![(
                "A",
                "str",
                flag(
                    "ABSENT = 1",
                    Some(Tree::map(vec![("literal", Tree::Scalar(N::Null))])),
                    None,
                ),
            )]);
            assert_eq!(f[0].code, C::FlagMissingValueRequired);
        }
        #[test]

        fn nested_flag_declarations_precede_enclosing_case_reference_checkpoint() {
            let case = Tree::map(vec![(
                "case",
                Tree::List(vec![
                    Tree::map(vec![
                        ("when", Tree::text("UNKNOWN = 1")),
                        ("then", flag("ID >> 1", Some(Tree::text("N")), None)),
                    ]),
                    Tree::map(vec![(
                        "otherwise",
                        Tree::map(vec![("literal", Tree::text("x"))]),
                    )]),
                ]),
            )]);
            assert_eq!(
                diagnostics(vec![("F", "str", case)])
                    .iter()
                    .map(|d| d.code)
                    .collect::<Vec<_>>(),
                [
                    C::ExpressionInvalid,
                    C::FlagMissingValueRequired,
                    C::PredicateUnknownField
                ]
            );
        }
        #[test]

        fn independent_flag_admissions_keep_complete_deferred_grammar_and_name_findings() {
            let first = diagnostics(vec![
                ("A", "str", flag("ID >> 1", None, None)),
                ("B", "str", flag("ABSENT = 1", None, None)),
            ]);
            let second = diagnostics(vec![
                ("A", "str", flag("ID >> 1", None, None)),
                ("B", "str", flag("ABSENT = 1", None, None)),
            ]);
            assert_eq!(first, second);
            assert_eq!(
                first.iter().map(|d| d.code).collect::<Vec<_>>(),
                [C::ExpressionInvalid, C::PredicateUnknownField]
            );
            assert_eq!(first[0].spec_paths, ["columns.A.derivation.flag.condition"]);
            assert_eq!(first[1].spec_paths, ["columns.B.derivation.flag.condition"]);
        }
        #[test]

        fn regex_input_type_is_flag_owned_without_changing_generic_predicate_mapping() {
            let condition = Condition::ContainsExpectedText {
                actual: ValueType::Int,
            };
            assert_eq!(condition.rule(), "FORMULA-8");
            let error: EvaluationError<()> = EvaluationError {
                spec_path: "flag.condition".into(),
                expression: "original".into(),
                route: vec![Route::Value],
                source_span: None,
                kind: ErrorKind::Condition(condition),
            };
            let diagnostic = yamaa_core::dataset::flag_condition_diagnostic(&error).unwrap();
            assert_eq!(diagnostic.code, C::PredicateInputType);
            assert_eq!(diagnostic.definition().rule, Some("FORMULA-5"));
            assert_eq!(diagnostic.operand_route, Some(vec!["Value".into()]));
        }
        #[test]

        fn all_unselected_literals_and_repeated_row_defaults_remain_complete() {
            use support::specification_with_rows;
            let names = (0..40).map(|i| format!("F{i}")).collect::<Vec<_>>();
            let columns = names
                .iter()
                .map(|name| {
                    (
                        name.as_str(),
                        "str",
                        flag(
                            "TRUE",
                            Some(Tree::text("x".repeat(10_000))),
                            Some(Tree::map(vec![("literal", Tree::Scalar(N::Null))])),
                        ),
                    )
                })
                .collect();
            let rows = (0..16)
                .map(|i| {
                    Tree::Map(vec![
                        ("id".into(), Tree::text(format!("r{i}"))),
                        ("derivations".into(), Tree::map(vec![])),
                    ])
                })
                .collect();
            let spec = specification_with_rows(columns, rows);
            let source = TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            }])
            .unwrap();
            let plan = PreparedSpecification::prepare(&spec)
                .unwrap()
                .bind(&source)
                .unwrap();
            assert_eq!(resolved::templates(&plan).len(), 16);
            let flags = resolved::columns(&plan)
                .into_iter()
                .filter_map(|binding| match resolved::op(&plan, binding.expr) {
                    Op::Flag(flag) => Some(flag),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(flags.len(), 40);
            for flag in flags {
                assert_eq!(flag.value(Truth::False), ValueRef::Str(&"x".repeat(10_000)));
                assert_eq!(flag.value(Truth::Unknown), ValueRef::Missing);
            }
        }
        #[test]

        fn existing_between_case_binds_one_name_and_preserves_two_subject_occurrences() {
            let case = Tree::map(vec![(
                "case",
                Tree::List(vec![Tree::map(vec![
                    ("when", Tree::text("ID BETWEEN 1 AND 2")),
                    ("then", flag("ID = 1", None, None)),
                ])]),
            )]);
            let spec = specification(vec![("F", "str", case)]);
            let prepared = PreparedSpecification::prepare(&spec).unwrap();
            let source = TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            }])
            .unwrap();
            let plan = prepared.bind(&source).unwrap();
            let case = resolved::columns(&plan)[0].expr;
            assert_eq!(resolved::op(&plan, case), &Op::Case);
            let when = resolved::operands(&plan, case, OperandRole::When)[0];
            let Op::Predicate(when) = resolved::op(&plan, when) else {
                panic!("case")
            };
            assert_eq!(when.bindings().len(), 1);
            assert_eq!(when.plan().identifiers().collect::<Vec<_>>(), ["ID"]);
            // The subject is one written operand, resolved again for each comparison.
            struct Trace(Vec<String>);
            impl yamaa_core::expression::Resolver for Trace {
                type Error = std::convert::Infallible;
                fn resolve(
                    &mut self,
                    name: &str,
                ) -> Result<yamaa_core::value::Selection, Self::Error> {
                    self.0.push(name.into());
                    Ok(yamaa_core::value::Selection::Present(Value::Int(1)))
                }
            }
            let mut trace = Trace(Vec::new());
            assert_eq!(when.plan().evaluate_truth(&mut trace).unwrap(), Truth::True);
            assert_eq!(trace.0, ["ID", "ID"]);
        }
    }
    mod dependency_analysis {
        use yamaa_core::dependency_analysis::{analyze, Analysis, Error};

        /// Read hand-authored declaration indices; this fixture notation is not a language grammar.
        fn indices(text: &str) -> Vec<usize> {
            if text == "-" {
                vec![]
            } else {
                text.split(',').map(|n| n.parse().unwrap()).collect()
            }
        }

        /// Independent truth distinguishes first reachable cycles from globally earliest cycles.
        #[test]
        fn shared_dependency_graph_truth() {
            for line in include_str!("fixtures/dependency_analysis.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = line.split('\t').collect();
                let mut graph = vec![vec![]; fields[1].parse().unwrap()];
                if fields[2] != "-" {
                    for edges in fields[2].split(';') {
                        let (node, dependencies) = edges.split_once(':').unwrap();
                        graph[node.parse::<usize>().unwrap()] = indices(dependencies);
                    }
                }
                let expected = Analysis {
                    cycle: (fields[3] != "-").then(|| indices(fields[3])),
                    order: indices(fields[4]),
                };
                assert_eq!(analyze(&graph), Ok(expected), "{}", fields[0]);
            }
        }

        /// Invalid references remain distinct from a valid graph containing a cycle.
        #[test]
        fn graph_shape_is_separate_from_cycles() {
            let graph = vec![vec![1, 1, 99], vec![]];
            assert_eq!(
                analyze(&graph),
                Err(Error::InvalidDependency {
                    node: 0,
                    dependency: 99
                })
            );
            assert_eq!(
                analyze(&[vec![0]]),
                Ok(Analysis {
                    cycle: Some(vec![0, 0]),
                    order: vec![]
                })
            );
        }

        /// A deep graph does not consume the host stack, and reuse has fresh state.
        #[test]
        fn deep_graphs_and_repeated_analysis_are_independent() {
            let count = 4096;
            let mut graph: Vec<_> = (0..count)
                .map(|node| {
                    if node + 1 < count {
                        vec![node + 1]
                    } else {
                        vec![]
                    }
                })
                .collect();
            let expected = Analysis {
                cycle: None,
                order: (0..count).rev().collect(),
            };
            for _ in 0..2 {
                assert_eq!(analyze(&graph), Ok(expected.clone()));
            }
            graph[count - 1].push(0);
            let mut cycle: Vec<_> = (0..count).collect();
            cycle.push(0);
            assert_eq!(
                analyze(&graph),
                Ok(Analysis {
                    cycle: Some(cycle),
                    order: vec![]
                })
            );
            graph[count - 1].clear();
            assert_eq!(analyze(&graph), Ok(expected));
        }
    }
    mod intermediate_match_values {
        //! Independent normalized-document admission, with no engine or host effects.
        use crate::support::document::Tree;
        use crate::support::resolved;
        use yamaa_core::{
            dataset::{MatchRead, MatchValue},
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::{BindError, PrepareError, PreparedSpecification},
            table::{Column, TableSchema, ValueRef},
            value::{compare_present_refs, ColumnType as T, Value},
        };

        fn doc(key: Tree) -> SpecificationDocument {
            let column = |name, kind, variable| {
                Tree::map(vec![
                    ("name", Tree::text(name)),
                    ("type", Tree::text(kind)),
                    (
                        "derivation",
                        Tree::map(vec![(
                            "value",
                            Tree::map(vec![(
                                "source",
                                Tree::map(vec![("variable", Tree::text(variable))]),
                            )]),
                        )]),
                    ),
                ])
            };
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("OUT")),
                ("base", Tree::text("SRC")),
                (
                    "input",
                    Tree::map(vec![
                        (
                            "SRC",
                            Tree::map(vec![
                                ("path", Tree::text("src.csv")),
                                ("types", Tree::map(vec![("N", Tree::text("int"))])),
                            ]),
                        ),
                        ("TAB", Tree::map(vec![("path", Tree::text("tab.csv"))])),
                    ]),
                ),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("LOOK")),
                        ("dataset", Tree::text("TAB")),
                        ("key", key),
                        (
                            "no_match",
                            Tree::map(vec![("literal", Tree::Scalar(N::Null))]),
                        ),
                    ])]),
                ),
                (
                    "columns",
                    Tree::List(vec![
                        column("ID", "str", "SRC.ID"),
                        column("N", "int", "SRC.N"),
                        column("V", "str", "LOOK.VAL"),
                    ]),
                ),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("out.csv")),
                        (
                            "columns",
                            Tree::List(vec![Tree::text("ID"), Tree::text("N"), Tree::text("V")]),
                        ),
                    ]),
                ),
            ]);
            let mut nodes = Vec::new();
            let root = tree.append(&mut nodes);
            SpecificationDocument::admit(Document::new(nodes, root).unwrap())
                .unwrap()
                .unwrap()
        }
        fn schemas() -> (TableSchema, TableSchema) {
            let schema = |fields: &[(&str, T)]| {
                TableSchema::new(
                    fields
                        .iter()
                        .map(|(name, kind)| Column {
                            name: (*name).into(),
                            kind: *kind,
                        })
                        .collect(),
                )
                .unwrap()
            };
            (
                schema(&[("ID", T::Str), ("N", T::Int)]),
                schema(&[
                    ("K", T::Str),
                    ("KINT", T::Int),
                    ("KNUM", T::Float),
                    ("VAL", T::Str),
                ]),
            )
        }
        fn pad<'a>(source: &'a str, width: &'a str) -> Tree {
            Tree::map(vec![(
                "str_pad",
                Tree::map(vec![
                    ("source", Tree::text(source)),
                    ("width", Tree::Scalar(N::Integer(width.into()))),
                ]),
            )])
        }
        fn finding_ids(key: Tree) -> Vec<(String, Vec<String>)> {
            let prepared = PreparedSpecification::prepare(&doc(key)).unwrap();
            let (src, tab) = schemas();
            let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &tab]) else {
                panic!("invalid")
            };
            findings
                .iter()
                .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                .map(|d| (d.code.definition().rule.unwrap().to_string(), d.spec_paths))
                .collect()
        }
        #[test]
        fn padding_and_source_values_have_no_destination_and_preserve_dependency_scope() {
            for source in ["N", "SRC.N"] {
                let prepared =
                    PreparedSpecification::prepare(&doc(Tree::map(vec![("K", pad(source, "8"))])))
                        .unwrap();
                let (src, tab) = schemas();
                let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
                let keys = &resolved::intermediates(&plan)[0].keys;
                assert_eq!(keys.len(), 1);
                assert_eq!(keys[0].source_column, 0);
                assert_eq!(keys[0].path, "intermediates[0].key.K");
                let MatchValue::StrPad {
                    read,
                    operation: Some(op),
                } = &keys[0].value
                else {
                    panic!("owned padding")
                };
                assert_eq!(op.source(), source);
                assert_eq!(
                    matches!(read, MatchRead::Output { column: 1, .. }),
                    source == "N"
                );
                assert_eq!(resolved::output(&plan).columns().len(), 3);
            }
            let prepared = PreparedSpecification::prepare(&doc(Tree::map(vec![(
                "KNUM",
                Tree::map(vec![("source", Tree::text("SRC.N"))]),
            )])))
            .unwrap();
            let (src, tab) = schemas();
            let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
            assert!(matches!(
                &resolved::intermediates(&plan)[0].keys[0].value,
                MatchValue::Source(MatchRead::Record { column: 1, .. })
            ));
        }
        #[test]
        fn authored_type_donor_width_reference_then_dependent_read_failure_order() {
            for (field, first) in [
                ("K", None),
                ("KINT", Some("LOOKUP-9")),
                ("ABSENT", Some("LOOKUP-5")),
            ] {
                let actual = finding_ids(Tree::map(vec![(field, pad("SRC.ABSENT", "0"))]));
                let mut expected = Vec::new();
                if let Some(id) = first {
                    expected.push((id.into(), vec!["intermediates[0].key".into()]));
                }
                expected.extend([
                    (
                        "TEXT-2".into(),
                        vec![format!("intermediates[0].key.{field}.str_pad.width")],
                    ),
                    (
                        "LOOKUP-5".into(),
                        vec![format!("intermediates[0].key.{field}")],
                    ),
                    ("NAME-3".into(), vec!["columns.V.derivation.source".into()]),
                ]);
                assert_eq!(actual, expected);
            }
            assert_eq!(
                finding_ids(Tree::map(vec![("K", pad("SRC.N", "0"))])),
                [(
                    "TEXT-2".into(),
                    vec!["intermediates[0].key.K.str_pad.width".into()]
                )]
            );
        }
        /// LOOKUP-6 compares each key expression's result type (EXPR-14) with
        /// its key column before matching: a fixed, parameter or input type
        /// alike, and a type decided per value at run time.
        #[test]
        fn key_expressions_compare_by_their_one_result_type_before_matching() {
            let compute = |expr: &str| {
                Tree::map(vec![(
                    "compute",
                    Tree::map(vec![("expr", Tree::text(expr))]),
                )])
            };
            let mapping = |entries: Vec<(&str, Tree)>| {
                Tree::map(vec![(
                    "mapping",
                    Tree::map(vec![
                        ("source", Tree::text("SRC.ID")),
                        ("dict", Tree::map(entries)),
                    ]),
                )])
            };
            let greatest = |sources: &[&str]| {
                Tree::map(vec![(
                    "greatest",
                    Tree::map(vec![(
                        "sources",
                        Tree::List(sources.iter().map(Tree::text).collect()),
                    )]),
                )])
            };
            let refused = |field: &str, value: Tree| {
                let prepared =
                    PreparedSpecification::prepare(&doc(Tree::map(vec![(field, value)]))).unwrap();
                let (src, tab) = schemas();
                let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &tab]) else {
                    panic!("{field}: a key whose type cannot compare is refused")
                };
                let diagnostic = findings
                    .iter()
                    .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                    .next()
                    .unwrap();
                (
                    diagnostic.definition().condition,
                    diagnostic.spec_paths,
                    diagnostic.context["expected"].clone(),
                    diagnostic.context["actual"].clone(),
                )
            };
            let text = |value: &str| yamaa_core::json::Value::String(value.into());
            for (field, value, expected) in [
                ("K", compute("N + 1"), "int"),
                ("K", compute("N / 2"), "float"),
                ("K", mapping(vec![("one", Tree::integer(1))]), "int"),
                ("K", greatest(&["N", "SRC.N"]), "int"),
                ("KINT", mapping(vec![("one", Tree::text("1"))]), "str"),
            ] {
                assert_eq!(
                    refused(field, value),
                    (
                        "incompatible_input_type",
                        vec!["intermediates[0].key".to_string()],
                        text(expected),
                        text(if field == "K" { "str" } else { "int" }),
                    ),
                    "{field}"
                );
            }
            for (field, value) in [
                ("KINT", compute("N + 1")),
                ("KNUM", compute("N + 1")),
                ("KNUM", greatest(&["N", "SRC.N"])),
                // An int or a string, decided per value: compared when matched.
                (
                    "KINT",
                    mapping(vec![("one", Tree::integer(1)), ("two", Tree::text("2"))]),
                ),
            ] {
                let prepared =
                    PreparedSpecification::prepare(&doc(Tree::map(vec![(field, value)]))).unwrap();
                let (src, tab) = schemas();
                prepared
                    .bind_sources(&[&src, &tab])
                    .unwrap_or_else(|error| panic!("{field}: {error:?}"));
            }
        }
        #[test]
        fn literal_text_is_not_a_reference_and_boolean_family_is_not_coerced() {
            let prepared = PreparedSpecification::prepare(&doc(Tree::map(vec![(
                "K",
                Tree::map(vec![("literal", Tree::text("SRC.ABSENT"))]),
            )])))
            .unwrap();
            let (src, tab) = schemas();
            let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
            assert_eq!(
                resolved::intermediates(&plan)[0].keys[0].value,
                MatchValue::Literal(Value::Str("SRC.ABSENT".into()))
            );
            assert_eq!(
                finding_ids(Tree::map(vec![(
                    "KINT",
                    Tree::map(vec![("literal", Tree::Scalar(N::Boolean(true)))])
                )]))[0]
                    .0,
                "LOOKUP-9"
            );
        }
        #[test]
        fn fixed_intermediate_key_count_precedes_deferred_name_findings() {
            let keys = (0..65).map(|index| format!("K{index}")).collect::<Vec<_>>();
            for sequence in [false, true] {
                let document = |count| {
                    let selected = keys.iter().take(count);
                    let key = if sequence {
                        Tree::List(selected.map(Tree::text).collect())
                    } else {
                        Tree::map(
                            selected
                                .map(|name| {
                                    (
                                        name.as_str(),
                                        Tree::map(vec![(
                                            "literal",
                                            Tree::Scalar(N::Integer("1".into())),
                                        )]),
                                    )
                                })
                                .collect(),
                        )
                    };
                    doc(key)
                };
                PreparedSpecification::prepare(&document(64)).unwrap();
                let Err(PrepareError::Limit(refusal)) =
                    PreparedSpecification::prepare(&document(65))
                else {
                    panic!("fixed intermediate key boundary")
                };
                assert_eq!(refusal.resource, "intermediate_keys");
                assert_eq!(refusal.limit, 64);
                assert_eq!(refusal.actual, Some(65));
                assert_eq!(refusal.spec_paths, ["intermediates[0].key"]);
            }
        }

        #[test]
        fn unsupported_operations_keep_the_authored_gate_path() {
            let (key, operation, path) = (
                Tree::map(vec![(
                    "K",
                    Tree::map(vec![("vendor_extension", Tree::text("unrecognized"))]),
                )]),
                "vendor_extension",
                "intermediates[0].key.K.vendor_extension",
            );
            let Err(PrepareError::Unsupported(features)) =
                PreparedSpecification::prepare(&doc(key))
            else {
                panic!("explicit unsupported gate")
            };
            assert!(features
                .iter()
                .any(|f| f.operation == operation && f.path == path));
        }
        #[test]
        fn borrowed_equality_preserves_complete_integer_precision_and_missingness() {
            use std::cmp::Ordering::*;
            for (left, right, result) in [
                (Value::Int(7), Value::float(7.), Equal),
                (
                    Value::Int(9007199254740993),
                    Value::float(9007199254740992.),
                    Greater,
                ),
                (
                    Value::Int(i64::MAX),
                    Value::float(9223372036854775808.),
                    Less,
                ),
                (
                    Value::Int(i64::MIN),
                    Value::float(-9223372036854775808.),
                    Equal,
                ),
                (Value::float(-0.), Value::float(0.), Equal),
            ] {
                assert_eq!(
                    compare_present_refs(ValueRef::from(&left), ValueRef::from(&right)),
                    Ok(result)
                );
            }
            assert!(compare_present_refs(ValueRef::Missing, ValueRef::Int(0)).is_err());
        }

        #[test]
        fn scalar_keys_retain_all_current_operands_without_synthetic_dependencies_or_destinations()
        {
            let prepared = PreparedSpecification::prepare(&doc(Tree::map(vec![(
                "KINT",
                Tree::map(vec![(
                    "greatest",
                    Tree::map(vec![(
                        "sources",
                        Tree::List(vec![Tree::text("N"), Tree::text("SRC.N"), Tree::text("N")]),
                    )]),
                )]),
            )])))
            .unwrap();
            let (src, tab) = schemas();
            let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
            assert_eq!(
                resolved::output(&plan)
                    .columns()
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "N", "V"]
            );
            let item = resolved::intermediates(&plan)[0];
            assert_eq!(item.output_dependencies().collect::<Vec<_>>(), [1]);
            let MatchValue::Expression(expression) = &item.keys[0].value else {
                panic!("bound shared scalar")
            };
            assert_eq!(
                expression.reads,
                [
                    MatchRead::Output {
                        column: 1,
                        identifier: "N".into()
                    },
                    MatchRead::Record {
                        column: 1,
                        identifier: "SRC.N".into()
                    },
                ]
            );
            assert_eq!(
                expression.assignment.path,
                "intermediates[0].key.KINT.greatest"
            );
            assert_eq!(
                expression.result_type,
                yamaa_core::result_type::ResultType::INT,
                "greatest returns the type its sources share (EXPR-14)"
            );
            assert!(!expression.windowed);
        }
    }
    mod intermediate_reference {
        use yamaa_core::{
            intermediate_reference::{
                validate_field, validate_read, Finding as F, Intermediate, Read, Source,
            },
            reference_binding::{Catalog, Dataset, Field},
            value::ColumnType,
        };

        /// Authored source metadata includes a literal dotted field and a Unicode field.
        fn catalog() -> Catalog {
            Catalog::compile(
                &[],
                &[Dataset {
                    name: "SRC",
                    fields: &[
                        Field {
                            name: "K",
                            column_type: ColumnType::Str,
                        },
                        Field {
                            name: "N",
                            column_type: ColumnType::Int,
                        },
                        Field {
                            name: "X.Y",
                            column_type: ColumnType::Str,
                        },
                        Field {
                            name: "é",
                            column_type: ColumnType::Str,
                        },
                    ],
                }],
            )
            .unwrap()
        }

        /// A planned lookup adds one field while otherwise exposing its stored columns.
        fn target() -> Intermediate<'static> {
            Intermediate {
                source: Source::Dataset("SRC"),
                derived: &["D"],
                readable: &[],
                dependencies: &[],
            }
        }

        /// LOOKUP-11 expectations come from authored visibility, not generated runtime results.
        #[test]
        fn stored_derived_and_self_visibility() {
            let catalog = catalog();
            let all = target();
            let selected = Intermediate {
                readable: &["K", "D", "ABSENT"],
                ..all
            };
            let own = Intermediate {
                source: Source::SelfFields(&["K"]),
                ..all
            };
            let absent = Intermediate {
                source: Source::Dataset("ABSENT"),
                ..all
            };
            for (field, target, finding) in [
                ("K", all, None),
                ("D", all, None),
                ("N", all, None),
                ("X.Y", all, None),
                ("é", all, None),
                ("É", all, Some(F::UnknownField)),
                ("ABSENT", all, Some(F::UnknownField)),
                ("K", selected, None),
                ("D", selected, None),
                ("N", selected, Some(F::UnknownField)),
                ("ABSENT", selected, Some(F::UnknownField)),
                ("K", own, None),
                ("D", own, None),
                ("N", own, Some(F::UnknownField)),
                (
                    "K",
                    Intermediate {
                        readable: &["D"],
                        ..own
                    },
                    Some(F::UnknownField),
                ),
                ("ABSENT", absent, None),
                (
                    "N",
                    Intermediate {
                        derived: &["N"],
                        ..all
                    },
                    None,
                ),
            ] {
                assert_eq!(
                    validate_field(&catalog, field, target),
                    finding,
                    "{field}, {target:?}"
                );
            }
        }

        /// Donor records expose stored and earlier-derived names; dependencies keep declaration order.
        #[test]
        fn donor_scope_phase_and_priority() {
            let catalog = catalog();
            let target = Intermediate {
                dependencies: &[
                    "K",
                    "DONOR.K",
                    "DONOR.X.Y",
                    "DONOR.D",
                    "OTHER.K",
                    "LATER",
                    "OTHER.K",
                ],
                ..target()
            };
            let read = Read {
                reader: "reader",
                target_name: "lookup",
                target: Some(target),
                field: "K",
                donor_dataset: "DONOR",
                visible: &["K", "X.Y", "D"],
            };
            let missing = vec![
                F::UnavailableDependency { dependency: 4 },
                F::UnavailableDependency { dependency: 5 },
                F::UnavailableDependency { dependency: 6 },
            ];
            for (read, expected) in [
                (read, missing.clone()),
                (
                    Read {
                        field: "ABSENT",
                        ..read
                    },
                    [vec![F::UnknownField], missing.clone()].concat(),
                ),
                (
                    Read {
                        target: None,
                        ..read
                    },
                    vec![],
                ),
                (
                    Read {
                        reader: "lookup",
                        ..read
                    },
                    vec![],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            source: Source::SelfFields(&["K"]),
                            ..target
                        }),
                        ..read
                    },
                    vec![F::SelfPhase],
                ),
                (
                    Read {
                        reader: "lookup",
                        target: Some(Intermediate {
                            source: Source::SelfFields(&[]),
                            ..target
                        }),
                        ..read
                    },
                    vec![],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            dependencies: &[],
                            ..target
                        }),
                        field: "D",
                        ..read
                    },
                    vec![],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            source: Source::Dataset("ABSENT"),
                            dependencies: &[],
                            ..target
                        }),
                        field: "D",
                        ..read
                    },
                    vec![],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            source: Source::Dataset("ABSENT"),
                            dependencies: &[],
                            ..target
                        }),
                        ..read
                    },
                    vec![F::UnknownField],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            readable: &["D"],
                            dependencies: &[],
                            ..target
                        }),
                        ..read
                    },
                    vec![F::UnknownField],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            dependencies: &["DONOR.X.Y"],
                            ..target
                        }),
                        visible: &["X", "Y"],
                        ..read
                    },
                    vec![F::UnavailableDependency { dependency: 0 }],
                ),
                (
                    Read {
                        target: Some(Intermediate {
                            dependencies: &["é", "DONOR.é"],
                            ..target
                        }),
                        visible: &["é", "é"],
                        ..read
                    },
                    vec![],
                ),
            ] {
                assert_eq!(validate_read(&catalog, read), expected, "{read:?}");
            }
        }

        /// Self-reads stay deferred to cycle analysis, and Unicode visibility has no shared state.
        #[test]
        fn self_reads_and_unicode_visibility_are_reusable() {
            let catalog = catalog();
            let target = Intermediate {
                source: Source::Dataset("SRC"),
                derived: &[],
                readable: &[],
                dependencies: &[],
            };
            assert_eq!(validate_field(&catalog, "é", target), None);
            let read = Read {
                reader: "same",
                target_name: "same",
                target: None,
                field: "K",
                donor_dataset: "D",
                visible: &["é"],
            };
            assert_eq!(validate_read(&catalog, read), vec![]);
            assert_eq!(validate_field(&catalog, "é", target), None);
        }
    }
    mod reference_binding {
        use yamaa_core::reference_binding::{Binding, Catalog, Dataset, Diagnostic, Error, Field};
        use yamaa_core::value::ColumnType::{self, Date, Float, Int, Str};

        /// Named typed declarations, independent of records or any host table representation.
        fn field(name: &str, column_type: ColumnType) -> Field<'_> {
            Field { name, column_type }
        }

        /// Copy metadata from short-lived buffers so every test also exercises catalog ownership.
        fn catalog() -> Catalog {
            let outputs = [
                field("OUT", Str),
                field("X", Int),
                field("WHEN", Date),
                field("β", Float),
            ];
            let z = [
                field("X", Int),
                field("F", Str),
                field("IT.X", Str),
                field("é", Str),
            ];
            let a = [field("X", Float), field("A", Date), field("F", Int)];
            let alpha = [field("X", Str)];
            let datasets = [
                Dataset {
                    name: "Z",
                    fields: &z,
                },
                Dataset {
                    name: "A",
                    fields: &a,
                },
                Dataset {
                    name: "α",
                    fields: &alpha,
                },
            ];
            Catalog::compile(&outputs, &datasets).unwrap()
        }

        /// Dataset spelling is exact; bare outputs win without implicit source or ODM resolution.
        #[test]
        fn exact_names_and_declared_types() {
            let catalog = catalog();
            for (name, expected) in [
                (
                    "OUT",
                    Some(Binding::Output {
                        column: 0,
                        column_type: Str,
                    }),
                ),
                (
                    "X",
                    Some(Binding::Output {
                        column: 1,
                        column_type: Int,
                    }),
                ),
                (
                    "WHEN",
                    Some(Binding::Output {
                        column: 2,
                        column_type: Date,
                    }),
                ),
                (
                    "β",
                    Some(Binding::Output {
                        column: 3,
                        column_type: Float,
                    }),
                ),
                (
                    "Z.X",
                    Some(Binding::Dataset {
                        dataset: 0,
                        field: 0,
                        column_type: Int,
                    }),
                ),
                (
                    "A.X",
                    Some(Binding::Dataset {
                        dataset: 1,
                        field: 0,
                        column_type: Float,
                    }),
                ),
                (
                    "Z.IT.X",
                    Some(Binding::Dataset {
                        dataset: 0,
                        field: 2,
                        column_type: Str,
                    }),
                ),
                (
                    "Z.é",
                    Some(Binding::Dataset {
                        dataset: 0,
                        field: 3,
                        column_type: Str,
                    }),
                ),
                (
                    "α.X",
                    Some(Binding::Dataset {
                        dataset: 2,
                        field: 0,
                        column_type: Str,
                    }),
                ),
                ("F", None),
                ("OUT.X", None),
                ("DOMAIN.OUT", None),
                ("Z.IT.Y", None),
                ("Z.e\u{301}", None),
                ("z.X", None),
                ("Z.x", None),
                ("Z.", None),
                ("", None),
                (".X", None),
            ] {
                assert_eq!(catalog.bind(name), expected, "{name}");
            }
        }

        /// Unknown names precede phase failures, which precede strict expected-type checks.
        #[test]
        fn output_conditions_and_suggestion_order() {
            let catalog = catalog();
            for (name, expected, available, candidates, diagnostic) in [
                ("OUT", Some(Str), None, vec![], None),
                (
                    "X",
                    Some(Float),
                    None,
                    vec![],
                    Some(Diagnostic::IncompatibleInputType {
                        expected: Float,
                        actual: Int,
                    }),
                ),
                (
                    "X",
                    Some(Float),
                    Some(vec![0]),
                    vec![],
                    Some(Diagnostic::PhaseBoundary { column: 1 }),
                ),
                ("X", Some(Int), Some(vec![1]), vec![], None),
                (
                    "UNKNOWN",
                    Some(Int),
                    Some(vec![]),
                    vec![2, 0, 1],
                    Some(Diagnostic::UnknownField),
                ),
                (
                    "F",
                    Some(Int),
                    Some(vec![]),
                    vec![0, 1],
                    Some(Diagnostic::UnresolvableName { dataset: 1 }),
                ),
                (
                    "F",
                    None,
                    None,
                    vec![0],
                    Some(Diagnostic::UnresolvableName { dataset: 0 }),
                ),
                ("F", None, None, vec![2], Some(Diagnostic::UnknownField)),
                ("F", None, None, vec![], Some(Diagnostic::UnknownField)),
                ("β", Some(Float), Some(vec![3]), vec![], None),
            ] {
                assert_eq!(
                    catalog.validate_output(name, expected, available.as_deref(), &candidates),
                    Ok(diagnostic),
                    "{name}"
                );
            }
        }

        /// Large candidate sets keep lexical selection, duplicate admission and per-query isolation.
        #[test]
        fn large_candidate_membership_preserves_suggestions() {
            let names: Vec<_> = (0..256).rev().map(|index| format!("D{index:03}")).collect();
            let fields = [field("F", Str)];
            let datasets: Vec<_> = names
                .iter()
                .map(|name| Dataset {
                    name,
                    fields: &fields,
                })
                .collect();
            let catalog = Catalog::compile(&[], &datasets).unwrap();
            let candidates: Vec<_> = (0..256).collect();
            assert_eq!(
                catalog.validate_output("ABSENT", None, None, &candidates),
                Ok(Some(Diagnostic::UnknownField))
            );
            assert_eq!(
                catalog.validate_output("F", None, None, &candidates),
                Ok(Some(Diagnostic::UnresolvableName { dataset: 255 }))
            );
            assert_eq!(
                catalog.validate_output("F", None, None, &[0; 256]),
                Ok(Some(Diagnostic::UnresolvableName { dataset: 0 }))
            );
            assert_eq!(
                catalog.validate_output("F", None, None, &[]),
                Ok(Some(Diagnostic::UnknownField))
            );
            assert_eq!(
                catalog.validate_output("F", None, None, &[0; 257]),
                Ok(Some(Diagnostic::UnresolvableName { dataset: 0 }))
            );
        }

        /// Invalid metadata keeps its structural cause and never poisons later catalogs.
        #[test]
        fn catalog_shape_and_reuse() {
            let outputs = [field("X", Int), field("X", Int)];
            assert!(matches!(
                Catalog::compile(&outputs, &[]),
                Err(Error::DuplicateOutput)
            ));
            assert!(matches!(
                Catalog::compile(&[field("", Int)], &[]),
                Err(Error::EmptyName)
            ));
            let duplicates = [
                Dataset {
                    name: "D",
                    fields: &[],
                },
                Dataset {
                    name: "D",
                    fields: &[],
                },
            ];
            assert!(matches!(
                Catalog::compile(&[], &duplicates),
                Err(Error::DuplicateDataset)
            ));
            assert!(matches!(
                Catalog::compile(
                    &[],
                    &[Dataset {
                        name: "D",
                        fields: &outputs
                    }]
                ),
                Err(Error::DuplicateField)
            ));
            let catalog = catalog();
            assert_eq!(
                catalog.bind("X"),
                Some(Binding::Output {
                    column: 1,
                    column_type: Int
                })
            );
        }

        /// Bad context indices are metadata errors and never turn into a plausible name failure.
        #[test]
        fn query_admission_is_separate_and_stateless() {
            let catalog = catalog();
            for _ in 0..2 {
                assert_eq!(
                    catalog.validate_output("F", None, Some(&[4]), &[]),
                    Err(Error::InvalidAvailableOutput)
                );
                assert_eq!(
                    catalog.validate_output("F", None, None, &[3]),
                    Err(Error::InvalidCandidateDataset)
                );
                assert_eq!(
                    catalog.validate_output("Z.X", None, None, &[]),
                    Err(Error::QualifiedOutputQuery)
                );
                assert_eq!(
                    catalog.validate_output("OUT", None, Some(&[]), &[]),
                    Ok(Some(Diagnostic::PhaseBoundary { column: 0 }))
                );
                assert_eq!(
                    catalog.validate_output("OUT", None, Some(&[0]), &[]),
                    Ok(None)
                );
            }
            let unicode = Catalog::compile(&[field("é", Str)], &[]).unwrap();
            assert_eq!(
                unicode.bind("é"),
                Some(Binding::Output {
                    column: 0,
                    column_type: Str,
                })
            );
            assert_eq!(unicode.validate_output("é", None, None, &[]), Ok(None));
        }

        /// Whole-relation binding preserves declaration indices, including empty schemas and exact Unicode names.
        #[test]
        fn relation_identity_is_not_a_stored_field() {
            let catalog = Catalog::compile(
                &[field("OUT", Str)],
                &[
                    Dataset {
                        name: "EMPTY",
                        fields: &[],
                    },
                    Dataset {
                        name: "A.B",
                        fields: &[],
                    },
                ],
            )
            .unwrap();
            assert_eq!(catalog.bind_relation("EMPTY"), Some(0));
            assert_eq!(catalog.bind_relation("A.B"), Some(1));
            assert_eq!(catalog.bind_relation("OUT"), None);
            assert_eq!(catalog.bind_relation(""), None);
            assert_eq!(catalog.bind("EMPTY"), None);
            assert_eq!(catalog.bind_relation("\u{e9}\u{e9}\u{e9}"), None);
            assert_eq!(catalog.bind_relation("EMPTY"), Some(0));
        }
    }
}
