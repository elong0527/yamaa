//! Key-bounded partitions (#3763): subject-keyed windows and aggregates
//! keep their whole-table results inside one partition, an input not grouped
//! by its keys is one partition, a cancelled run keeps nothing, and the
//! output divides into partitions as large as the largest driver partition
//! (D264).

use super::*;

/// An aggregate `expr` of `relation`, whose `V` reads its records' column
/// `v`.
fn aggregate(
    expr: &str,
    v: Option<usize>,
    relation: AggregateRelation,
    phase: AggregatePhase,
) -> Expression {
    let path = "columns.R.derivation.aggregate";
    Expression::Aggregate(Box::new(
        BoundAggregate::new(
            crate::expression::compile(
                expr,
                crate::expression::Context::Aggregate,
                &format!("{path}.expr"),
            )
            .unwrap(),
            relation,
            v.map(|column| AggregateBinding {
                name: "V".into(),
                column,
            })
            .into_iter()
            .collect(),
            None,
            phase,
            None,
            path,
        )
        .unwrap(),
    ))
}

/// A window of `kind` over each subject's rows, S and U, in the order of D.
fn by_subject(kind: WindowKind) -> Expression {
    Expression::Window(Window {
        kind,
        group_by: vec![0, 1],
        order_by: vec![OrderTerm {
            column: 2,
            descending: false,
            nulls_first: false,
        }],
        filter: None,
    })
}

/// Three subjects' visits: S and U name the subject, D the visit and V its
/// value; the records of a, then of b, then of c.
fn subjects() -> Rows {
    let integer = ColumnType::Int;
    Rows::new(
        schema(&[
            ("S", ColumnType::Str),
            ("U", ColumnType::Str),
            ("D", integer),
            ("V", integer),
        ]),
        [
            ("a", 2, 20),
            ("a", 1, 10),
            ("b", 1, 5),
            ("b", 3, 7),
            ("b", 2, 6),
            ("c", 1, 1),
        ]
        .iter()
        .map(|&(u, d, v)| vec![text("s"), text(u), int(d), int(v)])
        .collect(),
    )
}

/// One row per visit, keyed [S, U, D]: RN numbers each subject's visits in
/// the record template's window stage, PREV reads the value of the
/// subject's previous visit and TOT sums the subject's values, both in the
/// column phase. Each, a row window, a column window and an unqualified
/// aggregate, groups by S and U.
fn visits() -> (DatasetPlan, Rows) {
    let input = subjects();
    let integer = ColumnType::Int;
    let plan = DatasetPlan::new(
        input.schema.clone(),
        schema(&[
            ("S", ColumnType::Str),
            ("U", ColumnType::Str),
            ("D", integer),
            ("V", integer),
            ("RN", integer),
            ("PREV", integer),
            ("TOT", integer),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
                assign(2, Expression::Source(2)),
                assign(3, Expression::Source(3)),
                assign(4, by_subject(WindowKind::RowNumber)),
            ],
            filter: None,
        }],
        vec![
            assign(
                5,
                by_subject(WindowKind::RowValue {
                    column: 3,
                    offset: -1,
                }),
            ),
            assign(
                6,
                aggregate(
                    "SUM(V)",
                    Some(3),
                    AggregateRelation::Output {
                        group_by: vec![0, 1],
                    },
                    AggregatePhase::Derivation,
                ),
            ),
        ],
        vec![0, 1, 2],
        Vec::new(),
    );
    (plan, input)
}

/// The whole-table attempt of [`visits`], derived by hand: within each
/// subject, D orders the visits a1, a2, then b1, b2, b3, then c1; RN numbers
/// them in that order, PREV is the value of the visit before (none for the
/// first), and TOT is 10 + 20, 5 + 6 + 7 and 1. The rows keep their records'
/// order.
fn visits_attempt() -> ExecutionAttempt<Infallible> {
    let integer = ColumnType::Int;
    let row = |u: &str, d: i64, v: i64, rn: i64, prev: Option<i64>, tot: i64| {
        vec![
            text("s"),
            text(u),
            int(d),
            int(v),
            int(rn),
            prev.map_or(Value::Missing, int),
            int(tot),
        ]
    };
    ExecutionAttempt {
        result: Ok(Execution {
            partition_rows: None,
            text_date_orders: Vec::new(),
            unchosen_selections: Vec::new(),
            dataset: Dataset::from_rows(
                schema(&[
                    ("S", ColumnType::Str),
                    ("U", ColumnType::Str),
                    ("D", integer),
                    ("V", integer),
                    ("RN", integer),
                    ("PREV", integer),
                    ("TOT", integer),
                ]),
                vec![
                    row("a", 2, 20, 2, Some(10), 30),
                    row("a", 1, 10, 1, None, 30),
                    row("b", 1, 5, 1, None, 18),
                    row("b", 3, 7, 3, Some(6), 18),
                    row("b", 2, 6, 2, Some(5), 18),
                    row("c", 1, 1, 1, None, 1),
                ],
            ),
            verifications: Vec::new(),
        }),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 6,
            read: 6,
        }],
    }
}

/// Over key-bounded partitions a row window, a column window and an
/// unqualified aggregate grouped by S and U have their whole-table results.
/// Under two keys no partition splits a subject's records: of at most one
/// or three records, a, b and c are partitions of their own; of at most
/// four, a, then b with c. Under one key, S, every record is one group and
/// so one partition. Under three keys the windows, which do not group by D,
/// need the whole table: the run is refused at RN, the first of them, before
/// it reads a record, and with the fallback it runs over the whole input.
#[test]
fn subject_windows_and_aggregates_have_their_whole_table_results_over_key_bounded_partitions() {
    let (plan, input) = visits();
    let resolved = plan.resolve().unwrap();
    let expected = visits_attempt();
    let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(whole, expected, "the whole-table run");
    for (keys, rows) in [(1, 1), (1, 6), (2, 1), (2, 3), (2, 4), (2, 6), (3, 1)] {
        let fallback = keys == 3;
        assert_eq!(
            as_stated(divided_run(
                &resolved,
                &input,
                by_keys(keys, rows, fallback),
                &|| false
            )),
            expected,
            "{keys} keys, at most {rows} records, fallback {fallback}"
        );
    }
    input.reads.store(0, Ordering::Relaxed);
    let refused = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::PartitionGlobal {
            path: "columns.C4.derivation".into(),
        })),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    assert_eq!(
        (
            divided_run(&resolved, &input, by_keys(3, 1, false), &|| false),
            input.reads.load(Ordering::Relaxed),
        ),
        (refused, 0),
        "three keys without the fallback, and the cells it read"
    );
}

/// A key pairing the driver field `field` with the output column `column`
/// named `name`.
fn pairing(field: usize, column: usize, name: &str) -> IntermediateKey {
    IntermediateKey {
        source_column: field,
        path: format!("columns.TOT.derivation.aggregate.key[{column}]"),
        value: MatchValue::Variable(MatchRead::Output {
            column,
            identifier: name.into(),
        }),
    }
}

/// One row per subject of [`subjects`], keyed [S, U]: a template grouped by
/// S and U counts the subject's visits (N) and takes its largest value
/// (TOP), and TOT, in the column phase, sums the values of the driver
/// records its key pairs with the row's S and U. Two aggregates of a
/// grouped row template and a qualified aggregate of the driver input.
fn subject_rows() -> (DatasetPlan, Rows) {
    let input = subjects();
    let current = |expr: &str, v: Option<usize>| {
        aggregate(
            expr,
            v,
            AggregateRelation::CurrentGroup,
            AggregatePhase::RowConstruction,
        )
    };
    let integer = ColumnType::Int;
    let plan = DatasetPlan::new(
        input.schema.clone(),
        schema(&[
            ("S", ColumnType::Str),
            ("U", ColumnType::Str),
            ("N", integer),
            ("TOP", integer),
            ("TOT", integer),
        ]),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0, 1]),
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
                assign(2, current("COUNT(D.*)", None)),
                assign(3, current("MAX(V)", Some(3))),
            ],
            filter: None,
        }],
        vec![assign(
            4,
            aggregate(
                "SUM(V)",
                Some(3),
                AggregateRelation::Matched {
                    source: AggregateSource::Primary,
                    keys: vec![pairing(0, 0, "S"), pairing(1, 1, "U")],
                    group_by: Vec::new(),
                },
                AggregatePhase::Derivation,
            ),
        )],
        vec![0, 1],
        Vec::new(),
    );
    (plan, input)
}

/// Grouped rows' aggregates and a qualified aggregate of the driver input
/// keyed by S and U have their whole-table results over key-bounded
/// partitions: a has 2 visits, largest 20, summing 30; b 3, 7 and 18; c 1, 1
/// and 1, in the order of each subject's first record.
#[test]
fn grouped_and_qualified_subject_aggregates_have_their_whole_table_results_over_key_bounded_partitions(
) {
    let (plan, input) = subject_rows();
    let resolved = plan.resolve().unwrap();
    let integer = ColumnType::Int;
    let row = |u: &str, n: i64, top: i64, total: i64| {
        vec![text("s"), text(u), int(n), int(top), int(total)]
    };
    let expected = ExecutionAttempt {
        result: Ok(Execution {
            partition_rows: None,
            text_date_orders: Vec::new(),
            unchosen_selections: Vec::new(),
            dataset: Dataset::from_rows(
                schema(&[
                    ("S", ColumnType::Str),
                    ("U", ColumnType::Str),
                    ("N", integer),
                    ("TOP", integer),
                    ("TOT", integer),
                ]),
                vec![row("a", 2, 20, 30), row("b", 3, 7, 18), row("c", 1, 1, 1)],
            ),
            verifications: Vec::new(),
        }),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 3,
            read: 6,
        }],
    };
    let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(whole, expected, "the whole-table run");
    for (keys, rows) in [(1, 1), (2, 1), (2, 4), (2, 6)] {
        assert_eq!(
            as_stated(divided_run(
                &resolved,
                &input,
                by_keys(keys, rows, false),
                &|| false
            )),
            expected,
            "{keys} keys, at most {rows} records"
        );
    }
}

/// A driver input whose records are not grouped by the partition keys runs
/// as one partition (#3763, decided on the issue), and either way the run
/// fails as the whole-table run does, in base-record order. In the order a,
/// b, a the subject a recurs: the run divides nothing, so it polls only at
/// the whole-table run's checkpoints. In the order a, a, b, the subjects
/// are two partitions: Y's first partition fails, and the second partition,
/// which runs only what comes before Y's binding, polls at each of its six
/// derivation checkpoints up to Y's. Either way X's handler fires for 0.5
/// and 1.5 before Y fails at the earliest record whose B is not integral,
/// the second.
#[test]
fn an_input_not_grouped_by_its_keys_is_one_partition_and_fails_in_base_record_order() {
    let failed = |b: f64, u: &str, seq: i64| ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Conversion {
            path: "columns.Y".into(),
            output_row: 1,
            error: ConversionError {
                value: crate::json::Value::from(&Value::float(b)),
                ..not_integral()
            },
            identity: Some(RowIdentity {
                position: 1,
                values: vec![text("s"), text(u), int(seq)],
            }),
        })),
        handler_counts: vec![HandlerCount {
            spec_path: "columns.X.unconvertible".into(),
            handler: HandlerKind::Unconvertible,
            count: 2,
        }],
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 3,
            read: 3,
        }],
    };
    let cases = [
        (
            "a, b, a: not grouped",
            [("a", 1, 0.5, 1.0), ("b", 1, 1.0, 2.5), ("a", 2, 1.5, 3.5)],
            failed(2.5, "b", 1),
            0,
        ),
        (
            "a, a, b: grouped",
            [("a", 1, 0.5, 1.0), ("a", 2, 1.5, 3.5), ("b", 1, 1.0, 2.5)],
            failed(3.5, "a", 2),
            6,
        ),
    ];
    for (label, records, expected, between) in cases {
        let (plan, input) = conversions(&records);
        let resolved = plan.resolve().unwrap();
        let probe = Probe::new(None);
        let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| {
            probe.cancelled()
        });
        assert_eq!(whole, expected, "{label}: the whole-table run");
        let whole_polls = probe.polls.get();
        let probe = Probe::new(None);
        let keyed = divided_run(&resolved, &input, by_keys(2, 1, false), &|| {
            probe.cancelled()
        });
        assert_eq!(
            (keyed, probe.polls.get()),
            (expected, whole_polls + between),
            "{label}: key-bounded partitions and their polls"
        );
    }
}

/// Cancellation between key-bounded partitions keeps nothing any partition
/// built (#3763), as between consecutive ones. [`visits`] over its three
/// subjects polls its five derivation checkpoints in each, then its output
/// keys and its order. Cancelled at any of those polls, the run keeps
/// nothing, and its observer exits the phase it stopped in. The cut follows
/// admission, so a run cancelled at its admission checkpoint has read no
/// record.
#[test]
fn a_run_cancelled_between_key_bounded_partitions_keeps_nothing_and_names_its_phase() {
    let (plan, input) = visits();
    let resolved = plan.resolve().unwrap();
    let (derivation, tail) = polled(&resolved);
    assert_eq!((derivation, tail), (5, 2), "the checkpoints polled");
    let total = 1 + 3 * derivation + tail;
    let run = |probe: &Probe, observe: &mut dyn FnMut(ExecutionEvent)| {
        resolved.execute_divided(
            by_keys(2, 1, false),
            &input,
            &[],
            &mut NoFunctions::new(),
            Some(observe),
            &|| probe.cancelled(),
        )
    };
    let probe = Probe::new(None);
    assert_eq!(
        as_stated(run(&probe, &mut |_| {})),
        visits_attempt(),
        "uncancelled"
    );
    assert_eq!(probe.polls.get(), total, "polls of an uncancelled run");
    assert_cancelled_at_every_poll(&run, (3, derivation, total));
    input.reads.store(0, Ordering::Relaxed);
    run(&Probe::new(Some(1)), &mut |_| {});
    assert_eq!(
        input.reads.load(Ordering::Relaxed),
        0,
        "cells read when cancelled at admission"
    );
}

/// A partitioned run that divides every step records the most rows one
/// partition of its output holds, as many as its largest driver partition
/// holds and at least one (D264); the adapters encode, admit and publish the
/// output in partitions of that many rows. Over [`graded`]'s five records,
/// consecutive partitions of 5, of 1 each, of 0, 5 and 0, of 2, 0 and 3,
/// and of 4 and 1 hold at most 5, 1, 5, 3 and 4. Over [`visits`]' subjects
/// a, b and c, of 2, 3 and 1 records, key-bounded partitions by S and U of
/// at most one record are a, b and c, so at most 3, and of at most four, a
/// then b with c, so 4. A whole-table run, a run whose fallback runs the
/// windows that need the whole table admit their output whole and record
/// none.
#[test]
fn the_output_divides_into_partitions_as_large_as_the_largest_driver_partition() {
    let rows = |attempt: ExecutionAttempt<Infallible>| {
        attempt
            .result
            .ok()
            .and_then(|execution| execution.partition_rows)
            .map(std::num::NonZeroUsize::get)
    };
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let consecutive: [(&[usize], usize); 5] = [
        (&[5], 5),
        (&[1; 5], 1),
        (&[0, 5, 0], 5),
        (&[2, 0, 3], 3),
        (&[4, 1], 4),
    ];
    for (sizes, largest) in consecutive {
        assert_eq!(
            rows(partitioned(&resolved, &input, &[], sizes)),
            Some(largest),
            "consecutive partitions {sizes:?}"
        );
    }
    let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(rows(whole), None, "the whole-table run");
    let (plan, input) = visits();
    let resolved = plan.resolve().unwrap();
    for (records, largest) in [(1, 3), (4, 4)] {
        assert_eq!(
            rows(divided_run(
                &resolved,
                &input,
                by_keys(2, records, false),
                &|| false
            )),
            Some(largest),
            "key-bounded partitions of at most {records} records"
        );
    }
    assert_eq!(
        rows(divided_run(&resolved, &input, by_keys(3, 1, true), &|| {
            false
        })),
        None,
        "the fallback runs the windows over the whole input"
    );
}
