//! Admission contracts run with no engine or table port dependency.
use yamaa_core::{
    dataset::{
        Assignment, Check, DatasetPlan, Expression, PlanError, RowMode, RowTemplate, Verification,
    },
    reduction::NumericReducer,
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};

fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|(name, kind)| Column {
                name: (*name).into(),
                kind: *kind,
            })
            .collect(),
    )
    .unwrap()
}
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
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
        make().with_column_verifications(vec![group(0, "wrong-phase", Check::Unique(vec![0]))]),
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
    assert_eq!(
        make(
            vec![
                records(vec![]),
                grouped(vec![assign(0, Expression::Source(0))])
            ],
            vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Literal(Value::Missing))
            ]
        ),
        Err(PlanError::InconsistentRowColumns)
    );
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
                Expression::FirstAvailable(Box::new(FirstAvailable::new(sources, Value::Int(99)))),
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
    assert_eq!(
        DatasetPlan::new(
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
        ),
        Err(PlanError::InvalidKeyMode)
    );
    assert_eq!(
        DatasetPlan::new(
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
        ),
        Err(PlanError::InvalidKeyMode)
    );
}
