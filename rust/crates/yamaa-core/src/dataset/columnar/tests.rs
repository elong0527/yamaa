//! Hand-built plans on both executors: the attempt of the executor over the
//! resolved plan equals today's, including a failure's identity and the
//! handler counts before it. The equivalence gate compares every benchmark
//! and fixture; these keep the executor exercised in ordinary CI.
use crate::bound_expression::{Binding, BoundPredicate, Read};
use crate::dataset::{
    Assignment, CaseBranch, Check, ColumnVerifications, DatasetPlan, Expression, Intermediate,
    IntermediateKey, IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm, RowMode,
    RowTemplate, SecondarySource, SourceSchemas, SourceSelection, Verification, Window, WindowKind,
};
use crate::function_invocation::NoFunctions;
use crate::table::{CellError, Column, TableAccess, TableSchema, ValueRef};
use crate::value::{ColumnType, Value};
use std::convert::Infallible;

struct Rows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Rows {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

fn schema(columns: &[(&str, ColumnType)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}
fn assign(column: usize, expression: Expression, path: &str) -> Assignment {
    Assignment {
        column,
        expression,
        path: path.into(),
    }
}
fn predicate(text: &str, path: &str, bindings: Vec<(&str, Read)>) -> BoundPredicate {
    BoundPredicate::new(
        crate::expression::compile(text, crate::expression::Context::Predicate, path).unwrap(),
        bindings
            .into_iter()
            .map(|(name, read)| Binding {
                name: name.into(),
                read,
            })
            .collect(),
    )
    .unwrap()
}
fn ints(values: &[&[Option<i64>]]) -> Vec<Vec<Value>> {
    values
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| value.map_or(Value::Missing, Value::Int))
                .collect()
        })
        .collect()
}

/// Run both executors over the same tables and require equal attempts.
fn assert_same(plan: &DatasetPlan, primary: &Rows, secondary: &[&Rows]) {
    let ports: Vec<&dyn TableAccess<Error = Infallible>> = secondary
        .iter()
        .map(|table| *table as &dyn TableAccess<Error = Infallible>)
        .collect();
    let mut events = Vec::new();
    let today = plan.execute(
        primary,
        &ports,
        &mut NoFunctions::new(),
        Some(&mut |event| events.push(event)),
    );
    let resolved = plan.resolve().unwrap();
    let mut resolved_events = Vec::new();
    let candidate = resolved.execute(
        primary,
        &ports,
        &mut NoFunctions::new(),
        Some(&mut |event| resolved_events.push(event)),
    );
    assert_eq!(format!("{candidate:#?}"), format!("{today:#?}"));
    assert_eq!(resolved_events, events);
}

/// Source reads, a case, a filter and a column-phase window with a check.
#[test]
fn a_record_template_with_a_filter_and_a_window_matches() {
    let source = Rows {
        schema: schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        rows: ints(&[
            &[Some(3), Some(30)],
            &[Some(1), None],
            &[Some(2), Some(-5)],
            &[Some(4), Some(30)],
        ]),
    };
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("V", ColumnType::Int),
        ("BAND", ColumnType::Str),
        ("RANK", ColumnType::Int),
    ]);
    let band = Expression::Case(vec![
        CaseBranch {
            when: Some(predicate(
                "V >= 10",
                "columns.BAND.derivation.case[0].when",
                vec![("V", Read::Column(1))],
            )),
            assignment: Box::new(assign(
                2,
                Expression::Literal(Value::Str("HIGH".into())),
                "columns.BAND.derivation.case[0].then",
            )),
        },
        CaseBranch {
            when: None,
            assignment: Box::new(assign(
                2,
                Expression::Literal(Value::Str("LOW".into())),
                "columns.BAND.derivation.case[1].otherwise",
            )),
        },
    ]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "columns.ID.derivation"),
                assign(1, Expression::Source(1), "columns.V.derivation"),
                assign(2, band, "columns.BAND.derivation"),
            ],
            filter: Some(predicate(
                "ID > 1",
                "row.filter",
                vec![("ID", Read::Source(0))],
            )),
        }],
        vec![assign(
            3,
            Expression::Window(Window {
                kind: WindowKind::Competition,
                group_by: vec![2],
                order_by: vec![OrderTerm {
                    column: 1,
                    descending: true,
                    nulls_first: false,
                }],
                filter: None,
            }),
            "columns.RANK.derivation",
        )],
        vec![0],
        vec![Verification {
            path: "verifications[0]".into(),
            check: Check::Unique(vec![2, 3]),
        }],
    )
    .unwrap()
    .with_column_verifications(vec![ColumnVerifications {
        column: 3,
        checks: vec![Verification {
            path: "columns.RANK.verifications[0]".into(),
            check: Check::NotMissing,
        }],
    }])
    .unwrap()
    .with_output_order(vec![OrderTerm {
        column: 0,
        descending: false,
        nulls_first: false,
    }])
    .unwrap();
    assert_same(&plan, &source, &[]);
}

/// A key-grain plan: keys from every record, then collections per key.
#[test]
fn a_key_grain_plan_with_collections_matches() {
    let source = Rows {
        schema: schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        rows: ints(&[
            &[Some(2), Some(7)],
            &[Some(1), Some(5)],
            &[Some(2), Some(7)],
            &[None, Some(1)],
        ]),
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Str)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0), "columns.ID.derivation")],
            filter: None,
        }],
        vec![assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "V".into(),
                filter: None,
                selection: None,
            },
            "columns.V.derivation",
        )],
        vec![0],
        vec![],
    )
    .unwrap();
    // The missing key fails the output keys after V converts to text.
    assert_same(&plan, &source, &[]);
}

/// A conversion failure in the middle of the row stage replays row by row
/// and reports the first failing row with the keys completed so far.
#[test]
fn a_row_stage_failure_keeps_its_first_row_and_identity() {
    let source = Rows {
        schema: schema(&[("ID", ColumnType::Int), ("T", ColumnType::Str)]),
        rows: vec![
            vec![Value::Int(1), Value::Str("4".into())],
            vec![Value::Int(2), Value::Str("x".into())],
            vec![Value::Int(3), Value::Str("y".into())],
        ],
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[("N", ColumnType::Int), ("ID", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(1, Expression::Source(0), "columns.ID.derivation"),
                assign(0, Expression::Source(1), "columns.N.derivation"),
            ],
            filter: None,
        }],
        vec![],
        vec![1],
        vec![],
    )
    .unwrap();
    assert_same(&plan, &source, &[]);
}

/// A named intermediate with a selection counts `multiple_matches` and
/// `no_match` per read.
#[test]
fn a_named_intermediate_with_a_selection_matches() {
    let source = Rows {
        schema: schema(&[("ID", ColumnType::Int)]),
        rows: ints(&[&[Some(1)], &[Some(2)], &[Some(3)]]),
    };
    let donor = Rows {
        schema: schema(&[("ID", ColumnType::Int), ("VAL", ColumnType::Int)]),
        rows: ints(&[
            &[Some(1), Some(10)],
            &[Some(2), Some(20)],
            &[Some(1), Some(5)],
        ]),
    };
    let item = Intermediate {
        identifier: "L".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "intermediates[0].key[0]".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        filter: None,
        selection: Some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 1,
                descending: false,
                nulls_first: false,
            }],
            keep: Keep::First,
        }),
        range: None,
        columns: None,
        no_match: Some(Value::Int(-1)),
        require_match: false,
        unique: vec![],
        derivations: None,
        pending_schema: None,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: source.schema.clone(),
            secondary: vec![SecondarySource {
                name: "LKP".into(),
                schema: donor.schema.clone(),
            }],
        },
        vec![item],
        schema(&[("ID", ColumnType::Int), ("VAL", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0), "columns.ID.derivation"),
                assign(
                    1,
                    Expression::Intermediate {
                        index: 0,
                        column: 1,
                    },
                    "columns.VAL.derivation",
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_same(&plan, &source, &[&donor]);
}

/// Grouped rows reduce their records and keep the group's first member.
#[test]
fn grouped_reductions_match() {
    let source = Rows {
        schema: schema(&[("G", ColumnType::Int), ("V", ColumnType::Int)]),
        rows: ints(&[&[Some(1), Some(2)], &[Some(2), None], &[Some(1), Some(3)]]),
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[
            ("G", ColumnType::Int),
            ("TOTAL", ColumnType::Int),
            ("N", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assign(0, Expression::Source(0), "columns.G.derivation"),
                assign(
                    1,
                    Expression::Reduce {
                        identifier: Some("V".into()),
                        column: 1,
                        reducer: crate::reduction::NumericReducer::Sum,
                        text: "SUM(V)".into(),
                    },
                    "columns.TOTAL.derivation",
                ),
                assign(
                    2,
                    Expression::Count {
                        column: Some(1),
                        text: "COUNT(V)".into(),
                    },
                    "columns.N.derivation",
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_same(&plan, &source, &[]);
}

/// A buffer the executor cannot reserve fails the run as an allocation
/// failure, as today's executor's reservations do, instead of aborting the
/// process: a frame, a filled vector, and the hash partition.
#[test]
fn a_refused_reservation_is_an_allocation_failure() {
    use crate::dataset::execution::ExecutionError;
    let mut frame = super::frame::Frame::new(3);
    let refused = super::reserved::<(), Infallible>(frame.try_reserve(usize::MAX, 0));
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    let refused = super::filled::<u64, Infallible>(usize::MAX, 0);
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    assert!(super::group::partition(usize::MAX, 1, |_, _| ValueRef::Missing).is_err());
    // A reservation that fits changes nothing a reader sees.
    super::reserved::<(), Infallible>(frame.try_reserve(4, 4)).unwrap();
    assert_eq!(frame.rows(), 0);
}
