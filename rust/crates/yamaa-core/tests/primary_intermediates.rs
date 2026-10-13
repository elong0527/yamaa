//! Primary donor metadata must describe the already admitted original input.
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource,
        MatchRead, MatchValue, PlanError, RowMode, RowTemplate, SourceSchemas,
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
