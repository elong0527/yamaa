//! Primary donor metadata must describe the already admitted original input.
use crate::support::id_value_schema as schema;
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource,
        MatchRead, MatchValue, PlanError, RowMode, RowTemplate, SourceSchemas,
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
