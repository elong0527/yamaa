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
