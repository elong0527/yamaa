//! Relations contracts.
#[allow(dead_code)]
mod support;
mod relations {
    mod reference_scope {
        use yamaa_core::{
            reference_binding::{Catalog, Dataset, Error, Field},
            reference_scope::{validate, Finding as F, Phase, Reach, Scope},
            value::ColumnType::{Float, Int, Str},
        };

        /// Independent typed source metadata; no reference implementation generates expected findings.
        fn catalog() -> Catalog {
            let source = [
                Field {
                    name: "K",
                    column_type: Str,
                },
                Field {
                    name: "N",
                    column_type: Int,
                },
            ];
            let other = [Field {
                name: "N",
                column_type: Float,
            }];
            Catalog::compile(
                &[],
                &[
                    Dataset {
                        name: "SRC",
                        fields: &source,
                    },
                    Dataset {
                        name: "OTHER",
                        fields: &other,
                    },
                ],
            )
            .unwrap()
        }

        /// A direct scalar read in the selected row or column phase.
        fn scope(phase: Phase<'_>) -> Scope<'_> {
            Scope {
                drivers: &["SRC"],
                current_driver: false,
                reach: Reach::Scalar,
                joined: false,
                phase,
            }
        }

        /// Stop after driver/existence failures, but retain scope-before-type diagnostic order.
        #[test]
        fn independent_scope_and_priority_truth() {
            let catalog = catalog();
            let row = scope(Phase::Row { group_by: None });
            let group = scope(Phase::Row {
                group_by: Some(&["SRC.K"]),
            });
            let column = scope(Phase::Column { groups: &[] });
            let columns = scope(Phase::Column {
                groups: &[&["SRC.K", "SRC.N"], &["SRC.K"]],
            });
            for (name, expected, scope, findings) in [
                ("SRC.N", Some(Int), row, vec![]),
                (
                    "SRC.N",
                    Some(Str),
                    row,
                    vec![F::IncompatibleInputType {
                        expected: Str,
                        actual: Int,
                    }],
                ),
                ("SRC.K", Some(Str), group, vec![]),
                ("SRC.N", None, group, vec![F::RowGroup]),
                (
                    "SRC.N",
                    Some(Str),
                    group,
                    vec![
                        F::RowGroup,
                        F::IncompatibleInputType {
                            expected: Str,
                            actual: Int,
                        },
                    ],
                ),
                ("SRC.ABSENT", Some(Str), group, vec![F::UnknownField]),
                (
                    "OTHER.ABSENT",
                    Some(Str),
                    Scope {
                        current_driver: true,
                        ..group
                    },
                    vec![F::DriverMismatch],
                ),
                (
                    "SRC.ABSENT",
                    None,
                    Scope {
                        current_driver: true,
                        ..row
                    },
                    vec![F::UnknownField],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        current_driver: true,
                        drivers: &[],
                        ..row
                    },
                    vec![F::DriverMismatch],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        current_driver: true,
                        drivers: &["SRC", "OTHER"],
                        ..row
                    },
                    vec![F::DriverMismatch],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        current_driver: true,
                        drivers: &["SRC", "SRC"],
                        ..row
                    },
                    vec![],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        joined: true,
                        ..group
                    },
                    vec![],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        reach: Reach::Record,
                        ..group
                    },
                    vec![],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        reach: Reach::Relation,
                        ..group
                    },
                    vec![],
                ),
                (
                    "SRC.N",
                    None,
                    Scope {
                        reach: Reach::Declared,
                        ..group
                    },
                    vec![],
                ),
                (
                    "OTHER.N",
                    Some(Str),
                    group,
                    vec![F::IncompatibleInputType {
                        expected: Str,
                        actual: Float,
                    }],
                ),
                (
                    "OTHER.N",
                    Some(Str),
                    Scope {
                        reach: Reach::Relation,
                        ..group
                    },
                    vec![
                        F::RowPhase,
                        F::IncompatibleInputType {
                            expected: Str,
                            actual: Float,
                        },
                    ],
                ),
                (
                    "OTHER.N",
                    None,
                    Scope {
                        reach: Reach::Declared,
                        ..group
                    },
                    vec![],
                ),
                ("SRC.N", None, columns, vec![F::ColumnGroup]),
                (
                    "SRC.N",
                    Some(Str),
                    columns,
                    vec![
                        F::ColumnGroup,
                        F::IncompatibleInputType {
                            expected: Str,
                            actual: Int,
                        },
                    ],
                ),
                ("SRC.K", Some(Str), columns, vec![]),
                (
                    "SRC.N",
                    None,
                    Scope {
                        reach: Reach::Relation,
                        ..columns
                    },
                    vec![],
                ),
                ("OTHER.N", Some(Str), column, vec![]),
                ("OTHER.ABSENT", Some(Str), column, vec![F::UnknownField]),
                (
                    "OTHER.N",
                    Some(Str),
                    Scope {
                        joined: true,
                        ..column
                    },
                    vec![F::IncompatibleInputType {
                        expected: Str,
                        actual: Float,
                    }],
                ),
                (
                    "OTHER.N",
                    Some(Str),
                    Scope {
                        reach: Reach::Declared,
                        ..column
                    },
                    vec![F::IncompatibleInputType {
                        expected: Str,
                        actual: Float,
                    }],
                ),
                (
                    "OTHER.N",
                    Some(Str),
                    Scope {
                        reach: Reach::Relation,
                        ..column
                    },
                    vec![F::IncompatibleInputType {
                        expected: Str,
                        actual: Float,
                    }],
                ),
                (
                    "SRC.K",
                    None,
                    scope(Phase::Row {
                        group_by: Some(&[]),
                    }),
                    vec![F::RowGroup],
                ),
                (
                    "SRC.K",
                    None,
                    scope(Phase::Column { groups: &[&[]] }),
                    vec![F::ColumnGroup],
                ),
            ] {
                assert_eq!(
                    validate(&catalog, name, expected, scope),
                    Ok(findings),
                    "{name}: {scope:?}"
                );
            }
        }

        /// A malformed query remains separate from later valid scope checks.
        #[test]
        fn qualified_query_shape_and_reuse() {
            let catalog = catalog();
            let row = scope(Phase::Row { group_by: None });
            assert_eq!(
                validate(&catalog, "N", None, row),
                Err(Error::BareQualifiedQuery)
            );
            assert_eq!(validate(&catalog, "SRC.N", Some(Int), row), Ok(vec![]));
        }
    }
    mod relation_predicates {
        //! Typed current-side ownership cannot bypass phase, source or group admission.
        use crate::support::int_schema as schema;
        use yamaa_core::{
            bound_expression::{Binding, BoundPredicate, Read},
            dataset::{
                Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
                IntermediateKey, IntermediateSource, MatchRead, MatchValue, PlanError,
                RelationPredicate, RowMode, RowTemplate, SecondarySource, SourceSchemas,
            },
            predicate_compiler,
            value::Value,
        };
        fn assignment(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("C{column}"),
            }
        }
        fn filter(identifier: &str, read: CurrentRead) -> RelationPredicate {
            RelationPredicate::new(
                BoundPredicate::new(
                    predicate_compiler::compile(
                        &format!("D.V > {identifier}"),
                        "intermediates[0].filter",
                    )
                    .unwrap(),
                    vec![
                        Binding {
                            name: "D.V".into(),
                            read: Read::Source(1),
                        },
                        Binding {
                            name: identifier.into(),
                            read: Read::Column(0),
                        },
                    ],
                )
                .unwrap(),
                vec![CurrentField {
                    identifier: identifier.into(),
                    read,
                }],
            )
            .unwrap()
        }
        fn item(filter: Option<RelationPredicate>) -> Intermediate {
            Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Input(0),
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                filter,
                selection: None,
                no_match: Some(Value::Missing),
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,

                range: None,
                columns: None,
            }
        }
        fn make(
            items: Vec<Intermediate>,
            assignments: Vec<Assignment>,
            mode: RowMode,
        ) -> Result<DatasetPlan, PlanError> {
            DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: schema(&["ID", "V"]),
                    secondary: vec![SecondarySource {
                        name: "D".into(),
                        schema: schema(&["ID", "V"]),
                    }],
                },
                items,
                schema(&["ID", "CURRENT", "A"]),
                vec![RowTemplate {
                    mode,
                    assignments,
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
        }
        #[test]
        fn completed_current_output_slots_retain_dependencies_and_exact_availability() {
            let relation = item(Some(filter("CURRENT", CurrentRead::Output(1))));
            assert_eq!(relation.output_dependencies().collect::<Vec<_>>(), [0, 1]);
            let assignments = vec![
                assignment(0, Expression::Source(0)),
                assignment(1, Expression::Literal(Value::Int(2))),
                assignment(
                    2,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                ),
            ];
            assert!(make(
                vec![relation.clone()],
                assignments.clone(),
                RowMode::Records
            )
            .is_ok());
            let mut early = assignments;
            early.swap(1, 2);
            assert_eq!(
                make(vec![relation], early, RowMode::Records),
                Err(PlanError::UnavailableColumn)
            );
        }
        #[test]
        fn typed_kept_intermediate_reads_validate_their_earlier_declared_owner_and_dependencies() {
            let first = item(None);
            let mut second = item(Some(filter(
                "KEPT",
                CurrentRead::Intermediate {
                    index: 0,
                    column: 1,
                },
            )));
            second.identifier = "Q".into();
            second.path = "intermediates[1]".into();
            let assignments = vec![
                assignment(0, Expression::Source(0)),
                assignment(1, Expression::Literal(Value::Int(2))),
                assignment(
                    2,
                    Expression::Intermediate {
                        index: 1,
                        column: 1,
                    },
                ),
            ];
            assert!(make(
                vec![first.clone(), second.clone()],
                assignments.clone(),
                RowMode::Records
            )
            .is_ok());
            second.filter = Some(filter(
                "KEPT",
                CurrentRead::Intermediate {
                    index: 1,
                    column: 1,
                },
            ));
            assert_eq!(
                make(vec![first, second], assignments, RowMode::Records),
                Err(PlanError::InvalidIntermediate)
            );
        }
        #[test]
        fn wrong_driver_coordinates_hidden_in_numeric_reads_are_rejected_without_cells() {
            let relation = item(Some(filter(
                "SRC.V",
                CurrentRead::Record {
                    source: 0,
                    column: 1,
                },
            )));
            let numeric = yamaa_core::bound_expression::BoundNumeric::new(
                yamaa_core::numeric_compiler::compile_numeric("P.V + 1", "C2").unwrap(),
                vec![Binding {
                    name: "P.V".into(),
                    read: Read::Intermediate {
                        index: 0,
                        column: 1,
                    },
                }],
            )
            .unwrap();
            assert_eq!(
                make(
                    vec![relation],
                    vec![
                        assignment(0, Expression::Source(0)),
                        assignment(1, Expression::Literal(Value::Int(2))),
                        assignment(2, Expression::Compute(numeric))
                    ],
                    RowMode::Driver {
                        source: 1,
                        mode: Box::new(RowMode::Records)
                    }
                ),
                Err(PlanError::InvalidIntermediate)
            );
            let relation = item(Some(filter(
                "BAD.V",
                CurrentRead::Record {
                    source: usize::MAX,
                    column: usize::MAX,
                },
            )));
            assert_eq!(
                make(
                    vec![relation],
                    vec![
                        assignment(0, Expression::Source(0)),
                        assignment(1, Expression::Literal(Value::Int(2))),
                        assignment(
                            2,
                            Expression::Intermediate {
                                index: 0,
                                column: 1
                            }
                        )
                    ],
                    RowMode::Records
                ),
                Err(PlanError::InvalidIntermediate)
            );
        }
        #[test]
        fn grouped_self_correlations_read_only_current_group_keys() {
            let output = schema(&["ID", "V", "A"]);
            let make = |column| {
                let mut relation = item(Some(filter(
                    "SRC.V",
                    CurrentRead::Record { source: 0, column },
                )));
                relation.source = IntermediateSource::SelfRows {
                    schema: output.clone(),
                    columns: vec![0, 1],
                };
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(&["ID", "V"]),
                        secondary: vec![],
                    },
                    vec![relation],
                    output.clone(),
                    vec![
                        RowTemplate {
                            mode: RowMode::Records,
                            assignments: vec![
                                assignment(0, Expression::Source(0)),
                                assignment(1, Expression::Source(1)),
                                assignment(2, Expression::Literal(Value::Int(0))),
                            ],
                            filter: None,
                        },
                        RowTemplate {
                            mode: RowMode::Groups(vec![0]),
                            assignments: vec![
                                assignment(0, Expression::Source(0)),
                                assignment(1, Expression::Literal(Value::Int(2))),
                                assignment(
                                    2,
                                    Expression::Intermediate {
                                        index: 0,
                                        column: 1,
                                    },
                                ),
                            ],
                            filter: None,
                        },
                    ],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            assert!(make(0).is_ok());
            assert_eq!(make(1), Err(PlanError::NonGroupSource));
        }

        #[test]
        fn kept_current_selectors_cannot_hide_a_first_template_self_read() {
            let output = schema(&["ID", "CURRENT", "A"]);
            let mut first = item(None);
            first.source = IntermediateSource::SelfRows {
                schema: output,
                columns: vec![0, 1],
            };
            let mut second = item(Some(filter(
                "KEPT",
                CurrentRead::Intermediate {
                    index: 0,
                    column: 1,
                },
            )));
            second.identifier = "Q".into();
            second.path = "intermediates[1]".into();
            assert_eq!(
                make(
                    vec![first, second],
                    vec![
                        assignment(0, Expression::Source(0)),
                        assignment(1, Expression::Literal(Value::Int(2))),
                        assignment(
                            2,
                            Expression::Intermediate {
                                index: 1,
                                column: 1
                            }
                        )
                    ],
                    RowMode::Records
                ),
                Err(PlanError::InvalidIntermediate)
            );
        }

        fn bound_intermediate_compute() -> Expression {
            Expression::Compute(
                yamaa_core::bound_expression::BoundNumeric::new(
                    yamaa_core::numeric_compiler::compile_numeric("P.V + 1", "C2").unwrap(),
                    vec![Binding {
                        name: "P.V".into(),
                        read: Read::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    }],
                )
                .unwrap(),
            )
        }
        fn bound_intermediate_flag() -> Expression {
            Expression::Flag(
                yamaa_core::dataset::Flag::new(
                    predicate_compiler::compile("P.V > 0", "C2").unwrap(),
                    vec![yamaa_core::dataset::FlagBinding {
                        name: "P.V".into(),
                        read: yamaa_core::dataset::FlagRead::Read(Read::Intermediate {
                            index: 0,
                            column: 1,
                        }),
                    }],
                    Value::Int(1),
                    None,
                    None,
                )
                .unwrap(),
            )
        }

        #[test]
        fn secondary_driver_record_coordinates_hidden_in_compute_and_flag_reads_are_rejected() {
            // The intermediate matches on shared output values, but its filter's
            // current field reads a record coordinate. `validate_named_read` permits
            // it here because the current field names the template driver's own
            // source (1); only the primary-coordinate guard in `current_record_read`
            // separates the direct form from the bound forms, so the Compute and
            // Flag arms must inspect their intermediate bindings like the fixed
            // Function/Case arms (issue #2430).
            let relation = item(Some(filter(
                "DVR.V",
                CurrentRead::Record {
                    source: 1,
                    column: 1,
                },
            )));
            let mode = RowMode::Driver {
                source: 1,
                mode: Box::new(RowMode::Records),
            };
            for expression in [
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
                bound_intermediate_compute(),
                bound_intermediate_flag(),
            ] {
                assert_eq!(
                    make(
                        vec![relation.clone()],
                        vec![
                            assignment(0, Expression::Source(0)),
                            assignment(1, Expression::Literal(Value::Int(2))),
                            assignment(2, expression),
                        ],
                        mode.clone(),
                    ),
                    Err(PlanError::InvalidIntermediate)
                );
            }
            // Shared output matching without any record coordinate remains admitted
            // through the same bound form.
            let shared = item(Some(filter("OUT.V", CurrentRead::Output(1))));
            assert!(make(
                vec![shared],
                vec![
                    assignment(0, Expression::Source(0)),
                    assignment(1, Expression::Literal(Value::Int(2))),
                    assignment(2, bound_intermediate_compute()),
                ],
                mode,
            )
            .is_ok());
        }

        #[test]
        fn secondary_driver_column_phase_rejects_bound_record_reads_and_reductions() {
            let relation = item(Some(filter(
                "DVR.V",
                CurrentRead::Record {
                    source: 1,
                    column: 1,
                },
            )));
            let column_plan = |expression: Expression| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(&["ID", "V"]),
                        secondary: vec![SecondarySource {
                            name: "D".into(),
                            schema: schema(&["ID", "V"]),
                        }],
                    },
                    vec![relation.clone()],
                    schema(&["ID", "CURRENT", "A"]),
                    vec![RowTemplate {
                        mode: RowMode::Driver {
                            source: 1,
                            mode: Box::new(RowMode::Records),
                        },
                        assignments: vec![
                            assignment(0, Expression::Source(0)),
                            assignment(1, Expression::Literal(Value::Int(2))),
                        ],
                        filter: None,
                    }],
                    vec![assignment(2, expression)],
                    vec![0],
                    vec![],
                )
            };
            // Column assignments execute against the primary table, so a bound
            // intermediate read of a record coordinate is rejected in the column
            // phase (`RecordReadScope::All`) just like the direct form.
            for expression in [
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
                bound_intermediate_compute(),
                bound_intermediate_flag(),
            ] {
                assert_eq!(column_plan(expression), Err(PlanError::InvalidKeyMode));
            }
            // PR #2030 Minor, adjudicated with #2430: column-phase Reduce/Count read
            // the primary table with the secondary driver's row indices, so they
            // are current record reads in `RecordReadScope::All` as well.
            let grouped_column_plan = |expression: Expression| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(&["ID", "V"]),
                        secondary: vec![SecondarySource {
                            name: "D".into(),
                            schema: schema(&["ID", "V"]),
                        }],
                    },
                    vec![],
                    schema(&["ID", "CURRENT", "A"]),
                    vec![RowTemplate {
                        mode: RowMode::Driver {
                            source: 1,
                            mode: Box::new(RowMode::Groups(vec![0])),
                        },
                        assignments: vec![
                            assignment(0, Expression::Source(0)),
                            assignment(1, Expression::Literal(Value::Int(2))),
                        ],
                        filter: None,
                    }],
                    vec![assignment(2, expression)],
                    vec![0],
                    vec![],
                )
            };
            for expression in [
                Expression::Reduce {
                    identifier: None,
                    column: 1,
                    reducer: yamaa_core::reduction::NumericReducer::Sum,
                    text: "SUM(x.V)".into(),
                },
                Expression::Count {
                    column: Some(1),
                    text: "COUNT(x.V)".into(),
                },
            ] {
                assert_eq!(
                    grouped_column_plan(expression),
                    Err(PlanError::InvalidKeyMode)
                );
            }
        }
    }
    mod relation_range_dependencies {
        //! A kept current read inherits the earlier selector's window dependency.
        use crate::support::table_schema as schema;
        use yamaa_core::{
            bound_expression::{Binding, BindingError, BoundPredicate, Read},
            dataset::{
                visit_output_dependencies, window_stages, Assignment, CurrentField, CurrentRead,
                DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource,
                MatchRead, MatchValue, OrderTerm, PlanError, RelationRange, RelationRangeBound,
                RowMode, RowTemplate, SecondarySource, SourceSchemas, Window, WindowKind,
            },
            value::{ColumnType as T, Value, ValueType},
        };
        fn item(identifier: &str, current: CurrentRead) -> Intermediate {
            Intermediate {
                identifier: identifier.into(),
                path: format!("intermediates.{identifier}"),
                source: IntermediateSource::Input(0),
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: format!("intermediates.{identifier}.key"),
                    value: MatchValue::Variable(MatchRead::Output {
                        column: 0,
                        identifier: "ID".into(),
                    }),
                }],
                filter: None,
                selection: None,
                columns: Some(vec![2]),
                no_match: Some(Value::Missing),
                unique: Vec::new(),
                derivations: None,
                pending_schema: None,
                range: Some(
                    RelationRange::new(
                        format!("intermediates.{identifier}.between"),
                        CurrentField {
                            read: current,
                            identifier: if identifier == "P" { "SEQ" } else { "P.V" }.into(),
                        },
                        Some(ValueType::Int),
                        Some(RelationRangeBound {
                            column: 1,
                            identifier: "LOW".into(),
                            kind: Some(ValueType::Int),
                        }),
                        None,
                    )
                    .unwrap(),
                ),
            }
        }
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("row.C{column}"),
            }
        }
        #[test]
        fn a_transitive_kept_range_read_cannot_enter_a_prewindow_filter() {
            let items = vec![
                item("P", CurrentRead::Output(1)),
                item(
                    "Q",
                    CurrentRead::Intermediate {
                        index: 0,
                        column: 2,
                    },
                ),
            ];
            let assignments = vec![
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
                assign(
                    2,
                    Expression::Intermediate {
                        index: 1,
                        column: 2,
                    },
                ),
            ];
            assert_eq!(window_stages(&assignments, 3, &items), [false, true, true]);
            let mut dependencies = Vec::new();
            visit_output_dependencies(
                1,
                |index| items.get(index),
                |column| dependencies.push(column),
            )
            .unwrap();
            assert_eq!(dependencies, [0, 0, 1]);
            let filter = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("VALUE > 0", "row.filter").unwrap(),
                vec![Binding {
                    name: "VALUE".into(),
                    read: Read::Column(2),
                }],
            )
            .unwrap();
            let sources = SourceSchemas {
                primary: schema(&[("ID", T::Int)]),
                secondary: vec![SecondarySource {
                    name: "D".into(),
                    schema: schema(&[("ID", T::Int), ("LOW", T::Int), ("V", T::Int)]),
                }],
            };
            let output = schema(&[("ID", T::Int), ("SEQ", T::Int), ("VALUE", T::Int)]);
            for filter in [None, Some(filter)] {
                let rejected = filter.is_some();
                let result = DatasetPlan::new_with_intermediates(
                    sources.clone(),
                    items.clone(),
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: assignments.clone(),
                        filter,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                );
                if rejected {
                    assert_eq!(
                        result,
                        Err(PlanError::Filter(BindingError::UnavailableColumn))
                    );
                } else {
                    assert!(result.is_ok(), "{result:?}");
                }
            }
        }

        #[test]
        fn a_declared_current_kind_cannot_be_forged_to_bypass_range_admission() {
            let mut p = item("P", CurrentRead::Output(0));
            p.range = Some(
                RelationRange::new(
                    "intermediates.P.between".into(),
                    CurrentField {
                        identifier: "ID".into(),
                        read: CurrentRead::Output(0),
                    },
                    Some(ValueType::Float),
                    Some(RelationRangeBound {
                        column: 1,
                        identifier: "LOW".into(),
                        kind: Some(ValueType::Int),
                    }),
                    None,
                )
                .unwrap(),
            );
            let result = DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: schema(&[("ID", T::Int)]),
                    secondary: vec![SecondarySource {
                        name: "D".into(),
                        schema: schema(&[("ID", T::Int), ("LOW", T::Int), ("V", T::Int)]),
                    }],
                },
                vec![p],
                schema(&[("ID", T::Int), ("VALUE", T::Int)]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::Intermediate {
                                index: 0,
                                column: 2,
                            },
                        ),
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            );
            assert_eq!(result, Err(PlanError::InvalidIntermediate));
        }
    }
    mod relation_ranges {
        //! Inclusive range narrowing preserves original endpoint effects and exact values.
        use yamaa_core::{
            dataset::{
                CurrentField, CurrentRead, RelationRange, RelationRangeBound, RelationRangeError,
                RelationRangeFailure, RelationRangeTypes,
            },
            table::ValueRef,
            value::{FiniteFloat, ValueType},
        };
        fn bound(column: usize, kind: Option<ValueType>) -> RelationRangeBound {
            RelationRangeBound {
                column,
                identifier: if column == 0 { "LOW" } else { "HIGH" }.into(),
                kind,
            }
        }
        fn range(lower: bool, upper: bool) -> RelationRange {
            RelationRange::new(
                "intermediates[0].between".into(),
                CurrentField {
                    read: CurrentRead::Output(2),
                    identifier: "VALUE".into(),
                },
                None,
                lower.then(|| bound(0, None)),
                upper.then(|| bound(1, None)),
            )
            .unwrap()
        }
        #[test]
        fn declared_bounds_are_optional_but_cannot_both_be_omitted() {
            for (lo, hi) in [(true, false), (false, true), (true, true)] {
                let mut reads = Vec::new();
                assert!(range(lo, hi)
                    .allows(ValueRef::Int(7), |column| {
                        reads.push(column);
                        Ok::<_, ()>(ValueRef::Int(7))
                    })
                    .unwrap());
                assert_eq!(
                    reads,
                    [(lo, 0), (hi, 1)]
                        .into_iter()
                        .filter_map(|(stated, column)| stated.then_some(column))
                        .collect::<Vec<_>>()
                );
            }
            assert_eq!(
                RelationRange::new(
                    "range".into(),
                    CurrentField {
                        read: CurrentRead::Output(0),
                        identifier: "V".into()
                    },
                    None,
                    None,
                    None
                ),
                Err(RelationRangeError::NoBounds)
            );
        }
        #[test]
        fn missing_current_or_stated_lower_stops_before_later_opaque_reads() {
            let mut reads = Vec::new();
            assert!(!range(true, true)
                .allows(ValueRef::Missing, |column| {
                    reads.push(column);
                    Err::<ValueRef<'_>, _>("unexpected read")
                })
                .unwrap());
            assert!(reads.is_empty());
            assert!(!range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    if column == 0 {
                        Ok(ValueRef::Missing)
                    } else {
                        Err("upper must stay unread")
                    }
                })
                .unwrap());
            assert_eq!(reads, vec![0]);
            reads.clear();
            assert!(!range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Int(1)
                    } else {
                        ValueRef::Missing
                    })
                })
                .unwrap());
            assert_eq!(reads, vec![0, 1]);
        }
        #[test]
        fn both_present_endpoints_are_read_before_lower_exclusion_and_upper_comparison_is_lazy() {
            let mut reads = Vec::new();
            assert!(!range(true, true)
                .allows(ValueRef::Int(3), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Int(4)
                    } else {
                        ValueRef::Str("unused incompatible comparison")
                    })
                })
                .unwrap());
            assert_eq!(reads, vec![0, 1]);
        }
        #[test]
        fn endpoint_access_failure_preserves_original_box_and_complete_read_prefix() {
            #[derive(Debug, PartialEq, Eq)]
            struct Opaque(&'static str);
            let mut original = Some(Box::new(Opaque("held source failure")));
            let address = original.as_ref().unwrap().as_ref() as *const Opaque;
            let mut reads = Vec::new();
            let error = range(true, true)
                .allows(ValueRef::Int(3), |column| {
                    reads.push(column);
                    if column == 0 {
                        Ok(ValueRef::Int(4))
                    } else {
                        Err(original.take().unwrap())
                    }
                })
                .unwrap_err();
            assert_eq!(reads, vec![0, 1]);
            let RelationRangeFailure::Access(error) = error else {
                panic!("original access owner expected")
            };
            assert_eq!(error.as_ref() as *const Opaque, address);
        }
        #[test]
        fn runtime_type_failure_reports_all_present_endpoint_types_after_both_reads() {
            let mut reads = Vec::new();
            let error = range(true, true)
                .allows(ValueRef::Int(7), |column| {
                    reads.push(column);
                    Ok::<_, ()>(if column == 0 {
                        ValueRef::Str("wrong")
                    } else {
                        ValueRef::Float(FiniteFloat::new(8.0).unwrap())
                    })
                })
                .unwrap_err();
            assert_eq!(reads, vec![0, 1]);
            assert_eq!(
                error,
                RelationRangeFailure::Incomparable(RelationRangeTypes {
                    value: ValueType::Int,
                    lower: Some(ValueType::Str),
                    upper: Some(ValueType::Float)
                })
            );
        }
        #[test]
        fn large_signed_integer_bounds_compare_without_rounding_to_binary64() {
            let value = 9_007_199_254_740_993;
            assert!(!range(false, true)
                .allows(ValueRef::Int(value), |_| Ok::<_, ()>(ValueRef::Float(
                    FiniteFloat::new(9_007_199_254_740_992.0).unwrap()
                )))
                .unwrap());
            assert!(range(true, true)
                .allows(ValueRef::Int(i64::MAX), |column| Ok::<_, ()>(
                    if column == 0 {
                        ValueRef::Int(i64::MIN)
                    } else {
                        ValueRef::Int(i64::MAX)
                    }
                ))
                .unwrap());
            assert!(range(true, false)
                .allows(ValueRef::Int(i64::MIN), |_| Ok::<_, ()>(ValueRef::Float(
                    FiniteFloat::new(-9_223_372_036_854_775_808.0).unwrap()
                )))
                .unwrap());
        }
        #[test]
        fn known_type_mismatch_is_refused_before_any_endpoint_context_exists() {
            assert_eq!(
                RelationRange::new(
                    "range".into(),
                    CurrentField {
                        read: CurrentRead::Output(0),
                        identifier: "DATE".into()
                    },
                    Some(ValueType::Date),
                    Some(bound(0, Some(ValueType::Int))),
                    Some(bound(1, Some(ValueType::Int)))
                ),
                Err(RelationRangeError::Incomparable(RelationRangeTypes {
                    value: ValueType::Date,
                    lower: Some(ValueType::Int),
                    upper: Some(ValueType::Int)
                }))
            );
        }

        #[test]
        fn aggregate_type_projection_omits_only_the_intermediate_owner() {
            use yamaa_core::json::Value as JsonValue;

            let types = RelationRangeTypes {
                value: ValueType::Date,
                lower: None,
                upper: Some(ValueType::Int),
            };
            let diagnostic = types.diagnostic_for("columns.V.aggregate.between", None);
            assert_eq!(
                (
                    diagnostic.definition().phase,
                    diagnostic.definition().condition,
                    diagnostic.definition().requirement
                ),
                ("validation", "incomparable_range_types", Some("REQ-0121"))
            );
            assert_eq!(diagnostic.spec_paths, ["columns.V.aggregate.between"]);
            assert_eq!(
                diagnostic.context,
                [
                    ("value_type".into(), JsonValue::String("date".into())),
                    ("upper_type".into(), JsonValue::String("int".into()))
                ]
                .into()
            );
            assert_eq!(diagnostic.source_span, None);
            assert_eq!(diagnostic.operand_route, None);
            let intermediate = types.diagnostic("intermediates[0].between", "P");
            assert_eq!(intermediate.context.len(), 3);
            assert_eq!(
                intermediate.context["intermediate"],
                JsonValue::String("P".into())
            );
        }
    }
    mod relation_shapes {
        //! Metadata never coerces an unknown derivation into a stored-column kind.
        use yamaa_core::{
            dataset::{
                CurrentField, CurrentRead, RelationField, RelationFieldOrigin, RelationRange,
                RelationRangeBound, RelationRangeError, RelationShape, RelationShapeError,
            },
            value::{ColumnType, ValueType},
        };
        fn fields() -> Vec<RelationField> {
            vec![
                RelationField::stored(0, "ID".into(), ColumnType::Int),
                RelationField::stored(1, "LOW".into(), ColumnType::Float),
                RelationField {
                    identifier: "DERIVED".into(),
                    kind: None,
                    origin: RelationFieldOrigin::Derived(0),
                },
                RelationField {
                    identifier: "FLAG".into(),
                    kind: Some(ValueType::Bool),
                    origin: RelationFieldOrigin::Derived(1),
                },
            ]
        }
        #[test]
        fn projection_restricts_reads_while_key_filter_and_order_metadata_stay_available() {
            let shape = RelationShape::new(fields(), Some(vec![2])).unwrap();
            assert_eq!(shape.visible_index("DERIVED"), Some(2));
            assert_eq!(shape.visible_index("LOW"), None);
            assert_eq!(shape.index("LOW"), Some(1));
            assert_eq!(shape.fields()[2].kind, None);
            assert_eq!(shape.fields()[3].kind, Some(ValueType::Bool));
        }
        #[test]
        fn declaration_shadowing_wrong_origins_and_invalid_projection_cannot_publish_a_shape() {
            let mut f = fields();
            f[2].identifier = "LOW".into();
            assert_eq!(
                RelationShape::new(f, None),
                Err(RelationShapeError::DuplicateIdentifier)
            );
            let mut f = fields();
            f[2].origin = RelationFieldOrigin::Derived(1);
            assert_eq!(
                RelationShape::new(f, None),
                Err(RelationShapeError::InvalidOrigin)
            );
            for projection in [vec![4], vec![1, 1], vec![usize::MAX]] {
                assert_eq!(
                    RelationShape::new(fields(), Some(projection)),
                    Err(RelationShapeError::InvalidProjection)
                );
            }
        }
        #[test]
        fn only_derived_fields_may_have_unknown_or_boolean_logical_kind() {
            for kind in [None, Some(ValueType::Bool)] {
                let mut fields = fields();
                fields[0].kind = kind;
                assert_eq!(
                    RelationShape::new(fields, None),
                    Err(RelationShapeError::InvalidKind)
                );
            }
        }
        #[test]
        fn empty_projection_hides_every_field_without_discarding_donor_metadata() {
            let shape = RelationShape::new(fields(), Some(vec![])).unwrap();
            assert_eq!(shape.visible_index("ID"), None);
            assert_eq!(shape.index("ID"), Some(0));
            assert_eq!(shape.fields().len(), 4);
        }
        #[test]
        fn fixed_relation_width_precedes_coordinate_admission_and_keeps_exact_shape() {
            let fields = |count| {
                (0..count)
                    .map(|index| RelationField::stored(index, format!("F{index}"), ColumnType::Int))
                    .collect()
            };
            let shape = RelationShape::new(fields(1024), Some(vec![1023])).unwrap();
            assert_eq!(shape.fields().len(), 1024);
            assert_eq!(shape.visible_index("F1023"), Some(1023));
            assert_eq!(
                RelationShape::new(fields(1025), Some(vec![usize::MAX])),
                Err(RelationShapeError::Limit(
                    yamaa_core::limits::LimitExceeded {
                        resource: "columns",
                        limit: 1024,
                        actual: Some(1025)
                    }
                ))
            );
            assert_eq!(
                RelationShape::new(fields(1024), Some(vec![usize::MAX])),
                Err(RelationShapeError::InvalidProjection)
            );
        }

        #[test]
        fn unknown_derived_bound_kind_survives_while_forged_coordinates_or_kinds_are_rejected() {
            let shape = RelationShape::new(fields(), None).unwrap();
            let make = |column, kind| {
                RelationRange::new(
                    "range".into(),
                    CurrentField {
                        read: CurrentRead::Output(0),
                        identifier: "V".into(),
                    },
                    None,
                    Some(RelationRangeBound {
                        column,
                        identifier: "DERIVED".into(),
                        kind,
                    }),
                    None,
                )
                .unwrap()
            };
            assert!(make(2, None).validate_shape(&shape).is_ok());
            assert_eq!(
                make(2, Some(ValueType::Str)).validate_shape(&shape),
                Err(RelationRangeError::InvalidMetadata)
            );
            assert_eq!(
                make(usize::MAX, None).validate_shape(&shape),
                Err(RelationRangeError::InvalidMetadata)
            );
        }
    }
    mod row_intermediate_context {
        //! Row admission uses original named selectors and actual completed match values.
        use crate::support::document::Tree;
        use crate::support::table_schema as schema;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead,
                MatchValue, PlanError, RowMode, RowTemplate, SecondarySource, SourceSchemas,
            },
            schema::{Document, DocumentNode as N, SpecificationDocument},
            specification::{BindError, PrepareError, PreparedSpecification},
            value::{ColumnType as T, Value},
        };

        fn doc(
            reordered: bool,
            drivers: [Option<&str>; 2],
            grouped: bool,
            field: &str,
        ) -> SpecificationDocument {
            let source = |name| {
                Tree::map(vec![(
                    "value",
                    Tree::map(vec![(
                        "source",
                        Tree::map(vec![("variable", Tree::text(name))]),
                    )]),
                )])
            };
            let mut inputs = vec![
                ("SRC", Tree::map(vec![("path", Tree::text("src.csv"))])),
                ("DONOR", Tree::map(vec![("path", Tree::text("donor.csv"))])),
            ];
            if reordered {
                inputs.reverse();
            }
            let mut rows = Vec::new();
            for (id, driver) in ["first", "second"].iter().zip(drivers) {
                let mut fields = vec![
                    ("id", Tree::text(id)),
                    ("derivations", Tree::map(vec![("ROWV", source(field))])),
                ];
                if let Some(driver) = driver {
                    fields.push(("dataset", Tree::text(driver)));
                }
                if grouped {
                    fields.push((
                        "group_by",
                        Tree::List(vec![Tree::text("SRC.ID"), Tree::text("SRC.K")]),
                    ));
                }
                rows.push(Tree::map(fields));
            }
            let tree = Tree::map(vec![
                ("schema_version", Tree::text("1.0")),
                ("domain", Tree::text("TEST")),
                ("input", Tree::map(inputs)),
                ("keys", Tree::List(vec![Tree::text("ID")])),
                (
                    "intermediates",
                    Tree::List(vec![Tree::map(vec![
                        ("id", Tree::text("P")),
                        ("dataset", Tree::text("DONOR")),
                        ("key", Tree::map(vec![("K", Tree::text("KEY"))])),
                        ("no_match", Tree::Scalar(N::Null)),
                    ])]),
                ),
                (
                    "columns",
                    Tree::List(vec![
                        Tree::map(vec![
                            ("name", Tree::text("ID")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.ID")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("KEY")),
                            ("type", Tree::text("str")),
                            ("derivation", source("SRC.K")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("ROWV")),
                            ("type", Tree::text("str")),
                        ]),
                        Tree::map(vec![
                            ("name", Tree::text("COLV")),
                            ("type", Tree::text("str")),
                            ("derivation", source("P.V")),
                        ]),
                    ]),
                ),
                ("rows", Tree::List(rows)),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("out.csv")),
                        (
                            "columns",
                            Tree::List(vec![
                                Tree::text("ID"),
                                Tree::text("KEY"),
                                Tree::text("ROWV"),
                                Tree::text("COLV"),
                            ]),
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
        #[test]
        fn common_authored_input_needs_no_base_and_can_follow_secondary_input_in_written_order() {
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            for reordered in [false, true] {
                let prepared = PreparedSpecification::prepare(&doc(
                    reordered,
                    [Some("SRC"), Some("SRC")],
                    false,
                    "P.V",
                ))
                .unwrap();
                assert_eq!(prepared.source().name, "SRC");
                let schemas = if reordered {
                    vec![&donor, &src]
                } else {
                    vec![&src, &donor]
                };
                let plan = prepared.bind_sources(&schemas).unwrap();
                assert_eq!(plan.secondary()[0].name, "DONOR");
                assert_eq!(
                    plan.intermediates()[0]
                        .output_dependencies()
                        .collect::<Vec<_>>(),
                    [1]
                );
                for template in plan.templates() {
                    assert_eq!(template.mode, RowMode::Records);
                    assert_eq!(
                        template
                            .assignments
                            .iter()
                            .map(|a| a.column)
                            .collect::<Vec<_>>(),
                        [0, 1, 2]
                    );
                    assert!(matches!(
                        template.assignments[2].expression,
                        Expression::Intermediate {
                            index: 0,
                            column: 1
                        }
                    ));
                }
                assert_eq!(plan.columns().len(), 1);
                assert!(matches!(
                    plan.columns()[0].expression,
                    Expression::Intermediate {
                        index: 0,
                        column: 1
                    }
                ));
            }
        }
        #[test]
        fn unknown_and_omitted_drivers_keep_model_preflight_and_exact_typed_context() {
            for (driver, expected) in [(Some("ABSENT"), Some("ABSENT")), (None, None)] {
                let Err(PrepareError::Invalid(findings)) = PreparedSpecification::prepare(&doc(
                    false,
                    [Some("SRC"), driver],
                    false,
                    "P.V",
                )) else {
                    panic!("preflight")
                };
                assert_eq!(findings.len(), 1);
                let diagnostic = findings[0].diagnostic();
                assert_eq!(
                    (
                        diagnostic.definition().phase,
                        diagnostic.definition().condition,
                        diagnostic.definition().requirement
                    ),
                    ("validation", "driver_unavailable", Some("REQ-0034"))
                );
                assert_eq!(diagnostic.spec_paths, ["rows[1].dataset"]);
                assert_eq!(diagnostic.context.len(), 2);
                assert_eq!(
                    diagnostic.context["row"],
                    yamaa_core::json::Value::String("second".into())
                );
                assert_eq!(
                    diagnostic.context["dataset"],
                    yamaa_core::json::Value::from(
                        expected
                            .map(|v| Value::Str(v.into()))
                            .unwrap_or(Value::Missing)
                    )
                );
            }
        }
        #[test]
        fn foreign_driver_defaults_require_raw_keys_and_grouped_named_reads_bind() {
            let prepared = PreparedSpecification::prepare(&doc(
                false,
                [Some("SRC"), Some("DONOR")],
                false,
                "P.V",
            ))
            .unwrap();
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &donor]) else {
                panic!("a foreign default cannot bootstrap its own matching key")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|finding| finding.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            // REQ-0156/0157: this template's raw DONOR record must carry ID.
            assert_eq!(diagnostics.len(), 2);
            for (diagnostic, path) in diagnostics.iter().zip([
                "columns.ID.derivation.source",
                "columns.KEY.derivation.source",
            ]) {
                assert_eq!(
                    (
                        diagnostic.definition().phase,
                        diagnostic.definition().condition,
                        diagnostic.definition().requirement
                    ),
                    ("validation", "unknown_field", Some("REQ-0103"))
                );
                assert_eq!(diagnostic.spec_paths, [path]);
                assert_eq!(
                    diagnostic.context,
                    std::collections::BTreeMap::from([(
                        "identifier".into(),
                        yamaa_core::json::Value::String("DONOR.ID".into())
                    )])
                );
            }
            let prepared = PreparedSpecification::prepare(&doc(
                false,
                [Some("SRC"), Some("SRC")],
                true,
                "P.V",
            ))
            .unwrap();
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
            for template in plan.templates() {
                assert_eq!(template.mode, RowMode::Groups(vec![0, 1]));
                assert!(matches!(
                    template.assignments[2].expression,
                    Expression::Intermediate {
                        index: 0,
                        column: 1
                    }
                ));
            }
        }
        #[test]
        fn unknown_named_field_retains_full_lookup_cause_at_authored_read_boundary() {
            let prepared = PreparedSpecification::prepare(&doc(
                false,
                [Some("SRC"), Some("SRC")],
                false,
                "P.ABSENT",
            ))
            .unwrap();
            let src = schema(&[("ID", T::Str), ("K", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let Err(BindError::Invalid(findings)) = prepared.bind_sources(&[&src, &donor]) else {
                panic!("lookup")
            };
            let diagnostics = findings
                .iter()
                .flat_map(|f| f.diagnostics(prepared.source()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(diagnostics.len(), 2);
            for (index, d) in diagnostics.iter().enumerate() {
                assert_eq!(
                    (
                        d.definition().phase,
                        d.definition().condition,
                        d.definition().requirement
                    ),
                    ("validation", "unknown_field", Some("REQ-0125"))
                );
                assert_eq!(
                    d.spec_paths,
                    [format!("rows[{index}].derivations.ROWV.source")]
                );
                assert_eq!(
                    d.context,
                    std::collections::BTreeMap::from([(
                        "identifier".into(),
                        yamaa_core::json::Value::String("P.ABSENT".into())
                    )])
                );
            }
        }
        #[test]
        fn complete_source_metadata_is_preserved_even_without_source_rows() {
            let prepared = PreparedSpecification::prepare(&doc(
                false,
                [Some("SRC"), Some("SRC")],
                false,
                "P.V",
            ))
            .unwrap();
            let huge = "q".repeat(1_000_000);
            let src = schema(&[("ID", T::Str), ("K", T::Str), (&huge, T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let plan = prepared.bind_sources(&[&src, &donor]).unwrap();
            assert_eq!(plan.source().columns()[2].name, huge);
            assert_eq!(src.columns()[2].name, huge);
        }

        #[test]
        fn typed_record_plan_requires_completed_keys_and_admits_grouped_named_reads() {
            let src = schema(&[("ID", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let output = schema(&[("ID", T::Str), ("V", T::Str)]);
            for (mode, assignments, expected) in [
                (
                    RowMode::Records,
                    vec![
                        Assignment {
                            column: 1,
                            path: "rows[0].derivations.V.source".into(),
                            expression: Expression::Intermediate {
                                index: 0,
                                column: 1,
                            },
                        },
                        Assignment {
                            column: 0,
                            path: "rows[0].derivations.ID.source".into(),
                            expression: Expression::Source(0),
                        },
                    ],
                    Some(PlanError::InvalidLookup),
                ),
                (
                    RowMode::Groups(vec![0]),
                    vec![
                        Assignment {
                            column: 0,
                            path: "rows[0].derivations.ID.source".into(),
                            expression: Expression::Source(0),
                        },
                        Assignment {
                            column: 1,
                            path: "rows[0].derivations.V.source".into(),
                            expression: Expression::Intermediate {
                                index: 0,
                                column: 1,
                            },
                        },
                    ],
                    None,
                ),
            ] {
                let result = DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: src.clone(),
                        secondary: vec![SecondarySource {
                            name: "DONOR".into(),
                            schema: donor.clone(),
                        }],
                    },
                    vec![Intermediate {
                        identifier: "P".into(),
                        path: "intermediates[0]".into(),
                        source: yamaa_core::dataset::IntermediateSource::Input(0),
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
                    }],
                    output.clone(),
                    vec![RowTemplate {
                        mode,
                        assignments,
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                );
                if let Some(expected) = expected {
                    assert_eq!(result.unwrap_err(), expected);
                } else {
                    assert_eq!(
                        result.unwrap().templates()[0].mode,
                        RowMode::Groups(vec![0])
                    );
                }
            }
        }

        #[test]
        fn secondary_row_drivers_share_output_matching_without_reusing_primary_record_coordinates()
        {
            let src = schema(&[("ID", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let output = schema(&[("ID", T::Str), ("V", T::Str)]);
            let make = |read| {
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: src.clone(),
                        secondary: vec![SecondarySource {
                            name: "DONOR".into(),
                            schema: donor.clone(),
                        }],
                    },
                    vec![Intermediate {
                        identifier: "P".into(),
                        path: "intermediates[0]".into(),
                        source: yamaa_core::dataset::IntermediateSource::Input(0),
                        keys: vec![IntermediateKey {
                            source_column: 0,
                            path: "intermediates[0].key".into(),
                            value: MatchValue::Variable(read),
                        }],
                        filter: None,
                        selection: None,
                        no_match: Some(Value::Missing),
                        unique: Vec::new(),
                        derivations: None,
                        pending_schema: None,

                        range: None,
                        columns: None,
                    }],
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Driver {
                            source: 1,
                            mode: Box::new(RowMode::Records),
                        },
                        assignments: vec![
                            Assignment {
                                column: 0,
                                path: "rows[0].derivations.ID.source".into(),
                                expression: Expression::Source(0),
                            },
                            Assignment {
                                column: 1,
                                path: "rows[0].derivations.V.source".into(),
                                expression: Expression::Intermediate {
                                    index: 0,
                                    column: 1,
                                },
                            },
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
            };
            assert!(make(MatchRead::Output {
                column: 0,
                identifier: "ID".into()
            })
            .is_ok());
            assert_eq!(
                make(MatchRead::Record {
                    column: 0,
                    identifier: "SRC.ID".into()
                }),
                Err(PlanError::InvalidIntermediate)
            );
        }

        #[test]
        fn contextual_match_metadata_cannot_survive_an_executable_typed_read() {
            let source = schema(&[("ID", T::Str)]);
            let donor = schema(&[("K", T::Str), ("V", T::Str)]);
            let output = schema(&[("ID", T::Str), ("V", T::Str)]);
            for used in [false, true] {
                let result = DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: source.clone(),
                        secondary: vec![SecondarySource {
                            name: "DONOR".into(),
                            schema: donor.clone(),
                        }],
                    },
                    vec![Intermediate {
                        identifier: "P".into(),
                        path: "intermediates[0]".into(),
                        source: yamaa_core::dataset::IntermediateSource::Input(0),
                        keys: vec![IntermediateKey {
                            source_column: 0,
                            path: "intermediates[0].key".into(),
                            value: MatchValue::Contextual(
                                yamaa_core::dataset::ContextualMatch::Variable(
                                    "FUTURE.FIELD".into(),
                                ),
                            ),
                        }],
                        filter: None,
                        selection: None,
                        range: None,
                        columns: None,
                        no_match: None,
                        unique: vec![],
                        derivations: None,
                        pending_schema: None,
                    }],
                    output.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        filter: None,
                        assignments: vec![
                            Assignment {
                                column: 0,
                                path: "rows[0].derivations.ID.source".into(),
                                expression: Expression::Source(0),
                            },
                            Assignment {
                                column: 1,
                                path: "rows[0].derivations.V.source".into(),
                                expression: if used {
                                    Expression::Intermediate {
                                        index: 0,
                                        column: 1,
                                    }
                                } else {
                                    Expression::Literal(Value::Missing)
                                },
                            },
                        ],
                    }],
                    vec![],
                    vec![0],
                    vec![],
                );
                if used {
                    assert_eq!(result.unwrap_err(), PlanError::InvalidIntermediate);
                } else {
                    assert!(matches!(
                        result.unwrap().intermediates()[0].keys[0].value,
                        MatchValue::Contextual(_)
                    ));
                }
            }
        }
    }
    mod row_source_integration {
        //! Combined source containers retain row read scopes and window phase boundaries.
        use crate::support::int_schema as schema;
        use yamaa_core::{
            bound_expression::{Binding, BindingError, BoundPredicate, Read},
            dataset::{
                Assignment, DatasetPlan, Expression, FirstAvailable, OrderTerm, PlanError, RowMode,
                RowTemplate, SecondarySource, SelectionRead, SelectionSource, Window, WindowKind,
            },
            value::Value,
        };
        fn assignment(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("row.C{column}"),
            }
        }
        fn wrapped(column: usize, expression: Expression) -> Expression {
            Expression::FirstAvailable(Box::new(FirstAvailable::new(
                vec![SelectionSource {
                    path: "row.selection[0]".into(),
                    read: SelectionRead::Expression(Box::new(assignment(
                        column,
                        Expression::SourceBinding {
                            source: Box::new(assignment(column, expression)),
                            absent: None,
                            variable_absent: false,
                            selection_declared: false,
                            variable: "SOURCE.ID".into(),
                            path: "row.source".into(),
                        },
                    ))),
                }],
                Value::Missing,
            )))
        }
        #[test]
        fn hidden_raw_reads_remain_refused_after_keyed_or_secondary_row_construction() {
            let source = schema(&["ID"]);
            let output = schema(&["ID", "VALUE"]);
            for mode in [
                RowMode::Keys,
                RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
            ] {
                let result = DatasetPlan::new_with_sources(
                    source.clone(),
                    vec![SecondarySource {
                        name: "OTHER".into(),
                        schema: source.clone(),
                    }],
                    output.clone(),
                    vec![RowTemplate {
                        mode,
                        assignments: vec![assignment(0, Expression::Source(0))],
                        filter: None,
                    }],
                    vec![assignment(1, wrapped(1, Expression::Source(0)))],
                    vec![0],
                    vec![],
                );
                assert_eq!(result, Err(PlanError::InvalidKeyMode));
            }
        }
        #[test]
        fn source_selection_readers_of_windows_remain_unavailable_to_record_filters() {
            let predicate = BoundPredicate::new(
                yamaa_core::predicate_compiler::compile("VALUE > 1", "rows[0].filter").unwrap(),
                vec![Binding {
                    name: "VALUE".into(),
                    read: Read::Column(2),
                }],
            )
            .unwrap();
            let result = DatasetPlan::new(
                schema(&["ID"]),
                schema(&["ID", "SEQ", "VALUE"]),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        assignment(0, Expression::Source(0)),
                        assignment(
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
                        assignment(2, wrapped(2, Expression::Column(1))),
                    ],
                    filter: Some(predicate),
                }],
                vec![],
                vec![0],
                vec![],
            );
            assert_eq!(
                result,
                Err(PlanError::Filter(BindingError::UnavailableColumn))
            );
        }
    }
    mod self_relations {
        //! Fabricated typed SELF metadata cannot bypass row/phase admission.
        use crate::support::id_value_schema as schema;
        use yamaa_core::{
            dataset::{
                Assignment, DatasetPlan, Expression, FirstAvailable, Intermediate, IntermediateKey,
                IntermediateSource, MatchRead, MatchValue, PlanError, RowMode, RowTemplate,
                SelectionRead, SelectionSource, SourceSchemas,
            },
            value::Value,
        };
        fn assign(column: usize, expression: Expression) -> Assignment {
            Assignment {
                column,
                expression,
                path: format!("rows.C{column}"),
            }
        }
        fn selector(columns: Vec<usize>) -> Intermediate {
            Intermediate {
                identifier: "P".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::SelfRows {
                    schema: schema(),
                    columns,
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
        fn first_template_self_hidden_in_first_available_is_rejected() {
            let hidden = Expression::FirstAvailable(Box::new(FirstAvailable::new(
                vec![SelectionSource {
                    path: "rows[0].first_available.sources[0]".into(),
                    read: SelectionRead::Expression(Box::new(assign(
                        1,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    ))),
                }],
                Value::Missing,
            )));
            let template = RowTemplate {
                mode: RowMode::Records,
                assignments: vec![assign(0, Expression::Source(0)), assign(1, hidden)],
                filter: None,
            };
            assert_eq!(
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(),
                        secondary: vec![]
                    },
                    vec![selector(vec![0, 1])],
                    schema(),
                    vec![template],
                    vec![],
                    vec![0],
                    vec![]
                ),
                Err(PlanError::InvalidIntermediate)
            );
        }
        #[test]
        fn fabricated_later_column_metadata_is_rejected_and_column_coordinates_stay_output_based() {
            let mut item = selector(vec![0]);
            assert_eq!(item.source_column(&[], "ID"), Some(0));
            assert_eq!(item.source_column(&[], "V"), None);
            assert_eq!(item.source_schema(&[]), Some(&schema()));
            item.source = IntermediateSource::SelfRows {
                schema: schema(),
                columns: vec![0, 1],
            };
            let template = RowTemplate {
                mode: RowMode::Records,
                assignments: vec![assign(0, Expression::Source(0))],
                filter: None,
            };
            assert_eq!(
                DatasetPlan::new_with_intermediates(
                    SourceSchemas {
                        primary: schema(),
                        secondary: vec![]
                    },
                    vec![item],
                    schema(),
                    vec![template],
                    vec![assign(1, Expression::Literal(Value::Int(7)))],
                    vec![0],
                    vec![]
                ),
                Err(PlanError::InvalidIntermediate)
            );
        }

        #[test]
        fn typed_aggregate_raw_field_labels_cannot_invent_a_self_relation_owner() {
            use yamaa_core::{
                aggregate_compiler::compile_aggregate,
                dataset::{
                    AggregateBinding, AggregatePhase, AggregateRelation, AggregateSource,
                    BoundAggregate,
                },
            };
            let expression = compile_aggregate("SUM(P.V)", "rows[0].V.aggregate").unwrap();
            let aggregate = BoundAggregate::new(
                expression,
                AggregateRelation::Matched {
                    source: AggregateSource::Primary,
                    keys: vec![IntermediateKey {
                        source_column: 0,
                        path: "rows[0].key".into(),
                        value: MatchValue::Variable(MatchRead::Record {
                            column: 0,
                            identifier: "ID".into(),
                        }),
                    }],
                    group_by: vec![0],
                },
                vec![AggregateBinding {
                    name: "P.V".into(),
                    column: 1,
                }],
                None,
                AggregatePhase::RowConstruction,
                None,
                "rows[0].V.aggregate",
            )
            .unwrap();
            let template = RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Aggregate(Box::new(aggregate))),
                ],
                filter: None,
            };
            assert!(DatasetPlan::new_with_intermediates(
                SourceSchemas {
                    primary: schema(),
                    secondary: vec![]
                },
                vec![selector(vec![0, 1])],
                schema(),
                vec![template],
                vec![],
                vec![0],
                vec![]
            )
            .is_ok());
        }

        #[test]
        fn first_template_self_hidden_in_mapping_and_cut_is_rejected() {
            use yamaa_core::mapping_cut::{Cut, Mapping, MappingCut};
            let operations = [
                MappingCut::Mapping(
                    Mapping::new(
                        "P.V".into(),
                        "mapping".into(),
                        vec![("5".into(), Value::Int(15))],
                        true,
                        None,
                        None,
                    )
                    .unwrap(),
                ),
                MappingCut::Cut(
                    Cut::new(
                        "P.V".into(),
                        "cut".into(),
                        vec![],
                        vec!["15".into()],
                        true,
                        None,
                    )
                    .unwrap(),
                ),
            ];
            for operation in operations {
                let hidden = Expression::MappingCut {
                    source: Box::new(assign(
                        1,
                        Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    )),
                    operation,
                };
                let template = RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![assign(0, Expression::Source(0)), assign(1, hidden)],
                    filter: None,
                };
                assert_eq!(
                    DatasetPlan::new_with_intermediates(
                        SourceSchemas {
                            primary: schema(),
                            secondary: vec![]
                        },
                        vec![selector(vec![0, 1])],
                        schema(),
                        vec![template],
                        vec![],
                        vec![0],
                        vec![],
                    ),
                    Err(PlanError::InvalidIntermediate),
                );
            }
        }
    }
    mod source_ordinal {
        use yamaa_core::{
            source_ordinal::{stored_fields, Error, Ordinal},
            table::{Column, TableSchema},
            value::ColumnType,
        };

        fn schema() -> TableSchema {
            TableSchema::new(vec![Column {
                name: "ID".into(),
                kind: ColumnType::Str,
            }])
            .unwrap()
        }

        #[test]
        fn positions_and_stored_contract_are_exact_at_the_admission_boundary() {
            let original = schema();
            let admitted = Ordinal::admit(&original, 3, "REC").unwrap();
            assert_eq!(
                stored_fields(admitted.schema(), Some("REC")).unwrap(),
                original.columns()
            );
            assert_eq!(
                admitted.schema().columns()[1],
                Column {
                    name: "REC".into(),
                    kind: ColumnType::Int
                }
            );
            assert_eq!(
                (
                    admitted.value(0),
                    admitted.value(2),
                    admitted.value(3),
                    admitted.value(usize::MAX)
                ),
                (Some(1), Some(3), None, None)
            );

            assert!(matches!(
                Ordinal::admit(&original, usize::MAX, "REC"),
                Err(Error::ResourceLimit { .. })
            ));
            assert!(matches!(
                Ordinal::admit(&original, 0, "ID"),
                Err(Error::Collision)
            ));
            assert!(matches!(
                stored_fields(&original, Some("REC")),
                Err(Error::InvalidTable)
            ));
            assert!(matches!(
                stored_fields(&original, Some("ID")),
                Err(Error::InvalidTable)
            ));
            let empty = Ordinal::admit(&original, 0, "REC").unwrap();
            assert_eq!(empty.schema(), admitted.schema());
            assert_eq!(empty.value(0), None);
            assert_eq!(stored_fields(&original, None).unwrap(), original.columns());
            let edge = Ordinal::admit(&original, 10_000_000, "REC").unwrap();
            assert_eq!(edge.value(9_999_999), Some(10_000_000));
            assert!(matches!(
                Ordinal::admit(&original, 10_000_001, "REC"),
                Err(Error::ResourceLimit {
                    resource: "table_rows",
                    limit: 10_000_000,
                    actual: Some(10_000_001)
                })
            ));
            let wide = |count| {
                TableSchema::new(
                    (0..count)
                        .map(|index| Column {
                            name: format!("F{index}"),
                            kind: ColumnType::Str,
                        })
                        .collect(),
                )
                .unwrap()
            };
            let admitted = Ordinal::admit(&wide(1023), 625_000, "REC").unwrap();
            assert_eq!(admitted.schema().columns().len(), 1024);
            assert!(matches!(
                Ordinal::admit(&wide(1024), 0, "REC"),
                Err(Error::ResourceLimit {
                    resource: "columns",
                    limit: 1024,
                    actual: Some(1025)
                })
            ));
            assert!(matches!(
                Ordinal::admit(&wide(1023), 625_001, "REC"),
                Err(Error::ResourceLimit {
                    resource: "table_cells",
                    limit: 640_000_000,
                    actual: Some(640_001_024)
                })
            ));
        }
    }
    mod verification_logs {
        use std::collections::BTreeMap;
        use yamaa_core::json::Value as Json;
        use yamaa_core::{
            dataset::VerificationSeverity,
            verification_logs::{self, Error, Failure, PreparedCell, Record},
        };

        fn held() -> Record {
            Record {
                path: "columns.AGE.verifications[0].not_missing".into(),
                id: None,
                check: "not_missing".into(),
                target: Some("AGE".into()),
                requirement: "REQ-0376".into(),
                severity: VerificationSeverity::Error,
                evaluated: 8,
                failure: None,
            }
        }
        fn warning() -> Record {
            Record {
                path: "columns.AGE.verifications[1].range".into(),
                id: Some("é🙂".into()),
                check: "range".into(),
                target: Some("AGE".into()),
                requirement: "REQ-0377".into(),
                severity: VerificationSeverity::Warning,
                evaluated: 8,
                failure: Some(Failure {
                    condition: "range_failed".into(),
                    count: 7,
                    keys: (0..7)
                        .map(|n| Json::Object([("ID".into(), Json::from(n))].into_iter().collect()))
                        .collect(),
                    details: BTreeMap::from([
                        ("z".into(), Json::String("é🙂\n\t\0\"\\".into())),
                        ("minimum".into(), Json::from(i64::MIN)),
                    ]),
                }),
            }
        }
        #[test]
        fn governed_column_schemas_stay_owned_by_core() {
            assert_eq!(
                verification_logs::VERIFICATION_COLUMNS,
                [
                    "REPORT_VERSION",
                    "ARTIFACT",
                    "SPEC_PATH",
                    "VERIFICATION_ID",
                    "CHECK",
                    "TARGET",
                    "REQUIREMENT",
                    "SEVERITY",
                    "OUTCOME",
                    "CONDITION",
                    "EVALUATED_COUNT",
                    "FAILURE_COUNT",
                    "DETAILS"
                ]
            );
            assert_eq!(verification_logs::WARNING_COLUMNS.len(), 10);
        }
        #[test]
        fn complete_ordered_logs_prepare_missing_metadata_typed_details_and_all_keys() {
            let prepared =
                verification_logs::prepare(&[held(), warning()], "out.csv", true, true, true)
                    .unwrap();
            let verification = prepared.verification.unwrap();
            assert_eq!(verification.len(), 2);
            assert_eq!(verification[0][3], PreparedCell::Missing);
            assert_eq!(verification[0][8], PreparedCell::Text("held".into()));
            assert_eq!(verification[0][9], PreparedCell::Missing);
            assert_eq!(verification[0][11], PreparedCell::Count(0));
            assert_eq!(verification[0][12], PreparedCell::Details(BTreeMap::new()));
            let warning_record = warning();
            let failure = warning_record.failure.as_ref().unwrap();
            assert_eq!(
                verification[1][12],
                PreparedCell::Details(failure.details.clone())
            );
            let warning_rows = prepared.warning.unwrap();
            assert_eq!(warning_rows.len(), 1);
            assert_eq!(warning_rows[0][7], PreparedCell::Count(7));
            assert_eq!(warning_rows[0][8], PreparedCell::Keys(failure.keys.clone()));
            assert_eq!(
                warning_rows[0][9],
                PreparedCell::Details(failure.details.clone())
            );
        }
        #[test]
        fn failures_expose_only_verification_prefix_and_empty_declarations_prepare_no_rows() {
            let prepared =
                verification_logs::prepare(&[warning()], "out.csv", true, true, false).unwrap();
            assert_eq!(prepared.verification.unwrap().len(), 1);
            assert!(prepared.warning.is_none());
            let prepared = verification_logs::prepare(&[], "out.csv", true, true, true).unwrap();
            assert_eq!(prepared.verification.unwrap().len(), 0);
            assert_eq!(prepared.warning.unwrap().len(), 0);
        }
        #[test]
        fn malformed_records_refuse_before_rows_are_returned() {
            for malformed in [
                {
                    let mut record = warning();
                    record.failure.as_mut().unwrap().count = 9;
                    record
                },
                {
                    let mut record = warning();
                    record
                        .failure
                        .as_mut()
                        .unwrap()
                        .details
                        .insert("keys".into(), Json::Null);
                    record
                },
                {
                    let mut record = warning();
                    record.failure.as_mut().unwrap().keys.push(Json::from(1));
                    record
                },
                {
                    let mut record = warning();
                    record
                        .failure
                        .as_mut()
                        .unwrap()
                        .details
                        // Number owns JSON grammar; a numeric log context still requires
                        // a finite binary64 value for fractional/exponent spellings.
                        .insert("bad".into(), Json::Number("1e999".parse().unwrap()));
                    record
                },
            ] {
                assert!(matches!(
                    verification_logs::prepare(&[malformed], "out.csv", true, true, true),
                    Err(Error::InvalidRecord)
                ));
            }
            assert!(matches!(
                verification_logs::prepare(&[held(), held()], "out.csv", true, true, true),
                Err(Error::InvalidRecord)
            ));
        }

        #[test]
        fn canonical_json_numbers_and_temporal_values_are_retained_as_typed_details() {
            let mut record = warning();
            let details = BTreeMap::from([
                (
                    "date".into(),
                    Json::from(yamaa_core::value::Value::Date(
                        "2025-01-02".parse().unwrap(),
                    )),
                ),
                (
                    "datetime".into(),
                    Json::from(yamaa_core::value::Value::DateTime(
                        "2025-01-02T03:04".parse().unwrap(),
                    )),
                ),
                (
                    "integer_float".into(),
                    Json::from(yamaa_core::value::Value::float(1.0)),
                ),
                (
                    "large".into(),
                    Json::from(yamaa_core::value::Value::float(1.0e20)),
                ),
                (
                    "negzero".into(),
                    Json::from(yamaa_core::value::Value::float(-0.0)),
                ),
                (
                    "tiny".into(),
                    Json::from(yamaa_core::value::Value::float(1.0e-7)),
                ),
                (
                    "wide".into(),
                    Json::Number(
                        "12345678901234567890123456789012345678901234567890"
                            .parse()
                            .unwrap(),
                    ),
                ),
            ]);
            record.failure.as_mut().unwrap().details = details.clone();
            let prepared =
                verification_logs::prepare(&[record], "out.csv", true, true, true).unwrap();
            // The governed byte spelling of these exact typed values is
            // asserted by the adapter projection tests.
            assert_eq!(
                prepared.verification.as_ref().unwrap()[0][12],
                PreparedCell::Details(details.clone())
            );
            assert_eq!(
                prepared.warning.as_ref().unwrap()[0][9],
                PreparedCell::Details(details)
            );
        }

        #[test]
        fn complete_record_sets_keep_all_rows_and_metadata_without_copy_allowances() {
            let records: Vec<_> = (0..6_000)
                .map(|index| {
                    let mut record = held();
                    record.path = format!("columns.AGE.verifications[{index}].not_missing");
                    record.id = Some(format!("check {index}"));
                    record
                })
                .collect();
            let prepared =
                verification_logs::prepare(&records, "out.csv", true, true, true).unwrap();
            let verification = prepared.verification.unwrap();
            assert_eq!(verification.len(), records.len());
            for (index, record) in records.iter().enumerate() {
                assert_eq!(
                    verification[index][2],
                    PreparedCell::Text(record.path.clone())
                );
                assert_eq!(
                    verification[index][3],
                    PreparedCell::Text(record.id.clone().unwrap())
                );
                assert_eq!(verification[index][10], PreparedCell::Count(8));
            }
            assert_eq!(prepared.warning.unwrap().len(), 0);
        }

        #[test]
        fn recursive_json_details_use_the_shared_semantic_depth_boundary() {
            use yamaa_core::limits::Limits;
            for depth in [
                Limits::SEMANTIC_DEPTH - 1,
                Limits::SEMANTIC_DEPTH,
                Limits::SEMANTIC_DEPTH + 1,
            ] {
                let mut detail = Json::Null;
                for _ in 0..depth {
                    detail = Json::Array(vec![detail]);
                }
                let mut record = held();
                record.failure = Some(Failure {
                    condition: "missing_value".into(),
                    count: 1,
                    keys: vec![],
                    details: BTreeMap::from([("nested".into(), detail)]),
                });
                let result = verification_logs::prepare(&[record], "out.csv", true, false, false);
                if depth <= Limits::SEMANTIC_DEPTH {
                    assert!(result.is_ok());
                } else {
                    let Err(Error::Limit(error)) = result else {
                        panic!("deep details must refuse before encoding")
                    };
                    assert_eq!(error.resource, "semantic_depth");
                    assert_eq!(error.limit, Limits::SEMANTIC_DEPTH);
                    assert_eq!(error.actual, Some(depth));
                }
            }
        }
    }
}
