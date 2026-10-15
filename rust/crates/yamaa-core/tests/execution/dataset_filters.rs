//! Independent ordering, admission, failure and committed benchmark observations.
use crate::support::table_schema as schema;
use yamaa_core::{
    reduction::NumericReducer,
    table::{CellError, TableSchema},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};

use yamaa_core::{
    dataset::{
        Assignment, Check, DatasetPlan, ExecutionError, Expression, Limits, PlanError, RowMode,
        RowTemplate,
    },
    table_grouping::GroupingError,
};

use super::dataset::*;

use yamaa_core::bound_expression::{Binding, BindingError, BoundPredicate as Filter, Read};
use yamaa_core::predicate::{self, Comparison, Node, Scalar};

/// Admit a single authored predicate and its explicit scope bindings.
pub(crate) fn filter(node: Node, bindings: Vec<Binding>) -> Filter {
    Filter::new(
        predicate::Plan::new(
            vec![node],
            0,
            "rows[0].filter".into(),
            "authored predicate".into(),
            predicate::Limits::default(),
        )
        .unwrap(),
        bindings,
    )
    .unwrap()
}
/// Bind an occurrence without guessing whether it belongs to source or candidate scope.
pub(crate) fn binding(name: &str, read: Read) -> Binding {
    Binding {
        name: name.into(),
        read,
    }
}
/// Keep rows whose completed value exceeds zero; missing values yield unknown.
pub(crate) fn positive(read: Read) -> Filter {
    filter(
        Node::Compare {
            operator: Comparison::Greater,
            left: Scalar::Identifier("x".into()),
            right: Scalar::Literal(Value::Int(0)),
        },
        vec![binding("x", read)],
    )
}
/// Filters preserve source membership through the later whole-column phase.
#[test]
fn row_filters_keep_true_in_original_order_and_drop_unknown() {
    let source = table(
        &[("id", ColumnType::Int), ("x", ColumnType::Int)],
        vec![
            vec![Value::Int(8), Value::Int(-1)],
            vec![Value::Int(3), Value::Int(2)],
            vec![Value::Int(7), Value::Missing],
            vec![Value::Int(1), Value::Int(4)],
        ],
    );
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(1, Expression::Source(1))],
            filter: Some(positive(Read::Column(1))),
        }],
        vec![assign(0, Expression::Source(0))],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        &[
            vec![Value::Int(3), Value::Int(2)],
            vec![Value::Int(1), Value::Int(4)]
        ]
    );
    assert_eq!(
        *source.reads.borrow(),
        vec![(0, 1), (1, 1), (2, 1), (3, 1), (1, 0), (3, 0)]
    );
}
/// Group filters see the finished reduction and preserve the surviving group's first member.
#[test]
fn grouped_filters_follow_complete_reductions() {
    let source = table(
        &[("id", ColumnType::Int), ("x", ColumnType::Int)],
        vec![
            vec![Value::Int(8), Value::Int(-3)],
            vec![Value::Int(3), Value::Int(2)],
            vec![Value::Int(8), Value::Int(1)],
            vec![Value::Int(3), Value::Int(4)],
        ],
    );
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![assign(
                1,
                Expression::Reduce {
                    identifier: None,
                    column: 1,
                    reducer: NumericReducer::Sum,
                    text: "SUM(T.x)".into(),
                },
            )],
            filter: Some(positive(Read::Column(1))),
        }],
        vec![assign(0, Expression::Source(0))],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        &[vec![Value::Int(3), Value::Int(6)]]
    );
}
/// A false filter cannot hide earlier conversion failures; empty input invents no evaluations.
#[test]
fn filter_timing_preserves_conversion_and_empty_input_behavior() {
    let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Literal(Value::Bool(true)))],
            filter: Some(filter(Node::Boolean(false), vec![])),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Conversion { .. }
    ));
    let empty = table(&[("id", ColumnType::Int)], vec![]);
    assert!(plan
        .execute(&empty, limits())
        .unwrap()
        .dataset
        .rows()
        .is_empty());
}
/// Whole-plan binding errors are caught before a source cell is accessed.
#[test]
fn filter_bindings_enforce_complete_names_and_phase_scope() {
    for (bindings, expected) in [
        (vec![], BindingError::MissingName),
        (
            vec![binding("x", Read::Source(0)), binding("x", Read::Source(0))],
            BindingError::DuplicateName,
        ),
        (
            vec![binding("y", Read::Source(0))],
            BindingError::UnusedName,
        ),
    ] {
        let predicate = predicate::Plan::new(
            vec![Node::IsNull {
                value: Scalar::Identifier("x".into()),
                negated: false,
            }],
            0,
            "rows[0].filter".into(),
            "x IS NULL".into(),
            predicate::Limits::default(),
        )
        .unwrap();
        assert_eq!(Filter::new(predicate, bindings), Err(expected));
    }
    let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
    for (mode, read, expected) in [
        (
            RowMode::Records,
            Read::Column(0),
            BindingError::UnavailableColumn,
        ),
        (
            RowMode::Records,
            Read::Source(1),
            BindingError::InvalidSource,
        ),
        (
            RowMode::Groups(vec![0]),
            Read::Source(0),
            BindingError::GroupedSource,
        ),
    ] {
        let result = DatasetPlan::new(
            source.schema.clone(),
            source.schema.clone(),
            vec![RowTemplate {
                mode,
                assignments: vec![],
                filter: Some(positive(read)),
            }],
            vec![assign(0, Expression::Source(0))],
            vec![0],
            vec![],
        );
        assert_eq!(result, Err(PlanError::Filter(expected)));
    }
    assert!(source.reads.borrow().is_empty());
}
/// Resolved source errors remain opaque and stop at the first written occurrence.
#[test]
fn filter_source_errors_keep_occurrence_and_provenance() {
    let mut source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
    source.fail = Some((0, 0));
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![],
            filter: Some(positive(Read::Source(0))),
        }],
        vec![assign(0, Expression::Source(0))],
        vec![0],
        vec![],
    )
    .unwrap();
    let ExecutionError::Predicate { source_row, error } =
        *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("predicate port failure")
    };
    assert_eq!(source_row, 0);
    assert_eq!(error.spec_path, "rows[0].filter");
    assert!(matches!(
        error.kind,
        predicate::ErrorKind::Resolution {
            error: CellError::Access("source failure"),
            ..
        }
    ));
    assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
}
/// Discarding values refunds retained output bytes, never cumulative operand bytes.
#[test]
fn rejected_candidates_release_retained_text_but_not_work() {
    use yamaa_core::dataset::Resource;
    let source = table(
        &[("id", ColumnType::Int)],
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
    );
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[("id", ColumnType::Int), ("x", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(1, Expression::Literal(Value::Str("abc".into())))],
            filter: Some(filter(Node::Boolean(false), vec![])),
        }],
        vec![assign(0, Expression::Source(0))],
        vec![0],
        vec![],
    )
    .unwrap();
    let policy = Limits {
        output_text_bytes: 3,
        scalar_text_bytes: 6,
        ..limits()
    };
    for _ in 0..2 {
        assert!(plan
            .execute(&source, policy)
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 5,
                    ..policy
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            limit: 5,
            required: Some(6)
        }
    ));
}
/// Predicate operands and LIKE cells accumulate across rows and restart only for a new run.
#[test]
fn filter_limits_are_cumulative_and_recover_on_new_execution() {
    use yamaa_core::dataset::Resource;
    let source = table(
        &[("id", ColumnType::Str)],
        vec![vec![Value::Str("abc".into())]; 2],
    );
    let predicate = filter(
        Node::Like {
            value: Scalar::Identifier("x".into()),
            pattern: Scalar::Literal(Value::Str("z".into())),
            escape: None,
            negated: false,
        },
        vec![binding("x", Read::Source(0))],
    );
    let plan = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![],
            filter: Some(predicate),
        }],
        vec![assign(0, Expression::Source(0))],
        vec![0],
        vec![],
    )
    .unwrap();
    for (policy, resource) in [
        (
            Limits {
                scalar_text_bytes: 7,
                ..limits()
            },
            Resource::PredicateTextBytes,
        ),
        (
            Limits {
                work_cells: 10,
                ..limits()
            },
            Resource::PredicateLikeWork,
        ),
    ] {
        assert!(
            matches!(*plan.execute(&source, policy).unwrap_err(), ExecutionError::Limit { resource: r, .. } if r == resource)
        );
        assert!(plan
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}

/// Supply a verification predicate with its actual owning declaration path.
pub(crate) fn check_predicate(node: Node, bindings: Vec<Binding>, path: &str) -> Filter {
    Filter::new(
        predicate::Plan::new(
            vec![node],
            0,
            path.into(),
            "authored verification predicate".into(),
            predicate::Limits::default(),
        )
        .unwrap(),
        bindings,
    )
    .unwrap()
}
/// Asserts fail on false and unknown; with `when` they fail only for true -> nontrue.
#[test]
fn predicate_checks_follow_truth_tables_and_keep_every_record() {
    let source = table(
        &[("id", ColumnType::Int)],
        vec![vec![Value::Int(1)], vec![Value::Int(2)]],
    );
    let truths = [
        Node::Boolean(true),
        Node::Boolean(false),
        Node::Compare {
            operator: Comparison::Equal,
            left: Scalar::Literal(Value::Missing),
            right: Scalar::Literal(Value::Int(1)),
        },
    ];
    for (index, node) in truths.iter().enumerate() {
        let plan = record_plan(
            &source,
            source.schema.clone(),
            vec![assign(0, Expression::Source(0))],
            vec![verification(Check::Assert {
                when: None,
                require: check_predicate(node.clone(), vec![], "verifications[0].assert.require"),
            })],
        );
        let result = plan.execute(&source, limits());
        if index == 0 {
            let records = result.unwrap().verifications;
            assert_eq!(records[0].evaluated_count, 2);
            assert_eq!(records[0].failed_count, 0);
        } else {
            let ExecutionError::VerificationFailures(records) = *result.unwrap_err() else {
                panic!("assert data failure")
            };
            assert_eq!(records[0].definition().condition, "assert_failed");
            assert_eq!(records[0].failed_count, 2);
            assert_eq!(
                records[0]
                    .offending_rows
                    .iter()
                    .map(|row| row.position)
                    .collect::<Vec<_>>(),
                vec![0, 1]
            );
        }
    }
    for (a, when) in truths.iter().enumerate() {
        for (b, require) in truths.iter().enumerate() {
            let plan = record_plan(
                &source,
                source.schema.clone(),
                vec![assign(0, Expression::Source(0))],
                vec![
                    verification(Check::Assert {
                        when: Some(check_predicate(
                            when.clone(),
                            vec![],
                            "verifications[0].assert.when",
                        )),
                        require: check_predicate(
                            require.clone(),
                            vec![],
                            "verifications[0].assert.require",
                        ),
                    }),
                    verification(Check::RowCount {
                        min: Some(2),
                        max: Some(2),
                    }),
                ],
            );
            let result = plan.execute(&source, limits());
            if a == 0 && b != 0 {
                let ExecutionError::VerificationFailures(records) = *result.unwrap_err() else {
                    panic!("guarded assert data failure")
                };
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].definition().condition, "assert_failed");
                assert_eq!(records[0].definition().requirement.unwrap(), "REQ-0384");
                assert_eq!(records[0].failed_count, 2);
                assert_eq!(records[1].failed_count, 0);
            } else {
                assert_eq!(result.unwrap().verifications[0].failed_count, 0);
            }
        }
    }
}
/// Invalid declarations fail even for empty output, preserving earlier successful/failed checks.
#[test]
fn predicate_declaration_validation_is_sequential_on_empty_output() {
    for count in [0, 1] {
        let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]; count]);
        let invalid = check_predicate(
            Node::Compare {
                operator: Comparison::Equal,
                left: Scalar::Identifier("id".into()),
                right: Scalar::Literal(Value::Str("bad".into())),
            },
            vec![binding("id", Read::Column(0))],
            "verifications[1].assert.require",
        );
        let plan = record_plan(
            &source,
            source.schema.clone(),
            vec![assign(0, Expression::Source(0))],
            vec![
                verification(Check::RowCount {
                    min: Some(2),
                    max: None,
                }),
                verification(Check::Assert {
                    when: Some(check_predicate(
                        Node::Boolean(false),
                        vec![],
                        "verifications[1].assert.when",
                    )),
                    require: invalid,
                }),
                verification(Check::RowCount {
                    min: Some(0),
                    max: None,
                }),
            ],
        );
        let ExecutionError::VerificationPredicate { error, records } =
            *plan.execute(&source, limits()).unwrap_err()
        else {
            panic!("predicate declaration failure")
        };
        assert_eq!(error.spec_path, "verifications[1].assert.require");
        assert!(matches!(
            error.kind,
            predicate::ErrorKind::Condition(predicate::Condition::IncompatiblePair { .. })
        ));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].failed_count, 1);
        assert_eq!(records[0].output_rows, count);
    }
}
/// A false `when` never masks a later dynamic `require` pattern failure.
#[test]
fn assert_when_evaluates_dynamic_require_and_keeps_prefix() {
    let source = table(
        &[("id", ColumnType::Int), ("p", ColumnType::Str)],
        vec![
            vec![Value::Int(1), Value::Str("ok".into())],
            vec![Value::Int(2), Value::Str("tail!".into())],
        ],
    );
    let require = check_predicate(
        Node::Like {
            value: Scalar::Literal(Value::Str("text".into())),
            pattern: Scalar::Identifier("p".into()),
            escape: Some('!'),
            negated: false,
        },
        vec![binding("p", Read::Column(1))],
        "verifications[1].assert.require",
    );
    let plan = record_plan(
        &source,
        source.schema.clone(),
        vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Source(1)),
        ],
        vec![
            verification(Check::RowCount {
                min: Some(2),
                max: Some(2),
            }),
            verification(Check::Assert {
                when: Some(check_predicate(
                    Node::Boolean(false),
                    vec![],
                    "verifications[1].assert.when",
                )),
                require,
            }),
        ],
    );
    let ExecutionError::VerificationPredicate { error, records } =
        *plan.execute(&source, limits()).unwrap_err()
    else {
        panic!("dynamic require failure")
    };
    assert_eq!(
        error.kind,
        predicate::ErrorKind::Condition(predicate::Condition::DanglingEscape)
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].failed_count, 0);
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
}
/// Check predicates cannot escape completed-output scope and read source values.
#[test]
fn predicate_checks_admit_only_completed_output_bindings() {
    let source = table(&[("id", ColumnType::Int)], vec![vec![Value::Int(1)]]);
    for (read, error) in [
        (Read::Source(0), BindingError::GroupedSource),
        (Read::Column(1), BindingError::UnavailableColumn),
    ] {
        let plan = DatasetPlan::new(
            source.schema.clone(),
            source.schema.clone(),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![],
                filter: None,
            }],
            vec![assign(0, Expression::Source(0))],
            vec![0],
            vec![verification(Check::Assert {
                when: None,
                require: positive(read),
            })],
        );
        assert_eq!(plan, Err(PlanError::Predicate(error)));
    }
    assert!(source.reads.borrow().is_empty());
}

/// A compiler checkpoint validates only a declaration, never data or a check record.
#[test]
fn predicate_declaration_checkpoint_does_not_evaluate_rows() {
    let source = table(
        &[("id", ColumnType::Int), ("p", ColumnType::Str)],
        vec![vec![Value::Int(1), Value::Str("tail!".into())]],
    );
    let predicate = check_predicate(
        Node::Like {
            value: Scalar::Literal(Value::Str("text".into())),
            pattern: Scalar::Identifier("p".into()),
            escape: Some('!'),
            negated: false,
        },
        vec![binding("p", Read::Column(1))],
        "verifications[1].assert.when",
    );
    let plan = record_plan(
        &source,
        source.schema.clone(),
        vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Source(1)),
        ],
        vec![
            verification(Check::RowCount {
                min: Some(1),
                max: Some(1),
            }),
            verification(Check::PredicateDeclaration(predicate)),
        ],
    );
    let actual = plan.execute(&source, limits()).unwrap();
    assert_eq!(actual.verifications.len(), 1);
    assert_eq!(actual.dataset.rows(), source.rows);
}

/// Sample validation consumes the same cumulative budget, including on empty datasets.
#[test]
fn predicate_check_samples_share_budgets_and_fresh_runs_recover() {
    use yamaa_core::dataset::Resource;
    let source = table(&[("id", ColumnType::Str)], vec![]);
    let plan = record_plan(
        &source,
        source.schema.clone(),
        vec![assign(0, Expression::Source(0))],
        vec![
            verification(Check::Assert {
                when: None,
                require: check_predicate(
                    Node::Boolean(true),
                    vec![],
                    "verifications[0].assert.require",
                ),
            }),
            verification(Check::Assert {
                when: None,
                require: check_predicate(
                    Node::Boolean(true),
                    vec![],
                    "verifications[1].assert.require",
                ),
            }),
        ],
    );
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    work_cells: 1,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::PredicateWork,
            limit: 1,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications.len(),
        2
    );
    let plan = record_plan(
        &source,
        source.schema.clone(),
        vec![assign(0, Expression::Source(0))],
        vec![verification(Check::Assert {
            when: None,
            require: check_predicate(
                Node::IsNull {
                    value: Scalar::Identifier("id".into()),
                    negated: true,
                },
                vec![binding("id", Read::Column(0))],
                "verifications[0].assert.require",
            ),
        })],
    );
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 5,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::PredicateTextBytes,
            limit: 5,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications[0].evaluated_count,
        0
    );
}

#[test]
fn all_or_none_declared_reads_share_work_budget_and_fresh_runs_recover() {
    use yamaa_core::dataset::Resource;
    let source = table(
        &[
            ("ID", ColumnType::Int),
            ("V", ColumnType::Int),
            ("W", ColumnType::Int),
        ],
        vec![vec![Value::Int(1), Value::Int(2), Value::Int(3)]],
    );
    let assignments = || {
        (0..3)
            .map(|column| assign(column, Expression::Source(column)))
            .collect()
    };
    let policy = Limits {
        work_cells: 100,
        ..limits()
    };
    let control = record_plan(&source, source.schema.clone(), assignments(), vec![]);
    assert!(control.execute(&source, policy).is_ok());
    let columns = (0..5001).map(|index| 1 + index % 2).collect();
    let plan = record_plan(
        &source,
        source.schema.clone(),
        assignments(),
        vec![verification(Check::AllOrNone(columns))],
    );
    assert!(matches!(
        *plan.execute(&source, policy).unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            limit: 100,
            ..
        }
    ));
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(result.verifications.len(), 1);
    assert_eq!(result.verifications[0].failed_count, 0);
    assert_eq!(result.verifications[0].evaluated_count, 1);
}

/// Bind no-template keys separately from later whole-column derivations.
pub(crate) fn key_plan(
    source: &Table,
    output: TableSchema,
    keys: Vec<usize>,
    key_assignments: Vec<Assignment>,
    columns: Vec<Assignment>,
) -> DatasetPlan {
    DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: key_assignments,
            filter: None,
        }],
        columns,
        keys,
        vec![],
    )
    .unwrap()
}
/// Read one distinct present raw source value across the complete feeding group.
pub(crate) fn collect(column: usize) -> Expression {
    Expression::Collect {
        column,
        identifier: format!("SRC.C{column}"),
        filter: None,
        selection: None,
    }
}
/// Converted identity collapses records in first appearance; source readings omit missing.
#[test]
fn key_grain_preserves_order_membership_and_completed_conversion() {
    let source = table(
        &[("id", ColumnType::Str), ("value", ColumnType::Str)],
        vec![
            vec![Value::Str("02".into()), Value::Missing],
            vec![Value::Str("01".into()), Value::Str("7".into())],
            vec![Value::Str("2".into()), Value::Str("9".into())],
            vec![Value::Str("2".into()), Value::Str("9".into())],
        ],
    );
    let plan = key_plan(
        &source,
        schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        vec![assign(1, collect(1))],
    );
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(
        result.dataset.rows(),
        &[
            vec![Value::Int(2), Value::Int(9)],
            vec![Value::Int(1), Value::Int(7)]
        ]
    );
    assert_eq!(
        *source.reads.borrow(),
        vec![
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (0, 1),
            (2, 1),
            (3, 1),
            (1, 1)
        ]
    );
    let reordered = key_plan(
        &source,
        schema(&[("ID", ColumnType::Int), ("OTHER", ColumnType::Int)]),
        vec![1, 0],
        vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Literal(Value::Int(99))),
        ],
        vec![],
    );
    assert_eq!(
        reordered.execute(&source, limits()).unwrap().dataset.rows(),
        &[
            vec![Value::Int(2), Value::Int(99)],
            vec![Value::Int(1), Value::Int(99)]
        ]
    );
}
/// Every raw distinct value is counted before target conversion; later port errors still win.
#[test]
fn key_grain_conflicts_count_all_raw_values_and_keep_identity() {
    let mut source = table(
        &[("id", ColumnType::Int), ("value", ColumnType::Str)],
        vec![
            vec![Value::Int(1), Value::Str("07".into())],
            vec![Value::Int(1), Value::Str("7".into())],
            vec![Value::Int(1), Value::Str("8".into())],
            vec![Value::Int(1), Value::Missing],
            vec![Value::Int(1), Value::Str("8".into())],
        ],
    );
    let plan = key_plan(
        &source,
        schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        vec![assign(1, collect(1))],
    );
    let error = plan.execute(&source, limits()).unwrap_err();
    let ExecutionError::MultipleValues {
        path,
        identifier,
        value_count,
        identity,
    } = *error
    else {
        panic!("{error:?}")
    };
    assert_eq!(
        (path.as_str(), identifier.as_str(), value_count),
        ("columns.C1.derivation", "SRC.C1", 3)
    );
    let identity = identity.unwrap();
    assert_eq!(identity.position, 0);
    assert_eq!(identity.values, vec![Value::Int(1)]);
    source.fail = Some((4, 1));
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Cell {
            source_row: 4,
            error: CellError::Access("source failure"),
            ..
        }
    ));
}
/// Key errors precede every non-key derivation, including on an earlier input record.
#[test]
fn key_grain_completes_all_key_probes_before_nonkeys() {
    let source = table(
        &[("id", ColumnType::Str), ("value", ColumnType::Str)],
        vec![
            vec![Value::Str("1".into()), Value::Str("bad value".into())],
            vec![Value::Str("bad key".into()), Value::Str("7".into())],
        ],
    );
    let plan = key_plan(
        &source,
        schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        vec![assign(1, collect(1))],
    );
    assert!(
        matches!(*plan.execute(&source,limits()).unwrap_err(),ExecutionError::Conversion {path,identity:None,..} if path=="columns.ID")
    );
    assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
}
/// Missing records remain separate and cannot collide with sentinel-shaped user identities.
#[test]
fn key_grain_missing_records_reach_output_gate_without_collapsing() {
    let source = table(
        &[
            ("id", ColumnType::Str),
            ("tag", ColumnType::Str),
            ("value", ColumnType::Str),
        ],
        vec![
            vec![
                Value::Str("1".into()),
                Value::Str("__missing_key__".into()),
                Value::Str("7".into()),
            ],
            vec![
                Value::Missing,
                Value::Str("present".into()),
                Value::Str("8".into()),
            ],
            vec![
                Value::Missing,
                Value::Str("present".into()),
                Value::Str("9".into()),
            ],
        ],
    );
    let plan = key_plan(
        &source,
        schema(&[
            ("ID", ColumnType::Int),
            ("TAG", ColumnType::Str),
            ("VALUE", ColumnType::Int),
        ]),
        vec![1, 0],
        vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Source(1)),
        ],
        vec![assign(2, collect(2))],
    );
    let error = plan.execute(&source, limits()).unwrap_err();
    let ExecutionError::KeyFailures(records) = *error else {
        panic!("{error:?}")
    };
    assert_eq!(records.len(), 2);
    assert_eq!(
        (
            records[0].path.as_str(),
            records[0].failed_count,
            records[0].output_rows
        ),
        ("keys[1]", 2, 3)
    );
    assert_eq!(
        records[0]
            .offending_rows
            .iter()
            .map(|r| r.position)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        records[0].offending_rows[0].values,
        vec![Value::Str("present".into()), Value::Missing]
    );
    assert_eq!(
        (records[1].definition().condition, records[1].failed_count),
        ("duplicate_key", 1)
    );
}
/// Key storage is admitted before reads; collapsed output and released duplicate text have independent bounds.
#[test]
fn key_grain_budgets_cover_probes_and_release_duplicates() {
    let source = table(
        &[("id", ColumnType::Str), ("value", ColumnType::Str)],
        vec![vec![Value::Str("same".into()), Value::Str("abcdefgh".into())]; 3],
    );
    let plan = key_plan(
        &source,
        schema(&[("ID", ColumnType::Str), ("VALUE", ColumnType::Str)]),
        vec![0],
        vec![assign(0, Expression::Source(0))],
        vec![assign(1, collect(1))],
    );
    let bounded = Limits {
        output_rows: 1,
        output_cells: 2,
        key_cells: 3,
        output_text_bytes: 12,
        ..limits()
    };
    assert_eq!(
        plan.execute(&source, bounded).unwrap().dataset.rows().len(),
        1
    );
    source.reads.borrow_mut().clear();
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    key_cells: 2,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Grouping(GroupingError::KeyCellLimit { limit: 2 })
    ));
    assert!(source.reads.borrow().is_empty());
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    output_text_bytes: 11,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::OutputTextBytes,
            ..
        }
    ));
    assert!(matches!(
        *plan
            .execute(
                &source,
                Limits {
                    scalar_text_bytes: 35,
                    ..bounded
                }
            )
            .unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    assert!(plan.execute(&source, bounded).is_ok());
    let empty = table(
        &[("id", ColumnType::Str), ("value", ColumnType::Str)],
        vec![],
    );
    assert!(plan
        .execute(&empty, bounded)
        .unwrap()
        .dataset
        .rows()
        .is_empty());
}
/// Collected values use exact typed equality and preserve the first present temporal precision/zero sign.
#[test]
fn collected_values_preserve_exact_identity_and_first_representation() {
    for (kind, values, expected) in [
        (
            ColumnType::Int,
            vec![Value::Int(9007199254740992), Value::Int(9007199254740993)],
            None,
        ),
        (
            ColumnType::Float,
            vec![Value::float(-0.0), Value::float(0.0)],
            Some(Value::float(-0.0)),
        ),
        (
            ColumnType::Date,
            vec![
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Year).unwrap()),
                Value::Date(Date::new(2024, 1, 1, DatePrecision::Day).unwrap()),
            ],
            Some(Value::Date(
                Date::new(2024, 1, 1, DatePrecision::Year).unwrap(),
            )),
        ),
        (
            ColumnType::Str,
            vec![Value::Missing, Value::Missing],
            Some(Value::Missing),
        ),
    ] {
        let source = table(
            &[("value", kind)],
            values.into_iter().map(|v| vec![v]).collect(),
        );
        let plan = key_plan(
            &source,
            schema(&[("ID", ColumnType::Int), ("VALUE", kind)]),
            vec![0],
            vec![assign(0, Expression::Literal(Value::Int(1)))],
            vec![assign(1, collect(0))],
        );
        match expected {
            None => assert!(matches!(
                *plan.execute(&source, limits()).unwrap_err(),
                ExecutionError::MultipleValues { value_count: 2, .. }
            )),
            Some(expected) => {
                let result = plan.execute(&source, limits()).unwrap();
                assert_eq!(result.dataset.rows()[0][1], expected);
                if let Value::Float(v) = &result.dataset.rows()[0][1] {
                    assert_eq!(v.get().to_bits(), (-0.0_f64).to_bits());
                }
                if let Value::Date(v) = &result.dataset.rows()[0][1] {
                    assert_eq!(v.collected_precision(), DatePrecision::Year);
                }
            }
        }
    }
}
/// Typed admission rejects incomplete key sets and collection-only reducers in key phase.
#[test]
fn key_grain_rejects_incomplete_or_wrong_phase_assignments() {
    let source = schema(&[("id", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]);
    for (assignments, columns) in [
        (
            vec![],
            vec![assign(0, Expression::Source(0)), assign(1, collect(0))],
        ),
        (
            vec![assign(
                0,
                Expression::Reduce {
                    column: 0,
                    reducer: NumericReducer::Sum,
                    text: "SUM(SRC.id)".into(),
                    identifier: Some("SRC.id".into()),
                },
            )],
            vec![assign(1, collect(0))],
        ),
        (
            vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Literal(Value::Int(1))),
            ],
            vec![],
        ),
        (
            vec![assign(0, Expression::Source(0))],
            vec![assign(1, Expression::Source(0))],
        ),
    ] {
        assert_eq!(
            DatasetPlan::new(
                source.clone(),
                output.clone(),
                vec![RowTemplate {
                    mode: RowMode::Keys,
                    assignments,
                    filter: None
                }],
                columns,
                vec![0],
                vec![]
            )
            .unwrap_err(),
            PlanError::InvalidKeyMode
        );
    }
}
