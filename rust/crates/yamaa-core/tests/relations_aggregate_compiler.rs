//! Relations contracts.
#[allow(dead_code)]
mod support;
mod relations {
    mod aggregate_compiler {
        use yamaa_core::{
            aggregate_compiler::{compile_aggregate, AggregateOperand, AggregateResult},
            evaluation::NumericResolver,
            numeric::Number,
            value::{Selection, Value},
        };

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
                let AggregateOperand::Numeric(operand) = &reduction.operand else {
                    panic!("numeric aggregate operand")
                };
                assert_eq!(operand.expression(), expression);
                assert_eq!(operand.spec_path(), path);
            }
            assert_eq!(expression.matches("SUM(").count(), 512);
        }

        #[test]
        fn a_bare_identifier_outside_reduction_uses_numeric_coercion() {
            let plan = compile_aggregate("A", "columns.VALUE.aggregate").unwrap();
            let AggregateResult::Numeric(result) = plan.result() else {
                panic!("outer expressions use the numeric language")
            };
            assert_eq!(
                result
                    .evaluate(&mut Resolver { calls: Vec::new() })
                    .unwrap(),
                Number::Int(7)
            );
            assert_eq!(plan.ungrouped_identifiers().collect::<Vec<_>>(), ["A"]);
        }
        impl NumericResolver for Resolver {
            type Error = std::convert::Infallible;
            fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                self.calls.push(name.into());
                Ok(Selection::Present(Value::Int(match name {
                    "EX.DOSE" => 3,
                    "SUM(EX.DOSE * 2)" => 6,
                    "MAX(EX.DOSE)" => 4,
                    "A" => 7,
                    _ => panic!("unbound aggregate resolver identity {name}"),
                })))
            }
        }

        #[test]
        fn outer_and_per_record_arithmetic_share_the_numeric_evaluator_without_rewriting() {
            let expression = "POWER(SUM(EX.DOSE * 2), 2) + MAX(EX.DOSE)";
            let plan = compile_aggregate(expression, "columns.VALUE.derivation.aggregate").unwrap();
            assert_eq!(plan.identifiers().collect::<Vec<_>>(), ["EX.DOSE"]);
            let AggregateOperand::Numeric(operand) = &plan.reductions()[0].operand else {
                panic!()
            };
            let mut resolver = Resolver { calls: Vec::new() };
            assert_eq!(operand.evaluate(&mut resolver).unwrap(), Number::Int(6));
            assert_eq!(operand.expression(), expression);
            let AggregateResult::Numeric(result) = plan.result() else {
                panic!()
            };
            assert_eq!(result.evaluate(&mut resolver).unwrap(), Number::float(40.0));
            assert_eq!(
                resolver.calls,
                ["EX.DOSE", "SUM(EX.DOSE * 2)", "MAX(EX.DOSE)"]
            );
            assert_eq!(result.expression(), expression);
        }

        #[test]
        fn bare_reducers_retain_typed_reads_and_record_stars() {
            let plan =
                compile_aggregate("(ONLY(EX.TEXT))", "rows[0].derivations.TEXT.aggregate").unwrap();
            assert_eq!(plan.result(), &AggregateResult::Reduction(0));
            assert_eq!(
                plan.reductions()[0].operand,
                AggregateOperand::Value("EX.TEXT".into())
            );
            let count = compile_aggregate("COUNT(EX.*)", "columns.N.aggregate").unwrap();
            assert_eq!(count.record_relations().collect::<Vec<_>>(), ["EX"]);
            assert_eq!(
                count.reductions()[0].operand,
                AggregateOperand::Records("EX".into())
            );
            let group = compile_aggregate("KEY + SUM(EX.DOSE)", "columns.X.aggregate").unwrap();
            assert_eq!(group.ungrouped_identifiers().collect::<Vec<_>>(), ["KEY"]);
        }

        #[test]
        fn failures_preserve_full_authored_expression_and_inner_span() {
            let expression = "SUM(A / 0) + 1";
            let plan = compile_aggregate(expression, "columns.X.aggregate").unwrap();
            let AggregateOperand::Numeric(operand) = &plan.reductions()[0].operand else {
                panic!()
            };
            let error = operand
                .evaluate(&mut Resolver { calls: Vec::new() })
                .unwrap_err();
            assert_eq!(error.evaluation.location.expression, expression);
            assert_eq!(
                &expression[error.source_span.start..error.source_span.end],
                "A / 0"
            );
            assert_eq!(error.evaluation.location.spec_path, "columns.X.aggregate");
            let complete = compile_aggregate("SUM(A + B) + SUM(C)", "columns.X.aggregate").unwrap();
            assert_eq!(complete.reductions().len(), 2);
            assert_eq!(complete.identifiers().collect::<Vec<_>>(), ["A", "B", "C"]);
            assert!(matches!(
                compile_aggregate("SUM(MEAN(A))", "columns.X.aggregate"),
                Err(yamaa_core::aggregate_parser::ParseError::Grammar { .. })
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
                    "REQ-0072",
                    "operation",
                    vec![1, 2, 1],
                ),
                (
                    Diagnostic::ForwardReference {
                        column: 0,
                        dependency: 2,
                    },
                    "forward_reference",
                    "REQ-0071",
                    "operation",
                    vec![0, 2],
                ),
                (
                    Diagnostic::MissingKeyDerivation { column: 2 },
                    "key_dependency",
                    "REQ-0074",
                    "declaration",
                    vec![2],
                ),
                (
                    Diagnostic::KeyDependency {
                        column: 2,
                        dependency: 0,
                    },
                    "key_dependency",
                    "REQ-0074",
                    "expression",
                    vec![2, 0],
                ),
            ];
            for (diagnostic, condition, requirement, location, columns) in cases {
                assert_eq!(
                    (
                        diagnostic.condition(),
                        diagnostic.requirement(),
                        diagnostic.location(),
                        diagnostic.columns()
                    ),
                    (condition, requirement, location, columns)
                );
            }
        }
    }
    mod dataset_compiler {
        //! The temporary decoded vocabulary compiles in core, without an engine or IO.
        use crate::support::int_schema as schema;
        use yamaa_core::{
            bound_expression::{Binding, Read},
            dataset::{
                compile_dataset, AssignmentDeclaration as A, DatasetCompileError as Error,
                DatasetDeclaration, Expression, ExpressionDeclaration as D, FunctionArgument,
                FunctionInput, PlanError, RowMode, SourceSchemas, TemplateDeclaration, Window,
                WindowKind,
            },
            function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
            value::{ColumnType, Value, ValueType},
        };

        fn assignment(column: usize, expression: D) -> A {
            A {
                column,
                path: format!("columns.C{column}.derivation"),
                expression,
            }
        }
        fn declaration(expression: D) -> DatasetDeclaration {
            DatasetDeclaration {
                sources: SourceSchemas {
                    primary: schema(&["ID"]),
                    secondary: vec![],
                },
                intermediates: vec![],
                output: schema(&["ID", "VALUE"]),
                templates: vec![TemplateDeclaration {
                    mode: RowMode::Keys,
                    assignments: vec![assignment(0, D::Source(0))],
                    filter: None,
                }],
                columns: vec![assignment(1, expression)],
                keys: vec![0],
                verifications: vec![],
                conversion_handlers: vec![],
                functions: vec![],
            }
        }
        fn compute(text: &str, name: &str, read: Read) -> D {
            D::Compute {
                text: text.into(),
                bindings: vec![Binding {
                    name: name.into(),
                    read,
                }],
            }
        }
        fn signature() -> InvocationPlan {
            InvocationPlan::new(
                FunctionIdentity {
                    name: "identity".into(),
                    contract_version: "1".into(),
                    implementation_version: "1".into(),
                    call: "identity".into(),
                },
                vec![Parameter {
                    name: "value".into(),
                    host_name: "x".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                }],
                ColumnType::Int,
                false,
            )
            .unwrap()
        }
        fn call(slot: usize) -> D {
            D::Function {
                slot,
                arguments: vec![FunctionArgument {
                    name: "value".into(),
                    input: FunctionInput::Read(Read::Column(0)),
                }],
            }
        }

        #[test]
        fn compiling_preserves_deferred_arithmetic_and_authored_path() {
            let compiled =
                compile_dataset(declaration(compute("ID + 1 / 0", "ID", Read::Column(0)))).unwrap();
            let Expression::Compute(numeric) = &compiled.plan().columns()[0].expression else {
                panic!("compiled numeric expression")
            };
            assert_eq!(
                numeric.bindings(),
                &[Binding {
                    name: "ID".into(),
                    read: Read::Column(0)
                }]
            );
            assert_eq!(numeric.expression().spec_path(), "columns.C1.derivation");
            // Division by zero remains execution work; compilation grants a valid plan.
            assert_eq!(compiled.plan().columns()[0].column, 1);
            assert!(compiled.functions().is_empty());
        }

        #[test]
        fn grammar_binding_and_phase_errors_are_owned_by_pure_compilation() {
            assert!(matches!(
                compile_dataset(declaration(compute("ID +", "ID", Read::Column(0)))),
                Err(Error::Numeric(
                    yamaa_core::numeric_parser::ParseError::Grammar { .. }
                ))
            ));
            assert!(matches!(
                compile_dataset(declaration(compute("ID + 1", "OTHER", Read::Column(0)))),
                Err(Error::Plan(PlanError::InvalidColumns))
            ));
            assert!(matches!(
                compile_dataset(declaration(compute("ID + 1", "ID", Read::Column(1)))),
                Err(Error::Plan(PlanError::UnavailableColumn))
            ));
            assert!(matches!(
                compile_dataset(declaration(call(0))),
                Err(Error::Plan(PlanError::InvalidFunction))
            ));
        }

        #[test]
        fn row_and_column_function_plans_retain_their_complete_shared_signature() {
            let mut input = declaration(call(0));
            input.functions = vec![signature()];
            input.templates[0].assignments[0].expression = D::Function {
                slot: 0,
                arguments: vec![FunctionArgument {
                    name: "value".into(),
                    input: FunctionInput::Literal(Value::Int(1)),
                }],
            };
            input.templates[0].mode = RowMode::Records;
            let compiled = compile_dataset(input).unwrap();
            let Expression::Function(row) =
                &compiled.plan().templates()[0].assignments[0].expression
            else {
                panic!("compiled row function")
            };
            let Expression::Function(column) = &compiled.plan().columns()[0].expression else {
                panic!("compiled column function")
            };
            assert_eq!(row.slot(), 0);
            assert_eq!(column.slot(), 0);
            assert_eq!(row.signature(), &signature());
            assert_eq!(column.signature(), &signature());
            assert_eq!(compiled.functions(), &[signature()]);
        }

        #[test]
        fn legacy_number_and_value_window_forms_have_one_core_admission_gate() {
            let window = |kind| Window {
                kind,
                group_by: vec![],
                order_by: vec![],
                filter: None,
            };
            assert!(matches!(
                compile_dataset(declaration(D::Number(window(WindowKind::Locf {
                    column: 0,
                    include_current: true,
                })))),
                Err(Error::Plan(PlanError::InvalidWindow))
            ));
            assert!(matches!(
                compile_dataset(declaration(D::Window(window(WindowKind::Dense)))),
                Err(Error::Plan(PlanError::InvalidWindow))
            ));
        }

        #[test]
        fn invalid_recovery_never_exposes_a_partly_admitted_plan() {
            let mut input = declaration(D::Literal(Value::Int(1)));
            input
                .conversion_handlers
                .push(yamaa_core::dataset::ConversionHandler {
                    assignment_path: "columns.ABSENT.derivation".into(),
                    handler: yamaa_core::conversion::LiteralHandler {
                        spec_path: "columns.ABSENT.unconvertible".into(),
                        value: Value::Int(0),
                    },
                });
            assert!(matches!(
                compile_dataset(input),
                Err(Error::Plan(PlanError::InvalidConversionHandler))
            ));
        }

        #[test]
        fn ignored_draft_faults_and_unused_reservations_cannot_publish_a_complete_prefix() {
            use yamaa_core::dataset::{DatasetCompiler, DatasetMetadata};
            let metadata = || {
                let input = declaration(D::Literal(Value::Int(1)));
                DatasetMetadata {
                    sources: input.sources,
                    intermediates: input.intermediates,
                    output: input.output,
                    keys: input.keys,
                    verifications: input.verifications,
                    conversion_handlers: input.conversion_handlers,
                }
            };
            let complete_prefix = || {
                let mut draft = DatasetCompiler::new(vec![signature()]);
                draft.begin_template(RowMode::Keys, None).unwrap();
                draft
                    .template_assignment(assignment(0, D::Source(0)))
                    .unwrap();
                draft
                    .column_assignment(assignment(1, D::Literal(Value::Int(1))))
                    .unwrap();
                draft
            };
            let mut draft = complete_prefix();
            assert!(draft
                .column_assignment(assignment(1, compute("ID +", "ID", Read::Column(0))))
                .is_err());
            assert!(matches!(
                draft.finish(metadata()),
                Err(Error::Plan(PlanError::InvalidColumns))
            ));
            let mut draft = complete_prefix();
            assert!(draft.reserve_function(999).is_err());
            assert!(draft.finish(metadata()).is_err());
            let mut draft = complete_prefix();
            draft.reserve_function(0).unwrap();
            assert!(matches!(
                draft.finish(metadata()),
                Err(Error::Plan(PlanError::InvalidFunction))
            ));
            let mut draft = complete_prefix();
            draft.reserve_numeric("ID + 1", "authored").unwrap();
            assert!(matches!(
                draft.finish(metadata()),
                Err(Error::Plan(PlanError::InvalidColumns))
            ));
            let mut draft = complete_prefix();
            draft.reserve_numeric("ID + 1", "authored").unwrap();
            assert!(draft
                .column_assignment(assignment(1, compute("ID + 1", "ID", Read::Column(0))))
                .is_err());
            assert!(draft.finish(metadata()).is_err());
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
    mod dataset_model {
        //! Admission contracts run with no engine or table port dependency.
        use crate::support::table_schema as schema;
        use yamaa_core::{
            dataset::{
                Assignment, Check, DatasetPlan, Expression, PlanError, RowMode, RowTemplate,
                Verification,
            },
            reduction::NumericReducer,
            value::{ColumnType, Value},
        };

        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("columns.C{column}.derivation"),
            }
        }

        #[test]
        fn implicit_source_joins_admit_completed_output_keys_in_record_and_group_modes_only() {
            use yamaa_core::dataset::{Lookup, MatchKey, SecondarySource};
            let driver = schema(&[("ID", ColumnType::Int)]);
            let output = schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]);
            let donor = SecondarySource {
                name: "D".into(),
                schema: output.clone(),
            };
            let expression = || Expression::SourceLookup {
                lookup: Lookup {
                    source: 0,
                    column: 1,
                    keys: vec![MatchKey {
                        source_column: 0,
                        output_column: 0,
                    }],
                },
                filter: None,
                selection: None,
            };
            for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
                for completed in [true, false] {
                    let result = DatasetPlan::new_with_sources(
                        driver.clone(),
                        vec![donor.clone()],
                        output.clone(),
                        vec![RowTemplate {
                            mode: mode.clone(),
                            assignments: if completed {
                                vec![assign(0, Expression::Source(0))]
                            } else {
                                vec![]
                            },
                            filter: None,
                        }],
                        vec![assign(1, expression())],
                        vec![0],
                        vec![],
                    );
                    if completed {
                        assert!(result.is_ok(), "{result:?}");
                    } else {
                        assert_eq!(result, Err(PlanError::InvalidLookup));
                    }
                }
            }
            let mut wrong = donor;
            wrong.schema = schema(&[("ID", ColumnType::Str), ("VALUE", ColumnType::Int)]);
            assert_eq!(
                DatasetPlan::new_with_sources(
                    driver,
                    vec![wrong],
                    output,
                    vec![RowTemplate {
                        mode: RowMode::Groups(vec![0]),
                        assignments: vec![assign(0, Expression::Source(0))],
                        filter: None
                    }],
                    vec![assign(1, expression())],
                    vec![0],
                    vec![]
                ),
                Err(PlanError::InvalidLookup),
            );
        }

        #[test]
        fn malformed_row_filter_columns_retain_the_existing_scope_error() {
            use yamaa_core::bound_expression::{Binding, BindingError, BoundPredicate, Read};
            let source = schema(&[("ID", ColumnType::Int)]);
            for mode in [RowMode::Records, RowMode::Groups(vec![0]), RowMode::Keys] {
                for column in [1, usize::MAX] {
                    let filter = BoundPredicate::new(
                        yamaa_core::predicate_compiler::compile("F > 0", "row.filter").unwrap(),
                        vec![Binding {
                            name: "F".into(),
                            read: Read::Column(column),
                        }],
                    )
                    .unwrap();
                    assert_eq!(
                        DatasetPlan::new(
                            source.clone(),
                            source.clone(),
                            vec![RowTemplate {
                                mode: mode.clone(),
                                assignments: vec![assign(0, Expression::Source(0))],
                                filter: Some(filter),
                            }],
                            vec![],
                            vec![0],
                            vec![],
                        ),
                        Err(PlanError::Filter(BindingError::UnavailableColumn)),
                    );
                }
            }
        }

        #[test]
        fn record_filters_reject_windows_and_group_filters_admit_completed_windows() {
            use yamaa_core::{
                bound_expression::{Binding, BindingError, BoundPredicate, Read},
                dataset::{OrderTerm, Window, WindowKind},
            };
            let source = schema(&[("ID", ColumnType::Int)]);
            let output = schema(&[
                ("ID", ColumnType::Int),
                ("SEQ", ColumnType::Int),
                ("COPY", ColumnType::Int),
            ]);
            for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
                for column in [0, 1, 2] {
                    let filter = BoundPredicate::new(
                        yamaa_core::predicate_compiler::compile("F > 0", "row.filter").unwrap(),
                        vec![Binding {
                            name: "F".into(),
                            read: Read::Column(column),
                        }],
                    )
                    .unwrap();
                    let plan = DatasetPlan::new(
                        source.clone(),
                        output.clone(),
                        vec![RowTemplate {
                            mode: mode.clone(),
                            assignments: vec![
                                assign(0, Expression::Source(0)),
                                assign(
                                    1,
                                    Expression::Window(Window {
                                        kind: WindowKind::RowNumber,
                                        group_by: vec![],
                                        order_by: vec![OrderTerm {
                                            column: 0,
                                            descending: false,
                                            nulls_first: false,
                                        }],
                                        filter: None,
                                    }),
                                ),
                                assign(2, Expression::Column(1)),
                            ],
                            filter: Some(filter),
                        }],
                        vec![],
                        vec![0],
                        vec![],
                    );
                    if column == 0 || matches!(mode, RowMode::Groups(_)) {
                        assert!(plan.is_ok(), "{plan:?}");
                    } else {
                        assert_eq!(
                            plan,
                            Err(PlanError::Filter(BindingError::UnavailableColumn))
                        );
                    }
                }
            }
        }

        #[test]
        fn key_grain_admission_sees_raw_reads_through_recursive_source_containers() {
            use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
            let source = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
            let binding = |expression| Expression::SourceBinding {
                source: Box::new(assign(1, expression)),
                absent: None,
                variable_absent: false,
                selection_declared: false,
                variable: "SRC.V".into(),
                path: "columns.V.derivation.source".into(),
            };
            let selection = |expression| {
                Expression::FirstAvailable(Box::new(FirstAvailable::new(
                    vec![SelectionSource {
                        path: "columns.V.derivation.first_available.sources[0]".into(),
                        read: SelectionRead::Expression(Box::new(assign(1, expression))),
                    }],
                    Value::Int(0),
                )))
            };
            for raw in [false, true] {
                let value = if raw {
                    Expression::Source(1)
                } else {
                    Expression::Literal(Value::Int(7))
                };
                for expression in [
                    binding(value.clone()),
                    selection(value.clone()),
                    binding(selection(value.clone())),
                    selection(binding(value.clone())),
                ] {
                    let plan = DatasetPlan::new(
                        source.clone(),
                        source.clone(),
                        vec![RowTemplate {
                            mode: RowMode::Keys,
                            assignments: vec![assign(0, Expression::Source(0))],
                            filter: None,
                        }],
                        vec![assign(1, expression)],
                        vec![0],
                        vec![],
                    );
                    if raw {
                        assert_eq!(plan, Err(PlanError::InvalidKeyMode));
                    } else {
                        assert!(plan.is_ok(), "{plan:?}");
                    }
                }
            }
        }

        #[test]
        fn nested_windows_admit_completed_dependencies_and_refuse_uncompleted_dependencies() {
            use yamaa_core::{
                bound_expression::{Binding, BoundPredicate, Read},
                dataset::{CaseBranch, OrderTerm, Window, WindowKind},
            };
            let output = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
            let window = Expression::Window(Window {
                kind: WindowKind::RowNumber,
                group_by: vec![],
                order_by: vec![OrderTerm {
                    column: 0,
                    descending: false,
                    nulls_first: false,
                }],
                filter: None,
            });
            let make = |expression| {
                DatasetPlan::new(
                    output.clone(),
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Keys,
                        assignments: vec![assign(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![assign(1, expression)],
                    vec![0],
                    vec![],
                )
            };
            assert!(make(window.clone()).is_ok());
            let predicate = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("ID > 0", "case.when").unwrap(),
                vec![Binding {
                    name: "ID".into(),
                    read: Read::Column(0),
                }],
            )
            .unwrap();
            let mut nested = window;
            for _ in 0..2 {
                nested = Expression::Case(vec![CaseBranch {
                    when: Some(predicate.clone()),
                    assignment: Box::new(assign(1, nested)),
                }]);
                assert!(make(nested.clone()).is_ok());
            }
            let unavailable = Expression::Case(vec![CaseBranch {
                when: Some(predicate),
                assignment: Box::new(assign(
                    1,
                    Expression::Window(Window {
                        kind: WindowKind::RowNumber,
                        group_by: vec![],
                        order_by: vec![OrderTerm {
                            column: 1,
                            descending: false,
                            nulls_first: false,
                        }],
                        filter: None,
                    }),
                )),
            }]);
            assert_eq!(make(unavailable), Err(PlanError::InvalidWindow));
        }

        fn verification(check: Check) -> Verification {
            Verification {
                path: "verifications[0]".into(),
                check,
            }
        }

        #[test]
        fn column_verification_admission_preserves_declared_order_and_closed_phase() {
            use yamaa_core::dataset::ColumnVerifications;
            let output = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
            let make = || {
                DatasetPlan::new(
                    output.clone(),
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
            };
            let group = |column, path: &str, check| ColumnVerifications {
                column,
                checks: vec![Verification {
                    path: path.into(),
                    check,
                }],
            };
            assert_eq!(
                make().with_column_verifications(vec![group(2, "invalid", Check::NotMissing)]),
                Err(PlanError::InvalidColumns)
            );
            assert_eq!(
                make().with_column_verifications(vec![
                    group(1, "late", Check::NotMissing),
                    group(0, "early", Check::NotMissing)
                ]),
                Err(PlanError::InvalidColumns)
            );
            assert_eq!(
                make().with_column_verifications(vec![
                    group(0, "same", Check::NotMissing),
                    group(1, "same", Check::NotMissing)
                ]),
                Err(PlanError::DuplicateVerificationPath)
            );
            assert_eq!(
                make().with_column_verifications(vec![group(0, "", Check::NotMissing)]),
                Err(PlanError::EmptyPath)
            );
            assert_eq!(
                make().with_column_verifications(vec![group(
                    0,
                    "wrong-phase",
                    Check::Unique(vec![0])
                )]),
                Err(PlanError::InvalidColumns)
            );
            assert!(make()
                .with_column_verifications(vec![
                    group(0, "first", Check::NotMissing),
                    group(1, "second", Check::NotMissing)
                ])
                .is_ok());
            assert_eq!(
                DatasetPlan::new(
                    output.clone(),
                    output,
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1))
                        ],
                        filter: None
                    }],
                    vec![],
                    vec![0],
                    vec![verification(Check::NotMissing)]
                ),
                Err(PlanError::InvalidColumns)
            );
        }

        #[test]
        fn column_value_admission_requires_converted_values_and_valid_types_and_bounds() {
            use yamaa_core::dataset::ColumnVerifications;
            let make = |kind, check| {
                let table = schema(&[("V", kind)]);
                DatasetPlan::new(
                    table.clone(),
                    table,
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![assign(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
                .with_column_verifications(vec![ColumnVerifications {
                    column: 0,
                    checks: vec![verification(check)],
                }])
            };
            for check in [
                Check::AllowedValues(vec![]),
                Check::AllowedValues(vec![Value::Missing]),
                Check::AllowedValues(vec![Value::float(1.0)]),
            ] {
                assert_eq!(make(ColumnType::Int, check), Err(PlanError::InvalidColumns));
            }
            assert!(make(
                ColumnType::Int,
                Check::AllowedValues(vec![Value::Int(i64::MIN), Value::Int(i64::MAX)])
            )
            .is_ok());
            for (kind, check) in [
                (
                    ColumnType::Str,
                    Check::Range {
                        min: Some(Value::Int(1)),
                        max: None,
                    },
                ),
                (
                    ColumnType::Int,
                    Check::Range {
                        min: None,
                        max: None,
                    },
                ),
                (
                    ColumnType::Int,
                    Check::Range {
                        min: Some(Value::Int(2)),
                        max: Some(Value::Int(1)),
                    },
                ),
                (
                    ColumnType::Int,
                    Check::Range {
                        min: Some(Value::Str("1".into())),
                        max: None,
                    },
                ),
                (ColumnType::Int, Check::MaxLength(1)),
                (ColumnType::Str, Check::MaxLength(0)),
            ] {
                assert_eq!(make(kind, check), Err(PlanError::InvalidBounds));
            }
            assert!(make(
                ColumnType::Int,
                Check::Range {
                    min: Some(Value::float(0.5)),
                    max: Some(Value::Int(2))
                }
            )
            .is_ok());
            assert!(make(ColumnType::Str, Check::MaxLength(1)).is_ok());
            let pattern = yamaa_core::portable_regex::Pattern::compile("b").unwrap();
            assert_eq!(
                make(ColumnType::Int, Check::Matches(pattern.clone())),
                Err(PlanError::InvalidColumns)
            );
            assert!(make(ColumnType::Str, Check::Matches(pattern)).is_ok());
        }

        /// Admission rejects latent invalid paths even for an empty source snapshot.
        #[test]
        fn plan_admission_checks_every_template_and_dependency() {
            let source = schema(&[("id", ColumnType::Int), ("other", ColumnType::Int)]);
            let output = source.clone();
            let make = |templates, columns| {
                DatasetPlan::new(
                    source.clone(),
                    output.clone(),
                    templates,
                    columns,
                    vec![0],
                    vec![],
                )
            };
            let records = |assignments| RowTemplate {
                filter: None,
                mode: RowMode::Records,
                assignments,
            };
            let grouped = |assignments| RowTemplate {
                filter: None,
                mode: RowMode::Groups(vec![0]),
                assignments,
            };
            assert_eq!(make(vec![], vec![]), Err(PlanError::NoTemplates));
            assert_eq!(
                make(
                    vec![records(vec![assign(0, Expression::Column(1))])],
                    vec![]
                ),
                Err(PlanError::UnavailableColumn)
            );
            assert_eq!(
                make(
                    vec![records(vec![assign(0, Expression::Source(2))])],
                    vec![]
                ),
                Err(PlanError::InvalidSource)
            );
            assert_eq!(
                make(
                    vec![grouped(vec![assign(0, Expression::Source(1))])],
                    vec![]
                ),
                Err(PlanError::NonGroupSource)
            );
            let reduce = Expression::Reduce {
                identifier: None,
                column: 1,
                reducer: NumericReducer::Sum,
                text: "SUM(x.other)".into(),
            };
            assert_eq!(
                make(vec![records(vec![assign(0, reduce)])], vec![]),
                Err(PlanError::UngroupedReduction)
            );
            assert_eq!(
                make(
                    vec![records(vec![
                        assign(0, Expression::Source(0)),
                        assign(0, Expression::Source(0))
                    ])],
                    vec![]
                ),
                Err(PlanError::DuplicateAssignment)
            );
            assert_eq!(
                make(
                    vec![records(vec![assign(0, Expression::Source(0))])],
                    vec![]
                ),
                Err(PlanError::IncompleteRow)
            );
            // Row-phase columns are per template: one template may derive a
            // column up front while another completes it column-phase.
            assert!(make(
                vec![
                    records(vec![]),
                    grouped(vec![assign(0, Expression::Source(0))])
                ],
                vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Literal(Value::Missing))
                ]
            )
            .is_ok());
            // A non-group field in the later column phase is invalid too.
            assert_eq!(
                make(
                    vec![grouped(vec![assign(0, Expression::Source(0))])],
                    vec![assign(1, Expression::Source(1))]
                ),
                Err(PlanError::NonGroupSource)
            );
        }

        /// Invalid typed verification declarations remain static plan errors.
        #[test]
        fn invalid_verifications_are_rejected() {
            let source = schema(&[("id", ColumnType::Str)]);
            let template = || {
                vec![RowTemplate {
                    filter: None,
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0))],
                }]
            };
            for check in [
                Check::RowCount {
                    min: None,
                    max: None,
                },
                Check::RowCount {
                    min: Some(2),
                    max: Some(1),
                },
                Check::WideRowCount {
                    min: None,
                    max: None,
                },
                Check::WideRowCount {
                    min: Some(
                        yamaa_core::dataset::RowCountBound::new("999999999999999999999").unwrap(),
                    ),
                    max: Some(
                        yamaa_core::dataset::RowCountBound::new("999999999999999999998").unwrap(),
                    ),
                },
            ] {
                assert_eq!(
                    DatasetPlan::new(
                        source.clone(),
                        source.clone(),
                        template(),
                        vec![],
                        vec![0],
                        vec![verification(check)]
                    ),
                    Err(PlanError::InvalidBounds)
                );
            }
            for columns in [vec![], vec![1]] {
                assert_eq!(
                    DatasetPlan::new(
                        source.clone(),
                        source.clone(),
                        template(),
                        vec![],
                        vec![0],
                        vec![verification(Check::Unique(columns))]
                    ),
                    Err(PlanError::InvalidColumns)
                );
            }
        }

        #[test]
        fn all_or_none_admission_requires_two_distinct_declared_columns() {
            let source = schema(&[
                ("ID", ColumnType::Str),
                ("V", ColumnType::Int),
                ("W", ColumnType::Date),
            ]);
            let make = |columns| {
                DatasetPlan::new(
                    source.clone(),
                    source.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        filter: None,
                        assignments: (0..3)
                            .map(|column| assign(column, Expression::Source(column)))
                            .collect(),
                    }],
                    vec![],
                    vec![0],
                    vec![verification(Check::AllOrNone(columns))],
                )
            };
            for columns in [vec![], vec![1], vec![1, 1], vec![1, 2, 3]] {
                assert_eq!(make(columns), Err(PlanError::InvalidColumns));
            }
            assert!(make(vec![1, 1, 2]).is_ok());
        }

        #[test]
        fn first_available_admits_every_operand_before_any_source_access() {
            use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
            let source = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
            let output = source.clone();
            let make = |sources| {
                DatasetPlan::new(
                    source.clone(),
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Keys,
                        assignments: vec![assign(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![assign(
                        1,
                        Expression::FirstAvailable(Box::new(FirstAvailable::new(
                            sources,
                            Value::Int(99),
                        ))),
                    )],
                    vec![0],
                    vec![],
                )
            };
            let valid = SelectionSource {
                path: "columns.V.derivation.first_available.sources[0]".into(),
                read: SelectionRead::Column(0),
            };
            assert!(make(vec![valid.clone()]).is_ok());
            assert!(make(vec![]).is_ok());
            for (read, error) in [
                (SelectionRead::Column(1), PlanError::UnavailableColumn),
                (
                    SelectionRead::Collect {
                        column: 2,
                        identifier: "SRC.V".into(),
                        filter: None,
                    },
                    PlanError::InvalidSource,
                ),
            ] {
                assert_eq!(
                    make(vec![
                        valid.clone(),
                        SelectionSource {
                            path: "later".into(),
                            read
                        }
                    ]),
                    Err(error)
                );
            }
            assert_eq!(
                make(vec![SelectionSource {
                    path: String::new(),
                    read: SelectionRead::Column(0)
                }]),
                Err(PlanError::EmptyPath)
            );
            let selection =
                Expression::FirstAvailable(Box::new(FirstAvailable::new(vec![], Value::Int(1))));
            assert!(DatasetPlan::new(
                source.clone(),
                output.clone(),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments: vec![assign(0, selection.clone())],
                    filter: None
                }],
                vec![assign(1, Expression::Literal(Value::Int(2)))],
                vec![0],
                vec![]
            )
            .is_ok());
            assert!(DatasetPlan::new(
                source,
                output,
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0))],
                    filter: None
                }],
                vec![assign(1, selection)],
                vec![0],
                vec![]
            )
            .is_ok());
        }

        #[test]
        fn driver_wrappers_admit_only_selected_record_or_group_schema_coordinates() {
            use yamaa_core::dataset::SecondarySource;
            let primary = schema(&[("ID", ColumnType::Int)]);
            let donor = schema(&[("PAD", ColumnType::Str), ("ID", ColumnType::Int)]);
            let output = schema(&[("ID", ColumnType::Int)]);
            let make = |mode, column| {
                DatasetPlan::new_with_sources(
                    primary.clone(),
                    vec![SecondarySource {
                        name: "DONOR".into(),
                        schema: donor.clone(),
                    }],
                    output.clone(),
                    vec![RowTemplate {
                        mode,
                        assignments: vec![assign(0, Expression::Source(column))],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            let selected = |source, mode| RowMode::Driver {
                source,
                mode: Box::new(mode),
            };
            assert!(make(selected(1, RowMode::Records), 1).is_ok());
            assert!(make(selected(1, RowMode::Groups(vec![1])), 1).is_ok());
            assert_eq!(make(RowMode::Records, 1), Err(PlanError::InvalidSource));
            assert_eq!(
                make(selected(1, RowMode::Records), 2),
                Err(PlanError::InvalidSource)
            );
            assert_eq!(
                make(selected(1, RowMode::Groups(vec![0])), 1),
                Err(PlanError::NonGroupSource)
            );
            for mode in [
                selected(0, RowMode::Records),
                selected(2, RowMode::Records),
                selected(1, RowMode::Keys),
                selected(1, selected(1, RowMode::Records)),
            ] {
                assert_eq!(make(mode, 0), Err(PlanError::InvalidSource));
            }
        }
    }
    mod declarative_flags {
        use crate::support::diagnostic::declarative_flags::*;
        #[test]

        fn raw_truth_literals_presence_null_and_complete_name_map() {
            let plan = predicate_compiler::compile("ID = 1", "flag.condition").unwrap();
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
                    C::PredicateInvalidExpression,
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
                    C::PredicateInvalidExpression,
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
                [C::PredicateInvalidExpression, C::PredicateUnknownField]
            );
            assert_eq!(first[0].spec_paths, ["columns.A.derivation.flag.condition"]);
            assert_eq!(first[1].spec_paths, ["columns.B.derivation.flag.condition"]);
        }
        #[test]

        fn regex_input_type_is_flag_owned_without_changing_generic_predicate_mapping() {
            let condition = Condition::ContainsExpectedText {
                actual: ValueType::Int,
            };
            assert_eq!(condition.requirement(), "REQ-1244");
            let error: EvaluationError<()> = EvaluationError {
                spec_path: "flag.condition".into(),
                expression: "original".into(),
                route: vec![Route::Value],
                kind: ErrorKind::Condition(condition),
            };
            let diagnostic = yamaa_core::dataset::flag_condition_diagnostic(&error).unwrap();
            assert_eq!(diagnostic.code, C::PredicateInputType);
            assert_eq!(diagnostic.definition().requirement, Some("REQ-0190"));
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
            assert_eq!(plan.templates().len(), 16);
            let flags = plan
                .columns()
                .iter()
                .filter_map(|assignment| match &assignment.expression {
                    yamaa_core::dataset::Expression::Flag(flag) => Some(flag),
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
            let yamaa_core::dataset::Expression::Case(branches) = &plan.columns()[0].expression
            else {
                panic!("case")
            };
            let when = branches[0].when.as_ref().unwrap();
            assert_eq!(when.bindings().len(), 1);
            assert_eq!(when.plan().identifiers(), ["ID", "ID"]);
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
                .map(|d| {
                    (
                        d.code.definition().requirement.unwrap().to_string(),
                        d.spec_paths,
                    )
                })
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
                let keys = &plan.intermediates()[0].keys;
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
                assert_eq!(plan.output().columns().len(), 3);
            }
            let prepared = PreparedSpecification::prepare(&doc(Tree::map(vec![(
                "KNUM",
                Tree::map(vec![("source", Tree::text("SRC.N"))]),
            )])))
            .unwrap();
            let (src, tab) = schemas();
            let plan = prepared.bind_sources(&[&src, &tab]).unwrap();
            assert!(matches!(
                &plan.intermediates()[0].keys[0].value,
                MatchValue::Source(MatchRead::Record { column: 1, .. })
            ));
        }
        #[test]
        fn authored_type_donor_width_reference_then_dependent_read_failure_order() {
            for (field, first) in [
                ("K", None),
                ("KINT", Some("REQ-0323")),
                ("ABSENT", Some("REQ-0116")),
            ] {
                let actual = finding_ids(Tree::map(vec![(field, pad("SRC.ABSENT", "0"))]));
                let mut expected = Vec::new();
                if let Some(id) = first {
                    expected.push((id.into(), vec!["intermediates[0].key".into()]));
                }
                expected.extend([
                    (
                        "REQ-1261".into(),
                        vec![format!("intermediates[0].key.{field}.str_pad.width")],
                    ),
                    (
                        "REQ-0117".into(),
                        vec![format!("intermediates[0].key.{field}")],
                    ),
                    (
                        "REQ-0103".into(),
                        vec!["columns.V.derivation.source".into()],
                    ),
                ]);
                assert_eq!(actual, expected);
            }
            assert_eq!(
                finding_ids(Tree::map(vec![("K", pad("SRC.N", "0"))])),
                [(
                    "REQ-1261".into(),
                    vec!["intermediates[0].key.K.str_pad.width".into()]
                )]
            );
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
                plan.intermediates()[0].keys[0].value,
                MatchValue::Literal(Value::Str("SRC.ABSENT".into()))
            );
            assert_eq!(
                finding_ids(Tree::map(vec![(
                    "KINT",
                    Tree::map(vec![("literal", Tree::Scalar(N::Boolean(true)))])
                )]))[0]
                    .0,
                "REQ-0323"
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
                plan.output()
                    .columns()
                    .iter()
                    .map(|column| column.name.as_str())
                    .collect::<Vec<_>>(),
                ["ID", "N", "V"]
            );
            let item = &plan.intermediates()[0];
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
            assert!(
                expression.result_type.is_none(),
                "greatest retains runtime comparability"
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

        /// REQ-0125 expectations come from authored visibility, not generated runtime results.
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
    mod match_value {
        use yamaa_core::{
            key_relation::ComparableType,
            match_value::{result_type, Expression, LiteralKind},
            reference_binding::{Catalog, Field},
            value::ColumnType,
        };

        /// Unknown names defer comparison without changing the immutable catalog's known types.
        #[test]
        fn unknown_results_do_not_poison_catalog() {
            let catalog = Catalog::compile(
                &[Field {
                    name: "A",
                    column_type: ColumnType::Int,
                }],
                &[],
            )
            .unwrap();
            assert_eq!(result_type(&catalog, Expression::Source("éé")), None);
            assert_eq!(result_type(&catalog, Expression::Source("xxx")), None);
            assert_eq!(
                result_type(&catalog, Expression::Source("A")),
                Some(ColumnType::Int.into())
            );
            assert_eq!(
                result_type(&catalog, Expression::Literal(LiteralKind::Bool)),
                Some(ComparableType::Boolean)
            );
            assert_eq!(
                result_type(&catalog, Expression::Literal(LiteralKind::Missing)),
                None
            );
        }

        #[test]
        fn operations_outside_the_complete_req1259_table_defer_comparison() {
            let catalog = Catalog::compile(&[], &[]).unwrap();
            for (name, target) in [
                ("shift", Some(ColumnType::Date)),
                ("shift", Some(ColumnType::DateTime)),
                ("diff", Some(ColumnType::DateTime)),
                ("duration_value", None),
                ("row_value", None),
                ("locf", None),
                ("mapping", None),
                ("case", None),
                ("greatest", None),
                ("least", None),
            ] {
                assert_eq!(
                    result_type(&catalog, Expression::Operation { name, target }),
                    None,
                    "{name}"
                );
            }
        }

        #[test]
        fn a_declared_temporal_type_selects_the_static_result_type() {
            let catalog = Catalog::compile(&[], &[]).unwrap();
            for (name, target, expected) in [
                ("impute", Some(ColumnType::Date), ColumnType::Date),
                ("impute", Some(ColumnType::DateTime), ColumnType::DateTime),
                ("diff", Some(ColumnType::Date), ColumnType::Int),
                ("precision", Some(ColumnType::Date), ColumnType::Str),
                ("precision", Some(ColumnType::DateTime), ColumnType::Str),
                ("imputation_flag", None, ColumnType::Str),
                ("to_date", None, ColumnType::Date),
            ] {
                assert_eq!(
                    result_type(&catalog, Expression::Operation { name, target }),
                    Some(expected.into()),
                    "{name}"
                );
            }
        }
    }
    mod primary_intermediates {
        //! Primary donor metadata must describe the already admitted original input.
        use crate::support::id_value_schema as schema;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey,
                IntermediateSource, MatchRead, MatchValue, PlanError, RowMode, RowTemplate,
                SourceSchemas,
            },
            table::{Column, TableSchema},
            value::{ColumnType, Value},
        };
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("rows.C{column}"),
            }
        }
        fn selector() -> Intermediate {
            Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Primary {
                    schema: schema(),
                    name: "SRC".into(),
                },
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                filter: None,
                selection: None,
                no_match: Some(Value::Missing),
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            }
        }

        #[test]
        fn only_exact_original_schema_and_a_named_primary_relation_are_admitted() {
            let template = RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(
                        1,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    ),
                ],
                filter: None,
            };
            let make = |item| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(),
                        secondary: vec![],
                    },
                    vec![item],
                    schema(),
                    vec![template.clone()],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            let item = selector();
            assert_eq!(item.source_schema(&[]), Some(&schema()));
            assert_eq!(item.source_column(&[], "V"), Some(1));
            assert!(make(item).is_ok());
            let mut item = selector();
            item.source = IntermediateSource::Primary {
                schema: schema(),
                name: String::new(),
            };
            assert_eq!(make(item), Err(PlanError::InvalidIntermediate));
            let mut item = selector();
            item.source = IntermediateSource::Primary {
                schema: TableSchema::new(vec![
                    Column {
                        name: "DIFFERENT".into(),
                        kind: ColumnType::Int,
                    },
                    Column {
                        name: "V".into(),
                        kind: ColumnType::Int,
                    },
                ])
                .unwrap(),
                name: "SRC".into(),
            };
            assert_eq!(make(item), Err(PlanError::InvalidIntermediate));
        }

        #[test]
        fn empty_lookup_keys_are_admitted_only_for_the_actual_driver_and_never_for_scalar_reads() {
            let mut item = selector();
            item.keys.clear();
            item.no_match = None;
            let driver = |index| RowMode::IntermediateDriver {
                index,
                source: 0,
                mode: Box::new(RowMode::Records),
            };
            let make = |mode, read| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(),
                        secondary: vec![],
                    },
                    vec![item.clone()],
                    schema(),
                    vec![RowTemplate {
                        mode,
                        assignments: vec![assign(0, Expression::Source(0)), assign(1, read)],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            assert!(make(driver(0), Expression::Source(1)).is_ok());
            for mode in [RowMode::Records, driver(1)] {
                assert_eq!(
                    make(mode, Expression::Source(1)),
                    Err(PlanError::InvalidIntermediate)
                );
            }
            assert_eq!(
                make(
                    driver(0),
                    Expression::Intermediate {
                        index: 0,
                        column: 1
                    }
                ),
                Err(PlanError::InvalidIntermediate)
            );
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
