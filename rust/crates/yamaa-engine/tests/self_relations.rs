//! SELF selection reads only borrowed completed donors; opaque source prefix is exact.
use std::cell::RefCell;
use yamaa_core::{
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, IntermediateSource,
        Keep, MatchRead, MatchValue, OrderTerm, RowMode, RowTemplate, SecondarySource,
        SourceSchemas, SourceSelection,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, Limits};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl TableAccess for Table {
    type Error = std::convert::Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        Ok((&self.rows[row][column]).into())
    }
}
fn schema(names: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        names
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
        path: format!("read.C{column}"),
    }
}
#[test]
fn completed_grouped_and_stored_driver_templates_add_no_study_reads() {
    let primary = Table {
        schema: schema(&[
            ("ID", T::Int),
            ("K", T::Int),
            ("SEQ", T::Int),
            ("V", T::Str),
        ]),
        rows: vec![
            vec![
                Value::Int(1),
                Value::Int(7),
                Value::Int(2),
                Value::Str("chosen Ω".into()),
            ],
            vec![
                Value::Int(2),
                Value::Int(7),
                Value::Int(1),
                Value::Str("other".into()),
            ],
        ],
        reads: RefCell::default(),
    };
    let other = Table {
        schema: schema(&[("ID", T::Int), ("K", T::Int)]),
        rows: vec![
            vec![Value::Int(4), Value::Int(7)],
            vec![Value::Int(5), Value::Int(7)],
        ],
        reads: RefCell::default(),
    };
    let output = schema(&[
        ("ID", T::Int),
        ("K", T::Int),
        ("SEQ", T::Int),
        ("V", T::Str),
        ("FINAL", T::Str),
    ]);
    let item = Intermediate {
        identifier: "P".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::SelfRows {
            schema: output.clone(),
            columns: vec![0, 1, 2, 3],
        },
        keys: vec![IntermediateKey {
            source_column: 1,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 1,
                identifier: "K".into(),
            }),
        }],
        filter: None,
        selection: Some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 2,
                descending: false,
                nulls_first: false,
            }],
            keep: Keep::Last,
        }),
        no_match: None,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,

        range: None,
        columns: None,
    };
    let read = || Expression::Intermediate {
        index: 0,
        column: 3,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: primary.schema.clone(),
            secondary: vec![SecondarySource {
                name: "OTHER".into(),
                schema: other.schema.clone(),
            }],
        },
        vec![item],
        output,
        vec![
            RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Source(2)),
                    assign(3, Expression::Source(3)),
                ],
                filter: None,
            },
            RowTemplate {
                mode: RowMode::Groups(vec![1]),
                assignments: vec![
                    assign(0, Expression::Literal(Value::Int(3))),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Literal(Value::Int(8))),
                    assign(3, read()),
                ],
                filter: None,
            },
            RowTemplate {
                mode: RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                    assign(2, Expression::Literal(Value::Int(9))),
                    assign(3, read()),
                ],
                filter: None,
            },
        ],
        vec![assign(4, read())],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed_sources(
        &primary,
        &[&other],
        Limits {
            source_rows: 10,
            output_rows: 10,
            output_cells: 100,
            key_cells: 100,
            work_cells: 100_000,
            scalar_text_bytes: 100_000,
            output_text_bytes: 100_000,
            identity_cells: 100,
            identity_text_bytes: 1000,
        },
    );
    let result = attempt.result.unwrap();
    assert_eq!(
        result.dataset.rows(),
        &[
            vec![
                Value::Int(1),
                Value::Int(7),
                Value::Int(2),
                Value::Str("chosen Ω".into()),
                Value::Str("chosen Ω".into())
            ],
            vec![
                Value::Int(2),
                Value::Int(7),
                Value::Int(1),
                Value::Str("other".into()),
                Value::Str("chosen Ω".into())
            ],
            vec![
                Value::Int(3),
                Value::Int(7),
                Value::Int(8),
                Value::Str("chosen Ω".into()),
                Value::Str("chosen Ω".into())
            ],
            vec![
                Value::Int(4),
                Value::Int(7),
                Value::Int(9),
                Value::Str("chosen Ω".into()),
                Value::Str("chosen Ω".into())
            ],
            vec![
                Value::Int(5),
                Value::Int(7),
                Value::Int(9),
                Value::Str("chosen Ω".into()),
                Value::Str("chosen Ω".into())
            ],
        ]
    );
    assert_eq!(
        *primary.reads.borrow(),
        vec![
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 0),
            (1, 1),
            (1, 2),
            (1, 3),
            (0, 1),
            (1, 1),
            (0, 1)
        ]
    );
    assert_eq!(*other.reads.borrow(), vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
}
