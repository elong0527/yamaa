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
//! must equal the one authored attempt. Every divided run also runs with
//! four partitions deriving at once (#4193), which must give the same
//! attempt as one at a time.
use super::execution::{
    CheckRecord, Dataset, Execution, ExecutionAttempt, ExecutionError, ExecutionEvent,
    ExecutionPhase, RowIdentity, TemplateRows,
};
use super::{
    Assignment, CaseBranch, Check, ColumnVerifications, ConversionHandler, DatasetPlan, Expression,
    OrderTerm, RowMode, RowTemplate, SharedDecode, Verification, Window, WindowKind,
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
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

/// A snapshot of typed rows that counts its cell reads, and that lends its
/// integer columns as typed storage once [`Rows::lending`] holds them, as
/// the adapters' column tables do (#4202).
struct Rows {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
    reads: AtomicUsize,
    /// Each lent column's payloads and presence bits, by column.
    lent: Vec<Option<(Vec<i64>, Vec<u64>)>>,
}
impl Rows {
    fn new(schema: TableSchema, rows: Vec<Vec<Value>>) -> Self {
        Self {
            schema,
            rows,
            reads: AtomicUsize::new(0),
            lent: Vec::new(),
        }
    }

    /// The same rows, each integer column lent as storage.
    fn lending(mut self) -> Self {
        let rows = &self.rows;
        self.lent = (0..self.schema.columns().len())
            .map(|column| {
                if self.schema.columns()[column].kind != ColumnType::Int {
                    return None;
                }
                let mut words = vec![0u64; rows.len().div_ceil(64)];
                let values = rows
                    .iter()
                    .enumerate()
                    .map(|(row, values)| match values[column] {
                        Value::Int(value) => {
                            words[row / 64] |= 1 << (row % 64);
                            value
                        }
                        _ => 0,
                    })
                    .collect();
                Some((values, words))
            })
            .collect();
        self
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
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
    fn column(&self, column: usize) -> Option<crate::table::ColumnRef<'_>> {
        let (values, words) = self.lent.get(column)?.as_ref()?;
        crate::table::ColumnRef::new(crate::table::ColumnValues::Int(values), Some(words))
    }
    fn shared(&self) -> Option<&(dyn TableAccess<Error = Infallible> + Sync)> {
        Some(self)
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

/// The partitions deriving at once that every divided run is checked under,
/// against one at a time (#4193).
const FOUR: NonZeroUsize = match NonZeroUsize::new(4) {
    Some(four) => four,
    None => unreachable!(),
};

/// `input` divided into consecutive partitions of `sizes` records, each
/// decoded as a copy of its records when the run asks for it (#3830). It
/// counts the partitions it decoded and the most that were alive at once;
/// the partition `undecodable` names does not decode.
struct Source<'i> {
    input: &'i Rows,
    ranges: Vec<std::ops::Range<usize>>,
    decoded: usize,
    live: Arc<AtomicUsize>,
    most: usize,
    undecodable: Option<usize>,
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
            live: Arc::default(),
            most: 0,
            undecodable: None,
        }
    }

    /// The same decode as threads share it (#4193), except that none of the
    /// partitions `undecodable` names decodes.
    fn shared(
        &self,
        undecodable: &'static [usize],
    ) -> impl Fn(usize, Range<usize>) -> Option<Decoded> + Sync + 'i {
        let (input, live) = (self.input, Arc::clone(&self.live));
        move |index, range| {
            if undecodable.contains(&index) {
                return None;
            }
            live.fetch_add(1, Ordering::Relaxed);
            Some(Decoded {
                rows: Rows::new(input.schema.clone(), input.rows[range].to_vec()),
                live: Arc::clone(&live),
            })
        }
    }
}

/// How a run decodes its partitions (#4193).
#[derive(Clone, Copy, Debug)]
enum Decoding {
    /// On the calling thread, each in turn.
    Calling,
    /// On the threads that derive them.
    Shared,
}

/// Every way a run decodes its partitions.
const DECODINGS: [Decoding; 2] = [Decoding::Calling, Decoding::Shared];

/// Run `run` over `source`, decoding its partitions as `decoding` says.
fn decoded_as<R>(
    source: &mut Source<'_>,
    decoding: Decoding,
    run: impl FnOnce((&mut Source<'_>, Option<&SharedDecode<'_, Decoded>>)) -> R,
) -> R {
    let shared = source.shared(&[]);
    match decoding {
        Decoding::Calling => run((source, None)),
        Decoding::Shared => run((source, Some(&shared))),
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
        if self.undecodable == Some(index) {
            return None;
        }
        let live = self.live.fetch_add(1, Ordering::Relaxed) + 1;
        self.most = self.most.max(live);
        let rows = self.input.rows[self.ranges[index].clone()].to_vec();
        Some(Decoded {
            rows: Rows::new(self.input.schema.clone(), rows),
            live: Arc::clone(&self.live),
        })
    }
}

/// One decoded partition, alive until the run drops it.
struct Decoded {
    rows: Rows,
    live: Arc<AtomicUsize>,
}
impl Drop for Decoded {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::Relaxed);
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
    fn shared(&self) -> Option<&(dyn TableAccess<Error = Infallible> + Sync)> {
        Some(self)
    }
    fn into_shared<'t>(
        self,
    ) -> Result<Box<dyn TableAccess<Error = Infallible> + Send + Sync + 't>, Self>
    where
        Self: 't,
    {
        Ok(Box::new(self))
    }
}

/// One partitioned attempt over `input` divided into `sizes`, the same with
/// one partition deriving at a time and with four at once, decoded on the
/// calling thread or on the threads that derive them.
fn partitioned(
    plan: &super::ResolvedPlan,
    input: &Rows,
    secondary: &Secondary<'_>,
    sizes: &[usize],
) -> ExecutionAttempt<Infallible> {
    let run = |threads, decoding| {
        decoded_as(&mut Source::new(input, sizes), decoding, |source| {
            plan.execute_partitioned(
                source,
                secondary,
                &mut NoFunctions::new(),
                None,
                &|| false,
                (0, threads),
            )
        })
    };
    let attempt = run(NonZeroUsize::MIN, Decoding::Calling);
    for decoding in DECODINGS {
        assert_eq!(
            run(FOUR, decoding),
            attempt,
            "partitions {sizes:?} on four threads, decoded {decoding:?}"
        );
    }
    attempt
}

/// Consecutive partitions of at most `rows` records (#3697), with or
/// without the whole-table fallback.
fn consecutive(rows: usize, fallback: bool) -> Option<Partitioning> {
    Some(Partitioning {
        rows: std::num::NonZeroUsize::new(rows).unwrap(),
        leading_keys: None,
        whole_table_fallback: fallback,
        whole_output: false,
        threads: NonZeroUsize::MIN,
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

/// Key-bounded partitions keep every key group whole (#4211), so the output
/// key check counts each partition's key-grain groups, numbered after the
/// earlier partitions', instead of grouping the output again. Over IDs
/// missing, missing, 1, 1, 2, grouped by ID, each missing record is its own
/// candidate and the 1s are one, so the candidates are missing, missing, 1,
/// 2: the two missing keys fail OUTPUT-2 and form the one repeated key of
/// OUTPUT-3, named by its first candidate, of three key groups, whole and
/// over key-bounded partitions of at most one record (the runs missing,
/// missing / 1, 1 / 2) or of the whole input, read cell by cell and from
/// lent storage.
#[test]
fn key_bounded_partitions_count_the_output_keys_from_each_key_grain() {
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
            vec![Value::Missing],
            vec![Value::Missing],
            vec![int(1)],
            vec![int(1)],
            vec![int(2)],
        ],
    );
    let missing = |position| RowIdentity {
        position,
        values: vec![Value::Missing],
    };
    let mut absent = record("keys[0]", ConditionCode::OutputMissingKeyFailed, (4, 2, 4));
    absent.offending_rows = vec![missing(0), missing(1)];
    let mut duplicates = record("keys", ConditionCode::OutputDuplicateKeyFailed, (3, 1, 4));
    duplicates.offending_rows = vec![missing(0)];
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
    // Read cell by cell, and from storage the port lends, which the key
    // grain's keys then read without completing their bindings.
    let ported = Rows::new(input.schema.clone(), input.rows.clone()).lending();
    for (port, input) in [("cells", &input), ("storage", &ported)] {
        let whole = resolved.execute(input, &[], &mut NoFunctions::new(), None, &|| false);
        assert_eq!(whole, expected, "{port}: the whole-table run");
        for rows in [1, 5] {
            let attempt = divided_run(&resolved, input, by_keys(1, rows, false), &|| false);
            assert_eq!(
                as_stated(attempt),
                expected,
                "{port}: key-bounded partitions of at most {rows} records"
            );
        }
    }
}

/// The output keys of two templates that bind P to literals (#4211): rows of
/// templates whose literals differ never share a key, so each template's
/// rows group on their own; rows of templates that bind the same literal
/// group together, whole or within each key-bounded partition. Over IDs 1,
/// 1, 2, each template builds (1, P), (1, P), (2, P), its rows after the
/// first template's. With P bound to A and B the key groups are (1, A),
/// (2, A), (1, B) and (2, B), and (1, A) and (1, B) repeat, named by rows 0
/// and 3. With P bound to A in both, the groups are (1, A), of rows 0, 1, 3
/// and 4, and (2, A), of rows 2 and 5: both repeat, named by rows 0 and 2.
/// Each attempt holds whole and over every division of the input, and over
/// key-bounded partitions of at most one record (the runs 1, 1 / 2) or of
/// the whole input.
#[test]
fn templates_binding_distinct_literals_group_their_output_keys_apart() {
    let source = schema(&[("ID", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int), ("P", ColumnType::Str)]);
    let template = |literal: &str| RowTemplate {
        mode: RowMode::Records,
        assignments: vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Literal(text(literal))),
        ],
        filter: None,
    };
    let input = Rows::new(
        source.clone(),
        vec![vec![int(1)], vec![int(1)], vec![int(2)]],
    );
    let named = |position, id, p: &str| RowIdentity {
        position,
        values: vec![int(id), text(p)],
    };
    let cases = [
        (["A", "B"], 4, vec![named(0, 1, "A"), named(3, 1, "B")]),
        (["A", "A"], 2, vec![named(0, 1, "A"), named(2, 2, "A")]),
    ];
    for (literals, groups, offending) in cases {
        let plan = DatasetPlan::new(
            source.clone(),
            output.clone(),
            literals.iter().map(|&literal| template(literal)).collect(),
            Vec::new(),
            vec![0, 1],
            Vec::new(),
        );
        let mut duplicates = record(
            "keys",
            ConditionCode::OutputDuplicateKeyFailed,
            (groups, 2, 6),
        );
        duplicates.offending_rows = offending;
        let rows = TemplateRows {
            emitted: 3,
            read: 3,
        };
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::KeyFailures(vec![duplicates]))),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: vec![rows, rows],
        };
        assert_every_division(&plan, &input, &[], &expected);
        let resolved = plan.resolve().unwrap();
        for rows in [1, 3] {
            let attempt = divided_run(&resolved, &input, by_keys(1, rows, false), &|| false);
            assert_eq!(
                as_stated(attempt),
                expected,
                "P bound to {literals:?}, key-bounded partitions of at most {rows} records"
            );
        }
    }
}

/// Two templates over (ID, V), each copying both and binding P to its own
/// literal, A then B, with the output keys ID and P and the output order
/// `order` (#4219).
fn ordered_templates(order: Vec<OrderTerm>) -> DatasetPlan {
    let template = |literal: &str| RowTemplate {
        mode: RowMode::Records,
        assignments: vec![
            assign(0, Expression::Source(0)),
            assign(1, Expression::Literal(text(literal))),
            assign(2, Expression::Source(1)),
        ],
        filter: None,
    };
    DatasetPlan::new(
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        schema(&[
            ("ID", ColumnType::Int),
            ("P", ColumnType::Str),
            ("V", ColumnType::Int),
        ]),
        vec![template("A"), template("B")],
        Vec::new(),
        vec![0, 1],
        Vec::new(),
    )
    .with_output_order(order)
}

/// The output order of a run that publishes its key-bounded partitions
/// (#4219) is the whole-table run's, whether each partition's rows, in its
/// own order, come before the next partition's or not. Over the records
/// (ID, V) (1, 30), (2, 10), (3, 40), (4, 20), each template builds a row
/// per record, so the construction order is A1, A2, A3, A4, B1, B2, B3, B4.
/// Derived by hand from OUTPUT-11 and VALUE-23, ties keeping that order:
///
/// - without order terms, that construction order, each template's rows of
///   every partition before the next template's;
/// - by ID, the cut field in its own direction: A1, B1, A2, B2, A3, B3, A4,
///   B4, which is each partition's rows in order, one partition after
///   another;
/// - by ID descending: A4, B4, A3, B3, A2, B2, A1, B1, which puts later
///   partitions first;
/// - by V, another column: A2, B2 (10), A4, B4 (20), A1, B1 (30), A3, B3
///   (40), which interleaves the partitions.
///
/// Each holds whole, over every division of the input, and over key-bounded
/// partitions of at most one, two or four records.
#[test]
fn a_published_output_is_ordered_as_the_whole_table_run_orders_it() {
    let input = Rows::new(
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        [(1, 30), (2, 10), (3, 40), (4, 20)]
            .iter()
            .map(|&(id, v)| vec![int(id), int(v)])
            .collect(),
    );
    let term = |column, descending| OrderTerm {
        column,
        descending,
        nulls_first: false,
    };
    let cases = [
        (
            "without order terms",
            Vec::new(),
            [
                (1, "A"),
                (2, "A"),
                (3, "A"),
                (4, "A"),
                (1, "B"),
                (2, "B"),
                (3, "B"),
                (4, "B"),
            ],
        ),
        (
            "by ID",
            vec![term(0, false)],
            [
                (1, "A"),
                (1, "B"),
                (2, "A"),
                (2, "B"),
                (3, "A"),
                (3, "B"),
                (4, "A"),
                (4, "B"),
            ],
        ),
        (
            "by ID descending",
            vec![term(0, true)],
            [
                (4, "A"),
                (4, "B"),
                (3, "A"),
                (3, "B"),
                (2, "A"),
                (2, "B"),
                (1, "A"),
                (1, "B"),
            ],
        ),
        (
            "by V",
            vec![term(2, false)],
            [
                (2, "A"),
                (2, "B"),
                (4, "A"),
                (4, "B"),
                (1, "A"),
                (1, "B"),
                (3, "A"),
                (3, "B"),
            ],
        ),
    ];
    let output = schema(&[
        ("ID", ColumnType::Int),
        ("P", ColumnType::Str),
        ("V", ColumnType::Int),
    ]);
    let v = |id: i64| [30, 10, 40, 20][id as usize - 1];
    let built = TemplateRows {
        emitted: 4,
        read: 4,
    };
    for (label, order, rows) in cases {
        let plan = ordered_templates(order);
        let resolved = plan.resolve().unwrap();
        let expected = ExecutionAttempt {
            result: Ok(Execution {
                partition_rows: None,
                dataset: Dataset::from_rows(
                    output.clone(),
                    rows.iter()
                        .map(|&(id, p)| vec![int(id), text(p), int(v(id))])
                        .collect(),
                ),
                verifications: Vec::new(),
            }),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: vec![built, built],
        };
        assert_every_division(&plan, &input, &[], &expected);
        for records in [1, 2, 4] {
            let attempt = divided_run(&resolved, &input, by_keys(1, records, false), &|| false);
            assert_eq!(
                as_stated(attempt),
                expected,
                "{label}, key-bounded partitions of at most {records} records"
            );
        }
    }
}

/// Missing output keys of a run that publishes its key-bounded partitions
/// (#4219) are named at their whole-output rows. Over the records with ID
/// 1, missing and 2, the two templates of [`ordered_templates`] build A1,
/// A-, A2, B1, B-, B2: by hand from OUTPUT-2, ID is missing at rows 1 and
/// 4, of the six, the rows (missing, A) and (missing, B), which repeat no
/// key, since P tells them apart. Over key-bounded partitions of one
/// record, the missing ID is a partition of its own, whose rows are the
/// second of each template's.
#[test]
fn missing_output_keys_of_a_published_partition_are_named_at_their_whole_output_rows() {
    let input = Rows::new(
        schema(&[("ID", ColumnType::Int), ("V", ColumnType::Int)]),
        vec![
            vec![int(1), int(5)],
            vec![Value::Missing, int(6)],
            vec![int(2), int(7)],
        ],
    );
    let plan = ordered_templates(Vec::new());
    let mut missing = record("keys[0]", ConditionCode::OutputMissingKeyFailed, (6, 2, 6));
    missing.offending_rows = vec![
        RowIdentity {
            position: 1,
            values: vec![Value::Missing, text("A")],
        },
        RowIdentity {
            position: 4,
            values: vec![Value::Missing, text("B")],
        },
    ];
    let built = TemplateRows {
        emitted: 3,
        read: 3,
    };
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::KeyFailures(vec![missing]))),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![built, built],
    };
    assert_every_division(&plan, &input, &[], &expected);
    let resolved = plan.resolve().unwrap();
    for records in [1, 3] {
        let attempt = divided_run(&resolved, &input, by_keys(1, records, false), &|| false);
        assert_eq!(
            as_stated(attempt),
            expected,
            "key-bounded partitions of at most {records} records"
        );
    }
}

/// A caller that passes leading keys with ranges no cut by them made
/// (#4211): a plan whose leading key copies no driver field has no
/// key-bounded cut, so its output keys group over every partition. Two
/// templates bind ID to the literal 1 over three records, so the six rows
/// are one key group, repeated, named by row 0, over ranges of one record
/// each passed with one leading key, as over the whole table.
#[test]
fn leading_keys_without_a_cut_group_the_output_keys_over_every_partition() {
    let source = schema(&[("V", ColumnType::Int)]);
    let output = schema(&[("ID", ColumnType::Int)]);
    let template = || RowTemplate {
        mode: RowMode::Records,
        assignments: vec![assign(0, Expression::Literal(int(1)))],
        filter: None,
    };
    let plan = DatasetPlan::new(
        source.clone(),
        output,
        vec![template(), template()],
        Vec::new(),
        vec![0],
        Vec::new(),
    );
    let input = Rows::new(source, vec![vec![int(5)], vec![int(6)], vec![int(7)]]);
    let mut duplicates = record("keys", ConditionCode::OutputDuplicateKeyFailed, (1, 1, 6));
    duplicates.offending_rows = vec![identity(0, 1)];
    let rows = TemplateRows {
        emitted: 3,
        read: 3,
    };
    let expected = ExecutionAttempt {
        result: Err(Box::new(ExecutionError::KeyFailures(vec![duplicates]))),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: vec![rows, rows],
    };
    let resolved = plan.resolve().unwrap();
    let whole = resolved.execute(&input, &[], &mut NoFunctions::new(), None, &|| false);
    assert_eq!(whole, expected, "the whole-table run");
    let attempt = resolved.execute_partitioned(
        (&mut Source::new(&input, &[1, 1, 1]), None),
        &[],
        &mut NoFunctions::new(),
        None,
        &|| false,
        (1, NonZeroUsize::MIN),
    );
    assert_eq!(
        as_stated(attempt),
        expected,
        "ranges of one record passed with one leading key"
    );
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
        (&mut source, None),
        &[],
        &mut NoFunctions::new(),
        Some(&mut observe),
        &|| false,
        (0, NonZeroUsize::MIN),
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
        (&mut source, None),
        &[],
        &mut NoFunctions::new(),
        None,
        &|| false,
        (0, NonZeroUsize::MIN),
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
            (&mut source, None),
            &[],
            &mut NoFunctions::new(),
            None,
            &|| false,
            (0, NonZeroUsize::MIN),
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
            (&mut Source::new(&input, &[1; 5]), None),
            &[],
            &mut NoFunctions::new(),
            Some(observe),
            &|| probe.cancelled(),
            (0, NonZeroUsize::MIN),
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

/// The events of `run`, with its attempt.
fn observed_run(
    run: impl FnOnce(&mut dyn FnMut(ExecutionEvent)) -> ExecutionAttempt<Infallible>,
) -> (ExecutionAttempt<Infallible>, Vec<ExecutionEvent>) {
    let mut events = Vec::new();
    let attempt = run(&mut |event| events.push(event));
    (attempt, events)
}

/// Four partitions deriving at once report what one at a time reports
/// (#4193): the same events in the same order, each partition the serial
/// run decodes reported once, in order, and the same attempt, whichever
/// thread decodes them. [`graded`] over five one-record partitions reports
/// partitions 1 to 5 and succeeds; [`conversions`] over a, a, then b, under
/// its two keys, reports both partitions and fails in the first, at a2's
/// 3.5, so the second partition's run stops where the first failed.
#[test]
fn a_run_on_four_threads_reports_the_events_and_attempt_of_a_run_on_one() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let run = |threads, decoding| {
        observed_run(|observe| {
            decoded_as(&mut Source::new(&input, &[1; 5]), decoding, |source| {
                resolved.execute_partitioned(
                    source,
                    &[],
                    &mut NoFunctions::new(),
                    Some(observe),
                    &|| false,
                    (0, threads),
                )
            })
        })
    };
    let (one, events) = run(NonZeroUsize::MIN, Decoding::Calling);
    let partitions: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ExecutionEvent::Partition { number, records } => Some((*number, *records)),
            _ => None,
        })
        .collect();
    assert_eq!(
        partitions,
        [(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)],
        "the partitions one thread reports"
    );
    assert_eq!(
        as_stated(run(NonZeroUsize::MIN, Decoding::Calling).0),
        graded_attempt(),
        "graded on one thread"
    );
    let one = (one, events);
    for decoding in DECODINGS {
        assert_eq!(
            run(FOUR, decoding),
            one,
            "graded on four threads, decoded {decoding:?}"
        );
    }
    let (plan, input) = conversions(&[("a", 1, 0.5, 1.0), ("a", 2, 1.5, 3.5), ("b", 1, 1.0, 2.5)]);
    let resolved = plan.resolve().unwrap();
    let run = |threads| {
        observed_run(|observe| {
            resolved.execute_divided(
                by_keys(2, 1, false).map(|partitioning| Partitioning {
                    threads,
                    ..partitioning
                }),
                &input,
                &[],
                &mut NoFunctions::new(),
                Some(observe),
                &|| false,
            )
        })
    };
    let one = run(NonZeroUsize::MIN);
    assert!(
        one.1.contains(&ExecutionEvent::Partition {
            number: 2,
            records: 1
        }),
        "one thread reports the second partition: {:?}",
        one.1
    );
    assert_eq!(run(FOUR), one, "conversions on four threads");
}

/// Four partitions deriving at once poll the probe on the calling thread
/// alone, which the probe's `Cell` holds it to (#4193): at admission, then
/// each time the calling thread hands out a partition or waits for one,
/// so at least once for each of [`graded`]'s five one-record partitions and
/// once more before the run combines them. Cancelled at any of its first
/// six polls, the run keeps nothing, polls no more, and its observer exits
/// admission at the first poll and derivation at the others.
/// So it is whichever thread decodes the partitions.
#[test]
fn a_run_on_four_threads_cancelled_in_derivation_keeps_nothing_and_names_its_phase() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    for (from, decoding) in (1..=6).flat_map(|from| DECODINGS.map(|decoding| (from, decoding))) {
        let probe = Probe::new(Some(from));
        let (attempt, events) = observed_run(|observe| {
            decoded_as(&mut Source::new(&input, &[1; 5]), decoding, |source| {
                resolved.execute_partitioned(
                    source,
                    &[],
                    &mut NoFunctions::new(),
                    Some(observe),
                    &|| probe.cancelled(),
                    (0, FOUR),
                )
            })
        });
        let phase = match from {
            1 => ExecutionPhase::Admission,
            _ => ExecutionPhase::Derivation,
        };
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: Vec::new(),
        };
        assert_eq!(
            (attempt, probe.polls.get(), &events[events.len() - 2..]),
            (
                expected,
                from,
                &[
                    ExecutionEvent::Exit {
                        phase,
                        outcome: Outcome::Failed,
                    },
                    ExecutionEvent::Phase(ExecutionPhase::Finished),
                ][..]
            ),
            "cancelled at poll {from}, decoded {decoding:?}: the attempt, the polls and the phase it exits"
        );
    }
}

/// A partition that does not decode on the thread deriving it is decoded
/// again on the calling thread in the serial run's turn (#4193), so a
/// decode that fails only on a thread changes nothing, and one that fails
/// on every thread ends the run where the serial run ends it. [`graded`]
/// over five one-record partitions: when the third does not decode on its
/// thread, the run succeeds with the authored attempt and reports
/// partitions 1 to 5; when it does not decode at all, the run keeps
/// nothing, as a cancelled run, and reports partitions 1 to 3, the third
/// being the one the serial run was decoding when it ended.
#[test]
fn a_partition_that_does_not_decode_on_its_thread_decodes_again_in_turn() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let run = |threads, (calling, shared): (Option<usize>, &'static [usize])| {
        let mut source = Source::new(&input, &[1; 5]);
        source.undecodable = calling;
        let decode = source.shared(shared);
        let shared = (threads != NonZeroUsize::MIN).then_some(&decode as &SharedDecode<'_, _>);
        let (attempt, events) = observed_run(|observe| {
            resolved.execute_partitioned(
                (&mut source, shared),
                &[],
                &mut NoFunctions::new(),
                Some(observe),
                &|| false,
                (0, threads),
            )
        });
        let partitions: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ExecutionEvent::Partition { number, records } => Some((*number, *records)),
                _ => None,
            })
            .collect();
        (attempt, partitions)
    };
    let cancelled = || ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Cancelled)),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    // Each case: whether the third partition decodes on the calling
    // thread, the threads, and the partitions that do not decode on theirs.
    let cases: [(&str, bool, NonZeroUsize, &'static [usize]); 4] = [
        ("on one thread", true, NonZeroUsize::MIN, &[]),
        ("only on its thread", true, FOUR, &[2]),
        ("anywhere, on one thread", false, NonZeroUsize::MIN, &[2]),
        ("anywhere, on four threads", false, FOUR, &[2, 3]),
    ];
    for (label, decodes, threads, shared) in cases {
        let (attempt, reported) = match decodes {
            true => (graded_attempt(), 5),
            false => (cancelled(), 3),
        };
        let partitions: Vec<_> = (1..=reported).map(|number| (number, 1)).collect();
        let (got, events) = run(threads, ((!decodes).then_some(2), shared));
        assert_eq!(
            (as_stated(got), events),
            (attempt, partitions),
            "the third partition does not decode {label}: the attempt and the partitions reported"
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
        threads: NonZeroUsize::MIN,
    })
}

/// One run of `input` divided as `partitioning` asks, polling `cancelled`;
/// unless it was cancelled, the same with four partitions deriving at once.
fn divided_run(
    plan: &super::ResolvedPlan,
    input: &Rows,
    partitioning: Option<Partitioning>,
    cancelled: &dyn Fn() -> bool,
) -> ExecutionAttempt<Infallible> {
    let run = |partitioning, cancelled: &dyn Fn() -> bool| {
        plan.execute_divided(
            partitioning,
            input,
            &[],
            &mut NoFunctions::new(),
            None,
            cancelled,
        )
    };
    let attempt = run(partitioning, cancelled);
    let four = partitioning.map(|partitioning| Partitioning {
        threads: FOUR,
        ..partitioning
    });
    if !matches!(&attempt.result, Err(error) if **error == ExecutionError::Cancelled) {
        assert_eq!(
            run(four, &|| false),
            attempt,
            "{partitioning:?} on four threads"
        );
    }
    attempt
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
