//! Partitioned execution (#3697): a run over the partitions of its driver
//! input against independently expected whole-table results, the refusal of
//! a step that needs the whole table and its whole-table fallback, and the
//! end states of a cancelled or failed partitioned run; then the same over
//! key-bounded partitions (#3763), which keep subject-keyed windows and
//! aggregates inside one partition.
//!
//! No benchmark or host fixture reaches this seam, because the hosts run
//! whole tables: these hand-built plans call `ResolvedPlan::execute_divided`
//! and `ResolvedPlan::execute_partitioned` directly.
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
        denominator: None,
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

/// The secondary inputs of a run, which no partitioning divides.
type Secondary<'a> = [&'a dyn TableAccess<Error = Infallible>];

/// One partitioned attempt over `input` divided into `sizes`.
fn partitioned(
    plan: &super::ResolvedPlan,
    input: &Rows,
    secondary: &Secondary<'_>,
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
        secondary,
        &mut NoFunctions::new(),
        None,
        &|| false,
        (fallback, 0),
    )
}

/// `attempt` as a whole-table run states it. A partitioned run also records
/// the most rows one partition of its output holds (D264), which
/// [`the_output_divides_into_partitions_as_large_as_the_largest_driver_partition`]
/// proves, so a comparison with the authored whole-table attempt sets it
/// aside.
fn as_stated(mut attempt: ExecutionAttempt<Infallible>) -> ExecutionAttempt<Infallible> {
    if let Ok(execution) = &mut attempt.result {
        execution.partition_rows = None;
    }
    attempt
}

/// The plan runs whole and over every division of `input` with the one
/// authored attempt.
fn assert_every_division(
    plan: &DatasetPlan,
    input: &Rows,
    secondary: &Secondary<'_>,
    expected: &ExecutionAttempt<Infallible>,
) {
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(input, secondary, &mut NoFunctions::new(), None, &|| false);
    assert_eq!(&whole, expected, "the whole-table run");
    for sizes in divisions(input.rows.len()) {
        let attempt = as_stated(partitioned(&resolved, input, secondary, &sizes, false));
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
            partition_rows: None,
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
    assert_every_division(&plan, &input, &[], &graded_attempt());
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
            partition_rows: None,
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
    assert_every_division(&plan, &input, &[], &expected);
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
            partition_rows: None,
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
    assert_every_division(&plan, &input, &[], &expected);
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
    assert_every_division(&plan, &input, &[], &expected);
}

/// A conversion of 1.5 to an integer, which `conversion::integral` refuses.
fn not_integral() -> ConversionError {
    ConversionError {
        target: ColumnType::Int,
        value: crate::json::Value::from(&Value::float(1.5)),
        source_type: Some(ValueType::Float),
        reason: ConversionReason::IntegerRangeOrFraction,
    }
}

/// The first failure is the whole-table run's, with its handler counts and
/// retained check records: X's handler fires for the first record's 0.5 and
/// the warning range check on X finds the second record's 1, both before
/// the column phase; then the column step Y, the earlier unit, fails at the
/// second record's 1.5 before the column step Z reaches the first record's
/// 2.5, whichever partition each record is in.
#[test]
fn the_earliest_failing_step_fails_the_run_whichever_partition_holds_it() {
    let float = ColumnType::Float;
    let source = schema(&[
        ("ID", ColumnType::Int),
        ("A", float),
        ("B", float),
        ("C", float),
    ]);
    let plan = DatasetPlan::new(
        source.clone(),
        schema(&[
            ("ID", ColumnType::Int),
            ("X", ColumnType::Int),
            ("Y", ColumnType::Int),
            ("Z", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
            ],
            filter: None,
        }],
        vec![
            assign(2, Expression::Source(2)),
            assign(3, Expression::Source(3)),
        ],
        vec![0],
        Vec::new(),
    )
    .with_column_verifications(vec![ColumnVerifications {
        column: 1,
        checks: vec![verification(
            "columns.X.verifications[0]",
            Check::Range {
                min: Some(int(0)),
                max: Some(int(0)),
            },
        )],
    }])
    .with_warning_verifications(vec!["columns.X.verifications[0]".into()])
    .with_conversion_handlers(vec![ConversionHandler {
        assignment_path: "columns.C1.derivation".into(),
        handler: LiteralHandler {
            spec_path: "columns.X.unconvertible".into(),
            value: Value::Missing,
        },
    }]);
    let input = Rows::new(
        source,
        vec![
            vec![
                int(1),
                Value::float(0.5),
                Value::float(1.0),
                Value::float(2.5),
            ],
            vec![
                int(2),
                Value::float(1.0),
                Value::float(1.5),
                Value::float(3.0),
            ],
        ],
    );
    let mut range = record(
        "columns.X.verifications[0]",
        ConditionCode::VerificationRangeFailed,
        (2, 1, 2),
    );
    range.offending_rows = vec![identity(1, 2)];
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Conversion {
            path: "columns.Y".into(),
            output_row: 1,
            error: not_integral(),
            identity: Some(identity(1, 2)),
        })),
        handler_counts: vec![HandlerCount {
            spec_path: "columns.X.unconvertible".into(),
            handler: HandlerKind::Unconvertible,
            count: 1,
        }],
        retained_verifications: vec![range],
        template_rows: vec![TemplateRows {
            emitted: 2,
            read: 2,
        }],
    };
    assert_every_division(&plan, &input, &[], &expected);
}

/// A column bound per driver runs one binding after another, as the
/// whole-table run does (#2829), however its rows interleave in frame
/// order. Templates a (the primary input, filtered to ID < 2), b (SEC) and
/// c (the primary input) build the candidates a1, b3, c1, c2; W's binding
/// for a and c runs first and fails at c2's 2.5, before W's binding for b
/// would fail at b3's 3.5.
#[test]
fn a_column_bound_per_driver_fails_at_its_first_binding_whatever_the_frame_order() {
    use super::{ColumnBinding, ColumnScope, SecondarySource, SourceSchemas};
    let float = ColumnType::Float;
    let source = schema(&[("ID", ColumnType::Int), ("V", float)]);
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("T", ColumnType::Str),
        ("W", ColumnType::Int),
    ]);
    let template = |mode: RowMode, tag: &str, filter: Option<BoundPredicate>| RowTemplate {
        mode,
        assignments: vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Literal(text(tag))),
        ],
        filter,
    };
    let below = predicate(
        "ID < 2",
        "rows[0].filter",
        vec![Bound {
            name: "ID".into(),
            read: BoundRead::Source(0),
        }],
    );
    let per_driver = |templates: Vec<usize>| ColumnBinding {
        assignment: assign(2, Expression::Source(1)),
        templates,
        scope: ColumnScope::Template,
    };
    let plan = DatasetPlan::new_with_bindings(
        SourceSchemas {
            primary: source.clone(),
            secondary: vec![SecondarySource {
                name: "SEC".into(),
                schema: source.clone(),
            }],
        },
        Vec::new(),
        output,
        vec![
            template(RowMode::Records, "a", Some(below)),
            template(
                RowMode::Driver {
                    source: 1,
                    mode: Box::new(RowMode::Records),
                },
                "b",
                None,
            ),
            template(RowMode::Records, "c", None),
        ],
        vec![per_driver(vec![0, 2]), per_driver(vec![1])],
        vec![0, 1],
        Vec::new(),
    );
    let input = Rows::new(
        source.clone(),
        vec![
            vec![int(1), Value::float(1.0)],
            vec![int(2), Value::float(2.5)],
        ],
    );
    let alternate = Rows::new(source, vec![vec![int(3), Value::float(3.5)]]);
    let not_integral = ConversionError {
        value: crate::json::Value::from(&Value::float(2.5)),
        ..not_integral()
    };
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Conversion {
            path: "columns.W".into(),
            output_row: 3,
            error: not_integral,
            identity: Some(RowIdentity {
                position: 3,
                values: vec![int(2), text("c")],
            }),
        })),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![
            TemplateRows {
                emitted: 1,
                read: 2,
            },
            TemplateRows {
                emitted: 1,
                read: 1,
            },
            TemplateRows {
                emitted: 2,
                read: 2,
            },
        ],
    };
    assert_every_division(&plan, &input, &[&alternate], &expected);
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
        (false, 0),
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
            partition_rows: None,
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
            partitioned(&resolved, &input, &[], &sizes, true),
            numbered,
            "fallback over partitions {sizes:?}"
        );
    }
}

/// A partition whose schema is not the driver's fails admission as a schema
/// mismatch, as a whole input would, before any record is read: every
/// handler site of the plan is reported at zero.
#[test]
fn a_partition_with_another_schema_fails_admission() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let other = Rows::new(schema(&[("ID", ColumnType::Int)]), vec![vec![int(9)]]);
    let parts: Vec<&dyn TableAccess<Error = Infallible>> = vec![&input, &other];
    let attempt = resolved.execute_partitioned(
        &parts,
        &[],
        &mut NoFunctions::new(),
        None,
        &|| false,
        (false, 0),
    );
    let zero = |spec_path: &str| HandlerCount {
        spec_path: spec_path.into(),
        handler: HandlerKind::Unconvertible,
        count: 0,
    };
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::SchemaMismatch)),
        handler_counts: vec![
            zero("columns.W.unconvertible"),
            zero("columns.W2.unconvertible"),
        ],
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    assert_eq!(attempt, expected);
    assert_eq!(input.reads.get() + other.reads.get(), 0, "cells read");
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
        (false, 0),
    );
    assert_eq!(as_stated(attempt), graded_attempt(), "uncancelled");
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
            (false, 0),
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
                    partition_rows: None,
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
            resolved.execute_partitioned(&refs, &[], &mut port, None, &|| false, (fallback, 0));
        assert_eq!(attempt, expected, "fallback {fallback}");
        assert_eq!(port.calls.get(), calls, "calls with fallback {fallback}");
    }
}

use super::{
    AggregateBinding, AggregatePhase, AggregateRelation, AggregateSource, BoundAggregate,
    IntermediateKey, MatchRead, MatchValue, Partitioning,
};

/// Key-bounded partitions over the leading `keys` declared keys, of at most
/// `rows` records each unless one group holds more (#3763).
fn by_keys(keys: usize, rows: usize, fallback: bool) -> Option<Partitioning> {
    Some(Partitioning {
        rows: std::num::NonZeroUsize::new(rows).unwrap(),
        leading_keys: std::num::NonZeroUsize::new(keys),
        whole_table_fallback: fallback,
        whole_output: false,
    })
}

/// One run of `input` divided as `partitioning` asks.
fn divided_run(
    plan: &super::ResolvedPlan,
    input: &Rows,
    partitioning: Option<Partitioning>,
    cancelled: &dyn Fn() -> bool,
) -> ExecutionAttempt<Infallible> {
    plan.execute_divided(
        partitioning,
        input,
        &[],
        &mut NoFunctions::new(),
        None,
        cancelled,
    )
}

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
    input.reads.set(0);
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
            input.reads.get(),
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

/// Records of subjects in S and U, numbered SEQ within each, keyed [S, U,
/// SEQ]: N numbers each subject's records, then X converts A to an integer
/// through its `unconvertible` handler and Y converts B with none, all in
/// the column phase.
fn conversions(records: &[(&str, i64, f64, f64)]) -> (DatasetPlan, Rows) {
    let float = ColumnType::Float;
    let source = schema(&[
        ("S", ColumnType::Str),
        ("U", ColumnType::Str),
        ("SEQ", ColumnType::Int),
        ("A", float),
        ("B", float),
    ]);
    let plan = DatasetPlan::new(
        source.clone(),
        schema(&[
            ("S", ColumnType::Str),
            ("U", ColumnType::Str),
            ("SEQ", ColumnType::Int),
            ("N", ColumnType::Int),
            ("X", ColumnType::Int),
            ("Y", ColumnType::Int),
        ]),
        vec![RowTemplate {
            mode: RowMode::Records,
            assignments: vec![
                assign(0, Expression::Source(0)),
                assign(1, Expression::Source(1)),
                assign(2, Expression::Source(2)),
            ],
            filter: None,
        }],
        vec![
            assign(
                3,
                Expression::Window(Window {
                    kind: WindowKind::RowNumber,
                    group_by: vec![0, 1],
                    order_by: vec![OrderTerm {
                        column: 2,
                        descending: false,
                        nulls_first: false,
                    }],
                    filter: None,
                }),
            ),
            assign(4, Expression::Source(3)),
            assign(5, Expression::Source(4)),
        ],
        vec![0, 1, 2],
        Vec::new(),
    )
    .with_conversion_handlers(vec![ConversionHandler {
        assignment_path: "columns.C4.derivation".into(),
        handler: LiteralHandler {
            spec_path: "columns.X.unconvertible".into(),
            value: Value::Missing,
        },
    }]);
    let input = Rows::new(
        source,
        records
            .iter()
            .map(|&(u, seq, a, b)| {
                vec![
                    text("s"),
                    text(u),
                    int(seq),
                    Value::float(a),
                    Value::float(b),
                ]
            })
            .collect(),
    );
    (plan, input)
}

/// A driver input whose records are not grouped by the partition keys runs
/// as one partition (#3763, decided on the issue), and either way the run
/// fails as the whole-table run does, in base-record order. In the order a,
/// b, a the subject a recurs: the run divides nothing, so it polls only at
/// the whole-table run's checkpoints. In the order a, a, b, the subjects
/// are two partitions: the row stage and the column steps N and X each poll
/// once between them, and Y's first partition fails. Either way X's handler
/// fires for 0.5 and 1.5 before Y fails at the earliest record whose B is
/// not integral, the second.
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
            3,
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
/// subjects divides four units, its row stage, its window stage and its two
/// column steps, so it polls twice more in each. Cancelled at any of those
/// polls, the run keeps nothing, and its observer exits the phase it stopped
/// in. The cut follows admission, so a run cancelled at its admission
/// checkpoint has read no record.
#[test]
fn a_run_cancelled_between_key_bounded_partitions_keeps_nothing_and_names_its_phase() {
    let (plan, input) = visits();
    let resolved = plan.resolve().unwrap();
    let checkpoints = resolved
        .region_schedule(super::resolved::RegionId::DATASET)
        .iter()
        .filter(|info| info.checkpoint)
        .count();
    let total = checkpoints + 4 * 2;
    let probe = Probe::new(None);
    let attempt = divided_run(&resolved, &input, by_keys(2, 1, false), &|| {
        probe.cancelled()
    });
    assert_eq!(as_stated(attempt), visits_attempt(), "uncancelled");
    assert_eq!(probe.polls.get(), total, "polls of an uncancelled run");
    for from in 1..=total {
        let probe = Probe::new(Some(from));
        let mut events = Vec::new();
        let mut observe = |event| events.push(event);
        input.reads.set(0);
        let attempt = resolved.execute_divided(
            by_keys(2, 1, false),
            &input,
            &[],
            &mut NoFunctions::new(),
            Some(&mut observe),
            &|| probe.cancelled(),
        );
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: Vec::new(),
        };
        assert_eq!(attempt, expected, "cancelled at poll {from}");
        assert_eq!(probe.polls.get(), from, "polls once cancelled at {from}");
        if from == 1 {
            assert_eq!(
                input.reads.get(),
                0,
                "cells read when cancelled at admission"
            );
        }
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

/// A partitioned run that divides every step records the most rows one
/// partition of its output holds, as many as its largest driver partition
/// holds and at least one (D264); the adapters encode, admit and publish the
/// output in partitions of that many rows. Over [`graded`]'s five records,
/// consecutive partitions of 5, of 1 each, of 0, 5 and 0, of 2, 0 and 3,
/// and of 4 and 1 hold at most 5, 1, 5, 3 and 4. Over [`visits`]' subjects
/// a, b and c, of 2, 3 and 1 records, key-bounded partitions by S and U of
/// at most one record are a, b and c, so at most 3, and of at most four, a
/// then b with c, so 4. A whole-table run, a run whose fallback runs the
/// windows that need the whole table, and a run over either key-bounded
/// division told to admit its output whole, as a run over a SAS or ODM
/// driver input is, admit their output whole and record none.
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
            rows(partitioned(&resolved, &input, &[], sizes, false)),
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
    for records in [1, 4] {
        let whole_output = by_keys(2, records, false).map(|partitioning| Partitioning {
            whole_output: true,
            ..partitioning
        });
        assert_eq!(
            rows(divided_run(&resolved, &input, whole_output, &|| false)),
            None,
            "key-bounded partitions of at most {records} records, admitting the output whole"
        );
    }
}
