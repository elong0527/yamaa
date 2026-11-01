//! Partitioned execution (#3697, #3830): one run over the partitions of its
//! driver input, holding one decoded partition at a time.
//!
//! Admission checks each partition of the driver input within the row and
//! cell limits on its own (#3792), every other input whole, before any
//! record is read. A plan with a step that needs the whole table
//! ([`ResolvedPlan::whole_table_step`]) is refused there; its whole-table
//! fallback reads the input whole instead ([`ResolvedPlan::execute_divided`]).
//! Every other plan reads together only records that one partition holds:
//! the confinement gate refuses a plan whose groups, key grain or ODM reads
//! no cut keeps whole, and the partitions are cut between runs of the
//! fields those reads share ([`ResolvedPlan::cut_fields`]), as a reader cuts
//! them (`dataset::cut`).
//!
//! Each partition is then decoded, run through every derivation step as a
//! whole-table run over its own records, and dropped once its run ends: its
//! row templates or its key grain, then its column phase. On one thread the
//! run decodes each partition after the one before has run; on more, up to
//! that many partitions run at once on threads of their own, and the
//! calling thread takes their runs in partition order (`concurrent`,
//! #4193). The run keeps each partition's candidate rows, and checks, keys
//! and orders the candidates of every partition once, after the last
//! partition.
//!
//! A run whose partitions no output key group spans, and whose plan
//! declares no column or dataset check, publishes each partition as it is
//! instead (#4219): each partition's run checks its own output keys as it
//! ends, orders its own rows, and keeps only its columns, which the dataset
//! reads partition by partition, in the whole-table run's order. The run
//! adds up the partitions' key findings, moved to the whole output's
//! positions; an order that the partitions' own orders do not give, one
//! partition's rows after another's, is the whole output's, over their
//! columns joined.
//!
//! The first failure, the handler counts, the check records and the template
//! rows are the whole-table run's. A partition's run passes the same
//! numbered marks in the same order whatever its records hold ([`Marks`]):
//! one for each part of a step whose failures the whole-table run orders
//! apart, which it finishes over every partition before the next. So the
//! whole-table run fails at the earliest mark any partition fails at, in the
//! earliest such partition, and once a partition fails each later one runs
//! only the marks before it. A partition's run records its handler counts at
//! each mark, so the run adds up what the whole-table run had counted when it
//! failed; it records where the column checks run, which the run evaluates
//! over every partition's candidates in their order, up to the failure; and a
//! failure's record and row positions, its partition's own, are moved to the
//! whole input's and the whole output's.
//!
//! A cancelled run keeps nothing any partition built.
use super::buffer::Values;
use super::checks;
use super::frame::Frame;
use super::handlers::{HandlerMark, Handlers};
use super::{reserved, row_completed, Fail, Found, Run};
use crate::dataset::cut::{consecutive, PartitionCut};
use crate::dataset::execution::{
    CheckRecord, Dataset, Execution, ExecutionAttempt, ExecutionError, ExecutionEvent,
    ExecutionPhase, RowIdentity, TemplateRows,
};
use crate::dataset::resolved::{
    FilterPlacement, Op, RegionId, ResolvedPlan, Stage, Step, TemplateId,
};
use crate::dataset::Partitioning;
use crate::function_invocation::FunctionPort;
use crate::table::{CellError, ColumnRef, TableAccess, TableSchema, ValueRef};
use crate::table_grouping::GroupingError;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod concurrent;

/// The partitions of a driver input that a partitioned run decodes in
/// order, on the calling thread (#3830), or on the threads that derive them
/// through a [`SharedDecode`] the source gives with it (#4193).
pub trait PartitionSource {
    /// A decoded partition, which a run derives on another thread when it
    /// moves there ([`TableAccess::into_shared`], #4193).
    type Table: TableAccess;
    /// The schema every partition has.
    fn schema(&self) -> &TableSchema;
    /// The records of each partition, in order, as positions in the whole
    /// input. No partition splits what the plan reads together: the
    /// partitions are cut between runs of [`ResolvedPlan::cut_fields`].
    fn ranges(&self) -> &[Range<usize>];
    /// Decode partition `index`: a table of its records, read by their
    /// positions in it. None ends the run as a cancelled one; the source
    /// holds why.
    fn decode(&mut self, index: usize) -> Option<Self::Table>;
}

/// A decode of a partition, by its index and its records, that the threads
/// deriving the partitions of a run share (#4193): what
/// [`PartitionSource::decode`] gives, or none when it does not decode. The
/// calling thread then decodes that partition again through
/// [`PartitionSource::decode`], in the serial run's turn, to find why.
pub type SharedDecode<'s, T> = dyn Fn(usize, Range<usize>) -> Option<T> + Sync + 's;

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
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        self.table
            .column(column)?
            .slice(self.start..self.start + self.rows)
    }
    /// The same records of the table as threads may share it, when they may.
    fn into_shared<'s>(
        self,
    ) -> Result<Box<dyn TableAccess<Error = T::Error> + Send + Sync + 's>, Self>
    where
        Self: 's,
    {
        match self.table.shared() {
            Some(table) => Ok(Box::new(RowRange {
                table,
                start: self.start,
                rows: self.rows,
            })),
            None => Err(self),
        }
    }
}

/// The driver input's schema with no record: what the run's own admission,
/// checks and ordering see of it, outside every partition.
struct Empty<'s, E> {
    schema: &'s TableSchema,
    error: std::marker::PhantomData<fn() -> E>,
}

impl<E> TableAccess for Empty<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        0
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        Err(CellError::OutOfBounds { row, column })
    }
}

/// What the positions of a partition's failure count (#3830): its
/// candidates, the key grain's probes, which are its records, or, while a
/// window of the template at this index is prepared, the window's scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Coordinates {
    Frame,
    Records,
    Window(usize),
}

/// Where one partition's run has reached (#3830): the numbered marks it
/// entered, the mark it stops at, and what it recorded on the way.
#[derive(Debug)]
pub(super) struct Marks {
    entered: usize,
    /// The mark an earlier partition failed at, which this one never enters.
    limit: usize,
    /// Whether this is the first partition's run, which alone builds the
    /// rows of a template that does not read the driver input and verifies
    /// the intermediates over the other inputs.
    pub first: bool,
    /// Whether the run stopped at its limit.
    limited: bool,
    coordinates: Coordinates,
    /// The handler counts as each mark was entered, where they changed.
    snapshots: Vec<(usize, HandlerMark)>,
    /// Where the column checks run: after how many marks, and the column
    /// just completed.
    checks: Vec<(usize, Option<usize>)>,
    /// After how many marks each template completed.
    completed: Vec<usize>,
    /// Raised once the run enters its second mark, past the first, the only
    /// one at which a partition's run fails before it reads a record: the
    /// first partition's verification of the intermediates (#4193).
    opened: Option<Arc<AtomicBool>>,
    /// The rows an output admitted whole would hold at each of its
    /// capacity checks, after how many marks: one per template and for the
    /// key grain, which every partition's run makes in the same order.
    capacity: Vec<(usize, usize)>,
}

impl Marks {
    fn new(limit: usize, first: bool) -> Self {
        Self {
            entered: 0,
            limit,
            first,
            limited: false,
            coordinates: Coordinates::Frame,
            snapshots: Vec::new(),
            checks: Vec::new(),
            completed: Vec::new(),
            capacity: Vec::new(),
            opened: None,
        }
    }

    /// The marks entered and the handler counts recorded so far.
    pub(super) fn position(&self) -> (usize, usize) {
        (self.entered, self.snapshots.len())
    }

    /// Return to `position`, before a unit that replays.
    pub(super) fn restore(&mut self, (entered, snapshots): (usize, usize)) {
        self.entered = entered;
        self.snapshots.truncate(snapshots);
    }

    /// An output admitted whole would hold `rows` rows here; a step's second
    /// check at one mark replaces its first.
    pub(super) fn capacity(&mut self, rows: usize) {
        if let Some(last) = self
            .capacity
            .last_mut()
            .filter(|(at, _)| *at == self.entered)
        {
            last.1 = rows;
        } else {
            self.capacity.push((self.entered, rows));
        }
    }

    /// The column checks run here, `column` just completed.
    pub(super) fn check(&mut self, column: Option<usize>) {
        self.checks.push((self.entered, column));
    }
}

impl<E> Run<'_, '_, E> {
    /// Enter the next mark of a partition's run (#3830), whose failures
    /// `coordinates` count. The run stops at its limit, the mark an earlier
    /// partition failed at; a whole-table run has no marks.
    pub(super) fn mark(&mut self, coordinates: Coordinates) -> Result<(), Fail<E>> {
        let Some(marks) = self.marks.as_mut() else {
            return Ok(());
        };
        if marks.entered >= marks.limit {
            marks.limited = true;
            return Err(Box::new(ExecutionError::Cancelled));
        }
        if marks
            .snapshots
            .last()
            .is_none_or(|(_, counts)| !self.handlers.holds(counts))
        {
            marks.snapshots.push((marks.entered, self.handlers.mark()));
        }
        marks.entered += 1;
        marks.coordinates = coordinates;
        if marks.entered == 2 {
            if let Some(opened) = marks.opened.take() {
                opened.store(true, Ordering::Release);
            }
        }
        Ok(())
    }

    /// A template has completed: a partition's run records where.
    pub(super) fn completed_template(&mut self) {
        if let Some(marks) = self.marks.as_mut() {
            marks.completed.push(marks.entered);
        }
    }

    /// Whether the run builds `template`'s rows. Those of a template that
    /// does not read the driver input belong to no partition, so a
    /// partitioned run builds them once, in its first partition's run.
    pub(super) fn builds(&self, template: TemplateId) -> bool {
        self.plan.reads_driver(template) || self.marks.as_ref().is_none_or(|marks| marks.first)
    }

    /// Pass the marks and checkpoints of a template whose rows the first
    /// partition's run builds, building none (#3830): its driver step, row
    /// stage, each window's preparation and evaluation, its post-window
    /// filter, then its SELF verification, as `process_template` enters them.
    pub(super) fn pass_template(
        &mut self,
        template: TemplateId,
        rows: usize,
        templates: &mut Vec<TemplateRows>,
    ) -> Result<(), Fail<E>> {
        let plan = self.plan;
        let windows = plan
            .row_bindings(template)
            .filter(|&binding| plan.binding(binding).stage == Stage::Window)
            .count();
        let postwindow = plan
            .template(template)
            .filter
            .is_some_and(|filter| filter.placement == FilterPlacement::Postwindow);
        self.mark(Coordinates::Frame)?;
        self.poll(Step::Driver(template))?;
        // The output holds the rows of the templates before it here.
        self.check_capacity(plan.template(template).region, rows, 0)?;
        for _ in 0..1 + 2 * windows + usize::from(postwindow) {
            self.mark(Coordinates::Frame)?;
        }
        templates.push(TemplateRows {
            emitted: 0,
            read: 0,
        });
        self.completed_template();
        self.mark(Coordinates::Frame)?;
        self.relations.publish_self(plan);
        self.poll(Step::VerifyIntermediates { self_rows: true })
    }

    /// Run a partition through every derivation step (#3830): its
    /// intermediates over the other inputs in the first partition's run, its
    /// row templates or key grain, then its column phase.
    fn derive_partition(
        &mut self,
        frame: &mut Frame,
        records: &mut Vec<CheckRecord>,
        templates: &mut Vec<TemplateRows>,
    ) -> Result<(), Fail<E>> {
        let inputs = self.inputs();
        self.mark(Coordinates::Frame)?;
        self.poll(Step::VerifyIntermediates { self_rows: false })?;
        self.derive(frame, &inputs, records, templates)
    }
}

/// How the partitions of a run that publishes them (#4219) group their own
/// output keys: each template's rows apart, when no two templates' rows
/// share a key ([`ResolvedPlan::templates_apart`]), or all of them at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Units {
    Templates,
    Partition,
}

/// How a published partition's rows are ordered (#4219).
#[derive(Debug)]
enum Ordered {
    /// In construction order: the output has no order terms.
    Construction,
    /// The partition's rows in the output's order, by their positions.
    Sorted(Vec<usize>),
    /// The order terms could not order its rows, which the whole output's
    /// order then refuses as well.
    Refused,
}

/// One partition's candidates as a run that publishes them keeps them
/// (#4219): a buffer per output column, in construction order, what its
/// output key check found, and its own order.
#[derive(Debug)]
struct Published {
    values: Vec<super::buffer::Values>,
    keys: Result<checks::KeyFindings, checks::KeyFault>,
    ordered: Ordered,
}

/// One partition's run (#3830).
struct Part<E> {
    /// The position of the partition's first record in the whole input.
    records: usize,
    /// The candidate rows it built, each template's after the one before;
    /// empty once they are published.
    frame: Frame,
    /// The rows of each template among its candidates, in template order.
    blocks: Vec<Range<usize>>,
    /// Its candidates as the run publishes them, once it ended without a
    /// failure in a run that publishes its partitions (#4219).
    published: Option<Published>,
    templates: Vec<TemplateRows>,
    /// Its check records: the intermediates' over the other inputs, which
    /// the first partition's run verifies.
    checked: Vec<CheckRecord>,
    marks: Marks,
    /// Its handler counts where it ended.
    counts: HandlerMark,
    /// The key grain's group of each of its candidates (#4211).
    key_groups: Option<Vec<usize>>,
    /// Its failure, at the last mark it entered, and what its positions
    /// count.
    failure: Option<(Fail<E>, Coordinates)>,
}

impl<E> Part<E> {
    /// A partition after a failure at the first mark, which runs nothing.
    fn passed(records: usize, (width, templates): (usize, usize), counts: HandlerMark) -> Self {
        Self {
            records,
            frame: Frame::new(width),
            blocks: vec![0..0; templates.max(1)],
            published: None,
            templates: Vec::new(),
            checked: Vec::new(),
            marks: Marks::new(0, false),
            counts,
            key_groups: None,
            failure: None,
        }
    }

    /// The mark it failed at.
    fn failed_at(&self) -> Option<usize> {
        self.failure.as_ref().map(|_| self.marks.entered - 1)
    }

    /// Whether this run, which stopped at mark `used`, is the run that stops
    /// at `limit`, no later mark (#4193): one that failed before `limit`,
    /// one that ended without entering it, or one that stopped there too.
    fn holds_under(&self, used: usize, limit: usize) -> bool {
        match self.failed_at() {
            Some(mark) => mark < limit,
            None if self.marks.limited => used == limit,
            None => self.marks.entered <= limit,
        }
    }

    /// Its handler counts once every mark before `position` is done.
    fn counts_before(&self, position: usize) -> &HandlerMark {
        if position >= self.marks.entered {
            return &self.counts;
        }
        let index = self
            .marks
            .snapshots
            .partition_point(|(mark, _)| *mark <= position);
        &self.marks.snapshots[index - 1].1
    }
}

/// What a partitioned run reads (#3830).
struct Input<'d, T> {
    schema: &'d TableSchema,
    /// The partitions admission checks, each on its own.
    admitted: Vec<Range<usize>>,
    /// The partitions the run executes, cut once admission has passed;
    /// none to execute those admitted.
    cut: &'d dyn Fn() -> Option<Vec<Range<usize>>>,
    /// Decode a partition, its index and its records.
    decode: &'d mut dyn FnMut(usize, Range<usize>) -> Option<T>,
    /// The leading declared keys no partition splits a group of.
    keys: usize,
    /// Whether the output is admitted whole, as a SAS or ODM driver
    /// input's is (D264).
    whole_output: bool,
    /// How many partitions may derive at once (#4193).
    threads: NonZeroUsize,
    /// The decode the threads that derive the partitions share, if any.
    shared: Option<&'d SharedDecode<'d, T>>,
}

/// Where one partition's run runs (#3830).
#[derive(Clone, Copy)]
struct Placement {
    /// The position of its first record in the whole input.
    records: usize,
    /// Whether it is the first partition.
    first: bool,
    /// The mark it stops at.
    limit: usize,
    /// The most rows one partition of the output holds (D264).
    output_partition: Option<NonZeroUsize>,
    /// How the partition groups its output keys, when the run publishes
    /// its partitions (#4219).
    publish: Option<Units>,
}

/// Where a partition's failure is in the whole run: the partition and the
/// mark.
type Failure = Option<(usize, usize)>;

/// The partitions a run derives and how it decodes them (#4193).
struct Work<'w, T> {
    ranges: &'w [Range<usize>],
    schema: &'w TableSchema,
    decode: &'w mut dyn FnMut(usize, Range<usize>) -> Option<T>,
    /// The same decode, which the threads that derive the partitions run
    /// when they may.
    shared: Option<&'w SharedDecode<'w, T>>,
}

/// Every partition's run and the first failure among them, or none when the
/// run was cancelled or a partition did not decode.
type Derived<E> = Option<(Vec<Part<E>>, Failure)>;

/// The other inputs of a run as threads share them (#4193).
type Shared<'s, E> = [&'s (dyn TableAccess<Error = E> + Sync)];

/// The other inputs `shared` names, as a run reads them.
fn plain<'s, E>(shared: &Shared<'s, E>) -> Vec<&'s dyn TableAccess<Error = E>> {
    shared
        .iter()
        .map(|&table| table as &dyn TableAccess<Error = E>)
        .collect()
}

impl ResolvedPlan {
    /// Execute the driver input a partition at a time (#3830): each
    /// partition of `source` is decoded when the run reaches it and dropped
    /// once its run ends, so the run holds as many decoded partitions of its
    /// driver input as derive at once, and one more decoded ahead (#4193).
    /// Its records keep their positions in the whole input, which every
    /// partition holds in order; `keys` names the leading declared keys no
    /// partition splits a group of, zero for consecutive records (#3763),
    /// and up to `threads` partitions derive at once. With `shared`, each
    /// partition decodes on the thread that derives it; without it, the
    /// calling thread decodes each in turn.
    ///
    /// Admission checks each partition within the row and cell limits on its
    /// own (#3792), every other source whole, and the partitions' schema,
    /// before any record is read. A plan with a step that needs the whole
    /// table ([`Self::whole_table_step`]) then fails with
    /// `ExecutionError::PartitionGlobal`, naming that step's path; its
    /// whole-table fallback reads the input whole ([`Self::execute_divided`]).
    /// The output is admitted a partition at a time (D264), each holding at
    /// most as many rows as the largest partition holds records.
    ///
    /// A successful or failed attempt equals the whole-table attempt on the
    /// same input: the same dataset, failure, handler counts, verifications
    /// and template rows. A cancelled one keeps nothing, and so does a run
    /// whose source could not decode a partition.
    pub fn execute_partitioned<P: PartitionSource>(
        &self,
        (source, shared): (&mut P, Option<&SharedDecode<'_, P::Table>>),
        secondary: &[&dyn TableAccess<Error = <P::Table as TableAccess>::Error>],
        functions: &mut dyn FunctionPort<Error = <P::Table as TableAccess>::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
        (keys, threads): (usize, NonZeroUsize),
    ) -> ExecutionAttempt<<P::Table as TableAccess>::Error> {
        let schema = source.schema().clone();
        let admitted = source.ranges().to_vec();
        let mut decode = |index, _| source.decode(index);
        let input = Input {
            schema: &schema,
            admitted,
            cut: &|| None,
            decode: &mut decode,
            keys: keys.min(self.keys().len()),
            whole_output: false,
            threads,
            shared,
        };
        self.run_divided(input, secondary, functions, observer, cancelled)
    }

    /// Execute `table` whole, or divided as `partitioning` asks: into
    /// consecutive partitions of its `rows` records (#3697), or into
    /// key-bounded partitions over its leading `keys` declared keys (#3763),
    /// up to `partitioning.threads` partitions at once
    /// ([`Self::execute_partitioned`]).
    ///
    /// Partitions are cut between runs of records whose [`Self::cut_fields`]
    /// hold equal values, each holding as many whole runs as fit in `rows`
    /// records, a longer run alone, once admission has passed, so a run
    /// refused or cancelled at admission reads no record: key-bounded
    /// partitions by the fields the partition keys copy, and consecutive
    /// partitions of a plan with a cross-record read by the fields those
    /// reads share. The input stays one partition when it is not grouped by
    /// those fields, when a field cannot be read, or when the driver input
    /// does not hold every partition key. A specification that declares no
    /// key runs over consecutive partitions.
    ///
    /// With `partitioning.whole_table_fallback`, a plan with a step that
    /// needs the whole table runs over the whole input, as [`Self::execute`]
    /// runs it; without it, such a plan is refused at admission. The output
    /// is admitted a partition at a time (D264), unless
    /// `partitioning.whole_output` admits it whole.
    pub fn execute_divided<T: TableAccess + ?Sized>(
        &self,
        partitioning: Option<Partitioning>,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecutionAttempt<T::Error> {
        let whole =
            |functions, observer| self.execute(table, secondary, functions, observer, cancelled);
        let Some(partitioning) = partitioning else {
            return whole(functions, observer);
        };
        let keys = partitioning
            .leading_keys
            .map_or(0, |keys| keys.get().min(self.keys().len()));
        if partitioning.whole_table_fallback && self.whole_table_step(keys).is_some() {
            return whole(functions, observer);
        }
        let rows = table.row_count();
        let size = partitioning.rows;
        let fields = self.cut_fields(keys);
        // Until admission has passed, partitions cut by fields are one.
        let admitted = match &fields {
            Some(fields) if fields.is_empty() => consecutive(rows, size.get()),
            _ => std::iter::once(0..rows).collect(),
        };
        let cut = || {
            let fields = fields.as_ref().filter(|fields| !fields.is_empty())?;
            let mut cut = PartitionCut::new(fields.clone(), size);
            cut.read(table);
            cut.finish()
        };
        let mut decode = |_, range: Range<usize>| {
            Some(RowRange {
                table,
                start: range.start,
                rows: range.len(),
            })
        };
        let input = Input {
            schema: table.schema(),
            admitted,
            cut: &cut,
            decode: &mut decode,
            keys,
            whole_output: partitioning.whole_output,
            threads: partitioning.threads,
            shared: None,
        };
        self.run_divided(input, secondary, functions, observer, cancelled)
    }

    /// One partitioned attempt over `input` (#3830).
    fn run_divided<T: TableAccess>(
        &self,
        input: Input<'_, T>,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecutionAttempt<T::Error> {
        let empty = Empty {
            schema: input.schema,
            error: std::marker::PhantomData,
        };
        self.observed(observer, |observer| {
            let mut outer = Run::new(self, (&empty, secondary), functions, cancelled);
            match self.divided(&mut outer, input, observer) {
                Ok(found) => found,
                Err(error) => Found {
                    result: Err(error),
                    handler_counts: outer.handlers.snapshot(self),
                    records: Vec::new(),
                    templates: Vec::new(),
                    cancelled: outer.stopped.get(),
                },
            }
        })
    }

    /// Admit the run, then run each partition and combine them (#3830).
    fn divided<T: TableAccess>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        input: Input<'_, T>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Found<T::Error>, Fail<T::Error>> {
        let admitted: Vec<usize> = input.admitted.iter().map(Range::len).collect();
        outer.admit(Some(&admitted))?;
        if let Some(step) = self.whole_table_step(input.keys) {
            return Err(Box::new(ExecutionError::PartitionGlobal {
                path: self.step_path(step).into_owned(),
            }));
        }
        let ranges = (input.cut)().unwrap_or(input.admitted);
        let largest = ranges
            .iter()
            .map(Range::len)
            .max()
            .and_then(NonZeroUsize::new)
            .unwrap_or(NonZeroUsize::MIN);
        outer.output_partition = (!input.whole_output).then_some(largest);
        outer.publish = self.publishing(input.keys, ranges.len());
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::Admission,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Divided(
            crate::dataset::Division::Partitions {
                keys: input.keys,
                count: ranges.len(),
            },
        ));
        observer(ExecutionEvent::Phase(ExecutionPhase::Derivation));
        let work = Work {
            ranges: &ranges,
            schema: input.schema,
            decode: input.decode,
            shared: input.shared,
        };
        let threads = input.threads.get().min(ranges.len());
        // A plan that calls the host runs on the calling thread (FUNC-8);
        // none is partitioned today, since such a step needs the whole table.
        // So does a run over another input that threads may not share.
        let others = outer.secondary;
        let shared: Option<Vec<_>> = others.iter().map(|&table| table.shared()).collect();
        let derived = match shared {
            Some(shared) if threads > 1 && self.functions().is_empty() => {
                self.derive_concurrently(outer, work, (&shared, threads), observer)?
            }
            _ => self.derive_serially(outer, work, observer)?,
        };
        match derived {
            Some((parts, failure)) => {
                let whole = self.keeps_key_groups(input.keys, ranges.len());
                self.combine(outer, (parts, whole), failure, observer)
            }
            None => Ok(Found::cancelled()),
        }
    }

    /// Run each partition in turn on the calling thread (#3830): decoded
    /// when the run reaches it and dropped before the next is decoded.
    fn derive_serially<T: TableAccess>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        work: Work<'_, T>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Derived<T::Error>, Fail<T::Error>> {
        let mut parts = Vec::with_capacity(work.ranges.len());
        let mut failure: Failure = None;
        for (index, range) in work.ranges.iter().enumerate() {
            let limit = failure.map_or(usize::MAX, |(_, mark)| mark);
            let part = match limit {
                0 => self.passed(outer, range.start),
                _ => {
                    observer(ExecutionEvent::Partition {
                        number: index + 1,
                        records: range.len(),
                    });
                    match self.decode_and_run(
                        outer,
                        &mut *work.decode,
                        (work.schema, work.ranges),
                        (index, limit),
                    )? {
                        Some(part) => part,
                        None => return Ok(None),
                    }
                }
            };
            if let Some(mark) = part.failed_at() {
                failure = Some((index, mark));
            }
            parts.push(part);
        }
        Ok(Some((parts, failure)))
    }

    /// A partition after a failure at the first mark, which the run neither
    /// decodes nor reports.
    fn passed<E, F>(&self, outer: &Run<'_, '_, F>, records: usize) -> Part<E> {
        let width = self.region(RegionId::DATASET).output.columns().len();
        let templates = self.region_templates(RegionId::DATASET).count();
        Part::passed(records, (width, templates), outer.handlers.mark())
    }

    /// Decode partition `index` and run it on the calling thread, stopping
    /// at `limit`; none when the run was cancelled or the partition did not
    /// decode.
    fn decode_and_run<T: TableAccess>(
        &self,
        outer: &mut Run<'_, '_, T::Error>,
        decode: &mut dyn FnMut(usize, Range<usize>) -> Option<T>,
        (schema, ranges): (&TableSchema, &[Range<usize>]),
        (index, limit): (usize, usize),
    ) -> Result<Option<Part<T::Error>>, Fail<T::Error>> {
        let range = ranges[index].clone();
        let Some(table) = decode(index, range.clone()) else {
            return Ok(None);
        };
        if table.schema() != schema {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        Ok(self.run_here(outer, &table, (range.start, index == 0, limit)))
    }

    /// Run the decoded partition `table`, whose first record is at
    /// `records`, on the calling thread, stopping at `limit`; none when the
    /// run was cancelled.
    fn run_here<E>(
        &self,
        outer: &mut Run<'_, '_, E>,
        table: &dyn TableAccess<Error = E>,
        (records, first, limit): (usize, bool, usize),
    ) -> Option<Part<E>> {
        let placement = Placement {
            records,
            first,
            limit,
            output_partition: outer.output_partition,
            publish: outer.publish,
        };
        self.run_partition(
            (table, outer.secondary),
            &mut *outer.functions,
            outer.cancelled,
            (placement, None),
        )
    }

    /// Run one partition, `primary`, through every derivation step, placed
    /// as `placement` says, raising `opened` once it enters its second mark
    /// (#3830, #4193). None when the run was cancelled.
    fn run_partition<'b, E>(
        &self,
        (primary, secondary): (
            &'b dyn TableAccess<Error = E>,
            &'b [&'b dyn TableAccess<Error = E>],
        ),
        functions: &'b mut dyn FunctionPort<Error = E>,
        cancelled: &'b dyn Fn() -> bool,
        (placement, opened): (Placement, Option<Arc<AtomicBool>>),
    ) -> Option<Part<E>> {
        let Placement {
            records,
            first,
            limit,
            output_partition,
            publish,
        } = placement;
        let mut run = Run::new(self, (primary, secondary), functions, cancelled);
        run.output_partition = output_partition;
        let mut marks = Marks::new(limit, first);
        marks.opened = opened;
        run.marks = Some(marks);
        let mut frame = Frame::new(self.region(RegionId::DATASET).output.columns().len());
        let mut checked = Vec::new();
        let mut templates = Vec::new();
        let result = run.derive_partition(&mut frame, &mut checked, &mut templates);
        if run.stopped.get() {
            return None;
        }
        let mut marks = run.marks.take().expect("a partition's run has marks");
        marks.opened = None;
        let ended = result.is_ok();
        let failure = match result {
            Ok(()) => None,
            Err(_) if marks.limited => None,
            Err(error) if matches!(*error, ExecutionError::Cancelled) => return None,
            Err(error) => Some((error, marks.coordinates)),
        };
        let blocks = frame.blocks(self.region_templates(RegionId::DATASET).count());
        let mut key_groups = run.key_groups.take();
        // A run that publishes its partitions keeps none of their frames:
        // one that ended checks its output keys and orders its rows now, as
        // its run ends, and keeps its columns (#4219).
        let mut published = None;
        if let Some(units) = publish {
            let frame = std::mem::take(&mut frame);
            if ended {
                published = Some(self.publish(frame, key_groups.take(), &blocks, units));
            }
        }
        Some(Part {
            records,
            frame,
            blocks,
            published,
            templates,
            checked,
            counts: run.handlers.mark(),
            key_groups,
            marks,
            failure,
        })
    }

    /// The candidates `frame` of a partition whose run ended, as a run that
    /// publishes its partitions keeps them (#4219): its output keys checked
    /// in `units`, with the key grain's `key_groups` when it kept them, its
    /// rows in the output's order, and its columns.
    fn publish(
        &self,
        frame: Frame,
        key_groups: Option<Vec<usize>>,
        blocks: &[Range<usize>],
        units: Units,
    ) -> Published {
        let units: Vec<Vec<Range<usize>>> = match units {
            Units::Templates => blocks.iter().map(|block| vec![block.clone()]).collect(),
            Units::Partition => vec![vec![0..frame.rows()]],
        };
        let keys = checks::partition_keys(self, &frame, key_groups, &units);
        let ordered = if self.output_order().is_empty() {
            Ordered::Construction
        } else {
            match checks::output_order::<std::convert::Infallible>(self, &frame) {
                Ok(order) => Ordered::Sorted(order),
                Err(_) => Ordered::Refused,
            }
        };
        Published {
            values: frame.into_values(),
            keys,
            ordered,
        }
    }

    /// The whole-table run's attempt, from every partition's run (#3830):
    /// the column checks it reached, in order, over every partition's
    /// candidates; then its failure, or else its output keys, dataset checks
    /// and order over those candidates.
    fn combine<E>(
        &self,
        outer: &mut Run<'_, '_, E>,
        (mut parts, whole): (Vec<Part<E>>, bool),
        failure: Failure,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Found<E>, Fail<E>> {
        // An output admitted whole is checked as the whole-table run checks
        // it, every partition's rows added up at each capacity check.
        if let Some((mark, error)) = outer
            .output_partition
            .is_none()
            .then(|| self.capacity_failure(&parts, failure))
            .flatten()
        {
            let counts = compose(&parts, |_, part| part.counts_before(mark + 1));
            return Ok(Found {
                result: Err(error),
                handler_counts: report(self, counts),
                records: std::mem::take(&mut parts[0].checked),
                templates: template_rows(&parts, failure.map_or(0, |(index, _)| index), mark),
                cancelled: false,
            });
        }
        let blocks: Vec<Vec<Range<usize>>> = parts.iter().map(|part| part.blocks.clone()).collect();
        // The failing partition entered every mark the whole-table run
        // reached; without a failure, every partition entered every mark.
        let end = failure.map_or(usize::MAX, |(_, mark)| mark);
        let reference = failure.map_or(0, |(index, _)| index);
        let checks: Vec<(usize, Option<usize>)> = parts[reference]
            .marks
            .checks
            .iter()
            .copied()
            .filter(|&(position, _)| position <= end)
            .collect();
        let mut records = std::mem::take(&mut parts[0].checked);
        // A run that publishes its partitions declares no column check, so
        // none of the checks' places checks anything, and it merges nothing
        // (#4219).
        let publishing = outer.publish.is_some();
        let frame = if !publishing && (failure.is_none() || !checks.is_empty()) {
            merged(
                &mut parts,
                &blocks,
                self.region(RegionId::DATASET).output.columns().len(),
                failure.is_none(),
            )?
        } else {
            Frame::new(0)
        };
        let mut completed = row_completed(self);
        let mut next = 0;
        let checks = if publishing { Vec::new() } else { checks };
        for &(position, column) in &checks {
            if let Some(column) = column {
                completed[column] = true;
            }
            if let Err(error) =
                checks::check_columns(outer, &frame, &completed, &mut next, &mut records)
            {
                let counts = compose(&parts, |_, part| part.counts_before(position));
                return Ok(Found {
                    result: Err(error),
                    handler_counts: report(self, counts),
                    records,
                    templates: template_rows(&parts, reference, position),
                    cancelled: false,
                });
            }
        }
        if let Some((index, mark)) = failure {
            let (mut error, coordinates) = parts[index].failure.take().expect("a failed partition");
            rebase(&mut error, &parts, &blocks, (index, coordinates));
            let counts = compose(&parts, |at, part| match at.cmp(&index) {
                std::cmp::Ordering::Less => part.counts_before(mark + 1),
                std::cmp::Ordering::Equal => &part.counts,
                std::cmp::Ordering::Greater => part.counts_before(mark),
            });
            return Ok(Found {
                result: Err(error),
                handler_counts: report(self, counts),
                records,
                templates: template_rows(&parts, reference, mark),
                cancelled: false,
            });
        }
        outer
            .handlers
            .restore(compose(&parts, |_, part| &part.counts));
        let templates = template_rows(&parts, reference, end);
        if publishing {
            let result = self.finish_published(outer, &mut parts, &blocks, &mut records, observer);
            return Ok(Found {
                result,
                handler_counts: outer.handlers.snapshot(self),
                records,
                templates,
                cancelled: outer.stopped.get(),
            });
        }
        // The output key check groups what no key group leaves on its own:
        // each partition's key grain, each template's rows, or each
        // partition's rows (#4211).
        if whole {
            outer.key_groups = composed_key_groups(&parts, &blocks);
        }
        if outer.key_groups.is_none() {
            let apart = self.templates_apart();
            outer.key_partitions = (apart || whole).then(|| merged_rows(&blocks, apart));
        }
        drop(parts);
        let inputs = outer.inputs();
        let result = outer.finish(frame, &inputs, &mut records, observer);
        Ok(Found {
            result,
            handler_counts: outer.handlers.snapshot(self),
            records,
            templates,
            cancelled: outer.stopped.get(),
        })
    }
}

impl ResolvedPlan {
    /// Whether no output key group of a run over `partitions` partitions,
    /// key-bounded over the leading `keys` declared keys when there are
    /// any, spans two partitions (#4211). One partition holds every group.
    /// Key-bounded partitions are cut between runs of the fields the
    /// leading keys copy unchanged in every template that reads the driver
    /// input ([`ResolvedPlan::partition_fields`]), and the cut finds no cut
    /// when a run of those fields recurs after it ended (`dataset::cut`),
    /// so two records of different partitions differ in those fields. When
    /// every template reads the driver input, each output row's leading
    /// keys are its records' fields, so two rows of different partitions
    /// differ in a key: no key group spans them. A template that does not
    /// read the driver input builds its rows in the first partition's run
    /// with keys of any value, and consecutive partitions keep nothing
    /// whole, so their output keys group over every partition at once. A
    /// plan whose leading keys copy no fields has no key-bounded cut: its
    /// output keys group over every partition too, whatever ranges a
    /// caller passed.
    fn keeps_key_groups(&self, keys: usize, partitions: usize) -> bool {
        partitions == 1
            || (keys > 0
                && self.partition_fields(keys).is_some()
                && self
                    .region_templates(RegionId::DATASET)
                    .all(|template| self.reads_driver(template)))
    }

    /// Whether no two templates' rows share an output key (#4211): the
    /// dataset has at most one template, or every template binds some key
    /// to a present literal of the key's type, each template's distinct, so
    /// rows of two templates differ in that key, as the templates of a BDS
    /// specification bind PARAMCD.
    fn templates_apart(&self) -> bool {
        use crate::value::{ColumnType, Value};
        let templates: Vec<TemplateId> = self.region_templates(RegionId::DATASET).collect();
        templates.len() < 2
            || self.keys().iter().any(|&key| {
                let kind = self.column(key).kind;
                let mut seen: Vec<crate::table_grouping::Key<'_>> = Vec::new();
                templates.iter().all(|&template| {
                    let Some(binding) = self.cell(template, key) else {
                        return false;
                    };
                    let Op::Literal(value) = &self.expr(self.binding(binding).expr).op else {
                        return false;
                    };
                    let typed = matches!(
                        (value, kind),
                        (Value::Str(_), Some(ColumnType::Str))
                            | (Value::Int(_), Some(ColumnType::Int))
                            | (Value::Float(_), Some(ColumnType::Float))
                            | (Value::Date(_), Some(ColumnType::Date))
                            | (Value::DateTime(_), Some(ColumnType::DateTime))
                    );
                    let literal = crate::table_grouping::Key::from(ValueRef::from(value));
                    let distinct = typed && !seen.contains(&literal);
                    seen.push(literal);
                    distinct
                })
            })
    }

    /// The first capacity check of an output admitted whole that the whole
    /// table fails, every partition's rows added up (#3830), in a step
    /// before the first partition's failure: the mark it fails at, after
    /// every partition's driver step, and the failure.
    fn capacity_failure<E>(&self, parts: &[Part<E>], failure: Failure) -> Option<(usize, Fail<E>)> {
        let (reference, end) = failure.unwrap_or((0, usize::MAX));
        let width = self.region(RegionId::DATASET).output.columns().len();
        parts[reference]
            .marks
            .capacity
            .iter()
            .enumerate()
            .take_while(|(_, &(entered, _))| entered - 1 < end)
            .find_map(|(index, &(entered, _))| {
                let rows = parts.iter().try_fold(0usize, |rows, part| {
                    rows.checked_add(part.marks.capacity.get(index).map_or(0, |&(_, rows)| rows))
                });
                let error = match rows {
                    Some(rows) => Run::capacity(rows, None, width).err()?,
                    None => Box::new(ExecutionError::Limit {
                        resource: crate::dataset::execution::Resource::OutputRows,
                        limit: crate::limits::Limits::TABLE_ROWS,
                        required: None,
                    }),
                };
                Some((entered - 1, error))
            })
    }
}

impl ResolvedPlan {
    /// How the partitions of a run over `partitions` partitions,
    /// key-bounded over the leading `keys` declared keys when there are
    /// any, group their own output keys when the run publishes them
    /// (#4219); none when the run merges their candidates into one frame.
    /// A run publishes its partitions when no output key group spans two
    /// of them ([`Self::keeps_key_groups`]), so that each partition's run
    /// can check its own keys, and its plan declares no column or dataset
    /// check, which read every partition's candidates as one frame.
    fn publishing(&self, keys: usize, partitions: usize) -> Option<Units> {
        let checked = self.checks().iter().any(|check| {
            matches!(
                check.scope,
                crate::dataset::resolved::CheckScope::Column(_)
                    | crate::dataset::resolved::CheckScope::Dataset
            )
        });
        (!checked && self.keeps_key_groups(keys, partitions)).then(|| {
            if self.templates_apart() {
                Units::Templates
            } else {
                Units::Partition
            }
        })
    }

    /// Check, key and order the candidates each partition's run published
    /// (#4219), as [`Run::finish`] checks, keys and orders them merged into
    /// one frame. The output key check adds up what each partition's run
    /// found, its rows moved to the whole output's, and the dataset reads
    /// each partition's columns: in the whole-table run's order without
    /// order terms; in each partition's own order, one partition after
    /// another, when every partition's rows come before the next one's; or
    /// else in the whole output's order over their columns joined.
    fn finish_published<E>(
        &self,
        outer: &mut Run<'_, '_, E>,
        parts: &mut [Part<E>],
        blocks: &[Vec<Range<usize>>],
        records: &mut Vec<CheckRecord>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Fail<E>> {
        super::relations::check_unobserved_filter_types(self)?;
        let layout = Layout::new(blocks);
        let published: Vec<&Published> = parts
            .iter()
            .map(|part| part.published.as_ref().expect("a published partition"))
            .collect();
        let keys = checks::key_columns(self);
        let width = self.region(RegionId::DATASET).output.columns().len();
        let partition = outer.output_partition;
        checks::verify_published(
            outer,
            layout.rows,
            |_| self.published_keys(&published, &layout, (&keys, width), partition),
            |positions| {
                positions
                    .into_iter()
                    .map(|position| {
                        let (part, row) = layout.locate(position);
                        RowIdentity {
                            position,
                            values: keys
                                .iter()
                                .map(|&key| published[part].values[key].value(row))
                                .collect(),
                        }
                    })
                    .collect()
            },
            records,
            observer,
        )?;
        outer.poll(Step::Order)?;
        let dataset = self.published_dataset(parts, &layout)?;
        Ok(Execution {
            dataset,
            verifications: std::mem::take(records),
            partition_rows: outer.output_partition,
        })
    }

    /// What the output key check finds over every partition's published
    /// candidates (#4219): each partition's missing keys and repeated keys,
    /// moved to the whole output's positions in order, and its groups added
    /// up, since no key group spans two partitions. The grouping is
    /// admitted over the whole output, as the check over one frame admits
    /// it, before any partition's refusal of memory counts.
    fn published_keys<E>(
        &self,
        published: &[&Published],
        layout: &Layout,
        (keys, width): (&[usize], usize),
        partition: Option<NonZeroUsize>,
    ) -> Result<checks::KeyFindings, Fail<E>> {
        let mut missing = vec![Vec::new(); keys.len()];
        let mut groups = 0usize;
        let mut repeated = Vec::new();
        let mut fault = None;
        for (index, part) in published.iter().enumerate() {
            match &part.keys {
                Ok(found) => {
                    for (rows, local) in missing.iter_mut().zip(&found.missing) {
                        rows.extend(local.iter().map(|&row| layout.position(index, row)));
                    }
                    groups += found.groups;
                    repeated.extend(
                        found
                            .repeated
                            .iter()
                            .map(|&row| layout.position(index, row)),
                    );
                }
                Err(error) => {
                    fault.get_or_insert(*error);
                }
            }
        }
        for rows in &mut missing {
            rows.sort_unstable();
        }
        super::group::admit(keys, width, layout.rows, partition)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        if let Some(fault) = fault {
            return Err(fault.failure());
        }
        repeated.sort_unstable();
        Ok(checks::KeyFindings {
            missing,
            groups,
            repeated,
        })
    }

    /// The dataset of every partition's published columns (#4219), in the
    /// output's order: without order terms, the whole-table run's order,
    /// read from each partition's columns as they are; in each partition's
    /// own order, one partition after another, when that is the output's
    /// order ([`Self::ordered_apart`]); or else the whole output's order
    /// over their columns joined in the whole-table run's order, each
    /// partition's column dropped once joined.
    fn published_dataset<E>(
        &self,
        parts: &mut [Part<E>],
        layout: &Layout,
    ) -> Result<Dataset, Fail<E>> {
        let schema = self.region(RegionId::DATASET).output.clone();
        let width = schema.columns().len();
        let mut published: Vec<Published> = parts
            .iter_mut()
            .map(|part| part.published.take().expect("a published partition"))
            .collect();
        if self.output_order().is_empty() {
            let values = published.into_iter().map(|part| part.values).collect();
            let runs = layout.runs.iter().map(|run| (run.part, run.rows.clone()));
            return Ok(Dataset::from_parts(schema, values, runs));
        }
        if self.ordered_apart(&published) {
            let mut values = Vec::with_capacity(published.len());
            let mut runs = Vec::with_capacity(published.len());
            for (index, part) in published.into_iter().enumerate() {
                let Ordered::Sorted(order) = part.ordered else {
                    unreachable!("ordered apart: every partition sorted")
                };
                runs.push((index, 0..order.len()));
                values.push(gathered(part.values, &order)?);
            }
            return Ok(Dataset::from_parts(schema, values, runs));
        }
        let mut columns = Vec::with_capacity(width);
        for column in 0..width {
            let mut joined = Values::missing(0);
            for run in &layout.runs {
                joined.extend_from(&published[run.part].values[column], run.rows.clone());
            }
            for part in &mut published {
                part.values[column] = Values::default();
            }
            columns.push(joined);
        }
        let order = checks::order_rows(self, layout.rows, |row, column| columns[column].get(row))?;
        let columns = gathered(columns, &order)?;
        Ok(Dataset::from_columns(schema, columns, layout.rows))
    }

    /// Whether every published partition's rows, each partition sorted in
    /// the output's order, come strictly before the next nonempty
    /// partition's (#4219): the last of one precedes the first of the next
    /// on the order terms. Then the whole output's order, which keeps the
    /// construction order only of rows it ties (VALUE-23), is each
    /// partition's own order, one partition after another, whatever their
    /// templates. A partition whose rows did not sort orders nothing apart.
    fn ordered_apart(&self, published: &[Published]) -> bool {
        let mut last: Option<(&[Values], usize)> = None;
        for part in published {
            let Ordered::Sorted(order) = &part.ordered else {
                return false;
            };
            let (Some(&first), Some(&end)) = (order.first(), order.last()) else {
                continue;
            };
            if let Some((before, row)) = last {
                let after = &part.values;
                if !checks::precedes(
                    self,
                    |column| before[column].get(row),
                    |column| after[column].get(first),
                ) {
                    return false;
                }
            }
            last = Some((&part.values, end));
        }
        true
    }
}

/// `values`, each column's rows in `order`, a permutation of its rows:
/// moved whole when `order` keeps every row in place, and otherwise gathered
/// a column at a time, each column dropped once gathered (#3870).
fn gathered<E>(values: Vec<Values>, order: &[usize]) -> Result<Vec<Values>, Fail<E>> {
    if order
        .iter()
        .enumerate()
        .all(|(position, &row)| position == row)
    {
        return Ok(values);
    }
    let mut ordered = Vec::new();
    reserved(ordered.try_reserve_exact(values.len()))?;
    for column in values {
        ordered.push(reserved(column.gather(order))?);
    }
    Ok(ordered)
}

/// The candidates of every partition's run in the whole-table run's order
/// (#4219): each template's rows, partition after partition, before the
/// next template's.
struct Layout {
    /// Each nonempty run of one partition's rows, in that order.
    runs: Vec<LayoutRun>,
    /// For each partition, the rows of each template among its candidates.
    blocks: Vec<Vec<Range<usize>>>,
    /// For each partition, the output row each template's rows start at.
    starts: Vec<Vec<usize>>,
    rows: usize,
}

/// Rows `rows` of partition `part`, the output's from row `at` on.
struct LayoutRun {
    part: usize,
    rows: Range<usize>,
    at: usize,
}

impl Layout {
    /// The order of every partition's candidates, `blocks` holding each
    /// partition's rows of each template.
    fn new(blocks: &[Vec<Range<usize>>]) -> Self {
        let templates = blocks.first().map_or(0, Vec::len);
        let mut runs = Vec::new();
        let mut starts = vec![vec![0; templates]; blocks.len()];
        let mut at = 0;
        for template in 0..templates {
            for (part, local) in blocks.iter().enumerate() {
                let rows = local[template].clone();
                starts[part][template] = at;
                if !rows.is_empty() {
                    runs.push(LayoutRun {
                        part,
                        rows: rows.clone(),
                        at,
                    });
                }
                at += rows.len();
            }
        }
        Self {
            runs,
            blocks: blocks.to_vec(),
            starts,
            rows: at,
        }
    }

    /// The output row of row `row` of partition `part`'s candidates.
    fn position(&self, part: usize, row: usize) -> usize {
        let blocks = &self.blocks[part];
        let template = blocks.partition_point(|block| block.end <= row);
        self.starts[part][template] + (row - blocks[template].start)
    }

    /// The partition and row among its candidates of output row `position`.
    fn locate(&self, position: usize) -> (usize, usize) {
        let run = &self.runs[self
            .runs
            .partition_point(|run| run.at + run.rows.len() <= position)];
        (run.part, run.rows.start + (position - run.at))
    }
}

impl<E> Found<E> {
    /// A cancelled attempt, which keeps nothing.
    fn cancelled() -> Self {
        Self {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            records: Vec::new(),
            templates: Vec::new(),
            cancelled: true,
        }
    }
}

/// Every partition's candidates as the whole-table run builds them: each
/// template's rows, partition after partition, before the next template's.
/// With `consume`, each partition's candidates are dropped once its last
/// template's rows are merged, so the run never holds them twice (#3870).
fn merged<E>(
    parts: &mut [Part<E>],
    blocks: &[Vec<Range<usize>>],
    width: usize,
    consume: bool,
) -> Result<Frame, Fail<E>> {
    let mut frame = Frame::new(width);
    let rows = parts.iter().map(|part| part.frame.rows()).sum();
    let members = parts.iter().map(|part| part.frame.member_count()).sum();
    reserved(frame.try_reserve(rows, members))?;
    let templates = blocks.first().map_or(0, Vec::len);
    for template in 0..templates {
        for (part, blocks) in parts.iter_mut().zip(blocks) {
            frame.append(&part.frame, blocks[template].clone(), part.records);
            if consume && template + 1 == templates {
                part.frame = Frame::default();
            }
        }
    }
    Ok(frame)
}

/// The output key group of each row of the merged frame, from the key
/// grain's groups of every partition's candidates (#4211), when every
/// partition kept them: each partition's groups numbered after every
/// earlier partition's. Only for partitions that no key group spans
/// ([`ResolvedPlan::keeps_key_groups`]).
fn composed_key_groups<E>(parts: &[Part<E>], blocks: &[Vec<Range<usize>>]) -> Option<Vec<usize>> {
    let mut offsets = Vec::with_capacity(parts.len());
    let mut groups = 0usize;
    for part in parts {
        offsets.push(groups);
        groups += part
            .key_groups
            .as_ref()?
            .iter()
            .max()
            .map_or(0, |&group| group + 1);
    }
    let rows = parts
        .iter()
        .map(|part| part.key_groups.as_ref().map_or(0, Vec::len))
        .sum();
    let mut composed = Vec::new();
    composed.try_reserve_exact(rows).ok()?;
    let templates = blocks.first().map_or(0, Vec::len);
    for template in 0..templates {
        for ((part, blocks), &offset) in parts.iter().zip(blocks).zip(&offsets) {
            let local = part.key_groups.as_ref()?;
            composed.extend(
                local[blocks[template].clone()]
                    .iter()
                    .map(|&group| group + offset),
            );
        }
    }
    Some(composed)
}

/// The rows of each template in the merged frame ([`merged`]), every
/// partition's in one range, with `by_template`; or else the rows of each
/// partition, its templates' blocks in order.
fn merged_rows(blocks: &[Vec<Range<usize>>], by_template: bool) -> Vec<Vec<Range<usize>>> {
    let templates = blocks.first().map_or(0, Vec::len);
    let mut rows: Vec<Vec<Range<usize>>> =
        vec![Vec::new(); if by_template { templates } else { blocks.len() }];
    let mut start = 0;
    for template in 0..templates {
        for (part, blocks) in blocks.iter().enumerate() {
            let len = blocks[template].len();
            if by_template {
                match rows[template].last_mut() {
                    Some(range) => range.end += len,
                    None => rows[template].push(start..start + len),
                }
            } else {
                rows[part].push(start..start + len);
            }
            start += len;
        }
    }
    rows
}

/// The handler counts of every partition's run, each at the point `at`
/// gives for its index.
fn compose<'p, E: 'p>(
    parts: &'p [Part<E>],
    at: impl Fn(usize, &'p Part<E>) -> &'p HandlerMark,
) -> HandlerMark {
    let mut parts = parts.iter().enumerate();
    let (_, first) = parts.next().expect("a run has a partition");
    let mut counts = at(0, first).clone();
    for (index, part) in parts {
        counts.add(at(index, part));
    }
    counts
}

/// The handler counts `counts` as a run reports them.
fn report(plan: &ResolvedPlan, counts: HandlerMark) -> Vec<crate::numeric_lifecycle::HandlerCount> {
    let mut handlers = Handlers::new(plan);
    handlers.restore(counts);
    handlers.snapshot(plan)
}

/// The rows of each template completed after at most `end` marks, every
/// partition's added up, as the reference partition completed them.
fn template_rows<E>(parts: &[Part<E>], reference: usize, end: usize) -> Vec<TemplateRows> {
    let completed = parts[reference]
        .marks
        .completed
        .iter()
        .filter(|&&position| position <= end)
        .count();
    (0..completed)
        .map(|template| {
            parts.iter().fold(
                TemplateRows {
                    emitted: 0,
                    read: 0,
                },
                |total, part| {
                    let rows = part.templates[template];
                    TemplateRows {
                        emitted: total.emitted.saturating_add(rows.emitted),
                        read: total.read.saturating_add(rows.read),
                    }
                },
            )
        })
        .collect()
}

/// Move the positions of the failure of partition `index`, its own, to the
/// whole run's (#3830): a record read to its position in the whole input; a
/// candidate to its row among every partition's, each template's rows after
/// the earlier templates'; a key-grain probe, which is a record, likewise;
/// and a value a row window collects while it is prepared, to its position
/// in the window's scope, its template's rows of every partition.
fn rebase<E>(
    error: &mut ExecutionError<E>,
    parts: &[Part<E>],
    blocks: &[Vec<Range<usize>>],
    (index, coordinates): (usize, Coordinates),
) {
    let records = parts[index].records;
    let earlier = |template: usize| -> usize {
        blocks[..index]
            .iter()
            .map(|blocks| blocks[template].len())
            .sum()
    };
    let candidate = |row: usize| -> usize {
        let local = &blocks[index];
        let template = local
            .iter()
            .rposition(|block| block.start <= row)
            .unwrap_or(0);
        let before: usize = (0..template)
            .map(|template| {
                blocks
                    .iter()
                    .map(|blocks| blocks[template].len())
                    .sum::<usize>()
            })
            .sum();
        before + earlier(template) + row - local[template].start
    };
    let rows = |row: usize| match coordinates {
        Coordinates::Records => records + row,
        Coordinates::Frame | Coordinates::Window(_) => candidate(row),
    };
    let collected = |position: usize| match coordinates {
        Coordinates::Window(template) => earlier(template) + position,
        _ => rows(position),
    };
    let identity = |identity: &mut Option<RowIdentity>, map: &dyn Fn(usize) -> usize| {
        if let Some(identity) = identity {
            identity.position = map(identity.position);
        }
    };
    use ExecutionError as X;
    match error {
        X::SemanticDiagnostic { identity: held, .. }
        | X::ProjectFunction { identity: held, .. }
        | X::Numeric { identity: held, .. }
        | X::MultipleMatches { identity: held, .. }
        | X::UnmatchedKey { identity: held, .. }
        | X::Reduction { identity: held, .. } => identity(held, &rows),
        X::MultipleValues { identity: held, .. } => identity(held, &collected),
        X::Conversion {
            output_row,
            identity: held,
            ..
        } => {
            *output_row = rows(*output_row);
            identity(held, &rows);
        }
        X::Predicate { source_row, .. } | X::Cell { source_row, .. } => *source_row += records,
        X::Grouping(GroupingError::Cell { row, .. }) => *row += records,
        X::ScalarDiagnostic(_)
        | X::IntermediateVerifications(_)
        | X::IntermediateFilterTypes(_)
        | X::VerificationDiagnostic { .. }
        | X::FunctionBinding { .. }
        | X::HandlerAccounting(_)
        | X::SecondaryCell { .. }
        | X::BaselineAmbiguity { .. }
        | X::VerificationDeclaration { .. }
        | X::VerificationPredicate { .. }
        | X::Limit { .. }
        | X::SourceLimit { .. }
        | X::ScopedLimit { .. }
        | X::SchemaMismatch
        | X::Capacity
        | X::Allocation
        | X::Grouping(_)
        | X::ReductionType { .. }
        | X::KeyFailures(_)
        | X::VerificationFailures(_)
        | X::OutputGrouping(_)
        | X::Cancelled
        | X::PartitionGlobal { .. } => (),
    }
}
