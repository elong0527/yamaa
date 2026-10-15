//! Independent ordering, admission, failure and committed benchmark observations.
use crate::support::table_schema as schema;
#[allow(unused_imports)]
use yamaa_core::{
    reduction::NumericReducer,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision},
    value::{ColumnType, Value},
};

#[allow(unused_imports)]
use yamaa_core::dataset::{
    Assignment, Check, DatasetPlan, ExecutionError, Expression, Limits, PlanError, RowMode,
    RowTemplate, Verification,
};

use super::dataset::*;
use super::dataset_filters::*;
use super::dataset_sources::*;
use super::dataset_windows::*;
#[allow(unused_imports)]
use yamaa_core::bound_expression::{Binding, BindingError, BoundPredicate as Filter, Read};
#[allow(unused_imports)]
use yamaa_core::predicate::{self, Comparison, Node, Scalar};

/// Bind a replacement independently of assignment execution order.
fn conversion_handler(column: usize, value: Value) -> yamaa_core::dataset::ConversionHandler {
    yamaa_core::dataset::ConversionHandler {
        assignment_path: format!("columns.C{column}.derivation"),
        handler: yamaa_core::numeric_lifecycle::LiteralHandler {
            spec_path: format!("columns.C{column}.derivation.unconvertible"),
            value,
        },
    }
}

/// Replacement finishes before dependents read it, with zero counts and fresh ledgers.
#[test]
fn completed_conversion_handlers_preserve_counts_values_and_failures() {
    let source = table(
        &[("ID", ColumnType::Int), ("X", ColumnType::Str)],
        vec![
            vec![Value::Int(1), Value::Str("bad".into())],
            vec![Value::Int(2), Value::Str("4".into())],
        ],
    );
    let plan = record_plan(
        &source,
        schema(&[
            ("ID", ColumnType::Int),
            ("X", ColumnType::Int),
            ("COPY", ColumnType::Int),
        ]),
        vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Source(1)),
            assign(2, Expression::Column(1)),
        ],
        vec![],
    );
    let declarations = vec![
        conversion_handler(2, Value::Int(99)),
        conversion_handler(1, Value::Int(7)),
        conversion_handler(0, Value::Int(0)),
    ];
    let bound = plan
        .clone()
        .with_conversion_handlers(declarations.clone())
        .unwrap();
    for _ in 0..2 {
        let attempt = bound.execute_observed(&source, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            vec![
                vec![Value::Int(1), Value::Int(7), Value::Int(7)],
                vec![Value::Int(2), Value::Int(4), Value::Int(4)]
            ]
        );
        assert_eq!(
            attempt
                .handler_counts
                .iter()
                .map(|entry| entry.count)
                .collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
        assert_eq!(
            attempt
                .handler_counts
                .iter()
                .map(|entry| &entry.spec_path)
                .collect::<Vec<_>>(),
            declarations
                .iter()
                .map(|entry| &entry.handler.spec_path)
                .collect::<Vec<_>>()
        );
    }
    let empty = table(&[("ID", ColumnType::Int), ("X", ColumnType::Str)], vec![]);
    let attempt = bound.execute_observed(&empty, limits());
    assert!(attempt.result.unwrap().dataset.rows().is_empty());
    assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
    let missing = plan
        .clone()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Missing)])
        .unwrap()
        .execute_observed(&source, limits());
    assert_eq!(
        missing.result.unwrap().dataset.rows()[0],
        vec![Value::Int(1), Value::Missing, Value::Missing]
    );
    let bad = plan
        .clone()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Str("still bad".into()))])
        .unwrap()
        .execute_observed(&source, limits());
    assert!(
        matches!(*bad.result.unwrap_err(),ExecutionError::Conversion { path, identity:Some(_), .. } if path=="columns.C1.derivation.unconvertible")
    );
    assert_eq!(bad.handler_counts[0].count, 1);
    let mut broken = source;
    broken.fail = Some((0, 1));
    let attempt = bound.execute_observed(&broken, limits());
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Cell { .. }
    ));
    assert!(attempt.handler_counts.iter().all(|entry| entry.count == 0));
}

/// Unused literals consume no scalar budget; invalid handler bindings read no cells.
#[test]
fn conversion_handler_admission_and_reached_resource_accounting() {
    let source = table(&[("ID", ColumnType::Int)], vec![vec![Value::Int(1)]]);
    let plan = record_plan(
        &source,
        schema(&[("ID", ColumnType::Int)]),
        vec![assign(0, Expression::Source(0))],
        vec![],
    );
    for declarations in [
        vec![conversion_handler(9, Value::Int(0))],
        vec![conversion_handler(0, Value::Int(0)); 2],
    ] {
        assert_eq!(
            plan.clone().with_conversion_handlers(declarations),
            Err(PlanError::InvalidConversionHandler)
        );
    }
    assert!(source.reads.borrow().is_empty());
    let mut tiny = limits();
    tiny.scalar_text_bytes = 0;
    let bound = plan
        .with_conversion_handlers(vec![conversion_handler(
            0,
            Value::Str("large unused replacement".into()),
        )])
        .unwrap();
    assert!(bound.execute(&source, tiny).is_ok());
    let bad = record_plan(
        &source,
        schema(&[("ID", ColumnType::Int)]),
        vec![assign(
            0,
            Expression::Literal(Value::Float(
                yamaa_core::value::FiniteFloat::new(1.5).unwrap(),
            )),
        )],
        vec![],
    )
    .with_conversion_handlers(vec![conversion_handler(0, Value::Str("0".into()))])
    .unwrap()
    .execute_observed(&source, tiny);
    assert!(matches!(
        *bad.result.unwrap_err(),
        ExecutionError::Limit {
            resource: yamaa_core::dataset::Resource::ScalarTextBytes,
            ..
        }
    ));
    assert_eq!(bad.handler_counts[0].count, 0);
}

/// Raw record/group match values remain independent of already converted output keys.
#[test]
fn row_lookups_match_driver_fields_and_count_records_before_donor_reads() {
    use yamaa_core::dataset::{RowLookup, RowMatchKey, SecondarySource, SourceSchemas};
    let driver = table(
        &[("ID", ColumnType::Str)],
        vec![vec![Value::Str("02".into())]],
    );
    let mut right = table(
        &[("ID", ColumnType::Str), ("V", ColumnType::Str)],
        vec![
            vec![Value::Str("02".into()), Value::Str("7".into())],
            vec![Value::Str("2".into()), Value::Str("99".into())],
        ],
    );
    let binding = RowLookup {
        source: 0,
        column: 1,
        keys: vec![RowMatchKey {
            source_column: 0,
            driver_column: 0,
        }],
    };
    let make = |mode, binding| {
        DatasetPlan::new_with_intermediates(
            SourceSchemas {
                primary: driver.schema.clone(),
                secondary: vec![SecondarySource {
                    name: "OTHER".into(),
                    schema: right.schema.clone(),
                }],
            },
            vec![],
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![RowTemplate {
                mode,
                filter: None,
                assignments: vec![
                    assign(0, Expression::Literal(Value::Int(99))),
                    assign(1, Expression::RowLookup(binding)),
                ],
            }],
            vec![],
            vec![0],
            vec![],
        )
    };
    for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
        let plan = make(mode, binding.clone()).unwrap();
        for _ in 0..2 {
            driver.reads.borrow_mut().clear();
            right.reads.borrow_mut().clear();
            assert_eq!(
                plan.execute_observed_sources(&driver, &[&right], limits())
                    .result
                    .unwrap()
                    .dataset
                    .rows(),
                &[vec![Value::Int(99), Value::Int(7)]]
            );
            assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (0, 1)]);
        }
    }
    assert_eq!(
        make(RowMode::Keys, binding.clone()),
        Err(PlanError::InvalidKeyMode)
    );
    let mut invalid = binding.clone();
    invalid.keys[0].driver_column = 9;
    assert_eq!(
        make(RowMode::Records, invalid),
        Err(PlanError::InvalidLookup)
    );
    let plan = make(RowMode::Records, binding).unwrap();
    right.rows.push(right.rows[0].clone());
    right.fail = Some((0, 1));
    right.reads.borrow_mut().clear();
    let failed = plan.execute_observed_sources(&driver, &[&right], limits());
    match *failed.result.unwrap_err() {
        ExecutionError::MultipleMatches {
            match_count,
            matched_key,
            identity,
            ..
        } => {
            assert_eq!(match_count, 2);
            assert_eq!(matched_key, vec![("ID".into(), Value::Str("02".into()))]);
            assert_eq!(identity.unwrap().values, vec![Value::Int(99)]);
        }
        error => panic!("unexpected {error:?}"),
    }
    assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
    right.reads.borrow_mut().clear();
    let missing = table(&[("ID", ColumnType::Str)], vec![vec![Value::Missing]]);
    assert_eq!(
        plan.execute_observed_sources(&missing, &[&right], limits())
            .result
            .unwrap()
            .dataset
            .rows(),
        &[vec![Value::Int(99), Value::Missing]]
    );
    assert!(right.reads.borrow().is_empty());
    let mut failing = driver;
    failing.fail = Some((0, 0));
    assert!(matches!(
        *plan
            .execute_observed_sources(&failing, &[&right], limits())
            .result
            .unwrap_err(),
        ExecutionError::Cell { source_row: 0, .. }
    ));
    let mut tiny = limits();
    tiny.work_cells = 0;
    failing.reads.borrow_mut().clear();
    assert!(matches!(
        *plan
            .execute_observed_sources(&failing, &[&right], tiny)
            .result
            .unwrap_err(),
        ExecutionError::Limit { .. }
    ));
    assert!(failing.reads.borrow().is_empty());
}

/// Placement preserves filtering order without changing raw-key matching or duplicate priority.
#[test]
fn row_lookup_assignment_phase_controls_reads_for_discarded_candidates() {
    use yamaa_core::dataset::{RowLookup, RowMatchKey, SecondarySource};
    for mode in [RowMode::Records, RowMode::Groups(vec![0])] {
        let driver = table(
            &[("ID", ColumnType::Int)],
            vec![vec![Value::Int(1)], vec![Value::Int(2)]],
        );
        let mut right = table(
            &[("ID", ColumnType::Int), ("V", ColumnType::Int)],
            vec![
                vec![Value::Int(1), Value::Int(10)],
                vec![Value::Int(1), Value::Int(10)],
                vec![Value::Int(2), Value::Int(20)],
            ],
        );
        // A donor access error must not mask ambiguity or reach a discarded row.
        right.fail = Some((0, 1));
        let lookup = assign(
            1,
            Expression::RowLookup(RowLookup {
                source: 0,
                column: 1,
                keys: vec![RowMatchKey {
                    source_column: 0,
                    driver_column: 0,
                }],
            }),
        );
        for in_template in [true, false] {
            let mut assignments = vec![assign(0, Expression::Source(0))];
            let mut columns = vec![];
            if in_template {
                assignments.push(lookup.clone());
            } else {
                columns.push(lookup.clone());
            }
            let plan = DatasetPlan::new_with_sources(
                driver.schema.clone(),
                vec![SecondarySource {
                    name: "OTHER".into(),
                    schema: right.schema.clone(),
                }],
                schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
                vec![RowTemplate {
                    mode: mode.clone(),
                    assignments,
                    filter: Some(filter(
                        Node::Compare {
                            operator: Comparison::Greater,
                            left: Scalar::Identifier("id".into()),
                            right: Scalar::Literal(Value::Int(1)),
                        },
                        vec![binding("id", Read::Column(0))],
                    )),
                }],
                columns,
                vec![0],
                vec![],
            )
            .unwrap();
            for _ in 0..2 {
                right.reads.borrow_mut().clear();
                let attempt = plan.execute_observed_sources(&driver, &[&right], limits());
                if in_template {
                    assert!(matches!(
                        *attempt.result.unwrap_err(),
                        ExecutionError::MultipleMatches { match_count: 2, .. }
                    ));
                    assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0)]);
                } else {
                    assert_eq!(
                        attempt.result.unwrap().dataset.rows(),
                        &[vec![Value::Int(2), Value::Int(20)]]
                    );
                    // Only the retained candidate scans; its donor is read once.
                    assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (2, 1)]);
                }
            }
            if !in_template {
                // The later phase still rejects ambiguity in a retained candidate.
                right.rows.push(vec![Value::Int(2), Value::Int(20)]);
                right.fail = Some((2, 1));
                right.reads.borrow_mut().clear();
                let attempt = plan.execute_observed_sources(&driver, &[&right], limits());
                assert!(matches!(
                    *attempt.result.unwrap_err(),
                    ExecutionError::MultipleMatches { match_count: 2, .. }
                ));
                assert_eq!(*right.reads.borrow(), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
            }
        }
    }
}

mod functions_impl {
    use super::*;
    use yamaa_core::value::ValueType;
    use yamaa_core::{
        dataset::{BoundFunction, FunctionArgument, FunctionInput},
        function_invocation::{Argument, FailureKind, FunctionPort, HostError},
        function_signature::{FunctionIdentity, InvocationPlan, Parameter, Presence},
    };

    /// Fixed activated identity and independently declared host-argument ordering.
    fn signature(returns: ColumnType) -> InvocationPlan {
        InvocationPlan::new(
            FunctionIdentity {
                name: "sum".into(),
                contract_version: "1".into(),
                implementation_version: "2".into(),
                call: "project.sum".into(),
            },
            vec![
                Parameter {
                    name: "first".into(),
                    host_name: "lhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "second".into(),
                    host_name: "rhs".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Required,
                },
                Parameter {
                    name: "factor".into(),
                    host_name: "scale".into(),
                    kind: ValueType::Int,
                    accepts_missing: false,
                    presence: Presence::Optional(Value::Int(5)),
                },
            ],
            returns,
            false,
        )
        .unwrap()
    }

    struct Callbacks {
        signature: InvocationPlan,
        calls: Vec<Vec<(String, String)>>,
        result: Result<Value, &'static str>,
    }
    impl FunctionPort for Callbacks {
        type Error = &'static str;
        /// Slot lookup borrows metadata and never invokes the callable.
        fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
            (slot == 0).then_some(&self.signature)
        }
        /// Capture the complete host call while preserving an opaque failure payload.
        fn call(
            &mut self,
            slot: usize,
            arguments: &[Argument<'_>],
        ) -> Result<Value, HostError<Self::Error>> {
            assert_eq!(slot, 0);
            self.calls.push(
                arguments
                    .iter()
                    .map(|a| (a.name.into(), format!("{:?}", a.value)))
                    .collect(),
            );
            self.result.clone().map_err(HostError::Raised)
        }
    }
    /// Construct an explicitly bound callback without activation or discovery effects.
    fn callbacks() -> Callbacks {
        Callbacks {
            signature: signature(ColumnType::Int),
            calls: vec![],
            result: Ok(Value::Int(11)),
        }
    }
    /// The authored order is deliberately the reverse of parameter declaration order.
    fn arguments() -> Vec<FunctionArgument> {
        vec![
            FunctionArgument {
                name: "second".into(),
                input: FunctionInput::Read(Read::Source(2)),
            },
            FunctionArgument {
                name: "first".into(),
                input: FunctionInput::Read(Read::Source(1)),
            },
        ]
    }
    /// Bind a call at one of the two assignment phases without changing its signature.
    fn plan(
        source: &Table,
        mode: RowMode,
        row_call: bool,
        predicate: Option<Filter>,
        signature: InvocationPlan,
        args: Vec<FunctionArgument>,
    ) -> Result<DatasetPlan, PlanError> {
        let call = assign(
            1,
            Expression::Function(BoundFunction::new(0, signature, args)?),
        );
        let mut row = vec![assign(0, Expression::Source(0))];
        let mut columns = vec![];
        if row_call {
            row.push(call);
        } else {
            columns.push(call);
        }
        DatasetPlan::new(
            source.schema.clone(),
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![RowTemplate {
                mode,
                assignments: row,
                filter: predicate,
            }],
            columns,
            vec![0],
            vec![],
        )
    }
    /// Three independent integer source fields with deterministic row membership.
    fn source(rows: Vec<Vec<Value>>) -> Table {
        table(
            &[
                ("ID", ColumnType::Int),
                ("A", ColumnType::Int),
                ("B", ColumnType::Int),
            ],
            rows,
        )
    }

    /// Phase observation must not repeat source reads or callbacks or expose partial output.
    #[test]
    fn phase_observation_preserves_execution_and_effects() {
        use yamaa_core::dataset::ExecutionPhase::*;
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Missing, Value::Int(30)],
        ]);
        let plan = plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap();
        let mut calls = callbacks();
        let mut phases = Vec::new();
        let measured =
            plan.execute_with_phase_observer(&source, &[], &mut calls, limits(), &mut |phase| {
                phases.push((phase, source.reads.borrow().len()))
            });
        assert_eq!(
            phases,
            vec![
                (Admission, 0),
                (Derivation, 0),
                (OutputKeys, 6),
                (Verification, 6),
                (Finished, 6)
            ]
        );
        assert_eq!(
            measured.result.as_ref().unwrap().dataset.rows(),
            &[
                vec![Value::Int(1), Value::Int(11)],
                vec![Value::Int(2), Value::Missing]
            ]
        );
        assert_eq!(calls.calls.len(), 1);
        let reads = source.reads.borrow().clone();
        assert_eq!(reads, vec![(0, 0), (0, 2), (0, 1), (1, 0), (1, 2), (1, 1)]);
        source.reads.borrow_mut().clear();
        let mut plain_calls = callbacks();
        let plain = plan.execute_observed_functions(&source, &[], &mut plain_calls, limits());
        assert_eq!(measured, plain);
        assert_eq!(calls.calls, plain_calls.calls);
        assert_eq!(*source.reads.borrow(), reads);
    }

    #[test]
    fn typed_execution_exits_close_actual_success_and_each_failed_stage_before_finished() {
        use yamaa_core::application_observer::Outcome;
        use yamaa_core::dataset::{ExecutionEvent, ExecutionPhase::*};
        for failure in [
            None,
            Some(Admission),
            Some(Derivation),
            Some(OutputKeys),
            Some(Verification),
        ] {
            let mut source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
            let plan = record_plan(
                &source,
                schema(&[("ID", ColumnType::Int)]),
                vec![assign(0, Expression::Source(0))],
                vec![verification(Check::RowCount {
                    min: None,
                    max: Some(if failure == Some(Verification) { 0 } else { 1 }),
                })],
            );
            match failure {
                Some(Admission) => source.schema = schema(&[("OTHER", ColumnType::Int)]),
                Some(Derivation) => source.fail = Some((0, 0)),
                Some(OutputKeys) => source.rows[0][0] = Value::Missing,
                _ => (),
            }
            let mut events = Vec::new();
            let attempt = plan.execute_with_execution_observer(
                &source,
                &[],
                &mut callbacks(),
                limits(),
                &mut |event| events.push(event),
            );
            assert_eq!(attempt.result.is_ok(), failure.is_none());
            let mut expected = Vec::new();
            for phase in [Admission, Derivation, OutputKeys, Verification] {
                expected.push(ExecutionEvent::Phase(phase));
                expected.push(ExecutionEvent::Exit {
                    phase,
                    outcome: if failure == Some(phase) {
                        Outcome::Failed
                    } else {
                        Outcome::Succeeded
                    },
                });
                if failure == Some(phase) {
                    break;
                }
            }
            expected.push(ExecutionEvent::Phase(Finished));
            assert_eq!(events, expected);
            assert_eq!(source.reads.borrow().is_empty(), failure == Some(Admission));
        }
    }

    /// Every returned failure closes the reached phase without inventing later work.
    #[test]
    fn phase_observation_finishes_at_the_actual_failure_boundary() {
        use yamaa_core::dataset::ExecutionPhase::*;
        for failure in [Admission, Derivation, OutputKeys, Verification] {
            let mut source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
            let plan = record_plan(
                &source,
                schema(&[("ID", ColumnType::Int)]),
                vec![assign(0, Expression::Source(0))],
                vec![verification(Check::RowCount {
                    min: None,
                    max: Some(0),
                })],
            );
            match failure {
                Admission => source.schema = schema(&[("OTHER", ColumnType::Int)]),
                Derivation => source.fail = Some((0, 0)),
                OutputKeys => source.rows[0][0] = Value::Missing,
                Verification => (),
                Finished => unreachable!(),
            }
            let mut phases = Vec::new();
            let measured = plan.execute_with_phase_observer(
                &source,
                &[],
                &mut callbacks(),
                limits(),
                &mut |phase| phases.push(phase),
            );
            let error = measured.result.unwrap_err();
            assert!(match failure {
                Admission => matches!(*error, ExecutionError::SchemaMismatch),
                Derivation => matches!(
                    *error,
                    ExecutionError::Cell {
                        error: CellError::Access("source failure"),
                        ..
                    }
                ),
                OutputKeys => matches!(*error, ExecutionError::KeyFailures(_)),
                Verification => matches!(*error, ExecutionError::VerificationFailures(_)),
                Finished => false,
            });
            let mut expected = vec![Admission, Derivation, OutputKeys, Verification];
            expected.truncate(expected.iter().position(|phase| *phase == failure).unwrap() + 1);
            expected.push(Finished);
            assert_eq!(phases, expected);
        }
    }

    /// Authored reads precede declaration-order checks/defaults and each attempt is fresh.
    #[test]
    fn authored_reads_and_mapped_calls_preserve_distinct_orders() {
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Missing, Value::Int(30)],
        ]);
        let plan = plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap();
        let mut callbacks = callbacks();
        for count in 1..=2 {
            source.reads.borrow_mut().clear();
            let result = plan
                .execute_observed_functions(&source, &[], &mut callbacks, limits())
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![Value::Int(1), Value::Int(11)],
                    vec![Value::Int(2), Value::Missing]
                ]
            );
            assert_eq!(
                *source.reads.borrow(),
                vec![(0, 0), (0, 2), (0, 1), (1, 0), (1, 2), (1, 1)]
            );
            assert_eq!(callbacks.calls.len(), count);
            assert_eq!(
                callbacks.calls[count - 1],
                vec![
                    ("lhs".into(), "Int(10)".into()),
                    ("rhs".into(), "Int(20)".into()),
                    ("scale".into(), "Int(5)".into())
                ]
            );
        }
    }

    /// Missing arguments never hide a later authored resolution error or trigger a callback.
    #[test]
    fn missing_does_not_skip_remaining_argument_resolution() {
        let mut source = source(vec![vec![Value::Int(1), Value::Missing, Value::Int(20)]]);
        source.fail = Some((0, 2));
        let mut args = arguments();
        args.reverse();
        let plan = plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            args,
        )
        .unwrap();
        let mut callbacks = callbacks();
        let error = plan
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
            .result
            .unwrap_err();
        assert!(matches!(
            *error,
            ExecutionError::Cell {
                source_row: 0,
                error: CellError::Access("source failure"),
                ..
            }
        ));
        assert_eq!(*source.reads.borrow(), vec![(0, 0), (0, 1), (0, 2)]);
        assert!(callbacks.calls.is_empty());
    }

    /// Row callbacks precede filtering, whereas column callbacks run only on retained rows.
    #[test]
    fn callback_assignment_phase_and_empty_input_control_effects() {
        let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
        for row_call in [true, false] {
            let plan = plan(
                &source,
                RowMode::Records,
                row_call,
                Some(filter(Node::Boolean(false), vec![])),
                signature(ColumnType::Int),
                arguments(),
            )
            .unwrap();
            let mut callbacks = callbacks();
            assert!(plan
                .execute_observed_functions(&source, &[], &mut callbacks, limits())
                .result
                .unwrap()
                .dataset
                .rows()
                .is_empty());
            assert_eq!(callbacks.calls.len(), usize::from(row_call));
            let empty = table(
                &[
                    ("ID", ColumnType::Int),
                    ("A", ColumnType::Int),
                    ("B", ColumnType::Int),
                ],
                vec![],
            );
            assert!(plan
                .execute_observed_functions(&empty, &[], &mut callbacks, limits())
                .result
                .unwrap()
                .dataset
                .rows()
                .is_empty());
            assert_eq!(callbacks.calls.len(), usize::from(row_call));
        }
    }

    /// Signature mismatches and legacy entrypoints refuse functions before any table access.
    #[test]
    fn all_function_bindings_are_required_before_table_access() {
        struct Unreadable;
        impl TableAccess for Unreadable {
            type Error = &'static str;
            fn schema(&self) -> &TableSchema {
                panic!("schema read before binding admission")
            }
            fn row_count(&self) -> usize {
                panic!("row count read before binding admission")
            }
            fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                panic!("cell read before binding admission")
            }
        }
        let source = source(vec![]);
        let plan = plan(
            &source,
            RowMode::Records,
            false,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap();
        assert!(matches!(
            *plan
                .execute_observed(&Unreadable, limits())
                .result
                .unwrap_err(),
            ExecutionError::FunctionBinding { slot: 0 }
        ));
        let mut callbacks = callbacks();
        callbacks.signature = signature(ColumnType::Str);
        assert!(matches!(
            *plan
                .execute_observed_functions(&Unreadable, &[], &mut callbacks, limits())
                .result
                .unwrap_err(),
            ExecutionError::FunctionBinding { slot: 0 }
        ));
        assert!(callbacks.calls.is_empty());
    }

    /// Fatal callback and result failures retain provenance and bypass conversion handlers.
    #[test]
    fn invocation_failures_are_fatal_and_do_not_retry_or_recover() {
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Int(30), Value::Int(40)],
        ]);
        let plan = plan(
            &source,
            RowMode::Records,
            false,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Int(9))])
        .unwrap();
        for (result, expected) in [
            (Err("callback payload"), "function_call_failed"),
            (Ok(Value::Bool(true)), "invalid_function_result"),
            (Ok(Value::Missing), "invalid_function_result"),
        ] {
            let mut callbacks = callbacks();
            callbacks.result = result;
            let attempt = plan.execute_observed_functions(&source, &[], &mut callbacks, limits());
            match *attempt.result.unwrap_err() {
                ExecutionError::Function {
                    path,
                    error,
                    identity,
                } => {
                    assert_eq!(path, "columns.C1.derivation");
                    assert_eq!(error.condition(), expected);
                    assert_eq!(error.identity, callbacks.signature.identity().clone());
                    assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
                    if expected == "function_call_failed" {
                        assert_eq!(error.kind, FailureKind::CallFailed("callback payload"));
                    }
                }
                other => panic!("unexpected {other:?}"),
            }
            assert_eq!(callbacks.calls.len(), 1);
            assert_eq!(attempt.handler_counts[0].count, 0);
        }
        let mut callbacks = callbacks();
        callbacks.signature = signature(ColumnType::Str);
        callbacks.result = Ok(Value::Str("bad".into()));
        let converted = functions_impl::plan(
            &source,
            RowMode::Records,
            false,
            None,
            signature(ColumnType::Str),
            arguments(),
        )
        .unwrap()
        .with_conversion_handlers(vec![conversion_handler(1, Value::Int(9))])
        .unwrap();
        let attempt = converted.execute_observed_functions(&source, &[], &mut callbacks, limits());
        assert_eq!(
            attempt.result.unwrap().dataset.rows(),
            &[
                vec![Value::Int(1), Value::Int(9)],
                vec![Value::Int(2), Value::Int(9)]
            ]
        );
        assert_eq!(attempt.handler_counts[0].count, 2);
        assert_eq!(callbacks.calls.len(), 2);
    }

    /// Phase and name admission are independent of data cardinality and callback presence.
    #[test]
    fn function_bindings_validate_names_dependencies_and_source_scope() {
        let source = source(vec![]);
        assert_eq!(
            BoundFunction::new(0, signature(ColumnType::Int), vec![]),
            Err(PlanError::InvalidFunction)
        );
        let mut args = arguments();
        args.push(args[0].clone());
        assert_eq!(
            BoundFunction::new(0, signature(ColumnType::Int), args),
            Err(PlanError::InvalidFunction)
        );
        let mut args = arguments();
        args[0].name = "unknown".into();
        assert_eq!(
            BoundFunction::new(0, signature(ColumnType::Int), args),
            Err(PlanError::InvalidFunction)
        );
        assert_eq!(
            plan(
                &source,
                RowMode::Groups(vec![0]),
                true,
                None,
                signature(ColumnType::Int),
                arguments()
            ),
            Err(PlanError::NonGroupSource)
        );
        assert_eq!(
            plan(
                &source,
                RowMode::Keys,
                true,
                None,
                signature(ColumnType::Int),
                arguments()
            ),
            Err(PlanError::InvalidKeyMode)
        );
        assert_eq!(
            plan(
                &source,
                RowMode::Keys,
                false,
                None,
                signature(ColumnType::Int),
                arguments()
            ),
            Err(PlanError::InvalidKeyMode)
        );
        let mut args = arguments();
        args[0].input = FunctionInput::Read(Read::Column(1));
        assert_eq!(
            plan(
                &source,
                RowMode::Records,
                true,
                None,
                signature(ColumnType::Int),
                args
            ),
            Err(PlanError::UnavailableColumn)
        );
    }

    /// Work exhaustion precedes argument reads and calls; text is charged before copying.
    #[test]
    fn function_limits_precede_callback_effects_and_reset_between_attempts() {
        let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]]);
        let plan = plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap();
        let mut callbacks = callbacks();
        let mut tiny = limits();
        tiny.work_cells = 3;
        assert!(matches!(
            *plan
                .execute_observed_functions(&source, &[], &mut callbacks, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit { .. }
        ));
        assert_eq!(*source.reads.borrow(), vec![(0, 0)]);
        assert!(callbacks.calls.is_empty());
        assert!(plan
            .execute_observed_functions(&source, &[], &mut callbacks, limits())
            .result
            .is_ok());
        assert_eq!(callbacks.calls.len(), 1);
        let mut args = arguments();
        args[0].input = FunctionInput::Literal(Value::Str("large".into()));
        let text_plan = functions_impl::plan(
            &source,
            RowMode::Records,
            true,
            None,
            signature(ColumnType::Int),
            args,
        )
        .unwrap();
        tiny = limits();
        tiny.scalar_text_bytes = 0;
        assert!(matches!(
            *text_plan
                .execute_observed_functions(&source, &[], &mut callbacks, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_core::dataset::Resource::ScalarTextBytes,
                ..
            }
        ));
        assert_eq!(callbacks.calls.len(), 1);
    }

    /// Grouped calls run once per group; key-grain calls consume completed outputs only.
    #[test]
    fn group_and_key_grain_calls_use_logical_candidates() {
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Int(30), Value::Int(40)],
        ]);
        let grouped = plan(
            &source,
            RowMode::Groups(vec![0, 1, 2]),
            true,
            None,
            signature(ColumnType::Int),
            arguments(),
        )
        .unwrap();
        let mut calls = callbacks();
        assert_eq!(
            grouped
                .execute_observed_functions(&source, &[], &mut calls, limits())
                .result
                .unwrap()
                .dataset
                .rows(),
            &[
                vec![Value::Int(1), Value::Int(11)],
                vec![Value::Int(2), Value::Int(11)]
            ]
        );
        assert_eq!(calls.calls.len(), 2);
        let args = vec![
            FunctionArgument {
                name: "first".into(),
                input: FunctionInput::Read(Read::Column(0)),
            },
            FunctionArgument {
                name: "second".into(),
                input: FunctionInput::Literal(Value::Int(3)),
            },
        ];
        let keyed = plan(
            &source,
            RowMode::Keys,
            false,
            None,
            signature(ColumnType::Int),
            args,
        )
        .unwrap();
        calls.calls.clear();
        assert_eq!(
            keyed
                .execute_observed_functions(&source, &[], &mut calls, limits())
                .result
                .unwrap()
                .dataset
                .rows(),
            &[
                vec![Value::Int(1), Value::Int(11)],
                vec![Value::Int(2), Value::Int(11)]
            ]
        );
        assert_eq!(calls.calls.len(), 2);
        assert_eq!(calls.calls[0][0].1, "Int(1)");
        assert_eq!(calls.calls[1][0].1, "Int(2)");
    }

    /// Whole-column traversal finishes conversion before dependent callbacks see a value.
    #[test]
    fn later_function_columns_observe_converted_values_in_column_order() {
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Int(30), Value::Int(40)],
        ]);
        let args = |first, second| {
            vec![
                FunctionArgument {
                    name: "first".into(),
                    input: FunctionInput::Read(Read::Column(first)),
                },
                FunctionArgument {
                    name: "second".into(),
                    input: FunctionInput::Read(Read::Column(second)),
                },
            ]
        };
        let plan = DatasetPlan::new(
            source.schema.clone(),
            schema(&[
                ("ID", ColumnType::Int),
                ("A", ColumnType::Int),
                ("B", ColumnType::Int),
            ]),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![assign(0, Expression::Source(0))],
                filter: None,
            }],
            vec![
                assign(
                    1,
                    Expression::Function(
                        BoundFunction::new(0, signature(ColumnType::Str), args(0, 0)).unwrap(),
                    ),
                ),
                assign(
                    2,
                    Expression::Function(
                        BoundFunction::new(0, signature(ColumnType::Str), args(1, 0)).unwrap(),
                    ),
                ),
            ],
            vec![0],
            vec![],
        )
        .unwrap();
        let mut calls = callbacks();
        calls.signature = signature(ColumnType::Str);
        calls.result = Ok(Value::Str("17".into()));
        assert_eq!(
            plan.execute_observed_functions(&source, &[], &mut calls, limits())
                .result
                .unwrap()
                .dataset
                .rows(),
            &[
                vec![Value::Int(1), Value::Int(17), Value::Int(17)],
                vec![Value::Int(2), Value::Int(17), Value::Int(17)]
            ]
        );
        let observed: Vec<_> = calls
            .calls
            .iter()
            .map(|a| (a[0].1.as_str(), a[1].1.as_str()))
            .collect();
        assert_eq!(
            observed,
            vec![
                ("Int(1)", "Int(1)"),
                ("Int(2)", "Int(2)"),
                ("Int(17)", "Int(1)"),
                ("Int(17)", "Int(2)")
            ]
        );
    }
    /// A key-grain call explicitly collects each source leaf in authored order.
    fn collected_arguments() -> Vec<FunctionArgument> {
        [("second", 2, "SOURCE.B"), ("first", 1, "SOURCE.A")]
            .into_iter()
            .map(|(name, column, identifier)| FunctionArgument {
                name: name.into(),
                input: FunctionInput::Collect {
                    column,
                    identifier: identifier.into(),
                },
            })
            .collect()
    }

    /// Equal donors collapse without changing the first representation or callback order.
    #[test]
    fn collected_arguments_use_all_feeders_and_preserve_missing() {
        let source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(2), Value::Missing, Value::Int(30)],
        ]);
        let plan = plan(
            &source,
            RowMode::Keys,
            false,
            None,
            signature(ColumnType::Int),
            collected_arguments(),
        )
        .unwrap();
        for _ in 0..2 {
            source.reads.borrow_mut().clear();
            let mut calls = callbacks();
            let result = plan
                .execute_observed_functions(&source, &[], &mut calls, limits())
                .result
                .unwrap();
            assert_eq!(
                result.dataset.rows(),
                &[
                    vec![Value::Int(1), Value::Int(11)],
                    vec![Value::Int(2), Value::Missing]
                ]
            );
            assert_eq!(
                calls.calls,
                vec![vec![
                    ("lhs".into(), "Int(10)".into()),
                    ("rhs".into(), "Int(20)".into()),
                    ("scale".into(), "Int(5)".into())
                ]]
            );
            assert_eq!(
                *source.reads.borrow(),
                vec![
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (0, 2),
                    (1, 2),
                    (0, 1),
                    (1, 1),
                    (2, 2),
                    (2, 1)
                ]
            );
        }
    }

    /// Missing earlier arguments cannot suppress later ambiguity or opaque source failures.
    #[test]
    fn collected_argument_resolution_finishes_before_missing_short_circuit() {
        let mut source = source(vec![
            vec![Value::Int(1), Value::Int(10), Value::Missing],
            vec![Value::Int(1), Value::Int(11), Value::Missing],
            vec![Value::Int(1), Value::Int(12), Value::Missing],
        ]);
        let plan = plan(
            &source,
            RowMode::Keys,
            false,
            None,
            signature(ColumnType::Int),
            collected_arguments(),
        )
        .unwrap();
        let mut calls = callbacks();
        let error = plan
            .execute_observed_functions(&source, &[], &mut calls, limits())
            .result
            .unwrap_err();
        match *error {
            ExecutionError::MultipleValues {
                path,
                identifier,
                value_count,
                identity,
            } => {
                assert_eq!(path, "columns.C1.derivation");
                assert_eq!(identifier, "SOURCE.A");
                assert_eq!(value_count, 3);
                assert_eq!(identity.unwrap().values, vec![Value::Int(1)]);
            }
            other => panic!("unexpected error {other:?}"),
        }
        assert!(calls.calls.is_empty());
        assert_eq!(
            *source.reads.borrow(),
            vec![
                (0, 0),
                (1, 0),
                (2, 0),
                (0, 2),
                (1, 2),
                (2, 2),
                (0, 1),
                (1, 1),
                (2, 1)
            ]
        );
        source.fail = Some((2, 1));
        source.reads.borrow_mut().clear();
        assert!(matches!(
            *plan
                .execute_observed_functions(&source, &[], &mut calls, limits())
                .result
                .unwrap_err(),
            ExecutionError::Cell {
                source_row: 2,
                error: CellError::Access("source failure"),
                ..
            }
        ));
        assert!(calls.calls.is_empty());
    }

    /// Collected leaves are only legal after key construction, with bounded valid metadata.
    #[test]
    fn collected_argument_admission_precedes_source_effects() {
        let source = source(vec![]);
        for mode in [RowMode::Records, RowMode::Groups(vec![0, 1, 2])] {
            assert_eq!(
                plan(
                    &source,
                    mode,
                    false,
                    None,
                    signature(ColumnType::Int),
                    collected_arguments()
                ),
                Err(PlanError::InvalidKeyMode)
            );
        }
        for (column, identifier, expected) in [
            (99, "SOURCE.X", PlanError::InvalidSource),
            (1, "", PlanError::InvalidKeyMode),
        ] {
            let mut args = collected_arguments();
            args[0].input = FunctionInput::Collect {
                column,
                identifier: identifier.into(),
            };
            assert_eq!(
                plan(
                    &source,
                    RowMode::Keys,
                    false,
                    None,
                    signature(ColumnType::Int),
                    args
                ),
                Err(expected)
            );
        }
        assert!(source.reads.borrow().is_empty());
    }

    /// Work limits stop before a collected scan; fresh attempts and text limits remain independent.
    #[test]
    fn collected_argument_budgets_precede_reads_and_callbacks() {
        let source = source(vec![vec![Value::Int(1), Value::Int(10), Value::Int(20)]; 2]);
        let plan = plan(
            &source,
            RowMode::Keys,
            false,
            None,
            signature(ColumnType::Int),
            collected_arguments(),
        )
        .unwrap();
        let mut calls = callbacks();
        let mut tiny = limits();
        tiny.work_cells = 14;
        assert!(matches!(
            *plan
                .execute_observed_functions(&source, &[], &mut calls, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_core::dataset::Resource::WorkCells,
                ..
            }
        ));
        assert_eq!(*source.reads.borrow(), vec![(0, 0), (1, 0)]);
        assert!(calls.calls.is_empty());
        assert!(plan
            .execute_observed_functions(&source, &[], &mut calls, limits())
            .result
            .is_ok());
        assert_eq!(calls.calls.len(), 1);

        let mut text_source = table(
            &[
                ("ID", ColumnType::Int),
                ("A", ColumnType::Int),
                ("B", ColumnType::Str),
            ],
            vec![vec![Value::Int(1), Value::Int(10), Value::Str("large".into())]; 2],
        );
        let text_plan = functions_impl::plan(
            &text_source,
            RowMode::Keys,
            false,
            None,
            signature(ColumnType::Int),
            collected_arguments(),
        )
        .unwrap();
        tiny = limits();
        tiny.scalar_text_bytes = 4;
        calls.calls.clear();
        assert!(matches!(
            *text_plan
                .execute_observed_functions(&text_source, &[], &mut calls, tiny)
                .result
                .unwrap_err(),
            ExecutionError::Limit {
                resource: yamaa_core::dataset::Resource::ScalarTextBytes,
                ..
            }
        ));
        assert_eq!(*text_source.reads.borrow(), vec![(0, 0), (1, 0), (0, 2)]);
        assert!(calls.calls.is_empty());
        text_source.rows.clear();
        assert!(text_plan
            .execute_observed_functions(&text_source, &[], &mut calls, limits())
            .result
            .is_ok());
        assert!(calls.calls.is_empty());
    }
}

/// Regex work survives row/template boundaries; rejected attempts never refund counters.
#[test]
fn contains_limits_span_rows_templates_and_fresh_dataset_runs() {
    use yamaa_core::dataset::Resource;
    use yamaa_core::regex::{CompileLimits, MatchBudget, MatchLimits, Pattern};
    let subject = "a".repeat(100);
    let pattern = Pattern::compile("z", CompileLimits::default()).unwrap();
    let mut measured = MatchBudget::new(MatchLimits::default());
    assert!(pattern
        .search_with_budget(&subject, MatchLimits::default(), &mut measured)
        .unwrap()
        .is_none());
    let usage = measured.used();
    let ceiling = 2 * usage.work.max(usage.state_cells) - 1;
    for (rows, templates) in [(2, 1), (1, 2)] {
        let source = table(
            &[("id", ColumnType::Str)],
            vec![vec![Value::Str(subject.clone())]; rows],
        );
        let template = RowTemplate {
            mode: RowMode::Records,
            assignments: vec![],
            filter: Some(filter(
                Node::Contains {
                    value: Scalar::Identifier("x".into()),
                    pattern: "z".into(),
                },
                vec![binding("x", Read::Source(0))],
            )),
        };
        let plan = DatasetPlan::new(
            source.schema.clone(),
            source.schema.clone(),
            vec![template; templates],
            vec![assign(0, Expression::Source(0))],
            vec![0],
            vec![],
        )
        .unwrap();
        let error = plan
            .execute(
                &source,
                Limits {
                    work_cells: ceiling,
                    ..limits()
                },
            )
            .unwrap_err();
        assert!(
            matches!(*error, ExecutionError::Limit { resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells, limit, .. } if limit == ceiling)
        );
        assert_eq!(source.reads.borrow().len(), 2);
        assert!(plan
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}

/// Empty-output declaration checks share matching quotas across predicate plans.
#[test]
fn contains_declaration_checks_do_not_reset_match_budgets() {
    use yamaa_core::dataset::Resource;
    let source = table(&[("id", ColumnType::Int)], vec![]);
    let checks: Vec<_> = (0..2)
        .map(|index| {
            verification(Check::Assert {
                when: None,
                require: check_predicate(
                    Node::Contains {
                        value: Scalar::Literal(Value::Str("a".repeat(100))),
                        pattern: "z".into(),
                    },
                    vec![],
                    &format!("verifications[{index}].assert.require"),
                ),
            })
        })
        .collect();
    let single = record_plan(
        &source,
        source.schema.clone(),
        vec![assign(0, Expression::Source(0))],
        vec![checks[0].clone()],
    );
    assert_eq!(
        single
            .execute(
                &source,
                Limits {
                    work_cells: 1000,
                    ..limits()
                }
            )
            .unwrap()
            .verifications
            .len(),
        1
    );
    let plan = record_plan(
        &source,
        source.schema.clone(),
        vec![assign(0, Expression::Source(0))],
        checks,
    );
    // Each literal search fits alone, while both exhaust this dataset-wide state quota.
    let error = plan
        .execute(
            &source,
            Limits {
                work_cells: 1000,
                ..limits()
            },
        )
        .unwrap_err();
    assert!(matches!(
        *error,
        ExecutionError::Limit {
            resource: Resource::PredicateRegexWork | Resource::PredicateRegexStateCells,
            limit: 1000,
            ..
        }
    ));
    assert_eq!(
        plan.execute(&source, limits()).unwrap().verifications.len(),
        2
    );
}

#[test]
fn numeric_reductions_check_present_values_after_collecting_source_arguments() {
    use yamaa_core::table_reduction::TableReductionError;
    use yamaa_core::value::ValueType;
    for reducer in [NumericReducer::Sum, NumericReducer::Mean] {
        let mut source = table(
            &[("id", ColumnType::Str), ("x", ColumnType::Str)],
            vec![
                vec![Value::Str("A".into()), Value::Missing],
                vec![Value::Str("A".into()), Value::Missing],
            ],
        );
        let make = |identifier| {
            DatasetPlan::new(
                source.schema.clone(),
                schema(&[("id", ColumnType::Str), ("total", ColumnType::Float)]),
                vec![RowTemplate {
                    mode: RowMode::Groups(vec![0]),
                    filter: None,
                    assignments: vec![
                        assign(0, Expression::Source(0)),
                        assign(
                            1,
                            Expression::Reduce {
                                identifier,
                                column: 1,
                                reducer,
                                text: format!("{}(T.x)", reducer.name()),
                            },
                        ),
                    ],
                }],
                vec![],
                vec![0],
                vec![],
            )
            .unwrap()
        };
        let plan = make(Some("T.x".into()));
        let fallback = make(None);
        assert_eq!(
            plan.execute(&source, limits()).unwrap().dataset.rows(),
            &[vec![Value::Str("A".into()), Value::Missing]]
        );
        source.rows[1][1] = Value::Str("2".into());
        // Numeric-looking text is not coerced. The authored expression/operand
        // remain attached to the runtime type finding, with no output identity.
        for (plan, name) in [(&plan, "T.x"), (&fallback, "x")] {
            assert_eq!(
                *plan.execute(&source, limits()).unwrap_err(),
                ExecutionError::ReductionType {
                    path: "columns.C1.derivation".into(),
                    expression: format!("{}(T.x)", reducer.name()),
                    reducer,
                    source: name.into(),
                    actual: ValueType::Str,
                }
            );
        }
        source.rows[0][1] = Value::Str("first bad value".into());
        source.fail = Some((1, 1));
        assert!(matches!(
            *plan.execute(&source, limits()).unwrap_err(),
            ExecutionError::Reduction {
                error: TableReductionError::Cell {
                    position: 1,
                    error: CellError::Access("source failure")
                },
                ..
            }
        ));
        source.fail = None;
        source.rows.clear();
        assert!(plan
            .execute(&source, limits())
            .unwrap()
            .dataset
            .rows()
            .is_empty());
    }
}

#[test]
fn first_available_skips_unselected_reads_and_does_not_fallback_after_conversion() {
    use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
    let mut source = table(
        &[
            ("ID", ColumnType::Int),
            ("A", ColumnType::Str),
            ("B", ColumnType::Str),
        ],
        vec![
            vec![
                Value::Int(1),
                Value::Str("bad".into()),
                Value::Str("20".into()),
            ],
            vec![
                Value::Int(1),
                Value::Str("other".into()),
                Value::Str("20".into()),
            ],
        ],
    );
    let make = |columns: &[usize]| {
        key_plan(
            &source,
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![0],
            vec![assign(0, Expression::Source(0))],
            vec![assign(
                1,
                Expression::FirstAvailable(Box::new(FirstAvailable::new(
                    columns
                        .iter()
                        .map(|&column| SelectionSource {
                            path: format!("operand.{column}"),
                            read: SelectionRead::Collect {
                                column,
                                identifier: format!("SRC.C{column}"),
                                filter: None,
                            },
                        })
                        .collect(),
                    Value::Int(99),
                ))),
            )],
        )
    };
    let skipped = make(&[2, 1]);
    let chosen = make(&[1, 2]);
    source.fail = Some((0, 1));
    assert_eq!(
        skipped.execute(&source, limits()).unwrap().dataset.rows(),
        &[vec![Value::Int(1), Value::Int(20)]]
    );
    assert!(source.reads.borrow().iter().all(|&(_, column)| column != 1));
    source.fail = None;
    source.rows[1][1] = Value::Str("bad".into());
    source.reads.borrow_mut().clear();
    assert!(
        matches!(*chosen.execute(&source, limits()).unwrap_err(),ExecutionError::Conversion { ref path, output_row:0, .. } if path=="columns.V")
    );
    assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));
}

/// Collection ambiguity keeps the selection owner through an absence wrapper;
/// reached host failures and handlers retain their separate operand provenance.
#[test]
fn first_available_expression_owns_collection_failure_without_fallback() {
    use yamaa_core::dataset::{FirstAvailable, SelectionRead, SelectionSource};
    let mut source = table(
        &[
            ("ID", ColumnType::Int),
            ("A", ColumnType::Int),
            ("B", ColumnType::Int),
        ],
        vec![
            vec![Value::Int(1), Value::Int(10), Value::Int(20)],
            vec![Value::Int(1), Value::Int(11), Value::Int(20)],
        ],
    );
    let owner = "columns.V.derivation.first_available";
    let operand = "columns.V.derivation.first_available.sources[0]";
    let raw = "columns.V.derivation.first_available.sources[0].value";
    let make = |absent_variable| {
        let first = Assignment {
            column: 1,
            path: operand.into(),
            expression: Expression::SourceBinding {
                source: Box::new(Assignment {
                    column: 1,
                    path: raw.into(),
                    expression: if absent_variable {
                        Expression::Literal(Value::Missing)
                    } else {
                        Expression::Collect {
                            column: 1,
                            identifier: "SRC.A".into(),
                            filter: None,
                            selection: None,
                        }
                    },
                }),
                absent: Some(Value::Int(9)),
                variable_absent: absent_variable,
                selection_declared: false,
                variable: if absent_variable {
                    "UNDECLARED"
                } else {
                    "SRC.A"
                }
                .into(),
                path: operand.into(),
            },
        };
        key_plan(
            &source,
            schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
            vec![0],
            vec![assign(0, Expression::Source(0))],
            vec![Assignment {
                column: 1,
                path: owner.into(),
                expression: Expression::FirstAvailable(Box::new(FirstAvailable::new(
                    vec![
                        SelectionSource {
                            path: operand.into(),
                            read: SelectionRead::Expression(Box::new(first)),
                        },
                        SelectionSource {
                            path: "columns.V.derivation.first_available.sources[1]".into(),
                            read: SelectionRead::Collect {
                                column: 2,
                                identifier: "SRC.B".into(),
                                filter: None,
                            },
                        },
                    ],
                    Value::Int(99),
                ))),
            }],
        )
    };
    let plan = make(false);
    let absent = make(true);
    let attempt = plan.execute_observed(&source, limits());
    let ExecutionError::MultipleValues {
        path,
        identifier,
        value_count,
        identity,
    } = *attempt.result.unwrap_err()
    else {
        panic!("distinct reached readings must fail the selection");
    };
    assert_eq!(path, owner);
    assert_eq!(identifier, "SRC.A");
    assert_eq!(value_count, 2);
    assert_eq!(
        identity,
        Some(yamaa_core::dataset::RowIdentity {
            position: 0,
            values: vec![Value::Int(1)],
        })
    );
    assert_eq!(
        attempt.handler_counts,
        vec![yamaa_core::numeric_lifecycle::HandlerCount {
            spec_path: format!("{operand}.absent"),
            handler: yamaa_core::numeric_lifecycle::HandlerKind::Absent,
            count: 0,
        }],
    );
    assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));

    source.reads.borrow_mut().clear();
    source.fail = Some((1, 1));
    assert!(matches!(
        *plan.execute(&source, limits()).unwrap_err(),
        ExecutionError::Cell {
            path,
            source_row: 1,
            error: CellError::Access("source failure"),
        } if path == raw
    ));
    assert!(source.reads.borrow().iter().all(|&(_, column)| column != 2));

    source.reads.borrow_mut().clear();
    let attempt = absent.execute_observed(&source, limits());
    assert_eq!(
        attempt.result.unwrap().dataset.rows(),
        &[vec![Value::Int(1), Value::Int(9)]]
    );
    assert_eq!(attempt.handler_counts.len(), 1);
    assert_eq!(
        attempt.handler_counts[0].spec_path,
        format!("{operand}.absent")
    );
    assert_eq!(
        attempt.handler_counts[0].handler,
        yamaa_core::numeric_lifecycle::HandlerKind::Absent
    );
    assert_eq!(attempt.handler_counts[0].count, 1);
    assert!(source.reads.borrow().iter().all(|&(_, column)| column == 0));
}

/// UNKNOWN continues; inactive branches cannot read a failing donor. Selected
/// string literals are converted once by the owning output assignment.
#[test]
fn ordered_cases_are_lazy_and_convert_the_selected_raw_value() {
    use yamaa_core::dataset::CaseBranch;
    let mut source = table(
        &[
            ("ID", ColumnType::Int),
            ("FLAG", ColumnType::Int),
            ("BAD", ColumnType::Str),
        ],
        vec![
            vec![Value::Int(1), Value::Int(1), Value::Str("bad".into())],
            vec![Value::Int(2), Value::Int(-1), Value::Str("bad".into())],
            vec![Value::Int(3), Value::Missing, Value::Str("bad".into())],
        ],
    );
    source.fail = Some((0, 2));
    let never = filter(
        Node::Compare {
            operator: Comparison::Equal,
            left: Scalar::Literal(Value::Int(0)),
            right: Scalar::Literal(Value::Int(1)),
        },
        vec![],
    );
    let branch = |when, expression| CaseBranch {
        when,
        assignment: Box::new(assign(2, expression)),
    };
    let plan = DatasetPlan::new(
        source.schema.clone(),
        schema(&[
            ("ID", ColumnType::Int),
            ("FLAG", ColumnType::Int),
            ("V", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![assign(
            2,
            Expression::Case(vec![
                branch(
                    Some(positive(Read::Column(1))),
                    Expression::Literal(Value::Str("7".into())),
                ),
                branch(Some(never), Expression::Source(2)),
                branch(None, Expression::Literal(Value::Int(8))),
            ]),
        )],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute(&source, limits()).unwrap().dataset.rows(),
        &[
            vec![Value::Int(1), Value::Int(1), Value::Int(7)],
            vec![Value::Int(2), Value::Int(-1), Value::Int(8)],
            vec![Value::Int(3), Value::Missing, Value::Int(8)]
        ]
    );
    assert!(source.reads.borrow().iter().all(|&(_, column)| column < 2));
}

/// Matching uses the driver's raw code even when the output code has another
/// value. Conflicting raw readings are a collection failure, never first-row wins.
#[test]
fn named_record_matching_keeps_raw_driver_keys_independent_of_output_columns() {
    use yamaa_core::dataset::{IntermediateKey, MatchRead, MatchValue};
    let driver = table(
        &[("ID", ColumnType::Int), ("RAW", ColumnType::Str)],
        vec![
            vec![Value::Int(1), Value::Str("RAW".into())],
            vec![Value::Int(1), Value::Str("RAW".into())],
        ],
    );
    let right = table(
        &[
            ("ID", ColumnType::Int),
            ("CODE", ColumnType::Str),
            ("ORD", ColumnType::Int),
        ],
        vec![
            vec![Value::Int(1), Value::Str("RAW".into()), Value::Int(1)],
            vec![Value::Int(1), Value::Str("OUTPUT".into()), Value::Int(2)],
        ],
    );
    let mut item = named_item();
    item.keys = vec![IntermediateKey {
        source_column: 1,
        path: "intermediates[0].key".into(),
        value: MatchValue::Variable(MatchRead::Record {
            column: 1,
            identifier: "DRIVER.RAW".into(),
        }),
    }];
    use yamaa_core::dataset::{SecondarySource, SourceSchemas};
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: driver.schema.clone(),
            secondary: vec![SecondarySource {
                name: "OTHER".into(),
                schema: right.schema.clone(),
            }],
        },
        vec![item],
        schema(&[
            ("ID", ColumnType::Int),
            ("CODE", ColumnType::Str),
            ("V", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![
            assign(1, Expression::Literal(Value::Str("OUTPUT".into()))),
            assign(
                2,
                Expression::Intermediate {
                    index: 0,
                    column: 2,
                },
            ),
        ],
        vec![0],
        vec![],
    )
    .unwrap();
    assert_eq!(
        plan.execute_observed_sources(&driver, &[&right], limits())
            .result
            .unwrap()
            .dataset
            .rows(),
        &[vec![
            Value::Int(1),
            Value::Str("OUTPUT".into()),
            Value::Int(1)
        ]]
    );
    let conflict = table(
        &[("ID", ColumnType::Int), ("RAW", ColumnType::Str)],
        vec![
            vec![Value::Int(1), Value::Str("RAW".into())],
            vec![Value::Int(1), Value::Str("OTHER".into())],
        ],
    );
    assert!(matches!(
        *plan
            .execute_observed_sources(&conflict, &[&right], limits())
            .result
            .unwrap_err(),
        ExecutionError::MultipleValues { .. }
    ));
}

/// Presentation admits every declared logical type, preserving stable ties and missing placement.
#[test]
fn presentation_order_preserves_all_types_ties_and_direction_independent_nulls() {
    use yamaa_core::dataset::OrderTerm;
    use yamaa_core::{
        temporal::{DateTime, DateTimePrecision},
        value::FiniteFloat,
    };
    let date = |year, precision| Value::Date(Date::new(year, 1, 1, precision).unwrap());
    let datetime = |year, precision| {
        Value::DateTime(
            DateTime::new(
                Date::new(year, 1, 1, DatePrecision::Day).unwrap(),
                0,
                0,
                0,
                precision,
            )
            .unwrap(),
        )
    };
    for (kind, low, high, tied) in [
        (
            ColumnType::Str,
            Value::Str("A\0z".into()),
            Value::Str("\u{00e9}".into()),
            Value::Str("A\0z".into()),
        ),
        (
            ColumnType::Int,
            Value::Int(9_007_199_254_740_992),
            Value::Int(9_007_199_254_740_993),
            Value::Int(9_007_199_254_740_992),
        ),
        (
            ColumnType::Float,
            Value::Float(FiniteFloat::new(-0.0).unwrap()),
            Value::Float(FiniteFloat::new(1.0).unwrap()),
            Value::Float(FiniteFloat::new(0.0).unwrap()),
        ),
        (
            ColumnType::Date,
            date(2020, DatePrecision::Year),
            date(2021, DatePrecision::Day),
            date(2020, DatePrecision::Month),
        ),
        (
            ColumnType::DateTime,
            datetime(2020, DateTimePrecision::Day),
            datetime(2021, DateTimePrecision::Second),
            datetime(2020, DateTimePrecision::Second),
        ),
    ] {
        let source = table(
            &[("ID", ColumnType::Int), ("V", kind)],
            vec![
                vec![Value::Int(1), high],
                vec![Value::Int(2), Value::Missing],
                vec![Value::Int(3), low],
                vec![Value::Int(4), tied],
                vec![Value::Int(5), Value::Missing],
            ],
        );
        for descending in [false, true] {
            for nulls_first in [false, true] {
                let plan = DatasetPlan::new(
                    source.schema.clone(),
                    source.schema.clone(),
                    vec![RowTemplate {
                        mode: RowMode::Records,
                        assignments: vec![
                            assign(0, Expression::Source(0)),
                            assign(1, Expression::Source(1)),
                        ],
                        filter: None,
                    }],
                    vec![],
                    vec![0],
                    vec![],
                )
                .unwrap()
                .with_output_order(vec![OrderTerm {
                    column: 1,
                    descending,
                    nulls_first,
                }])
                .unwrap();
                let result = plan.execute(&source, limits()).unwrap();
                let present = if descending {
                    vec![1, 3, 4]
                } else {
                    vec![3, 4, 1]
                };
                let expected = if nulls_first {
                    [vec![2, 5], present].concat()
                } else {
                    [present, vec![2, 5]].concat()
                };
                assert_eq!(
                    result
                        .dataset
                        .rows()
                        .iter()
                        .map(|r| match r[0] {
                            Value::Int(v) => v,
                            _ => panic!("ID"),
                        })
                        .collect::<Vec<_>>(),
                    expected,
                    "{kind:?} desc={descending} first={nulls_first}"
                );
                // Exact cell representations stay with their original row, including zero sign and precision.
                for row in result.dataset.rows() {
                    let Value::Int(id) = row[0] else { panic!("ID") };
                    let original = &source.rows[usize::try_from(id - 1).unwrap()][1];
                    match (&row[1], original) {
                        (Value::Float(a), Value::Float(b)) => {
                            assert_eq!(a.get().to_bits(), b.get().to_bits())
                        }
                        (Value::Date(a), Value::Date(b)) => {
                            assert_eq!(a.collected_precision(), b.collected_precision())
                        }
                        (Value::DateTime(a), Value::DateTime(b)) => {
                            assert_eq!(a.collected_precision(), b.collected_precision())
                        }
                        (actual, expected) => assert_eq!(actual, expected),
                    }
                }
            }
        }
    }
}

/// Ordering follows window evaluation and admits only distinct bound columns.
#[test]
fn presentation_keeps_window_neighbors_and_rejects_invalid_terms() {
    use yamaa_core::dataset::{OrderTerm, WindowKind};
    let source = table(
        &[
            ("ID", ColumnType::Int),
            ("G", ColumnType::Str),
            ("V", ColumnType::Int),
        ],
        vec![
            vec![Value::Int(1), Value::Str("a".into()), Value::Int(10)],
            vec![Value::Int(2), Value::Str("a".into()), Value::Int(20)],
            vec![Value::Int(3), Value::Str("a".into()), Value::Int(30)],
        ],
    );
    let plan = value_window_plan(
        &source,
        WindowKind::RowValue {
            column: 2,
            offset: -1,
        },
        None,
    )
    .unwrap()
    .with_output_order(vec![OrderTerm {
        column: 0,
        descending: true,
        nulls_first: false,
    }])
    .unwrap();
    let result = plan.execute(&source, limits()).unwrap();
    assert_eq!(
        result
            .dataset
            .rows()
            .iter()
            .map(|r| (r[0].clone(), r[3].clone()))
            .collect::<Vec<_>>(),
        [
            (Value::Int(3), Value::Int(20)),
            (Value::Int(2), Value::Int(10)),
            (Value::Int(1), Value::Missing)
        ]
    );
    assert!(plan
        .clone()
        .with_output_order(vec![OrderTerm {
            column: 99,
            descending: false,
            nulls_first: false
        }])
        .is_err());
    let duplicate = OrderTerm {
        column: 0,
        descending: false,
        nulls_first: false,
    };
    assert!(plan
        .with_output_order(vec![duplicate.clone(), duplicate])
        .is_err());
}

/// Exhaustion during presentation returns no partial artifact and retains completed checks.
#[test]
fn presentation_budget_failure_retains_checks_and_recovers_on_a_fresh_run() {
    use yamaa_core::dataset::{OrderTerm, Resource};
    let source = table(
        &[("ID", ColumnType::Int)],
        vec![
            vec![Value::Int(3)],
            vec![Value::Int(1)],
            vec![Value::Int(2)],
        ],
    );
    let base = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![Verification {
            path: "verifications[0].row_count".into(),
            check: Check::RowCount {
                min: Some(3),
                max: Some(3),
            },
        }],
    )
    .unwrap();
    let ordered = base
        .clone()
        .with_output_order(vec![OrderTerm {
            column: 0,
            descending: false,
            nulls_first: false,
        }])
        .unwrap();
    let quota = (1..limits().work_cells)
        .find(|quota| {
            base.execute(
                &source,
                Limits {
                    work_cells: *quota,
                    ..limits()
                },
            )
            .is_ok()
        })
        .unwrap();
    let attempt = ordered.execute_observed(
        &source,
        Limits {
            work_cells: quota,
            ..limits()
        },
    );
    assert!(matches!(
        *attempt.result.unwrap_err(),
        ExecutionError::Limit {
            resource: Resource::WorkCells,
            ..
        }
    ));
    assert_eq!(attempt.retained_verifications.len(), 1);
    assert_eq!(attempt.retained_verifications[0].failed_count, 0);
    let recovered = ordered.execute(&source, limits()).unwrap();
    assert_eq!(
        recovered.dataset.rows(),
        [
            vec![Value::Int(1)],
            vec![Value::Int(2)],
            vec![Value::Int(3)]
        ]
    );
    assert_eq!(recovered.verifications.len(), 1);
    // A failed verification wins before presentation can consume any sort work.
    let fatal = DatasetPlan::new(
        source.schema.clone(),
        source.schema.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![Verification {
            path: "verifications[0].row_count".into(),
            check: Check::RowCount {
                min: Some(4),
                max: None,
            },
        }],
    )
    .unwrap()
    .with_output_order(vec![OrderTerm {
        column: 0,
        descending: false,
        nulls_first: false,
    }])
    .unwrap();
    assert!(matches!(
        *fatal
            .execute(
                &source,
                Limits {
                    work_cells: quota,
                    ..limits()
                }
            )
            .unwrap_err(),
        ExecutionError::VerificationFailures(_)
    ));
}

#[test]
fn grouped_row_count_failures_retain_first_output_positions_and_empty_artifacts() {
    use yamaa_core::dataset::{RowCount, RowCountBound};
    for (rows, grouped, expected_positions) in [
        (
            vec![
                vec![Value::Int(1), Value::Str("a".into())],
                vec![Value::Int(2), Value::Str("a".into())],
                vec![Value::Int(3), Value::Str("b".into())],
                vec![Value::Int(4), Value::Str("b".into())],
            ],
            true,
            vec![0, 2],
        ),
        (vec![], false, vec![0]),
    ] {
        let source = table(&[("ID", ColumnType::Int), ("G", ColumnType::Str)], rows);
        let check = RowCount {
            min: Some(RowCountBound::new(if grouped { "3" } else { "1" }).unwrap()),
            max: None,
            min_fraction: None,
            max_fraction: None,
            group_by: grouped.then_some(vec![1]),
            filter: None,
            when: None,
        };
        let plan = DatasetPlan::new(
            source.schema.clone(),
            source.schema.clone(),
            vec![RowTemplate {
                mode: RowMode::Records,
                assignments: vec![
                    assign(0, Expression::Source(0)),
                    assign(1, Expression::Source(1)),
                ],
                filter: None,
            }],
            vec![],
            vec![0],
            vec![Verification {
                path: "verifications[0].row_count".into(),
                check: Check::ScopedRowCount(check),
            }],
        )
        .unwrap();
        let attempt = plan.execute_observed(&source, limits());
        let ExecutionError::VerificationFailures(records) = *attempt.result.unwrap_err() else {
            panic!("expected complete failed row-count records")
        };
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(
            record
                .offending_rows
                .iter()
                .map(|row| row.position)
                .collect::<Vec<_>>(),
            expected_positions
        );
        assert_eq!(record.failed_count, if grouped { 2 } else { 1 });
        assert_eq!(
            record.row_count.as_ref().unwrap().counts,
            if grouped { vec![2, 2] } else { vec![0] }
        );
    }
}

#[test]
fn filtered_intermediate_grouping_charges_only_eligible_key_cells() {
    use yamaa_core::{
        bound_expression::{Binding, BoundPredicate, Read},
        dataset::{
            Intermediate, IntermediateKey, IntermediateSource, MatchRead, MatchValue, SourceSchemas,
        },
    };
    let names = (0..16)
        .map(|i| format!("K{i}"))
        .chain(["KEEP".into()])
        .collect::<Vec<_>>();
    let fields = names
        .iter()
        .map(|name| (name.as_str(), ColumnType::Int))
        .collect::<Vec<_>>();
    let mut rows = vec![vec![Value::Int(0); 17]; 100];
    rows[99][0] = Value::Int(1);
    rows[99][16] = Value::Int(1);
    let source = table(&fields, rows);
    let filter = BoundPredicate::new(
        yamaa_core::predicate_compiler::compile(
            "KEEP = 1",
            "intermediates[0].filter",
            Default::default(),
        )
        .unwrap(),
        vec![Binding {
            name: "KEEP".into(),
            read: Read::Source(16),
        }],
    )
    .unwrap();
    let item = Intermediate {
        identifier: "KEPT".into(),
        path: "intermediates[0]".into(),
        source: IntermediateSource::Primary {
            schema: source.schema.clone(),
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
        filter: Some(filter.into()),
        selection: None,
        no_match: None,
        range: None,
        columns: None,
        unique: Vec::new(),
        derivations: None,
        pending_schema: None,
    };
    let plan = DatasetPlan::new_with_intermediates(
        SourceSchemas {
            primary: source.schema.clone(),
            secondary: vec![],
        },
        vec![item],
        schema(&[("ID", ColumnType::Int)]),
        vec![RowTemplate {
            mode: RowMode::IntermediateDriver {
                index: 0,
                source: 0,
                mode: Box::new(RowMode::Groups((0..16).collect())),
            },
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        vec![],
        vec![0],
        vec![],
    )
    .unwrap();
    let result = plan
        .execute(
            &source,
            Limits {
                work_cells: 1000,
                ..limits()
            },
        )
        .unwrap();
    assert_eq!(result.dataset.rows(), [vec![Value::Int(1)]]);
}
