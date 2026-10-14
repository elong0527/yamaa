use std::{cell::RefCell, convert::Infallible};
use yamaa_core::{
    dataset::{Assignment, DatasetPlan, Expression, RowMode, RowTemplate},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, Limits},
    numeric_lifecycle::HandlerKind,
};
struct Table {
    schema: TableSchema,
    values: Vec<Value>,
    reads: RefCell<Vec<usize>>,
}
impl TableAccess for Table {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, _row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.reads.borrow_mut().push(column);
        Ok((&self.values[column]).into())
    }
}
fn schema(names: &[&str]) -> TableSchema {
    TableSchema::new(
        names
            .iter()
            .map(|&name| Column {
                name: name.into(),
                kind: ColumnType::Str,
            })
            .collect(),
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 1000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
#[test]
fn missing_concat_short_circuits_source_reads_and_retains_nested_handler_observations() {
    let source = schema(&["ID", "FIRST", "LAST"]);
    let table = Table {
        schema: source.clone(),
        values: vec![
            Value::Str("a".into()),
            Value::Missing,
            Value::Str("unread".into()),
        ],
        reads: RefCell::default(),
    };
    let path = "columns.RESULT.derivation.str_concat";
    let expression = Expression::StrConcat {
        operands: vec![
            Assignment {
                column: 1,
                path: format!("{path}.sources[0]"),
                expression: Expression::StrConcat {
                    operands: vec![Assignment {
                        column: 1,
                        path: format!("{path}.sources[0].str_concat.sources[0]"),
                        expression: Expression::Source(1),
                    }],
                    missing: Some(Value::Missing),
                    path: format!("{path}.sources[0].str_concat"),
                },
            },
            Assignment {
                column: 1,
                path: format!("{path}.sources[1]"),
                expression: Expression::Source(2),
            },
        ],
        missing: Some(Value::Str("fallback".into())),
        path: path.into(),
    };
    let plan = DatasetPlan::new(
        source,
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                Assignment {
                    column: 0,
                    path: "columns.ID.derivation".into(),
                    expression: Expression::Source(0),
                },
                Assignment {
                    column: 1,
                    path: path.into(),
                    expression,
                },
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&table, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [vec![Value::Str("a".into()), Value::Str("fallback".into())]]
    );
    assert_eq!(*table.reads.borrow(), [0, 1]);
    assert_eq!(
        attempt
            .handler_counts
            .iter()
            .map(|count| (count.handler, count.count))
            .collect::<Vec<_>>(),
        [(HandlerKind::Missing, 1), (HandlerKind::Missing, 1)]
    );
}
#[test]
fn equal_extremes_retain_the_first_dates_collected_precision() {
    use yamaa_core::temporal::{Date, DatePrecision};
    let source = schema(&["ID"]);
    let table = Table {
        schema: source.clone(),
        values: vec![Value::Str("a".into())],
        reads: RefCell::default(),
    };
    let output = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "RESULT".into(),
            kind: ColumnType::Date,
        },
    ])
    .unwrap();
    for largest in [true, false] {
        let operands = [DatePrecision::Month, DatePrecision::Day]
            .into_iter()
            .enumerate()
            .map(|(index, precision)| Assignment {
                column: 1,
                path: format!("columns.RESULT.derivation.greatest.sources[{index}]"),
                expression: Expression::Literal(Value::Date(
                    Date::new(2025, 1, 12, precision).unwrap(),
                )),
            })
            .collect();
        let plan = DatasetPlan::new(
            source.clone(),
            output.clone(),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    Assignment {
                        column: 0,
                        path: "columns.ID.derivation".into(),
                        expression: Expression::Source(0),
                    },
                    Assignment {
                        column: 1,
                        path: "columns.RESULT.derivation.greatest".into(),
                        expression: Expression::Extreme { operands, largest },
                    },
                ],
                filter: None,
            }],
            vec![],
            vec![0],
            vec![],
        )
        .unwrap();
        let result = plan.execute_observed(&table, limits()).result.unwrap();
        let Value::Date(date) = result.dataset.rows()[0][1] else {
            panic!("date result")
        };
        assert_eq!(date.collected_precision(), DatePrecision::Month);
    }
}

#[test]
fn equal_numeric_extremes_keep_first_present_signed_zero_and_collect_every_operand() {
    let source = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "INTEGER".into(),
            kind: ColumnType::Int,
        },
        Column {
            name: "FLOAT".into(),
            kind: ColumnType::Float,
        },
        Column {
            name: "MISSING".into(),
            kind: ColumnType::Float,
        },
    ])
    .unwrap();
    let output = TableSchema::new(vec![
        Column {
            name: "ID".into(),
            kind: ColumnType::Str,
        },
        Column {
            name: "RESULT".into(),
            kind: ColumnType::Float,
        },
    ])
    .unwrap();
    for largest in [true, false] {
        for (columns, expected) in [([3, 1, 2], 0.0_f64), ([3, 2, 1], -0.0_f64)] {
            let table = Table {
                schema: source.clone(),
                values: vec![
                    Value::Str("a".into()),
                    Value::Int(0),
                    Value::float(-0.0),
                    Value::Missing,
                ],
                reads: RefCell::default(),
            };
            let operation = if largest { "greatest" } else { "least" };
            let path = format!("columns.RESULT.derivation.{operation}");
            let operands = columns
                .into_iter()
                .enumerate()
                .map(|(index, column)| Assignment {
                    column: 1,
                    path: format!("{path}.sources[{index}]"),
                    expression: Expression::Source(column),
                })
                .collect();
            let plan = DatasetPlan::new(
                source.clone(),
                output.clone(),
                vec![RowTemplate {
                    mode: RowMode::Records,
                    assignments: vec![
                        Assignment {
                            column: 0,
                            path: "columns.ID.derivation".into(),
                            expression: Expression::Source(0),
                        },
                        Assignment {
                            column: 1,
                            path,
                            expression: Expression::Extreme { operands, largest },
                        },
                    ],
                    filter: None,
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap();
            let result = plan.execute_observed(&table, limits()).result.unwrap();
            let Value::Float(value) = result.dataset.rows()[0][1] else {
                panic!("float output")
            };
            assert_eq!(
                value.get().to_bits(),
                expected.to_bits(),
                "{operation} {columns:?}"
            );
            assert_eq!(
                *table.reads.borrow(),
                [0, columns[0], columns[1], columns[2]]
            );
        }
    }
}
