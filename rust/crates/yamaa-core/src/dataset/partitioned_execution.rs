//! Partitioned execution (#3697): a run over the partitions of its driver
//! input against independently expected whole-table results, the refusal of
//! a step that needs the whole table and its whole-table fallback, and the
//! end states of a cancelled or failed partitioned run; then the same over
//! key-bounded partitions (#3763), which keep subject-keyed windows and
//! aggregates inside one partition; and a run that holds one decoded
//! partition at a time (#3830), whose partitions are cut between runs of the
//! fields its grouped templates and key grain read together.
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

/// `input` divided into consecutive partitions of `sizes` records, each
/// decoded as a copy of its records when the run asks for it (#3830). It
/// counts the partitions it decoded and the most that were alive at once.
struct Source<'i> {
    input: &'i Rows,
    ranges: Vec<std::ops::Range<usize>>,
    decoded: usize,
    live: std::rc::Rc<Cell<usize>>,
    most: usize,
}
impl<'i> Source<'i> {
    fn new(input: &'i Rows, sizes: &[usize]) -> Self {
        assert_eq!(sizes.iter().sum::<usize>(), input.rows.len());
        let mut start = 0;
        let ranges = sizes
            .iter()
            .map(|&size| {
                start += size;
                start - size..start
            })
            .collect();
        Self {
            input,
            ranges,
            decoded: 0,
            live: std::rc::Rc::default(),
            most: 0,
        }
    }
}
impl super::PartitionSource for Source<'_> {
    type Table = Decoded;
    fn schema(&self) -> &TableSchema {
        &self.input.schema
    }
    fn ranges(&self) -> &[std::ops::Range<usize>] {
        &self.ranges
    }
    fn decode(&mut self, index: usize) -> Option<Decoded> {
        self.decoded += 1;
        self.live.set(self.live.get() + 1);
        self.most = self.most.max(self.live.get());
        let rows = self.input.rows[self.ranges[index].clone()].to_vec();
        Some(Decoded {
            rows: Rows::new(self.input.schema.clone(), rows),
            live: std::rc::Rc::clone(&self.live),
        })
    }
}

/// One decoded partition, alive until the run drops it.
struct Decoded {
    rows: Rows,
    live: std::rc::Rc<Cell<usize>>,
}
impl Drop for Decoded {
    fn drop(&mut self) {
        self.live.set(self.live.get() - 1);
    }
}
impl TableAccess for Decoded {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        self.rows.schema()
    }
    fn row_count(&self) -> usize {
        self.rows.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.rows.cell(row, column)
    }
}

/// One partitioned attempt over `input` divided into `sizes`.
fn partitioned(
    plan: &super::ResolvedPlan,
    input: &Rows,
    secondary: &Secondary<'_>,
    sizes: &[usize],
) -> ExecutionAttempt<Infallible> {
    plan.execute_partitioned(
        &mut Source::new(input, sizes),
        secondary,
        &mut NoFunctions::new(),
        None,
        &|| false,
        0,
    )
}

/// Consecutive partitions of at most `rows` records (#3697), with or
/// without the whole-table fallback.
fn consecutive(rows: usize, fallback: bool) -> Option<Partitioning> {
    Some(Partitioning {
        rows: std::num::NonZeroUsize::new(rows).unwrap(),
        leading_keys: None,
        whole_table_fallback: fallback,
        whole_output: false,
    })
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
        let attempt = as_stated(partitioned(&resolved, input, secondary, &sizes));
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

/// `plan` over `input` divided into consecutive partitions of at most each
/// `rows`, cut between runs of the fields its cross-record reads share
/// (#3830): every run has `expected`, the whole-table attempt, and its
/// largest partition, which bounds its output's (D264), holds `largest`.
fn assert_cut(
    plan: &DatasetPlan,
    input: &Rows,
    expected: impl Fn() -> ExecutionAttempt<Infallible>,
    cases: &[(usize, usize)],
    label: &str,
) {
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(whole, expected(), "{label}: the whole-table run");
    for &(rows, largest) in cases {
        let attempt = divided_run(&resolved, input, consecutive(rows, false), &|| false);
        let partition = attempt
            .result
            .as_ref()
            .ok()
            .and_then(|execution| execution.partition_rows)
            .map(std::num::NonZeroUsize::get);
        assert_eq!(
            (as_stated(attempt), partition),
            (expected(), Some(largest)),
            "{label}: partitions of at most {rows} records"
        );
    }
}

/// A grouped template's records never meet across partitions (#3830): the
/// partitions are cut between runs of its group field G, each holding as
/// many whole runs as fit. Over A, A, B, C, C the runs are 0..2, 2..3 and
/// 3..5, so partitions of at most one or two records are those runs, the
/// largest two, of three are A with B, then C, and of five the whole input.
/// Over A, B, A, C, B, A, where A recurs after its run, the input is not
/// grouped by G and is one partition of six. Either way G groups A, B and C
/// in that order, and each group's one distinct V is collected over all its
/// records.
#[test]
fn a_grouped_template_divides_between_runs_of_its_group_field() {
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
    let value = |g: &str| match g {
        "A" => 10,
        "B" => 20,
        _ => 30,
    };
    let input = |groups: &[&str]| {
        Rows::new(
            source.clone(),
            groups
                .iter()
                .map(|&g| vec![text(g), int(value(g))])
                .collect(),
        )
    };
    let expected = |read: usize| ExecutionAttempt {
        result: Ok(Execution {
            partition_rows: None,
            dataset: Dataset::from_rows(
                output.clone(),
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
        template_rows: vec![TemplateRows { emitted: 3, read }],
    };
    assert_cut(
        &plan,
        &input(&["A", "A", "B", "C", "C"]),
        || expected(5),
        &[(1, 2), (2, 2), (3, 3), (5, 5)],
        "grouped by G",
    );
    assert_cut(
        &plan,
        &input(&["A", "B", "A", "C", "B", "A"]),
        || expected(6),
        &[(1, 6), (6, 6)],
        "not grouped by G",
    );
}

/// The key grain's keys never meet across partitions (#3830): the
/// partitions are cut between runs of ID, the field its one key copies.
/// The filter drops the record whose V is 0, and keys 1 and 2 remain. Over
/// IDs 1, 1, 2, 3 the runs are 0..2, 2..3 and 3..4, so partitions of at
/// most one record hold at most two, and of four the whole input; over
/// 1, 2, 1, 3, 2 the input is not grouped by ID and is one partition of
/// five.
#[test]
fn the_key_grain_divides_between_runs_of_the_field_its_key_copies() {
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
    let input = |records: &[(i64, i64)]| {
        Rows::new(
            source.clone(),
            records
                .iter()
                .map(|&(id, v)| vec![int(id), int(v)])
                .collect(),
        )
    };
    let expected = |read: usize| ExecutionAttempt {
        result: Ok(Execution {
            partition_rows: None,
            dataset: Dataset::from_rows(
                output.clone(),
                vec![vec![int(1), int(5)], vec![int(2), int(6)]],
            ),
            verifications: vec![record(
                "verifications[0]",
                ConditionCode::VerificationRowCountFailed,
                (1, 0, 2),
            )],
        }),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows { emitted: 2, read }],
    };
    assert_cut(
        &plan,
        &input(&[(1, 5), (1, 5), (2, 6), (3, 0)]),
        || expected(4),
        &[(1, 2), (4, 4)],
        "grouped by ID",
    );
    assert_cut(
        &plan,
        &input(&[(1, 5), (2, 6), (1, 5), (3, 0), (2, 6)]),
        || expected(5),
        &[(1, 5), (5, 5)],
        "not grouped by ID",
    );
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

/// The key grain's output keys are the groups it built (#4188): over IDs
/// 1, 1, missing, 2, missing, the two 1s are one candidate and each missing
/// record its own, so the candidates are 1, missing, 2, missing. Their two
/// missing keys fail OUTPUT-2 and form the one repeated key of OUTPUT-3,
/// named by its first candidate, of three key groups, whether the whole-table
/// run reuses the key grain's groups or a run cut between runs of ID, into
/// partitions of at most one record or of the whole input, groups the output
/// again.
#[test]
fn the_key_grain_output_keys_repeat_only_its_missing_keys() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let plan = DatasetPlan::new(
        source.clone(),
        source.clone(),
        vec![RowTemplate {
            mode: RowMode::Keys,
            assignments: vec![assign(0, Expression::Source(0))],
            filter: None,
        }],
        Vec::new(),
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(
        source,
        vec![
            vec![int(1)],
            vec![int(1)],
            vec![Value::Missing],
            vec![int(2)],
            vec![Value::Missing],
        ],
    );
    let missing = |position| RowIdentity {
        position,
        values: vec![Value::Missing],
    };
    let mut absent = record("keys[0]", ConditionCode::OutputMissingKeyFailed, (4, 2, 4));
    absent.offending_rows = vec![missing(1), missing(3)];
    let mut duplicates = record("keys", ConditionCode::OutputDuplicateKeyFailed, (3, 1, 4));
    duplicates.offending_rows = vec![missing(1)];
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::KeyFailures(vec![
            absent, duplicates,
        ]))),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![TemplateRows {
            emitted: 4,
            read: 5,
        }],
    };
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(whole, expected, "the whole-table run");
    for rows in [1, 5] {
        let attempt = divided_run(&resolved, &input, consecutive(rows, false), &|| false);
        assert_eq!(
            as_stated(attempt),
            expected,
            "partitions of at most {rows} records"
        );
    }
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
/// admission, naming the window's column binding, before any partition is
/// decoded; it keeps nothing, and its observer exits admission as failed.
/// With the fallback, the run reads the input whole and the window runs over
/// every record, whatever the partitions' size: V orders the records 2, 1,
/// 3, numbered 1 to 3.
#[test]
fn a_step_that_needs_the_whole_table_refuses_the_run_unless_it_falls_back() {
    let (plan, input) = numbered();
    let resolved = plan.resolve().unwrap();
    let mut source = Source::new(&input, &[1, 1, 1]);
    let mut events = Vec::new();
    let mut observe = |event| events.push(event);
    let refused = resolved.execute_partitioned(
        &mut source,
        &[],
        &mut NoFunctions::new(),
        Some(&mut observe),
        &|| false,
        0,
    );
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::PartitionGlobal {
            path: "columns.C2.derivation".into(),
        })),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    assert_eq!(
        (refused, source.decoded),
        (expected, 0),
        "refused without the fallback, and the partitions it decoded"
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
    for rows in 1..=3 {
        assert_eq!(
            divided_run(&resolved, &input, consecutive(rows, true), &|| false),
            numbered,
            "fallback over partitions of {rows}"
        );
    }
}

/// A source whose partitions' schema is not the driver's fails admission as
/// a schema mismatch, as a whole input would, before any partition is
/// decoded: every handler site of the plan is reported at zero.
#[test]
fn a_source_with_another_schema_fails_admission() {
    let (plan, _) = graded();
    let resolved = plan.resolve().unwrap();
    let other = Rows::new(schema(&[("ID", ColumnType::Int)]), vec![vec![int(9)]]);
    let mut source = Source::new(&other, &[1]);
    let attempt = resolved.execute_partitioned(
        &mut source,
        &[],
        &mut NoFunctions::new(),
        None,
        &|| false,
        0,
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
    assert_eq!((attempt, source.decoded), (expected, 0));
}

/// A partitioned run holds at most one decoded partition of its driver
/// input at a time (#3830): each partition is decoded when the run reaches
/// it and dropped before the next, so over every division of [`graded`]'s
/// records into partitions, at most one is alive at once, each is decoded
/// once, and the run has the whole-table attempt.
#[test]
fn a_partitioned_run_holds_one_decoded_partition_at_a_time() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    for sizes in divisions(input.rows.len()) {
        let mut source = Source::new(&input, &sizes);
        let attempt = resolved.execute_partitioned(
            &mut source,
            &[],
            &mut NoFunctions::new(),
            None,
            &|| false,
            0,
        );
        assert_eq!(
            (as_stated(attempt), source.most, source.decoded),
            (graded_attempt(), 1, sizes.len()),
            "partitions {sizes:?}: the attempt, the most alive at once, and those decoded"
        );
    }
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

/// The checkpoints of `plan`'s schedule a partitioned run polls (#3830):
/// those of each partition's derivation, between admission and the output
/// keys, and those from the output keys on, which it polls once.
fn polled(plan: &super::ResolvedPlan) -> (usize, usize) {
    let schedule = plan.region_schedule(super::resolved::RegionId::DATASET);
    let keys = schedule
        .iter()
        .position(|info| info.step == super::resolved::Step::OutputKeys)
        .unwrap();
    let count =
        |steps: &[super::resolved::StepInfo]| steps.iter().filter(|info| info.checkpoint).count();
    (count(&schedule[1..keys]), count(&schedule[keys..]))
}

/// A run polling `Probe` and reporting to an observer.
type Probed<'r> =
    dyn Fn(&Probe, &mut dyn FnMut(ExecutionEvent)) -> ExecutionAttempt<Infallible> + 'r;

/// Cancel `run`, whose `partitions` each poll `derivation` times, at each of
/// its `total` polls in turn: it keeps nothing any partition built, and its
/// observer exits the phase it stopped in, admission at the first poll, then
/// derivation in every partition's run, then output keys and verification.
fn assert_cancelled_at_every_poll(
    run: &Probed<'_>,
    (partitions, derivation, total): (usize, usize, usize),
) {
    for from in 1..=total {
        let probe = Probe::new(Some(from));
        let mut events = Vec::new();
        let attempt = run(&probe, &mut |event| events.push(event));
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: Vec::new(),
        };
        assert_eq!(attempt, expected, "cancelled at poll {from}");
        assert_eq!(probe.polls.get(), from, "polls once cancelled at {from}");
        let derived = 1 + partitions * derivation;
        let phase = match from {
            1 => ExecutionPhase::Admission,
            from if from <= derived => ExecutionPhase::Derivation,
            from if from == derived + 1 => ExecutionPhase::OutputKeys,
            _ => ExecutionPhase::Verification,
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

/// A partitioned run polls once at admission, at every checkpoint of each
/// partition's derivation, then at the checkpoints of its output keys and
/// checks. [`graded`] over five one-record partitions polls its five
/// derivation checkpoints in each, then its output keys and its row count.
/// Cancelled at any of those polls, the run keeps nothing any partition
/// built, and its observer exits the phase it stopped in.
#[test]
fn a_run_cancelled_between_partitions_keeps_nothing_and_names_its_phase() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let (derivation, tail) = polled(&resolved);
    assert_eq!((derivation, tail), (5, 2), "the checkpoints polled");
    let total = 1 + 5 * derivation + tail;
    let run = |probe: &Probe, observe: &mut dyn FnMut(ExecutionEvent)| {
        resolved.execute_partitioned(
            &mut Source::new(&input, &[1; 5]),
            &[],
            &mut NoFunctions::new(),
            Some(observe),
            &|| probe.cancelled(),
            0,
        )
    };
    let probe = Probe::new(None);
    assert_eq!(
        as_stated(run(&probe, &mut |_| {})),
        graded_attempt(),
        "uncancelled"
    );
    assert_eq!(probe.polls.get(), total, "polls of an uncancelled run");
    assert_cancelled_at_every_poll(&run, (5, derivation, total));
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
        let attempt = resolved.execute_divided(
            consecutive(2, fallback),
            &input,
            &[],
            &mut port,
            None,
            &|| false,
        );
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
    input.reads.set(0);
    run(&Probe::new(Some(1)), &mut |_| {});
    assert_eq!(
        input.reads.get(),
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
