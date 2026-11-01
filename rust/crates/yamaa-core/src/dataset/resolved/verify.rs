//! The resolution check, run on every bind whatever the build profile:
//! topological validity. Every data edge reads a cell that an earlier step of
//! the reading cell's template completed, windows and output scans read
//! complete relations, and every cell is completed.
//!
//! The first failure refuses the plan and names the step that fails.
use super::schedule::step_exprs;
use super::*;

pub(super) fn verify(plan: &ResolvedPlan) -> Result<(), ResolutionError> {
    for index in 0..plan.regions.len() {
        topology(plan, RegionId(index as u32))?;
    }
    Ok(())
}

const NEVER: usize = usize::MAX;

/// The templates a read is evaluated for, and whether the template's window
/// stage has begun.
#[derive(Clone, Copy)]
enum Scope<'a> {
    Row {
        template: TemplateId,
        windowed: bool,
    },
    Cells(&'a [TemplateId]),
    Candidates,
}

/// Topological validity of one region's schedule.
fn topology(plan: &ResolvedPlan, region: RegionId) -> Result<(), ResolutionError> {
    let entry = plan.region(region);
    let steps = plan.region_schedule(region);
    let templates: Vec<_> = plan.region_templates(region).collect();
    let width = (entry.columns.end - entry.columns.start) as usize;
    let mut check = Topology {
        plan,
        region,
        templates: &templates,
        width,
        complete: vec![NEVER; templates.len() * width],
        position: 0,
    };
    for (position, info) in steps.iter().enumerate() {
        check.position = position;
        let step = info.step;
        match step {
            Step::Row(binding) | Step::Window(binding) => {
                let node = plan.binding(binding);
                let template = *node.templates.first().ok_or_else(|| {
                    check.failure(
                        step,
                        format!("row binding {} serves no template", node.path),
                    )
                })?;
                let windowed = matches!(step, Step::Window(_));
                check.expr(step, node.expr, Scope::Row { template, windowed })?;
            }
            Step::Filter(template) | Step::PostwindowFilter(template) => {
                let windowed = matches!(step, Step::PostwindowFilter(_));
                for expr in step_exprs(plan, step) {
                    check.expr(step, expr, Scope::Row { template, windowed })?;
                }
            }
            Step::Column(column) => {
                for member in plan.step_columns(column) {
                    for binding in plan.column_bindings(member) {
                        let node = plan.binding(binding);
                        check.expr(step, node.expr, Scope::Cells(&node.templates))?;
                        for &template in &node.templates {
                            let offset = check.offset(member).expect("same region");
                            let row = check.row(template);
                            check.complete[row * check.width + offset] = position;
                        }
                    }
                }
            }
            Step::Checks(id) => {
                for read in plan.check_reads(id) {
                    check.read(step, *read, Scope::Candidates, &mut Vec::new())?;
                }
                for &expr in &plan.check(id).predicates {
                    check.expr(step, expr, Scope::Candidates)?;
                }
            }
            Step::OutputKeys => {
                for (index, node) in plan.checks.iter().enumerate() {
                    if node.scope == CheckScope::OutputKeys {
                        for read in plan.check_reads(CheckId(index as u32)) {
                            check.read(step, *read, Scope::Candidates, &mut Vec::new())?;
                        }
                    }
                }
            }
            _ => (),
        }
        check.complete_step(step)?;
    }
    // Every cell of every template is completed by the end of the region.
    for (row, &template) in templates.iter().enumerate() {
        for offset in 0..width {
            if check.complete[row * width + offset] == NEVER {
                let column = ColumnId(entry.columns.start + offset as u32);
                let last = steps.last().map_or(Step::Admit, |info| info.step);
                check.position = steps.len().saturating_sub(1);
                return Err(check.failure(
                    last,
                    format!(
                        "column {} of template {} is never completed",
                        plan.column(column).name,
                        template.0 - entry.templates.start
                    ),
                ));
            }
        }
    }
    Ok(())
}

struct Topology<'a> {
    plan: &'a ResolvedPlan,
    region: RegionId,
    templates: &'a [TemplateId],
    width: usize,
    /// The step completing each (template, column) cell of the region.
    complete: Vec<usize>,
    position: usize,
}

impl Topology<'_> {
    fn failure(&self, step: Step, read: String) -> ResolutionError {
        ResolutionError::Topology {
            region: self.region.index(),
            position: self.position,
            step,
            read,
        }
    }
    fn row(&self, template: TemplateId) -> usize {
        (template.0 - self.plan.region(self.region).templates.start) as usize
    }
    fn offset(&self, column: ColumnId) -> Option<usize> {
        let start = self.plan.region(self.region).columns.start;
        column
            .0
            .checked_sub(start)
            .map(|offset| offset as usize)
            .filter(|&offset| offset < self.width)
    }
    /// Record the cells this step completes, checking each against its cell binding.
    fn complete_step(&mut self, step: Step) -> Result<(), ResolutionError> {
        let bindings: Vec<BindingId> = match step {
            Step::Row(binding) | Step::Window(binding) => vec![binding],
            Step::Column(column) => self.plan.step_bindings(column).collect(),
            _ => return Ok(()),
        };
        for binding in bindings {
            let node = self.plan.binding(binding);
            let offset = self.offset(node.column).ok_or_else(|| {
                self.failure(
                    step,
                    format!("binding {} completes another region", node.path),
                )
            })?;
            for &template in &node.templates {
                if self.plan.cell(template, node.column) != Some(binding) {
                    return Err(self.failure(
                        step,
                        format!(
                            "binding {} is not the cell binding of column {}",
                            node.path,
                            self.plan.column(node.column).name
                        ),
                    ));
                }
                let row = self.row(template);
                self.complete[row * self.width + offset] = self.position;
            }
        }
        Ok(())
    }
    fn expr(&self, step: Step, expr: ExprId, scope: Scope<'_>) -> Result<(), ResolutionError> {
        let mut visiting = Vec::new();
        self.expr_in(step, expr, scope, &mut visiting)
    }
    fn expr_in(
        &self,
        step: Step,
        expr: ExprId,
        scope: Scope<'_>,
        visiting: &mut Vec<RelationId>,
    ) -> Result<(), ResolutionError> {
        for read in self.plan.reads(expr) {
            self.read(step, *read, scope, visiting)?;
        }
        for operand in self.plan.operands(expr) {
            self.expr_in(step, operand.expr, scope, visiting)?;
        }
        Ok(())
    }
    /// A completed cell read by every template in scope, before this step.
    fn cell(
        &self,
        step: Step,
        column: ColumnId,
        templates: &[TemplateId],
    ) -> Result<(), ResolutionError> {
        let offset = self.offset(column).ok_or_else(|| {
            self.failure(step, format!("reads column {} of another region", column.0))
        })?;
        for &template in templates {
            // Completions are recorded as the schedule advances, so an
            // unrecorded cell is one no earlier step completes.
            if self.complete[self.row(template) * self.width + offset] == NEVER {
                return Err(self.failure(
                    step,
                    format!(
                        "reads column {} of template {} before any step completes it",
                        self.plan.column(column).name,
                        self.row(template)
                    ),
                ));
            }
        }
        Ok(())
    }
    fn read(
        &self,
        step: Step,
        read: Read,
        scope: Scope<'_>,
        visiting: &mut Vec<RelationId>,
    ) -> Result<(), ResolutionError> {
        match read {
            Read::Field(_) => Ok(()),
            Read::Column(column) => match scope {
                Scope::Row { template, .. } => self.cell(step, column, &[template]),
                Scope::Cells(templates) => self.cell(step, column, templates),
                Scope::Candidates => self.cell(step, column, self.templates),
            },
            Read::Window(window) => self.window(step, window, scope),
            Read::Relation(relation, _) => {
                if visiting.contains(&relation) {
                    return Ok(());
                }
                visiting.push(relation);
                let result = self.relation(step, relation, scope, visiting);
                visiting.pop();
                result
            }
        }
    }
    /// A window reads its whole scope: the template's rows after its
    /// prewindow filter, or every candidate in the column phase.
    fn window(
        &self,
        step: Step,
        window: WindowId,
        scope: Scope<'_>,
    ) -> Result<(), ResolutionError> {
        let node = self.plan.window(window);
        let rows: Vec<TemplateId> = match scope {
            Scope::Row {
                windowed: false, ..
            } => {
                return Err(self.failure(
                    step,
                    format!("evaluates window {} before the window stage", node.path),
                ))
            }
            Scope::Row { template, .. } => vec![template],
            Scope::Cells(_) | Scope::Candidates => self.templates.to_vec(),
        };
        let reads = node
            .group_by
            .iter()
            .chain(node.order_by.iter().map(|term| &term.read));
        for read in reads {
            if let Read::Column(column) = read {
                self.cell(step, *column, &rows)?;
            }
        }
        let delayed = match (step, node.function) {
            (Step::Column(column), WindowFunction::RowValue { offset }) if offset < 0 => {
                self.plan.recurrence(column)
            }
            _ => None,
        };
        for read in &node.operands {
            if let Read::Column(column) = read {
                if !delayed.is_some_and(|group| group.columns.contains(column)) {
                    self.cell(step, *column, &rows)?;
                }
            }
        }
        if let Some(filter) = node.filter {
            for read in self.plan.reads(filter) {
                if let Read::Column(column) = read {
                    self.cell(step, *column, &rows)?;
                }
            }
        }
        Ok(())
    }
    /// A relation reads the current row through its keys, the reference of
    /// its nearest selection and its correlated operands; an output
    /// aggregate also scans every candidate.
    fn relation(
        &self,
        step: Step,
        relation: RelationId,
        scope: Scope<'_>,
        visiting: &mut Vec<RelationId>,
    ) -> Result<(), ResolutionError> {
        let node = self.plan.relation(relation);
        for key in node.keys.iter().chain(&node.nearest) {
            match &key.value {
                KeyValue::Read(read) | KeyValue::Padded(read) => {
                    self.read(step, *read, scope, visiting)?
                }
                KeyValue::Expr(expr) => self.expr_in(step, *expr, scope, visiting)?,
                KeyValue::Literal(_) => (),
                KeyValue::Unbound => {
                    return Err(self.failure(
                        step,
                        format!("reads {} whose key is unbound here", node.path),
                    ))
                }
            }
        }
        for read in &node.current {
            self.read(step, *read, scope, visiting)?;
        }
        if let Some(range) = &node.range {
            self.read(step, range.current, scope, visiting)?;
        }
        if let Some(filter) = node.filter {
            self.expr_in(step, filter, scope, visiting)?;
        }
        if !node.scan.is_empty() {
            if let Scope::Row { .. } = scope {
                return Err(self.failure(
                    step,
                    format!(
                        "scans the output relation {} during the row phase",
                        node.path
                    ),
                ));
            }
            for &column in &node.scan {
                self.cell(step, column, self.templates)?;
            }
        }
        Ok(())
    }
}
