use crate::dataset::Limits;
use crate::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, DatasetPlan, Expression, OrderTerm, RowMode, RowTemplate, Window, WindowKind,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use std::cell::RefCell;
use std::vec;

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
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("row.C{column}"),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 10,
        output_rows: 10,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10_000,
        scalar_text_bytes: 1000,
        output_text_bytes: 1000,
        identity_cells: 100,
        identity_text_bytes: 1000,
    }
}
fn plan(expression: &str, name: &str, column: usize) -> DatasetPlan {
    DatasetPlan::new(
        schema(&["ID"]),
        schema(&["ID", "SEQ", "ALIAS"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
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
                assign(2, Expression::Column(1)),
            ],
            filter: Some(
                BoundPredicate::new(
                    crate::predicate_compiler::compile(
                        expression,
                        "row.filter",
                        Default::default(),
                    )
                    .unwrap(),
                    vec![Binding {
                        name: name.into(),
                        read: Read::Column(column),
                    }],
                )
                .unwrap(),
            ),
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap()
}

#[test]
fn grouped_window_filter_runs_once_after_complete_windows_without_renumbering() {
    for (expression, name, column, expected) in [
        (
            "SEQ > 1",
            "SEQ",
            1,
            vec![
                vec![Value::Int(2), Value::Int(2), Value::Int(2)],
                vec![Value::Int(3), Value::Int(3), Value::Int(3)],
            ],
        ),
        (
            "ALIAS > 1",
            "ALIAS",
            2,
            vec![
                vec![Value::Int(2), Value::Int(2), Value::Int(2)],
                vec![Value::Int(3), Value::Int(3), Value::Int(3)],
            ],
        ),
        (
            "ID > 1",
            "ID",
            0,
            vec![
                vec![Value::Int(2), Value::Int(1), Value::Int(1)],
                vec![Value::Int(3), Value::Int(2), Value::Int(2)],
            ],
        ),
    ] {
        let table = Table {
            schema: schema(&["ID"]),
            rows: [1, 1, 2, 3, 3]
                .into_iter()
                .map(|id| vec![Value::Int(id)])
                .collect(),
            reads: RefCell::default(),
        };
        assert_eq!(
            plan(expression, name, column)
                .execute(&table, limits())
                .unwrap()
                .dataset
                .rows(),
            expected
        );
        // Partitioning reads each source key, then each group borrows its first
        // ID exactly once. Both filter phases use completed output cells only.
        assert_eq!(
            *table.reads.borrow(),
            vec![
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (0, 0),
                (2, 0),
                (3, 0)
            ]
        );
    }
}

#[test]
fn grouped_postwindow_filters_use_contextual_join_and_named_lookup_reads_lazily() {
    use crate::dataset::{
        AggregateSource, Intermediate, IntermediateKey, IntermediateSource, JoinedRead, MatchKey,
        MatchRead, MatchValue, SourceSchemas,
    };
    use std::sync::Arc;
    for (named, keep) in [(false, true), (true, true), (false, false), (true, false)] {
        let input = schema(&["ID", "MATCH"]);
        let table = Table {
            schema: input.clone(),
            rows: [1, 2, 3]
                .into_iter()
                .map(|id| vec![Value::Int(id), Value::Int(id)])
                .collect(),
            reads: RefCell::default(),
        };
        let item = Intermediate {
            identifier: "MATCHED".into(),
            path: "intermediates[0]".into(),
            source: IntermediateSource::Primary {
                schema: input.clone(),
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
            range: None,
            columns: None,
            no_match: None,
            unique: Vec::new(),
            derivations: None,
            pending_schema: None,
        };
        let mut template = plan("SEQ > 1", "SEQ", 1).templates()[0].clone();
        template.filter = Some(
            BoundPredicate::new_contextual(
                crate::predicate_compiler::compile(
                    if keep {
                        "SEQ > 1 AND MATCH = 2"
                    } else {
                        "SEQ < 0 AND MATCH = 2"
                    },
                    "row.filter",
                    Default::default(),
                )
                .unwrap(),
                vec![
                    Binding {
                        name: "SEQ".into(),
                        read: Read::Column(1),
                    },
                    Binding {
                        name: "MATCH".into(),
                        read: if named {
                            Read::Intermediate {
                                index: 0,
                                column: 1,
                            }
                        } else {
                            Read::Joined(Box::new(JoinedRead {
                                record_keys: None,
                                source: AggregateSource::Primary,
                                column: 1,
                                keys: vec![MatchKey {
                                    source_column: 0,
                                    output_column: 0,
                                }],
                                name: "SRC".into(),
                                schema: Arc::new(input.clone()),
                            }))
                        },
                    },
                ],
                Vec::new(),
            )
            .unwrap(),
        );
        let plan = DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: input,
                secondary: vec![],
            },
            if named { vec![item] } else { vec![] },
            schema(&["ID", "SEQ", "ALIAS"]),
            vec![template],
            vec![],
            vec![0],
            vec![],
        )
        .unwrap();
        let expected = if keep {
            vec![vec![Value::Int(2), Value::Int(2), Value::Int(2)]]
        } else {
            vec![]
        };
        assert_eq!(
            plan.execute(&table, limits()).unwrap().dataset.rows(),
            expected
        );
        // The source value column is captured once. Contextual evaluation over
        // the completed window store must not revisit those original cells,
        // including when the window operand short-circuits every donor read.
        assert_eq!(
            table
                .reads
                .borrow()
                .iter()
                .filter(|(_, column)| *column == 1)
                .copied()
                .collect::<Vec<_>>(),
            vec![(0, 1), (1, 1), (2, 1)]
        );
    }
}
