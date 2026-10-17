//! Exact named-selection values, original coordinates and attempt-local read order.
use super::*;
use crate::{table::Column, value::ColumnType};
use std::cell::RefCell;
use std::rc::Rc;

type Trace = Rc<RefCell<Vec<&'static str>>>;
type Original = std::convert::Infallible;
struct Table {
    schema: TableSchema,
    values: Vec<Value>,
    names: Vec<&'static str>,
    trace: Trace,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        1
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        assert_eq!(row, 0);
        self.trace.borrow_mut().push(self.names[column]);
        Ok(ValueRef::from(&self.values[column]))
    }
}
fn schema(fields: &[(&str, ColumnType)]) -> TableSchema {
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
struct Fixture {
    primary: Table,
    donor: Table,
    plan: DatasetPlan,
    trace: Trace,
}
impl Fixture {
    fn new() -> Self {
        Self::with_value(None)
    }
    fn with_value(value: Option<MatchValue>) -> Self {
        let trace = Trace::default();
        let primary = Table {
            schema: schema(&[("K", ColumnType::Str)]),
            values: vec![Value::Str("x".into())],
            names: vec!["BASE.K"],
            trace: trace.clone(),
        };
        let donor = Table {
            schema: schema(&[("K", ColumnType::Str), ("V", ColumnType::Int)]),
            values: vec![Value::Str("x".into()), Value::Int(7)],
            names: vec!["DONOR.K", "DONOR.V"],
            trace: trace.clone(),
        };
        let plan = DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: primary.schema.clone(),
                secondary: vec![SecondarySource {
                    name: "DONOR".into(),
                    schema: donor.schema.clone(),
                }],
            },
            vec![Intermediate {
                identifier: "MATCH".into(),
                path: "intermediates[0]".into(),
                source: IntermediateSource::Input(0),
                keys: vec![IntermediateKey {
                    source_column: 0,
                    path: "intermediates[0].key".into(),
                    value: value.unwrap_or_else(|| {
                        MatchValue::Variable(MatchRead::Record {
                            column: 0,
                            identifier: "BASE.K".into(),
                        })
                    }),
                }],
                filter: None,
                selection: None,
                no_match: None,
                unique: vec![],
                derivations: None,
                pending_schema: None,
                range: None,
                columns: None,
            }],
            schema(&[("ID", ColumnType::Int), ("VALUE", ColumnType::Int)]),
            vec![RowTemplate {
                mode: RowMode::Records,
                filter: None,
                assignments: vec![
                    Assignment {
                        column: 0,
                        path: "columns.ID".into(),
                        expression: Expression::Literal(Value::Int(1)),
                    },
                    Assignment {
                        column: 1,
                        path: "columns.VALUE".into(),
                        expression: Expression::Intermediate {
                            index: 0,
                            column: 1,
                        },
                    },
                ],
            }],
            vec![],
            vec![0],
            vec![],
        )
        .unwrap();
        Self {
            primary,
            donor,
            plan,
            trace,
        }
    }
    fn read(&self, run: &mut Run) -> Result<Value, Box<ExecutionError<Original>>> {
        let candidate = Candidate {
            template: Some(0),
            selection_identity: SelectionIdentity::Candidate(0),
            members: vec![0],
            values: vec![Value::Int(1), Value::Missing],
            completed: vec![true, false],
        };
        read(
            0,
            1,
            &self.plan.templates()[0].assignments[1],
            &candidate,
            &self.plan,
            0,
            &self.primary,
            &mut EvaluationState {
                functions: &mut crate::function_invocation::NoFunctions::new(),
                handlers: &mut HandlerCounter::default(),
                secondary: &[&self.donor],
                primary: &self.primary,
                self_rows: &[],
                intermediates: run,
                output_rows: &[],
                aggregate_row: None,
                output_offset: 0,
                windows: &mut windows::Cache::default(),
            },
        )
    }
}
#[test]
fn named_selection_retains_value_original_coordinate_and_read_order() {
    let fixture = Fixture::new();
    let mut run = Run::default();
    assert_eq!(fixture.read(&mut run).unwrap(), Value::Int(7));
    assert_eq!(*fixture.trace.borrow(), ["BASE.K", "DONOR.K", "DONOR.V"]);
    assert_eq!(run.choices.len(), 1);
    assert!(matches!(
        run.choices[&(SelectionIdentity::Candidate(0), 0)],
        Choice::Record {
            row: 0,
            multiple: false
        }
    ));
}

#[test]
fn repeated_selection_uses_the_original_cached_choice_and_reads_only_the_payload() {
    let fixture = Fixture::new();
    let mut run = Run::default();
    assert_eq!(fixture.read(&mut run).unwrap(), Value::Int(7));
    fixture.trace.borrow_mut().clear();
    assert_eq!(fixture.read(&mut run).unwrap(), Value::Int(7));
    assert_eq!(*fixture.trace.borrow(), ["DONOR.V"]);
    assert_eq!(run.choices.len(), 1);
}

struct Tuples {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}
impl TableAccess for Tuples {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}

fn item_with_key_columns(fixture: &Fixture, columns: &[usize]) -> Intermediate {
    let mut item = fixture.plan.intermediates()[0].clone();
    item.keys = columns
        .iter()
        .map(|&source_column| IntermediateKey {
            source_column,
            path: "intermediates[0].key".into(),
            value: MatchValue::Variable(MatchRead::Record {
                column: 0,
                identifier: "BASE.K".into(),
            }),
        })
        .collect();
    item
}

#[test]
fn key_index_keeps_every_original_coordinate_in_source_order() {
    let keys = Tuples {
        schema: schema(&[("K", ColumnType::Str)]),
        rows: (0..256)
            .map(|row| vec![Value::Str(std::format!("K{:03}", row % 64))])
            .collect(),
    };
    let fixture = Fixture::new();
    let eligible = (0..256).collect::<Vec<_>>();
    let index = key_index::KeyIndex::build(&keys, &fixture.plan.intermediates()[0], &eligible)
        .unwrap()
        .unwrap();
    assert_eq!(index.bucket_count(), 64);
    assert_eq!(
        index
            .matching::<Original>(&[(0, Value::Str("K015".into()))])
            .unwrap(),
        [15, 79, 143, 207]
    );
}

#[test]
fn key_index_matches_int_keys_and_their_float_spelling() {
    let keys = Tuples {
        schema: schema(&[("K", ColumnType::Int)]),
        rows: (0..256)
            .map(|row| vec![Value::Int((row % 64) as i64)])
            .collect(),
    };
    let fixture = Fixture::new();
    let eligible = (0..256).collect::<Vec<_>>();
    let index = key_index::KeyIndex::build(&keys, &fixture.plan.intermediates()[0], &eligible)
        .unwrap()
        .unwrap();
    assert_eq!(index.bucket_count(), 64);
    let expected: &[usize] = &[15, 79, 143, 207];
    assert_eq!(
        index.matching::<Original>(&[(0, Value::Int(15))]).unwrap(),
        expected
    );
    assert_eq!(
        index
            .matching::<Original>(&[(0, Value::float(15.0))])
            .unwrap(),
        expected
    );
    assert!(index
        .matching::<Original>(&[(0, Value::float(15.5))])
        .unwrap()
        .is_empty());
    assert!(index
        .matching::<Original>(&[(0, Value::Str("15".into()))])
        .unwrap()
        .is_empty());
}

#[test]
fn key_index_matches_composite_keys_in_donor_order_and_skips_missing() {
    let keys = Tuples {
        schema: schema(&[("S", ColumnType::Str), ("N", ColumnType::Int)]),
        rows: vec![
            vec![Value::Str("a".into()), Value::Int(1)],
            vec![Value::Str("a".into()), Value::Int(2)],
            vec![Value::Missing, Value::Int(1)],
            vec![Value::Str("a".into()), Value::Int(1)],
            vec![Value::Str("b".into()), Value::Int(1)],
            vec![Value::Str("a".into()), Value::Missing],
        ],
    };
    let fixture = Fixture::new();
    let item = item_with_key_columns(&fixture, &[0, 1]);
    let eligible = (0..6).collect::<Vec<_>>();
    let index = key_index::KeyIndex::build(&keys, &item, &eligible)
        .unwrap()
        .unwrap();
    assert_eq!(index.bucket_count(), 3);
    assert_eq!(
        index
            .matching::<Original>(&[(0, Value::Str("a".into())), (1, Value::Int(1))])
            .unwrap(),
        [0, 3]
    );
    // Lookup key order follows the resolved keys, not declaration order.
    assert_eq!(
        index
            .matching::<Original>(&[(1, Value::float(1.0)), (0, Value::Str("a".into()))])
            .unwrap(),
        [0, 3]
    );
    assert!(index
        .matching::<Original>(&[(0, Value::Str("a".into())), (1, Value::Int(3))])
        .unwrap()
        .is_empty());
}

#[test]
fn key_index_unifies_zero_signs_and_ignores_temporal_precision() {
    use crate::temporal::{Date, DatePrecision};
    let keys = Tuples {
        schema: schema(&[("F", ColumnType::Float), ("D", ColumnType::Date)]),
        rows: vec![
            vec![
                Value::float(-0.0),
                Value::Date(Date::new(2020, 1, 2, DatePrecision::Year).unwrap()),
            ],
            vec![
                Value::float(0.5),
                Value::Date(Date::new(2020, 1, 2, DatePrecision::Day).unwrap()),
            ],
        ],
    };
    let fixture = Fixture::new();
    let item = item_with_key_columns(&fixture, &[0, 1]);
    let eligible = (0..2).collect::<Vec<_>>();
    let index = key_index::KeyIndex::build(&keys, &item, &eligible)
        .unwrap()
        .unwrap();
    assert_eq!(
        index
            .matching::<Original>(&[
                (0, Value::Int(0)),
                (
                    1,
                    Value::Date(Date::new(2020, 1, 2, DatePrecision::Day).unwrap())
                )
            ])
            .unwrap(),
        [0]
    );
}

#[test]
fn key_index_declines_a_donor_cell_whose_type_fits_no_column_type() {
    let keys = Tuples {
        schema: schema(&[("K", ColumnType::Str)]),
        rows: vec![vec![Value::Int(7)]],
    };
    let fixture = Fixture::new();
    let eligible = vec![0];
    assert!(
        key_index::KeyIndex::build(&keys, &fixture.plan.intermediates()[0], &eligible)
            .unwrap()
            .is_none()
    );
}

#[test]
fn scalar_key_snapshots_read_current_once_and_apply_only_the_authored_missing_handler() {
    use crate::{
        dataset::MatchExpression,
        text_case::{Case, StringCase},
    };
    let path = "intermediates[0].key.K.str_case";
    let mut fixture = Fixture::with_value(Some(MatchValue::Expression(Box::new(
        MatchExpression::new(
            Assignment {
                column: 2,
                path: path.into(),
                expression: Expression::StrCase {
                    source: Box::new(Assignment {
                        column: 2,
                        path: std::format!("{path}.source"),
                        expression: Expression::Source(0),
                    }),
                    operation: StringCase::new(
                        "BASE.K".into(),
                        path.into(),
                        Case::Lower,
                        Some(Value::Str("x".into())),
                    )
                    .unwrap(),
                },
            },
            vec![MatchRead::Record {
                column: 0,
                identifier: "BASE.K".into(),
            }],
            schema(&[("BASE.K", ColumnType::Str)]),
            Some(ColumnType::Str.into()),
        )
        .unwrap(),
    ))));
    let candidate = Candidate {
        template: Some(0),
        selection_identity: SelectionIdentity::Candidate(0),
        members: vec![0],
        values: vec![Value::Int(1), Value::Missing],
        completed: vec![true, false],
    };
    for missing in [false, true] {
        fixture.primary.values[0] = if missing {
            Value::Missing
        } else {
            Value::Str("x".into())
        };
        fixture.trace.borrow_mut().clear();
        let mut handlers = HandlerCounter::default();
        handlers.register(&std::format!("{path}.missing"), HandlerKind::Missing);
        let mut state = EvaluationState {
            functions: &mut crate::function_invocation::NoFunctions::new(),
            handlers: &mut handlers,
            secondary: &[&fixture.donor],
            primary: &fixture.primary,
            self_rows: &[],
            intermediates: &mut Run::default(),
            output_rows: &[],
            aggregate_row: None,
            output_offset: 0,
            windows: &mut windows::Cache::default(),
        };
        let item = &fixture.plan.intermediates()[0];
        let current = match_values::resolve(
            item,
            &candidate,
            &fixture.plan,
            0,
            &fixture.primary,
            &mut state,
        )
        .unwrap();
        assert_eq!(*fixture.trace.borrow(), ["BASE.K"]);
        let result = match_values::evaluate(
            item,
            &current,
            &candidate,
            &fixture.plan,
            0,
            &fixture.primary,
            &mut state,
        );
        assert_eq!(result.unwrap(), [(0, Value::Str("x".into()))]);
        assert_eq!(
            *fixture.trace.borrow(),
            ["BASE.K"],
            "scalar evaluation uses the resolved snapshot only"
        );
        assert_eq!(handlers.snapshot()[0].count, u64::from(missing));
    }
}
