//! Destination-free authored key execution, effect boundaries and exact equality.
use std::{cell::RefCell, rc::Rc};
use yamaa_core::{
    bound_expression::{Binding, BoundPredicate, Read},
    dataset::{
        Assignment, DatasetPlan, Expression, Intermediate, IntermediateKey, MatchRead, MatchValue,
        PlanError, RowMode, RowTemplate, SecondarySource, SourceSchemas,
    },
    predicate::{self, Comparison, Node, Scalar},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    text_pad::StringPad,
    value::{ColumnType as T, Value},
};
use yamaa_engine::dataset::{DatasetExecution, ExecutionError, Limits, Resource};
#[derive(Debug)]
struct Failure(Box<u8>, Rc<()>);
type Trace = Rc<RefCell<Vec<(&'static str, usize, usize)>>>;
struct Table {
    name: &'static str,
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    fail: RefCell<Option<(usize, Failure)>>,
    trace: Trace,
}
impl TableAccess for Table {
    type Error = Failure;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Failure>> {
        self.trace.borrow_mut().push((self.name, row, column));
        if self
            .fail
            .borrow()
            .as_ref()
            .is_some_and(|(c, _)| *c == column)
        {
            return Err(CellError::Access(self.fail.borrow_mut().take().unwrap().1));
        }
        Ok(ValueRef::from(&self.rows[row][column]))
    }
}
fn schema(fields: &[(&str, T)]) -> TableSchema {
    TableSchema::new(
        fields
            .iter()
            .map(|(n, t)| Column {
                name: (*n).into(),
                kind: *t,
            })
            .collect(),
    )
    .unwrap()
}
fn tables(raw: Value, kind: T, donor: Value) -> (Table, Table, Trace) {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let primary = Table {
        name: "SRC",
        schema: schema(&[("ID", T::Int), ("RAW", kind)]),
        rows: vec![vec![Value::Int(1), raw]],
        fail: RefCell::default(),
        trace: trace.clone(),
    };
    let secondary = Table {
        name: "TAB",
        schema: schema(&[
            ("K", kind),
            ("VAL", T::Str),
            ("K2", T::Str),
            ("FILTER", T::Int),
        ]),
        rows: vec![vec![
            donor,
            Value::Str("7".into()),
            Value::Str("unused".into()),
            Value::Int(1),
        ]],
        fail: RefCell::default(),
        trace: trace.clone(),
    };
    (primary, secondary, trace)
}
fn read() -> MatchRead {
    MatchRead::Record {
        column: 1,
        identifier: "SRC.RAW".into(),
    }
}
fn key(value: MatchValue) -> IntermediateKey {
    IntermediateKey {
        source_column: 0,
        path: "intermediates[0].key.K".into(),
        value,
    }
}
fn item(keys: Vec<IntermediateKey>) -> Intermediate {
    Intermediate {
        identifier: "LOOK".into(),
        path: "intermediates[0]".into(),
        source: yamaa_core::dataset::IntermediateSource::Input(0),
        keys,
        filter: None,
        selection: None,
        no_match: Some(Value::Missing),

        range: None,
        columns: None,
    }
}
fn pad(width: &str) -> MatchValue {
    MatchValue::StrPad {
        read: read(),
        operation: Some(
            StringPad::new("SRC.RAW", "intermediates[0].key.K.str_pad", width).unwrap(),
        ),
    }
}
fn limits() -> Limits {
    Limits {
        source_rows: 100,
        output_rows: 100,
        output_cells: 100,
        key_cells: 100,
        work_cells: 10000,
        scalar_text_bytes: 10000,
        output_text_bytes: 10000,
        identity_cells: 100,
        identity_text_bytes: 10000,
    }
}
fn plan(src: &Table, tab: &Table, item: Intermediate, target: T) -> Result<DatasetPlan, PlanError> {
    let assign = |column, expression| Assignment {
        column,
        path: format!("columns.C{column}.derivation.source"),
        expression,
    };
    DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: src.schema.clone(),
            secondary: vec![SecondarySource {
                name: "TAB".into(),
                schema: tab.schema.clone(),
            }],
        },
        vec![item],
        schema(&[("ID", T::Int), ("V", target), ("V2", target)]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(
                1,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
            assign(
                2,
                Expression::Intermediate {
                    index: 0,
                    column: 1,
                },
            ),
        ],
        vec![0],
        vec![],
    )
}
fn filter() -> BoundPredicate {
    BoundPredicate::new(
        predicate::Plan::new(
            vec![Node::Compare {
                operator: Comparison::Greater,
                left: Scalar::Identifier("TAB.FILTER".into()),
                right: Scalar::Literal(Value::Int(0)),
            }],
            0,
            "intermediates[0].filter".into(),
            "TAB.FILTER > 0".into(),
            Default::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "TAB.FILTER".into(),
            read: Read::Source(3),
        }],
    )
    .unwrap()
}
#[test]
fn padding_without_dummy_output_preserves_unicode_width_and_single_cached_selection() {
    let (src, tab, trace) = tables(
        Value::Str("\u{e9}\u{1f980}".into()),
        T::Str,
        Value::Str("  \u{e9}\u{1f980}".into()),
    );
    let mut selected = item(vec![key(pad("4"))]);
    selected.filter = Some(filter().into());
    let plan = plan(&src, &tab, selected, T::Int).unwrap();
    for _ in 0..2 {
        trace.borrow_mut().clear();
        let observed = plan.execute_observed_sources(&src, &[&tab], limits());
        assert_eq!(
            observed.result.unwrap().dataset.rows(),
            [vec![Value::Int(1), Value::Int(7), Value::Int(7)]]
        );
        assert!(observed.handler_counts.is_empty());
        assert_eq!(
            *trace.borrow(),
            [
                ("SRC", 0, 0),
                ("SRC", 0, 1),
                ("TAB", 0, 3),
                ("TAB", 0, 0),
                ("TAB", 0, 1),
                ("TAB", 0, 1)
            ]
        );
    }
}
#[test]
fn expression_dependency_opaque_failure_precedes_filter_and_preserves_payload_identity() {
    let (src, tab, trace) = tables(Value::Missing, T::Str, Value::Str("".into()));
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *src.fail.borrow_mut() = Some((1, Failure(payload, marker.clone())));
    *tab.fail.borrow_mut() = Some((3, Failure(Box::new(8), Rc::new(()))));
    let mut selected = item(vec![key(pad("4"))]);
    selected.filter = Some(filter().into());
    let observed = plan(&src, &tab, selected, T::Str)
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
    let ExecutionError::Cell {
        path,
        source_row,
        error: CellError::Access(Failure(retained, identity)),
    } = *observed.result.unwrap_err()
    else {
        panic!("original current dependency failure")
    };
    assert_eq!(&*retained as *const u8, address);
    assert!(Rc::ptr_eq(&identity, &marker));
    assert_eq!(path, "intermediates[0]");
    assert_eq!(source_row, 0);
    assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
    assert!(tab.fail.borrow().is_some());
    assert!(observed.handler_counts.is_empty());
}
#[test]
fn ordinary_selector_keeps_filter_before_raw_dependency_and_its_opaque_payload() {
    let (src, tab, trace) = tables(Value::Missing, T::Str, Value::Str("".into()));
    *src.fail.borrow_mut() = Some((1, Failure(Box::new(9), Rc::new(()))));
    let payload = Box::new(8);
    let address = &*payload as *const u8;
    let marker = Rc::new(());
    *tab.fail.borrow_mut() = Some((3, Failure(payload, marker.clone())));
    let mut selected = item(vec![key(MatchValue::Variable(read()))]);
    selected.filter = Some(filter().into());
    let observed = plan(&src, &tab, selected, T::Str)
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
    let ExecutionError::Predicate { error, .. } = *observed.result.unwrap_err() else {
        panic!("original predicate port failure")
    };
    let predicate::ErrorKind::Resolution {
        error: CellError::Access(Failure(retained, identity)),
        ..
    } = error.kind
    else {
        panic!("original predicate payload")
    };
    assert_eq!(&*retained as *const u8, address);
    assert!(Rc::ptr_eq(&identity, &marker));
    assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("TAB", 0, 3)]);
    assert!(src.fail.borrow().is_some());
    assert!(observed.handler_counts.is_empty());
}
#[test]
fn missing_first_key_does_not_skip_later_padding_refusal_or_current_dependency() {
    let (src, tab, trace) = tables(Value::Str("present".into()), T::Str, Value::Missing);
    let mut later = key(pad(&"9".repeat(81)));
    later.source_column = 2;
    later.path = "intermediates[0].key.K2".into();
    let selected = item(vec![key(MatchValue::Literal(Value::Missing)), later]);
    let observed = plan(&src, &tab, selected, T::Str)
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
    assert!(matches!(
        *observed.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            required: None,
            ..
        }
    ));
    assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
    assert!(observed.handler_counts.is_empty());
}
#[test]
fn key_missingness_never_matches_and_counts_each_reading_handler() {
    let (src, tab, _) = tables(Value::Missing, T::Str, Value::Missing);
    let observed = plan(&src, &tab, item(vec![key(pad("4"))]), T::Str)
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
    assert_eq!(
        observed.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Missing, Value::Missing]]
    );
    assert_eq!(
        observed
            .handler_counts
            .iter()
            .map(|h| (h.spec_path.as_str(), h.count))
            .collect::<Vec<_>>(),
        [
            ("columns.C1.derivation.source.no_match", 1),
            ("columns.C2.derivation.source.no_match", 1)
        ]
    );
}
#[test]
fn whole_work_and_cumulative_retention_refuse_before_later_effects_and_fresh_retry_succeeds() {
    let (src, tab, trace) = tables(Value::Str("ab".into()), T::Str, Value::Str("ab".into()));
    let plan = plan(
        &src,
        &tab,
        item(vec![key(MatchValue::Source(read()))]),
        T::Str,
    )
    .unwrap();
    let observed = plan.execute_observed_sources(
        &src,
        &[&tab],
        Limits {
            work_cells: 0,
            ..limits()
        },
    );
    assert!(matches!(
        *observed.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert!(trace.borrow().is_empty());
    let observed = plan.execute_observed_sources(
        &src,
        &[&tab],
        Limits {
            scalar_text_bytes: 3,
            ..limits()
        },
    );
    assert!(matches!(
        *observed.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(*trace.borrow(), [("SRC", 0, 0), ("SRC", 0, 1)]);
    trace.borrow_mut().clear();
    assert!(plan
        .execute_observed_sources(&src, &[&tab], limits())
        .result
        .is_ok());
    assert!(trace
        .borrow()
        .iter()
        .any(|(name, _, col)| *name == "TAB" && *col == 1));
}
#[test]
fn typed_numeric_matching_preserves_precision_and_defers_the_sole_output_conversion() {
    for (current, donor, matched) in [
        (7, 7., true),
        (9007199254740993, 9007199254740992., false),
        (i64::MIN, -9223372036854775808., true),
        (i64::MAX, 9223372036854775808., false),
    ] {
        let (src, mut tab, _) = tables(Value::Int(current), T::Int, Value::float(donor));
        tab.schema = schema(&[
            ("K", T::Float),
            ("VAL", T::Str),
            ("K2", T::Str),
            ("FILTER", T::Int),
        ]);
        let observed = plan(
            &src,
            &tab,
            item(vec![key(MatchValue::Source(read()))]),
            T::Int,
        )
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
        assert_eq!(
            observed.result.unwrap().dataset.rows(),
            [vec![
                Value::Int(1),
                if matched {
                    Value::Int(7)
                } else {
                    Value::Missing
                },
                if matched {
                    Value::Int(7)
                } else {
                    Value::Missing
                }
            ]]
        );
    }
    let (src, mut tab, trace) = tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
    tab.rows.push(tab.rows[0].clone());
    tab.rows[0][1] = Value::Str("bad".into());
    let observed = plan(
        &src,
        &tab,
        item(vec![key(MatchValue::Source(read()))]),
        T::Int,
    )
    .unwrap()
    .execute_observed_sources(&src, &[&tab], limits());
    assert!(matches!(
        *observed.result.unwrap_err(),
        ExecutionError::MultipleMatches { match_count: 2, .. }
    ));
    assert!(trace
        .borrow()
        .iter()
        .all(|(name, _, col)| *name != "TAB" || *col == 0));
}
#[test]
fn malformed_bound_coordinates_conflicting_names_and_unadmitted_width_never_reach_ports() {
    let (src, tab, trace) = tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
    for value in [
        MatchValue::Source(MatchRead::Record {
            column: 99,
            identifier: "SRC.RAW".into(),
        }),
        MatchValue::StrPad {
            read: read(),
            operation: None,
        },
    ] {
        assert!(matches!(
            plan(&src, &tab, item(vec![key(value)]), T::Str),
            Err(PlanError::InvalidIntermediate)
        ));
    }
    let first = key(MatchValue::Source(read()));
    let mut second = key(MatchValue::Source(MatchRead::Record {
        column: 0,
        identifier: "SRC.RAW".into(),
    }));
    second.source_column = 2;
    assert!(matches!(
        plan(&src, &tab, item(vec![first, second]), T::Str),
        Err(PlanError::InvalidIntermediate)
    ));
    assert!(trace.borrow().is_empty());
}
#[test]
fn donor_filter_completes_before_padding_evaluation_or_its_output_quota() {
    let (src, mut tab, trace) = tables(
        Value::Str("present".into()),
        T::Str,
        Value::Str("present".into()),
    );
    tab.rows[0][3] = Value::Str("bad".into());
    let mut selected = item(vec![key(pad("1000000"))]);
    selected.filter = Some(filter().into());
    let observed = plan(&src, &tab, selected, T::Str)
        .unwrap()
        .execute_observed_sources(
            &src,
            &[&tab],
            Limits {
                scalar_text_bytes: 100,
                ..limits()
            },
        );
    assert!(matches!(
        *observed.result.unwrap_err(),
        ExecutionError::Predicate { .. }
    ));
    assert_eq!(
        *trace.borrow(),
        [("SRC", 0, 0), ("SRC", 0, 1), ("TAB", 0, 3)]
    );
    assert!(observed.handler_counts.is_empty());
}
#[test]
fn donor_filter_keeps_only_true_in_written_order_before_match_count_and_value() {
    let (src, mut tab, trace) = tables(Value::Str("x".into()), T::Str, Value::Str("x".into()));
    tab.rows[0][3] = Value::Missing;
    let mut excluded = tab.rows[0].clone();
    excluded[3] = Value::Int(-1);
    let mut kept = tab.rows[0].clone();
    kept[1] = Value::Str("8".into());
    kept[3] = Value::Int(1);
    tab.rows.extend([excluded, kept]);
    let mut selected = item(vec![key(MatchValue::Source(read()))]);
    selected.filter = Some(filter().into());
    let observed = plan(&src, &tab, selected, T::Int)
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
    assert_eq!(
        observed.result.unwrap().dataset.rows(),
        [vec![Value::Int(1), Value::Int(8), Value::Int(8)]]
    );
    assert_eq!(
        *trace.borrow(),
        [
            ("SRC", 0, 0),
            ("SRC", 0, 1),
            ("TAB", 0, 3),
            ("TAB", 1, 3),
            ("TAB", 2, 3),
            ("TAB", 2, 0),
            ("TAB", 2, 1),
            ("TAB", 2, 1)
        ]
    );
}
#[test]
fn typed_calendar_and_datetime_keys_compare_calendar_values_including_collected_precision() {
    use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
    let year = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
    let day = Date::new(2025, 1, 1, DatePrecision::Day).unwrap();
    let midnight = DateTime::new(day, 0, 0, 0, DateTimePrecision::Day).unwrap();
    let second = DateTime::new(day, 0, 0, 0, DateTimePrecision::Second).unwrap();
    for (current, donor, kind) in [
        (Value::Date(year), Value::Date(day), T::Date),
        (
            Value::DateTime(midnight),
            Value::DateTime(second),
            T::DateTime,
        ),
        (Value::float(-0.), Value::float(0.), T::Float),
    ] {
        let (src, tab, _) = tables(current, kind, donor);
        let observed = plan(
            &src,
            &tab,
            item(vec![key(MatchValue::Source(read()))]),
            T::Str,
        )
        .unwrap()
        .execute_observed_sources(&src, &[&tab], limits());
        assert_eq!(
            observed.result.unwrap().dataset.rows(),
            [vec![
                Value::Int(1),
                Value::Str("7".into()),
                Value::Str("7".into())
            ]]
        );
    }
}
