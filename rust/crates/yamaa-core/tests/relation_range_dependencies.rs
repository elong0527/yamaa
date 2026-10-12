//! A kept current read inherits the earlier selector's window dependency.
use yamaa_core::{
    bound_expression::{Binding, BindingError, BoundPredicate, Read},
    dataset::{
        visit_output_dependencies, window_stages, Assignment, CurrentField, CurrentRead,
        DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource, MatchRead,
        MatchValue, OrderTerm, PlanError, RelationRange, RelationRangeBound, RowMode, RowTemplate,
        SecondarySource, SourceSchemas, Window, WindowKind,
    },
    table::{Column, TableSchema},
    value::{ColumnType as T, Value, ValueType},
};
fn schema(fields: &[(&str, T)]) -> TableSchema {
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
}
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
                4096,
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
        yamaa_core::predicate_compiler::compile("VALUE > 0", "row.filter", Default::default())
            .unwrap(),
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
            4096,
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
