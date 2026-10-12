//! Fabricated typed SELF metadata cannot bypass row/phase admission.
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, FirstAvailable, Intermediate, IntermediateKey,
        IntermediateSource, MatchRead, MatchValue, PlanError, RowMode, RowTemplate, SelectionRead,
        SelectionSource, SourceSchemas,
    },
    table::{Column, TableSchema},
    value::{ColumnType, Value},
};
fn schema() -> TableSchema {
    TableSchema::new(
        ["ID", "V"]
            .iter()
            .map(|name| Column {
                name: (*name).into(),
                kind: ColumnType::Int,
            })
            .collect(),
    )
    .unwrap()
}
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
