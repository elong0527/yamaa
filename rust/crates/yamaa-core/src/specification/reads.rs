//! The names a derivation's declaration names, as preparation has always
//! scanned them for its dependency checks: each verb's operands, formula and
//! predicate names, `case` conditions and results, project call arguments,
//! an aggregate's names, keys, groups, filter and range value, and, unless
//! left out, a window's operands, partitions, order and filter, followed by
//! every `str_template` placeholder.
//!
//! This is narrower than [`ast::Spec::reads`], which reads every name below
//! a root: a structured source's filter, order and bounds, an aggregate's
//! bounds and bindings, imputation bounds and fallbacks, and `odm` reads are
//! not scanned here, and `diff` and `study_day` operands are scanned twice.
//! Each of these decides a diagnostic in some spec, so the narrower scan
//! keeps them unchanged.
use super::*;
use crate::ast::{JoinKey, ScalarOp, Slot, WindowFunction};

/// Which names of an aggregate and of a window the scan includes.
#[derive(Clone, Copy)]
pub(super) struct Scan {
    /// An aggregate's qualified names and record relations, groups and
    /// filter names; without it only their bare names.
    pub relation: bool,
    /// A window's operands, partitions, order and filter.
    pub windows: bool,
}

/// Every name a dependency check scans `root` for, in scan order; `calls`
/// says whether project call arguments are among them.
pub(super) fn names(
    spec: &ast::Spec,
    root: ExprId,
    scan: Scan,
    calls: bool,
) -> Result<Vec<String>, PrepareError> {
    let mut names = Vec::new();
    Scanner { spec, scan, calls }.scan(root, &mut names, 0)?;
    names.extend(placeholders(spec, root));
    Ok(names)
}

/// Each `str_template` placeholder at `root` or a `case` result below it.
pub(super) fn placeholders(spec: &ast::Spec, root: ExprId) -> Vec<String> {
    checks::positions(spec, root)
        .into_iter()
        .flat_map(|e| match &spec.expr(e).kind {
            ExprKind::Scalar {
                op: ScalarOp::StrTemplate { .. },
                operands,
                ..
            } => operands
                .iter()
                .filter_map(|operand| checks::name(spec, operand.expr).map(String::from))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

struct Scanner<'a> {
    spec: &'a ast::Spec,
    scan: Scan,
    calls: bool,
}

impl Scanner<'_> {
    fn name(&self, e: ExprId, names: &mut Vec<String>) {
        if let Some(name) = checks::name(self.spec, e) {
            names.push(name.into());
        }
    }
    /// A source operand's variable: a name, or a structured binding's.
    fn variable(&self, e: ExprId, names: &mut Vec<String>) {
        if let Some(name) = checks::variable(self.spec, e) {
            names.push(name.into());
        }
    }
    fn formula(
        &self,
        e: Option<ExprId>,
        keep: impl Fn(&str) -> bool,
        names: &mut Vec<String>,
    ) -> Result<(), PrepareError> {
        let Some(ExprKind::Formula(formula)) = e.map(|e| &self.spec.expr(e).kind) else {
            return Ok(());
        };
        match &formula.compiled {
            Ok(compiled) => names.extend(
                compiled
                    .identifiers()
                    .filter(|&name| keep(name))
                    .map(String::from),
            ),
            Err(CompileError::Invalid { .. }) => {}
            Err(error) => return Err(PrepareError::expression(error.clone(), None)),
        }
        Ok(())
    }
    fn scan(&self, e: ExprId, names: &mut Vec<String>, depth: usize) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                None,
            ));
        }
        let spec = self.spec;
        let relation = self.scan.relation;
        match &spec.expr(e).kind {
            // A `source` operation: its variable and its range value.
            ExprKind::Ref(name) => names.push(name.0.clone()),
            ExprKind::Source(read) => {
                self.name(read.variable, names);
                if let Some(between) = &read.between {
                    self.name(between.value, names);
                }
            }
            ExprKind::Scalar { op, operands, .. } => {
                let operand = |slot| operands.iter().find(|operand| operand.slot == slot);
                match op {
                    ScalarOp::Mapping { .. } | ScalarOp::Cut { .. } => {
                        if let Some(operand) = operand(Slot::Source) {
                            self.variable(operand.expr, names);
                        }
                    }
                    ScalarOp::Greatest | ScalarOp::Least => {
                        for operand in operands {
                            self.name(operand.expr, names);
                        }
                    }
                    ScalarOp::Diff { .. } | ScalarOp::StudyDay => {
                        // Scanned as operands, then again as the two dates.
                        for _ in 0..2 {
                            for operand in operands {
                                self.name(operand.expr, names);
                            }
                        }
                    }
                    ScalarOp::ToEpochDay
                    | ScalarOp::Shift { .. }
                    | ScalarOp::DurationValue { .. }
                    | ScalarOp::Dtc
                    | ScalarOp::TimeOfDay => {
                        for operand in operands {
                            self.name(operand.expr, names);
                        }
                    }
                    // A template's placeholders follow the scan.
                    ScalarOp::StrTemplate { .. } => {}
                    // Only the source: imputation bounds and its fallback are
                    // read when a source needs them.
                    _ => {
                        if let Some(operand) = operand(Slot::Source) {
                            self.name(operand.expr, names);
                        }
                    }
                }
            }
            ExprKind::Call { args, .. } => {
                if self.calls {
                    for &(_, value) in args {
                        self.name(value, names);
                    }
                }
            }
            ExprKind::Formula(_) => self.formula(Some(e), |_| true, names)?,
            ExprKind::Aggregate(aggregate) => {
                let filtered = |name: &str| relation || !name.contains('.');
                if let ExprKind::Formula(formula) = &spec.expr(aggregate.expr).kind {
                    match &formula.compiled {
                        Ok(compiled) => {
                            names.extend(
                                compiled
                                    .identifiers()
                                    .filter(|&name| filtered(name))
                                    .map(String::from),
                            );
                            if relation {
                                names.extend(
                                    compiled
                                        .record_relations()
                                        .map(|dataset| format!("{dataset}.*")),
                                );
                            }
                        }
                        Err(CompileError::Invalid { .. }) => {}
                        Err(error) => return Err(PrepareError::expression(error.clone(), None)),
                    }
                }
                match &aggregate.key {
                    Some(JoinKey::Columns { columns, .. }) => {
                        names.extend(columns.iter().map(|column| column.name.clone()))
                    }
                    Some(JoinKey::Pairs { pairs, .. }) => {
                        for &(_, value) in pairs {
                            match &spec.expr(value).kind {
                                ExprKind::Ref(name) => names.push(name.0.clone()),
                                _ => self.scan(value, names, depth + 1)?,
                            }
                        }
                    }
                    _ => {}
                }
                for &group in &aggregate.group_by {
                    if let Some(name) = checks::name(spec, group).filter(|&name| filtered(name)) {
                        names.push(name.into());
                    }
                }
                self.formula(aggregate.filter, filtered, names)?;
                if let Some(between) = &aggregate.between {
                    self.name(between.value, names);
                }
            }
            ExprKind::Flag { condition, .. } => self.formula(Some(*condition), |_| true, names)?,
            ExprKind::FirstAvailable { sources, .. } => {
                for &source in sources {
                    self.variable(source, names);
                }
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    self.formula(arm.condition, |_| true, names)?;
                    self.scan(arm.result, names, depth + 1)?;
                }
            }
            ExprKind::Window(window) if self.scan.windows => {
                // The operands in field order: source, date, reference_date,
                // value; then tie-break terms, partitions, order and filter.
                let operand = |slot| {
                    window
                        .operands
                        .iter()
                        .find(|operand| operand.slot == slot)
                        .map(|operand| operand.expr)
                };
                let (date, tie_break, order) = match &window.function {
                    WindowFunction::BaselineFlag {
                        window_order_by, ..
                    } => (
                        window.order_by.first(),
                        window.order_by.get(1..).unwrap_or_default(),
                        window_order_by.as_slice(),
                    ),
                    _ => (None, &[][..], window.order_by.as_slice()),
                };
                match &window.function {
                    WindowFunction::BaselineFlag { .. } => {
                        if let Some(date) = date {
                            self.name(date.variable, names);
                        }
                        for slot in [Slot::Reference, Slot::Source] {
                            if let Some(e) = operand(slot) {
                                self.name(e, names);
                            }
                        }
                    }
                    _ => {
                        if let Some(e) = operand(Slot::Source) {
                            self.name(e, names);
                        }
                    }
                }
                for term in tie_break {
                    self.name(term.variable, names);
                }
                for &group in &window.group_by {
                    self.name(group, names);
                }
                for term in order {
                    self.name(term.variable, names);
                }
                self.formula(window.filter, |_| true, names)?;
            }
            ExprKind::Window(_)
            | ExprKind::Literal(_)
            | ExprKind::Odm(_)
            | ExprKind::Unsupported(_) => {}
        }
        Ok(())
    }
}
