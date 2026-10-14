//! Independent relation, complete-context and opaque-read ordering controls.
use std::cell::RefCell;
use yamaa_core::{
    aggregate_compiler::compile_aggregate,
    diagnostic::{ConditionCode as C, ContextValue as V},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::dataset::*;

struct Table {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: RefCell<Vec<(usize, usize)>>,
    fail: Option<(usize, usize)>,
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
        if self.fail == Some((row, column)) {
            return Err(CellError::Access("late original"));
        }
        self.rows
            .get(row)
            .and_then(|row| row.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
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
fn table(rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: schema(&["ID", "V"]),
        rows,
        reads: Default::default(),
        fail: None,
    }
}
fn assignment(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns[{column}].derivation"),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 1000,
        key_cells: 1000,
        work_cells: 10000,
        scalar_text_bytes: 10000,
        output_text_bytes: 10000,
        identity_cells: 10000,
        identity_text_bytes: 10000,
    }
}
fn aggregate(
    text: &str,
    relation: AggregateRelation,
    bindings: &[(&str, usize)],
    phase: AggregatePhase,
    context: Option<AggregateRowContext>,
) -> Expression {
    Expression::Aggregate(Box::new(
        BoundAggregate::new(
            compile_aggregate(
                text,
                "columns.RESULT.derivation.aggregate.expr",
                Default::default(),
            )
            .unwrap(),
            relation,
            bindings
                .iter()
                .map(|(name, column)| AggregateBinding {
                    name: (*name).into(),
                    column: *column,
                })
                .collect(),
            None,
            phase,
            context,
            "columns.RESULT.derivation.aggregate",
        )
        .unwrap(),
    ))
}

#[test]
fn qualified_predicates_visit_only_matching_donor_records() {
    use yamaa_core::bound_expression::{Binding, BoundPredicate, Read};
    let primary = table(vec![vec![Value::Int(1), Value::Int(0)]]);
    let mut donor = table(vec![
        vec![Value::Int(1), Value::Int(5)],
        vec![Value::Int(2), Value::Int(8)],
    ]);
    donor.fail = Some((1, 1));
    let predicate = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile("S.V > 0", "aggregate.filter", Default::default())
            .unwrap(),
        vec![Binding {
            name: "S.V".into(),
            read: Read::Source(1),
        }],
    )
    .unwrap();
    let bound = BoundAggregate::new(
        compile_aggregate("SUM(S.V)", "aggregate", Default::default()).unwrap(),
        AggregateRelation::Matched {
            source: AggregateSource::Secondary(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "aggregate.key.ID".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 0,
                    identifier: "ID".into(),
                }),
            }],
            group_by: vec![],
        },
        vec![AggregateBinding {
            name: "S.V".into(),
            column: 1,
        }],
        Some(predicate),
        AggregatePhase::Derivation,
        None,
        "aggregate",
    )
    .unwrap();
    let plan = DatasetPlan::new_with_sources(
        primary.schema.clone(),
        vec![SecondarySource {
            name: "S".into(),
            schema: donor.schema.clone(),
        }],
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assignment(0, Expression::Source(0))],
            filter: None,
        }],
        vec![assignment(1, Expression::Aggregate(Box::new(bound)))],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute_observed_sources(&primary, &[&donor], limits())
            .result
            .unwrap()
            .dataset
            .rows(),
        &[vec![Value::Int(1), Value::Int(5)]]
    );
    assert_eq!(*donor.reads.borrow(), [(0, 0), (0, 1), (1, 0), (0, 1)]);
}

#[test]
fn correlated_sum_and_mean_keep_donor_order_through_cancellation_and_missing_values() {
    for (values, sum, mean) in [
        ([1e16, 1.0, -1e16], 0.0_f64, 0.0_f64),
        ([1e16, -1e16, 1.0], 1.0_f64, 1.0_f64 / 3.0),
    ] {
        let primary = table(vec![vec![Value::Int(1), Value::Int(0)]]);
        let donor = Table {
            schema: TableSchema::new(vec![
                Column {
                    name: "ID".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "V".into(),
                    kind: ColumnType::Float,
                },
            ])
            .unwrap(),
            rows: vec![
                vec![Value::Int(1), Value::float(values[0])],
                vec![Value::Int(2), Value::float(99.0)],
                vec![Value::Int(1), Value::float(values[1])],
                vec![Value::Int(1), Value::Missing],
                vec![Value::Int(1), Value::float(values[2])],
            ],
            reads: Default::default(),
            // This unrelated donor's value must never enter either reduction.
            fail: Some((1, 1)),
        };
        let relation = AggregateRelation::Matched {
            source: AggregateSource::Secondary(0),
            keys: vec![IntermediateKey {
                source_column: 0,
                path: "aggregate.key.ID".into(),
                value: MatchValue::Variable(MatchRead::Output {
                    column: 0,
                    identifier: "ID".into(),
                }),
            }],
            group_by: vec![],
        };
        let output = TableSchema::new(vec![
            Column {
                name: "ID".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "SUM".into(),
                kind: ColumnType::Float,
            },
            Column {
                name: "MEAN".into(),
                kind: ColumnType::Float,
            },
        ])
        .unwrap();
        let plan = DatasetPlan::new_with_sources(
            primary.schema.clone(),
            vec![SecondarySource {
                name: "S".into(),
                schema: donor.schema.clone(),
            }],
            output,
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![assignment(0, Expression::Source(0))],
                filter: None,
            }],
            ["SUM(S.V)", "MEAN(S.V)"]
                .iter()
                .enumerate()
                .map(|(index, expression)| {
                    assignment(
                        index + 1,
                        aggregate(
                            expression,
                            relation.clone(),
                            &[("S.V", 1)],
                            AggregatePhase::Derivation,
                            None,
                        ),
                    )
                })
                .collect(),
            vec![0],
            vec![],
        )
        .unwrap();
        let result = plan
            .execute_observed_sources(&primary, &[&donor], limits())
            .result
            .unwrap();
        assert_eq!(result.dataset.rows().len(), 1);
        assert_eq!(result.dataset.rows()[0][0], Value::Int(1));
        for (column, expected) in [(1, sum), (2, mean)] {
            let Value::Float(actual) = result.dataset.rows()[0][column] else {
                panic!("floating reduction result");
            };
            assert_eq!(actual.get().to_bits(), expected.to_bits());
        }
        let value_reads = donor
            .reads
            .borrow()
            .iter()
            .copied()
            .filter(|&(_, column)| column == 1)
            .collect::<Vec<_>>();
        assert_eq!(value_reads, [(0, 1), (2, 1), (3, 1), (4, 1)].repeat(2));
    }
}

#[test]
fn grouped_numeric_reducers_and_record_star_keep_order_and_empty_distinctions() {
    let source = table(vec![
        vec![Value::Int(1), Value::Int(3)],
        vec![Value::Int(1), Value::Int(5)],
        vec![Value::Int(2), Value::Missing],
    ]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&["ID", "RESULT", "N"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assignment(0, Expression::Source(0)),
                assignment(
                    1,
                    aggregate(
                        "SUM(D.V * 2) + D.ID",
                        AggregateRelation::CurrentGroup,
                        &[("D.V", 1), ("D.ID", 0)],
                        AggregatePhase::RowConstruction,
                        None,
                    ),
                ),
                assignment(
                    2,
                    aggregate(
                        "COUNT(D.*)",
                        AggregateRelation::CurrentGroup,
                        &[],
                        AggregatePhase::RowConstruction,
                        None,
                    ),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&source, limits());
    assert!(attempt.handler_counts.is_empty());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(17), Value::Int(2)],
            vec![Value::Int(2), Value::Missing, Value::Int(1)]
        ]
    );
}

#[test]
fn a_qualified_primary_relation_is_reduced_before_joining_to_each_record() {
    let source = table(vec![
        vec![Value::Int(1), Value::Int(3)],
        vec![Value::Int(1), Value::Int(5)],
    ]);
    let relation = AggregateRelation::Matched {
        source: AggregateSource::Primary,
        keys: vec![IntermediateKey {
            source_column: 0,
            path: "columns.RESULT.derivation.aggregate.key.ID".into(),
            value: MatchValue::Variable(MatchRead::Output {
                column: 0,
                identifier: "ID".into(),
            }),
        }],
        group_by: vec![],
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&["ID", "V", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assignment(0, Expression::Source(0)),
                assignment(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![assignment(
            2,
            aggregate(
                "SUM(D.V)",
                relation,
                &[("D.V", 1)],
                AggregatePhase::Derivation,
                None,
            ),
        )],
        vec![0, 1],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(3), Value::Int(8)],
            vec![Value::Int(1), Value::Int(5), Value::Int(8)]
        ]
    );
}

#[test]
fn output_group_reduction_reads_the_complete_constructed_relation() {
    let source = table(vec![
        vec![Value::Int(1), Value::Int(3)],
        vec![Value::Int(1), Value::Int(5)],
        vec![Value::Missing, Value::Missing],
    ]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&["ID", "V", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assignment(0, Expression::Source(0)),
                assignment(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![assignment(
            2,
            aggregate(
                "COUNT(V)",
                AggregateRelation::Output { group_by: vec![0] },
                &[("V", 1)],
                AggregatePhase::Derivation,
                None,
            ),
        )],
        vec![1],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&source, limits());
    // The missing output key causes its independent retained key failure only
    // after the reducer has seen the complete output relation.
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::KeyFailures(_)
    ));
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)]
    );
    let source = table(vec![
        vec![Value::Int(1), Value::Int(3)],
        vec![Value::Int(1), Value::Int(5)],
        vec![Value::Missing, Value::Int(7)],
    ]);
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        [
            vec![Value::Int(1), Value::Int(3), Value::Int(2)],
            vec![Value::Int(1), Value::Int(5), Value::Int(2)],
            vec![Value::Missing, Value::Int(7), Value::Int(1)]
        ]
    );
}

#[test]
fn only_is_eager_and_retains_complete_group_context_before_outer_coalesce() {
    let source = table(vec![
        vec![Value::Int(1), Value::Missing],
        vec![Value::Int(1), Value::Missing],
    ]);
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assignment(0, Expression::Source(0)),
                assignment(
                    1,
                    aggregate(
                        "COALESCE(1, ONLY(D.V))",
                        AggregateRelation::CurrentGroup,
                        &[("D.V", 1)],
                        AggregatePhase::RowConstruction,
                        Some(AggregateRowContext {
                            row: "summary".into(),
                            groups: vec![("D.ID".into(), 0)],
                        }),
                    ),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let attempt = plan.execute_observed(&source, limits());
    assert!(attempt.handler_counts.is_empty());
    let ExecutionError::SemanticDiagnostic {
        diagnostic,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("typed ONLY failure")
    };
    assert_eq!(diagnostic.code, C::AggregateRowMultipleRecords);
    assert_eq!(diagnostic.definition().requirement, Some("REQ-0501"));
    assert_eq!(
        diagnostic.spec_paths,
        ["columns.RESULT.derivation.aggregate"]
    );
    assert_eq!(identity, None);
    assert_eq!(diagnostic.source_span, None);
    assert_eq!(diagnostic.operand_route, None);
    assert_eq!(
        diagnostic.context,
        [
            (
                "expr".into(),
                V::Scalar(Value::Str("COALESCE(1, ONLY(D.V))".into()))
            ),
            ("reducer".into(), V::Scalar(Value::Str("ONLY".into()))),
            ("record_count".into(), V::Integer("2".into())),
            ("row".into(), V::Scalar(Value::Str("summary".into()))),
            (
                "group".into(),
                V::Object([("D.ID".into(), V::Scalar(Value::Int(1)))].into())
            )
        ]
        .into()
    );
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (1, 0), (0, 0), (0, 1), (1, 1), (0, 0)]
    );
}

#[test]
fn every_argument_is_collected_before_a_potential_earlier_fold_overflow() {
    let mut source = table(vec![
        vec![Value::Int(1), Value::Int(i64::MAX)],
        vec![Value::Int(1), Value::Int(1)],
        vec![Value::Int(1), Value::Int(0)],
    ]);
    source.fail = Some((2, 1));
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&["ID", "RESULT"]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![
                assignment(0, Expression::Source(0)),
                assignment(
                    1,
                    aggregate(
                        "SUM(D.V)",
                        AggregateRelation::CurrentGroup,
                        &[("D.V", 1)],
                        AggregatePhase::RowConstruction,
                        None,
                    ),
                ),
            ],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Cell {
            source_row: 2,
            error: CellError::Access("late original"),
            ..
        }
    ));
    assert_eq!(
        *source.reads.borrow(),
        [(0, 0), (1, 0), (2, 0), (0, 0), (0, 1), (1, 1), (2, 1)]
    );
}
