//! Expression evaluation over a selection of rows. Each node answers one
//! value per selected row, in selection order; lazy operands are evaluated
//! only for the rows that reach them.
use super::buffer::{Assembly, Values};
use super::{Cx, Fail, Run};
use crate::dataset::execution::{ExecutionError, RowIdentity};
use crate::dataset::resolved::{
    ExprId, ExprKind, HandlerSiteId, Op, OperandRole, Read, RegionKind,
};
use crate::numeric_lifecycle::HandlerKind;
use crate::table::ValueRef;
use crate::value::Value;

impl<'p, 'a, E> Run<'p, 'a, E> {
    /// Evaluate one expression over `rows`. `retained` is set while the
    /// value flows unchanged into the binding's completed result.
    pub(super) fn eval(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
        retained: bool,
    ) -> Result<Values, Fail<E>> {
        let plan = self.plan;
        let node = plan.expr(expr);
        match node.kind {
            ExprKind::ProjectFunction => super::functions::evaluate(self, cx, expr, rows),
            ExprKind::Window(_) | ExprKind::ContextWindow(_) => {
                super::windows::value(self, cx, expr, rows)
            }
            ExprKind::Joined(_) | ExprKind::JoinedSource(_) | ExprKind::SourceLookup(_) => {
                super::relations::join(self, cx, expr, rows)
            }
            ExprKind::Intermediate(_) => super::relations::read(self, cx, expr, rows, true),
            ExprKind::Aggregate(_) => super::relations::aggregate(self, cx, expr, rows),
            ExprKind::Odm(_) => super::relations::odm(self, cx, expr, rows),
            ExprKind::SourceBinding => self.source_binding(cx, expr, rows, retained),
            ExprKind::FirstAvailable => self.first_available(cx, expr, rows, retained),
            ExprKind::Flag => super::predicates::flag(self, cx, expr, rows),
            ExprKind::Compute => super::predicates::compute(self, cx, expr, rows),
            ExprKind::Case => self.case(cx, expr, rows, retained),
            ExprKind::MappingCut => super::scalar::mapping_cut(self, cx, expr, rows, retained),
            ExprKind::Round => super::scalar::round(self, cx, expr, rows),
            ExprKind::StrTemplate => super::scalar::str_template(self, cx, expr, rows),
            ExprKind::Extreme => super::scalar::extreme(self, cx, expr, rows),
            ExprKind::StrExtract => super::scalar::str_extract(self, cx, expr, rows),
            ExprKind::StrReplace => super::scalar::str_replace(self, cx, expr, rows),
            ExprKind::StrCase => super::scalar::str_case(self, cx, expr, rows),
            ExprKind::StrPad => super::scalar::str_pad(self, cx, expr, rows),
            ExprKind::StrSubstr => super::scalar::str_substr(self, cx, expr, rows),
            ExprKind::StrLength => super::scalar::str_length(self, cx, expr, rows),
            ExprKind::StrDecimal => super::scalar::str_decimal(self, cx, expr, rows),
            ExprKind::Imputation => super::scalar::imputation(self, cx, expr, rows),
            ExprKind::Temporal => super::scalar::temporal(self, cx, expr, rows),
            ExprKind::Literal => match &node.op {
                Op::Literal(value) => Ok(Values::repeat(ValueRef::from(value), rows.len())),
                _ => unreachable!("a literal node holds its value"),
            },
            ExprKind::Column => self.column_read(cx, expr, rows),
            ExprKind::Source => self.field_read(cx, expr, rows),
            ExprKind::Collect => super::scalar::collect(self, cx, expr, rows),
            ExprKind::Predicate | ExprKind::Derivation => {
                unreachable!("predicates and derivations are evaluated by their owners")
            }
        }
    }

    /// The operand of `role` at its position among the node's operands of that role.
    pub(super) fn operand(&self, expr: ExprId, role: OperandRole, position: usize) -> ExprId {
        self.plan
            .operands(expr)
            .iter()
            .filter(|operand| operand.role == role)
            .nth(position)
            .expect("an admitted operand")
            .expr
    }

    /// The node's site of one handler kind.
    pub(super) fn site(&self, expr: ExprId, kind: HandlerKind) -> Option<HandlerSiteId> {
        self.plan
            .sites(expr)
            .iter()
            .map(|site| site.site)
            .find(|&site| self.plan.handler(site).kind == kind)
    }

    /// Count one firing at a node's site of `kind`; `path` names the site
    /// when the node lists none.
    pub(super) fn fire_node(
        &mut self,
        expr: ExprId,
        kind: HandlerKind,
        path: impl FnOnce() -> String,
    ) -> Result<(), Fail<E>> {
        match self.site(expr, kind) {
            Some(site) => self.fire(site),
            None => self.fire_path(&path(), kind),
        }
    }

    /// Count `firings` firings at a full site path, through the node's site
    /// when it lists one: what as many one-row firings count. No firing
    /// counts nothing, and registers no site.
    pub(super) fn fire_many(
        &mut self,
        expr: Option<ExprId>,
        path: &str,
        kind: HandlerKind,
        firings: u64,
    ) -> Result<(), Fail<E>> {
        if firings == 0 {
            return Ok(());
        }
        let plan = self.plan;
        let site = expr.and_then(|expr| {
            plan.sites(expr).iter().map(|site| site.site).find(|&site| {
                let entry = plan.handler(site);
                entry.kind == kind && &*entry.path == path
            })
        });
        match site {
            Some(site) => self.handlers.fire(plan, site, firings),
            None => self.handlers.fire_path(plan, path, kind, firings),
        }
        .map_err(|error| Box::new(ExecutionError::HandlerAccounting(error)))
    }

    /// Count one firing at a full site path, through the node's site when
    /// it lists one.
    pub(super) fn fire_at(
        &mut self,
        expr: Option<ExprId>,
        path: &str,
        kind: HandlerKind,
    ) -> Result<(), Fail<E>> {
        if let Some(expr) = expr {
            if let Some(site) = self
                .plan
                .sites(expr)
                .iter()
                .map(|site| site.site)
                .find(|&site| {
                    let entry = self.plan.handler(site);
                    entry.kind == kind && &*entry.path == path
                })
            {
                return self.fire(site);
            }
        }
        self.fire_path(path, kind)
    }

    /// A completed current value of the row.
    fn column_read(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
    ) -> Result<Values, Fail<E>> {
        let read = self.plan.reads(expr)[0];
        let mut values = Values::missing(0);
        match read {
            Read::Column(column) => {
                let local = cx.local(self.plan, column);
                for &row in rows {
                    values.push(cx.frame.get(row, local));
                }
            }
            Read::Relation(_, field) => {
                let record = cx
                    .record
                    .expect("a derived field is read inside a derivation");
                for position in 0..rows.len() {
                    values.push(record.derived(position, field as usize));
                }
            }
            _ => unreachable!("a column node reads a column"),
        }
        Ok(values)
    }

    /// A raw field of the row's driver record, of the record a derivation
    /// enriches, or a match expression's operand.
    fn field_read(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
    ) -> Result<Values, Fail<E>> {
        let read = self.plan.reads(expr)[0];
        let mut values = Values::missing(0);
        match read {
            Read::Field(field) => {
                if let (Some(operands), Op::MatchOperand { index }) =
                    (cx.operands, &self.plan.expr(expr).op)
                {
                    let value = super::relations::operand_value(*index, operands);
                    return Ok(Values::repeat(ValueRef::from(value), rows.len()));
                }
                return driver_field(cx, field as usize, rows, &self.plan.expr(expr).path);
            }
            Read::Column(column) => {
                // A match operand that reads a completed output column.
                let local = cx.local(self.plan, column);
                for &row in rows {
                    values.push(cx.frame.get(row, local));
                }
            }
            Read::Relation(_, field) => {
                let record = cx
                    .record
                    .expect("a donor field is read inside a derivation");
                for position in 0..rows.len() {
                    values.push(record.field(self.plan, expr, position, field as usize)?);
                }
            }
            Read::Window(_) => unreachable!("a source node reads a field"),
        }
        Ok(values)
    }

    /// A source binding evaluates its bound read.
    fn source_binding(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
        retained: bool,
    ) -> Result<Values, Fail<E>> {
        let operand = self.operand(expr, OperandRole::Value, 0);
        self.eval(cx, operand, rows, retained)
    }

    /// Only the first TRUE branch is evaluated; FALSE and UNKNOWN continue.
    fn case(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
        retained: bool,
    ) -> Result<Values, Fail<E>> {
        let operands = self.plan.operands(expr).to_vec();
        let mut assembly = Assembly::new(rows.len());
        let mut remaining: Vec<usize> = (0..rows.len()).collect();
        let mut index = 0;
        while index < operands.len() && !remaining.is_empty() {
            let selected: Vec<usize> = if operands[index].role == OperandRole::When {
                let condition = operands[index].expr;
                index += 1;
                let subset: Vec<usize> = remaining.iter().map(|&position| rows[position]).collect();
                let truths = super::predicates::case_truth(self, cx, expr, condition, &subset)?;
                let mut chosen = Vec::new();
                let mut rest = Vec::new();
                for (&position, truth) in remaining.iter().zip(truths) {
                    if truth == crate::expression::Truth::True {
                        chosen.push(position);
                    } else {
                        rest.push(position);
                    }
                }
                remaining = rest;
                chosen
            } else {
                std::mem::take(&mut remaining)
            };
            let branch = operands[index].expr;
            index += 1;
            if !selected.is_empty() {
                let subset: Vec<usize> = selected.iter().map(|&position| rows[position]).collect();
                let values = self.eval(cx, branch, &subset, retained)?;
                assembly.part(&selected, values);
            }
        }
        Ok(assembly.finish())
    }

    /// The first present operand in authored order, each later one read only
    /// for the rows every earlier one left missing; then the declared answer
    /// for rows all missing, counted either way.
    fn first_available(
        &mut self,
        cx: &Cx<'_, E>,
        expr: ExprId,
        rows: &[usize],
        retained: bool,
    ) -> Result<Values, Fail<E>> {
        let Op::FirstAvailable {
            missing,
            missing_path,
            ..
        } = &self.plan.expr(expr).op
        else {
            unreachable!("a first_available node holds its answer")
        };
        let missing = missing.clone();
        let missing_path = missing_path.clone();
        let operands: Vec<ExprId> = self
            .plan
            .operands(expr)
            .iter()
            .map(|operand| operand.expr)
            .collect();
        let mut assembly = Assembly::new(rows.len());
        let mut remaining: Vec<usize> = (0..rows.len()).collect();
        for operand in operands {
            if remaining.is_empty() {
                break;
            }
            let subset: Vec<usize> = remaining.iter().map(|&position| rows[position]).collect();
            let values = self
                .eval(cx, operand, &subset, retained)
                .map_err(|mut error| {
                    // Collection cardinality belongs to the enclosing selection.
                    if let ExecutionError::MultipleValues { path, .. } = error.as_mut() {
                        *path = self.plan.expr(expr).path.to_string();
                    }
                    error
                })?;
            let mut present = Vec::new();
            let mut present_values = Values::missing(0);
            let mut rest = Vec::new();
            for (index, &position) in remaining.iter().enumerate() {
                let value = values.get(index);
                if matches!(value, ValueRef::Missing) {
                    rest.push(position);
                } else {
                    present.push(position);
                    present_values.push(value);
                }
            }
            if !present.is_empty() {
                assembly.part(&present, present_values);
            }
            remaining = rest;
        }
        if !remaining.is_empty() {
            for _ in &remaining {
                self.fire_node(expr, HandlerKind::Missing, || missing_path.clone())?;
            }
            assembly.part(
                &remaining,
                Values::repeat(ValueRef::from(&missing), remaining.len()),
            );
        }
        Ok(assembly.finish())
    }

    /// The output keys of a row when every key is complete: a partial key is
    /// unavailable, whereas a completed missing key is a known value.
    pub(super) fn failure_identity(&self, cx: &Cx<'_, E>, row: usize) -> Option<RowIdentity> {
        let keys: &[_] = match self.plan.region(cx.region).kind {
            RegionKind::Dataset => self.plan.keys(),
            RegionKind::Donor { .. } => &[],
        };
        let locals: Vec<usize> = keys.iter().map(|&key| cx.local(self.plan, key)).collect();
        if locals.iter().any(|&key| !cx.frame.is_completed(row, key)) {
            return None;
        }
        Some(RowIdentity {
            position: row,
            values: locals.iter().map(|&key| cx.frame.value(row, key)).collect(),
        })
    }

    /// A semantic diagnostic, with the row's keys when its phase retains them.
    pub(super) fn retained_diagnostic(
        &self,
        cx: &Cx<'_, E>,
        diagnostic: crate::diagnostic::Diagnostic,
        retain_keys: bool,
        row: usize,
    ) -> Fail<E> {
        let identity = if retain_keys {
            self.failure_identity(cx, row)
        } else {
            None
        };
        Box::new(ExecutionError::SemanticDiagnostic {
            diagnostic,
            identity,
        })
    }
}

/// One raw field of each selected row's driver record. A port that holds
/// the field as typed storage lends it whole, and the selection's values are
/// gathered from it in one pass (#4190); a cell that fails names `path`.
pub(super) fn driver_field<E>(
    cx: &Cx<'_, E>,
    field: usize,
    rows: &[usize],
    path: &str,
) -> Result<Values, Fail<E>> {
    if let Some(column) = cx.table.storage(field) {
        let source_rows: Vec<usize> = rows.iter().map(|&row| cx.frame.members(row)[0]).collect();
        if let Some(values) = Values::from_column(column, &source_rows) {
            return super::reserved(values);
        }
    }
    let mut values = Values::missing(0);
    for &row in rows {
        let source_row = cx.frame.members(row)[0];
        let value = cx.table.get(source_row, field).map_err(|error| {
            Box::new(ExecutionError::Cell {
                path: path.into(),
                source_row,
                error,
            })
        })?;
        values.push(value);
    }
    Ok(values)
}

/// An owned copy of one value.
pub(super) fn owned(value: ValueRef<'_>) -> Value {
    super::buffer::own(value)
}
