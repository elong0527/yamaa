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
            keyed: KeyedClass {
                keys: u32::MAX,
                class: PartitionClass::Safe,
            },
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
    let key_fields = key_fields(plan);
    let every = plan
        .region_templates(RegionId::DATASET)
        .all(|template| plan.reads_driver(template));
    let mut classes = Classes {
        plan,
        donors: vec![None; plan.regions.len()],
        key_fields: &key_fields,
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
            let mut class = Class::of(match step {
                Step::Admit | Step::VerifyIntermediates { .. } | Step::Order => {
                    PartitionClass::Global
                }
                Step::KeyGrain | Step::Driver(_) | Step::OutputKeys => PartitionClass::CombineStep,
                Step::PublishSelf(_) if self_rows => PartitionClass::Global,
                Step::Checks(check) => check_class(plan.check(check).family),
                _ => PartitionClass::Safe,
            });
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
                        let (_, effect) =
                            classes.relation(relation, Scope::default(), &mut Vec::new());
                        effects = effects.max(effect);
                    }
                }
                _ => (),
            }
            let scope = step_scope(plan, step, every);
            for expr in step_exprs(plan, step) {
                let (reached, effect) = classes.expr(expr, scope);
                class = class.max(reached);
                effects = effects.max(effect);
            }
            annotations.push((checkpoint, class, effects));
        }
    }
    for (info, (checkpoint, class, effects)) in plan.schedule.iter_mut().zip(annotations) {
        info.checkpoint = checkpoint;
        info.partition = class.rows;
        info.keyed = KeyedClass {
            keys: class.keys,
            class: class.keyed,
        };
        info.effects = effects;
    }
}

/// The driver field each declared key copies (#3763): the field that every
/// template reading the driver input binds the key to by a plain read whose
/// value keeps its type. None for a key that some such template derives any
/// other way, and for every key when no template reads the driver input.
fn key_fields(plan: &ResolvedPlan) -> Vec<Option<u32>> {
    let templates: Vec<TemplateId> = plan
        .region_templates(RegionId::DATASET)
        .filter(|&template| plan.reads_driver(template))
        .collect();
    plan.keys()
        .iter()
        .map(|&key| {
            let mut fields = templates
                .iter()
                .map(|&template| key_field(plan, template, key));
            let first = fields.next()??;
            fields.all(|field| field == Some(first)).then_some(first)
        })
        .collect()
}

/// The driver field `template` copies into `key`: a plain read of the field,
/// which converts nothing because the field's type is the column's, and for
/// a grouped template one of its group fields.
fn key_field(plan: &ResolvedPlan, template: TemplateId, key: ColumnId) -> Option<u32> {
    let binding = plan.binding(plan.cell(template, key)?);
    let node = plan.expr(binding.expr);
    let &[Read::Field(field)] = plan.reads(binding.expr) else {
        return None;
    };
    if node.kind != ExprKind::Source || !matches!(node.op, Op::Read) {
        return None;
    }
    let source = &plan.sources()[binding.driver.source.index()].schema;
    if Some(source.columns().get(field as usize)?.kind) != plan.column(key).kind {
        return None;
    }
    match &binding.driver.grain {
        Grain::Groups(fields) if !fields.contains(&field) => None,
        _ => Some(field),
    }
}

/// Whose rows `step`'s windows and aggregates read ([`Scope`]). `every`
/// says whether every template of the dataset reads the driver input.
fn step_scope(plan: &ResolvedPlan, step: Step, every: bool) -> Scope<'_> {
    match step {
        Step::Row(binding) | Step::Window(binding) => {
            let bound = plan.binding(binding);
            let rows = bound
                .templates
                .iter()
                .all(|&template| plan.reads_driver(template));
            Scope {
                driver: Some(&bound.driver),
                rows,
                candidates: rows,
            }
        }
        Step::Column(column) => Scope {
            driver: None,
            rows: plan
                .column_bindings(column)
                .flat_map(|binding| &plan.binding(binding).templates)
                .all(|&template| plan.reads_driver(template)),
            candidates: every,
        },
        _ => Scope::default(),
    }
}

impl ResolvedPlan {
    /// The first dataset step a partitioned run cannot divide (#3697): a
    /// step the schedule marks [`PartitionClass::Global`], whose evaluation
    /// reads a window, an aggregate, a named or joined relation that does not
    /// read another input held whole (#4191), SELF rows or a
    /// project function; or the driver step of a template driven by a
    /// named intermediate whose materialization calls a project function or
    /// is global itself, through its derivations, its filter or its keys.
    /// The run's own reconciliation points, which the inventory also marks
    /// global, are not such steps, because the run holds what they need:
    /// admission over the complete inputs, intermediate verification without
    /// global pair materialization, SELF
    /// publication, the final order over the complete accepted relation, and
    /// the declaration checks, which read no data.
    ///
    /// Under key-bounded partitions over the leading `keys` declared keys
    /// (#3763), a window or an aggregate that groups by every one of them
    /// needs no more than its partition ([`StepInfo::class`]); zero `keys`
    /// asks for partitions of consecutive records.
    ///
    /// A run that holds one partition of its driver input at a time (#3830)
    /// reads together only records one partition holds, so a plan whose
    /// records must meet needs the whole table unless its partitions keep
    /// them together (the confinement gate):
    /// - the driver step of a template driven by a named intermediate over
    ///   the driver input, and the verification of such an intermediate's
    ///   `unique` checks, whose records span every partition;
    /// - under key-bounded partitions, a grouped template or an ODM read
    ///   whose fields omit a partition key field, or any of them when the
    ///   driver input does not hold every partition key;
    /// - under consecutive partitions, the cross-record reads (grouped
    ///   templates, the key grain and ODM reads) once they share no driver
    ///   field to cut by ([`Self::cut_fields`]).
    pub fn whole_table_step(&self, keys: usize) -> Option<Step> {
        let unconfined = self.unconfined_step(keys);
        self.region_schedule(RegionId::DATASET)
            .iter()
            .find(|info| match info.step {
                _ if Some(info.step) == unconfined => true,
                // Pair ports are materialized globally at this checkpoint,
                // before any row, even when both sides read one input; so is
                // an intermediate over the driver input it verifies.
                Step::VerifyIntermediates { self_rows: false } => {
                    self.pair_ports() != 0 || self.verified_driver_intermediate().is_some()
                }
                Step::Admit
                | Step::VerifyIntermediates { .. }
                | Step::PublishSelf(_)
                | Step::Order => false,
                Step::Checks(check) => {
                    info.class(keys) == PartitionClass::Global
                        && !matches!(
                            self.check(check).family,
                            CheckFamily::InvalidDiagnostic
                                | CheckFamily::InvalidDeclaration
                                | CheckFamily::PredicateDeclaration
                        )
                }
                // The schedule marks a driver a combine step; a named one
                // materializes its intermediate in that step, over every
                // record when it reads the driver input.
                Step::Driver(template) => {
                    info.effects == Effects::Callbacks
                        || self
                            .template(template)
                            .driver
                            .named
                            .is_some_and(|relation| {
                                materialized_class(self, relation) == PartitionClass::Global
                                    || self.over_driver(relation)
                            })
                }
                _ => info.class(keys) == PartitionClass::Global,
            })
            .map(|info| info.step)
    }

    /// The driver fields a partitioned run cuts its driver input by, so that
    /// no partition splits what a step reads together (#3830): under
    /// key-bounded partitions, [`Self::partition_fields`]; under consecutive
    /// ones, the fields every cross-record read shares (a grouped template's
    /// group fields, the fields the key grain's keys copy, an ODM read's
    /// scope fields), none when the plan has no cross-record read. The cut
    /// keeps whole the runs of records whose fields are equal (`dataset::cut`).
    /// None when nothing keeps those records together: the input is then one
    /// partition, or, with a cross-record read, [`Self::whole_table_step`]
    /// names the step that refuses the run.
    pub fn cut_fields(&self, keys: usize) -> Option<Vec<usize>> {
        if keys > 0 {
            return self.partition_fields(keys);
        }
        let mut reads = cross_record_reads(self).into_iter();
        let Some((_, first)) = reads.next() else {
            return Some(Vec::new());
        };
        let shared = reads.fold(first, |mut shared, (_, fields)| {
            shared.retain(|field| fields.contains(field));
            shared
        });
        (!shared.is_empty()).then(|| shared.into_iter().map(|field| field as usize).collect())
    }

    /// The first cross-record step whose records no partition of the run's
    /// keeps together (#3830): under key-bounded partitions over `keys`, one
    /// whose fields omit a partition field, or the first when the driver
    /// input does not hold every partition key; under consecutive ones, the
    /// one whose fields leave no field shared with every earlier one.
    fn unconfined_step(&self, keys: usize) -> Option<Step> {
        let reads = cross_record_reads(self);
        if keys > 0 {
            let fields = self.partition_fields(keys);
            return reads
                .into_iter()
                .find(|(_, read)| {
                    fields.as_ref().is_none_or(|fields| {
                        fields.iter().any(|&field| !read.contains(&(field as u32)))
                    })
                })
                .map(|(step, _)| step);
        }
        let mut shared: Option<Vec<u32>> = None;
        for (step, fields) in reads {
            let shared = shared.get_or_insert_with(|| fields.clone());
            shared.retain(|field| fields.contains(field));
            if shared.is_empty() {
                return Some(step);
            }
        }
        None
    }

    /// Whether named relation `relation` is an intermediate over the driver
    /// input, whose records span every partition.
    fn over_driver(&self, relation: RelationId) -> bool {
        self.relation(relation)
            .named
            .as_deref()
            .is_some_and(|item| {
                matches!(
                    item.source,
                    crate::dataset::IntermediateSource::Primary { .. }
                )
            })
    }

    /// Whether the dataset's relation `relation` reads an input other than
    /// the driver input, held whole before any partition (#4191): an
    /// implicit join into such an input (`input.<id>.key`, a qualified
    /// source read), or a named intermediate over one that derives nothing.
    /// No partition of the driver input produces or changes a record of it,
    /// so every partition matches against the same records the whole-table
    /// run does. A relation over the driver input or `SELF`, a pair
    /// relation, and an intermediate whose donor plan derives values, which
    /// each partition would build and count again, stay global.
    fn reads_other_input(&self, relation: RelationId) -> bool {
        let node = self.relation(relation);
        let Donor::Source(source) = node.donor else {
            return false;
        };
        node.region == RegionId::DATASET
            && self.region_sources(RegionId::DATASET).next() != Some(source)
            && match node.kind {
                RelationKind::Join => true,
                RelationKind::Named { .. } => {
                    node.derivations.is_none()
                        && node.named.as_deref().is_some_and(|item| {
                            matches!(item.source, crate::dataset::IntermediateSource::Input(_))
                        })
                }
                _ => false,
            }
    }

    /// The first intermediate over the driver input with a `unique` check,
    /// which the run verifies over every record before any row.
    fn verified_driver_intermediate(&self) -> Option<RelationId> {
        self.region_relations(RegionId::DATASET).find(|&relation| {
            self.over_driver(relation)
                && self
                    .relation(relation)
                    .named
                    .as_deref()
                    .is_some_and(|item| !item.unique.is_empty())
        })
    }

    /// The driver fields that key-bounded partitions over the leading `keys`
    /// declared keys cut the driver input by (#3763): for each of those keys,
    /// the field that every template reading the driver input copies into it
    /// unchanged. None when the specification declares no key, or when a
    /// template derives one of those keys any other way: the partitions then
    /// have nothing to keep whole.
    pub fn partition_fields(&self, keys: usize) -> Option<Vec<usize>> {
        let fields = key_fields(self);
        let keys = keys.min(fields.len());
        if keys == 0 {
            return None;
        }
        fields[..keys]
            .iter()
            .map(|field| field.map(|field| field as usize))
            .collect()
    }

    /// The authored path a dataset step evaluates: its binding's, its
    /// filter's, its column's first binding's, its check's, a named driver's
    /// intermediate, a grouped template's `group_by`, the `keys` the key
    /// grain groups by, or the pair relation or intermediate over the driver
    /// input the first intermediate verification materializes; empty for a
    /// step that evaluates nothing authored.
    pub fn step_path(&self, step: Step) -> std::borrow::Cow<'_, str> {
        let path: &str = match step {
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
            Step::VerifyIntermediates { self_rows: false } => self
                .region_relations(RegionId::DATASET)
                .find(|&relation| self.relation(relation).donor == Donor::Pairs)
                .or_else(|| self.verified_driver_intermediate())
                .map_or("", |relation| &self.relation(relation).path),
            Step::Driver(template) => {
                let node = self.template(template);
                match (node.driver.named, &node.row) {
                    (Some(relation), _) => &self.relation(relation).path,
                    (None, Some(row)) => return format!("rows.{row}.group_by").into(),
                    (None, None) => "",
                }
            }
            Step::KeyGrain => "keys",
            _ => "",
        };
        path.into()
    }
}

/// The cross-record reads of the dataset (#3830), in schedule order, each
/// with the driver fields whose equal values keep its records together: a
/// grouped template that reads the driver input, its group fields; the key
/// grain, the fields its keys copy; an ODM read of the driver input, the
/// fields of its scope. An ODM read scoped by the keys reads its own row's
/// records, which the key grain or its template keeps together.
fn cross_record_reads(plan: &ResolvedPlan) -> Vec<(Step, Vec<u32>)> {
    let primary = plan.region_sources(RegionId::DATASET).next();
    let reads_driver =
        |driver: &DriverContext| driver.named.is_none() && Some(driver.source) == primary;
    let mut reads = Vec::new();
    for info in plan.region_schedule(RegionId::DATASET) {
        let step = info.step;
        match step {
            Step::Driver(template) if plan.reads_driver(template) => {
                if let Grain::Groups(fields) = &plan.template(template).driver.grain {
                    reads.push((step, fields.clone()));
                }
            }
            Step::KeyGrain => {
                let fields = key_fields(plan).into_iter().flatten().collect();
                reads.push((step, fields));
            }
            _ => (),
        }
        // Whether a read of the step's own driver reads the driver input.
        let over_driver = match step {
            Step::Row(binding) | Step::Window(binding) => {
                reads_driver(&plan.binding(binding).driver)
            }
            Step::Filter(template) | Step::PostwindowFilter(template) => {
                reads_driver(&plan.template(template).driver)
            }
            Step::Column(column) => plan
                .column_bindings(column)
                .any(|binding| reads_driver(&plan.binding(binding).driver)),
            _ => false,
        };
        for fields in odm_scopes(plan, step, over_driver) {
            reads.push((step, fields));
        }
    }
    reads
}

/// The scope fields of each ODM read of the driver input `step` evaluates;
/// `own` says whether a read of the step's own driver reads it.
fn odm_scopes(plan: &ResolvedPlan, step: Step, own: bool) -> Vec<Vec<u32>> {
    use crate::dataset::{AggregateSource, OdmScope};
    let mut scopes = Vec::new();
    let mut pending = step_exprs(plan, step);
    let mut seen = std::collections::BTreeSet::new();
    while let Some(expr) = pending.pop() {
        if !seen.insert(expr) {
            continue;
        }
        pending.extend(plan.operands(expr).iter().map(|operand| operand.expr));
        for read in plan.reads(expr) {
            if let Read::Window(window) = *read {
                pending.extend(plan.window(window).filter);
            }
        }
        let Op::Odm(bound) = &plan.expr(expr).op else {
            continue;
        };
        let read = bound.read();
        let over_driver = match read.source {
            None => own,
            Some(AggregateSource::Primary) => true,
            Some(AggregateSource::Secondary(_)) => false,
        };
        let fields: Option<Vec<usize>> = match &read.scope {
            OdmScope::Keys => None,
            OdmScope::Records => Some(crate::odm_source::HIERARCHY.to_vec()),
            OdmScope::Groups(fields) => Some(fields.clone()),
            OdmScope::Templates(contexts) => Some(
                crate::odm_source::HIERARCHY
                    .into_iter()
                    .filter(|field| {
                        contexts.iter().all(|context| {
                            context
                                .groups
                                .as_ref()
                                .is_none_or(|groups| groups.contains(field))
                        })
                    })
                    .collect(),
            ),
        };
        if let (true, Some(fields)) = (over_driver, fields) {
            scopes.push(fields.into_iter().map(|field| field as u32).collect());
        }
    }
    scopes
}

/// The partition class of materializing a named relation: the strongest
/// class of its donor plan's derivations, its filter and its key
/// expressions.
fn materialized_class(plan: &ResolvedPlan, relation: RelationId) -> PartitionClass {
    let key_fields = key_fields(plan);
    let mut classes = Classes {
        plan,
        donors: vec![None; plan.regions.len()],
        key_fields: &key_fields,
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
        .map(|expr| classes.expr(expr, Scope::default()).0.rows)
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

/// What an expression or a step reaches: its class under partitions of
/// consecutive records (#3697), its class under key-bounded partitions
/// (#3763), and how many leading declared keys every window and aggregate it
/// reads groups by, `u32::MAX` when it reads none.
#[derive(Clone, Copy)]
struct Class {
    rows: PartitionClass,
    keyed: PartitionClass,
    keys: u32,
}

impl Class {
    const SAFE: Self = Self::of(PartitionClass::Safe);

    /// The same class under both kinds of partitions.
    const fn of(class: PartitionClass) -> Self {
        Self {
            rows: class,
            keyed: class,
            keys: u32::MAX,
        }
    }

    /// A window or an aggregate whose grouping holds the leading `keys`
    /// declared keys: global over consecutive records, and inside one
    /// partition when the partitions keep those keys' groups whole.
    const fn grouped(keys: u32) -> Self {
        Self {
            rows: PartitionClass::Global,
            keyed: PartitionClass::Safe,
            keys,
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            rows: self.rows.max(other.rows),
            keyed: self.keyed.max(other.keyed),
            keys: self.keys.min(other.keys),
        }
    }
}

/// Whose rows a step's windows and aggregates read (#3763). A row of a
/// template that does not read the driver input belongs to no partition,
/// so a window or an aggregate that reads such a row holds no partition key.
#[derive(Clone, Copy, Default)]
struct Scope<'p> {
    /// The driver context of a row-stage binding, whose current group a
    /// grouped template's aggregate reduces.
    driver: Option<&'p DriverContext>,
    /// Every row the step evaluates belongs to a template that reads the
    /// driver input: the rows a qualified aggregate matches records for.
    rows: bool,
    /// Every row a window or an unqualified aggregate of the step ranges
    /// over belongs to such a template: the template's own rows in the row
    /// stage, and every candidate of the dataset in the column phase.
    candidates: bool,
}

/// Partition classes and effects of expressions. Each donor region's effects
/// are computed once, however many reads materialize it.
struct Classes<'a> {
    plan: &'a ResolvedPlan,
    donors: Vec<Option<Effects>>,
    /// The driver field each declared key copies, if any ([`key_fields`]).
    key_fields: &'a [Option<u32>],
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
            let (_, effect) = self.expr(plan.binding(binding).expr, Scope::default());
            effects = effects.max(effect);
        }
        self.donors[region.index()] = Some(effects);
        effects
    }

    /// Reading a relation: its class, and the effects of selecting from it
    /// (key expressions and the nearest reference, filter, and a donor plan
    /// it materializes). `scope` says whose rows the reading step reads.
    fn relation(
        &mut self,
        relation: RelationId,
        scope: Scope<'_>,
        pending: &mut Vec<ExprId>,
    ) -> (Class, Effects) {
        let node = self.plan.relation(relation);
        let class = match node.kind {
            RelationKind::OdmItems => Class::of(PartitionClass::CombineStep),
            RelationKind::CurrentGroup
            | RelationKind::MatchedAggregate
            | RelationKind::OutputAggregate => Class::grouped(self.aggregate_keys(relation, scope)),
            RelationKind::Named { .. } | RelationKind::Join
                if self.plan.reads_other_input(relation) =>
            {
                Class::SAFE
            }
            RelationKind::Named { .. } | RelationKind::Join => Class::of(PartitionClass::Global),
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

    /// The number of leading declared keys, each copied from a driver field,
    /// that `holds` accepts: it is given the key's column and that field.
    fn leading(&self, holds: impl Fn(ColumnId, u32) -> bool) -> u32 {
        let held = self
            .plan
            .keys()
            .iter()
            .zip(self.key_fields)
            .take_while(|(&key, field)| field.is_some_and(|field| holds(key, field)))
            .count();
        u32::try_from(held).unwrap_or(u32::MAX)
    }

    /// The leading keys a window groups by: its `group_by` reads the key's
    /// completed column, or the driver field the key copies (WINDOW-3,
    /// WINDOW-11). A window of an intermediate's derivations groups donor
    /// records, which no partition divides, and holds none, as does a window
    /// whose rows include one of a template that does not read the driver
    /// input.
    fn window_keys(&self, window: WindowId, scope: Scope<'_>) -> u32 {
        let node = self.plan.window(window);
        if node.region != RegionId::DATASET || !scope.candidates {
            return 0;
        }
        self.leading(|key, field| {
            node.group_by.contains(&Read::Column(key))
                || node.group_by.contains(&Read::Field(field))
        })
    }

    /// The leading keys an aggregate groups by (LOOKUP-18, LOOKUP-19): the
    /// group fields of the grouped row template it reduces, when that
    /// template reads the driver input; the `group_by` columns of an
    /// unqualified aggregate; or the keys of a qualified one, each reading
    /// the key's column or the driver field it copies, and over the driver
    /// input pairing it with that field, so every matched record shares the
    /// current row's partition keys. An aggregate of an intermediate's
    /// derivations holds none, as its windows do, and so does one that
    /// ranges over or is evaluated for a row of a template that does not read
    /// the driver input.
    fn aggregate_keys(&self, relation: RelationId, scope: Scope<'_>) -> u32 {
        let plan = self.plan;
        let node = plan.relation(relation);
        if node.region != RegionId::DATASET {
            return 0;
        }
        let reads = |key: ColumnId, field: u32| {
            move |pair: &&RelationKey| {
                pair.value == KeyValue::Read(Read::Column(key))
                    || pair.value == KeyValue::Read(Read::Field(field))
            }
        };
        match node.kind {
            RelationKind::CurrentGroup => match scope.driver {
                Some(DriverContext {
                    source,
                    named: None,
                    grain: Grain::Groups(fields),
                    ..
                }) if plan.region_sources(RegionId::DATASET).next() == Some(*source) => {
                    self.leading(|_, field| fields.contains(&field))
                }
                _ => 0,
            },
            RelationKind::OutputAggregate if scope.candidates => {
                self.leading(|key, field| node.keys.iter().any(|pair| reads(key, field)(&pair)))
            }
            RelationKind::MatchedAggregate if scope.rows => {
                let primary = plan.region_sources(node.region).next();
                let over_driver =
                    matches!(node.donor, Donor::Source(source) if Some(source) == primary);
                self.leading(|key, field| {
                    node.keys
                        .iter()
                        .filter(reads(key, field))
                        .any(|pair| !over_driver || pair.field == field)
                })
            }
            _ => 0,
        }
    }

    /// The strongest partition classes and effect of an expression and
    /// everything it reaches: operands, relations and windows. `scope`
    /// says whose rows the step the expression belongs to reads.
    fn expr(&mut self, root: ExprId, scope: Scope<'_>) -> (Class, Effects) {
        let plan = self.plan;
        let mut class = Class::SAFE;
        let mut effects = Effects::Pure;
        let mut pending = vec![root];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(expr) = pending.pop() {
            if !seen.insert(expr) {
                continue;
            }
            class = class.max(match plan.expr(expr).kind {
                ExprKind::Collect | ExprKind::Odm(_) => Class::of(PartitionClass::CombineStep),
                ExprKind::Window(window) | ExprKind::ContextWindow(window) => {
                    Class::grouped(self.window_keys(window, scope))
                }
                ExprKind::Aggregate(relation) => {
                    Class::grouped(self.aggregate_keys(relation, scope))
                }
                ExprKind::Intermediate(relation)
                | ExprKind::Joined(relation)
                | ExprKind::JoinedSource(relation)
                | ExprKind::SourceLookup(relation)
                    if plan.reads_other_input(relation) =>
                {
                    Class::SAFE
                }
                ExprKind::ProjectFunction
                | ExprKind::Intermediate(_)
                | ExprKind::Joined(_)
                | ExprKind::JoinedSource(_)
                | ExprKind::SourceLookup(_) => Class::of(PartitionClass::Global),
                _ => Class::SAFE,
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
                        let (relation_class, effect) = self.relation(relation, scope, &mut pending);
                        class = class.max(relation_class);
                        effects = effects.max(effect);
                    }
                    Read::Window(window) => {
                        class = class.max(Class::grouped(self.window_keys(window, scope)));
                        pending.extend(plan.window(window).filter);
                    }
                    Read::Column(_) | Read::Field(_) => (),
                }
            }
        }
        (class, effects)
    }
}
