//! The executor over the resolved plan (rewrite 1, #2841): the schedule runs
//! column at a time over typed column buffers.
//!
//! Every expression node evaluates over a selection of rows, in row order,
//! and answers one buffer aligned with it. Lazy operands stay lazy per row:
//! a `case` evaluates each condition over the rows still undecided and each
//! branch over the rows that chose it.
//!
//! The executor runs in units: a template's row stage, one window-stage
//! binding, a post-window filter, one column step, the key bindings of the
//! key grain, one derivation of a donor. A unit first runs in batch: every
//! node once over the whole selection. If any row fails, the unit's effects
//! are rolled back and it replays in row mode, one row at a time, which
//! reproduces the first failure in base-record order, its diagnostic
//! identity and the handler counts before it. A unit whose effects include
//! host callbacks runs in row mode from the start, so callbacks keep their
//! logical row order (FUNC-8).
use super::execution::{
    CheckRecord, Dataset, Execution, ExecutionAttempt, ExecutionError, ExecutionEvent,
    ExecutionPhase, Resource, TemplateRows,
};
use super::resolved::{
    BindingId, ColumnId, Effects, ExprId, RegionId, ResolvedPlan, Stage, TemplateId,
};
use crate::function_invocation::FunctionPort;
use crate::limits::Limits;
use crate::table::{CellError, ColumnRef, TableAccess, TableSchema, ValueRef};
use crate::value::Value;
use std::ops::Range;

mod aggregates;
pub(crate) mod buffer;
mod checks;
mod donor;
mod eval;
mod frame;
mod functions;
mod group;
mod handlers;
mod index;
mod joins;
mod keygrain;
mod odm;
mod pairs;
mod partitions;
pub use partitions::{PartitionSource, SharedDecode};
mod predicates;
mod relations;
mod scalar;
#[cfg(test)]
mod tests;
mod windows;

use buffer::Values;
use frame::{Frame, Identity};
use handlers::{HandlerMark, Handlers};

type Fail<E> = Box<ExecutionError<E>>;

/// A refused reservation fails the run as `ExecutionError::Allocation`, as
/// today's executor's reservations do, instead of aborting the process.
fn reserved<T, E>(reservation: Result<T, std::collections::TryReserveError>) -> Result<T, Fail<E>> {
    reservation.map_err(|_| Box::new(ExecutionError::Allocation))
}

/// `len` copies of `value`, reserved before they are written.
fn filled<T: Clone, E>(len: usize, value: T) -> Result<Vec<T>, Fail<E>> {
    let mut filled = Vec::new();
    reserved(filled.try_reserve_exact(len))?;
    filled.resize(len, value);
    Ok(filled)
}

/// A table one read addresses.
enum Table<'a, E> {
    /// A source port.
    Source(&'a dyn TableAccess<Error = E>),
    /// Frame rows `start..start + rows`, as donor records: completed rows
    /// of earlier templates (SELF), a materialized donor, or the output
    /// rows an aggregate ranges over.
    Rows {
        frame: &'a Frame,
        schema: &'a TableSchema,
        start: usize,
        rows: usize,
    },
    /// A donor's input while its derivations run: the donor frame's stored
    /// and derived fields, then the current row's values: one set for every
    /// record, or with a `stride`, one set per record.
    Input {
        frame: &'a Frame,
        current: &'a [Value],
        stride: usize,
        schema: &'a TableSchema,
    },
}

impl<E> Clone for Table<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<E> Copy for Table<'_, E> {}

impl<E> TableAccess for Table<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        match self {
            Self::Source(table) => table.schema(),
            Self::Rows { schema, .. } | Self::Input { schema, .. } => schema,
        }
    }
    fn row_count(&self) -> usize {
        match self {
            Self::Source(table) => table.row_count(),
            Self::Rows { rows, .. } => *rows,
            Self::Input { frame, .. } => frame.rows(),
        }
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        (*self).get(row, column)
    }
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        (*self).storage(column)
    }
}

impl<'a, E> Table<'a, E> {
    /// A source port's column, lent whole with the table's own lifetime
    /// when the port holds it as typed storage (#4190).
    fn storage(self, column: usize) -> Option<ColumnRef<'a>> {
        match self {
            Self::Source(table) => table.column(column),
            Self::Rows { .. } | Self::Input { .. } => None,
        }
    }

    /// Borrow one cell with the table's own lifetime.
    fn get(self, row: usize, column: usize) -> Result<ValueRef<'a>, CellError<E>> {
        match self {
            Self::Source(table) => table.cell(row, column),
            Self::Rows {
                frame, start, rows, ..
            } => {
                if row < rows && column < frame.width() {
                    Ok(frame.get(start + row, column))
                } else {
                    Err(CellError::OutOfBounds { row, column })
                }
            }
            Self::Input {
                frame,
                current,
                stride,
                ..
            } => {
                if row >= frame.rows() {
                    return Err(CellError::OutOfBounds { row, column });
                }
                if column < frame.width() {
                    Ok(frame.get(row, column))
                } else {
                    current
                        .get(row * stride + column - frame.width())
                        .map(ValueRef::from)
                        .ok_or(CellError::OutOfBounds { row, column })
                }
            }
        }
    }
}

/// What one evaluation reads besides its own operands.
struct Cx<'a, E> {
    pub region: RegionId,
    /// The region's output schema.
    pub schema: &'a TableSchema,
    pub frame: &'a Frame,
    /// The table a driver field reads: the template's driver in the row
    /// stage, the primary in the column phase, the input in a donor region.
    pub table: Table<'a, E>,
    /// The region's primary source.
    pub primary: Table<'a, E>,
    /// The region's secondary sources, in plan order.
    pub secondary: &'a [Table<'a, E>],
    /// Frame rows visible as SELF donor records: rows `0..self_rows`.
    pub self_rows: usize,
    /// The frame rows a window or an output aggregate ranges over.
    pub output: Range<usize>,
    /// A match expression's operand values, read by its field reads.
    pub operands: Option<&'a [Value]>,
    /// The donor record an aggregate derivation enriches, per selected row.
    pub record: Option<&'a relations::Record<'a, E>>,
}

impl<E> Clone for Cx<'_, E> {
    fn clone(&self) -> Self {
        Self {
            region: self.region,
            schema: self.schema,
            frame: self.frame,
            table: self.table,
            primary: self.primary,
            secondary: self.secondary,
            self_rows: self.self_rows,
            output: self.output.clone(),
            operands: self.operands,
            record: self.record,
        }
    }
}

impl<'a, E> Cx<'a, E> {
    /// The table and records a row's driver reads address: the row's own
    /// records, or the one donor record an aggregate derivation enriches.
    fn records(&self, row: usize) -> (Table<'a, E>, &[usize]) {
        match self.record {
            Some(record) => (record.table(), std::slice::from_ref(&record.source_row)),
            None => (self.table, self.frame.members(row)),
        }
    }

    /// The region-local position of a column slot.
    fn local(&self, plan: &ResolvedPlan, column: ColumnId) -> usize {
        column.index()
            - plan
                .region_columns(self.region)
                .next()
                .map_or(0, |first| first.index())
    }
}

/// Per-run state of one attempt.
struct Run<'p, 'a, E> {
    pub plan: &'p ResolvedPlan,
    pub primary: &'a dyn TableAccess<Error = E>,
    pub secondary: &'a [&'a dyn TableAccess<Error = E>],
    pub functions: &'a mut dyn FunctionPort<Error = E>,
    pub handlers: Handlers,
    /// Relation state of the region being evaluated; a donor
    /// materialization swaps in its own.
    pub relations: relations::State,
    /// Window runs of the unit being evaluated, by window slot.
    pub windows: windows::Runs,
    /// Whether the unit being evaluated may call the host, so nothing in it
    /// may be undone and evaluated again.
    pub calls: bool,
    /// Key indexes and filter answers over source ports, for every region.
    pub ports: index::Ports,
    /// The run's cancellation probe (#2822).
    pub cancelled: &'a dyn Fn() -> bool,
    /// Whether the probe has answered true: a run stays cancelled, even when
    /// a probe of a lookup swallowed the poll's failure.
    pub stopped: std::cell::Cell<bool>,
    /// The most rows one partition of the output holds, once admission has
    /// passed, for a partitioned run (D264); none while the output is
    /// admitted whole.
    pub output_partition: Option<std::num::NonZeroUsize>,
    /// Where one partition's run of a partitioned run has reached (#3830);
    /// none in a whole-table run.
    pub marks: Option<partitions::Marks>,
    /// The group of each frame row under the output keys, once the key
    /// grain has built the frame's rows (#4188), or composed from the key
    /// grain of each partition no key group spans (#4211), so the output
    /// key check does not group the same keys again.
    pub key_groups: Option<Vec<usize>>,
    /// The frame rows of each template, or of each partition, of a
    /// partitioned run, when no output key group spans two of them (#4211):
    /// the output key check groups each one's rows on its own.
    pub key_partitions: Option<Vec<Vec<std::ops::Range<usize>>>>,
}

/// What a batch unit restores before it replays in row mode.
struct Checkpoint {
    handlers: HandlerMark,
    relations: relations::Mark,
    /// The marks a partition's run had entered, and its handler snapshots.
    marks: Option<(usize, usize)>,
}

impl<'p, 'a, E> Run<'p, 'a, E> {
    /// A run of `plan` over its admitted inputs, with nothing counted yet.
    fn new(
        plan: &'p ResolvedPlan,
        (primary, secondary): (
            &'a dyn TableAccess<Error = E>,
            &'a [&'a dyn TableAccess<Error = E>],
        ),
        functions: &'a mut dyn FunctionPort<Error = E>,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            plan,
            primary,
            secondary,
            functions,
            handlers: Handlers::new(plan),
            relations: relations::State::new(plan, RegionId::DATASET),
            windows: windows::Runs::default(),
            calls: false,
            ports: index::Ports::default(),
            cancelled,
            stopped: std::cell::Cell::new(false),
            output_partition: None,
            marks: None,
            key_groups: None,
            key_partitions: None,
        }
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            handlers: self.handlers.mark(),
            relations: self.relations.mark(),
            marks: self.marks.as_ref().map(partitions::Marks::position),
        }
    }

    /// Undo a failed batch unit, its marks included, so that its replay
    /// enters them again under the same numbers.
    fn rollback(&mut self, checkpoint: Checkpoint) {
        if let (Some(marks), Some(position)) = (self.marks.as_mut(), checkpoint.marks) {
            marks.restore(position);
        }
        self.undo(checkpoint);
        self.windows = windows::Runs::default();
    }

    /// Undo the handler counts and relation entries since `checkpoint`,
    /// keeping the window runs of the unit being evaluated.
    fn undo(&mut self, checkpoint: Checkpoint) {
        self.handlers.restore(checkpoint.handlers);
        self.relations.restore(checkpoint.relations);
    }

    /// Count one firing of a handler site.
    fn fire(&mut self, site: super::resolved::HandlerSiteId) -> Result<(), Fail<E>> {
        self.handlers
            .fire(self.plan, site)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))
    }

    /// Count one firing at a path the resolved table may not list.
    fn fire_path(
        &mut self,
        path: &str,
        kind: crate::numeric_lifecycle::HandlerKind,
    ) -> Result<(), Fail<E>> {
        self.handlers
            .fire_path(self.plan, path, kind)
            .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))
    }

    /// Poll the cancellation probe before `step` when the dataset's schedule
    /// marks it a checkpoint (#2822).
    fn poll(&self, step: super::resolved::Step) -> Result<(), Fail<E>> {
        let checkpoint = self
            .plan
            .region_schedule(RegionId::DATASET)
            .iter()
            .any(|info| info.step == step && info.checkpoint);
        if checkpoint {
            self.stop_if_cancelled()?;
        }
        Ok(())
    }

    /// Fail with `ExecutionError::Cancelled` once the probe answers true; the
    /// probe is not asked again after that.
    fn stop_if_cancelled(&self) -> Result<(), Fail<E>> {
        if self.stopped.get() || (self.cancelled)() {
            self.stopped.set(true);
            return Err(Box::new(ExecutionError::Cancelled));
        }
        Ok(())
    }
}

/// A sized view of a table port.
struct Borrowed<'t, T: ?Sized>(&'t T);
impl<T: TableAccess + ?Sized> TableAccess for Borrowed<'_, T> {
    type Error = T::Error;
    fn schema(&self) -> &TableSchema {
        self.0.schema()
    }
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<T::Error>> {
        self.0.cell(row, column)
    }
    fn column(&self, column: usize) -> Option<ColumnRef<'_>> {
        self.0.column(column)
    }
}

/// What one attempt found, before its events close: its result, and the
/// evidence it keeps unless it was cancelled.
struct Found<E> {
    result: Result<Execution, Fail<E>>,
    handler_counts: Vec<crate::numeric_lifecycle::HandlerCount>,
    records: Vec<CheckRecord>,
    templates: Vec<TemplateRows>,
    /// Whether the probe answered true, so the attempt keeps nothing.
    cancelled: bool,
}

impl ResolvedPlan {
    /// Execute the admitted sources and the explicit function port once,
    /// retaining handler and verification evidence on success and failure.
    /// Binding returned this plan already resolved, and a run writes nothing
    /// into it.
    ///
    /// The run polls `cancelled` before every step the schedule marks a
    /// checkpoint and before each batch of project-function calls (#2822).
    /// Once it answers true the run stops, and its attempt fails with
    /// `ExecutionError::Cancelled`, keeping no output, handler counts,
    /// verifications or template rows: the per-run state is dropped with
    /// the run.
    pub fn execute<T: TableAccess + ?Sized>(
        &self,
        table: &T,
        secondary: &[&dyn TableAccess<Error = T::Error>],
        functions: &mut dyn FunctionPort<Error = T::Error>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecutionAttempt<T::Error> {
        let primary = Borrowed(table);
        self.observed(observer, |observer| {
            let mut run = Run::new(self, (&primary, secondary), functions, cancelled);
            let mut records = Vec::new();
            let mut templates = Vec::new();
            let result = run.execute_inner(&mut records, &mut templates, observer);
            Found {
                result,
                handler_counts: run.handlers.snapshot(self),
                records,
                templates,
                cancelled: run.stopped.get(),
            }
        })
    }

    /// One attempt, `attempt`, observed: its phases entered and left, its
    /// limit refusal reported, and a cancelled attempt keeping nothing it
    /// built.
    fn observed<E>(
        &self,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        attempt: impl FnOnce(&mut dyn FnMut(ExecutionEvent)) -> Found<E>,
    ) -> ExecutionAttempt<E> {
        let mut ignore = |_| {};
        let observer = observer.unwrap_or(&mut ignore);
        let active = std::cell::Cell::new(ExecutionPhase::Admission);
        let mut observer = |event| {
            if let ExecutionEvent::Phase(phase) = event {
                if phase != ExecutionPhase::Finished {
                    active.set(phase);
                }
            }
            observer(event);
        };
        observer(ExecutionEvent::Phase(ExecutionPhase::Admission));
        let found = attempt(&mut observer);
        let mut result = found.result;
        // A poll whose failure a lookup's probe swallowed still cancels the
        // run, and a cancelled run keeps nothing it built.
        let cancelled = found.cancelled
            || matches!(&result, Err(error) if matches!(**error, ExecutionError::Cancelled));
        if cancelled {
            result = Err(Box::new(ExecutionError::Cancelled));
        }
        if let Err(error) = &result {
            if let ExecutionError::Limit {
                resource,
                limit,
                required,
            }
            | ExecutionError::ScopedLimit {
                resource,
                limit,
                required,
                ..
            }
            | ExecutionError::SourceLimit {
                resource,
                limit,
                required,
                ..
            } = error.as_ref()
            {
                observer(ExecutionEvent::Refusal {
                    phase: active.get(),
                    refusal: crate::application_observer::Refusal {
                        resource: *resource,
                        limit: *limit,
                        required: *required,
                    },
                });
            }
        }
        observer(ExecutionEvent::Exit {
            phase: active.get(),
            outcome: crate::application_observer::Outcome::from_result(&result),
        });
        let attempt = if cancelled {
            ExecutionAttempt {
                result,
                handler_counts: Vec::new(),
                retained_verifications: Vec::new(),
                template_rows: Vec::new(),
            }
        } else {
            ExecutionAttempt {
                result,
                handler_counts: found.handler_counts,
                retained_verifications: found.records,
                template_rows: found.templates,
            }
        };
        observer(ExecutionEvent::Phase(ExecutionPhase::Finished));
        attempt
    }
}

impl<'p, 'a, E> Run<'p, 'a, E> {
    fn execute_inner(
        &mut self,
        records: &mut Vec<CheckRecord>,
        templates: &mut Vec<TemplateRows>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Fail<E>> {
        self.admit(None)?;
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::Admission,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Divided(super::Division::Whole));
        observer(ExecutionEvent::Phase(ExecutionPhase::Derivation));
        self.execute_admitted(records, templates, observer)
    }

    /// Admit the run before any record is read: its function bindings, the
    /// schemas and number of its inputs, their geometry within the row and
    /// cell limits, and the output's columns. `driver` holds the records of
    /// each partition of the driver input, which a partitioned run admits
    /// each on its own (#3792), or none for the whole input; every other
    /// source is admitted whole.
    fn admit(&mut self, driver: Option<&[usize]>) -> Result<(), Fail<E>> {
        self.poll(super::resolved::Step::Admit)?;
        functions::admit(self.plan, &*self.functions)?;
        let plan = self.plan;
        let sources: Vec<_> = plan.region_sources(RegionId::DATASET).collect();
        // A pair relation's port is filled by the run itself (LOOKUP-27).
        let expected = sources
            .len()
            .saturating_sub(1)
            .saturating_sub(plan.pair_ports());
        if self.primary.schema() != &plan.sources()[sources[0].index()].schema
            || self.secondary.len() != expected
            || self
                .secondary
                .iter()
                .zip(&sources[1..])
                .any(|(table, slot)| table.schema() != &plan.sources()[slot.index()].schema)
        {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        let count = self.secondary.len().checked_add(1);
        if count.is_none_or(|count| count > Limits::SOURCE_OPERANDS) {
            return Err(Box::new(ExecutionError::Limit {
                resource: Resource::SourceCount,
                limit: Limits::SOURCE_OPERANDS,
                required: count,
            }));
        }
        let width = self.primary.schema().columns().len();
        let whole;
        let driver = match driver {
            Some(driver) => driver,
            None => {
                whole = [self.primary.row_count()];
                &whole
            }
        };
        let primary = driver.iter().map(|&rows| (0, rows, width));
        let secondary = self.secondary.iter().enumerate().map(|(index, source)| {
            (
                index + 1,
                source.row_count(),
                source.schema().columns().len(),
            )
        });
        for (source_index, rows, columns) in primary.chain(secondary) {
            if let Err(error) = Limits::table(rows, columns) {
                return Err(Box::new(ExecutionError::SourceLimit {
                    resource: match error.resource {
                        "columns" => Resource::SourceColumns,
                        "table_rows" => Resource::SourceRows,
                        "table_cells" => Resource::SourceCells,
                        _ => unreachable!("table boundary resource"),
                    },
                    limit: error.limit,
                    required: error.actual,
                    source_index,
                }));
            }
        }
        self.check_capacity(RegionId::DATASET, 0, 0)
    }

    /// The secondary inputs, as the tables a read addresses.
    fn inputs(&self) -> Vec<Table<'a, E>> {
        self.secondary
            .iter()
            .map(|&table| Table::Source(table))
            .collect()
    }

    fn execute_admitted(
        &mut self,
        records: &mut Vec<CheckRecord>,
        templates: &mut Vec<TemplateRows>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Fail<E>> {
        let width = self.plan.region(RegionId::DATASET).output.columns().len();
        let mut frame = Frame::new(width);
        let inputs = self.inputs();
        self.poll(super::resolved::Step::VerifyIntermediates { self_rows: false })?;
        // Every pair relation is materialized before any intermediate is
        // verified and any row is built, and is read through its port, after
        // the inputs' (LOOKUP-27).
        let ports = self.pair_ports(&frame, &inputs)?;
        let mut secondary: Vec<Table<'_, E>> = inputs.clone();
        secondary.extend(ports.iter().map(|(records, schema)| Table::Rows {
            frame: records,
            schema,
            start: 0,
            rows: records.rows(),
        }));
        self.derive(&mut frame, &secondary, records, templates)?;
        self.finish(frame, &secondary, records, observer)
    }

    /// Build every candidate of the dataset: verify the intermediates over
    /// the inputs, build the rows of each template or of the key grain, then
    /// complete the column phase.
    fn derive<'s>(
        &mut self,
        frame: &mut Frame,
        secondary: &[Table<'s, E>],
        records: &mut Vec<CheckRecord>,
        templates: &mut Vec<TemplateRows>,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        let region = RegionId::DATASET;
        // A partitioned run verifies the intermediates over its other inputs
        // once, in its first partition's run (#3830).
        if self.marks.as_ref().is_none_or(|marks| marks.first) {
            self.verify_intermediates(frame, secondary, false, records)?;
        }
        let template_ids: Vec<TemplateId> = plan.region_templates(region).collect();
        let keyed = template_ids.first().is_some_and(|&template| {
            plan.template(template).driver.grain == super::resolved::Grain::Keys
        });
        if keyed {
            keygrain::construct(self, frame, secondary)?;
            templates.push(TemplateRows {
                emitted: frame.rows(),
                read: self.primary.row_count(),
            });
            self.completed_template();
        } else {
            let mut next_identity = 0usize;
            for (index, &template) in template_ids.iter().enumerate() {
                self.process_template(
                    (index, template),
                    frame,
                    secondary,
                    &mut next_identity,
                    templates,
                    records,
                )?;
            }
        }
        self.complete_columns(frame, secondary, records)
    }

    /// Check the completed candidates, their output keys and the dataset
    /// checks, then order them into the dataset.
    fn finish<'s>(
        &mut self,
        frame: Frame,
        secondary: &[Table<'s, E>],
        records: &mut Vec<CheckRecord>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        // A selector hidden by an empty input or a lazy branch must still report
        // its known filter-type refusal before a successful result can publish.
        relations::check_unobserved_filter_types(plan)?;
        checks::verify_output(self, &frame, secondary, records, observer)?;
        self.poll(super::resolved::Step::Order)?;
        let order = checks::output_order(plan, &frame)?;
        let rows = order.len();
        // The frame becomes the dataset a column at a time, so the run never
        // holds its candidates and its output whole at once (#3870).
        let columns = reserved(frame.into_columns(&order))?;
        Ok(Execution {
            dataset: Dataset::from_columns(
                plan.region(RegionId::DATASET).output.clone(),
                columns,
                rows,
            ),
            verifications: std::mem::take(records),
            partition_rows: self.output_partition,
        })
    }

    /// The records of each pair relation's port, in port order: each pair
    /// relation materialized once over the complete inputs `inputs`, its
    /// exposed fields copied (LOOKUP-27).
    fn pair_ports(
        &mut self,
        frame: &Frame,
        inputs: &[Table<'a, E>],
    ) -> Result<Vec<(Frame, TableSchema)>, Fail<E>> {
        let plan = self.plan;
        let mut ports = Vec::new();
        for relation in plan.region_relations(RegionId::DATASET) {
            let Some(item) = plan.relation(relation).named.as_deref() else {
                continue;
            };
            let Some(join) = item.source.pairs() else {
                continue;
            };
            let cx = self.probe_cx(frame, RegionId::DATASET, inputs);
            let built = pairs::materialize(self, &cx, relation)?;
            let columns = item.columns.as_deref().unwrap_or_default();
            let records = pairs::port_records(&built, columns, &join.fields)?;
            ports.push((join.port, records, join.fields.clone()));
        }
        ports.sort_by_key(|(port, ..)| *port);
        // The ports follow the inputs' in declaration order.
        if ports
            .iter()
            .enumerate()
            .any(|(offset, (port, ..))| *port != inputs.len() + offset)
        {
            return Err(Box::new(ExecutionError::SchemaMismatch));
        }
        Ok(ports
            .into_iter()
            .map(|(_, records, schema)| (records, schema))
            .collect())
    }

    /// Reject cardinality overflow and excessive output slots before deriving
    /// rows: the whole output, or each of its partitions in a run that admits
    /// it a partition at a time (D264).
    fn check_capacity(
        &mut self,
        region: RegionId,
        current: usize,
        additional: usize,
    ) -> Result<(), Fail<E>> {
        let rows = current.checked_add(additional).ok_or_else(|| {
            Box::new(ExecutionError::Limit {
                resource: Resource::OutputRows,
                limit: Limits::TABLE_ROWS,
                required: None,
            })
        })?;
        // A partition's run of an output admitted whole records what it
        // would hold; the run checks every partition's rows added up, as the
        // whole-table run checks them (#3830).
        if let (Some(marks), None) = (self.marks.as_mut(), self.output_partition) {
            marks.capacity(rows);
            return Ok(());
        }
        let width = self.plan.region(region).output.columns().len();
        Self::capacity(rows, self.output_partition, width)
    }

    /// Whether an output of `rows` rows of `width` columns, admitted in
    /// partitions of at most `partition` rows or whole, is within the limits.
    pub(super) fn capacity(
        rows: usize,
        partition: Option<std::num::NonZeroUsize>,
        width: usize,
    ) -> Result<(), Fail<E>> {
        Limits::partitioned(rows, partition, width).map_err(|error| {
            if error.resource == "columns" {
                return Box::new(ExecutionError::ScopedLimit {
                    resource: Resource::OutputColumns,
                    limit: error.limit,
                    required: error.actual,
                    spec_paths: vec!["output.columns".into()],
                });
            }
            Box::new(ExecutionError::Limit {
                resource: match error.resource {
                    "table_rows" => Resource::OutputRows,
                    "table_cells" => Resource::OutputCells,
                    _ => unreachable!("table boundary resource"),
                },
                limit: error.limit,
                required: error.actual,
            })
        })
    }

    /// The steps of one template's row stage: its prewindow bindings and filter.
    fn row_stage_callbacks(&self, template: TemplateId) -> bool {
        let plan = self.plan;
        plan.region_schedule(plan.template(template).region)
            .iter()
            .filter(|info| match info.step {
                super::resolved::Step::Row(binding) => {
                    plan.binding(binding).templates.contains(&template)
                }
                super::resolved::Step::Filter(owner) => owner == template,
                super::resolved::Step::Driver(owner) => owner == template,
                _ => false,
            })
            .any(|info| info.effects == Effects::Callbacks)
    }

    fn step_callbacks(&self, region: RegionId, wanted: super::resolved::Step) -> bool {
        self.plan
            .region_schedule(region)
            .iter()
            .any(|info| info.step == wanted && info.effects == Effects::Callbacks)
    }

    /// Build the candidates of one row template: evaluate its prewindow
    /// bindings, apply its filter, complete its window stage and post-window
    /// filter, then publish its SELF rows and verify the SELF intermediates,
    /// a warning's record joining `records`.
    ///
    /// A partition's run (#3830) marks each part of the template whose
    /// failures the whole-table run orders apart: its driver step, its row
    /// stage, each window binding's preparation and evaluation, its
    /// post-window filter and its SELF verification.
    fn process_template<'s>(
        &mut self,
        (index, template): (usize, TemplateId),
        frame: &mut Frame,
        secondary: &[Table<'s, E>],
        next_identity: &mut usize,
        templates: &mut Vec<TemplateRows>,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        if !self.builds(template) {
            return self.pass_template(template, frame.rows(), templates);
        }
        let node = plan.template(template);
        let region = node.region;
        let first = frame.rows();
        // The driver is the template's first step.
        self.mark(partitions::Coordinates::Frame)?;
        self.poll(super::resolved::Step::Driver(template))?;
        let driver = relations::driver(self, template, frame, secondary)?;
        let table = match &driver.materialized {
            Some(data) => data.table(),
            None => self.source_table(region, node.driver.source, secondary),
        };
        let read = driver.eligible.as_ref().map_or(table.row_count(), Vec::len);
        let groups: Vec<Vec<usize>> = match &node.driver.grain {
            super::resolved::Grain::Records => {
                let rows = driver.eligible.as_ref().map_or(table.row_count(), Vec::len);
                self.check_capacity(region, frame.rows(), rows)?;
                match &driver.eligible {
                    Some(eligible) => eligible.iter().map(|&row| vec![row]).collect(),
                    None => (0..table.row_count()).map(|row| vec![row]).collect(),
                }
            }
            super::resolved::Grain::Groups(fields) => {
                let fields: Vec<usize> = fields.iter().map(|&field| field as usize).collect();
                match &driver.eligible {
                    Some(eligible) => relations::driver_groups(table, eligible, &fields)?,
                    None => crate::table_grouping::partition(&table, &fields)
                        .map_err(|error| Box::new(ExecutionError::Grouping(error)))?,
                }
            }
            super::resolved::Grain::Keys => {
                unreachable!("key mode is admitted only as the sole template")
            }
        };
        self.check_capacity(region, frame.rows(), groups.len())?;
        let identities: Vec<usize> = (0..groups.len())
            .map(|offset| {
                next_identity
                    .checked_add(offset)
                    .ok_or_else(|| Box::new(ExecutionError::Capacity))
            })
            .collect::<Result<_, _>>()?;
        *next_identity = next_identity
            .checked_add(groups.len())
            .ok_or_else(|| Box::new(ExecutionError::Capacity))?;
        reserved(frame.try_reserve(groups.len(), groups.iter().map(Vec::len).sum()))?;
        let prewindow: Vec<BindingId> = plan
            .row_bindings(template)
            .filter(|&binding| plan.binding(binding).stage == Stage::Prewindow)
            .collect();
        let filter = node
            .filter
            .filter(|filter| filter.placement == super::resolved::FilterPlacement::Prewindow);
        let callbacks = self.row_stage_callbacks(template);
        // The candidates, one unit.
        self.mark(partitions::Coordinates::Frame)?;
        let start = frame.rows();
        let unit = |run: &mut Self, frame: &mut Frame, row_mode: bool| -> Result<(), Fail<E>> {
            let cx_template = index as u32;
            if row_mode {
                for (members, &identity) in groups.iter().zip(&identities) {
                    let row = frame.push(cx_template, Identity::Candidate(identity), members);
                    for &binding in &prewindow {
                        run.complete_binding(
                            frame,
                            binding,
                            (table, secondary),
                            first,
                            &[row],
                            None,
                        )?;
                    }
                    if let Some(filter) = filter {
                        let cx = run.template_cx(frame, region, table, secondary, first);
                        let truth = run.filter_truth(&cx, filter.expr, &[row])?;
                        if truth[0] != crate::expression::Truth::True {
                            frame.truncate(row);
                        }
                    }
                }
            } else {
                frame.extend(cx_template, groups.len(), |index, records| {
                    records.extend_from_slice(&groups[index]);
                    Identity::Candidate(identities[index])
                });
                let rows: Vec<usize> = (start..frame.rows()).collect();
                for &binding in &prewindow {
                    run.complete_binding(frame, binding, (table, secondary), first, &rows, None)?;
                }
                if let Some(filter) = filter {
                    let cx = run.template_cx(frame, region, table, secondary, first);
                    let truths = run.filter_truth(&cx, filter.expr, &rows)?;
                    let keep: Vec<bool> = truths
                        .iter()
                        .map(|truth| *truth == crate::expression::Truth::True)
                        .collect();
                    reserved(frame.retain_from(start, &keep))?;
                }
            }
            Ok(())
        };
        self.run_unit(frame, Footprint::Rows(start), callbacks, unit)?;
        // The window stage, one binding at a time over the template's rows.
        let windowed: Vec<BindingId> = plan
            .row_bindings(template)
            .filter(|&binding| plan.binding(binding).stage == Stage::Window)
            .collect();
        let rows: Vec<usize> = (first..frame.rows()).collect();
        // The window stage adds and removes no row, and its bindings share
        // the groupings of the columns no later binding writes.
        self.windows = windows::Runs::default();
        for binding in windowed {
            let step = super::resolved::Step::Window(binding);
            let callbacks = self.step_callbacks(region, step);
            let column = self.local_column(region, plan.binding(binding).column);
            let output = first..frame.rows();
            let columns = [column];
            let footprint = Footprint::Cells {
                columns: &columns,
                rows: &rows,
            };
            self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                // The whole-table run prepares the window over every
                // partition's rows before it evaluates any (#3830).
                run.mark(partitions::Coordinates::Window(index))?;
                run.windows.step();
                let mut cx = run.template_cx(frame, region, table, secondary, first);
                cx.output = output.clone();
                windows::prepare(run, &cx, plan.binding(binding).expr)?;
                run.mark(partitions::Coordinates::Frame)?;
                if row_mode {
                    for &row in &rows {
                        run.complete_binding(
                            frame,
                            binding,
                            (table, secondary),
                            first,
                            &[row],
                            Some(output.clone()),
                        )?;
                    }
                    Ok(())
                } else {
                    run.complete_binding(
                        frame,
                        binding,
                        (table, secondary),
                        first,
                        &rows,
                        Some(output.clone()),
                    )
                }
            })?;
        }
        if let Some(filter) = node
            .filter
            .filter(|filter| filter.placement == super::resolved::FilterPlacement::Postwindow)
        {
            self.mark(partitions::Coordinates::Frame)?;
            let callbacks =
                self.step_callbacks(region, super::resolved::Step::PostwindowFilter(template));
            let mut keep = Vec::new();
            let footprint = Footprint::Cells {
                columns: &[],
                rows: &[],
            };
            self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                let mut cx = run.template_cx(frame, region, table, secondary, first);
                cx.output = first..frame.rows();
                let truths = if row_mode {
                    let mut truths = Vec::with_capacity(rows.len());
                    for &row in &rows {
                        truths.extend(run.filter_truth(&cx, filter.expr, &[row])?);
                    }
                    truths
                } else {
                    run.filter_truth(&cx, filter.expr, &rows)?
                };
                keep = truths
                    .iter()
                    .map(|truth| *truth == crate::expression::Truth::True)
                    .collect();
                Ok(())
            })?;
            reserved(frame.retain_from(first, &keep))?;
        }
        templates.push(TemplateRows {
            emitted: frame.rows() - first,
            read,
        });
        self.completed_template();
        self.mark(partitions::Coordinates::Frame)?;
        self.relations.publish_self(plan);
        self.poll(super::resolved::Step::VerifyIntermediates { self_rows: true })?;
        self.verify_intermediates(frame, secondary, true, records)?;
        Ok(())
    }

    /// Run one unit in batch, replaying it in row mode after a failure; a
    /// unit with callbacks runs in row mode.
    /// The footprint says which frame cells or rows the unit may change.
    fn run_unit(
        &mut self,
        frame: &mut Frame,
        footprint: Footprint<'_>,
        callbacks: bool,
        mut unit: impl FnMut(&mut Self, &mut Frame, bool) -> Result<(), Fail<E>>,
    ) -> Result<(), Fail<E>> {
        if callbacks {
            let outer = std::mem::replace(&mut self.calls, true);
            let result = unit(self, frame, true);
            self.calls = outer;
            return result;
        }
        let checkpoint = self.checkpoint();
        let result = unit(self, frame, false);
        match &result {
            Ok(()) => return Ok(()),
            // A cancelled run stops; replaying the unit would only poll again.
            Err(error) if matches!(**error, ExecutionError::Cancelled) => return result,
            Err(_) => {}
        }
        self.rollback(checkpoint);
        match footprint {
            Footprint::Rows(first) => frame.truncate(first),
            Footprint::Cells { columns, rows } => {
                for &row in rows {
                    for &column in columns {
                        frame.reset(row, column);
                    }
                }
            }
        }
        unit(self, frame, true)
    }

    /// The evaluation context of a template's rows.
    fn template_cx<'f>(
        &self,
        frame: &'f Frame,
        region: RegionId,
        table: Table<'f, E>,
        secondary: &'f [Table<'f, E>],
        first: usize,
    ) -> Cx<'f, E>
    where
        'a: 'f,
        'p: 'f,
    {
        Cx {
            region,
            schema: &self.plan.region(region).output,
            frame,
            table,
            primary: Table::Source(self.primary),
            secondary,
            self_rows: first,
            output: 0..0,
            operands: None,
            record: None,
        }
    }

    /// The source table of a region's source slot.
    fn source_table<'s>(
        &self,
        region: RegionId,
        source: super::resolved::SourceId,
        secondary: &[Table<'s, E>],
    ) -> Table<'s, E>
    where
        'a: 's,
    {
        let first = self
            .plan
            .region_sources(region)
            .next()
            .expect("a region has a driver source");
        match source.index() - first.index() {
            0 => Table::Source(self.primary),
            index => secondary[index - 1],
        }
    }

    /// Evaluate one binding over `rows` and complete its cells with the
    /// converted results. Row-stage reads see the rows of earlier templates
    /// as SELF donors; a window stage passes the rows its windows range over.
    fn complete_binding<'t>(
        &mut self,
        frame: &mut Frame,
        binding: BindingId,
        (table, secondary): (Table<'t, E>, &[Table<'t, E>]),
        first: usize,
        rows: &[usize],
        output: Option<Range<usize>>,
    ) -> Result<(), Fail<E>> {
        let plan = self.plan;
        let bound = plan.binding(binding);
        let region = plan.template(bound.templates[0]).region;
        let values = {
            let mut cx = self.template_cx(frame, region, table, secondary, first);
            if let Some(output) = output.clone() {
                cx.output = output;
            }
            self.evaluate_binding(&cx, binding, rows)?
        };
        let column = {
            let cx = self.template_cx(frame, region, table, secondary, first);
            cx.local(plan, bound.column)
        };
        frame.complete_rows(rows, column, values);
        self.windows.written(column);
        Ok(())
    }

    /// One binding over `rows`, converted to its column's type.
    fn evaluate_binding(
        &mut self,
        cx: &Cx<'_, E>,
        binding: BindingId,
        rows: &[usize],
    ) -> Result<Values, Fail<E>> {
        let plan = self.plan;
        let bound = plan.binding(binding);
        let raw = if let Some(values) = windows::direct(self, cx, bound.expr, rows)? {
            values
        } else {
            self.eval(cx, bound.expr, rows, true)?
        };
        scalar::finish(self, cx, binding, raw, rows)
    }

    /// The truth of a template filter for each row.
    fn filter_truth(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
    ) -> Result<Vec<crate::expression::Truth>, Fail<E>> {
        predicates::filter(self, cx, expr, rows)
    }

    /// Complete the column phase in the compiler's order, checking each
    /// available declared prefix of column checks in order.
    ///
    /// A partition's run (#3830) completes each binding over one template's
    /// rows after another, each a unit of its own, as the whole-table run's
    /// failures order them: by column, binding and template, then partition
    /// and row. It marks where the column checks would run instead of running
    /// them, and the run checks every partition's candidates there once.
    fn complete_columns<'s>(
        &mut self,
        frame: &mut Frame,
        secondary: &[Table<'s, E>],
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        let region = RegionId::DATASET;
        self.relations.begin_columns(plan);
        // The column phase adds and removes no row, and its steps share the
        // groupings of the columns no later step writes.
        self.windows = windows::Runs::default();
        let columns: Vec<ColumnId> = plan.region_columns(region).collect();
        let mut completed = row_completed(plan);
        let mut next = 0;
        self.check_columns(frame, (&completed, None), &mut next, records)?;
        let order = plan.column_order(region).to_vec();
        for column in order {
            let local = column.index() - columns[0].index();
            let step = super::resolved::Step::Column(column);
            self.mark(partitions::Coordinates::Frame)?;
            self.poll(step)?;
            let callbacks = self.step_callbacks(region, step);
            let rows: Vec<usize> = (0..frame.rows())
                .filter(|&row| !frame.is_completed(row, local))
                .collect();
            // A column's bindings run in turn, each over the incomplete rows
            // of its own templates (#2829); a key-grain candidate belongs to
            // the sole key template.
            let templates: Vec<TemplateId> = plan.region_templates(region).collect();
            let bindings: Vec<(BindingId, Vec<usize>)> = plan
                .column_bindings(column)
                .map(|binding| {
                    let serves = &plan.binding(binding).templates;
                    let served = rows
                        .iter()
                        .copied()
                        .filter(|&row| {
                            serves.contains(&templates[frame.template(row).unwrap_or(0)])
                        })
                        .collect();
                    (binding, served)
                })
                .collect();
            let columns = [local];
            if self.marks.is_some() {
                self.windows.step();
                let mut prepared = None;
                for (binding, served) in &bindings {
                    let expr = plan.binding(*binding).expr;
                    let mut prepare = prepared != Some(expr);
                    prepared = Some(expr);
                    let mut serves = plan.binding(*binding).templates.clone();
                    serves.sort_unstable();
                    for template in serves {
                        let unit_rows: Vec<usize> = served
                            .iter()
                            .copied()
                            .filter(|&row| templates[frame.template(row).unwrap_or(0)] == template)
                            .collect();
                        // The binding's first unit prepares its expression,
                        // over every candidate, as its own mark.
                        let first_unit = std::mem::replace(&mut prepare, false);
                        let footprint = Footprint::Cells {
                            columns: &columns,
                            rows: &unit_rows,
                        };
                        self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                            if first_unit {
                                run.mark(partitions::Coordinates::Frame)?;
                                run.complete_column(frame, *binding, secondary, &[], true)?;
                            }
                            run.mark(partitions::Coordinates::Frame)?;
                            if row_mode {
                                for &row in &unit_rows {
                                    run.complete_column(frame, *binding, secondary, &[row], false)?;
                                }
                                Ok(())
                            } else {
                                run.complete_column(frame, *binding, secondary, &unit_rows, false)
                            }
                        })?;
                    }
                }
            } else {
                let footprint = Footprint::Cells {
                    columns: &columns,
                    rows: &rows,
                };
                self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                    run.windows.step();
                    // Bindings split from one column binding share its
                    // expression, and a column-level window over it is
                    // prepared once, through the first one's driver.
                    let mut prepared = None;
                    for (binding, served) in &bindings {
                        let expr = plan.binding(*binding).expr;
                        let prepare = prepared != Some(expr);
                        prepared = Some(expr);
                        if row_mode {
                            run.complete_column(frame, *binding, secondary, &[], prepare)?;
                            for &row in served {
                                run.complete_column(frame, *binding, secondary, &[row], false)?;
                            }
                        } else {
                            run.complete_column(frame, *binding, secondary, served, prepare)?;
                        }
                    }
                    Ok(())
                })?;
            }
            completed[local] = true;
            self.check_columns(frame, (&completed, Some(local)), &mut next, records)?;
        }
        Ok(())
    }

    /// Check the available declared prefix of column checks
    /// (`checks::check_columns`), `completed` the columns complete and the
    /// column just completed, if any. A partition's run records where it
    /// would check instead (#3830).
    fn check_columns(
        &mut self,
        frame: &Frame,
        (completed, column): (&[bool], Option<usize>),
        next: &mut usize,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>> {
        match self.marks.as_mut() {
            Some(marks) => {
                marks.check(column);
                Ok(())
            }
            None => checks::check_columns(self, frame, completed, next, records),
        }
    }

    /// One column-phase binding over `rows`, through its driver's port: a
    /// row's own template's driver, or the primary for a whole-dataset
    /// binding. Its first call prepares a direct window over every candidate
    /// before any row, even for no rows.
    fn complete_column<'s>(
        &mut self,
        frame: &mut Frame,
        binding: BindingId,
        secondary: &[Table<'s, E>],
        rows: &[usize],
        prepare: bool,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        let plan = self.plan;
        let bound = plan.binding(binding);
        let region = RegionId::DATASET;
        let values = {
            let table = self.source_table(region, bound.driver.source, secondary);
            let mut cx = self.template_cx(frame, region, table, secondary, frame.rows());
            cx.output = 0..frame.rows();
            if prepare {
                windows::prepare(self, &cx, bound.expr)?;
            }
            if rows.is_empty() {
                return Ok(());
            }
            self.evaluate_binding(&cx, binding, rows)?
        };
        let local = self.local_column(region, bound.column);
        frame.complete_rows(rows, local, values);
        self.windows.written(local);
        Ok(())
    }

    /// The region-local position of a column slot.
    fn local_column(&self, region: RegionId, column: ColumnId) -> usize {
        column.index()
            - self
                .plan
                .region_columns(region)
                .next()
                .map_or(0, |first| first.index())
    }

    /// Intermediate `unique` verifications: input-backed ones before any row,
    /// SELF ones after each completed template.
    fn verify_intermediates<'s>(
        &mut self,
        frame: &Frame,
        secondary: &[Table<'s, E>],
        self_rows: bool,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>>
    where
        'a: 's,
    {
        relations::verify(self, frame, secondary, self_rows, records)
    }
}

/// The frame cells or rows a unit may change, restored before it replays.
enum Footprint<'r> {
    /// Rows appended from this row on.
    Rows(usize),
    /// Some columns' cells on these rows, incomplete before the unit.
    Cells {
        columns: &'r [usize],
        rows: &'r [usize],
    },
}

/// The output columns every template completes in its row stage, by their
/// place in the region: the columns the column phase finds complete.
fn row_completed(plan: &ResolvedPlan) -> Vec<bool> {
    let region = RegionId::DATASET;
    let columns: Vec<ColumnId> = plan.region_columns(region).collect();
    let mut completed = vec![true; columns.len()];
    for template in plan.region_templates(region) {
        let mut present = vec![false; columns.len()];
        for binding in plan.row_bindings(template) {
            let local = plan.binding(binding).column.index() - columns[0].index();
            present[local] = true;
        }
        for (column, done) in completed.iter_mut().enumerate() {
            *done &= present[column];
        }
    }
    completed
}
