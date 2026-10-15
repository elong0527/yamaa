//! Typed current-side ownership cannot bypass phase, source or group admission.
use crate::support::int_schema as schema;
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
        IntermediateKey, IntermediateSource, MatchRead, MatchValue, PlanError, RelationPredicate,
        RowMode, RowTemplate, SecondarySource, SourceSchemas,
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
                Default::default(),
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
        yamaa_core::numeric_compiler::compile_numeric("P.V + 1", "C2", Default::default()).unwrap(),
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
            yamaa_core::numeric_compiler::compile_numeric("P.V + 1", "C2", Default::default())
                .unwrap(),
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
            predicate_compiler::compile("P.V > 0", "C2", Default::default()).unwrap(),
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
