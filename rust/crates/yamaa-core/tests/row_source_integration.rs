//! Combined source containers retain row read scopes and window phase boundaries.
use yamaa_core::{
    bound_expression::{Binding, BindingError, BoundPredicate, Read},
    dataset::{
        Assignment, DatasetPlan, Expression, FirstAvailable, OrderTerm, PlanError, RowMode,
        RowTemplate, SecondarySource, SelectionRead, SelectionSource, Window, WindowKind,
    },
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
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
        yamaa_core::predicate_compiler::compile("VALUE > 1", "rows[0].filter", Default::default())
            .unwrap(),
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
