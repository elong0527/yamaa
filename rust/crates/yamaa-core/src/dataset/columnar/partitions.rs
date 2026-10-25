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
//! whole-table run over its own records, and dropped before the next one is
//! decoded: its row templates or its key grain, then its column phase. The
//! run keeps each partition's candidate rows, and checks, keys and orders
//! the candidates of every partition once, after the last partition.
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
use super::checks;
use super::frame::Frame;
use super::handlers::{HandlerMark, Handlers};
use super::{reserved, row_completed, Fail, Found, Run};
use crate::dataset::cut::{consecutive, PartitionCut};
use crate::dataset::execution::{
    CheckRecord, ExecutionAttempt, ExecutionError, ExecutionEvent, ExecutionPhase, RowIdentity,
    TemplateRows,
};
use crate::dataset::resolved::{FilterPlacement, RegionId, ResolvedPlan, Stage, Step, TemplateId};
use crate::dataset::Partitioning;
use crate::function_invocation::FunctionPort;
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::table_grouping::GroupingError;
use std::num::NonZeroUsize;
use std::ops::Range;

/// The partitions of a driver input that a partitioned run decodes one at a
/// time (#3830).
pub trait PartitionSource {
    /// A decoded partition.
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

/// One partition's run (#3830).
struct Part<E> {
    /// The position of the partition's first record in the whole input.
    records: usize,
    /// The candidate rows it built, each template's after the one before.
    frame: Frame,
    templates: Vec<TemplateRows>,
    /// Its check records: the intermediates' over the other inputs, which
    /// the first partition's run verifies.
    checked: Vec<CheckRecord>,
    marks: Marks,
    /// Its handler counts where it ended.
    counts: HandlerMark,
    /// Its failure, at the last mark it entered, and what its positions
    /// count.
    failure: Option<(Fail<E>, Coordinates)>,
}

impl<E> Part<E> {
    /// A partition after a failure at the first mark, which runs nothing.
    fn passed(records: usize, width: usize, counts: HandlerMark) -> Self {
        Self {
            records,
            frame: Frame::new(width),
            templates: Vec::new(),
            checked: Vec::new(),
            marks: Marks::new(0, false),
            counts,
            failure: None,
        }
    }

    /// The mark it failed at.
    fn failed_at(&self) -> Option<usize> {
        self.failure.as_ref().map(|_| self.marks.entered - 1)
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

    /// Its candidate rows of each template, in template order: contiguous,
    /// since a run builds one template's rows after another's. The key
    /// grain's candidates are its one template's.
    fn blocks(&self, templates: usize) -> Vec<Range<usize>> {
        let mut row = 0;
        (0..templates.max(1))
            .map(|template| {
                let start = row;
                while row < self.frame.rows() && self.frame.template(row).unwrap_or(0) == template {
                    row += 1;
                }
                start..row
            })
            .collect()
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
}

/// Where a partition's failure is in the whole run: the partition and the
/// mark.
type Failure = Option<(usize, usize)>;

impl ResolvedPlan {
    /// Execute the driver input a partition at a time (#3830): each
    /// partition of `source` is decoded when the run reaches it and dropped
    /// before the next one is decoded, so the run holds at most one decoded
    /// partition of its driver input. Its records keep their positions in
    /// the whole input, which every partition holds in order; `keys` names
    /// the leading declared keys no partition splits a group of, zero for
    /// consecutive records (#3763).
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
        source: &mut P,
        secondary: &[&dyn TableAccess<Error = <P::Table as TableAccess>::Error>],
        functions: &mut dyn FunctionPort<Error = <P::Table as TableAccess>::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
        keys: usize,
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
        };
        self.run_divided(input, secondary, functions, observer, cancelled)
    }

    /// Execute `table` whole, or divided as `partitioning` asks: into
    /// consecutive partitions of its `rows` records (#3697), or into
    /// key-bounded partitions over its leading `keys` declared keys (#3763),
    /// one partition at a time ([`Self::execute_partitioned`]).
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
        let Some(partitioning) = partitioning else {
            return self.execute(table, secondary, functions, observer, cancelled);
        };
        let keys = partitioning
            .leading_keys
            .map_or(0, |keys| keys.get().min(self.keys().len()));
        if partitioning.whole_table_fallback && self.whole_table_step(keys).is_some() {
            return self.execute(table, secondary, functions, observer, cancelled);
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
        let width = self.region(RegionId::DATASET).output.columns().len();
        let mut parts = Vec::with_capacity(ranges.len());
        let mut failure: Failure = None;
        for (index, range) in ranges.iter().enumerate() {
            let limit = failure.map_or(usize::MAX, |(_, mark)| mark);
            if limit == 0 {
                parts.push(Part::passed(range.start, width, outer.handlers.mark()));
                continue;
            }
            let Some(table) = (input.decode)(index, range.clone()) else {
                return Ok(Found::cancelled());
            };
            if table.schema() != input.schema {
                return Err(Box::new(ExecutionError::SchemaMismatch));
            }
            let part = self.run_partition(
                (&table, outer.secondary),
                &mut *outer.functions,
                outer.cancelled,
                Placement {
                    records: range.start,
                    first: index == 0,
                    limit,
                    output_partition: outer.output_partition,
                },
            );
            drop(table);
            let Some(part) = part else {
                return Ok(Found::cancelled());
            };
            if let Some(mark) = part.failed_at() {
                failure = Some((index, mark));
            }
            parts.push(part);
        }
        self.combine(outer, parts, failure, observer)
    }

    /// Run one partition, `primary`, through every derivation step, placed
    /// as `placement` says (#3830). None when the run was cancelled.
    fn run_partition<'b, E>(
        &self,
        (primary, secondary): (
            &'b dyn TableAccess<Error = E>,
            &'b [&'b dyn TableAccess<Error = E>],
        ),
        functions: &'b mut dyn FunctionPort<Error = E>,
        cancelled: &'b dyn Fn() -> bool,
        placement: Placement,
    ) -> Option<Part<E>> {
        let Placement {
            records,
            first,
            limit,
            output_partition,
        } = placement;
        let mut run = Run::new(self, (primary, secondary), functions, cancelled);
        run.output_partition = output_partition;
        run.marks = Some(Marks::new(limit, first));
        let mut frame = Frame::new(self.region(RegionId::DATASET).output.columns().len());
        let mut checked = Vec::new();
        let mut templates = Vec::new();
        let result = run.derive_partition(&mut frame, &mut checked, &mut templates);
        if run.stopped.get() {
            return None;
        }
        let marks = run.marks.take().expect("a partition's run has marks");
        let failure = match result {
            Ok(()) => None,
            Err(_) if marks.limited => None,
            Err(error) if matches!(*error, ExecutionError::Cancelled) => return None,
            Err(error) => Some((error, marks.coordinates)),
        };
        Some(Part {
            records,
            frame,
            templates,
            checked,
            counts: run.handlers.mark(),
            marks,
            failure,
        })
    }

    /// The whole-table run's attempt, from every partition's run (#3830):
    /// the column checks it reached, in order, over every partition's
    /// candidates; then its failure, or else its output keys, dataset checks
    /// and order over those candidates.
    fn combine<E>(
        &self,
        outer: &mut Run<'_, '_, E>,
        mut parts: Vec<Part<E>>,
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
        let templates = self.region_templates(RegionId::DATASET).count();
        let blocks: Vec<Vec<Range<usize>>> =
            parts.iter().map(|part| part.blocks(templates)).collect();
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
        let frame = if failure.is_none() || !checks.is_empty() {
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
