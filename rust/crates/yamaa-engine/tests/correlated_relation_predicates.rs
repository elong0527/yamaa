//! Primary donors borrow the original table even while a secondary drives construction.
use std::cell::RefCell;
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    predicate_compiler,
};
use yamaa_core::{
    dataset::{
        Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
        IntermediateKey, IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm,
        RelationPredicate, RowMode, RowTemplate, SecondarySource, SourceSchemas, SourceSelection,
    },
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, Limits};
struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    failure: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.failure == Some((row, column)) {
            return Err(CellError::Access("held primary failure"));
        }
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

fn fixture(text: &str, failure: Option<(usize, usize)>) -> (Table, Table, DatasetPlan) {
    let current = Table {
        schema: schema(&[("ID", T::Int), ("CUT", T::Int), ("SEQ", T::Int)]),
        rows: vec![
            vec![Value::Int(1), Value::Int(2), Value::Int(1)],
            vec![Value::Int(1), Value::Int(4), Value::Int(2)],
            vec![Value::Int(3), Value::Int(0), Value::Int(3)],
        ],
        reads: RefCell::default(),
        failure,
    };
    let donor = Table {
        schema: schema(&[
            ("ID", T::Int),
            ("TIME", T::Int),
            ("A", T::Str),
            ("B", T::Str),
        ]),
        rows: vec![
            vec![
                Value::Int(1),
                Value::Int(1),
                Value::Str("one".into()),
                Value::Str("x".into()),
            ],
            vec![
                Value::Int(1),
                Value::Int(3),
                Value::Str("three".into()),
                Value::Missing,
            ],
            vec![
                Value::Int(2),
                Value::Int(9),
                Value::Str("trap".into()),
                Value::Str("trap".into()),
            ],
        ],
        reads: RefCell::default(),
        failure: Some((2, 1)),
    };
    let filter = RelationPredicate::new(
        BoundPredicate::new(
            predicate_compiler::compile(text, "intermediates[0].filter", Default::default())
                .unwrap(),
            vec![
                Binding {
                    name: "D.TIME".into(),
                    read: Read::Source(1),
                },
                Binding {
                    name: "SRC.CUT".into(),
                    read: Read::Column(0),
                },
            ],
        )
        .unwrap(),
        vec![CurrentField {
            identifier: "SRC.CUT".into(),
            read: CurrentRead::Record {
                source: 0,
                column: 1,
            },
        }],
    )
    .unwrap();
    let item = Intermediate {
        identifier: "P".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: Some(filter),
        selection: Some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 1,
                descending: false,
                nulls_first: false,
            }],
            keep: Keep::Last,
        }),
        no_match: Some(Value::Missing),
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,

        range: None,
        columns: None,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: current.schema.clone(),
            secondary: vec![SecondarySource {
                name: "D".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![item],
        schema(&[
            ("ID", T::Int),
            ("A", T::Str),
            ("B", T::Str),
            ("SEQ", T::Int),
        ]),
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
                assign(
                    2,
                    Expression::Intermediate {
                        index: 0,
                        column: 3,
                    },
                ),
                assign(3, Expression::Source(2)),
            ],
            filter: None,
        }],
        vec![],
        vec![0, 3],
        vec![],
    )
    .unwrap();
    (current, donor, plan)
}
fn limits() -> Limits {
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
    }
}

#[test]
fn equality_matches_are_filtered_per_row_and_all_fields_share_one_choice() {
    let (current, donor, plan) = fixture("D.TIME < SRC.CUT", None);
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [
            vec![
                Value::Int(1),
                Value::Str("one".into()),
                Value::Str("x".into()),
                Value::Int(1)
            ],
            vec![
                Value::Int(1),
                Value::Str("three".into()),
                Value::Missing,
                Value::Int(2)
            ],
            vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
        ]
    );
    assert_eq!(
        *current.reads.borrow(),
        [
            (0, 0),
            (0, 1),
            (0, 1),
            (0, 2),
            (1, 0),
            (1, 1),
            (1, 1),
            (1, 2),
            (2, 0),
            (2, 2)
        ]
    );
    assert_eq!(
        *donor.reads.borrow(),
        [
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (0, 2),
            (0, 3),
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (1, 1),
            (0, 1),
            (1, 2),
            (1, 3),
            (0, 0),
            (1, 0),
            (2, 0),
        ]
    );
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [
            (
                "read.C1.multiple_matches",
                yamaa_engine::numeric_lifecycle::HandlerKind::MultipleMatches,
                1
            ),
            (
                "read.C2.multiple_matches",
                yamaa_engine::numeric_lifecycle::HandlerKind::MultipleMatches,
                1
            ),
            (
                "read.C1.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                1
            ),
            (
                "read.C2.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                1
            ),
        ]
    );
}

#[test]
fn false_boolean_children_keep_written_current_failure_order() {
    let (current, donor, plan) = fixture("D.TIME < 0 AND SRC.CUT > 0", Some((0, 1)));
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert!(
        matches!(*result.result.unwrap_err(), yamaa_engine::dataset::ExecutionError::Cell {ref path,source_row:0,error:CellError::Access("held primary failure")} if path=="intermediates[0].filter")
    );
    assert_eq!(*current.reads.borrow(), [(0, 0), (0, 1)]);
    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1)]);
    assert!(result.handler_counts.is_empty());
}
#[test]
fn unmatched_rows_do_not_resolve_any_predicate_operand() {
    let (mut current, donor, plan) = fixture("D.TIME < SRC.CUT", Some((0, 1)));
    for (index, row) in current.rows.iter_mut().enumerate() {
        row[0] = Value::Int(3 + index as i64);
    }
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(1)],
            vec![Value::Int(4), Value::Missing, Value::Missing, Value::Int(2)],
            vec![Value::Int(5), Value::Missing, Value::Missing, Value::Int(3)],
        ]
    );
    assert_eq!(
        *current.reads.borrow(),
        [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
    );
    assert_eq!(
        *donor.reads.borrow(),
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 0),
            (1, 0),
            (2, 0)
        ]
    );
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [
            (
                "read.C1.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                3
            ),
            (
                "read.C2.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                3
            ),
        ]
    );
}
#[test]
fn a_current_access_failure_retains_its_owner_and_exact_successful_handler_prefix() {
    let (current, donor, plan) = fixture("D.TIME < SRC.CUT", Some((1, 1)));
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert!(
        matches!(*result.result.unwrap_err(),yamaa_engine::dataset::ExecutionError::Cell {ref path,source_row:1,error:CellError::Access("held primary failure")} if path=="intermediates[0].filter")
    );
    assert_eq!(
        *current.reads.borrow(),
        [(0, 0), (0, 1), (0, 1), (0, 2), (1, 0), (1, 1)]
    );
    assert_eq!(
        *donor.reads.borrow(),
        [
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (0, 2),
            (0, 3),
            (0, 0),
            (0, 1)
        ]
    );
    assert!(result.handler_counts.is_empty());
}

fn rebuilt(
    current: &Table,
    donor: &Table,
    old: &DatasetPlan,
    items: Vec<Intermediate>,
    assignments: Vec<Assignment>,
) -> DatasetPlan {
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: current.schema.clone(),
            secondary: vec![SecondarySource {
                name: "D".into(),
                schema: donor.schema.clone(),
            }],
        },
        items,
        old.output().clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments,
            filter: None,
        }],
        vec![],
        vec![0, 3],
        vec![],
    )
    .unwrap()
}
#[test]
fn completed_output_current_fields_keep_normal_missing_and_choice_semantics() {
    let (current, donor, old) = fixture("D.TIME < SRC.CUT", None);
    let mut items = old.intermediates().to_vec();
    items[0].filter = Some(
        RelationPredicate::new(
            items[0].filter.as_ref().unwrap().predicate().clone(),
            vec![CurrentField {
                identifier: "SRC.CUT".into(),
                read: CurrentRead::Output(3),
            }],
        )
        .unwrap(),
    );
    let mut assignments = old.templates()[0].assignments.clone();
    let seq = assignments.pop().unwrap();
    assignments.insert(1, seq);
    let plan = rebuilt(&current, &donor, &old, items, assignments);
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Missing, Value::Missing, Value::Int(1)],
            vec![
                Value::Int(1),
                Value::Str("one".into()),
                Value::Str("x".into()),
                Value::Int(2)
            ],
            vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
        ]
    );
    assert_eq!(
        *current.reads.borrow(),
        [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
    );
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [
            (
                "read.C1.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                2
            ),
            (
                "read.C2.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                2
            ),
        ]
    );
}
#[test]
fn current_kept_selections_share_the_attempt_cache_without_recovery_observations() {
    let (current, donor, old) = fixture("D.TIME < SRC.CUT", None);
    let mut first = old.intermediates()[0].clone();
    first.filter = None;
    let mut second = first.clone();
    second.identifier = "Q".into();
    second.path = "intermediates[1]".into();
    second.filter = Some(
        RelationPredicate::new(
            old.intermediates()[0]
                .filter
                .as_ref()
                .unwrap()
                .predicate()
                .clone(),
            vec![CurrentField {
                identifier: "SRC.CUT".into(),
                read: CurrentRead::Intermediate {
                    index: 0,
                    column: 1,
                },
            }],
        )
        .unwrap(),
    );
    let assignments = vec![
        assign(0, Expression::Source(0)),
        assign(
            1,
            Expression::Intermediate {
                index: 1,
                column: 2,
            },
        ),
        assign(
            2,
            Expression::Intermediate {
                index: 1,
                column: 3,
            },
        ),
        assign(3, Expression::Source(2)),
    ];
    let plan = rebuilt(&current, &donor, &old, vec![first, second], assignments);
    let result = plan.execute_observed_sources(&current, &[&donor], limits());
    assert_eq!(
        result.result.unwrap().dataset.rows(),
        [
            vec![
                Value::Int(1),
                Value::Str("one".into()),
                Value::Str("x".into()),
                Value::Int(1)
            ],
            vec![
                Value::Int(1),
                Value::Str("one".into()),
                Value::Str("x".into()),
                Value::Int(2)
            ],
            vec![Value::Int(3), Value::Missing, Value::Missing, Value::Int(3)],
        ]
    );
    assert_eq!(
        *current.reads.borrow(),
        [(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)]
    );
    assert_eq!(
        result
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.handler, h.count))
            .collect::<Vec<_>>(),
        [
            (
                "read.C1.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                1
            ),
            (
                "read.C2.no_match",
                yamaa_engine::numeric_lifecycle::HandlerKind::NoMatch,
                1
            ),
        ]
    );
}
