//! Partitioned execution (#3697): one run over the physical partitions of
//! its driver input, every partition-safe unit one partition at a time.
//!
//! The partitions are consecutive physical subsets of one immutable driver
//! snapshot, and every record keeps its coordinate in the whole input: the
//! run reads them as one [`Concatenated`] table, so a read, a failure and a
//! row identity name the record as the whole-table run names it. Admission
//! checks the complete input once, before any partition is read.
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
use crate::dataset::execution::{ExecutionAttempt, ExecutionError, ExecutionEvent};
use crate::dataset::resolved::{PartitionClass, RegionId, ResolvedPlan, Step, TemplateId};
use crate::dataset::Partitioning;
use crate::function_invocation::FunctionPort;
use crate::limits::Limits;
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::table_grouping::GroupingError;
use std::collections::BTreeMap;
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
    /// Whether a step that needs the whole table runs over it.
    pub fallback: bool,
    /// Whether some partition's schema is not the driver's.
    pub mismatched: bool,
}

impl Partitions {
    fn new<E>(input: &Concatenated<'_, E>, fallback: bool) -> Self {
        Self {
            starts: input.starts.clone(),
            fallback,
            mismatched: input.parts.iter().any(|part| part.schema() != input.schema),
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
    /// none of them needs the whole table.
    pub(super) fn divides(&self, steps: impl Fn(Step) -> bool) -> bool {
        self.partitions.is_some()
            && self
                .plan
                .region_schedule(RegionId::DATASET)
                .iter()
                .filter(|info| steps(info.step))
                .all(|info| info.partition != PartitionClass::Global)
    }

    /// Whether `template` reads the driver input record by record, so its
    /// candidates divide with the input's partitions.
    pub(super) fn reads_partitions(&self, template: TemplateId) -> bool {
        let plan = self.plan;
        let driver = &plan.template(template).driver;
        driver.named.is_none()
            && plan.region_sources(RegionId::DATASET).next() == Some(driver.source)
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
            return vec![0..len];
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
            return vec![0..rows.len()];
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
    /// records grouped, then the partitions' key groups merged. Admission,
    /// the order of the reads and the first failing cell are the whole
    /// input's.
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
        Limits::table(table.row_count(), width)
            .map_err(|error| grouping(GroupingError::Limit(error)))?;
        let partitions = self
            .partitions
            .as_ref()
            .expect("a divided template belongs to a partitioned run");
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
    /// Admission checks the complete input once, and every partition's
    /// schema, before any record is read. Then, without
    /// `whole_table_fallback`, a plan with a step that needs the whole table
    /// ([`Self::whole_table_step`]) fails at admission with
    /// `ExecutionError::PartitionGlobal`, naming that step's path. Otherwise
    /// every unit of the schedule whose steps are partition-safe and whose
    /// rows come from the driver input runs over one partition's rows at a
    /// time, and the probe is polled between partitions as well as at every
    /// checkpoint; a unit with a global step runs over the whole input.
    ///
    /// A successful or failed attempt equals the whole-table attempt on the
    /// same input: the same dataset, failure, handler counts, verifications
    /// and template rows. A cancelled one keeps nothing.
    pub fn execute_partitioned<E>(
        &self,
        partitions: &[&dyn TableAccess<Error = E>],
        secondary: &[&dyn TableAccess<Error = E>],
        functions: &mut dyn FunctionPort<Error = E>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
        whole_table_fallback: bool,
    ) -> ExecutionAttempt<E> {
        let driver = self
            .region_sources(RegionId::DATASET)
            .next()
            .expect("a dataset has a driver source");
        let input = Concatenated::new(partitions, &self.sources()[driver.index()].schema);
        let divided = Partitions::new(&input, whole_table_fallback);
        self.attempt(
            &input,
            Some(divided),
            secondary,
            functions,
            observer,
            cancelled,
        )
    }

    /// Execute `table` whole, or divided into consecutive partitions as
    /// `partitioning` asks (#3697).
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
        let size = partitioning.rows.get();
        // An empty table is one empty partition, so admission still checks
        // its schema.
        let ranges: Vec<RowRange<'_, T>> = (0..rows.max(1))
            .step_by(size)
            .map(|start| RowRange {
                table,
                start,
                rows: size.min(rows.saturating_sub(start)),
            })
            .collect();
        let parts: Vec<&dyn TableAccess<Error = T::Error>> = ranges
            .iter()
            .map(|range| range as &dyn TableAccess<Error = T::Error>)
            .collect();
        self.execute_partitioned(
            &parts,
            secondary,
            functions,
            observer,
            cancelled,
            partitioning.whole_table_fallback,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::runs;

    /// Consecutive equal keys form one run, an empty input one empty run.
    #[test]
    fn runs_split_items_where_their_key_changes() {
        let keys = [Some(0), Some(0), None, Some(1), Some(1), Some(0)];
        assert_eq!(
            runs(keys.len(), |item| keys[item]),
            [0..2, 2..3, 3..5, 5..6]
        );
        assert_eq!(runs(0, |_| Some(0)), [0..0]);
    }
}
