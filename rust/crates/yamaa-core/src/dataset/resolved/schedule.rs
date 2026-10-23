//! The schedule: the evaluation order, built from the resolved graph.
//!
//! The compiler already fixes the order: row bindings in each template's
//! dependency order, the prewindow/window split, the column phase in the
//! compiler's column order, column checks once their declared prefix and
//! where columns are complete, then output keys, dataset checks in
//! declaration order and presentation order.
use super::*;

pub(super) fn build(plan: &mut ResolvedPlan) -> Result<(), ResolutionError> {
    for index in 0..plan.regions.len() {
        let region = RegionId(index as u32);
        let steps = match plan.regions[index].kind {
            RegionKind::Dataset => dataset(plan, region)?,
            RegionKind::Donor { .. } => donor(plan, region),
        };
        let start = plan.schedule.len();
        plan.schedule.extend(steps.into_iter().map(|step| StepInfo {
            step,
            checkpoint: false,
            partition: PartitionClass::Safe,
            effects: Effects::Pure,
        }));
        plan.regions[index].schedule = u32::try_from(start)
            .ok()
            .zip(u32::try_from(plan.schedule.len()).ok())
            .map(|(start, end)| start..end)
            .ok_or(ResolutionError::Capacity)?;
    }
    ready(plan)
}

/// The step at which each check is ready: its own step, the output key
/// step, or the first intermediate verification that covers it.
fn ready(plan: &mut ResolvedPlan) -> Result<(), ResolutionError> {
    let dataset = range(&plan.region(RegionId::DATASET).schedule);
    let position = |wanted: &dyn Fn(&Step) -> bool| {
        dataset
            .clone()
            .find(|&index| wanted(&plan.schedule[index].step))
            .map(StepId::try_from)
            .transpose()
    };
    let keys = position(&|step| matches!(step, Step::OutputKeys))?;
    let inputs = position(&|step| matches!(step, Step::VerifyIntermediates { self_rows: false }))?;
    let rows = position(&|step| matches!(step, Step::VerifyIntermediates { self_rows: true }))?;
    let mut own = vec![None; plan.checks.len()];
    for index in dataset.clone() {
        if let Step::Checks(check) = plan.schedule[index].step {
            own[check.index()] = Some(StepId::try_from(index)?);
        }
    }
    for (index, check) in plan.checks.iter_mut().enumerate() {
        let step = match check.scope {
            CheckScope::OutputKeys => keys,
            CheckScope::Intermediate(relation) => {
                if plan.relations[relation.index()].donor == Donor::SelfRows {
                    rows
                } else {
                    inputs
                }
            }
            CheckScope::Column(_) | CheckScope::Dataset => own[index],
        };
        // A check no step reaches keeps an unreachable position.
        check.ready_at = step.unwrap_or(StepId(u32::MAX));
    }
    Ok(())
}

fn dataset(plan: &ResolvedPlan, region: RegionId) -> Result<Vec<Step>, ResolutionError> {
    let mut steps = vec![Step::Admit, Step::VerifyIntermediates { self_rows: false }];
    let templates: Vec<_> = plan.region_templates(region).collect();
    let keyed = templates
        .first()
        .is_some_and(|&template| plan.template(template).driver.grain == Grain::Keys);
    if keyed {
        // Key grain: the source filter over every record, the key bindings
        // row by row, then one candidate per distinct key.
        let template = templates[0];
        if plan.template(template).filter.is_some() {
            steps.push(Step::Filter(template));
        }
        steps.extend(plan.row_bindings(template).map(Step::Row));
        steps.push(Step::KeyGrain);
    } else {
        for &template in &templates {
            let node = plan.template(template);
            steps.push(Step::Driver(template));
            steps.extend(
                plan.row_bindings(template)
                    .filter(|&binding| plan.binding(binding).stage == Stage::Prewindow)
                    .map(Step::Row),
            );
            let placement = node.filter.map(|filter| filter.placement);
            if placement == Some(FilterPlacement::Prewindow) {
                steps.push(Step::Filter(template));
            }
            steps.extend(
                plan.row_bindings(template)
                    .filter(|&binding| plan.binding(binding).stage == Stage::Window)
                    .map(Step::Window),
            );
            if placement == Some(FilterPlacement::Postwindow) {
                steps.push(Step::PostwindowFilter(template));
            }
            steps.push(Step::PublishSelf(template));
            steps.push(Step::VerifyIntermediates { self_rows: true });
        }
    }
    column_phase(plan, region, &templates, &mut steps)?;
    steps.push(Step::UnobservedFilterTypes);
    steps.push(Step::OutputKeys);
    steps.extend(
        plan.checks
            .iter()
            .enumerate()
            .filter(|(_, check)| check.scope == CheckScope::Dataset)
            .map(|(index, _)| Step::Checks(CheckId(index as u32))),
    );
    steps.push(Step::Order);
    Ok(steps)
}

/// Column steps in the compiler's order. A column check group runs once
/// every key, every column of its declared prefix and its where columns are
/// complete; groups therefore run in declared column order.
fn column_phase(
    plan: &ResolvedPlan,
    region: RegionId,
    templates: &[TemplateId],
    steps: &mut Vec<Step>,
) -> Result<(), ResolutionError> {
    const NEVER: usize = usize::MAX;
    let columns: Vec<_> = plan.region_columns(region).collect();
    let first = columns.first().map_or(0, |column| column.index());
    let order = &plan.region(region).column_order;
    // The event completing each column: 0 when every template completes it in
    // its row phase, otherwise the column step that completes it.
    let mut done: Vec<usize> = columns
        .iter()
        .map(|&column| {
            let row_complete = templates.iter().all(|&template| {
                plan.cell(template, column)
                    .is_some_and(|binding| plan.binding(binding).stage != Stage::Column)
            });
            if row_complete {
                0
            } else {
                NEVER
            }
        })
        .collect();
    for (position, column) in order.iter().enumerate() {
        let slot = &mut done[column.index() - first];
        if *slot == NEVER {
            *slot = position + 1;
        }
    }
    let completed = |read: &Read| match read {
        Read::Column(column) => done
            .get(column.index().wrapping_sub(first))
            .copied()
            .unwrap_or(NEVER),
        _ => 0,
    };
    let keys = plan
        .keys
        .iter()
        .map(|key| completed(&Read::Column(*key)))
        .max()
        .unwrap_or(0);
    let mut at: Vec<Vec<CheckId>> = vec![Vec::new(); order.len() + 1];
    let mut pointer = keys;
    for (offset, &column) in columns.iter().enumerate() {
        let group: Vec<_> = plan
            .checks
            .iter()
            .enumerate()
            .filter(|(_, check)| check.scope == CheckScope::Column(column))
            .map(|(index, _)| CheckId(index as u32))
            .collect();
        let mut need = done[offset];
        for &check in &group {
            for read in plan.check_reads(check) {
                need = need.max(completed(read));
            }
        }
        pointer = pointer.max(need);
        if pointer == NEVER {
            break;
        }
        at[pointer].extend(group);
    }
    steps.extend(at[0].iter().copied().map(Step::Checks));
    for (position, &column) in order.iter().enumerate() {
        steps.push(Step::Column(column));
        steps.extend(at[position + 1].iter().copied().map(Step::Checks));
    }
    Ok(())
}

/// A donor copies its stored fields row by row, then evaluates each
/// derivation over every donor row in declaration order.
fn donor(plan: &ResolvedPlan, region: RegionId) -> Vec<Step> {
    let mut steps = Vec::new();
    for template in plan.region_templates(region) {
        steps.push(Step::Driver(template));
        steps.extend(plan.row_bindings(template).map(Step::Row));
    }
    steps.extend(
        plan.region(region)
            .column_order
            .iter()
            .copied()
            .map(Step::Column),
    );
    steps
}

/// Cancellation checkpoints, partition classes and effects of every step.
pub(super) fn annotate(plan: &mut ResolvedPlan) {
    let mut classes = Classes {
        plan,
        donors: vec![None; plan.regions.len()],
    };
    let mut annotations = Vec::with_capacity(plan.schedule.len());
    for index in 0..plan.regions.len() {
        let region = RegionId(index as u32);
        let self_rows = plan
            .region_relations(region)
            .any(|relation| plan.relation(relation).donor == Donor::SelfRows);
        let mut template = None;
        let mut after_keys = false;
        for info in plan.region_schedule(region) {
            let step = info.step;
            let owner = step_template(plan, step);
            let first_of_template = owner.is_some() && owner != template;
            if owner.is_some() {
                template = owner;
            }
            let checkpoint = region == RegionId::DATASET
                && (first_of_template
                    || after_keys
                    || matches!(
                        step,
                        Step::Admit
                            | Step::VerifyIntermediates { .. }
                            | Step::KeyGrain
                            | Step::Driver(_)
                            | Step::Column(_)
                            | Step::OutputKeys
                    )
                    || matches!(step, Step::Checks(check) if plan.check(check).scope == CheckScope::Dataset));
            after_keys = matches!(step, Step::OutputKeys);
            let mut partition = match step {
                Step::Admit | Step::VerifyIntermediates { .. } | Step::Order => {
                    PartitionClass::Global
                }
                Step::KeyGrain | Step::Driver(_) | Step::OutputKeys => PartitionClass::CombineStep,
                Step::PublishSelf(_) if self_rows => PartitionClass::Global,
                Step::Checks(check) => check_class(plan.check(check).family),
                _ => PartitionClass::Safe,
            };
            let mut effects = Effects::Pure;
            let converts = |binding: BindingId| plan.binding(binding).unconvertible.is_some();
            match step {
                Step::Row(binding) | Step::Window(binding) if converts(binding) => {
                    effects = Effects::Handlers
                }
                Step::Column(column) if plan.column_bindings(column).any(converts) => {
                    effects = Effects::Handlers
                }
                // A named driver materializes its donor rows in its driver step.
                Step::Driver(template) => {
                    if let Some(relation) = plan.template(template).driver.named {
                        let (_, effect) = classes.relation(relation, &mut Vec::new());
                        effects = effects.max(effect);
                    }
                }
                _ => (),
            }
            for expr in step_exprs(plan, step) {
                let (class, effect) = classes.expr(expr);
                partition = partition.max(class);
                effects = effects.max(effect);
            }
            annotations.push((checkpoint, partition, effects));
        }
    }
    for (info, (checkpoint, partition, effects)) in plan.schedule.iter_mut().zip(annotations) {
        info.checkpoint = checkpoint;
        info.partition = partition;
        info.effects = effects;
    }
}

impl ResolvedPlan {
    /// The first dataset step a partitioned run cannot divide (#3697): a
    /// step the schedule marks [`PartitionClass::Global`], whose evaluation
    /// reads a window, an aggregate, a named or joined relation, SELF rows
    /// or a project function; or the driver step of a template driven by a
    /// named intermediate whose materialization calls a project function or
    /// is global itself, through its derivations, its filter or its keys.
    /// The run's own reconciliation points, which the inventory also marks
    /// global, are not such steps, because the run holds what they need:
    /// admission over the complete inputs, intermediate verification, SELF
    /// publication, the final order over the complete accepted relation, and
    /// the declaration checks, which read no data.
    pub fn whole_table_step(&self) -> Option<Step> {
        self.region_schedule(RegionId::DATASET)
            .iter()
            .find(|info| match info.step {
                Step::Admit
                | Step::VerifyIntermediates { .. }
                | Step::PublishSelf(_)
                | Step::Order => false,
                Step::Checks(check) => {
                    info.partition == PartitionClass::Global
                        && !matches!(
                            self.check(check).family,
                            CheckFamily::InvalidDiagnostic
                                | CheckFamily::InvalidDeclaration
                                | CheckFamily::PredicateDeclaration
                        )
                }
                // The schedule marks a driver a combine step; a named one
                // materializes its intermediate in that step.
                Step::Driver(template) => {
                    info.effects == Effects::Callbacks
                        || self
                            .template(template)
                            .driver
                            .named
                            .is_some_and(|relation| {
                                materialized_class(self, relation) == PartitionClass::Global
                            })
                }
                _ => info.partition == PartitionClass::Global,
            })
            .map(|info| info.step)
    }

    /// The authored path a dataset step evaluates: its binding's, its
    /// filter's, its column's first binding's, its check's, or a named
    /// driver's intermediate; empty for a step that evaluates nothing
    /// authored.
    pub fn step_path(&self, step: Step) -> &str {
        match step {
            Step::Row(binding) | Step::Window(binding) => &self.binding(binding).path,
            Step::Filter(template) | Step::PostwindowFilter(template) => self
                .template(template)
                .filter
                .map_or("", |filter| &self.expr(filter.expr).path),
            Step::Column(column) => self
                .column_bindings(column)
                .next()
                .map_or("", |binding| &self.binding(binding).path),
            Step::Checks(check) => &self.check(check).path,
            Step::Driver(template) => self
                .template(template)
                .driver
                .named
                .map_or("", |relation| &self.relation(relation).path),
            _ => "",
        }
    }
}

/// The partition class of materializing a named relation: the strongest
/// class of its donor plan's derivations, its filter and its key
/// expressions.
fn materialized_class(plan: &ResolvedPlan, relation: RelationId) -> PartitionClass {
    let mut classes = Classes {
        plan,
        donors: vec![None; plan.regions.len()],
    };
    let node = plan.relation(relation);
    let mut exprs: Vec<ExprId> = node.filter.into_iter().collect();
    exprs.extend(node.keys.iter().filter_map(|key| match key.value {
        KeyValue::Expr(expr) => Some(expr),
        _ => None,
    }));
    if let Some(region) = node.derivations {
        exprs.extend(
            plan.region_bindings(region)
                .map(|binding| plan.binding(binding).expr),
        );
    }
    exprs
        .into_iter()
        .map(|expr| classes.expr(expr).0)
        .max()
        .unwrap_or(PartitionClass::Safe)
}

fn step_template(plan: &ResolvedPlan, step: Step) -> Option<TemplateId> {
    match step {
        Step::Driver(template)
        | Step::Filter(template)
        | Step::PostwindowFilter(template)
        | Step::PublishSelf(template) => Some(template),
        Step::Row(binding) | Step::Window(binding) => {
            plan.binding(binding).templates.first().copied()
        }
        _ => None,
    }
}

/// The expression roots a step evaluates.
pub(super) fn step_exprs(plan: &ResolvedPlan, step: Step) -> Vec<ExprId> {
    match step {
        Step::Row(binding) | Step::Window(binding) => vec![plan.binding(binding).expr],
        Step::Filter(template) | Step::PostwindowFilter(template) => plan
            .template(template)
            .filter
            .map(|filter| filter.expr)
            .into_iter()
            .collect(),
        Step::Column(column) => {
            let mut exprs: Vec<ExprId> = Vec::new();
            for binding in plan.column_bindings(column) {
                let expr = plan.binding(binding).expr;
                if !exprs.contains(&expr) {
                    exprs.push(expr);
                }
            }
            exprs
        }
        Step::Checks(check) => plan.check(check).predicates.clone(),
        _ => Vec::new(),
    }
}

fn check_class(family: CheckFamily) -> PartitionClass {
    match family {
        CheckFamily::InvalidDiagnostic
        | CheckFamily::InvalidDeclaration
        | CheckFamily::PredicateDeclaration
        | CheckFamily::IntermediateUnique => PartitionClass::Global,
        _ => PartitionClass::CombineStep,
    }
}

/// Partition classes and effects of expressions. Each donor region's effects
/// are computed once, however many reads materialize it.
struct Classes<'a> {
    plan: &'a ResolvedPlan,
    donors: Vec<Option<Effects>>,
}

impl Classes<'_> {
    /// Callbacks and reported conversions of a donor plan's derivations.
    fn donor(&mut self, region: RegionId) -> Effects {
        if let Some(effects) = self.donors[region.index()] {
            return effects;
        }
        // Donor plans read only earlier intermediates; the provisional entry
        // keeps the walk finite regardless.
        self.donors[region.index()] = Some(Effects::Pure);
        let plan = self.plan;
        let mut effects = if plan.conversions(region).is_empty() {
            Effects::Pure
        } else {
            Effects::Handlers
        };
        for binding in plan.region_bindings(region) {
            let (_, effect) = self.expr(plan.binding(binding).expr);
            effects = effects.max(effect);
        }
        self.donors[region.index()] = Some(effects);
        effects
    }

    /// Reading a relation: its class, and the effects of selecting from it
    /// (key expressions and the nearest reference, filter, and a donor plan
    /// it materializes).
    fn relation(
        &mut self,
        relation: RelationId,
        pending: &mut Vec<ExprId>,
    ) -> (PartitionClass, Effects) {
        let node = self.plan.relation(relation);
        let class = match node.kind {
            RelationKind::OdmItems => PartitionClass::CombineStep,
            _ => PartitionClass::Global,
        };
        pending.extend(node.filter);
        let keys = node.keys.iter().chain(&node.nearest);
        pending.extend(keys.filter_map(|key| match key.value {
            KeyValue::Expr(expr) => Some(expr),
            _ => None,
        }));
        let effects = node
            .derivations
            .map_or(Effects::Pure, |donor| self.donor(donor));
        (class, effects)
    }

    /// The strongest partition class and effect of an expression and
    /// everything it reaches: operands, relations and windows.
    fn expr(&mut self, root: ExprId) -> (PartitionClass, Effects) {
        let plan = self.plan;
        let mut class = PartitionClass::Safe;
        let mut effects = Effects::Pure;
        let mut pending = vec![root];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(expr) = pending.pop() {
            if !seen.insert(expr) {
                continue;
            }
            class = class.max(match plan.expr(expr).kind {
                ExprKind::Collect | ExprKind::Odm(_) => PartitionClass::CombineStep,
                ExprKind::ProjectFunction
                | ExprKind::Window(_)
                | ExprKind::ContextWindow(_)
                | ExprKind::Intermediate(_)
                | ExprKind::Joined(_)
                | ExprKind::JoinedSource(_)
                | ExprKind::SourceLookup(_)
                | ExprKind::Aggregate(_) => PartitionClass::Global,
                _ => PartitionClass::Safe,
            });
            if plan.expr(expr).kind == ExprKind::ProjectFunction {
                effects = effects.max(Effects::Callbacks);
            }
            if !plan.sites(expr).is_empty() {
                effects = effects.max(Effects::Handlers);
            }
            pending.extend(plan.operands(expr).iter().map(|operand| operand.expr));
            for read in plan.reads(expr) {
                match *read {
                    Read::Relation(relation, _) => {
                        let (relation_class, effect) = self.relation(relation, &mut pending);
                        class = class.max(relation_class);
                        effects = effects.max(effect);
                    }
                    Read::Window(window) => {
                        class = PartitionClass::Global;
                        pending.extend(plan.window(window).filter);
                    }
                    Read::Column(_) | Read::Field(_) => (),
                }
            }
        }
        (class, effects)
    }
}
