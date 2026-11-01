//! Windows over completed rows: one run per window and unit, partitioned
//! once. A batch completes every partition before it writes any answer; a
//! row-mode replay completes each partition when its first row is read.
use super::buffer::{Bits, Values};
use super::frame::{Frame, IdentityMap};
use super::group::Grouping;
use super::{Cx, Fail, Run, Table};
use crate::dataset::execution::{ExecutionError, Resource};
use crate::dataset::resolved::{ExprId, ExprKind, Op, WindowId};
use crate::dataset::{Window, WindowKind};
use crate::expression::Truth;
use crate::limits::Limits;
use crate::table::{TableAccess, TableSchema, ValueRef};
use std::cmp::Ordering;
use std::ops::Range;
use std::sync::Arc;

/// The window runs of the unit being evaluated.
#[derive(Default)]
pub(super) struct Runs {
    runs: IdentityMap<WindowId, WindowRun>,
    /// Output aggregates' indexes and filter answers, kept for the unit as
    /// window runs are: the frame values they read do not change in one.
    pub(super) outputs: super::aggregates::Outputs,
    /// The groupings of frame rows the windows and output aggregates share,
    /// one per key list and range of rows: the unit's, and in a window stage
    /// or the column phase, every later unit's whose key columns have not
    /// changed since.
    groupings: Vec<Shared>,
}

/// One grouping of frame rows `rows` by the frame columns `columns`.
struct Shared {
    columns: Vec<usize>,
    rows: Range<usize>,
    grouping: Arc<Grouping>,
}

impl Runs {
    /// The state of a new unit of the window stage or the column phase: no
    /// window run or output answer yet, and the groupings of the units
    /// before it, whose frame rows are the same and whose key columns no
    /// unit has written since.
    pub(super) fn step(&mut self) {
        let groupings = std::mem::take(&mut self.groupings);
        *self = Self {
            groupings,
            ..Self::default()
        };
    }

    /// Frame column `column` was written: no grouping by it stays.
    pub(super) fn written(&mut self, column: usize) {
        self.groupings
            .retain(|shared| !shared.columns.contains(&column));
    }

    /// The grouping of frame rows `rows` by the frame columns `columns`
    /// under grouping equality, items numbered from `rows.start`: built on
    /// first use, while the rows and key columns do not change. A grouping
    /// of the same rows by more keys, these among them, is merged into it
    /// rather than reading every row again.
    pub(super) fn grouping(
        &mut self,
        frame: &Frame,
        columns: &[usize],
        rows: Range<usize>,
    ) -> Result<Arc<Grouping>, std::collections::TryReserveError> {
        if let Some(shared) = self
            .groupings
            .iter()
            .find(|shared| shared.rows == rows && shared.columns == columns)
        {
            return Ok(Arc::clone(&shared.grouping));
        }
        let start = rows.start;
        let finer = self.groupings.iter().find(|shared| {
            shared.rows == rows && columns.iter().all(|column| shared.columns.contains(column))
        });
        let keys: Vec<&Values> = columns.iter().map(|&column| frame.column(column)).collect();
        let built = if columns.is_empty() {
            Grouping::whole(rows.len())
        } else if let Some(finer) = finer {
            // A key every row of the finer grouping holds alike merges
            // nothing apart.
            let varying: Vec<&Values> = columns
                .iter()
                .zip(&keys)
                .filter(|(column, _)| {
                    !finer
                        .grouping
                        .constant
                        .iter()
                        .any(|&position| finer.columns[position] == **column)
                })
                .map(|(_, &key)| key)
                .collect();
            if varying.is_empty() && !rows.is_empty() {
                Grouping::whole(rows.len())
            } else {
                finer
                    .grouping
                    .coarsen(varying.len(), |item, key| varying[key].get(start + item))
            }
        } else {
            Grouping::runs(&keys, rows.clone())
        };
        let grouping = Arc::new(built?);
        self.groupings.push(Shared {
            columns: columns.to_vec(),
            rows,
            grouping: Arc::clone(&grouping),
        });
        Ok(grouping)
    }
}

/// The answer of each position of a window's scope, one word each, written
/// a partition at a time and read by the window's kind: never a cloned
/// payload. A numbering kind holds the number, a row value or `locf` its
/// donor position plus one, and a baseline [`FLAG`] or, on every eligible
/// row of an ambiguous partition, its latest row plus two. Zero is no
/// answer: a missing value.
#[derive(Default)]
struct Answers {
    words: Vec<usize>,
    /// How many eligible rows of an ambiguous baseline partition share its
    /// latest date, by that latest row.
    ambiguous: IdentityMap<usize, usize>,
}

/// A baseline's flagged row.
const FLAG: usize = 1;

/// The member lists one partition is numbered with, kept from one partition
/// to the next.
#[derive(Default)]
struct Scratch {
    members: Vec<usize>,
    eligible: Vec<usize>,
}

/// One window over the rows of its scope: frame rows `start..start + len`,
/// positions index them. A context window appends the collected driver
/// fields after the output columns.
struct WindowRun {
    window: Window,
    start: usize,
    width: usize,
    /// A context window's collected driver fields, one buffer per field,
    /// indexed by position.
    collected: Vec<Values>,
    schema: Option<TableSchema>,
    grouping: Arc<Grouping>,
    /// Whether each group's partition is complete, and how many are not.
    completed: Vec<bool>,
    remaining: usize,
    /// The filter's truth at each position, from its column program; none
    /// when there is no filter or a position evaluates through the row
    /// evaluator.
    truths: Option<Vec<Truth>>,
    /// Whether every two values of each order term compare across the whole
    /// scope, so no partition's sort can fail and none checks again.
    admitted: bool,
    answers: Answers,
    /// How each group's partition stands to the order terms, found for
    /// every group when the first partition is completed ([`standings`]).
    standings: Option<Vec<Standing>>,
    scratch: Scratch,
}

impl WindowRun {
    /// The buffer holding a column of the scope, and the index of its first
    /// position in it: a frame column, or a collected driver field.
    fn lane<'f>(&'f self, cx: &'f Cx<'_, impl Sized>, column: usize) -> (&'f Values, usize) {
        if column < self.width {
            (cx.frame.column(column), self.start)
        } else {
            (&self.collected[column - self.width], 0)
        }
    }

    fn get<'f>(
        &'f self,
        cx: &'f Cx<'_, impl Sized>,
        position: usize,
        column: usize,
    ) -> ValueRef<'f> {
        let (values, first) = self.lane(cx, column);
        values.get(first + position)
    }
}

/// The window payload of a window node, and its assignment's column.
fn payload(
    plan: &crate::dataset::resolved::ResolvedPlan,
    expr: ExprId,
) -> (&Window, Option<&crate::dataset::ContextWindow>, usize) {
    match &plan.expr(expr).op {
        Op::Window { window, column } => (window, None, *column),
        Op::ContextWindow { context, column } => (&context.window, Some(context), *column),
        _ => unreachable!("a window node holds its window"),
    }
}

fn window_id(plan: &crate::dataset::resolved::ResolvedPlan, expr: ExprId) -> Option<WindowId> {
    match plan.expr(expr).kind {
        ExprKind::Window(window) | ExprKind::ContextWindow(window) => Some(window),
        _ => None,
    }
}

/// Partition the scope once, before any row's result is converted. A step
/// whose binding is a window builds it before its first row.
pub(super) fn prepare<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
) -> Result<(), Fail<E>> {
    let Some(id) = window_id(run.plan, expr) else {
        return Ok(());
    };
    let built = build(run, cx, expr)?;
    run.windows.runs.insert(id, built);
    Ok(())
}

/// A context window's driver fields at every position of its scope, in
/// position order and, at each position, in field order, as the row
/// evaluator reads them; each field's readings in one buffer.
fn collect<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    context: &crate::dataset::ContextWindow,
    path: &str,
) -> Result<Vec<Values>, Fail<E>> {
    let start = cx.output.start;
    let len = cx.output.len();
    if let Some(gathered) = gather(cx, context)? {
        return Ok(gathered);
    }
    let mut collected = Vec::new();
    super::reserved(collected.try_reserve_exact(context.sources.len()))?;
    collected.resize_with(context.sources.len(), || Values::missing(0));
    for position in 0..len {
        let row = start + position;
        for ((name, column), values) in context.sources.iter().zip(&mut collected) {
            let value = super::predicates::collect_lent(
                run,
                cx,
                super::predicates::Collection {
                    column: *column,
                    identifier: name,
                    filter: None,
                    selection: None,
                    path,
                    site: None,
                },
                row,
            )
            .map_err(|mut error| {
                // The collection's row is its position in the window's scope.
                if let ExecutionError::MultipleValues {
                    identity: Some(identity),
                    ..
                } = error.as_mut()
                {
                    identity.position = position;
                }
                error
            })?;
            values.push(value);
        }
    }
    Ok(collected)
}

/// A context window's driver fields gathered a field at a time, when every
/// row of the scope reads one record, whose reading is its one cell
/// ([`super::predicates::collect_lent`]), and every field is a column the
/// records' table holds: a donor frame's column or a port's typed storage.
/// None otherwise, or when a record lies outside the column, and the fields
/// are collected a row at a time, which reports any failure.
fn gather<E>(
    cx: &Cx<'_, E>,
    context: &crate::dataset::ContextWindow,
) -> Result<Option<Vec<Values>>, Fail<E>> {
    let rows = cx.output.clone();
    let Some(first) = rows.clone().next() else {
        return Ok(None);
    };
    let table = cx.records(first).0;
    let mut records = Vec::new();
    super::reserved(records.try_reserve_exact(rows.len()))?;
    for row in rows {
        match cx.records(row).1 {
            [record] => records.push(*record),
            _ => return Ok(None),
        }
    }
    let mut gathered = Vec::new();
    super::reserved(gathered.try_reserve_exact(context.sources.len()))?;
    for &(_, column) in &context.sources {
        let values = match table {
            Table::Source(_) => match table.storage(column) {
                Some(storage) => match Values::from_column(storage, &records) {
                    Some(values) => super::reserved(values)?,
                    None => return Ok(None),
                },
                None => return Ok(None),
            },
            Table::Rows {
                frame, start, rows, ..
            } if column < frame.width() && records.iter().all(|&record| record < rows) => {
                let at: Vec<usize> = records.iter().map(|&record| start + record).collect();
                super::reserved(frame.column(column).gather(&at))?
            }
            Table::Input { frame, .. }
                if column < frame.width()
                    && records.iter().all(|&record| record < frame.rows()) =>
            {
                super::reserved(frame.column(column).gather(&records))?
            }
            Table::Rows { .. } | Table::Input { .. } => return Ok(None),
        };
        gathered.push(values);
    }
    Ok(Some(gathered))
}

fn build<E>(run: &mut Run<'_, '_, E>, cx: &Cx<'_, E>, expr: ExprId) -> Result<WindowRun, Fail<E>> {
    let plan = run.plan;
    let (window, context, _) = payload(plan, expr);
    let path = plan.expr(expr).path.to_string();
    let start = cx.output.start;
    let len = cx.output.len();
    let width = cx.frame.width();
    let mut collected = Vec::new();
    let mut schema = None;
    if let Some(context) = context {
        let augmented = context
            .schema(cx.schema, cx.table.schema())
            .map_err(|_| Box::new(ExecutionError::Capacity))?;
        Limits::table(len, augmented.columns().len()).map_err(|error| {
            Box::new(ExecutionError::ScopedLimit {
                resource: match error.resource {
                    "columns" => Resource::SourceColumns,
                    "table_rows" => Resource::SourceRows,
                    "table_cells" => Resource::SourceCells,
                    _ => unreachable!("table boundary resource"),
                },
                limit: error.limit,
                required: error.actual,
                spec_paths: vec![path.clone()],
            })
        })?;
        collected = collect(run, cx, context, &path)?;
        schema = Some(augmented);
    }
    if i64::try_from(len).is_err() {
        return Err(Box::new(ExecutionError::Capacity));
    }
    let allocation = |_| {
        Box::new(ExecutionError::OutputGrouping(
            crate::table_grouping::GroupingError::Allocation,
        ))
    };
    let grouping = if window.group_by.is_empty() {
        Arc::new(super::reserved(Grouping::whole(len))?)
    } else {
        let width = schema.as_ref().unwrap_or(cx.schema).columns().len();
        super::group::admit(&window.group_by, width, len, None)
            .map_err(|error| Box::new(ExecutionError::OutputGrouping(error)))?;
        if window
            .group_by
            .iter()
            .all(|&column| column < cx.frame.width())
        {
            // Frame columns only: the grouping the unit's other windows and
            // output aggregates over the same keys share.
            run.windows
                .grouping(cx.frame, &window.group_by, start..start + len)
                .map_err(allocation)?
        } else if window
            .group_by
            .iter()
            .all(|&column| column >= cx.frame.width())
        {
            // Collected driver fields only, each a buffer of the scope.
            let keys: Vec<&Values> = window
                .group_by
                .iter()
                .map(|&column| &collected[column - cx.frame.width()])
                .collect();
            Arc::new(Grouping::runs(&keys, 0..len).map_err(allocation)?)
        } else {
            let read = |position: usize, column: usize| {
                if column < cx.frame.width() {
                    cx.frame.get(start + position, column)
                } else {
                    collected[column - cx.frame.width()].get(position)
                }
            };
            let grouping = Grouping::partition(len, window.group_by.len(), |position, key| {
                read(position, window.group_by[key])
            })
            .map_err(allocation)?;
            Arc::new(grouping)
        }
    };
    let completed = super::filled(grouping.groups.len(), false)?;
    let mut built = WindowRun {
        window: window.clone(),
        start,
        width,
        collected,
        schema,
        remaining: grouping.groups.len(),
        grouping,
        completed,
        truths: None,
        admitted: false,
        answers: Answers {
            words: super::filled(len, 0usize)?,
            ambiguous: IdentityMap::default(),
        },
        standings: None,
        scratch: Scratch::default(),
    };
    if !matches!(window.kind, WindowKind::BaselineFlag { .. }) && !window.order_by.is_empty() {
        // A typed buffer holds present values of one type, which compare
        // unless they are truth values (VALUE-19).
        let typed = window.order_by.iter().all(|term| {
            matches!(
                built.lane(cx, term.column).0,
                Values::Missing(_)
                    | Values::Int(..)
                    | Values::Float(..)
                    | Values::Date(..)
                    | Values::DateTime(..)
                    | Values::Str(..)
            )
        });
        let positions: Vec<usize> = if typed {
            Vec::new()
        } else {
            (0..len).collect()
        };
        let terms = window.order_by.len();
        built.admitted = typed
            || super::relations::comparable(&positions, terms, |&position, term| {
                built.get(cx, position, window.order_by[term].column)
            })
            .is_ok();
    }
    if let Some(filter) = &window.filter {
        built.truths = filter_truths(filter, &built, cx);
    }
    Ok(built)
}

/// The window filter's truth at every position of the scope, from its
/// column program (#4199), or none when a position would fail: then each
/// member evaluates through the row evaluator, which reports the failure.
fn filter_truths<E>(
    filter: &crate::bound_expression::BoundPredicate,
    built: &WindowRun,
    cx: &Cx<'_, E>,
) -> Option<Vec<Truth>> {
    let len = built.answers.words.len();
    if len < 2 {
        return None;
    }
    let rows: Vec<usize> = (built.start..built.start + len).collect();
    let positions: Vec<usize> = (0..len).collect();
    let records: Vec<usize> = rows
        .iter()
        .map(|&row| cx.frame.members(row).first().copied())
        .collect::<Option<_>>()?;
    super::predicates::record_truths(filter, cx.table, &records, |column| {
        if column < built.width {
            super::predicates::Operand::Frame(cx.frame.column(column), &rows)
        } else {
            // A collected field is lent, as its buffer is indexed by position.
            super::predicates::Operand::Frame(&built.collected[column - built.width], &positions)
        }
    })
}

/// A binding whose expression is a window: its prepared run's answers.
pub(super) fn direct<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Option<Values>, Fail<E>> {
    let Some(id) = window_id(run.plan, expr) else {
        return Ok(None);
    };
    if !run.windows.runs.contains_key(&id) {
        prepare(run, cx, expr)?;
    }
    answers(run, cx, expr, id, rows).map(Some)
}

/// A window read inside another expression: its run is built when a row
/// first reaches it. Building takes the whole run, so no map entry is held.
#[allow(clippy::map_entry)]
pub(super) fn value<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let id = window_id(run.plan, expr).expect("a window node");
    if !run.windows.runs.contains_key(&id) {
        let built = build(run, cx, expr)?;
        run.windows.runs.insert(id, built);
    }
    answers(run, cx, expr, id, rows)
}

/// The answers at `rows`: the partitions are completed first, then the
/// answers are written in bulk, each by the window's kind.
fn answers<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    id: WindowId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let mut built = run
        .windows
        .runs
        .remove(&id)
        .expect("the reached window has a run");
    let result = (|| {
        if rows.len() > 1 {
            // A batch completes every partition, in group order, whatever
            // rows it reads: a failure replays the unit a row at a time,
            // which completes only the partitions it reaches, in row order.
            if built.remaining > 0 {
                for group in 0..built.completed.len() {
                    complete(&mut built, cx, group)?;
                }
            }
        } else if let Some(&row) = rows.first() {
            let group = built.grouping.group_of[row - built.start];
            complete(&mut built, cx, group)?;
        }
        write(plan, &built, cx, expr, rows)
    })();
    run.windows.runs.insert(id, built);
    result
}

/// The completed answers at `rows`, as pushing each row's answer in turn
/// leaves the buffer; a cell is lent, never owned on its way in.
fn write<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    built: &WindowRun,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let words = &built.answers.words;
    if !rows.is_empty()
        && matches!(
            built.window.kind,
            WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense
        )
        && rows.iter().all(|&row| words[row - built.start] != 0)
    {
        // Every row numbered: the buffer pushing each number leaves.
        let mut numbers = Vec::new();
        super::reserved(numbers.try_reserve_exact(rows.len()))?;
        // A number is at most the scope's length, which fits.
        numbers.extend(rows.iter().map(|&row| words[row - built.start] as i64));
        return Ok(Values::Int(numbers, Bits::new(rows.len(), true)));
    }
    if matches!(built.window.kind, WindowKind::BaselineFlag { .. })
        && rows.iter().all(|&row| words[row - built.start] <= FLAG)
    {
        // Flags and missing values: the text once, lent by every flagged
        // row, as pushing each row's answer reads.
        if rows.iter().all(|&row| words[row - built.start] == 0) {
            return Ok(Values::missing(rows.len()));
        }
        let mut values = Values::repeat(ValueRef::Str("Y"), rows.len());
        if let Values::Str(_, valid) = &mut values {
            for (index, &row) in rows.iter().enumerate() {
                if words[row - built.start] == 0 {
                    valid.set(index, false);
                }
            }
        }
        return Ok(values);
    }
    let mut values = Values::missing(0);
    let mut reserved = false;
    for (index, &row) in rows.iter().enumerate() {
        let position = row - built.start;
        let word = words[position];
        let value = match built.window.kind {
            _ if word == 0 => ValueRef::Missing,
            WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense => {
                // A number is at most the scope's length, which fits.
                ValueRef::Int(word as i64)
            }
            WindowKind::RowValue { column, .. } | WindowKind::Locf { column, .. } => {
                built.get(cx, word - 1, column)
            }
            WindowKind::BaselineFlag { .. } if word == FLAG => ValueRef::Str("Y"),
            WindowKind::BaselineFlag { .. } => {
                return Err(ambiguity(plan, built, cx, expr, position, word - 2));
            }
        };
        values.push(value);
        // The buffer is typed at its first value: room for the rest then.
        if !reserved && !matches!(values, Values::Missing(_)) {
            super::reserved(values.try_reserve(rows.len() - index - 1))?;
            reserved = true;
        }
    }
    Ok(values)
}

/// Complete one group's partition, sorting it and evaluating every filter
/// in sorted order before numbering any row.
fn complete<E>(built: &mut WindowRun, cx: &Cx<'_, E>, group: usize) -> Result<(), Fail<E>> {
    if std::mem::replace(&mut built.completed[group], true) {
        return Ok(());
    }
    built.remaining -= 1;
    if built.standings.is_none() {
        built.standings = Some(standings(built, cx)?);
    }
    // The answers and member lists are written apart from the run, which
    // the window and the values it reads are borrowed from.
    let mut answers = std::mem::take(&mut built.answers);
    let mut scratch = std::mem::take(&mut built.scratch);
    scratch.members.clear();
    scratch
        .members
        .extend_from_slice(built.grouping.groups.group(group));
    let result = partition(built, cx, group, &mut scratch, &mut answers);
    built.answers = answers;
    built.scratch = scratch;
    result
}

/// How a partition's members, in construction order, stand to the order of
/// the window's terms.
#[derive(Clone, Copy, PartialEq)]
enum Standing {
    /// No pair out of order: the stable sort leaves them as they are.
    InOrder,
    /// Every pair strictly out of order, so no two tie: the stable sort
    /// reverses them.
    Reversed,
    Unordered,
}

/// How every group's partition stands to the window's order terms, when
/// the scope is admitted, so every pair compares: each member is compared
/// with the member before it in its partition, in construction order, term
/// by term until one decides. The pairs are compared a term at a time over
/// the positions in order, so each buffer is read front to back and the
/// member before is most often the position before, read just now. Nothing
/// here fails, but for room, or counts, so a row-mode replay finds them all
/// as a batch does.
/// Empty for a baseline, a window without order terms or a scope not
/// admitted, which no partition asks.
fn standings<E>(built: &WindowRun, cx: &Cx<'_, E>) -> Result<Vec<Standing>, Fail<E>> {
    let window = &built.window;
    if matches!(window.kind, WindowKind::BaselineFlag { .. })
        || window.order_by.is_empty()
        || !built.admitted
    {
        return Ok(Vec::new());
    }
    let len = built.answers.words.len();
    let groups = &built.grouping.groups;
    let group_of = &built.grouping.group_of;
    // Each position's order after the member before it, and whether a term
    // has decided it: a pair that fails to compare counts as a tie and
    // stops there, as the sort counts it. Members are in position order, so
    // the member before a position is the last position of its group read.
    let mut order = super::filled(len, Ordering::Equal)?;
    let mut decided = super::filled(len, false)?;
    let mut last = super::filled(groups.len(), usize::MAX)?;
    for term in &window.order_by {
        let (values, first) = built.lane(cx, term.column);
        let mut previous = ValueRef::Missing;
        last.fill(usize::MAX);
        for position in 0..len {
            let current = values.get(first + position);
            let member = std::mem::replace(&mut last[group_of[position]], position);
            if member != usize::MAX && !decided[position] {
                let left = if member + 1 == position {
                    previous
                } else {
                    values.get(first + member)
                };
                match super::relations::compare(left, current, term) {
                    Ok(Ordering::Equal) => {}
                    Ok(decision) => {
                        order[position] = decision;
                        decided[position] = true;
                    }
                    Err(_) => decided[position] = true,
                }
            }
            previous = current;
        }
    }
    let mut standings = Vec::new();
    super::reserved(standings.try_reserve_exact(groups.len()))?;
    standings.extend(groups.iter().map(|members| {
        let after = members.get(1..).unwrap_or(&[]);
        if after
            .iter()
            .all(|&member| order[member] != Ordering::Greater)
        {
            Standing::InOrder
        } else if after
            .iter()
            .all(|&member| order[member] == Ordering::Greater)
        {
            Standing::Reversed
        } else {
            Standing::Unordered
        }
    }));
    Ok(standings)
}

/// Stable positions shared by ordinary windows and causal components.
fn sort_members<E>(
    built: &WindowRun,
    cx: &Cx<'_, E>,
    group: usize,
    members: &mut [usize],
) -> Result<(), Fail<E>> {
    let window = &built.window;
    // A baseline's tie-break terms compare only eligible rows sharing the
    // latest date; its partition keeps construction order.
    let baseline = matches!(window.kind, WindowKind::BaselineFlag { .. });
    if !baseline && !window.order_by.is_empty() {
        // Members are in construction order, which a tie keeps (VALUE-23).
        if built.admitted {
            // Every pair compares; each buffer is read directly. A
            // partition already in order, as rows built in order often are,
            // or in strictly reverse order, is left as the stable sort would
            // leave it.
            let standing = built
                .standings
                .as_ref()
                .map_or(Standing::Unordered, |standings| standings[group]);
            if standing == Standing::Reversed {
                members.reverse();
            } else if standing == Standing::Unordered {
                let lanes: Vec<_> = window
                    .order_by
                    .iter()
                    .map(|term| (term, built.lane(cx, term.column)))
                    .collect();
                members.sort_by(|&left, &right| {
                    for (term, (values, first)) in &lanes {
                        let pair = (values.get(first + left), values.get(first + right));
                        match super::relations::compare(pair.0, pair.1, term) {
                            Ok(Ordering::Equal) => {}
                            Ok(order) => return order,
                            Err(_) => return Ordering::Equal,
                        }
                    }
                    Ordering::Equal
                });
            }
        } else {
            super::relations::sort(members, &window.order_by, |&member, term| {
                built.get(cx, member, window.order_by[term].column)
            })
            .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
        }
    }
    Ok(())
}

/// The members of every partition in window order. Its position answers are
/// independent of source values, so a backward read can lend a result only
/// when the component has completed its earlier row.
pub(super) fn row_order<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
) -> Result<Vec<usize>, Fail<E>> {
    let mut built = build(run, cx, expr)?;
    built.standings = Some(standings(&built, cx)?);
    let mut order = Vec::new();
    super::reserved(order.try_reserve_exact(cx.output.len()))?;
    let mut members = Vec::new();
    for (group, positions) in built.grouping.groups.iter().enumerate() {
        members.clear();
        super::reserved(members.try_reserve(positions.len()))?;
        members.extend_from_slice(positions);
        sort_members(&built, cx, group, &mut members)?;
        order.extend(members.iter().map(|position| built.start + position));
    }
    let id = window_id(run.plan, expr).expect("admitted recurrence window");
    run.windows.runs.insert(id, built);
    Ok(order)
}

/// Number one partition, `scratch.members` in construction order: sort it,
/// then evaluate every filter in sorted order before numbering any row.
fn partition<E>(
    built: &WindowRun,
    cx: &Cx<'_, E>,
    group: usize,
    scratch: &mut Scratch,
    answers: &mut Answers,
) -> Result<(), Fail<E>> {
    let window = &built.window;
    let Scratch { members, eligible } = scratch;
    sort_members(built, cx, group, members)?;
    if let Some(filter) = &window.filter {
        eligible.clear();
        for &member in members.iter() {
            let truth = match &built.truths {
                Some(truths) => truths[member],
                None => {
                    let row = built.start + member;
                    let source_row = cx.frame.members(row)[0];
                    super::predicates::row_truth(filter, cx.table, source_row, |column| {
                        built.get(cx, member, column)
                    })
                    .map_err(|error| {
                        Box::new(ExecutionError::Predicate {
                            source_row,
                            error: *error,
                        })
                    })?
                }
            };
            if truth == Truth::True {
                eligible.push(member);
            }
        }
        std::mem::swap(members, eligible);
    }
    let members = &members[..];
    let words = &mut answers.words;
    match window.kind {
        WindowKind::BaselineFlag {
            date,
            reference_date,
            value,
            inclusive,
        } => {
            let (dates, at) = built.lane(cx, date);
            let (references, reference_at) = built.lane(cx, reference_date);
            let (sources, source_at) = built.lane(cx, value);
            let tie = |member, best| ordered(built, cx, window, member, best);
            // Typed dates read from their buffers; anything else is lent,
            // and a pair of two kinds fails.
            let (latest, count) = match (dates, references) {
                (Values::Date(dates, valid), Values::Date(references, known)) => latest(
                    members,
                    inclusive,
                    |member| {
                        let (date, reference) = (at + member, reference_at + member);
                        (valid.get(date)
                            && known.get(reference)
                            && present(sources, source_at + member))
                        .then(|| (dates[date], references[reference]))
                    },
                    |left, right| Ok(left.cmp(&right)),
                    tie,
                )?,
                (Values::DateTime(dates, valid), Values::DateTime(references, known)) => latest(
                    members,
                    inclusive,
                    |member| {
                        let (date, reference) = (at + member, reference_at + member);
                        (valid.get(date)
                            && known.get(reference)
                            && present(sources, source_at + member))
                        .then(|| (dates[date], references[reference]))
                    },
                    |left, right| Ok(left.cmp(&right)),
                    tie,
                )?,
                _ => latest(
                    members,
                    inclusive,
                    |member| {
                        let candidate = dates.get(at + member);
                        let reference = references.get(reference_at + member);
                        (!matches!(candidate, ValueRef::Missing)
                            && !matches!(reference, ValueRef::Missing)
                            && present(sources, source_at + member))
                        .then_some((candidate, reference))
                    },
                    temporal_order,
                    tie,
                )?,
            };
            if let Some(row) = latest {
                if count > 1 {
                    for &member in members {
                        words[member] = row + 2;
                    }
                    answers.ambiguous.insert(row, count);
                } else {
                    words[row] = FLAG;
                }
            }
        }
        WindowKind::RowValue { offset, .. } => {
            for (index, &member) in members.iter().enumerate() {
                let target = usize::try_from(index as i128 + i128::from(offset)).ok();
                if let Some(&row) = target.and_then(|target| members.get(target)) {
                    words[member] = row + 1;
                }
            }
        }
        WindowKind::Locf {
            column,
            include_current,
        } => {
            let (sources, first) = built.lane(cx, column);
            let mut previous = None;
            for &member in members {
                let present = !matches!(sources.get(first + member), ValueRef::Missing);
                let donor = if present && include_current {
                    Some(member)
                } else {
                    previous
                };
                if let Some(row) = donor {
                    words[member] = row + 1;
                }
                if present {
                    previous = Some(member);
                }
            }
        }
        WindowKind::RowNumber | WindowKind::Competition | WindowKind::Dense => {
            let mut rank = 0;
            for (index, &member) in members.iter().enumerate() {
                let new_rank = index == 0
                    || window.kind == WindowKind::RowNumber
                    || ordered(built, cx, window, members[index - 1], member)? != Ordering::Equal;
                if new_rank {
                    rank = if window.kind == WindowKind::Dense {
                        rank + 1
                    } else {
                        index + 1
                    };
                }
                words[member] = rank;
            }
        }
    }
    Ok(())
}

/// A baseline partition's latest eligible row, and how many eligible rows
/// share its date: `read(member)` is a member's date and reference date when
/// both and its value are present, `order` puts two dates in order, and
/// `tie` orders two members of one date by the window's terms.
fn latest<T: Copy, E>(
    members: &[usize],
    inclusive: bool,
    read: impl Fn(usize) -> Option<(T, T)>,
    order: impl Fn(T, T) -> Result<Ordering, Fail<E>>,
    tie: impl Fn(usize, usize) -> Result<Ordering, Fail<E>>,
) -> Result<(Option<usize>, usize), Fail<E>> {
    let mut latest: Option<(usize, T)> = None;
    let mut count = 0;
    for &member in members {
        let Some((candidate, reference)) = read(member) else {
            continue;
        };
        match order(candidate, reference)? {
            Ordering::Greater => continue,
            Ordering::Equal if !inclusive => continue,
            _ => {}
        }
        let decided = match latest {
            None => Ordering::Greater,
            Some((best, date)) => match order(candidate, date)? {
                Ordering::Equal => tie(member, best)?,
                decided => decided,
            },
        };
        if decided == Ordering::Greater {
            latest = Some((member, candidate));
            count = 1;
        } else if decided == Ordering::Equal {
            count += 1;
        }
    }
    Ok((latest.map(|(member, _)| member), count))
}

/// Whether a buffer holds a value at `index`, read from its validity when
/// it is typed.
fn present(values: &Values, index: usize) -> bool {
    match values {
        Values::Missing(_) => false,
        Values::Int(_, valid)
        | Values::Float(_, valid)
        | Values::Bool(_, valid)
        | Values::Date(_, valid)
        | Values::DateTime(_, valid)
        | Values::Str(_, valid) => valid.get(index),
        Values::Mixed(_) => !matches!(values.get(index), ValueRef::Missing),
    }
}

/// The failure of reading position `position` of an ambiguous baseline
/// partition, whose latest row is `donor`.
fn ambiguity<E>(
    plan: &crate::dataset::resolved::ResolvedPlan,
    built: &WindowRun,
    cx: &Cx<'_, E>,
    expr: ExprId,
    position: usize,
    donor: usize,
) -> Fail<E> {
    let (window, _, column) = payload(plan, expr);
    let WindowKind::BaselineFlag { date, .. } = window.kind else {
        unreachable!("an ambiguous answer is a baseline's")
    };
    let path = plan.expr(expr).path.to_string();
    let column_name = cx
        .schema
        .columns()
        .get(column)
        .map(|column| column.name.clone())
        .unwrap_or_else(|| {
            path.rsplit_once(".key.")
                .and_then(|(_, value)| value.split('.').next())
                .map_or_else(|| path.clone(), |field| format!("key[{field}]"))
        });
    let schema = built.schema.as_ref().unwrap_or(cx.schema);
    Box::new(ExecutionError::BaselineAmbiguity {
        path,
        column: column_name,
        date: super::eval::owned(built.get(cx, donor, date)),
        match_count: built.answers.ambiguous[&donor],
        partition: window
            .group_by
            .iter()
            .map(|&group| {
                (
                    schema.columns()[group].name.clone(),
                    super::eval::owned(built.get(cx, position, group)),
                )
            })
            .collect(),
    })
}

/// Compare declared order terms between two positions. Binding refuses a
/// term whose values do not all compare (VALUE-22); a pair whose types no
/// source decided before the run fails the run here, never a guess.
fn ordered<E>(
    built: &WindowRun,
    cx: &Cx<'_, E>,
    window: &Window,
    left: usize,
    right: usize,
) -> Result<Ordering, Fail<E>> {
    for term in &window.order_by {
        let order = super::relations::compare(
            built.get(cx, left, term.column),
            built.get(cx, right, term.column),
            term,
        )
        .map_err(|_| Box::new(ExecutionError::SchemaMismatch))?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

/// A baseline's dates in order: binding admits a date and a reference of one
/// temporal type, decided before any row is read (VALUE-2); a pair of
/// another kind, whose types no source decided, fails the run.
fn temporal_order<E>(left: ValueRef<'_>, right: ValueRef<'_>) -> Result<Ordering, Fail<E>> {
    match (left, right) {
        (ValueRef::Date(left), ValueRef::Date(right)) => Ok(left.cmp(&right)),
        (ValueRef::DateTime(left), ValueRef::DateTime(right)) => Ok(left.cmp(&right)),
        _ => Err(Box::new(ExecutionError::SchemaMismatch)),
    }
}

#[cfg(test)]
mod tests {
    // The groupings the windows and output aggregates of a window stage or
    // of the column phase share (`Runs::grouping`), a seam no specification
    // reaches: which grouping a later unit reuses, merges from one by more
    // keys, or builds again once a key column is written, and that a key the
    // rows hold alike merges nothing apart. The groups follow
    // ROW-4, derived by hand: first occurrence, members in row order.
    use super::*;
    use crate::dataset::columnar::frame::Identity;

    fn groups(grouping: &Grouping) -> Vec<Vec<usize>> {
        grouping.groups.iter().map(<[usize]>::to_vec).collect()
    }

    #[test]
    fn a_grouping_is_shared_until_a_key_column_is_written() {
        let cells = [("S1", "A"), ("S1", "B"), ("S2", "A"), ("S1", "A")];
        let mut frame = Frame::new(3);
        for (row, (subject, parameter)) in cells.iter().enumerate() {
            frame.push(0, Identity::Candidate(row), &[row]);
            frame.complete(row, 0, ValueRef::Str(subject));
            frame.complete(row, 1, ValueRef::Str(parameter));
        }
        let mut runs = Runs::default();
        let both = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert_eq!(groups(&both), [vec![0, 3], vec![1], vec![2]], "both keys");
        let again = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert!(Arc::ptr_eq(&both, &again), "the same keys share one");
        let subject = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert_eq!(
            (groups(&subject), subject.group_of.clone()),
            (vec![vec![0, 1, 3], vec![2]], vec![0, 0, 1, 0]),
            "the first key, merged from both"
        );
        let tail = runs.grouping(&frame, &[0], 1..4).unwrap();
        assert_eq!(groups(&tail), [vec![0, 2], vec![1]], "rows 1 to 3");
        // Another column written leaves the groupings; a key column written
        // drops every grouping by it.
        frame.complete(0, 2, ValueRef::Int(1));
        runs.written(2);
        let kept = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert!(Arc::ptr_eq(&subject, &kept), "column 2 is no key");
        frame.reset(3, 0);
        frame.complete(3, 0, ValueRef::Str("S2"));
        runs.written(0);
        let subject = runs.grouping(&frame, &[0], 0..4).unwrap();
        assert_eq!(
            groups(&subject),
            [vec![0, 1], vec![2, 3]],
            "the first key, built again"
        );
        let both = runs.grouping(&frame, &[0, 1], 0..4).unwrap();
        assert_eq!(
            groups(&both),
            [vec![0], vec![1], vec![2, 3]],
            "both keys, built again"
        );
        // Rows 0 and 1 hold the first key alike: merged from both keys, the
        // grouping by it is one group.
        let pair = runs.grouping(&frame, &[0, 1], 0..2).unwrap();
        assert_eq!(groups(&pair), [vec![0], vec![1]], "both keys, rows 0 and 1");
        let alike = runs.grouping(&frame, &[0], 0..2).unwrap();
        assert_eq!(
            (groups(&alike), alike.group_of.clone()),
            (vec![vec![0, 1]], vec![0, 0]),
            "the first key, rows 0 and 1"
        );
    }
}
