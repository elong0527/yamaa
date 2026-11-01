//! The outcome of executing a dataset plan: the dataset, the evidence an
//! attempt retains on success and failure, and the failures it reports.
//! Binding resolves the plan once (`dataset/resolved`), and each run executes
//! that resolved plan column at a time over typed buffers (`dataset/columnar`).

mod diagnostics;

use crate::{
    conversion::ConversionError,
    expression::Reducer,
    numeric_lifecycle::{HandlerCount, HandlerCountOverflow},
    table::{CellError, ColumnRef, TableAccess, TableSchema, ValueRef},
    table_grouping::GroupingError,
    table_reduction::TableReductionError,
    value::{Value, ValueType},
};
use std::convert::Infallible;

/// Contiguous stages of one attempt; clocks and measurement storage belong to adapters.
/// Derivation includes source reads, callbacks and per-value result conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionPhase {
    Admission,
    Derivation,
    OutputKeys,
    Verification,
    Finished,
}

/// Completion is emitted at the actual admitted checkpoint or returned result,
/// before Finished. A panic retains entry without inventing a returned outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionEvent {
    Phase(ExecutionPhase),
    Exit {
        phase: ExecutionPhase,
        outcome: crate::application_observer::Outcome,
    },
    Refusal {
        phase: ExecutionPhase,
        refusal: crate::application_observer::Refusal,
    },
    /// A partitioned run enters its partition `number`, from 1, which holds
    /// `records` driver records (#3762).
    Partition {
        number: usize,
        records: usize,
    },
    /// How the run divides its driver input, once admission has passed
    /// (#3794).
    Divided(Division),
}

/// How a run divided its driver input (#3794): whole, or into `count`
/// partitions, key-bounded over the leading `keys` declared keys or, with no
/// keys, consecutive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Division {
    Whole,
    Partitions { keys: usize, count: usize },
}

/// How a run divides its driver input. Without `leading_keys` (#3697), into
/// consecutive partitions of `rows` records each, the last holding the
/// rest. With `leading_keys` (#3763), into key-bounded partitions:
/// consecutive partitions that never split a group of the specification's
/// first `leading_keys` declared keys, each holding as many whole groups as
/// fit in `rows` records, a larger group alone; a window or aggregate
/// that groups by every one of those keys is then partition-safe. A
/// specification that declares no key runs over consecutive partitions.
/// Each partition runs through every derivation step before the next, one
/// decoded partition at a time (#3830), and the run combines only the state
/// the partition inventory marks combinable. A step that needs the whole
/// table refuses the run at admission, unless `whole_table_fallback` runs it
/// over the whole input instead.
///
/// A run whose plan is safe under its partitions and whose one input is CSV
/// or Parquet reads that input by partition (#3792), each partition decoded
/// and admitted on its own: a CSV input scanned in partitions of `rows`
/// records, a Parquet input in runs of whole row groups of at most `rows`
/// records, a larger row group being a partition of its own. Partitions cut
/// between runs of fields are cut as the scan reads the records (#3830).
///
/// A run that divides every step admits its output a partition at a time
/// too (D264), unless `whole_output` admits it whole, so that the row and
/// cell limits refuse it as they refuse the whole-table run's output. A run
/// whose driver input is SAS or ODM sets it, since those limits bound such
/// an input whole (`specification_run`).
///
/// Up to `threads` partitions derive at once (#4193), each on a thread of
/// its own, while the calling thread decodes the next partitions, polls the
/// cancellation probe and reports progress; the attempt, its events in
/// order, and every count are a run on one thread's. A plan that calls a
/// project function runs on the calling thread alone (FUNC-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partitioning {
    pub rows: std::num::NonZeroUsize,
    pub leading_keys: Option<std::num::NonZeroUsize>,
    pub whole_table_fallback: bool,
    pub whole_output: bool,
    pub threads: std::num::NonZeroUsize,
}

/// Fixed boundary failures remain distinct from normative language conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    SourceRows,
    SourceCells,
    SourceColumns,
    SourceCount,
    OutputRows,
    OutputCells,
    OutputColumns,
    /// One text value a verb builds (`Limits::TEXT_BYTES`).
    TextBytes,
}

/// Complete owned rows are exposed only after output identity and checks succeed.
///
/// The rows are held a column at a time in the executor's typed buffers
/// (#3870): every present value of a column in one closed type, text in one
/// arena, so the dataset takes about what its values do. A clone shares
/// them, so one output's encoders and observations, and the build of its
/// host view, read one copy.
///
/// A partitioned run that publishes a partition at a time (#4219) keeps
/// each partition's columns as its run built them, and the dataset reads
/// the output's rows as runs of one partition's rows, in output order: no
/// partition's rows are copied into another's.
#[derive(Clone)]
pub struct Dataset {
    inner: std::sync::Arc<Columns>,
}

struct Columns {
    schema: TableSchema,
    /// The columns of each stored part: the whole output's, or each
    /// partition's.
    parts: Vec<Vec<super::columnar::buffer::Values>>,
    /// The output's rows in order, as runs of one part's rows.
    runs: Vec<PartRun>,
    rows: usize,
    /// The run the last cell read was in, where the next read of a pass in
    /// row order looks first. A hint only: any value reads the same cells.
    hint: std::sync::atomic::AtomicUsize,
}

/// Rows `rows` of part `part`, the output's from row `at` on.
struct PartRun {
    part: usize,
    rows: std::ops::Range<usize>,
    at: usize,
}

impl Dataset {
    /// Every accepted row in template/group/source order and declared column
    /// order, each an owned copy.
    pub fn rows(&self) -> Vec<Vec<Value>> {
        let width = self.inner.schema.columns().len();
        (0..self.inner.rows)
            .map(|row| {
                (0..width)
                    .map(|column| {
                        let (values, index) = self.inner.locate(row, column);
                        values.value(index)
                    })
                    .collect()
            })
            .collect()
    }
    /// Complete owned rows in their final order.
    #[cfg(test)]
    pub(crate) fn from_rows(schema: TableSchema, rows: Vec<Vec<Value>>) -> Self {
        let width = schema.columns().len();
        let columns = (0..width)
            .map(|column| {
                let mut values = super::columnar::buffer::Values::missing(0);
                for row in &rows {
                    values.push(ValueRef::from(&row[column]));
                }
                values
            })
            .collect();
        Self::from_columns(schema, columns, rows.len())
    }
    /// The columns `columns`, each holding `rows` values in output order.
    pub(crate) fn from_columns(
        schema: TableSchema,
        columns: Vec<super::columnar::buffer::Values>,
        rows: usize,
    ) -> Self {
        debug_assert!(columns.iter().all(|column| column.len() == rows));
        Self::from_parts(schema, vec![columns], std::iter::once((0, 0..rows)))
    }
    /// The columns of each of `parts`, read as `runs`, in output order:
    /// each run rows of one part (#4219). Empty runs hold no row, and a run
    /// that continues the one before it in the same part joins it.
    pub(crate) fn from_parts(
        schema: TableSchema,
        parts: Vec<Vec<super::columnar::buffer::Values>>,
        runs: impl IntoIterator<Item = (usize, std::ops::Range<usize>)>,
    ) -> Self {
        let mut joined: Vec<PartRun> = Vec::new();
        let mut rows = 0;
        for (part, range) in runs {
            debug_assert!(parts[part].iter().all(|column| range.end <= column.len()));
            if range.is_empty() {
                continue;
            }
            match joined.last_mut() {
                Some(last) if last.part == part && last.rows.end == range.start => {
                    last.rows.end = range.end;
                }
                _ => joined.push(PartRun {
                    part,
                    rows: range.clone(),
                    at: rows,
                }),
            }
            rows += range.len();
        }
        Self {
            inner: std::sync::Arc::new(Columns {
                schema,
                parts,
                runs: joined,
                rows,
                hint: std::sync::atomic::AtomicUsize::new(0),
            }),
        }
    }
}

impl Columns {
    /// The run that holds output row `row`, which must be below `rows`.
    #[inline]
    fn run(&self, row: usize) -> &PartRun {
        use std::sync::atomic::Ordering::Relaxed;
        let hint = self.hint.load(Relaxed);
        if let Some(run) = self
            .runs
            .get(hint)
            .filter(|run| run.at <= row && row - run.at < run.rows.len())
        {
            return run;
        }
        let index = self
            .runs
            .partition_point(|run| run.at + run.rows.len() <= row);
        self.hint.store(index, Relaxed);
        &self.runs[index]
    }

    /// The buffer and position of output row `row` of column `column`,
    /// both in bounds.
    #[inline]
    fn locate(&self, row: usize, column: usize) -> (&super::columnar::buffer::Values, usize) {
        let run = self.run(row);
        (
            &self.parts[run.part][column],
            run.rows.start + (row - run.at),
        )
    }
}

impl PartialEq for Dataset {
    /// Two datasets are equal when their schemas and every cell are,
    /// however each holds its rows.
    fn eq(&self, other: &Self) -> bool {
        let (left, right) = (&*self.inner, &*other.inner);
        let width = left.schema.columns().len();
        left.schema == right.schema
            && left.rows == right.rows
            && (0..width).all(|column| {
                (0..left.rows).all(|row| {
                    let (values, index) = left.locate(row, column);
                    let (others, at) = right.locate(row, column);
                    values.get(index) == others.get(at)
                })
            })
    }
}

impl std::fmt::Debug for Dataset {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Dataset")
            .field("schema", &self.inner.schema)
            .field("rows", &self.rows())
            .finish()
    }
}

impl TableAccess for Dataset {
    type Error = Infallible;
    /// Retain declared schema even when every template produces zero rows.
    fn schema(&self) -> &TableSchema {
        &self.inner.schema
    }
    /// Count completed rows independently of missing cells.
    fn row_count(&self) -> usize {
        self.inner.rows
    }
    /// Borrow a checked owned cell; missing is never an out-of-bounds substitute.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        if row >= self.inner.rows || column >= self.inner.schema.columns().len() {
            return Err(CellError::OutOfBounds { row, column });
        }
        let (values, index) = self.inner.locate(row, column);
        Ok(values.get(index))
    }
    /// Lend a typed column's buffer whole, so that the output's encoders,
    /// host view and report read it without a call per cell (#4201): the
    /// rows of one part, which a dataset of several runs does not have.
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        let inner = &*self.inner;
        match inner.runs.as_slice() {
            [] => inner
                .parts
                .first()?
                .get(column)?
                .column()
                .filter(ColumnRef::is_empty),
            [run] => inner.parts[run.part]
                .get(column)?
                .column()?
                .slice(run.rows.clone()),
            _ => None,
        }
    }
    /// Lend each run's rows of a typed column, for a dataset of several
    /// runs (#4219): what [`Dataset::column`] lends a dataset of one.
    fn column_runs(&self, column: usize) -> Option<Vec<crate::table::ColumnRun<'_>>> {
        let inner = &*self.inner;
        if inner.runs.len() < 2 || column >= inner.schema.columns().len() {
            return None;
        }
        Some(
            inner
                .runs
                .iter()
                .map(|run| {
                    let view = inner.parts[run.part][column]
                        .column()
                        .and_then(|view| view.slice(run.rows.clone()));
                    (run.at..run.at + run.rows.len(), view)
                })
                .collect(),
        )
    }
    /// The dataset as threads share it: its columns are only read, so the
    /// output's encoder reads them on the run's threads (#4220).
    fn shared(&self) -> Option<&(dyn TableAccess<Error = Infallible> + Sync)> {
        Some(self)
    }
}

/// Output identity remains available after a failed artifact has been discarded.
#[derive(Clone, Debug, PartialEq)]
pub struct RowIdentity {
    pub position: usize,
    pub values: Vec<Value>,
}

/// Portable observations retain full identities; a host may format bounded samples.
#[derive(Clone, Debug, PartialEq)]
pub struct RowCountObservation {
    pub counts: Vec<usize>,
    pub denominators: Option<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckRecord {
    pub path: String,
    pub code: crate::diagnostic::ConditionCode,
    pub evaluated_count: usize,
    pub failed_count: usize,
    pub output_rows: usize,
    pub offending_rows: Vec<RowIdentity>,
    /// Full offending values survive discarded output.
    pub codelist: Option<CodelistObservation>,
    pub inventory: Option<Vec<Value>>,
    pub row_count: Option<RowCountObservation>,
    /// The value-level entry identifiers each offending row matches, aligned
    /// with `offending_rows`, when two entries of one column select a row.
    pub entries: Option<Vec<Vec<String>>>,
    /// The rows a `not_missing` under `max_fraction` checked, the share's
    /// denominator (CHECK-5, D209).
    pub denominator: Option<usize>,
}

impl CheckRecord {
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.code.definition()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodelistObservation {
    pub id: String,
    pub values: Vec<Value>,
}

#[derive(Debug, PartialEq)]
pub struct Execution {
    pub dataset: Dataset,
    pub verifications: Vec<CheckRecord>,
    /// The most rows one partition of the output holds (D264). A
    /// partitioned run that divided every step admitted its output in
    /// consecutive partitions of at most as many rows as its largest driver
    /// partition holds, and the output is encoded, admitted and published one
    /// such partition at a time; none when the run admitted its output whole.
    pub partition_rows: Option<std::num::NonZeroUsize>,
}

impl Execution {
    /// The output's partitions, in order (D264): consecutive ranges of at
    /// most [`Self::partition_rows`] rows, the last holding the rest, or one
    /// range of the whole output. An empty output is one empty partition, so
    /// its columns are still admitted.
    pub fn output_partitions(&self) -> Vec<std::ops::Range<usize>> {
        let rows = self.dataset.row_count();
        let size = self
            .partition_rows
            .map_or(rows.max(1), std::num::NonZeroUsize::get);
        (0..rows.max(1))
            .step_by(size)
            .map(|start| start..rows.min(start.saturating_add(size)))
            .collect()
    }
}

/// One completed row template: the rows it emitted and the driver records it
/// read, counted where its candidates become final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemplateRows {
    pub emitted: usize,
    pub read: usize,
}

/// An attempted run retains handler evidence even when no dataset can be accepted.
#[derive(Debug, PartialEq)]
pub struct ExecutionAttempt<E> {
    pub result: Result<Execution, Box<ExecutionError<E>>>,
    pub handler_counts: Vec<HandlerCount>,
    /// Completed checks preceding an unrelated failure, outside its diagnostic.
    pub retained_verifications: Vec<CheckRecord>,
    /// Row templates completed in construction order; a failed construction
    /// retains the templates that finished before it.
    pub template_rows: Vec<TemplateRows>,
}

#[derive(Debug, PartialEq)]
pub enum ExecutionError<E> {
    /// A pure scalar operation's already classified cause and original field path.
    ScalarDiagnostic(crate::diagnostic::Diagnostic),
    IntermediateVerifications(Vec<crate::diagnostic::Diagnostic>),
    IntermediateFilterTypes(Vec<crate::diagnostic::Diagnostic>),
    SemanticDiagnostic {
        diagnostic: crate::diagnostic::Diagnostic,
        identity: Option<RowIdentity>,
    },
    VerificationDiagnostic {
        diagnostic: crate::diagnostic::Diagnostic,
        records: Vec<CheckRecord>,
    },
    /// Callback registry does not match a declared signature; detected before table access.
    FunctionBinding {
        slot: usize,
    },
    /// Versionless package failure retains the original host payload and name/call.
    ProjectFunction {
        path: String,
        error: crate::function_invocation::InvocationFailure<
            E,
            crate::function_signature::ProjectFunctionIdentity,
        >,
        identity: Option<RowIdentity>,
    },
    Numeric {
        error: crate::expression::EvaluationError<Infallible>,
        identity: Option<RowIdentity>,
    },
    HandlerAccounting(HandlerCountOverflow),
    MultipleMatches {
        path: String,
        dataset: String,
        intermediate: String,
        match_count: usize,
        matched_key: Vec<(String, Value)>,
        identity: Option<RowIdentity>,
    },
    /// A lookup found no record under `require_match` (LOOKUP-24).
    UnmatchedKey {
        path: String,
        dataset: String,
        intermediate: String,
        matched_key: Vec<(String, Value)>,
        identity: Option<RowIdentity>,
    },
    SecondaryCell {
        path: String,
        source: usize,
        source_row: usize,
        error: CellError<E>,
    },
    BaselineAmbiguity {
        path: String,
        column: String,
        date: Value,
        match_count: usize,
        partition: Vec<(String, Value)>,
    },
    MultipleValues {
        path: String,
        identifier: String,
        value_count: usize,
        identity: Option<RowIdentity>,
    },
    VerificationDeclaration {
        path: String,
        code: crate::diagnostic::ConditionCode,
        reason: String,
        records: Vec<CheckRecord>,
    },
    VerificationPredicate {
        error: crate::expression::EvaluationError<CellError<Infallible>>,
        records: Vec<CheckRecord>,
    },
    Predicate {
        source_row: usize,
        error: crate::expression::EvaluationError<CellError<E>>,
    },
    Limit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
    },
    /// Zero names the primary table; positive indices name the secondary ports.
    SourceLimit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
        source_index: usize,
    },
    /// Resource facts with an authored owner, such as a verification pattern.
    ScopedLimit {
        resource: Resource,
        limit: usize,
        required: Option<usize>,
        spec_paths: Vec<String>,
    },
    SchemaMismatch,
    Capacity,
    Allocation,
    Grouping(GroupingError<E>),
    Cell {
        path: String,
        source_row: usize,
        error: CellError<E>,
    },
    ReductionType {
        path: String,
        expression: String,
        reducer: Reducer,
        source: String,
        actual: ValueType,
    },
    Reduction {
        path: String,
        error: TableReductionError<E>,
        identity: Option<RowIdentity>,
    },
    Conversion {
        path: String,
        output_row: usize,
        error: ConversionError,
        identity: Option<RowIdentity>,
    },
    KeyFailures(Vec<CheckRecord>),
    VerificationFailures(Vec<CheckRecord>),
    OutputGrouping(GroupingError<Infallible>),
    /// The run was cancelled (#2822): its probe answered true at a
    /// checkpoint of the schedule, between two partitions of a unit, or
    /// before a batch of project-function calls. The attempt keeps no output
    /// and no evidence.
    Cancelled,
    /// A partitioned run without the whole-table fallback was refused at
    /// admission (#3697): the step at `path`, the first in the schedule,
    /// needs the whole table, so the run cannot divide it.
    PartitionGlobal {
        path: String,
    },
}

impl<E> ExecutionError<E> {
    /// The refusal of a text value longer than `Limits::TEXT_BYTES`, owned by
    /// the operation at `path` that would build it.
    pub(crate) fn text(limit: crate::limits::LimitExceeded, path: &str) -> Self {
        Self::ScopedLimit {
            resource: Resource::TextBytes,
            limit: limit.limit,
            required: limit.actual,
            spec_paths: vec![path.into()],
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for ExecutionError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "dataset execution: {self:?}")
    }
}
impl<E: std::fmt::Debug> std::error::Error for ExecutionError<E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::columnar::buffer::Values;
    use crate::table::{Column, ColumnReader};
    use crate::value::ColumnType;

    /// A buffer of `values`, pushed one row at a time.
    fn pushed(values: &[ValueRef<'_>]) -> Values {
        let mut buffer = Values::missing(0);
        for &value in values {
            buffer.push(value);
        }
        buffer
    }

    // The seam is the dataset of a run that publishes its partitions
    // (#4219): its rows are runs of its parts' rows, which its cells, its
    // column reader and its equality read in output order, as a dataset of
    // one buffer per column reads them. Part 1's text is all missing, so it
    // lends no storage and its runs read through cells.
    #[test]
    fn a_dataset_of_parts_reads_its_runs_in_output_order() {
        use ValueRef::{Int, Missing, Str};
        let schema = TableSchema::new(vec![
            Column {
                name: "N".into(),
                kind: ColumnType::Int,
            },
            Column {
                name: "T".into(),
                kind: ColumnType::Str,
            },
        ])
        .unwrap();
        let parts = vec![
            vec![
                pushed(&[Int(1), Int(2), Int(3)]),
                pushed(&[Str("a"), Missing, Str("c")]),
            ],
            vec![pushed(&[Int(10), Int(20)]), pushed(&[Missing, Missing])],
        ];
        // Part 1's first row, part 0's first two, an empty run, part 1's
        // second row and part 0's third.
        let runs = [(1, 0..1), (0, 0..2), (1, 1..1), (1, 1..2), (0, 2..3)];
        let dataset = Dataset::from_parts(schema.clone(), parts, runs);
        let expected: [[ValueRef<'_>; 2]; 5] = [
            [Int(10), Missing],
            [Int(1), Str("a")],
            [Int(2), Missing],
            [Int(20), Missing],
            [Int(3), Str("c")],
        ];
        let whole = Dataset::from_rows(
            schema.clone(),
            expected
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|value| match *value {
                            Int(value) => Value::Int(value),
                            Str(value) => Value::Str(value.into()),
                            _ => Value::Missing,
                        })
                        .collect()
                })
                .collect(),
        );
        assert_eq!(
            dataset, whole,
            "the parts' runs against one buffer per column"
        );
        for column in 0..2 {
            let reader = ColumnReader::new(&dataset, column);
            let mut read = Vec::new();
            for start in 0..=5 {
                for end in start..=5 {
                    let mut values = Vec::new();
                    reader
                        .try_for_each(
                            start..end,
                            |value| {
                                values.push(value);
                                Ok::<_, ()>(())
                            },
                            |_| (),
                        )
                        .unwrap();
                    read.push(values);
                }
            }
            let mut wanted = Vec::new();
            for start in 0..=5 {
                for end in start..=5 {
                    wanted.push(expected[start..end].iter().map(|row| row[column]).collect());
                }
            }
            let rows: Vec<_> = (0..5).map(|row| reader.get(row).unwrap()).collect();
            let past = reader.try_for_each(3..6, |_| Ok(()), |error| error);
            assert_eq!(
                (read, rows, past, dataset.column(column).is_some()),
                (
                    wanted,
                    expected.iter().map(|row| row[column]).collect::<Vec<_>>(),
                    Err(CellError::OutOfBounds { row: 5, column }),
                    false,
                ),
                "column {column}: every range of rows, each row, a range past the end, and no whole view"
            );
        }
        let joined = Dataset::from_parts(
            schema,
            vec![vec![
                pushed(&[Int(1), Int(2)]),
                pushed(&[Str("a"), Str("b")]),
            ]],
            [(0, 0..1), (0, 1..2)],
        );
        assert_eq!(
            (
                joined.column(0).map(|view| view.len()),
                joined.column_runs(0).is_some()
            ),
            (Some(2), false),
            "consecutive runs of one part join, and lend the part's storage whole"
        );
    }
}
