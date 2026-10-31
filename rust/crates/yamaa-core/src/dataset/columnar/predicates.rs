//! Compiled expressions over buffers: formulas, flags, conditions and
//! filters. Each identifier occurrence is resolved afresh, so a repeated
//! named or joined read counts its handler once per occurrence.
//!
//! A selection of several rows first evaluates as a column program
//! (#4199): the expression's column form reads each occurrence once over
//! the selection and applies each operator over the operand columns. If any
//! row fails, the program's reads are undone and the selection evaluates
//! through the row evaluator, one row at a time, which reports the failure.
//! One row, and a unit that may call the host, whose calls cannot be
//! undone, evaluate through the row evaluator from the start.
use super::buffer::Values;
use super::eval::owned;
use super::relations::{JoinSite, Site};
use super::{Cx, Fail, Run, Table};
use crate::bound_expression::{Binding, BoundPredicate, Read as BoundRead};
use crate::dataset::execution::ExecutionError;
use crate::dataset::resolved::{ExprId, Op, Read, RelationKind};
use crate::dataset::{FlagRead, SourceSelection};
use crate::expression::{
    Column, ColumnResolver, ErrorKind, EvaluationError, Resolved, Resolver, Stop, Truth,
};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::{CellError, ColumnRef, ValueRef};
use crate::table_grouping::Key;
use crate::value::{Selection, Value};
use std::collections::BTreeSet;

/// How a named read inside an expression observes its handlers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Observe {
    /// A formula operand: counts at the owner's path; joins count `absent`.
    Operand,
    /// A predicate operand: named reads record nothing, joins are uncounted.
    Predicate,
}

/// Resolve one bound read of an expression node for one row: the plan's read
/// and the binding it reads through.
pub(super) fn resolve_read<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    (read, bound): (Read, &BoundRead),
    path: &str,
    row: usize,
    observe: Observe,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    match read {
        Read::Field(field) => {
            if let (Some(operands), BoundRead::Source(index)) = (cx.operands, bound) {
                return Ok(super::relations::operand_value(*index, operands).clone());
            }
            let source_row = cx.frame.members(row)[0];
            cx.table
                .get(source_row, field as usize)
                .map(owned)
                .map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: path.into(),
                        source_row,
                        error,
                    })
                })
        }
        Read::Column(column) => Ok(cx.frame.value(row, cx.local(plan, column))),
        Read::Relation(relation, field) => match plan.relation(relation).kind {
            RelationKind::Named { .. } => super::relations::named_value(
                run,
                cx,
                owner,
                relation,
                field as usize,
                row,
                observe == Observe::Operand,
            ),
            RelationKind::Join => {
                let BoundRead::Joined(joined) = bound else {
                    unreachable!("a join read is bound as a joined read")
                };
                let site = match observe {
                    Observe::Operand => JoinSite::Operand(owner.path),
                    Observe::Predicate => JoinSite::Uncounted(path),
                };
                super::relations::joined_value(run, cx, owner, joined, None, site, row)
            }
            _ => {
                let record = cx
                    .record
                    .expect("a donor field is read inside a derivation");
                let position = record.position(row);
                record
                    .field(
                        plan,
                        owner.expr.expect("a derivation read has a node"),
                        position,
                        field as usize,
                    )
                    .map(owned)
            }
        },
        Read::Window(_) => unreachable!("an expression binding reads no window"),
    }
}

/// Run a column program over `rows` when the selection shares one: several
/// rows, in a unit that cannot call the host. A program that stops leaves
/// no handler count or relation entry behind, and answers None, so the
/// caller evaluates the rows one at a time.
fn columnwise<'p, 'a, E, T>(
    run: &mut Run<'p, 'a, E>,
    rows: &[usize],
    program: impl FnOnce(&mut Run<'p, 'a, E>) -> Result<T, Stop<Fail<E>>>,
) -> Option<T> {
    if rows.len() < 2 || run.calls {
        return None;
    }
    let checkpoint = run.checkpoint();
    match program(run) {
        Ok(result) => Some(result),
        Err(_) => {
            run.undo(checkpoint);
            None
        }
    }
}

/// One operand's values over the selection: lent from the frame or from a
/// source port's typed storage where they are held, otherwise gathered.
pub(super) enum Operand<'r> {
    Values(Values),
    /// A frame column, at the selected rows.
    Frame(&'r Values, &'r [usize]),
    /// A frame column, at the selected records of frame rows from `start`.
    Records {
        values: &'r Values,
        records: &'r [usize],
        start: usize,
    },
    /// A source port's storage, at the selected rows' driver records.
    Storage(ColumnRef<'r>, Vec<usize>),
}

impl Column for Operand<'_> {
    fn get(&self, row: usize) -> ValueRef<'_> {
        match self {
            Self::Values(values) => values.get(row),
            Self::Frame(values, rows) => values.get(rows[row]),
            Self::Records {
                values,
                records,
                start,
            } => values.get(start + records[row]),
            Self::Storage(column, rows) => column.get(rows[row]),
        }
    }
}

/// One raw field of each selected row's driver record, lent from the port's
/// typed storage when it holds the field (#4190).
fn driver_operand<'r, E>(
    cx: &'r Cx<'_, E>,
    field: usize,
    rows: &[usize],
    path: &str,
) -> Result<Operand<'r>, Fail<E>> {
    if let Some(column) = cx.table.storage(field) {
        let mut source_rows = Vec::new();
        super::reserved(source_rows.try_reserve_exact(rows.len()))?;
        source_rows.extend(rows.iter().map(|&row| cx.frame.members(row)[0]));
        if source_rows.iter().all(|&row| row < column.len()) {
            return Ok(Operand::Storage(column, source_rows));
        }
    }
    super::eval::driver_field(cx, field, rows, path).map(Operand::Values)
}

/// One bound read of an expression node over the selection: a driver field
/// and a completed column lent whole; any other read row by row, as
/// `resolve_read` reads it, counting the same handlers.
fn read_column<'r, E>(
    run: &mut Run<'_, '_, E>,
    cx: &'r Cx<'_, E>,
    owner: Site<'_>,
    (read, bound): (Read, &BoundRead),
    path: &str,
    rows: &'r [usize],
    observe: Observe,
) -> Result<Operand<'r>, Fail<E>> {
    match read {
        Read::Field(field) if cx.operands.is_none() || !matches!(bound, BoundRead::Source(_)) => {
            driver_operand(cx, field as usize, rows, path)
        }
        Read::Column(column) => Ok(Operand::Frame(
            cx.frame.column(cx.local(run.plan, column)),
            rows,
        )),
        _ => {
            let mut values = Values::missing(0);
            for &row in rows {
                values.push_value(resolve_read(
                    run,
                    cx,
                    owner,
                    (read, bound),
                    path,
                    row,
                    observe,
                )?);
            }
            Ok(Operand::Values(values))
        }
    }
}

/// The collection of one source read over the selection, row by row.
fn collect_column<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    (column, identifier, path): (usize, &str, &str),
    rows: &[usize],
) -> Result<Operand<'static>, Fail<E>> {
    let mut values = Values::missing(0);
    for &row in rows {
        values.push_value(collect_row(
            run,
            cx,
            Collection {
                column,
                identifier,
                filter: None,
                selection: None,
                path,
                site: None,
            },
            row,
        )?);
    }
    Ok(Operand::Values(values))
}

/// A formula's operands over the selection, counted as `Formula` counts them.
struct FormulaColumns<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    expr: ExprId,
    bindings: &'r [Binding],
    path: &'r str,
    rows: &'r [usize],
}

impl<'r, E> ColumnResolver for FormulaColumns<'r, '_, '_, '_, E> {
    type Error = Fail<E>;
    type Column = Operand<'r>;
    fn column(&mut self, identifier: &str) -> Result<Option<Operand<'r>>, Self::Error> {
        let (cx, rows) = (self.cx, self.rows);
        let index = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let read = self.run.plan.reads(self.expr)[index];
        read_column(
            self.run,
            cx,
            Site::node(self.run.plan, self.expr),
            (read, &self.bindings[index].read),
            self.path,
            rows,
            Observe::Operand,
        )
        .map(Some)
    }
}

struct Formula<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    expr: ExprId,
    bindings: &'r [Binding],
    path: &'r str,
    row: usize,
}

impl<E> Resolver for Formula<'_, '_, '_, '_, E> {
    type Error = Fail<E>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("numeric names were bound before execution");
        let read = self.run.plan.reads(self.expr)[index];
        resolve_read(
            self.run,
            self.cx,
            Site::node(self.run.plan, self.expr),
            (read, &self.bindings[index].read),
            self.path,
            self.row,
            Observe::Operand,
        )
        .map(Selection::Present)
    }
}

/// Keep scalar and aggregate arithmetic failures on the same typed route.
pub(super) fn project_result<E>(
    run: &Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    result: Result<crate::numeric::Number, Box<EvaluationError<Fail<E>>>>,
    row: usize,
) -> Result<Value, Fail<E>> {
    match result {
        Ok(value) => Ok(Value::from(value)),
        Err(error) => {
            let EvaluationError {
                spec_path,
                expression,
                route,
                source_span,
                kind,
            } = *error;
            match kind {
                ErrorKind::Resolution { error, .. } => Err(error),
                ErrorKind::Condition(condition) => {
                    let identity = if condition.phase() == "derivation" {
                        run.failure_identity(cx, row)
                    } else {
                        None
                    };
                    Err(Box::new(ExecutionError::Numeric {
                        error: EvaluationError {
                            spec_path,
                            expression,
                            route,
                            source_span,
                            kind: ErrorKind::Condition(condition),
                        },
                        identity,
                    }))
                }
            }
        }
    }
}

/// Compiled scalar arithmetic, resolved in written order.
pub(super) fn compute<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::Compute(bound) = &plan.expr(expr).op else {
        unreachable!("a formula node holds its formula")
    };
    let path = bound.expression().spec_path();
    let program = columnwise(run, rows, |run| {
        let numbers = bound.expression().number_column(
            rows.len(),
            &mut FormulaColumns {
                run,
                cx,
                expr,
                bindings: bound.bindings(),
                path,
                rows,
            },
        )?;
        Ok(Values::from_numbers(&numbers)?)
    });
    if let Some(values) = program {
        return Ok(values);
    }
    let mut values = Values::missing(0);
    for &row in rows {
        let result = bound.expression().evaluate_number(&mut Formula {
            run,
            cx,
            expr,
            bindings: bound.bindings(),
            path: bound.expression().spec_path(),
            row,
        });
        values.push_value(project_result(run, cx, result, row)?);
    }
    Ok(values)
}

struct FlagOperands<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    expr: ExprId,
    flag: &'r crate::dataset::Flag,
    row: usize,
}

impl<E> Resolver for FlagOperands<'_, '_, '_, '_, E> {
    type Error = Fail<E>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .flag
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("flag names are completely bound before execution");
        let path = self.flag.condition().spec_path();
        let value = match &self.flag.bindings()[index].read {
            FlagRead::Collect { column, identifier } => collect_row(
                self.run,
                self.cx,
                Collection {
                    column: *column,
                    identifier,
                    filter: None,
                    selection: None,
                    path,
                    site: None,
                },
                self.row,
            )?,
            FlagRead::Read(bound) => {
                let read = self.run.plan.reads(self.expr)[index];
                resolve_read(
                    self.run,
                    self.cx,
                    Site::node(self.run.plan, self.expr),
                    (read, bound),
                    path,
                    self.row,
                    Observe::Predicate,
                )?
            }
        };
        Ok(Selection::Present(value))
    }
}

/// A flag's operands over the selection, read as `FlagOperands` reads them.
struct FlagColumns<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    expr: ExprId,
    flag: &'r crate::dataset::Flag,
    rows: &'r [usize],
}

impl<'r, E> ColumnResolver for FlagColumns<'r, '_, '_, '_, E> {
    type Error = Fail<E>;
    type Column = Operand<'r>;
    fn column(&mut self, identifier: &str) -> Result<Option<Operand<'r>>, Self::Error> {
        let (cx, rows) = (self.cx, self.rows);
        let index = self
            .flag
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("flag names are completely bound before execution");
        let path = self.flag.condition().spec_path();
        match &self.flag.bindings()[index].read {
            FlagRead::Collect { column, identifier } => {
                collect_column(self.run, cx, (*column, identifier, path), rows)
            }
            FlagRead::Read(bound) => {
                let read = self.run.plan.reads(self.expr)[index];
                read_column(
                    self.run,
                    cx,
                    Site::node(self.run.plan, self.expr),
                    (read, bound),
                    path,
                    rows,
                    Observe::Predicate,
                )
            }
        }
        .map(Some)
    }
}

/// One condition selects a raw literal; an UNKNOWN condition is counted.
pub(super) fn flag<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Values, Fail<E>> {
    let plan = run.plan;
    let Op::Flag(flag) = &plan.expr(expr).op else {
        unreachable!("a flag node holds its flag")
    };
    let program = columnwise(run, rows, |run| {
        flag.condition().truth_column(
            rows.len(),
            &mut FlagColumns {
                run,
                cx,
                expr,
                flag,
                rows,
            },
        )
    });
    let mut values = Values::missing(0);
    if let Some(truths) = program {
        for truth in truths {
            if truth == Truth::Unknown {
                run.fire_node(expr, HandlerKind::Missing, || flag.missing_path())?;
            }
            values.push(flag.value(truth));
        }
        return Ok(values);
    }
    for &row in rows {
        let truth = flag
            .condition()
            .evaluate_truth(&mut FlagOperands {
                run,
                cx,
                expr,
                flag,
                row,
            })
            .map_err(|error| match error.kind {
                ErrorKind::Resolution { error, .. } => error,
                ErrorKind::Condition(_) => Box::new(ExecutionError::ScalarDiagnostic(
                    crate::dataset::flag_condition_diagnostic(&error)
                        .expect("condition has a core flag projection"),
                )),
            })?;
        if truth == Truth::Unknown {
            run.fire_node(expr, HandlerKind::Missing, || flag.missing_path())?;
        }
        values.push(flag.value(truth));
    }
    Ok(values)
}

struct Condition<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    /// The read whose path a named read inside the condition reports under.
    owner: Site<'r>,
    condition: ExprId,
    predicate: &'r BoundPredicate,
    row: usize,
}

impl<E> Resolver for Condition<'_, '_, '_, '_, E> {
    type Error = Fail<E>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        let index = self
            .predicate
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("case names are admitted before execution");
        let binding = &self.predicate.bindings()[index];
        let path = self.predicate.plan().spec_path();
        if let BoundRead::Source(column) = binding.read {
            if self.predicate.collects(identifier) {
                return collect_row(
                    self.run,
                    self.cx,
                    Collection {
                        column,
                        identifier,
                        filter: None,
                        selection: None,
                        path,
                        site: None,
                    },
                    self.row,
                )
                .map(Selection::Present);
            }
        }
        let read = self.run.plan.reads(self.condition)[index];
        resolve_read(
            self.run,
            self.cx,
            self.owner,
            (read, &binding.read),
            path,
            self.row,
            Observe::Predicate,
        )
        .map(Selection::Present)
    }
}

/// A condition's operands over the selection, read as `Condition` reads them.
struct ConditionColumns<'r, 'p, 'a, 'c, E> {
    run: &'r mut Run<'p, 'a, E>,
    cx: &'r Cx<'c, E>,
    owner: Site<'r>,
    condition: ExprId,
    predicate: &'r BoundPredicate,
    rows: &'r [usize],
}

impl<'r, E> ColumnResolver for ConditionColumns<'r, '_, '_, '_, E> {
    type Error = Fail<E>;
    type Column = Operand<'r>;
    fn column(&mut self, identifier: &str) -> Result<Option<Operand<'r>>, Self::Error> {
        let (cx, rows) = (self.cx, self.rows);
        let index = self
            .predicate
            .bindings()
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
            .expect("case names are admitted before execution");
        let binding = &self.predicate.bindings()[index];
        let path = self.predicate.plan().spec_path();
        if let BoundRead::Source(column) = binding.read {
            if self.predicate.collects(identifier) {
                return collect_column(self.run, cx, (column, identifier, path), rows).map(Some);
            }
        }
        let read = self.run.plan.reads(self.condition)[index];
        read_column(
            self.run,
            cx,
            self.owner,
            (read, &binding.read),
            path,
            rows,
            Observe::Predicate,
        )
        .map(Some)
    }
}

/// The truths of one condition over the selection as a column program, or
/// None when the rows evaluate one at a time.
fn condition_column<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    condition: ExprId,
    rows: &[usize],
) -> Option<Vec<Truth>> {
    let plan = run.plan;
    let Op::Predicate(predicate) = &plan.expr(condition).op else {
        unreachable!("a condition node holds its predicate")
    };
    columnwise(run, rows, |run| {
        predicate.plan().truth_column(
            rows.len(),
            &mut ConditionColumns {
                run,
                cx,
                owner,
                condition,
                predicate,
                rows,
            },
        )
    })
}

/// The truth of one condition evaluated through the case evaluator: named
/// reads, joined reads and collections resolve through the run.
fn condition_truth<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: Site<'_>,
    condition: ExprId,
    row: usize,
) -> Result<Truth, Fail<E>> {
    let plan = run.plan;
    let Op::Predicate(predicate) = &plan.expr(condition).op else {
        unreachable!("a condition node holds its predicate")
    };
    predicate
        .plan()
        .evaluate_truth(&mut Condition {
            run,
            cx,
            owner,
            condition,
            predicate,
            row,
        })
        .map_err(|error| {
            let EvaluationError {
                spec_path,
                expression,
                route,
                source_span,
                kind,
            } = *error;
            match kind {
                ErrorKind::Resolution { error, .. } => error,
                ErrorKind::Condition(condition) => Box::new(ExecutionError::Predicate {
                    source_row: cx.frame.members(row).first().copied().unwrap_or(0),
                    error: EvaluationError {
                        spec_path,
                        expression,
                        route,
                        source_span,
                        kind: ErrorKind::Condition(condition),
                    },
                }),
            }
        })
}

/// The truth of a case condition for each row.
pub(super) fn case_truth<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    owner: ExprId,
    condition: ExprId,
    rows: &[usize],
) -> Result<Vec<Truth>, Fail<E>> {
    let plan = run.plan;
    if let Some(truths) = condition_column(run, cx, Site::node(plan, owner), condition, rows) {
        return Ok(truths);
    }
    let mut truths = Vec::with_capacity(rows.len());
    for &row in rows {
        truths.push(condition_truth(
            run,
            cx,
            Site::node(plan, owner),
            condition,
            row,
        )?);
    }
    Ok(truths)
}

/// The truth of a dataset verification's predicate that reads a lookup,
/// through the case evaluator; named reads report under the check's path.
pub(super) fn verification_truth<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    predicate: ExprId,
    path: &str,
    row: usize,
) -> Result<Truth, Fail<E>> {
    let site = Site {
        expr: Some(predicate),
        path,
    };
    condition_truth(run, cx, site, predicate, row)
}

/// The truth of a template filter for each row: through the case evaluator
/// when it reads a lookup, otherwise over the row's driver record and values.
pub(super) fn filter<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    expr: ExprId,
    rows: &[usize],
) -> Result<Vec<Truth>, Fail<E>> {
    let plan = run.plan;
    let Op::Predicate(predicate) = &plan.expr(expr).op else {
        unreachable!("a filter node holds its predicate")
    };
    let contextual = predicate.bindings().iter().any(|binding| {
        matches!(
            binding.read,
            BoundRead::Intermediate { .. } | BoundRead::Joined(_)
        )
    });
    let program = if contextual {
        condition_column(run, cx, Site::node(plan, expr), expr, rows)
    } else {
        columnwise(run, rows, |_| {
            predicate.plan().truth_column(
                rows.len(),
                &mut RowColumns {
                    cx,
                    bindings: predicate.bindings(),
                    rows,
                },
            )
        })
    };
    if let Some(truths) = program {
        return Ok(truths);
    }
    let mut truths = Vec::with_capacity(rows.len());
    for &row in rows {
        let truth = if contextual {
            condition_truth(run, cx, Site::node(plan, expr), expr, row)?
        } else {
            let source_row = cx.frame.members(row)[0];
            row_truth(predicate, cx.table, source_row, |column| {
                cx.frame.get(row, column)
            })
            .map_err(|error| {
                Box::new(ExecutionError::Predicate {
                    source_row,
                    error: *error,
                })
            })?
        };
        truths.push(truth);
    }
    Ok(truths)
}

struct RowResolver<'r, E, F> {
    table: Table<'r, E>,
    source_row: usize,
    columns: F,
    bindings: &'r [Binding],
}

impl<'r, E, F: Fn(usize) -> ValueRef<'r>> Resolver for RowResolver<'r, E, F> {
    type Error = CellError<E>;
    fn resolve(&mut self, identifier: &str) -> Result<Selection, Self::Error> {
        self.resolve_value(identifier).map(|value| match value {
            Resolved::Absent => Selection::Absent,
            Resolved::Owned(value) => Selection::Present(value),
            Resolved::Borrowed(value) => Selection::Present(owned(value)),
        })
    }
    fn resolve_value(&mut self, identifier: &str) -> Result<Resolved<'_>, Self::Error> {
        let Ok(index) = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
        else {
            return Ok(Resolved::Absent);
        };
        match self.bindings[index].read {
            BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                unreachable!("predicate admission rejects intermediate reads")
            }
            BoundRead::Source(column) => self
                .table
                .get(self.source_row, column)
                .map(Resolved::Borrowed),
            BoundRead::Column(column) => Ok(Resolved::Borrowed((self.columns)(column))),
        }
    }
}

/// A predicate's operands over the selection's driver records and completed
/// values, read as `RowResolver` reads them.
struct RowColumns<'r, 'c, E> {
    cx: &'r Cx<'c, E>,
    bindings: &'r [Binding],
    rows: &'r [usize],
}

impl<'r, E> ColumnResolver for RowColumns<'r, '_, E> {
    type Error = Fail<E>;
    type Column = Operand<'r>;
    fn column(&mut self, identifier: &str) -> Result<Option<Operand<'r>>, Self::Error> {
        let (cx, rows) = (self.cx, self.rows);
        let Ok(index) = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
        else {
            return Ok(None);
        };
        match self.bindings[index].read {
            BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                unreachable!("predicate admission rejects intermediate reads")
            }
            // A failing cell stops the program, and the row evaluator
            // reports it as `row_truth` does.
            BoundRead::Source(column) => driver_operand(cx, column, rows, identifier).map(Some),
            BoundRead::Column(column) => Ok(Some(Operand::Frame(cx.frame.column(column), rows))),
        }
    }
}

/// The truth of a predicate on each of `records`, records of `table`, as
/// [`row_truth`] reads one: a source read reads the record, and
/// `columns(column)` lends a completed column's values, one per record. The
/// column program (#4199) answers when every record evaluates; otherwise
/// none, and each record evaluates through [`row_truth`], which reports the
/// failure. Nothing is counted either way.
pub(super) fn record_truths<'r, E>(
    predicate: &'r BoundPredicate,
    table: Table<'r, E>,
    records: &'r [usize],
    columns: impl FnMut(usize) -> Operand<'r>,
) -> Option<Vec<Truth>> {
    if records.len() < 2 {
        return None;
    }
    predicate
        .plan()
        .truth_column(
            records.len(),
            &mut RecordColumns {
                table,
                records,
                columns,
                bindings: predicate.bindings(),
            },
        )
        .ok()
}

/// A predicate's operands over records of a table, read as `RowResolver`
/// reads them.
struct RecordColumns<'r, E, F> {
    table: Table<'r, E>,
    records: &'r [usize],
    columns: F,
    bindings: &'r [Binding],
}

impl<'r, E, F: FnMut(usize) -> Operand<'r>> ColumnResolver for RecordColumns<'r, E, F> {
    type Error = CellError<E>;
    type Column = Operand<'r>;
    fn column(&mut self, identifier: &str) -> Result<Option<Operand<'r>>, Self::Error> {
        let Ok(index) = self
            .bindings
            .binary_search_by(|binding| binding.name.as_str().cmp(identifier))
        else {
            return Ok(None);
        };
        match self.bindings[index].read {
            BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                unreachable!("predicate admission rejects intermediate reads")
            }
            BoundRead::Source(column) => {
                let records = self.records;
                if let Table::Rows {
                    frame, start, rows, ..
                } = self.table
                {
                    if column < frame.width() && records.iter().all(|&record| record < rows) {
                        return Ok(Some(Operand::Records {
                            values: frame.column(column),
                            records,
                            start,
                        }));
                    }
                }
                if let Some(storage) = self.table.storage(column) {
                    if records.iter().all(|&record| record < storage.len()) {
                        return Ok(Some(Operand::Storage(storage, records.to_vec())));
                    }
                }
                let mut values = Values::missing(0);
                for &record in records {
                    values.push(self.table.get(record, column)?);
                }
                Ok(Some(Operand::Values(values)))
            }
            BoundRead::Column(column) => Ok(Some((self.columns)(column))),
        }
    }
}

/// A predicate over one record of a table and a row's completed values,
/// resolved lazily in written order.
pub(super) fn row_truth<'r, E>(
    predicate: &'r BoundPredicate,
    table: Table<'r, E>,
    source_row: usize,
    columns: impl Fn(usize) -> ValueRef<'r>,
) -> Result<Truth, Box<EvaluationError<CellError<E>>>> {
    predicate.plan().evaluate_truth(&mut RowResolver {
        table,
        source_row,
        columns,
        bindings: predicate.bindings(),
    })
}

/// One source read across the records feeding a row.
pub(super) struct Collection<'c> {
    pub column: usize,
    pub identifier: &'c str,
    /// A filter node over the records, evaluated before any value is read.
    pub filter: Option<ExprId>,
    pub selection: Option<&'c SourceSelection>,
    pub path: &'c str,
    /// The node counting a chosen selection under `multiple_matches`.
    pub site: Option<ExprId>,
}

/// Distinct present raw readings across the row's records, before
/// conversion. Eligibility is complete before any value is read, including
/// a later predicate failure; several distinct readings fail unless a
/// selection chooses one, counted.
pub(super) fn collect_row<E>(
    run: &mut Run<'_, '_, E>,
    cx: &Cx<'_, E>,
    collection: Collection<'_>,
    row: usize,
) -> Result<Value, Fail<E>> {
    let plan = run.plan;
    let (table, members) = cx.records(row);
    let retained: Vec<usize>;
    let members: &[usize] = if let Some(filter) = collection.filter {
        let Op::Predicate(predicate) = &plan.expr(filter).op else {
            unreachable!("a collection filter holds its predicate")
        };
        let mut kept = Vec::with_capacity(members.len());
        for &source_row in members {
            let truth = row_truth(predicate, table, source_row, |_| ValueRef::Missing).map_err(
                |error| {
                    Box::new(ExecutionError::Predicate {
                        source_row,
                        error: *error,
                    })
                },
            )?;
            if truth == Truth::True {
                kept.push(source_row);
            }
        }
        retained = kept;
        &retained
    } else {
        members
    };
    // A single record has at most one distinct reading.
    if members.len() == 1 && collection.selection.is_none() {
        let source_row = members[0];
        return table
            .get(source_row, collection.column)
            .map(owned)
            .map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: collection.path.into(),
                    source_row,
                    error,
                })
            });
    }
    let mut carrying = Vec::new();
    let mut distinct = BTreeSet::new();
    let mut first = None;
    for &source_row in members {
        let value = table.get(source_row, collection.column).map_err(|error| {
            Box::new(ExecutionError::Cell {
                path: collection.path.into(),
                source_row,
                error,
            })
        })?;
        if matches!(value, ValueRef::Missing) {
            continue;
        }
        distinct.insert(Key::from(value));
        first.get_or_insert(value);
        if collection.selection.is_some() {
            carrying.push(source_row);
        }
    }
    if distinct.len() > 1 {
        if let Some(selection) = collection.selection {
            let chosen = super::relations::select(&carrying, selection, |source_row, column| {
                table.get(source_row, column).map_err(|error| {
                    Box::new(ExecutionError::Cell {
                        path: collection.path.into(),
                        source_row,
                        error,
                    })
                })
            })?;
            let value = table.get(chosen, collection.column).map_err(|error| {
                Box::new(ExecutionError::Cell {
                    path: collection.path.into(),
                    source_row: chosen,
                    error,
                })
            })?;
            let path = collection.path;
            match collection.site {
                Some(site) => run.fire_node(site, HandlerKind::MultipleMatches, || {
                    format!("{path}.multiple_matches")
                })?,
                None => run.fire_path(
                    &format!("{path}.multiple_matches"),
                    HandlerKind::MultipleMatches,
                )?,
            }
            return Ok(owned(value));
        }
        return Err(Box::new(ExecutionError::MultipleValues {
            path: collection.path.into(),
            identifier: collection.identifier.into(),
            value_count: distinct.len(),
            identity: run.failure_identity(cx, row),
        }));
    }
    Ok(first.map_or(Value::Missing, owned))
}
