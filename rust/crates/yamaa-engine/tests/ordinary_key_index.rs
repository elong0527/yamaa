//! Independent donor order, missingness and native-effect controls for large keys.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, CurrentField, CurrentRead, DatasetPlan, Expression, Intermediate,
        IntermediateKey, IntermediateSource, Keep, MatchRead, MatchValue, OrderTerm,
        RelationPredicate, RowMode, RowTemplate, SecondarySource, SourceSchemas, SourceSelection,
    },
    predicate_compiler,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits, Resource};
use yamaa_engine::numeric_lifecycle::HandlerKind;
use yamaa_engine::table_grouping::GroupingError;
#[derive(Debug)]
struct Original(Box<u8>, Rc<()>);
type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
struct Table {
    name: &'static str,
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    trace: Trace,
    failure: RefCell<Option<(usize, usize, Original)>>,
}
impl TableAccess for Table {
    type Error = Original;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Original>> {
        self.trace.borrow_mut().push((self.name, row, column));
        if self
            .failure
            .borrow()
            .as_ref()
            .is_some_and(|(r, c, _)| (*r, *c) == (row, column))
        {
            return Err(CellError::Access(
                self.failure.borrow_mut().take().unwrap().2,
            ));
        }
        Ok((&self.rows[row][column]).into())
    }
}
fn schema(fields: &[(&str, T)]) -> TableSchema {
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
#[derive(Clone, Copy)]
enum Mode {
    Ordinary,
    Expression,
    Composite,
    Correlated,
}
fn fixture(keep: Keep, mode: Mode, selection: bool) -> (Table, Table, DatasetPlan, Trace) {
    let trace = Trace::default();
    let mut rows = (0..64)
        .map(|i| {
            vec![
                Value::Int(i),
                if i == 0 || i == 10 {
                    Value::Missing
                } else {
                    Value::Str(format!("K{i:03}"))
                },
                Value::Int(1),
            ]
        })
        .collect::<Vec<_>>();
    rows.push(vec![
        Value::Int(64),
        Value::Str("unmatched".into()),
        Value::Int(1),
    ]);
    let primary = Table {
        name: "BASE",
        schema: schema(&[("ID", T::Int), ("MATCH", T::Str), ("ACTIVE", T::Int)]),
        rows,
        trace: trace.clone(),
        failure: RefCell::default(),
    };
    let mut rows = Vec::new();
    for i in 0..64 {
        for donor in 0..4 {
            rows.push(vec![
                Value::Str(format!("K{i:03}")),
                Value::Int(1),
                Value::Int(i64::from(donor != 0)),
                Value::Int(i * 4 + donor),
            ]);
        }
    }
    let donors = Table {
        name: "DONOR",
        schema: schema(&[
            ("K", T::Str),
            ("ORDER", T::Int),
            ("ACTIVE", T::Int),
            ("VALUE", T::Int),
        ]),
        rows,
        trace: trace.clone(),
        failure: RefCell::default(),
    };
    let current = MatchRead::Record {
        column: 1,
        identifier: "BASE.MATCH".into(),
    };
    let mut keys = vec![IntermediateKey {
        source_column: 0,
        path: "intermediates[0].key.K".into(),
        value: if matches!(mode, Mode::Expression) {
            MatchValue::Source(current)
        } else {
            MatchValue::Variable(current)
        },
    }];
    if matches!(mode, Mode::Composite) {
        keys.push(IntermediateKey {
            source_column: 2,
            path: "intermediates[0].key.ACTIVE".into(),
            value: MatchValue::Variable(MatchRead::Record {
                column: 2,
                identifier: "BASE.ACTIVE".into(),
            }),
        });
    }
    let correlated = matches!(mode, Mode::Correlated);
    let predicate = predicate_compiler::compile(
        if correlated {
            "DONOR.ACTIVE = BASE.ACTIVE"
        } else {
            "DONOR.ACTIVE = 1"
        },
        "intermediates[0].filter",
        Default::default(),
    )
    .unwrap();
    let mut bindings = vec![Binding {
        name: "DONOR.ACTIVE".into(),
        read: Read::Source(2),
    }];
    let current = if correlated {
        bindings.push(Binding {
            name: "BASE.ACTIVE".into(),
            read: Read::Column(0),
        });
        vec![CurrentField {
            identifier: "BASE.ACTIVE".into(),
            read: CurrentRead::Record {
                source: 0,
                column: 2,
            },
        }]
    } else {
        vec![]
    };
    let item = Intermediate {
        identifier: "SELECTED".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Input(0),
        keys,
        filter: Some(
            RelationPredicate::new(BoundPredicate::new(predicate, bindings).unwrap(), current)
                .unwrap(),
        ),
        selection: selection.then_some(SourceSelection {
            order_by: vec![OrderTerm {
                column: 1,
                descending: false,
                nulls_first: false,
            }],
            keep,
        }),
        no_match: Some(Value::Missing),
        unique: vec![],
        derivations: None,
        pending_schema: None,
        range: None,
        columns: None,
    };
    let output = schema(&[("ID", T::Int), ("VALUE", T::Int)]);
    let assignments = vec![
        Assignment {
            column: 0,
            path: "columns.ID".into(),
            expression: Expression::Source(0),
        },
        Assignment {
            column: 1,
            path: "columns.VALUE".into(),
            expression: Expression::Intermediate {
                index: 0,
                column: 3,
            },
        },
    ];
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: primary.schema.clone(),
            secondary: vec![SecondarySource {
                name: "DONOR".into(),
                schema: donors.schema.clone(),
            }],
        },
        vec![item],
        output,
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments,
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    (primary, donors, plan, trace)
}
fn limits(index: bool) -> Limits {
    Limits {
        source_rows: 1024,
        output_rows: 1024,
        output_cells: 2048,
        key_cells: if index { 4096 } else { 65 },
        work_cells: 1_000_000,
        scalar_text_bytes: 1_000_000,
        output_text_bytes: 1_000_000,
        identity_cells: 1024,
        identity_text_bytes: 1_000_000,
    }
}
fn expected(keep: Keep) -> Vec<Vec<Value>> {
    (0..65)
        .map(|i| {
            vec![
                Value::Int(i),
                if i == 0 || i == 10 || i == 64 {
                    Value::Missing
                } else {
                    Value::Int(i * 4 + if keep == Keep::First { 1 } else { 3 })
                },
            ]
        })
        .collect()
}
#[test]
fn indexed_and_scan_truth_keep_filter_order_missingness_and_stable_first_last() {
    for keep in [Keep::First, Keep::Last] {
        for index in [true, false] {
            let (primary, donors, plan, trace) = fixture(keep, Mode::Ordinary, true);
            // Every first donor is filtered out before its key can be inspected.
            *donors.failure.borrow_mut() = Some((0, 0, Original(Box::new(17), Rc::new(()))));
            let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
            assert_eq!(attempted.result.unwrap().dataset.rows(), expected(keep));
            let events = trace.borrow();
            let first_current = events
                .iter()
                .position(|event| *event == ("BASE", 0, 1))
                .unwrap();
            assert_eq!(
                events[..first_current]
                    .iter()
                    .filter(|(name, _, col)| *name == "DONOR" && *col == 2)
                    .count(),
                256
            );
            let key_reads = events
                .iter()
                .filter(|(name, _, col)| *name == "DONOR" && *col == 0)
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(key_reads.len(), if index { 192 } else { 192 * 63 });
            if index {
                assert_eq!(
                    key_reads,
                    (0..256)
                        .filter(|r| r % 4 != 0)
                        .map(|r| ("DONOR", r, 0))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
#[test]
fn expression_composite_and_correlated_selectors_keep_the_existing_scan() {
    for mode in [Mode::Expression, Mode::Composite, Mode::Correlated] {
        for index in [true, false] {
            let (primary, donors, plan, trace) = fixture(Keep::First, mode, true);
            assert_eq!(
                plan.execute_observed_sources(&primary, &[&donors], limits(index))
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                expected(Keep::First)
            );
            let key_reads = trace
                .borrow()
                .iter()
                .filter(|(name, _, col)| *name == "DONOR" && *col == 0)
                .count();
            assert_eq!(
                key_reads,
                if matches!(mode, Mode::Correlated) {
                    256 * 63
                } else {
                    192 * 63
                }
            );
        }
    }
}
#[test]
fn first_donor_failure_retains_original_identity_and_the_exact_scan_prefix() {
    let mut traces = Vec::new();
    for index in [true, false] {
        let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
        let payload = Original(Box::new(29), Rc::new(()));
        let pointer = payload.0.as_ref() as *const u8;
        let marker = Rc::clone(&payload.1);
        *donors.failure.borrow_mut() = Some((130, 0, payload));
        let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
        let ExecutionError::SecondaryCell {
            source: 0,
            source_row: 130,
            error: CellError::Access(error),
            ..
        } = *attempted.result.unwrap_err()
        else {
            panic!("original donor failure")
        };
        assert_eq!(error.0.as_ref() as *const u8, pointer);
        assert!(Rc::ptr_eq(&error.1, &marker));
        traces.push(trace.borrow().clone());
    }
    assert_eq!(traces[0], traces[1]);
}
#[test]
fn multiple_match_refusal_reads_no_order_or_payload_and_builds_no_result() {
    for index in [true, false] {
        let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, false);
        let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
        assert!(matches!(
            *attempted.result.unwrap_err(),
            ExecutionError::MultipleMatches { match_count: 3, .. }
        ));
        assert!(!trace
            .borrow()
            .iter()
            .any(|(name, _, col)| *name == "DONOR" && matches!(col, 1 | 3)));
    }
}

#[test]
fn missing_donor_keys_and_selected_missing_payload_keep_distinct_handler_outcomes() {
    for keep in [Keep::First, Keep::Last] {
        let mut observations = Vec::new();
        for index in [true, false] {
            let (primary, mut donors, plan, _) = fixture(keep, Mode::Ordinary, true);
            for row in [5, 6, 7] {
                donors.rows[row][0] = Value::Missing;
            }
            // A present selected record with a missing field is still a match.
            for row in [9, 11] {
                donors.rows[row][3] = Value::Missing;
            }
            let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
            let mut truth = expected(keep);
            truth[1][1] = Value::Missing;
            truth[2][1] = Value::Missing;
            assert_eq!(attempted.result.unwrap().dataset.rows(), truth);
            assert_eq!(
                attempted
                    .handler_counts
                    .iter()
                    .find(|entry| entry.handler == HandlerKind::NoMatch)
                    .unwrap()
                    .count,
                4
            );
            assert_eq!(
                attempted
                    .handler_counts
                    .iter()
                    .find(|entry| entry.handler == HandlerKind::MultipleMatches)
                    .unwrap()
                    .count,
                61
            );
            observations.push(attempted.handler_counts);
        }
        assert_eq!(observations[0], observations[1]);
    }
}

#[test]
fn entire_source_filter_finishes_before_current_key_and_donor_key_effects() {
    let mut traces = Vec::new();
    for index in [true, false] {
        let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
        let original = Original(Box::new(41), Rc::new(()));
        let pointer = original.0.as_ref() as *const u8;
        let marker = Rc::clone(&original.1);
        *donors.failure.borrow_mut() = Some((130, 2, original));
        *primary.failure.borrow_mut() = Some((0, 1, Original(Box::new(43), Rc::new(()))));
        let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
        let ExecutionError::Predicate {
            source_row: 130,
            error,
        } = *attempted.result.unwrap_err()
        else {
            panic!("source-only filter keeps its original failure")
        };
        let yamaa_core::predicate::ErrorKind::Resolution {
            error: CellError::Access(original),
            ..
        } = error.kind
        else {
            panic!("original predicate operand failure")
        };
        assert_eq!(original.0.as_ref() as *const u8, pointer);
        assert!(Rc::ptr_eq(&original.1, &marker));
        assert!(primary.failure.borrow().is_some());
        assert!(!trace.borrow().iter().any(|(name, _, column)| {
            (*name == "BASE" && *column == 1) || (*name == "DONOR" && *column != 2)
        }));
        traces.push(trace.borrow().clone());
    }
    assert_eq!(traces[0], traces[1]);
}

#[test]
fn failing_current_key_reads_no_donor_key_order_or_payload() {
    let mut traces = Vec::new();
    for index in [true, false] {
        let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
        let original = Original(Box::new(47), Rc::new(()));
        let pointer = original.0.as_ref() as *const u8;
        let marker = Rc::clone(&original.1);
        *primary.failure.borrow_mut() = Some((1, 1, original));
        *donors.failure.borrow_mut() = Some((130, 0, Original(Box::new(53), Rc::new(()))));
        let attempted = plan.execute_observed_sources(&primary, &[&donors], limits(index));
        let ExecutionError::Cell {
            source_row: 1,
            error: CellError::Access(original),
            ..
        } = *attempted.result.unwrap_err()
        else {
            panic!("current key keeps its original failure")
        };
        assert_eq!(original.0.as_ref() as *const u8, pointer);
        assert!(Rc::ptr_eq(&original.1, &marker));
        assert!(donors.failure.borrow().is_some());
        assert!(!trace
            .borrow()
            .iter()
            .any(|(name, _, column)| *name == "DONOR" && *column != 2));
        traces.push(trace.borrow().clone());
    }
    assert_eq!(traces[0], traces[1]);
}

#[test]
fn existing_capacity_controls_keep_key_refusal_and_stop_work_or_text_before_payload() {
    for (key_cells, work_cells, scalar_text_bytes) in [
        (64, 1_000_000, 1_000_000),
        (4096, 0, 1_000_000),
        (4096, 1_000_000, 0),
    ] {
        let (primary, donors, plan, trace) = fixture(Keep::First, Mode::Ordinary, true);
        let bounded = Limits {
            key_cells,
            work_cells,
            scalar_text_bytes,
            ..limits(true)
        };
        let attempted = plan.execute_observed_sources(&primary, &[&donors], bounded);
        match *attempted.result.unwrap_err() {
            ExecutionError::OutputGrouping(GroupingError::KeyCellLimit { limit: 64 })
                if key_cells == 64 => {}
            ExecutionError::Limit {
                resource: Resource::WorkCells,
                limit: 0,
                ..
            } if work_cells == 0 => {}
            ExecutionError::Limit {
                resource: Resource::ScalarTextBytes,
                limit: 0,
                ..
            } if scalar_text_bytes == 0 => {}
            other => panic!("original capacity refusal: {other:?}"),
        }
        assert!(
            !trace
                .borrow()
                .iter()
                .any(|(name, _, column)| *name == "DONOR" && matches!(column, 1 | 3))
                || key_cells == 64
        );
        if key_cells == 64 {
            // Records complete before output-key admission. A low key budget
            // disables optional indexing without weakening that later refusal.
            assert_eq!(
                trace
                    .borrow()
                    .iter()
                    .filter(|(name, _, column)| *name == "DONOR" && *column == 0)
                    .count(),
                192 * 63
            );
        }
        if work_cells == 0 {
            assert!(trace.borrow().is_empty());
        }
    }
}
