//! Partitioned execution (#3697): a run over the partitions of its driver
//! input against independently expected whole-table results, the refusal of
//! a step that needs the whole table and its whole-table fallback, and the
//! end states of a cancelled or failed partitioned run.
//!
//! Every expectation is written by hand from the plan and its input: the
//! values, their order, the handler counts, the check records and the first
//! failure a whole-table run produces. Each plan runs whole and over every
//! listed division of its input, empty partitions included, and every run
//! must equal the one authored attempt.
use super::execution::{
    CheckRecord, Dataset, Execution, ExecutionAttempt, ExecutionError, ExecutionEvent,
    ExecutionPhase, RowIdentity, TemplateRows,
};
use super::{
    Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, DatasetPlan, Expression,
    OrderTerm, RowMode, RowTemplate, Verification, Window, WindowKind,
};
use crate::application_observer::Outcome;
use crate::bound_expression::{Binding as Bound, BoundPredicate, Read as BoundRead};
use crate::conversion::{ConversionError, ConversionReason, LiteralHandler};
use crate::diagnostic::ConditionCode;
use crate::function_invocation::NoFunctions;
use crate::numeric_lifecycle::{HandlerCount, HandlerKind};
use crate::table::{CellError, Column, TableAccess, TableSchema, ValueRef};
use crate::value::{ColumnType, Value, ValueType};
use std::cell::Cell;
use std::convert::Infallible;

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
fn assign(column: usize, expression: Expression) -> Assignment {
    Assignment {
        column,
        expression,
        path: format!("columns.C{column}.derivation"),
    }
}
fn predicate(text: &str, path: &str, bindings: Vec<Bound>) -> BoundPredicate {
    BoundPredicate::new(
        crate::expression::compile(text, crate::expression::Context::Predicate, path).unwrap(),
        bindings,
    )
    .unwrap()
}
fn verification(path: &str, check: Check) -> Verification {
    Verification {
        path: path.into(),
        check,
    }
}
fn int(value: i64) -> Value {
    Value::Int(value)
}
fn text(value: &str) -> Value {
    Value::Str(value.into())
}
fn record(path: &str, code: ConditionCode, counts: (usize, usize, usize)) -> CheckRecord {
    let (evaluated_count, failed_count, output_rows) = counts;
    CheckRecord {
        path: path.into(),
        code,
        evaluated_count,
        failed_count,
        output_rows,
        offending_rows: Vec::new(),
        codelist: None,
        inventory: None,
        row_count: None,
        entries: None,
    }
}
fn identity(position: usize, key: i64) -> RowIdentity {
    RowIdentity {
        position,
        values: vec![int(key)],
    }
}

/// A snapshot of typed rows that counts its cell reads.
struct Rows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: Cell<usize>,
}
impl Rows {
    fn new(schema: TableSchema, rows: Vec<Vec<Value>>) -> Self {
        Self {
            schema,
            rows,
            reads: Cell::new(0),
        }
    }
    /// The snapshot divided into consecutive partitions of these sizes.
    fn divided(&self, sizes: &[usize]) -> Vec<Rows> {
        assert_eq!(sizes.iter().sum::<usize>(), self.rows.len());
        let mut start = 0;
        sizes
            .iter()
            .map(|&size| {
                start += size;
                Rows::new(self.schema.clone(), self.rows[start - size..start].to_vec())
            })
            .collect()
    }
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
        self.reads.set(self.reads.get() + 1);
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

/// The divisions every plan runs over: one partition, one record each,
/// uneven, and with empty partitions first, between and last.
fn divisions(rows: usize) -> Vec<Vec<usize>> {
    let mut divisions = vec![vec![rows], vec![1; rows], vec![0, rows, 0]];
    if rows >= 3 {
        divisions.push(vec![2, 0, rows - 2]);
        divisions.push(vec![rows - 1, 1]);
    }
    divisions
}

/// One partitioned attempt over `input` divided into `sizes`.
fn partitioned(
    plan: &super::ResolvedPlan,
    input: &Rows,
    sizes: &[usize],
    fallback: bool,
) -> ExecutionAttempt<Infallible> {
    let parts = input.divided(sizes);
    let parts: Vec<&dyn TableAccess<Error = Infallible>> = parts
        .iter()
        .map(|part| part as &dyn TableAccess<Error = Infallible>)
        .collect();
    plan.execute_partitioned(
        &parts,
        &[],
        &mut NoFunctions::new(),
        None,
        &|| false,
        fallback,
    )
}

/// The plan runs whole and over every division of `input` with the one
/// authored attempt.
fn assert_every_division(
    plan: &DatasetPlan,
    input: &Rows,
    expected: &ExecutionAttempt<Infallible>,
) {
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(&whole, expected, "the whole-table run");
    for sizes in divisions(input.rows.len()) {
        let attempt = partitioned(&resolved, input, &sizes, false);
        assert_eq!(&attempt, expected, "partitions {sizes:?}");
    }
}

/// A partition-safe plan: a record template whose W converts V through an
/// `unconvertible` handler and whose GRADE is a case over V, a column phase
/// that copies GRADE and converts V again through its own handler, a
/// warning range check on W, a row count, output keys and an output order.
fn graded() -> (DatasetPlan, Rows) {
    let float = ColumnType::Float;
    let plan = DatasetPlan::new(
        schema(&[("ID", ColumnType::Int), ("V", float)]),
        schema(&[
            ("ID", ColumnType::Int),
            ("W", ColumnType::Int),
            ("GRADE", ColumnType::Str),
            ("G2", ColumnType::Str),
            ("W2", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
                assign(
                    2,
                    Expression::Case(vec![
                        CaseBranch {
                            when: Some(predicate(
                                "V > 1",
                                "columns.C2.derivation.case[0].when",
                                vec![Bound {
                                    name: "V".into(),
                                    read: BoundRead::Source(1),
                                }],
                            )),
                            assignment: Box::new(assign(2, Expression::Literal(text("HIGH")))),
                        },
                        CaseBranch {
                            when: None,
                            assignment: Box::new(assign(2, Expression::Literal(text("LOW")))),
                        },
                    ]),
                ),
            ],
            filter: None,
        }],
        vec![
            assign(3, Expression::Column(2)),
            assign(4, Expression::Source(1)),
        ],
        vec![0],
        vec![verification(
            "verifications[0]",
            Check::RowCount {
                min: Some(1),
                max: None,
            },
        )],
    )
    .with_column_verifications(vec![ColumnVerifications {
        column: 1,
        checks: vec![verification(
            "columns.W.verifications[0]",
            Check::Range {
                min: Some(int(0)),
                max: Some(int(2)),
            },
        )],
    }])
    .with_warning_verifications(vec!["columns.W.verifications[0]".into()])
    .with_output_order(vec![OrderTerm {
        column: 2,
        descending: false,
        nulls_first: false,
    }])
    .with_conversion_handlers(vec![
        ConversionHandler {
            assignment_path: "columns.C1.derivation".into(),
            handler: LiteralHandler {
                spec_path: "columns.W.unconvertible".into(),
                value: Value::Missing,
            },
        },
        ConversionHandler {
            assignment_path: "columns.C4.derivation".into(),
            handler: LiteralHandler {
                spec_path: "columns.W2.unconvertible".into(),
                value: Value::Missing,
            },
        },
    ]);
    let input = Rows::new(
        schema(&[("ID", ColumnType::Int), ("V", float)]),
        [2.0, 1.5, 0.5, 3.0, 2.5]
            .iter()
            .enumerate()
            .map(|(index, &v)| vec![int(index as i64 + 1), Value::float(v)])
            .collect(),
    );
    (plan, input)
}

/// The whole-table attempt of [`graded`], derived by hand: 1.5, 0.5 and 2.5
/// are not integral, so each conversion of V fires its handler three times
/// and keeps missing; V > 1 grades every record but the third HIGH; the
/// range check finds W = 3 on the fourth candidate; and the output order puts
/// the HIGH rows first, each in construction order.
fn graded_attempt() -> ExecutionAttempt<Infallible> {
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("W", ColumnType::Int),
        ("GRADE", ColumnType::Str),
        ("G2", ColumnType::Str),
        ("W2", ColumnType::Int),
    ]);
    let row =
        |id: i64, w: Value, grade: &str| vec![int(id), w.clone(), text(grade), text(grade), w];
    let mut range = record(
        "columns.W.verifications[0]",
        ConditionCode::VerificationRangeFailed,
        (5, 1, 5),
    );
    range.offending_rows = vec![identity(3, 4)];
    ExecutionAttempt {
        result: Ok(Execution {
            dataset: Dataset::from_rows(
                output,
                vec![
                    row(1, int(2), "HIGH"),
                    row(2, Value::Missing, "HIGH"),
                    row(4, int(3), "HIGH"),
                    row(5, Value::Missing, "HIGH"),
                    row(3, Value::Missing, "LOW"),
                ],
            ),
            verifications: vec![
                range,
                record(
                    "verifications[0]",
                    ConditionCode::VerificationRowCountFailed,
                    (1, 0, 5),
                ),
            ],
        }),
        handler_counts: vec![
            HandlerCount {
                spec_path: "columns.W.unconvertible".into(),
                handler: HandlerKind::Unconvertible,
                count: 3,
            },
            HandlerCount {
                spec_path: "columns.W2.unconvertible".into(),
                handler: HandlerKind::Unconvertible,
                count: 3,
            },
        ],
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 5,
            read: 5,
        }],
    }
}

/// Values, order, handler counts and check records of a partition-safe
/// plan do not depend on how its input is divided.
#[test]
fn a_partition_safe_plan_has_its_whole_table_result_over_any_partitions() {
    let (plan, input) = graded();
    assert_every_division(&plan, &input, &graded_attempt());
}

/// A group that spans partitions is merged into one candidate at its first
/// occurrence, with every record feeding it: G groups A, B and C in that
/// order, and each group's one distinct V is collected over all its records.
#[test]
fn a_group_that_spans_partitions_is_one_candidate_at_its_first_occurrence() {
    let source = schema(&[("G", ColumnType::Str), ("V", ColumnType::Int)]);
    let output = schema(&[("G", ColumnType::Str), ("N", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        output.clone(),
        vec![RowTemplate {
            mode: RowMode::Groups(vec![0]),
            assignments: vec![assign(0, Expression::Source(0))],
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
        )],
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(
        source,
        [
            ("A", 10),
            ("B", 20),
            ("A", 10),
            ("C", 30),
            ("B", 20),
            ("A", 10),
        ]
        .iter()
        .map(|&(g, v)| vec![text(g), int(v)])
        .collect(),
    );
    let expected = ExecutionAttempt {
        result: Ok(Execution {
            dataset: Dataset::from_rows(
                output,
                vec![
                    vec![text("A"), int(10)],
                    vec![text("B"), int(20)],
                    vec![text("C"), int(30)],
                ],
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
    assert_every_division(&plan, &input, &expected);
}

/// The key grain filters every record, derives its keys partition by
/// partition and merges equal keys across partitions: keys 1 and 2 each
/// span partitions, and the filtered record 3 builds nothing.
#[test]
fn equal_keys_in_different_partitions_build_one_key_grain_candidate() {
    let source = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int), ("N", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        output.clone(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: Some(predicate(
                "V > 0",
                "filter",
                vec![Bound {
                    name: "V".into(),
                    read: BoundRead::Source(1),
                }],
            )),
        }],
        vec![assign(
            1,
            Expression::Collect {
                column: 1,
                identifier: "V".into(),
                filter: None,
                selection: None,
            },
        )],
        vec![0],
        vec![verification(
            "verifications[0]",
            Check::RowCount {
                min: None,
                max: Some(10),
            },
        )],
    );
    let input = Rows::new(
        source,
        [(1, 5), (2, 6), (1, 5), (3, 0), (2, 6)]
            .iter()
            .map(|&(id, v)| vec![int(id), int(v)])
            .collect(),
    );
    let expected = ExecutionAttempt {
        result: Ok(Execution {
            dataset: Dataset::from_rows(output, vec![vec![int(1), int(5)], vec![int(2), int(6)]]),
            verifications: vec![record(
                "verifications[0]",
                ConditionCode::VerificationRowCountFailed,
                (1, 0, 2),
            )],
        }),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 2,
            read: 5,
        }],
    };
    assert_every_division(&plan, &input, &expected);
}

/// Output keys combine across partitions: the key 1 of the first and third
/// candidates is one duplicate group however the input is divided, named by
/// its first candidate.
#[test]
fn a_duplicate_key_split_across_partitions_fails_the_output_keys() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        source.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        Vec::new(),
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(source, vec![vec![int(1)], vec![int(2)], vec![int(1)]]);
    let mut duplicates = record("keys", ConditionCode::OutputDuplicateKeyFailed, (2, 1, 3));
    duplicates.offending_rows = vec![identity(0, 1)];
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::KeyFailures(vec![duplicates]))),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 3,
            read: 3,
        }],
    };
    assert_every_division(&plan, &input, &expected);
}

/// The first failure is the whole-table run's: the row stage, an earlier
/// unit, fails at the second record's X before the column phase reaches the
/// first record's Y, whichever partition each record is in.
#[test]
fn the_earliest_failing_step_fails_the_run_whichever_partition_holds_it() {
    let float = ColumnType::Float;
    let source = schema(&[("ID", ColumnType::Int), ("A", float), ("B", float)]);
    let plan = DatasetPlan::new(
        source.clone(),
        schema(&[
            ("ID", ColumnType::Int),
            ("X", ColumnType::Int),
            ("Y", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![assign(2, Expression::Source(2))],
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(
        source,
        vec![
            vec![int(1), Value::float(1.0), Value::float(2.5)],
            vec![int(2), Value::float(1.5), Value::float(1.0)],
        ],
    );
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Conversion {
            path: "columns.X".into(),
            output_row: 1,
            error: ConversionError {
                target: ColumnType::Int,
                value: crate::json::Value::from(&Value::float(1.5)),
                source_type: Some(ValueType::Float),
                reason: ConversionReason::IntegerRangeOrFraction,
            },
            identity: Some(identity(1, 2)),
        })),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    assert_every_division(&plan, &input, &expected);
}

/// One record template and a column-phase row number over V: a window,
/// which needs the whole table.
fn numbered() -> (DatasetPlan, Rows) {
    let source = schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        schema(&[
            ("ID", ColumnType::Int),
            ("V", ColumnType::Int),
            ("RN", ColumnType::Int),
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
            Expression::Window(Window {
                kind: WindowKind::RowNumber,
                group_by: vec![],
                order_by: vec![OrderTerm {
                    column: 1,
                    descending: false,
                    nulls_first: false,
                }],
                filter: None,
            }),
        )],
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(
        source,
        [(1, 20), (2, 10), (3, 30)]
            .iter()
            .map(|&(id, v)| vec![int(id), int(v)])
            .collect(),
    );
    (plan, input)
}

/// Without the fallback, a partitioned run of a plan with a window fails at
/// admission, naming the window's column binding, before any cell is read;
/// it keeps nothing, and its observer exits admission as failed. With the
/// fallback, the window runs over the whole input: V orders the records 2,
/// 1, 3, numbered 1 to 3.
#[test]
fn a_step_that_needs_the_whole_table_refuses_the_run_unless_it_falls_back() {
    let (plan, input) = numbered();
    let resolved = plan.resolve().unwrap();
    let parts = input.divided(&[1, 1, 1]);
    let refs: Vec<&dyn TableAccess<Error = Infallible>> = parts
        .iter()
        .map(|part| part as &dyn TableAccess<Error = Infallible>)
        .collect();
    let mut events = Vec::new();
    let mut observe = |event| events.push(event);
    let refused = resolved.execute_partitioned(
        &refs,
        &[],
        &mut NoFunctions::new(),
        Some(&mut observe),
        &|| false,
        false,
    );
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::PartitionGlobal {
            path: "columns.C2.derivation".into(),
        })),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    assert_eq!(refused, expected, "refused without the fallback");
    assert_eq!(
        parts.iter().map(|part| part.reads.get()).sum::<usize>(),
        0,
        "cells read before the refusal"
    );
    assert_eq!(
        events,
        [
            ExecutionEvent::Phase(ExecutionPhase::Admission),
            ExecutionEvent::Exit {
                phase: ExecutionPhase::Admission,
                outcome: Outcome::Failed,
            },
            ExecutionEvent::Phase(ExecutionPhase::Finished),
        ]
    );
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("V", ColumnType::Int),
        ("RN", ColumnType::Int),
    ]);
    let numbered = ExecutionAttempt {
        result: Ok(Execution {
            dataset: Dataset::from_rows(
                output,
                vec![
                    vec![int(1), int(20), int(2)],
                    vec![int(2), int(10), int(1)],
                    vec![int(3), int(30), int(3)],
                ],
            ),
            verifications: Vec::new(),
        }),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 3,
            read: 3,
        }],
    };
    for sizes in divisions(3) {
        assert_eq!(
            partitioned(&resolved, &input, &sizes, true),
            numbered,
            "fallback over partitions {sizes:?}"
        );
    }
}

/// A partition whose schema is not the driver's fails admission as a schema
/// mismatch, as a whole input would.
#[test]
fn a_partition_with_another_schema_fails_admission() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let other = Rows::new(schema(&[("ID", ColumnType::Int)]), vec![vec![int(9)]]);
    let parts: Vec<&dyn TableAccess<Error = Infallible>> = vec![&input, &other];
    let attempt =
        resolved.execute_partitioned(&parts, &[], &mut NoFunctions::new(), None, &|| false, false);
    assert!(
        matches!(
            attempt.result.as_ref().map_err(|error| &**error),
            Err(ExecutionError::SchemaMismatch)
        ),
        "{:?}",
        attempt.result
    );
}

/// A cancellation probe that counts its polls and answers true from poll
/// `from` on, or never.
struct Probe {
    polls: Cell<usize>,
    from: Option<usize>,
}
impl Probe {
    fn new(from: Option<usize>) -> Self {
        Self {
            polls: Cell::new(0),
            from,
        }
    }
    fn cancelled(&self) -> bool {
        let poll = self.polls.get() + 1;
        self.polls.set(poll);
        self.from.is_some_and(|from| poll >= from)
    }
}

/// A partitioned run polls at every checkpoint of its schedule and before
/// every partition of a divided unit after the first. [`graded`] over five
/// one-record partitions divides three units, its row stage and its two
/// column steps, so it polls four more times in each. Cancelled at any of
/// those polls, the run keeps nothing any partition built, and its observer
/// exits the phase it stopped in: admission at the first poll, output keys
/// and verification at the last two checkpoints, derivation in between.
#[test]
fn a_run_cancelled_between_partitions_keeps_nothing_and_names_its_phase() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let checkpoints = resolved
        .region_schedule(super::resolved::RegionId::DATASET)
        .iter()
        .filter(|info| info.checkpoint)
        .count();
    let total = checkpoints + 3 * 4;
    let parts = input.divided(&[1; 5]);
    let refs: Vec<&dyn TableAccess<Error = Infallible>> = parts
        .iter()
        .map(|part| part as &dyn TableAccess<Error = Infallible>)
        .collect();
    let probe = Probe::new(None);
    let attempt = resolved.execute_partitioned(
        &refs,
        &[],
        &mut NoFunctions::new(),
        None,
        &|| probe.cancelled(),
        false,
    );
    assert_eq!(attempt, graded_attempt(), "uncancelled");
    assert_eq!(probe.polls.get(), total, "polls of an uncancelled run");
    for from in 1..=total {
        let probe = Probe::new(Some(from));
        let mut events = Vec::new();
        let mut observe = |event| events.push(event);
        let attempt = resolved.execute_partitioned(
            &refs,
            &[],
            &mut NoFunctions::new(),
            Some(&mut observe),
            &|| probe.cancelled(),
            false,
        );
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: Vec::new(),
        };
        assert_eq!(attempt, expected, "cancelled at poll {from}");
        assert_eq!(probe.polls.get(), from, "polls once cancelled at {from}");
        let phase = match from {
            1 => ExecutionPhase::Admission,
            last if last == total => ExecutionPhase::Verification,
            keys if keys == total - 1 => ExecutionPhase::OutputKeys,
            _ => ExecutionPhase::Derivation,
        };
        assert_eq!(
            events[events.len() - 2..],
            [
                ExecutionEvent::Exit {
                    phase,
                    outcome: Outcome::Failed,
                },
                ExecutionEvent::Phase(ExecutionPhase::Finished),
            ],
            "the phase cancelled at poll {from}"
        );
    }
}

/// A function port that counts its calls and answers each with 0.
struct Counting {
    plan: crate::function_signature::ProjectInvocationPlan,
    calls: Cell<usize>,
}
impl crate::function_invocation::FunctionPort for Counting {
    type Error = Infallible;
    fn project_signature(
        &self,
        _slot: usize,
    ) -> Option<&crate::function_signature::ProjectInvocationPlan> {
        Some(&self.plan)
    }
    fn call(
        &mut self,
        _slot: usize,
        _arguments: &[crate::function_invocation::Argument<'_>],
    ) -> Result<Value, crate::function_invocation::HostError<Self::Error>> {
        self.calls.set(self.calls.get() + 1);
        Ok(int(0))
    }
}

/// A project function needs the whole table: without the fallback the run
/// is refused before any call, and with it the callbacks run as the whole
/// table runs them, once per record in record order.
#[test]
fn a_project_function_runs_only_under_the_fallback_and_then_once_per_record() {
    use super::{BoundProjectFunction, FunctionArgument, FunctionInput};
    use crate::function_signature::{
        Parameter, Presence, ProjectFunctionIdentity, ProjectInvocationPlan,
    };
    let signature = ProjectInvocationPlan::new(
        ProjectFunctionIdentity {
            name: "identity".into(),
            call: "identity".into(),
        },
        vec![Parameter {
            name: "value".into(),
            host_name: "value".into(),
            kind: ValueType::Int,
            accepts_missing: true,
            presence: Presence::Required,
        }],
        ColumnType::Int,
        true,
    )
    .unwrap();
    let source = schema(&[("ID", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int), ("F", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        output.clone(),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(
                    1,
                    Expression::ProjectFunction(
                        BoundProjectFunction::new_project(
                            0,
                            signature.clone(),
                            vec![FunctionArgument {
                                name: "value".into(),
                                input: FunctionInput::Read(BoundRead::Source(0)),
                            }],
                        )
                        .unwrap(),
                    ),
                ),
            ],
            filter: None,
        }],
        Vec::new(),
        vec![0],
        Vec::new(),
    );
    let resolved = plan.resolve().unwrap();
    let input = Rows::new(source, vec![vec![int(1)], vec![int(2)], vec![int(3)]]);
    let parts = input.divided(&[1, 2]);
    let refs: Vec<&dyn TableAccess<Error = Infallible>> = parts
        .iter()
        .map(|part| part as &dyn TableAccess<Error = Infallible>)
        .collect();
    for (fallback, calls, expected) in [
        (
            false,
            0,
            ExecutionAttempt {
                result: Err(Box::new(ExecutionError::PartitionGlobal {
                    path: "columns.C1.derivation".into(),
                })),
                handler_counts: Vec::new(),
                retained_verifications: Vec::new(),
                template_rows: Vec::new(),
            },
        ),
        (
            true,
            3,
            ExecutionAttempt {
                result: Ok(Execution {
                    dataset: Dataset::from_rows(
                        output.clone(),
                        vec![
                            vec![int(1), int(0)],
                            vec![int(2), int(0)],
                            vec![int(3), int(0)],
                        ],
                    ),
                    verifications: Vec::new(),
                }),
                handler_counts: Vec::new(),
                retained_verifications: Vec::new(),
                template_rows: vec![TemplateRows {
                    emitted: 3,
                    read: 3,
                }],
            },
        ),
    ] {
        let mut port = Counting {
            plan: signature.clone(),
            calls: Cell::new(0),
        };
        let attempt =
            resolved.execute_partitioned(&refs, &[], &mut port, None, &|| false, fallback);
        assert_eq!(attempt, expected, "fallback {fallback}");
        assert_eq!(port.calls.get(), calls, "calls with fallback {fallback}");
    }
}
