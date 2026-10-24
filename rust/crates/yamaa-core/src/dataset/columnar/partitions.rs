//! Partitioned execution (#3697): one run over the physical partitions of
//! its driver input, every partition-safe unit one partition at a time.
//!
//! The partitions are consecutive physical subsets of one immutable driver
//! snapshot, and every record keeps its coordinate in the whole input: the
//! run reads them as one [`Concatenated`] table, so a read, a failure and a
//! row identity name the record as the whole-table run names it. Admission
//! checks each partition of the driver input on its own, before any record
//! is read (#3792): the row and cell limits bound a partition, not the
//! input. Every other source is admitted whole.
//!
//! Key-bounded partitions (#3763) are cut only between two runs of records
//! whose partition key fields differ, so no partition splits a group of the
//! leading declared keys, and the cut is made once admission has passed, so
//! a run refused or cancelled at admission reads no record. A window or
//! aggregate that groups by every one of those keys, over rows that all come
//! from the driver input, then reads only rows of its own partition, and the
//! schedule classes it partition-safe
//! ([`crate::dataset::resolved::StepInfo::class`]). An input that is not
//! grouped by its partition keys is one partition, which keeps every first
//! failure in base-record order. A divided window step prepares its window
//! once over its whole scope, as the whole-table run does, then evaluates
//! each partition's rows as a unit, so the window's own failures come in
//! the whole-table run's order too.
//!
//! The schedule's units then run in order, as the whole-table run runs them.
//! A unit whose steps are all partition-safe ([`PartitionClass::Safe`] or
//! [`PartitionClass::CombineStep`]) and whose rows come from the driver
//! input runs over each partition's rows in turn, polling the cancellation
//! probe between partitions. Each partition runs as a unit of its own, in
//! batch and replayed row by row after a failure, so the first failure is
//! the whole-table run's: the earliest step that fails, then the earliest
//! row in base-record order.
//!
//! The state that crosses partitions is the combine state the partition
//! inventory names, and nothing else: the candidate rows the output is built
//! from, the handler counts (`handlers.rs`), which add up partition after
//! partition, the key groups (`group.rs`, `table_grouping::Key`), which merge
//! across partition boundaries, and the check records (`checks.rs`), which
//! each check builds at its checkpoint over every partition's candidates in
//! candidate order. A step that needs the whole table
//! ([`ResolvedPlan::whole_table_step`]) refuses the run at admission, unless
//! the whole-table fallback runs it over the whole input.
use super::frame::Frame;
use super::{group, Fail, Run, Table};
use crate::dataset::cut::{consecutive, PartitionCut};
use crate::dataset::execution::{ExecutionAttempt, ExecutionError, ExecutionEvent};
use crate::dataset::resolved::{PartitionClass, RegionId, ResolvedPlan, Step, TemplateId};
use crate::dataset::Partitioning;
use crate::function_invocation::FunctionPort;
use crate::limits::Limits;
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::table_grouping::GroupingError;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::ops::Range;

/// The partitions of a driver input, read as the one table they divide.
pub(super) struct Concatenated<'a, E> {
    parts: &'a [&'a dyn TableAccess<Error = E>],
    /// Where each partition starts in the whole input, then its size.
    starts: Vec<usize>,
    schema: &'a TableSchema,
}

impl<'a, E> Concatenated<'a, E> {
    /// The partitions `parts`, in order, of an input of `schema`. An input
    /// whose size overflows reads as the largest size, which admission
    /// refuses.
    pub(super) fn new(
        parts: &'a [&'a dyn TableAccess<Error = E>],
        schema: &'a TableSchema,
    ) -> Self {
        let mut starts = Vec::with_capacity(parts.len() + 1);
        let mut start = 0usize;
        starts.push(start);
        for part in parts {
            start = start.saturating_add(part.row_count());
            starts.push(start);
        }
        Self {
            parts,
            starts,
            schema,
        }
    }

    /// The partition holding record `row` of the whole input, and the
    /// record's position in it.
    fn locate(&self, row: usize) -> Option<(usize, usize)> {
        if row >= self.row_count() {
            return None;
        }
        let part = self.starts.partition_point(|&start| start <= row) - 1;
        Some((part, row - self.starts[part]))
    }
}

impl<E> TableAccess for Concatenated<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.starts[self.starts.len() - 1]
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        let (part, local) = self
            .locate(row)
            .ok_or(CellError::OutOfBounds { row, column })?;
        self.parts[part]
            .cell(local, column)
            .map_err(|error| match error {
                CellError::OutOfBounds { .. } => CellError::OutOfBounds { row, column },
                error => error,
            })
    }
}

/// Records `start..start + rows` of a table, as one partition of it.
struct RowRange<'t, T: ?Sized> {
    table: &'t T,
    start: usize,
    rows: usize,
}

impl<T: TableAccess + ?Sized> TableAccess for RowRange<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        self.table.schema()
    }
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<T::Error>> {
        if row >= self.rows {
            return Err(CellError::OutOfBounds { row, column });
        }
        self.table.cell(self.start + row, column)
    }
}

/// What a partitioned run knows of its partitions.
pub(super) struct Partitions {
    /// Where each partition starts in the whole input, then its size.
    starts: Vec<usize>,
    /// How many leading declared keys no partition splits a group of, which
    /// decides each step's class (#3763); zero for consecutive records.
    pub keys: usize,
    /// Whether a step that needs the whole table runs over it.
    pub fallback: bool,
    /// Whether some partition's schema is not the driver's.
    pub mismatched: bool,
    /// The partition fields and size of a key-bounded cut not yet made: the
    /// run makes it once admission has passed (#3763).
    cut: Option<(Vec<usize>, NonZeroUsize)>,
}

impl Partitions {
    fn new<E>(input: &Concatenated<'_, E>, keys: usize, fallback: bool) -> Self {
        Self {
            starts: input.starts.clone(),
            keys,
            fallback,
            mismatched: input.parts.iter().any(|part| part.schema() != input.schema),
            cut: None,
        }
    }

    /// The partition holding driver record `record`.
    fn of(&self, record: usize) -> usize {
        self.starts.partition_point(|&start| start <= record) - 1
    }

    /// The driver records of each partition, in order.
    pub(super) fn ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.starts.windows(2).map(|bounds| bounds[0]..bounds[1])
    }

    /// The records of the largest partition, at least one: the most rows one
    /// partition of the output holds when the run divides every step (D264).
    pub(super) fn largest(&self) -> std::num::NonZeroUsize {
        self.ranges()
            .map(|range| range.len())
            .max()
            .and_then(std::num::NonZeroUsize::new)
            .unwrap_or(std::num::NonZeroUsize::MIN)
    }
}

/// The key-bounded partitions of `table` (#3763): the runs of consecutive
/// records whose `fields` hold equal values, as `table_grouping::Key`
/// compares them, packed in order into partitions of at most `rows` records,
/// a longer run alone. None when a value of the fields recurs after its run
/// has ended, so the input is not grouped by them, or when a field cannot be
/// read: the input is then one partition. A reader that holds one partition
/// at a time makes the same cut (`dataset::cut`).
fn key_bounded<T: TableAccess + ?Sized>(
    table: &T,
    fields: &[usize],
    rows: NonZeroUsize,
) -> Option<Vec<Range<usize>>> {
    let mut cut = PartitionCut::new(fields.to_vec(), rows);
    cut.read(table);
    cut.finish()
}

/// `len` items split into consecutive runs of equal keys, in order. No
/// items make one empty run, so a unit over nothing still runs once.
fn runs(len: usize, key: impl Fn(usize) -> Option<usize>) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    let mut previous = None;
    for item in 0..len {
        let current = key(item);
        match runs.last_mut() {
            Some(run) if previous == Some(current) => run.end = item + 1,
            _ => runs.push(item..item + 1),
        }
        previous = Some(current);
    }
    if runs.is_empty() {
        runs.push(0..0);
    }
    runs
}

impl<E> Run<'_, '_, E> {
    /// Whether a partitioned run divides a unit of the dataset's `steps`:
    /// none of them needs the whole table under the run's partitions.
    pub(super) fn divides(&self, steps: impl Fn(Step) -> bool) -> bool {
        self.partitions.as_ref().is_some_and(|partitions| {
            self.plan
                .region_schedule(RegionId::DATASET)
                .iter()
                .filter(|info| steps(info.step))
                .all(|info| info.class(partitions.keys) != PartitionClass::Global)
        })
    }

    /// Whether `template` reads the driver input record by record, so its
    /// candidates divide with the input's partitions.
    pub(super) fn reads_partitions(&self, template: TemplateId) -> bool {
        self.plan.reads_driver(template)
    }

    /// The records of each partition of the driver input, which a
    /// partitioned run admits each on its own (#3792), or the whole input's
    /// records in a run over the whole input.
    pub(super) fn driver_admissions(&self) -> Vec<usize> {
        match &self.partitions {
            Some(partitions) => partitions.ranges().map(|range| range.len()).collect(),
            None => vec![self.primary.row_count()],
        }
    }

    /// The partition of a driver record, in a partitioned run.
    fn partition_of(&self, record: usize) -> Option<usize> {
        self.partitions
            .as_ref()
            .map(|partitions| partitions.of(record))
    }

    /// Items, each led by driver record `record(item)`, split into
    /// consecutive runs of one partition: the units a divided unit runs one
    /// after another. Undivided, they are one run.
    pub(super) fn partition_runs(
        &self,
        divided: bool,
        len: usize,
        record: impl Fn(usize) -> Option<usize>,
    ) -> Vec<Range<usize>> {
        if !divided {
            return std::iter::once(0..len).collect();
        }
        runs(len, |item| {
            record(item).and_then(|record| self.partition_of(record))
        })
    }

    /// Frame rows `rows`, in frame order, split into consecutive runs of
    /// one partition. A row of a template that does not read the driver
    /// input belongs to no partition, so its runs are its template's own.
    pub(super) fn row_runs(
        &self,
        divided: bool,
        frame: &Frame,
        rows: &[usize],
    ) -> Vec<Range<usize>> {
        if !divided {
            return std::iter::once(0..rows.len()).collect();
        }
        let templates: Vec<TemplateId> = self.plan.region_templates(RegionId::DATASET).collect();
        runs(rows.len(), |item| {
            let row = rows[item];
            let reads = frame
                .template(row)
                .is_none_or(|template| self.reads_partitions(templates[template]));
            reads
                .then(|| frame.members(row).first().copied())
                .flatten()
                .and_then(|record| self.partition_of(record))
        })
    }

    /// Cut a key-bounded run's driver input between its runs of equal
    /// partition keys (#3763), once admission has passed, so a run refused
    /// or cancelled at admission reads no record. An input that is not
    /// grouped by its partition fields, or whose fields cannot be read,
    /// stays one partition.
    pub(super) fn cut_by_keys(&mut self) {
        let primary = self.primary;
        let Some(partitions) = self.partitions.as_mut() else {
            return;
        };
        let Some((fields, rows)) = partitions.cut.take() else {
            return;
        };
        if let Some(bounds) = key_bounded(primary, &fields, rows) {
            partitions.starts = bounds
                .iter()
                .map(|bound| bound.start)
                .chain(std::iter::once(primary.row_count()))
                .collect();
        }
    }

    /// Poll the cancellation probe before every partition of a unit after
    /// its first (#3697).
    pub(super) fn next_partition(&self, run: usize) -> Result<(), Fail<E>> {
        if run > 0 {
            self.stop_if_cancelled()?;
        }
        Ok(())
    }

    /// A divided template's driver records grouped by `fields`, as
    /// `table_grouping::partition` groups the whole input: each partition's
    /// records grouped, then the partitions' key groups merged. Each
    /// partition is admitted on its own; the order of the reads and the
    /// first failing cell are the whole input's.
    pub(super) fn partition_groups<'t>(
        &self,
        table: Table<'t, E>,
        fields: &[usize],
    ) -> Result<Vec<Vec<usize>>, Fail<E>> {
        let grouping = |error| Box::new(ExecutionError::Grouping(error));
        if fields.is_empty() {
            return Err(grouping(GroupingError::EmptyColumns));
        }
        let width = table.schema().columns().len();
        for (position, &column) in fields.iter().enumerate() {
            if column >= width {
                return Err(grouping(GroupingError::InvalidColumn { position, column }));
            }
            if fields[..position].contains(&column) {
                return Err(grouping(GroupingError::DuplicateColumn {
                    position,
                    column,
                }));
            }
        }
        let partitions = self
            .partitions
            .as_ref()
            .expect("a divided template belongs to a partitioned run");
        // Each partition is admitted on its own (#3792).
        for range in partitions.ranges() {
            Limits::table(range.len(), width)
                .map_err(|error| grouping(GroupingError::Limit(error)))?;
        }
        let mut parts: Vec<Vec<Vec<usize>>> = Vec::new();
        // The key of each partition group's first record, which the merge
        // compares.
        let mut keys: BTreeMap<usize, Vec<ValueRef<'t>>> = BTreeMap::new();
        for range in partitions.ranges() {
            let mut values: Vec<ValueRef<'t>> = Vec::new();
            values
                .try_reserve_exact(range.len().saturating_mul(fields.len()))
                .map_err(|_| grouping(GroupingError::Allocation))?;
            for row in range.clone() {
                for &column in fields {
                    values.push(
                        table.get(row, column).map_err(|error| {
                            grouping(GroupingError::Cell { row, column, error })
                        })?,
                    );
                }
            }
            let key = |item: usize| &values[item * fields.len()..(item + 1) * fields.len()];
            let local = group::partition(range.len(), fields.len(), |item, position| {
                key(item)[position]
            })
            .map_err(|_| grouping(GroupingError::Allocation))?;
            let mut groups = Vec::new();
            for members in local.iter() {
                keys.insert(range.start + members[0], key(members[0]).to_vec());
                groups.push(members.iter().map(|&item| range.start + item).collect());
            }
            parts.push(groups);
        }
        let merged = group::merge(&parts, fields.len(), |record, position| {
            keys[&record][position]
        })
        .map_err(|_| grouping(GroupingError::Allocation))?;
        Ok(merged.into_vecs())
    }
}

impl ResolvedPlan {
    /// Execute the driver input divided into `partitions`, in order, as
    /// [`Self::execute`] executes it whole (#3697). Each record keeps its
    /// coordinate in the whole input, which is every partition in order.
    ///
    /// Admission checks each partition within the row and cell limits on
    /// its own (#3792), every other source whole, and every partition's
    /// schema, before any record is read. Then, without
    /// `whole_table_fallback`, a plan with a step that needs the whole table
    /// ([`Self::whole_table_step`]) fails at admission with
    /// `ExecutionError::PartitionGlobal`, naming that step's path. Otherwise
    /// every unit of the schedule whose steps are partition-safe and whose
    /// rows come from the driver input runs over one partition's rows at a
    /// time, and the probe is polled between partitions as well as at every
    /// checkpoint; a unit with a global step runs over the whole input.
    ///
    /// Partitions that never split a group of the leading `keys` declared
    /// keys, as a reader cuts them (#3830), divide a window or an aggregate
    /// that groups by every one of those keys; zero `keys` asks for
    /// partitions of consecutive records.
    ///
    /// A successful or failed attempt equals the whole-table attempt on the
    /// same input: the same dataset, failure, handler counts, verifications
    /// and template rows. A cancelled one keeps nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_partitioned<E>(
        &self,
        partitions: &[&dyn TableAccess<Error = E>],
        secondary: &[&dyn TableAccess<Error = E>],
        functions: &mut dyn FunctionPort<Error = E>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
        whole_table_fallback: bool,
        keys: usize,
    ) -> ExecutionAttempt<E> {
        let input = Concatenated::new(partitions, self.driver_schema());
        let divided = Partitions::new(&input, keys.min(self.keys().len()), whole_table_fallback);
        self.attempt(
            &input,
            Some(divided),
            secondary,
            functions,
            observer,
            cancelled,
        )
    }

    /// Execute `table` whole, or divided as `partitioning` asks: into
    /// consecutive partitions of its `rows` records (#3697), or into
    /// key-bounded partitions over its leading `keys` declared keys (#3763).
    ///
    /// Key-bounded partitions cut the driver input only between two runs of
    /// records whose partition key fields ([`Self::partition_fields`]) hold
    /// different values, and each holds as many whole runs as fit in `rows`
    /// records, a longer run alone. The cut is made once admission has
    /// passed, so a run refused or cancelled at admission reads no record.
    /// The input stays one partition when it is not grouped by those fields,
    /// when a field cannot be read, or when a template derives one of those
    /// keys from anything but a field. A specification that declares no key
    /// runs over consecutive partitions. Every partition-safe unit then runs
    /// one partition at a time, and a window or aggregate that groups by
    /// every partition key is partition-safe
    /// ([`crate::dataset::resolved::StepInfo::class`]).
    pub fn execute_divided<T: TableAccess + ?Sized>(
        &self,
        partitioning: Option<Partitioning>,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecutionAttempt<T::Error> {
        let Some(partitioning) = partitioning else {
            return self.execute(table, secondary, functions, observer, cancelled);
        };
        let rows = table.row_count();
        let size = partitioning.rows;
        let fallback = partitioning.whole_table_fallback;
        let keys = partitioning
            .leading_keys
            .map_or(0, |keys| keys.get().min(self.keys().len()));
        // Until admission has passed, key-bounded partitions are one.
        let bounds = if keys == 0 {
            consecutive(rows, size.get())
        } else {
            std::iter::once(0..rows).collect()
        };
        let ranges: Vec<RowRange<'_, T>> = bounds
            .into_iter()
            .map(|range| RowRange {
                table,
                start: range.start,
                rows: range.len(),
            })
            .collect();
        let parts: Vec<&dyn TableAccess<Error = T::Error>> = ranges
            .iter()
            .map(|range| range as &dyn TableAccess<Error = T::Error>)
            .collect();
        let input = Concatenated::new(&parts, self.driver_schema());
        let mut divided = Partitions::new(&input, keys, fallback);
        if keys > 0 {
            divided.cut = self.partition_fields(keys).map(|fields| (fields, size));
        }
        self.attempt(
            &input,
            Some(divided),
            secondary,
            functions,
            observer,
            cancelled,
        )
    }

    /// The schema of the dataset's driver input.
    fn driver_schema(&self) -> &TableSchema {
        let driver = self
            .region_sources(RegionId::DATASET)
            .next()
            .expect("a dataset has a driver source");
        &self.sources()[driver.index()].schema
    }
}

#[cfg(test)]
mod tests {
    use super::{key_bounded, runs};
    use crate::table::{CellError, Column, TableAccess, TableSchema, ValueRef};
    use crate::value::{ColumnType, Value};
    use std::convert::Infallible;
    use std::num::NonZeroUsize;
    use std::ops::Range;

    /// Records of two fields, K and J. The cut reads their cells and never
    /// their declared types, so a field may hold a value of any type.
    struct Records {
        schema: TableSchema,
        rows: Vec<[Value; 2]>,
    }
    impl Records {
        fn new(rows: Vec<[Value; 2]>) -> Self {
            let column = |name: &str, kind| Column {
                name: name.into(),
                kind,
            };
            Self {
                schema: TableSchema::new(vec![
                    column("K", ColumnType::Str),
                    column("J", ColumnType::Int),
                ])
                .unwrap(),
                rows,
            }
        }
    }
    impl TableAccess for Records {
        type Error = Infallible;
        fn schema(&self) -> &TableSchema {
            &self.schema
        }
        fn row_count(&self) -> usize {
            self.rows.len()
        }
        fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
            self.rows
                .get(row)
                .and_then(|values| values.get(column))
                .map(ValueRef::from)
                .ok_or(CellError::OutOfBounds { row, column })
        }
    }
    /// One partition of the records `range`.
    fn whole(range: Range<usize>) -> Vec<Range<usize>> {
        vec![range]
    }
    fn keyed(keys: &[&str]) -> Records {
        Records::new(
            keys.iter()
                .map(|&key| [Value::Str(key.into()), Value::Int(0)])
                .collect(),
        )
    }

    /// The executor's cut of key-bounded partitions (`key_bounded`), a seam
    /// no reader reaches: partitions hold whole runs of equal keys, as many
    /// as fit in the partition size, and a run longer than it alone. The
    /// runs of a, a, b, c, c, c, d are 0..2, 2..3, 3..6 and 6..7, so a size
    /// of 3 packs a with b, then c, then d, and a size of 4 packs a with b,
    /// then c with d.
    #[test]
    fn key_bounded_partitions_hold_whole_runs_as_many_as_fit() {
        let records = keyed(&["a", "a", "b", "c", "c", "c", "d"]);
        let cases: [(usize, Vec<Range<usize>>); 5] = [
            (1, vec![0..2, 2..3, 3..6, 6..7]),
            (2, vec![0..2, 2..3, 3..6, 6..7]),
            (3, vec![0..3, 3..6, 6..7]),
            (4, vec![0..3, 3..7]),
            (7, whole(0..7)),
        ];
        for (rows, expected) in cases {
            assert_eq!(
                key_bounded(&records, &[0], NonZeroUsize::new(rows).unwrap()),
                Some(expected),
                "partitions of at most {rows} records"
            );
        }
        assert_eq!(
            key_bounded(&keyed(&[]), &[0], NonZeroUsize::MIN),
            Some(whole(0..0)),
            "an empty input is one empty partition"
        );
    }

    /// The cut's runs (`key_bounded`), a seam no reader reaches: they are
    /// those of the key tuple, under `Key`'s equality, so missing equals
    /// missing, the float zero signs are equal, and an integer is not its
    /// decimal text. A value that recurs after its run has ended leaves the
    /// input ungrouped, which keeps no partition.
    #[test]
    fn key_bounded_partitions_follow_runs_of_the_whole_key_tuple() {
        let text = |value: &str| Value::Str(value.into());
        let cases = [
            (
                "a value of K recurs under another J",
                vec![
                    [text("a"), Value::Int(1)],
                    [text("b"), Value::Int(1)],
                    [text("a"), Value::Int(2)],
                ],
                vec![0, 1],
                Some(vec![0..1, 1..2, 2..3]),
            ),
            (
                "the same records by K alone",
                vec![
                    [text("a"), Value::Int(1)],
                    [text("b"), Value::Int(1)],
                    [text("a"), Value::Int(2)],
                ],
                vec![0],
                None,
            ),
            (
                "missing values are one run",
                vec![
                    [Value::Missing, Value::Int(1)],
                    [Value::Missing, Value::Int(2)],
                    [text("x"), Value::Int(3)],
                ],
                vec![0],
                Some(vec![0..2, 2..3]),
            ),
            (
                "a missing value recurs",
                vec![
                    [Value::Missing, Value::Int(1)],
                    [text("x"), Value::Int(2)],
                    [Value::Missing, Value::Int(3)],
                ],
                vec![0],
                None,
            ),
            (
                "an integer and its text are two keys",
                vec![[text("1"), Value::Int(1)], [Value::Int(1), Value::Int(1)]],
                vec![0],
                Some(vec![0..1, 1..2]),
            ),
            (
                "the float zero signs are one key",
                vec![
                    [Value::float(-0.0), Value::Int(1)],
                    [Value::float(0.0), Value::Int(2)],
                ],
                vec![0],
                Some(whole(0..2)),
            ),
        ];
        for (label, rows, fields, expected) in cases {
            assert_eq!(
                key_bounded(&Records::new(rows), &fields, NonZeroUsize::MIN),
                expected,
                "{label}"
            );
        }
    }

    /// Consecutive equal keys form one run, an empty input one empty run.
    #[test]
    fn runs_split_items_where_their_key_changes() {
        let keys = [Some(0), Some(0), None, Some(1), Some(1), Some(0)];
        assert_eq!(
            runs(keys.len(), |item| keys[item]),
            [0..2, 2..3, 3..5, 5..6]
        );
        assert_eq!(
            runs(0, |_| Some(0)),
            std::iter::once(0..0).collect::<Vec<_>>()
        );
    }
}
