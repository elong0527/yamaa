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
use crate::table::{CellError, TableAccess, TableSchema, ValueRef};
use crate::value::Value;
use std::ops::Range;

mod aggregates;
mod buffer;
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
mod partitions;
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
}

impl<'a, E> Table<'a, E> {
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
    /// The partitions of a partitioned run's driver input (#3697).
    pub partitions: Option<partitions::Partitions>,
    /// The most rows one partition of the output holds, once admission has
    /// passed, for a partitioned run that divides every step (D264); none
    /// while the output is admitted whole.
    pub output_partition: Option<std::num::NonZeroUsize>,
}

/// What a batch unit restores before it replays in row mode.
struct Checkpoint {
    handlers: HandlerMark,
    relations: relations::Mark,
}

impl<'p, 'a, E> Run<'p, 'a, E> {
    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            handlers: self.handlers.mark(),
            relations: self.relations.mark(),
        }
    }

    fn rollback(&mut self, checkpoint: Checkpoint) {
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
        self.attempt(&primary, None, secondary, functions, observer, cancelled)
    }

    /// One attempt over the driver input `primary`, whole or divided into
    /// `partitions` (#3697).
    fn attempt<E>(
        &self,
        primary: &dyn TableAccess<Error = E>,
        partitions: Option<partitions::Partitions>,
        secondary: &[&dyn TableAccess<Error = E>],
        functions: &mut dyn FunctionPort<Error = E>,
        observer: Option<&mut dyn FnMut(ExecutionEvent)>,
        cancelled: &dyn Fn() -> bool,
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
        let mut run = Run {
            plan: self,
            primary,
            secondary,
            functions,
            handlers: Handlers::new(self),
            relations: relations::State::new(self, RegionId::DATASET),
            windows: windows::Runs::default(),
            calls: false,
            ports: index::Ports::default(),
            cancelled,
            stopped: std::cell::Cell::new(false),
            partitions,
            output_partition: None,
        };
        let mut records = Vec::new();
        let mut templates = Vec::new();
        let mut result = run.execute_inner(&mut records, &mut templates, &mut observer);
        // A poll whose failure a lookup's probe swallowed still cancels the
        // run, and a cancelled run keeps nothing it built.
        let cancelled = run.stopped.get()
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
                handler_counts: run.handlers.snapshot(self),
                retained_verifications: records,
                template_rows: templates,
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
        self.poll(super::resolved::Step::Admit)?;
        functions::admit(self.plan, &*self.functions)?;
        let plan = self.plan;
        let sources: Vec<_> = plan.region_sources(RegionId::DATASET).collect();
        let expected = sources.len().saturating_sub(1);
        if self.primary.schema() != &plan.sources()[sources[0].index()].schema
            || self
                .partitions
                .as_ref()
                .is_some_and(|partitions| partitions.mismatched)
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
        // A partitioned run admits each partition of its driver input on its
        // own (#3792); every other source, and an undivided driver, whole.
        let width = self.primary.schema().columns().len();
        let primary = self
            .driver_admissions()
            .into_iter()
            .map(|rows| (0, rows, width));
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
        self.check_capacity(RegionId::DATASET, 0, 0)?;
        // A partitioned run divides only what the inventory lets it divide
        // under its partitions; a step that needs the whole table refuses it
        // before any record is read, unless the run falls back to the whole
        // table (#3697, #3763).
        if let Some(partitions) = self
            .partitions
            .as_ref()
            .filter(|partitions| !partitions.fallback)
        {
            if let Some(step) = plan.whole_table_step(partitions.keys) {
                return Err(Box::new(ExecutionError::PartitionGlobal {
                    path: plan.step_path(step).to_owned(),
                }));
            }
        }
        // Admitted: a key-bounded run cuts its input now (#3763), and a run
        // that divides every step admits its output a partition at a time
        // from here on (D264).
        self.cut_by_keys();
        self.output_partition = self
            .partitions
            .as_ref()
            .filter(|partitions| plan.whole_table_step(partitions.keys).is_none())
            .map(partitions::Partitions::largest);
        observer(ExecutionEvent::Exit {
            phase: ExecutionPhase::Admission,
            outcome: crate::application_observer::Outcome::Succeeded,
        });
        observer(ExecutionEvent::Phase(ExecutionPhase::Derivation));
        self.execute_admitted(records, templates, observer)
    }

    fn execute_admitted(
        &mut self,
        records: &mut Vec<CheckRecord>,
        templates: &mut Vec<TemplateRows>,
        observer: &mut dyn FnMut(ExecutionEvent),
    ) -> Result<Execution, Fail<E>> {
        let plan = self.plan;
        let region = RegionId::DATASET;
        let width = plan.region(region).output.columns().len();
        let mut frame = Frame::new(width);
        let secondary: Vec<Table<'a, E>> = self
            .secondary
            .iter()
            .map(|&table| Table::Source(table))
            .collect();
        self.poll(super::resolved::Step::VerifyIntermediates { self_rows: false })?;
        self.verify_intermediates(&frame, &secondary, false, records)?;
        let template_ids: Vec<TemplateId> = plan.region_templates(region).collect();
        let keyed = template_ids.first().is_some_and(|&template| {
            plan.template(template).driver.grain == super::resolved::Grain::Keys
        });
        if keyed {
            keygrain::construct(self, &mut frame, &secondary)?;
            templates.push(TemplateRows {
                emitted: frame.rows(),
                read: self.primary.row_count(),
            });
        } else {
            let mut next_identity = 0usize;
            for (index, &template) in template_ids.iter().enumerate() {
                self.process_template(
                    index,
                    template,
                    &mut frame,
                    &secondary,
                    &mut next_identity,
                    templates,
                    records,
                )?;
            }
        }
        self.complete_columns(&mut frame, &secondary, records)?;
        checks::verify_output(self, &frame, &secondary, records, observer)?;
        self.poll(super::resolved::Step::Order)?;
        let order = checks::output_order(self.plan, &frame);
        let mut rows = Vec::new();
        reserved(rows.try_reserve_exact(order.len()))?;
        rows.extend(order.iter().map(|&row| frame.row_values(row)));
        Ok(Execution {
            dataset: Dataset::from_rows(plan.region(region).output.clone(), rows),
            verifications: std::mem::take(records),
            partition_rows: self.output_partition,
        })
    }

    /// Reject cardinality overflow and excessive output slots before deriving
    /// rows: the whole output, or each of its partitions in a run that admits
    /// it a partition at a time (D264).
    fn check_capacity(
        &self,
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
        let width = self.plan.region(region).output.columns().len();
        Limits::partitioned(rows, self.output_partition, width).map_err(|error| {
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
    #[allow(clippy::too_many_arguments)]
    fn process_template(
        &mut self,
        index: usize,
        template: TemplateId,
        frame: &mut Frame,
        secondary: &[Table<'a, E>],
        next_identity: &mut usize,
        templates: &mut Vec<TemplateRows>,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>> {
        let plan = self.plan;
        let node = plan.template(template);
        let region = node.region;
        let first = frame.rows();
        // The driver is the template's first step.
        self.poll(super::resolved::Step::Driver(template))?;
        let driver = relations::driver(self, template, frame, secondary)?;
        let table = match &driver.materialized {
            Some(data) => data.table(),
            None => self.source_table(region, node.driver.source, secondary),
        };
        let read = driver.eligible.as_ref().map_or(table.row_count(), Vec::len);
        // A partitioned run divides the row stage of a template that reads
        // the driver input when none of its steps needs the whole table
        // (#3697).
        let divided = self.reads_partitions(template)
            && self.divides(|step| match step {
                super::resolved::Step::Row(binding) => {
                    plan.binding(binding).templates.contains(&template)
                }
                super::resolved::Step::Filter(owner) | super::resolved::Step::Driver(owner) => {
                    owner == template
                }
                _ => false,
            });
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
                    None if divided => self.partition_groups(table, &fields)?,
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
        // The candidates of one partition after another, each a unit of its
        // own; undivided, all of them are one unit.
        let runs = self.partition_runs(divided, groups.len(), |group| {
            groups[group].first().copied()
        });
        for (number, range) in runs.into_iter().enumerate() {
            self.next_partition(number)?;
            let start = frame.rows();
            let unit_groups = &groups[range.clone()];
            let unit_identities = &identities[range];
            let unit = |run: &mut Self, frame: &mut Frame, row_mode: bool| -> Result<(), Fail<E>> {
                let cx_template = index as u32;
                if row_mode {
                    for (members, &identity) in unit_groups.iter().zip(unit_identities) {
                        let row = frame.push(cx_template, Identity::Candidate(identity), members);
                        for &binding in &prewindow {
                            run.complete_binding(
                                frame,
                                binding,
                                table,
                                secondary,
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
                    for (members, &identity) in unit_groups.iter().zip(unit_identities) {
                        frame.push(cx_template, Identity::Candidate(identity), members);
                    }
                    let rows: Vec<usize> = (start..frame.rows()).collect();
                    for &binding in &prewindow {
                        run.complete_binding(frame, binding, table, secondary, first, &rows, None)?;
                    }
                    if let Some(filter) = filter {
                        let cx = run.template_cx(frame, region, table, secondary, first);
                        let truths = run.filter_truth(&cx, filter.expr, &rows)?;
                        let keep: Vec<bool> = truths
                            .iter()
                            .map(|truth| *truth == crate::expression::Truth::True)
                            .collect();
                        frame.retain_from(start, &keep);
                    }
                }
                Ok(())
            };
            self.run_unit(frame, Footprint::Rows(start), callbacks, unit)?;
        }
        // The window stage, one binding at a time over the template's rows.
        let windowed: Vec<BindingId> = plan
            .row_bindings(template)
            .filter(|&binding| plan.binding(binding).stage == Stage::Window)
            .collect();
        let rows: Vec<usize> = (first..frame.rows()).collect();
        for binding in windowed {
            let step = super::resolved::Step::Window(binding);
            let callbacks = self.step_callbacks(region, step);
            let column = self.local_column(region, plan.binding(binding).column);
            let output = first..frame.rows();
            let columns = [column];
            // Under key-bounded partitions a window that groups by every
            // partition key reads only rows of its own partition (#3763): its
            // run is prepared once over the template's rows, then each
            // partition's rows are evaluated as a unit of their own.
            // Consecutive partitions keep the window stage one unit (#3697).
            let keyed = self
                .partitions
                .as_ref()
                .is_some_and(|partitions| partitions.keys > 0);
            let divided =
                keyed && self.reads_partitions(template) && self.divides(|other| other == step);
            let runs = self.row_runs(divided, frame, &rows);
            for (number, range) in runs.into_iter().enumerate() {
                self.next_partition(number)?;
                let unit_rows = &rows[range];
                let footprint = Footprint::Cells {
                    columns: &columns,
                    rows: unit_rows,
                };
                self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                    if number == 0 {
                        run.windows = windows::Runs::default();
                        let mut cx = run.template_cx(frame, region, table, secondary, first);
                        cx.output = output.clone();
                        windows::prepare(run, &cx, plan.binding(binding).expr)?;
                    }
                    if row_mode {
                        for &row in unit_rows {
                            run.complete_binding(
                                frame,
                                binding,
                                table,
                                secondary,
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
                            table,
                            secondary,
                            first,
                            unit_rows,
                            Some(output.clone()),
                        )
                    }
                })?;
            }
        }
        if let Some(filter) = node
            .filter
            .filter(|filter| filter.placement == super::resolved::FilterPlacement::Postwindow)
        {
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
            frame.retain_from(first, &keep);
        }
        templates.push(TemplateRows {
            emitted: frame.rows() - first,
            read,
        });
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
    fn source_table(
        &self,
        region: RegionId,
        source: super::resolved::SourceId,
        secondary: &[Table<'a, E>],
    ) -> Table<'a, E> {
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
    #[allow(clippy::too_many_arguments)]
    fn complete_binding<'t>(
        &mut self,
        frame: &mut Frame,
        binding: BindingId,
        table: Table<'t, E>,
        secondary: &[Table<'t, E>],
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
    fn complete_columns(
        &mut self,
        frame: &mut Frame,
        secondary: &[Table<'a, E>],
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>> {
        let plan = self.plan;
        let region = RegionId::DATASET;
        self.relations.begin_columns(plan);
        let columns: Vec<ColumnId> = plan.region_columns(region).collect();
        let width = columns.len();
        let mut completed = vec![true; width];
        for template in plan.region_templates(region) {
            let mut present = vec![false; width];
            for binding in plan.row_bindings(template) {
                let local = plan.binding(binding).column.index() - columns[0].index();
                present[local] = true;
            }
            for (column, done) in completed.iter_mut().enumerate() {
                *done &= present[column];
            }
        }
        let mut next = 0;
        checks::check_columns(self, frame, &completed, &mut next, records)?;
        let order = plan.column_order(region).to_vec();
        for column in order {
            let local = column.index() - columns[0].index();
            let step = super::resolved::Step::Column(column);
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
                            let template = templates[frame.template(row).unwrap_or(0)];
                            serves.contains(&template)
                        })
                        .collect();
                    (binding, served)
                })
                .collect();
            let columns = [local];
            if self.divides(|candidate| candidate == step) {
                // A partitioned run completes a partition-safe column as the
                // whole-table run does, one binding after another (#2829),
                // and each binding over one partition's rows after another,
                // each a unit of its own (#3697).
                self.windows = windows::Runs::default();
                let mut prepared = None;
                for (binding, served) in &bindings {
                    let expr = plan.binding(*binding).expr;
                    let mut prepare = prepared != Some(expr);
                    prepared = Some(expr);
                    let runs = self.row_runs(true, frame, served);
                    for (number, range) in runs.into_iter().enumerate() {
                        self.next_partition(number)?;
                        let unit_rows = &served[range];
                        // The binding's first unit prepares its expression.
                        let first_unit = std::mem::replace(&mut prepare, false);
                        let footprint = Footprint::Cells {
                            columns: &columns,
                            rows: unit_rows,
                        };
                        self.run_unit(frame, footprint, callbacks, |run, frame, row_mode| {
                            if row_mode {
                                run.complete_column(frame, *binding, secondary, &[], first_unit)?;
                                for &row in unit_rows {
                                    run.complete_column(frame, *binding, secondary, &[row], false)?;
                                }
                                Ok(())
                            } else {
                                run.complete_column(
                                    frame, *binding, secondary, unit_rows, first_unit,
                                )
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
                    run.windows = windows::Runs::default();
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
            checks::check_columns(self, frame, &completed, &mut next, records)?;
        }
        // A selector hidden by an empty input or a lazy branch must still report
        // its known filter-type refusal before a successful result can publish.
        relations::check_unobserved_filter_types(plan)?;
        Ok(())
    }

    /// One column-phase binding over `rows`, through its driver's port: a
    /// row's own template's driver, or the primary for a whole-dataset
    /// binding. Its first call prepares a direct window over every candidate
    /// before any row, even for no rows.
    fn complete_column(
        &mut self,
        frame: &mut Frame,
        binding: BindingId,
        secondary: &[Table<'a, E>],
        rows: &[usize],
        prepare: bool,
    ) -> Result<(), Fail<E>> {
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
    fn verify_intermediates(
        &mut self,
        frame: &Frame,
        secondary: &[Table<'a, E>],
        self_rows: bool,
        records: &mut Vec<CheckRecord>,
    ) -> Result<(), Fail<E>> {
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
