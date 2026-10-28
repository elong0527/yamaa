//! Lowering: a bound plan and its donor plans become regions of one arena.
//!
//! The dataset is region 0. A derived intermediate's donor plan is lowered
//! once, as its own region, however many relations share it; regions are
//! lowered one after another so that each owns contiguous table ranges.
use super::*;
use crate::bound_expression::{BoundPredicate, Read as BoundRead};
use crate::dataset::{
    read_requires_window, window_stages, AggregateRelation, AggregateSource, Assignment,
    BoundAggregate, BoundOdm, Check, ColumnBinding, ColumnScope, CurrentField, CurrentRead,
    Expression, FirstAvailable, FlagRead, FunctionArgument, FunctionInput, ImputationKind,
    IntermediateKey, IntermediateSource, JoinAbsence, JoinedRead, MatchRead, MatchValue, OdmScope,
    RelationPredicate, RelationRange, RowMode, RowTemplate, SelectionRead, SourceSelection, Window,
    WindowKind,
};
use std::collections::{BTreeMap, VecDeque};

pub(super) fn resolve(plan: &DatasetPlan) -> Result<ResolvedPlan, ResolutionError> {
    let mut builder = Builder {
        out: ResolvedPlan {
            regions: Vec::new(),
            sources: Vec::new(),
            columns: Vec::new(),
            templates: Vec::new(),
            bindings: Vec::new(),
            cells: Vec::new(),
            exprs: Vec::new(),
            operands: Vec::new(),
            reads: Vec::new(),
            node_sites: Vec::new(),
            relations: Vec::new(),
            windows: Vec::new(),
            checks: Vec::new(),
            handlers: Vec::new(),
            functions: Vec::new(),
            schedule: Vec::new(),
            keys: Vec::new(),
            output_order: Vec::new(),
        },
        sites: Sites::default(),
        plans: Vec::new(),
        donors: Vec::new(),
        pending: VecDeque::new(),
        calls: Vec::new(),
        observed: Vec::new(),
    };
    let dataset = builder.reserve_region(plan, RegionKind::Dataset);
    builder.lower_region(dataset)?;
    while let Some(region) = builder.pending.pop_front() {
        builder.lower_region(region)?;
    }
    let Builder {
        mut out,
        sites,
        calls,
        ..
    } = builder;
    super::schedule::build(&mut out)?;
    super::registration::build(&mut out, sites, calls)?;
    super::schedule::annotate(&mut out);
    Ok(out)
}

/// A node before its operand, read and site lists join the arena.
struct Node {
    kind: ExprKind,
    path: Arc<str>,
    op: Op,
    operands: Vec<Operand>,
    reads: Vec<Read>,
    sites: Vec<NodeSite>,
}

/// A node's nested value assignments, each with its operand role and its
/// demand, lowered after the node's own facts.
type Values<'p> = Vec<(&'p Assignment, OperandRole, Demand)>;

/// Handler sites interned by (path, kind), numbered provisionally until the
/// registration walk fixes the report order.
#[derive(Default)]
pub(super) struct Sites {
    index: BTreeMap<(Arc<str>, u8), u32>,
    pub(super) entries: Vec<(Arc<str>, HandlerKind)>,
}
impl Sites {
    fn intern(&mut self, path: &str, kind: HandlerKind) -> HandlerSiteId {
        let order = match kind {
            HandlerKind::Absent => 0,
            HandlerKind::Missing => 1,
            HandlerKind::Unmapped => 2,
            HandlerKind::Invalid => 3,
            HandlerKind::Unconvertible => 4,
            HandlerKind::MultipleMatches => 5,
            HandlerKind::NoMatch => 6,
            HandlerKind::Partial => 7,
            HandlerKind::CrossedBounds => 8,
        };
        let path: Arc<str> = path.into();
        let next = self.entries.len() as u32;
        let id = *self.index.entry((path.clone(), order)).or_insert_with(|| {
            self.entries.push((path, kind));
            next
        });
        HandlerSiteId(id)
    }
}

struct Builder<'p> {
    out: ResolvedPlan,
    sites: Sites,
    /// The bound plan of every region.
    plans: Vec<&'p DatasetPlan>,
    /// Donor plans already given a region, by identity.
    donors: Vec<(*const DatasetPlan, RegionId)>,
    pending: VecDeque<RegionId>,
    calls: Vec<super::registration::Call>,
    /// The handlers a named read counts, per intermediate of the region
    /// being lowered.
    observed: Vec<crate::dataset::ReadHandlers>,
}

/// How a leaf `Source(c)` or `Column(c)` read is addressed in one context.
#[derive(Clone, Copy)]
enum Fields<'a> {
    /// The driver record of the evaluating row.
    Driver,
    /// The record a relation considers. Columns at or past `output` are the
    /// derived fields that follow an aggregate donor's `donor` stored fields.
    Relation {
        relation: RelationId,
        output: usize,
        donor: usize,
    },
    /// A resolved key expression: `Source(i)` is the i-th match operand.
    Match(&'a [MatchRead]),
}

/// One region being lowered: its plan and the first slot of each table.
#[derive(Clone, Copy)]
struct Cx<'p> {
    plan: &'p DatasetPlan,
    region: RegionId,
    sources: u32,
    columns: u32,
    templates: u32,
    named: u32,
    width: usize,
    dataset: bool,
}
impl Cx<'_> {
    fn column(&self, column: usize) -> Result<ColumnId, ResolutionError> {
        if column >= self.width {
            return Err(unsupported("column outside the output", column));
        }
        Ok(ColumnId(self.columns + column as u32))
    }
    fn source(&self, source: usize) -> Result<SourceId, ResolutionError> {
        if source > self.plan.secondary().len() {
            return Err(unsupported("source outside the plan", source));
        }
        Ok(SourceId(self.sources + source as u32))
    }
    fn template(&self, template: usize) -> Result<TemplateId, ResolutionError> {
        Ok(TemplateId(self.templates + field(template)?))
    }
    fn named(&self, index: usize) -> Result<RelationId, ResolutionError> {
        if index >= self.plan.intermediates().len() {
            return Err(unsupported("intermediate outside the plan", index));
        }
        Ok(RelationId(self.named + index as u32))
    }
    fn secondary_width(&self, index: usize) -> Result<usize, ResolutionError> {
        Ok(self
            .plan
            .secondary()
            .get(index)
            .ok_or_else(|| unsupported("source outside the plan", index + 1))?
            .schema
            .columns()
            .len())
    }
}

fn unsupported(what: &str, index: usize) -> ResolutionError {
    ResolutionError::Unsupported(format!("{what}: {index}"))
}
fn field(index: usize) -> Result<u32, ResolutionError> {
    u32::try_from(index).map_err(|_| ResolutionError::Capacity)
}
fn span(start: usize, end: usize) -> Result<Range<u32>, ResolutionError> {
    Ok(field(start)?..field(end)?)
}
fn relation(region: RegionId, kind: RelationKind, path: &str, donor: Donor) -> RelationNode {
    RelationNode {
        region,
        kind,
        path: path.into(),
        donor,
        keys: Vec::new(),
        nearest: None,
        current: Vec::new(),
        filter: None,
        range: None,
        selection: None,
        no_match: None,
        derivations: None,
        pair_values: None,
        windowed: false,
        scan: Vec::new(),
        named: None,
    }
}

/// A join's options: the range, filter and selection that narrow and choose
/// its record.
#[derive(Clone, Copy)]
struct JoinOptions<'o> {
    range: Option<&'o RelationRange>,
    filter: Option<&'o BoundPredicate>,
    selection: Option<&'o SourceSelection>,
}

/// One check as lowering declares it: its path, what it checks and at which
/// severity, its position among the checks of its scope, and what it
/// evaluates.
struct CheckDeclaration<'d> {
    path: &'d str,
    scope: CheckScope,
    family: CheckFamily,
    severity: VerificationSeverity,
    position: usize,
    payload: CheckPayload,
}

impl<'p> Builder<'p> {
    fn reserve_region(&mut self, plan: &'p DatasetPlan, kind: RegionKind) -> RegionId {
        let id = RegionId(self.out.regions.len() as u32);
        self.out.regions.push(Region {
            kind,
            sources: 0..0,
            columns: 0..0,
            templates: 0..0,
            bindings: 0..0,
            relations: 0..0,
            schedule: 0..0,
            cells: 0,
            conversions: Vec::new(),
            column_order: Vec::new(),
            output: plan.output().clone(),
            required_matches: plan.required_matches().clone(),
            secondary: plan.secondary().to_vec(),
        });
        self.plans.push(plan);
        id
    }

    /// One region per distinct donor plan, lowered after the region that found it.
    fn donor_region(
        &mut self,
        plan: &'p DatasetPlan,
        stored: usize,
        typed: &[usize],
    ) -> Result<RegionId, ResolutionError> {
        let key = plan as *const DatasetPlan;
        if let Some((_, region)) = self.donors.iter().find(|(known, _)| *known == key) {
            return Ok(*region);
        }
        // Typed columns hold region-local positions until the region's columns exist.
        let region = self.reserve_region(
            plan,
            RegionKind::Donor {
                stored_columns: field(stored)?,
                typed_columns: typed
                    .iter()
                    .map(|&column| field(column).map(ColumnId))
                    .collect::<Result<_, _>>()?,
            },
        );
        self.donors.push((key, region));
        self.pending.push_back(region);
        Ok(region)
    }

    fn lower_region(&mut self, region: RegionId) -> Result<(), ResolutionError> {
        let plan = self.plans[region.index()];
        let cx = Cx {
            plan,
            region,
            sources: field(self.out.sources.len())?,
            columns: field(self.out.columns.len())?,
            templates: field(self.out.templates.len())?,
            named: field(self.out.relations.len())?,
            width: plan.output().columns().len(),
            dataset: matches!(self.out.regions[region.index()].kind, RegionKind::Dataset),
        };
        self.out.sources.push(SourceSlot {
            name: None,
            schema: plan.source().clone(),
        });
        for source in plan.secondary() {
            self.out.sources.push(SourceSlot {
                name: Some(source.name.as_str().into()),
                schema: source.schema.clone(),
            });
        }
        for (index, column) in plan.output().columns().iter().enumerate() {
            self.out.columns.push(ColumnNode {
                name: column.name.as_str().into(),
                kind: plan.output_kind(index),
                phase: ColumnPhase::RowOnly,
                column_bindings: 0..0,
            });
        }
        if let RegionKind::Donor { typed_columns, .. } = &mut self.out.regions[region.index()].kind
        {
            for column in typed_columns.iter_mut() {
                *column = cx.column(column.index())?;
            }
        }
        self.observed = crate::dataset::read_handlers(plan.intermediates());
        // Named intermediates keep their plan positions as relation slots.
        for index in 0..plan.intermediates().len() {
            let path = plan.intermediates()[index].path.as_str();
            let index = field(index)?;
            let placeholder = relation(region, RelationKind::Named { index }, path, Donor::Driver);
            self.out.relations.push(placeholder);
        }
        for index in 0..plan.intermediates().len() {
            self.lower_named(&cx, index)?;
        }
        for template in plan.templates() {
            let driver = template_driver(&cx, &template.mode)?;
            self.out.templates.push(TemplateNode {
                region,
                driver,
                filter: None,
                row: None,
                row_context: None,
                row_bindings: 0..0,
            });
        }
        let first_binding = self.out.bindings.len();
        for (index, template) in plan.templates().iter().enumerate() {
            self.lower_template(&cx, index, template)?;
        }
        // A column's bindings are adjacent; its one step runs them in turn.
        for (position, binding) in plan.column_bindings().iter().enumerate() {
            let column = self.lower_column(&cx, position, binding)?;
            let order = &mut self.out.regions[region.index()].column_order;
            if order.last() != Some(&column) {
                order.push(column);
            }
        }
        self.fill_cells(&cx, first_binding)?;
        let conversions = plan
            .conversion_handlers()
            .iter()
            .map(|declaration| {
                let path = &declaration.handler.spec_path;
                self.sites.intern(path, HandlerKind::Unconvertible)
            })
            .collect();
        if cx.dataset {
            self.lower_checks(&cx)?;
            for &key in plan.keys() {
                let column = cx.column(key)?;
                self.out.keys.push(column);
            }
            for term in plan.output_order() {
                let key = OrderKey {
                    column: cx.column(term.column)?,
                    descending: term.descending,
                    nulls_first: term.nulls_first,
                };
                self.out.output_order.push(key);
            }
        }
        let entry = &mut self.out.regions[region.index()];
        entry.sources = span(cx.sources as usize, self.out.sources.len())?;
        entry.columns = span(cx.columns as usize, self.out.columns.len())?;
        entry.templates = span(cx.templates as usize, self.out.templates.len())?;
        entry.bindings = span(first_binding, self.out.bindings.len())?;
        entry.relations = span(cx.named as usize, self.out.relations.len())?;
        entry.conversions = conversions;
        Ok(())
    }

    fn lower_template(
        &mut self,
        cx: &Cx<'p>,
        index: usize,
        template: &'p RowTemplate,
    ) -> Result<(), ResolutionError> {
        let id = cx.template(index)?;
        let keyed = matches!(template.mode, RowMode::Keys);
        // The executor splits the stages on every run; resolution does it once.
        let deferred = window_stages(&template.assignments, cx.width, cx.plan.intermediates());
        let driver = self.out.templates[id.index()].driver.clone();
        let start = self.out.bindings.len();
        for (position, assignment) in template.assignments.iter().enumerate() {
            let expr = self.lower(cx, assignment, Fields::Driver)?;
            let stage = if !keyed && deferred.get(assignment.column).copied().unwrap_or(false) {
                Stage::Window
            } else {
                Stage::Prewindow
            };
            let binding = Binding {
                column: cx.column(assignment.column)?,
                templates: vec![id],
                stage,
                driver: driver.clone(),
                expr,
                path: assignment.path.as_str().into(),
                unconvertible: self.unconvertible(cx, assignment)?,
                handler: self.handler(cx, assignment)?,
                origin: BindingOrigin::Template {
                    template: field(index)?,
                    assignment: field(position)?,
                },
            };
            self.out.bindings.push(binding);
        }
        let node = &mut self.out.templates[id.index()];
        node.row_bindings = span(start, self.out.bindings.len())?;
        node.row = cx
            .plan
            .row_context(index)
            .map(|context| context.row.as_str().into());
        node.row_context = cx.plan.row_context(index).cloned();
        if let Some(filter) = &template.filter {
            let expr = self.predicate(cx, filter, Fields::Driver)?;
            let placement = if keyed {
                FilterPlacement::BeforeKeys
            } else if matches!(template.mode.local_mode(), RowMode::Groups(_))
                && filter.bindings().iter().any(|binding| {
                    read_requires_window(&binding.read, &deferred, cx.plan.intermediates())
                })
            {
                FilterPlacement::Postwindow
            } else {
                FilterPlacement::Prewindow
            };
            self.out.templates[id.index()].filter = Some(TemplateFilter { expr, placement });
        }
        Ok(())
    }

    /// The conversion handler of a completed result. Untyped donor results
    /// skip conversion, so their declared handlers never fire.
    fn unconvertible(
        &mut self,
        cx: &Cx<'_>,
        assignment: &Assignment,
    ) -> Result<Option<HandlerSiteId>, ResolutionError> {
        if let RegionKind::Donor { typed_columns, .. } = &self.out.regions[cx.region.index()].kind {
            if !typed_columns.contains(&cx.column(assignment.column)?) {
                return Ok(None);
            }
        }
        Ok(cx.plan.conversion_handler(&assignment.path).map(|handler| {
            self.sites
                .intern(&handler.spec_path, HandlerKind::Unconvertible)
        }))
    }

    /// The declared replacement of a completed result, under the same rule.
    fn handler(
        &self,
        cx: &Cx<'_>,
        assignment: &Assignment,
    ) -> Result<Option<LiteralHandler>, ResolutionError> {
        if let RegionKind::Donor { typed_columns, .. } = &self.out.regions[cx.region.index()].kind {
            if !typed_columns.contains(&cx.column(assignment.column)?) {
                return Ok(None);
            }
        }
        Ok(cx.plan.conversion_handler(&assignment.path).cloned())
    }

    /// One column binding of the bound plan, lowered once for the templates
    /// it serves; templates with equal driver contexts share one resolved
    /// binding (#2829). A binding in the template scope reads each row's own
    /// driver; one in the whole-dataset scope reads through the primary port.
    fn lower_column(
        &mut self,
        cx: &Cx<'p>,
        position: usize,
        bound: &'p ColumnBinding,
    ) -> Result<ColumnId, ResolutionError> {
        let assignment = &bound.assignment;
        let column = cx.column(assignment.column)?;
        let expr = self.lower(cx, assignment, Fields::Driver)?;
        let unconvertible = self.unconvertible(cx, assignment)?;
        let handler = self.handler(cx, assignment)?;
        let odm = deferred_odm(&assignment.expression);
        let primary = DriverContext {
            source: cx.source(0)?,
            named: None,
            grain: Grain::Records,
            odm: None,
        };
        let mut groups: Vec<(DriverContext, Vec<TemplateId>)> = Vec::new();
        for &index in &bound.templates {
            let id = cx.template(index)?;
            let mut driver = match bound.scope {
                ColumnScope::Template => self
                    .out
                    .templates
                    .get(id.index())
                    .ok_or_else(|| unsupported("template outside the plan", index))?
                    .driver
                    .clone(),
                ColumnScope::Dataset => primary.clone(),
            };
            driver.odm = odm.and_then(|scopes| scopes.get(index).cloned());
            match groups.iter_mut().find(|(known, _)| *known == driver) {
                Some((_, members)) => members.push(id),
                None => groups.push((driver, vec![id])),
            }
        }
        if groups.is_empty() {
            // Every template completed the column first. The column step still
            // visits the declaration, and registration still walks it.
            groups.push((primary, Vec::new()));
        }
        let start = self.out.bindings.len();
        for (driver, templates) in groups {
            self.out.bindings.push(Binding {
                column,
                templates,
                stage: Stage::Column,
                driver,
                expr,
                path: assignment.path.as_str().into(),
                unconvertible,
                handler: handler.clone(),
                origin: BindingOrigin::Column {
                    assignment: field(position)?,
                },
            });
        }
        // A column's bindings are adjacent, so their slots are contiguous.
        let end = field(self.out.bindings.len())?;
        let node = &mut self.out.columns[column.index()];
        if node.column_bindings.is_empty() {
            node.column_bindings = span(start, self.out.bindings.len())?;
        } else {
            node.column_bindings.end = end;
        }
        Ok(column)
    }

    /// Point every (template, column) cell at its binding and classify columns.
    fn fill_cells(&mut self, cx: &Cx<'_>, first_binding: usize) -> Result<(), ResolutionError> {
        let count = cx.plan.templates().len();
        let base = self.out.cells.len();
        self.out.regions[cx.region.index()].cells = field(base)?;
        self.out.cells.resize(base + count * cx.width, None);
        for id in first_binding..self.out.bindings.len() {
            let binding = &self.out.bindings[id];
            let offset = binding.column.index() - cx.columns as usize;
            for template in &binding.templates {
                let row = template.index() - cx.templates as usize;
                self.out.cells[base + row * cx.width + offset] = Some(BindingId(field(id)?));
            }
        }
        for offset in 0..cx.width {
            let row_bound = (0..count)
                .filter(|row| {
                    self.out.cells[base + row * cx.width + offset].is_some_and(|binding| {
                        self.out.bindings[binding.index()].stage != Stage::Column
                    })
                })
                .count();
            self.out.columns[cx.columns as usize + offset].phase = match row_bound {
                0 => ColumnPhase::ColumnOnly,
                bound if bound == count => ColumnPhase::RowOnly,
                _ => ColumnPhase::Mixed,
            };
        }
        Ok(())
    }

    fn push(&mut self, mut node: Node) -> Result<ExprId, ResolutionError> {
        let id = ExprId::try_from(self.out.exprs.len())?;
        // An operation registers its own sites in the fixed handler order.
        let entries = &self.sites.entries;
        node.sites
            .sort_by_key(|site| registration_rank(entries[site.site.index()].1));
        let out = &mut self.out;
        let operands = span(out.operands.len(), out.operands.len() + node.operands.len())?;
        let reads = span(out.reads.len(), out.reads.len() + node.reads.len())?;
        let sites = span(
            out.node_sites.len(),
            out.node_sites.len() + node.sites.len(),
        )?;
        out.operands.extend(node.operands);
        out.reads.extend(node.reads);
        out.node_sites.extend(node.sites);
        out.exprs.push(ExprNode {
            kind: node.kind,
            path: node.path,
            op: node.op,
            operands,
            reads,
            sites,
        });
        Ok(id)
    }

    /// A handler site the node fires when `declared`. A node lists its sites
    /// in registration order.
    fn site(&mut self, node: &mut Node, declared: bool, path: String, kind: HandlerKind) {
        if declared {
            let site = self.sites.intern(&path, kind);
            node.sites.push(NodeSite { site });
        }
    }

    /// An implicit join's `absent` count, unless the read fails instead of
    /// answering.
    fn absent(&mut self, cx: &Cx<'_>, node: &mut Node, absence: &JoinAbsence, input: &str) {
        let answers = !absence.requires_match(cx.plan.requires_match(input));
        let site = format!("{}.absent", absence.path);
        if answers && !self.has_site(node, &site) {
            self.site(node, true, site, HandlerKind::Absent);
        }
    }

    /// A formula operand read through an implicit join counts at its owner's path.
    fn joined_operands<'r>(
        &mut self,
        cx: &Cx<'_>,
        node: &mut Node,
        path: &str,
        reads: impl IntoIterator<Item = &'r BoundRead>,
    ) {
        for read in reads {
            if let BoundRead::Joined(read) = read {
                self.absent(cx, node, &JoinAbsence::operand(path), &read.name);
            }
        }
    }

    /// Whether the node already fires the site at this path.
    fn has_site(&self, node: &Node, path: &str) -> bool {
        node.sites
            .iter()
            .any(|entry| &*self.sites.entries[entry.site.index()].0 == path)
    }

    /// Lower one assignment and its nested assignments, children first.
    ///
    /// Each family of bound expression kinds lowers in a method of its own,
    /// which gives the node its kind, operation, reads and handler sites;
    /// the nested value assignments it names in `values` are lowered after
    /// the node's own facts, and the node joins the arena after them.
    fn lower(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        fields: Fields<'_>,
    ) -> Result<ExprId, ResolutionError> {
        use Expression as E;
        let path = assignment.path.as_str();
        let mut node = Node {
            kind: ExprKind::Literal,
            path: path.into(),
            op: Op::Read,
            operands: Vec::new(),
            reads: Vec::new(),
            sites: Vec::new(),
        };
        // Nested value assignments, lowered after the node's own facts.
        let mut values: Values<'p> = Vec::new();
        let mut call = None;
        match &assignment.expression {
            E::Literal(value) => node.op = Op::Literal(value.clone()),
            E::Source(_) | E::Column(_) | E::Collect { .. } => {
                self.lower_read(cx, assignment, fields, &mut node)?;
            }
            E::Compute(_) | E::Flag(_) | E::Case(_) => {
                self.lower_formula(cx, assignment, fields, &mut node)?;
            }
            E::MappingCut { .. }
            | E::Round { .. }
            | E::Imputation { .. }
            | E::Temporal { .. }
            | E::Extreme { .. } => {
                self.lower_value_function(cx, assignment, &mut node, &mut values);
            }
            E::StrTemplate { .. }
            | E::StrExtract { .. }
            | E::StrReplace { .. }
            | E::StrCase { .. }
            | E::StrPad { .. }
            | E::StrSubstr { .. }
            | E::StrLength { .. }
            | E::StrDecimal { .. } => {
                self.lower_string_function(assignment, &mut node, &mut values);
            }
            E::SourceBinding { .. } | E::FirstAvailable(_) => {
                self.lower_selection(cx, assignment, fields, &mut node, &mut values)?;
            }
            E::SourceLookup { .. }
            | E::Joined(_)
            | E::JoinedSource { .. }
            | E::Intermediate { .. } => {
                self.lower_relation_read(cx, assignment, &mut node)?;
            }
            E::Aggregate(bound) => self.aggregate(cx, &mut node, bound)?,
            E::Odm(bound) => self.odm(cx, &mut node, bound)?,
            E::ProjectFunction(function) => {
                node.kind = ExprKind::ProjectFunction;
                node.op = Op::ProjectFunction(function.clone());
                self.arguments(cx, &mut node, function.arguments(), fields, path)?;
                self.joined_operands(cx, &mut node, path, argument_reads(function.arguments()));
                call = Some((function.slot(), function.signature().clone()));
            }
            E::Window(payload) => {
                let window = self.window(cx, path, payload, &[])?;
                node.kind = ExprKind::Window(window);
                node.op = Op::Window {
                    window: payload.clone(),
                    column: assignment.column,
                };
                node.reads.push(Read::Window(window));
            }
            E::ContextWindow(context) => {
                let window = self.window(cx, path, &context.window, &context.sources)?;
                node.kind = ExprKind::ContextWindow(window);
                node.op = Op::ContextWindow {
                    context: context.clone(),
                    column: assignment.column,
                };
                node.reads.push(Read::Window(window));
            }
        }
        for (operand, role, demand) in values {
            let expr = self.lower(cx, operand, fields)?;
            node.operands.push(Operand { expr, role, demand });
        }
        let expr = self.push(node)?;
        if let Some((slot, signature)) = call {
            let call = super::registration::Call {
                expr,
                slot,
                signature,
            };
            self.calls.push(call);
        }
        Ok(expr)
    }

    /// A read of the row: a source field, an output column, or a source
    /// field's distinct readings across a key combination (`Collect`).
    fn lower_read(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        fields: Fields<'_>,
        node: &mut Node,
    ) -> Result<(), ResolutionError> {
        use Demand::Always;
        use Expression as E;
        use HandlerKind as H;
        let path = assignment.path.as_str();
        match &assignment.expression {
            E::Source(column) => {
                node.kind = ExprKind::Source;
                node.reads.push(self.source_read(cx, fields, *column)?);
                if let Fields::Match(_) = fields {
                    node.op = Op::MatchOperand { index: *column };
                }
            }
            E::Column(column) => {
                node.kind = ExprKind::Column;
                node.reads.push(self.column_read(cx, fields, *column)?);
            }
            E::Collect {
                column,
                identifier,
                filter,
                selection,
            } => {
                node.kind = ExprKind::Collect;
                node.op = Op::Collect {
                    identifier: identifier.clone(),
                    selection: selection.clone(),
                };
                node.reads.push(self.source_read(cx, fields, *column)?);
                if let Some(filter) = filter {
                    let expr = self.predicate(cx, filter, fields)?;
                    node.operands.push(Operand {
                        expr,
                        role: OperandRole::Filter,
                        demand: Always,
                    });
                }
                let site = format!("{path}.multiple_matches");
                let selected = selection.is_some();
                self.site(node, selected, site, H::MultipleMatches);
            }
            _ => unreachable!("lower routes only reads of the row here"),
        }
        Ok(())
    }

    /// A formula: compiled arithmetic, a flag, or a `case`, whose branches
    /// are lowered in authored order, each after its condition.
    fn lower_formula(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        fields: Fields<'_>,
        node: &mut Node,
    ) -> Result<(), ResolutionError> {
        use Demand::{Always, Reached};
        use Expression as E;
        use HandlerKind as H;
        let path = assignment.path.as_str();
        match &assignment.expression {
            E::Compute(numeric) => {
                node.kind = ExprKind::Compute;
                node.op = Op::Compute(numeric.clone());
                for binding in numeric.bindings() {
                    let read = self.bound_read(cx, fields, &binding.read, path)?;
                    node.reads.push(read);
                    self.observed(node, &binding.read, path)?;
                }
                self.joined_operands(cx, node, path, numeric.bindings().iter().map(|b| &b.read));
            }
            E::Flag(flag) => {
                node.kind = ExprKind::Flag;
                node.op = Op::Flag(flag.clone());
                // A missing operand is counted whether or not `missing` is declared.
                self.site(node, true, flag.missing_path(), H::Missing);
                for binding in flag.bindings() {
                    let read = match &binding.read {
                        FlagRead::Read(read) => self.bound_read(cx, fields, read, path)?,
                        FlagRead::Collect { column, .. } => {
                            self.source_read(cx, fields, *column)?
                        }
                    };
                    node.reads.push(read);
                }
            }
            E::Case(branches) => {
                node.kind = ExprKind::Case;
                node.op = Op::Case;
                for (index, branch) in branches.iter().enumerate() {
                    if let Some(when) = &branch.when {
                        let expr = self.predicate(cx, when, fields)?;
                        let demand = if index == 0 { Always } else { Reached };
                        node.operands.push(Operand {
                            expr,
                            role: OperandRole::When,
                            demand,
                        });
                    }
                    let expr = self.lower(cx, &branch.assignment, fields)?;
                    node.operands.push(Operand {
                        expr,
                        role: OperandRole::Branch,
                        demand: Reached,
                    });
                }
            }
            _ => unreachable!("lower routes only formulas here"),
        }
        Ok(())
    }

    /// A function of nested value assignments: a mapping cut, rounding, an
    /// imputation, a temporal function or an extreme. It names its handler
    /// sites and leaves its operands to `values`.
    fn lower_value_function(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        node: &mut Node,
        values: &mut Values<'p>,
    ) {
        use Demand::{Always, Reached};
        use Expression as E;
        use HandlerKind as H;
        use OperandRole::Value;
        let path = assignment.path.as_str();
        match &assignment.expression {
            E::MappingCut { source, operation } => {
                node.kind = ExprKind::MappingCut;
                node.op = Op::MappingCut {
                    operation: operation.clone(),
                    column: assignment.column,
                    handled: cx.plan.conversion_handler(path).is_some(),
                };
                let base = operation.path();
                let unmapped = operation.unmapped().is_some();
                // A missing source is counted whether or not `missing` is declared.
                self.site(node, true, format!("{base}.missing"), H::Missing);
                let site = format!("{base}.unmapped");
                self.site(node, unmapped, site, H::Unmapped);
                values.push((source, Value, Always));
            }
            E::Round { source, rounding } => {
                node.kind = ExprKind::Round;
                node.op = Op::Round(rounding.clone());
                values.push((source, Value, Always));
            }
            E::Imputation {
                operands,
                operation,
            } => {
                node.kind = ExprKind::Imputation;
                node.op = Op::Imputation(operation.clone());
                let base = operation.path();
                // A missing source is counted at `fallback` when one is declared.
                let missing = if operation.fallback().is_some() {
                    "fallback"
                } else {
                    "missing"
                };
                let counts = operation.counts_missing_source();
                let site = format!("{base}.{missing}");
                self.site(node, counts, site, H::Missing);
                let declared = operation.handler("invalid").is_some();
                let site = format!("{base}.invalid");
                self.site(node, declared, site, H::Invalid);
                let crossed = operation.counts_crossed_bounds();
                let site = format!("{base}.not_after");
                self.site(node, crossed, site, H::CrossedBounds);
                let partial = matches!(operation.kind(), ImputationKind::ToDate);
                let site = format!("{base}.partial");
                self.site(node, partial, site, H::Partial);
                // Bounds are evaluated only after a partial source needs them.
                for (index, operand) in operands.iter().enumerate() {
                    values.push((operand, Value, if index == 0 { Always } else { Reached }));
                }
            }
            E::Temporal {
                operands,
                operation,
            } => {
                node.kind = ExprKind::Temporal;
                node.op = Op::Temporal(operation.clone());
                let declared = operation.invalid_handler().is_some();
                let site = format!("{}.invalid", operation.path());
                self.site(node, declared, site, H::Invalid);
                // `dtc` reads a part only when every coarser part is present.
                let stops = operation.kind().stops_at_missing();
                for (index, operand) in operands.iter().enumerate() {
                    let demand = if stops && index > 0 { Reached } else { Always };
                    values.push((operand, Value, demand));
                }
            }
            E::Extreme { operands, largest } => {
                node.kind = ExprKind::Extreme;
                node.op = Op::Extreme { largest: *largest };
                values.extend(operands.iter().map(|operand| (operand, Value, Always)));
            }
            _ => unreachable!("lower routes only value functions here"),
        }
    }

    /// A string function of nested value assignments. It names its handler
    /// sites and leaves its operands to `values`.
    fn lower_string_function(
        &mut self,
        assignment: &'p Assignment,
        node: &mut Node,
        values: &mut Values<'p>,
    ) {
        use Demand::Always;
        use Expression as E;
        use HandlerKind as H;
        use OperandRole::Value;
        match &assignment.expression {
            E::StrTemplate {
                operands,
                operation,
            } => {
                node.kind = ExprKind::StrTemplate;
                node.op = Op::StrTemplate(operation.clone());
                let declared = operation.missing().is_some();
                let site = format!("{}.missing", operation.path());
                self.site(node, declared, site, H::Missing);
                values.extend(operands.iter().map(|operand| (operand, Value, Always)));
            }
            E::StrExtract { source, operation } => {
                node.kind = ExprKind::StrExtract;
                node.op = Op::StrExtract(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                let declared = operation.handler("no_match").is_some();
                let site = format!("{}.no_match", operation.path());
                self.site(node, declared, site, H::NoMatch);
                values.push((source, Value, Always));
            }
            E::StrReplace { source, operation } => {
                node.kind = ExprKind::StrReplace;
                node.op = Op::StrReplace(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                values.push((source, Value, Always));
            }
            E::StrCase { source, operation } => {
                node.kind = ExprKind::StrCase;
                node.op = Op::StrCase(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                values.push((source, Value, Always));
            }
            E::StrPad { source, operation } => {
                node.kind = ExprKind::StrPad;
                node.op = Op::StrPad(operation.clone());
                values.push((source, Value, Always));
            }
            E::StrSubstr { source, operation } => {
                node.kind = ExprKind::StrSubstr;
                node.op = Op::StrSubstr(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                values.push((source, Value, Always));
            }
            E::StrLength { source, operation } => {
                node.kind = ExprKind::StrLength;
                node.op = Op::StrLength(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                values.push((source, Value, Always));
            }
            E::StrDecimal { source, operation } => {
                node.kind = ExprKind::StrDecimal;
                node.op = Op::StrDecimal(operation.clone());
                let site = format!("{}.missing", operation.path());
                self.site(node, true, site, H::Missing);
                values.push((source, Value, Always));
            }
            _ => unreachable!("lower routes only string functions here"),
        }
    }

    /// A selection: a source binding, which leaves its operand to `values`,
    /// or the first available of authored operands.
    fn lower_selection(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        fields: Fields<'_>,
        node: &mut Node,
        values: &mut Values<'p>,
    ) -> Result<(), ResolutionError> {
        use Demand::Always;
        use Expression as E;
        use HandlerKind as H;
        use OperandRole::Value;
        match &assignment.expression {
            E::SourceBinding {
                source,
                absent,
                selection_declared,
                variable,
                path: binding,
            } => {
                node.kind = ExprKind::SourceBinding;
                node.op = Op::SourceBinding {
                    absent: absent.clone(),
                    variable: variable.clone(),
                    path: binding.clone(),
                };
                let site = format!("{binding}.absent");
                self.site(node, absent.is_some(), site, H::Absent);
                let site = format!("{}.multiple_matches", source.path);
                let kind = H::MultipleMatches;
                self.site(node, *selection_declared, site, kind);
                values.push((source, Value, Always));
            }
            E::FirstAvailable(selection) => {
                node.kind = ExprKind::FirstAvailable;
                node.op = Op::FirstAvailable {
                    missing: selection.missing().clone(),
                    missing_path: selection.missing_path(),
                    sources: selection
                        .sources()
                        .iter()
                        .map(|source| source.path.clone())
                        .collect(),
                };
                let site = selection.missing_path();
                self.site(node, true, site, H::Missing);
                self.first_available(cx, node, selection, fields)?;
            }
            _ => unreachable!("lower routes only selections here"),
        }
        Ok(())
    }

    /// A read through a relation: an implicit join on output keys, a join on
    /// a declared port, or a named intermediate.
    fn lower_relation_read(
        &mut self,
        cx: &Cx<'p>,
        assignment: &'p Assignment,
        node: &mut Node,
    ) -> Result<(), ResolutionError> {
        use Expression as E;
        use HandlerKind as H;
        let path = assignment.path.as_str();
        match &assignment.expression {
            E::SourceLookup {
                lookup,
                range,
                filter,
                selection,
                absence,
            } => {
                let keys = lookup
                    .keys
                    .iter()
                    .map(|key| {
                        let current = Read::Column(cx.column(key.output_column)?);
                        Ok((key.source_column, current))
                    })
                    .collect::<Result<Vec<_>, ResolutionError>>()?;
                let donor = cx.source(lookup.source + 1)?;
                let relation = self.join(
                    cx,
                    path,
                    donor,
                    keys,
                    JoinOptions {
                        range: range.as_ref(),
                        filter: filter.as_ref(),
                        selection: selection.as_ref(),
                    },
                )?;
                node.kind = ExprKind::SourceLookup(relation);
                node.op = Op::SourceLookup {
                    lookup: lookup.clone(),
                    range: range.clone(),
                    filter: filter.clone(),
                    selection: selection.clone(),
                    absence: absence.clone(),
                };
                node.reads
                    .push(Read::Relation(relation, field(lookup.column)?));
                let site = format!("{path}.multiple_matches");
                let selected = selection.is_some();
                self.site(node, selected, site, H::MultipleMatches);
                if let Some(absence) = absence {
                    let input = &cx.plan.secondary()[lookup.source].name;
                    self.absent(cx, node, absence, input);
                }
            }
            E::Joined(read) => {
                let relation = self.joined(cx, path, read, None, None, None)?;
                node.kind = ExprKind::Joined(relation);
                node.op = Op::Joined(read.as_ref().clone());
                node.reads
                    .push(Read::Relation(relation, field(read.column)?));
                self.absent(cx, node, &JoinAbsence::operand(path), &read.name);
            }
            E::JoinedSource {
                read,
                range,
                filter,
                selection,
                absence,
            } => {
                let relation = self.joined(
                    cx,
                    path,
                    read,
                    range.as_ref(),
                    filter.as_ref(),
                    selection.as_ref(),
                )?;
                node.kind = ExprKind::JoinedSource(relation);
                node.op = Op::JoinedSource {
                    read: read.as_ref().clone(),
                    range: range.clone(),
                    filter: filter.clone(),
                    selection: selection.clone(),
                    absence: absence.clone(),
                };
                node.reads
                    .push(Read::Relation(relation, field(read.column)?));
                let site = format!("{path}.multiple_matches");
                let selected = selection.is_some();
                self.site(node, selected, site, H::MultipleMatches);
                if let Some(absence) = absence {
                    self.absent(cx, node, absence, &read.name);
                }
            }
            E::Intermediate { index, column } => {
                let relation = cx.named(*index)?;
                node.kind = ExprKind::Intermediate(relation);
                node.op = Op::Intermediate { column: *column };
                node.reads.push(Read::Relation(relation, field(*column)?));
                let read = BoundRead::Intermediate {
                    index: *index,
                    column: *column,
                };
                self.observed(node, &read, path)?;
            }
            _ => unreachable!("lower routes only reads through a relation here"),
        }
        Ok(())
    }

    /// Operands in authored order; each later one is reached only when every
    /// earlier one was missing.
    fn first_available(
        &mut self,
        cx: &Cx<'p>,
        node: &mut Node,
        selection: &'p FirstAvailable,
        fields: Fields<'_>,
    ) -> Result<(), ResolutionError> {
        for (index, operand) in selection.sources().iter().enumerate() {
            let leaf = |kind, read| Node {
                kind,
                path: operand.path.as_str().into(),
                op: Op::Read,
                operands: Vec::new(),
                reads: vec![read],
                sites: Vec::new(),
            };
            let expr = match &operand.read {
                SelectionRead::Expression(assignment) => self.lower(cx, assignment, fields)?,
                SelectionRead::Column(column) => {
                    let read = self.column_read(cx, fields, *column)?;
                    self.push(leaf(ExprKind::Column, read))?
                }
                SelectionRead::Source(column) => {
                    let read = self.source_read(cx, fields, *column)?;
                    self.push(leaf(ExprKind::Source, read))?
                }
                SelectionRead::Collect {
                    column,
                    identifier,
                    filter,
                } => {
                    let mut collect =
                        leaf(ExprKind::Collect, self.source_read(cx, fields, *column)?);
                    collect.op = Op::Collect {
                        identifier: identifier.clone(),
                        selection: None,
                    };
                    if let Some(filter) = filter {
                        let expr = self.predicate(cx, filter, fields)?;
                        collect.operands.push(Operand {
                            expr,
                            role: OperandRole::Filter,
                            demand: Demand::Always,
                        });
                    }
                    self.push(collect)?
                }
            };
            node.operands.push(Operand {
                expr,
                role: OperandRole::Selection,
                demand: if index == 0 {
                    Demand::Always
                } else {
                    Demand::Reached
                },
            });
        }
        Ok(())
    }

    fn arguments(
        &mut self,
        cx: &Cx<'p>,
        node: &mut Node,
        arguments: &'p [FunctionArgument],
        fields: Fields<'_>,
        path: &str,
    ) -> Result<(), ResolutionError> {
        for argument in arguments {
            match &argument.input {
                FunctionInput::Literal(_) => (),
                FunctionInput::Read(read) => {
                    let slot = self.bound_read(cx, fields, read, path)?;
                    node.reads.push(slot);
                    self.observed(node, read, path)?;
                }
                FunctionInput::Collect { column, .. } => {
                    node.reads.push(self.source_read(cx, fields, *column)?);
                }
            }
        }
        Ok(())
    }

    /// A named read counts its own `multiple_matches` and absence at the
    /// reading path, its absence under `absent`, as an implicit join read
    /// counts its absence.
    fn observed(
        &mut self,
        node: &mut Node,
        read: &BoundRead,
        path: &str,
    ) -> Result<(), ResolutionError> {
        let BoundRead::Intermediate { index, .. } = read else {
            return Ok(());
        };
        let handlers = *self
            .observed
            .get(*index)
            .ok_or_else(|| unsupported("intermediate outside the plan", *index))?;
        let site = format!("{path}.multiple_matches");
        let multiple = handlers.multiple_matches;
        self.site(node, multiple, site, HandlerKind::MultipleMatches);
        if handlers.absent {
            let kind = HandlerKind::Absent;
            let site = format!("{path}.{}", kind.name());
            self.site(node, true, site, kind);
        }
        Ok(())
    }

    fn source_read(
        &self,
        cx: &Cx<'_>,
        fields: Fields<'_>,
        column: usize,
    ) -> Result<Read, ResolutionError> {
        Ok(match fields {
            Fields::Driver => Read::Field(field(column)?),
            Fields::Relation { relation, .. } => Read::Relation(relation, field(column)?),
            Fields::Match(reads) => match_read(
                cx,
                reads
                    .get(column)
                    .ok_or_else(|| unsupported("match operand outside the key", column))?,
            )?,
        })
    }

    fn column_read(
        &self,
        cx: &Cx<'_>,
        fields: Fields<'_>,
        column: usize,
    ) -> Result<Read, ResolutionError> {
        Ok(match fields {
            Fields::Relation {
                relation,
                output,
                donor,
            } if column >= output => Read::Relation(relation, field(donor + column - output)?),
            _ => Read::Column(cx.column(column)?),
        })
    }

    fn bound_read(
        &mut self,
        cx: &Cx<'p>,
        fields: Fields<'_>,
        read: &BoundRead,
        path: &str,
    ) -> Result<Read, ResolutionError> {
        Ok(match read {
            BoundRead::Source(column) => self.source_read(cx, fields, *column)?,
            BoundRead::Column(column) => self.column_read(cx, fields, *column)?,
            BoundRead::Intermediate { index, column } => {
                Read::Relation(cx.named(*index)?, field(*column)?)
            }
            BoundRead::Joined(joined) => {
                let relation = self.joined(cx, path, joined, None, None, None)?;
                Read::Relation(relation, field(joined.column)?)
            }
        })
    }

    /// A predicate's operands in binding order. Named reads in predicates
    /// record no handler.
    fn predicate(
        &mut self,
        cx: &Cx<'p>,
        predicate: &BoundPredicate,
        fields: Fields<'_>,
    ) -> Result<ExprId, ResolutionError> {
        let path = predicate.plan().spec_path();
        let mut reads = Vec::new();
        for binding in predicate.bindings() {
            reads.push(self.bound_read(cx, fields, &binding.read, path)?);
        }
        self.push(Node {
            kind: ExprKind::Predicate,
            path: path.into(),
            op: Op::Predicate(predicate.clone()),
            operands: Vec::new(),
            reads,
            sites: Vec::new(),
        })
    }

    /// A donor-only predicate: its source reads address the relation's record.
    fn donor_predicate(
        &mut self,
        cx: &Cx<'p>,
        relation: RelationId,
        predicate: &BoundPredicate,
    ) -> Result<ExprId, ResolutionError> {
        let fields = Fields::Relation {
            relation,
            output: usize::MAX,
            donor: 0,
        };
        self.predicate(cx, predicate, fields)
    }

    fn push_relation(&mut self, node: RelationNode) -> Result<RelationId, ResolutionError> {
        let id = RelationId::try_from(self.out.relations.len())?;
        self.out.relations.push(node);
        Ok(id)
    }

    /// An implicit join: equality keys and an optional range over one donor
    /// source, with an optional donor filter and stable selection (LOOKUP-25).
    fn join(
        &mut self,
        cx: &Cx<'p>,
        path: &str,
        donor: SourceId,
        keys: Vec<(usize, Read)>,
        options: JoinOptions<'_>,
    ) -> Result<RelationId, ResolutionError> {
        let JoinOptions {
            range,
            filter,
            selection,
        } = options;
        let mut node = relation(cx.region, RelationKind::Join, path, Donor::Source(donor));
        for (donor_field, current) in keys {
            node.keys.push(RelationKey {
                field: field(donor_field)?,
                value: KeyValue::Read(current),
            });
        }
        node.range = range.map(|range| range_node(cx, range)).transpose()?;
        node.selection = selection_shape(selection)?;
        let id = self.push_relation(node)?;
        if let Some(filter) = filter {
            let expr = self.donor_predicate(cx, id, filter)?;
            self.out.relations[id.index()].filter = Some(expr);
        }
        Ok(id)
    }

    /// A qualified implicit join: row templates match raw driver fields,
    /// column joins match completed output keys.
    fn joined(
        &mut self,
        cx: &Cx<'p>,
        path: &str,
        read: &JoinedRead,
        range: Option<&RelationRange>,
        filter: Option<&BoundPredicate>,
        selection: Option<&SourceSelection>,
    ) -> Result<RelationId, ResolutionError> {
        let donor = match read.source {
            AggregateSource::Primary => cx.source(0)?,
            AggregateSource::Secondary(index) => cx.source(index + 1)?,
        };
        let mut keys = Vec::new();
        for (position, key) in read.keys.iter().enumerate() {
            let current = match &read.record_keys {
                Some(fields) => {
                    Read::Field(field(*fields.get(position).ok_or_else(|| {
                        unsupported("record key outside the join", position)
                    })?)?)
                }
                None => Read::Column(cx.column(key.output_column)?),
            };
            keys.push((key.source_column, current));
        }
        let options = JoinOptions {
            range,
            filter,
            selection,
        };
        self.join(cx, path, donor, keys, options)
    }

    /// Match values in written order; expression keys become nodes.
    fn key_values(
        &mut self,
        cx: &Cx<'p>,
        keys: &'p [IntermediateKey],
    ) -> Result<(Vec<RelationKey>, bool), ResolutionError> {
        let mut values = Vec::new();
        let mut windowed = false;
        for key in keys {
            let value = match &key.value {
                MatchValue::Variable(read) | MatchValue::Source(read) => {
                    KeyValue::Read(match_read(cx, read)?)
                }
                MatchValue::StrPad { read, .. } => KeyValue::Padded(match_read(cx, read)?),
                MatchValue::Literal(value) => KeyValue::Literal(value.clone()),
                MatchValue::Expression(expression) => {
                    windowed |= expression.windowed;
                    let fields = if expression.windowed {
                        Fields::Driver
                    } else {
                        Fields::Match(&expression.reads)
                    };
                    KeyValue::Expr(self.lower(cx, &expression.assignment, fields)?)
                }
                MatchValue::Contextual(_) => KeyValue::Unbound,
            };
            values.push(RelationKey {
                field: field(key.source_column)?,
                value,
            });
        }
        Ok((values, windowed))
    }

    fn lower_named(&mut self, cx: &Cx<'p>, index: usize) -> Result<(), ResolutionError> {
        let item = &cx.plan.intermediates()[index];
        let id = cx.named(index)?;
        let donor = match &item.source {
            IntermediateSource::Input(source) => Donor::Source(cx.source(source + 1)?),
            IntermediateSource::Primary { .. } => Donor::Source(cx.source(0)?),
            IntermediateSource::SelfRows { .. } => Donor::SelfRows,
            IntermediateSource::Pairs(_) => Donor::Pairs,
        };
        let mut node = relation(
            cx.region,
            RelationKind::Named {
                index: field(index)?,
            },
            &item.path,
            donor,
        );
        (node.keys, node.windowed) = self.key_values(cx, &item.keys)?;
        // The reference a nearest selection measures to is read as a key's
        // match value is, after the keys (LOOKUP-26).
        if let Some(nearest) = &item.nearest {
            let (mut reference, windowed) =
                self.key_values(cx, std::slice::from_ref(&nearest.reference))?;
            node.windowed |= windowed;
            node.nearest = reference.pop();
        }
        // A contextual filter is only read in a rebased donor region.
        let filter = item.filter.as_ref().filter(|filter| {
            filter
                .current()
                .iter()
                .all(|field| field.read != CurrentRead::Contextual)
        });
        for current in filter.into_iter().flat_map(|filter| filter.current()) {
            node.current.push(current_read(cx, current)?);
        }
        if let Some(derived) = &item.derivations {
            for current in &derived.current {
                node.current.push(current_read(cx, current)?);
            }
            let region = self.donor_region(
                &derived.plan,
                derived.stored_columns,
                &derived.typed_columns,
            )?;
            node.derivations = Some(region);
        }
        // A pair relation's key values are evaluated over its left records
        // before the pairing (LOOKUP-27).
        if let Some(values) = item.source.pairs().and_then(|join| join.values.as_ref()) {
            let region =
                self.donor_region(&values.plan, values.stored_columns, &values.typed_columns)?;
            node.pair_values = Some(region);
        }
        // A contextual range value is no value of these rows: binding
        // refuses every row's read of it, and only a donor plan, which rebases
        // it onto the donor record, matches the range (LOOKUP-16).
        if let Some(range) = item
            .range
            .as_ref()
            .filter(|range| range.current().read != CurrentRead::Contextual)
        {
            node.range = Some(range_node(cx, range)?);
        }
        node.selection = selection_shape(item.selection.as_ref())?;
        node.no_match = item.no_match.clone();
        node.named = Some(Box::new(item.clone()));
        node.filter = filter
            .map(|filter| self.relation_predicate(cx, id, filter))
            .transpose()?;
        self.out.relations[id.index()] = node;
        Ok(())
    }

    /// A correlated filter: source reads address the donor record, column
    /// reads the current fields in their declared positions.
    fn relation_predicate(
        &mut self,
        cx: &Cx<'p>,
        relation: RelationId,
        filter: &RelationPredicate,
    ) -> Result<ExprId, ResolutionError> {
        let path = filter.plan().spec_path();
        let mut reads = Vec::new();
        for binding in filter.bindings() {
            reads.push(match &binding.read {
                BoundRead::Source(column) => Read::Relation(relation, field(*column)?),
                BoundRead::Column(position) => current_read(
                    cx,
                    filter.current().get(*position).ok_or_else(|| {
                        unsupported("current field outside the filter", *position)
                    })?,
                )?,
                BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                    return Err(ResolutionError::Unsupported(format!(
                        "named read inside the donor filter at {path}"
                    )))
                }
            });
        }
        self.push(Node {
            kind: ExprKind::Predicate,
            path: path.into(),
            op: Op::Predicate(filter.predicate().clone()),
            operands: Vec::new(),
            reads,
            sites: Vec::new(),
        })
    }

    /// An aggregate and its relation: the current group, a matched donor or
    /// the completed output. Key expressions and record derivations become
    /// operands; derivations read the donor record they enrich.
    fn aggregate(
        &mut self,
        cx: &Cx<'p>,
        node: &mut Node,
        bound: &'p BoundAggregate,
    ) -> Result<(), ResolutionError> {
        let (kind, donor, donor_width) = match bound.relation() {
            AggregateRelation::CurrentGroup => (
                RelationKind::CurrentGroup,
                Donor::Group,
                cx.plan.source().columns().len(),
            ),
            AggregateRelation::Matched {
                source: AggregateSource::Primary,
                ..
            } => (
                RelationKind::MatchedAggregate,
                Donor::Source(cx.source(0)?),
                cx.plan.source().columns().len(),
            ),
            AggregateRelation::Matched {
                source: AggregateSource::Secondary(index),
                ..
            } => (
                RelationKind::MatchedAggregate,
                Donor::Source(cx.source(index + 1)?),
                cx.secondary_width(*index)?,
            ),
            AggregateRelation::Output { .. } => {
                (RelationKind::OutputAggregate, Donor::Output, cx.width)
            }
        };
        let mut relation_node = relation(cx.region, kind, bound.operation_path(), donor);
        match bound.relation() {
            AggregateRelation::Matched { keys, .. } => {
                (relation_node.keys, relation_node.windowed) = self.key_values(cx, keys)?;
                for key in &relation_node.keys {
                    if let KeyValue::Expr(expr) = key.value {
                        node.operands.push(Operand {
                            expr,
                            role: OperandRole::Key,
                            demand: Demand::Always,
                        });
                    }
                }
            }
            AggregateRelation::Output { group_by } => {
                for &column in group_by {
                    let id = cx.column(column)?;
                    relation_node.keys.push(RelationKey {
                        field: field(column)?,
                        value: KeyValue::Read(Read::Column(id)),
                    });
                    relation_node.scan.push(id);
                }
                let order = bound.order().iter().map(|term| term.column);
                for column in bound.bindings().iter().map(|b| b.column).chain(order) {
                    let id = cx.column(column)?;
                    if !relation_node.scan.contains(&id) {
                        relation_node.scan.push(id);
                    }
                }
            }
            AggregateRelation::CurrentGroup => (),
        }
        if let Some(range) = bound.range() {
            relation_node.range = Some(range_node(cx, range)?);
        }
        let relation_id = self.push_relation(relation_node)?;
        node.kind = ExprKind::Aggregate(relation_id);
        node.op = Op::Aggregate(Box::new(bound.clone()));
        if let Some(filter) = bound.filter() {
            let expr = self.donor_predicate(cx, relation_id, filter)?;
            self.out.relations[relation_id.index()].filter = Some(expr);
        }
        let fields = Fields::Relation {
            relation: relation_id,
            output: cx.width,
            donor: donor_width,
        };
        for derivation in bound.derivations() {
            let inner = self.lower(cx, &derivation.assignment, fields)?;
            let mut wrapper = Node {
                kind: ExprKind::Derivation,
                path: derivation.path.as_str().into(),
                op: Op::Derivation {
                    column: derivation.assignment.column,
                    kind: derivation.kind,
                    handler: derivation.handler.clone(),
                },
                operands: vec![Operand {
                    expr: inner,
                    role: OperandRole::Value,
                    demand: Demand::Always,
                }],
                reads: Vec::new(),
                sites: Vec::new(),
            };
            if let Some(handler) = &derivation.handler {
                let site = handler.spec_path.clone();
                self.site(&mut wrapper, true, site, HandlerKind::Unconvertible);
            }
            let expr = self.push(wrapper)?;
            node.operands.push(Operand {
                expr,
                role: OperandRole::Derivation,
                demand: Demand::Always,
            });
        }
        for binding in bound.bindings() {
            node.reads
                .push(Read::Relation(relation_id, field(binding.column)?));
        }
        // The fields `STR_JOIN` orders the records by.
        for term in bound.order() {
            let read = Read::Relation(relation_id, field(term.column)?);
            if !node.reads.contains(&read) {
                node.reads.push(read);
            }
        }
        Ok(())
    }

    /// An ODM item read: its scope fields of the current row, or the keys.
    fn odm(
        &mut self,
        cx: &Cx<'p>,
        node: &mut Node,
        bound: &'p BoundOdm,
    ) -> Result<(), ResolutionError> {
        let read = bound.read();
        let donor = match read.source {
            None => Donor::Driver,
            Some(AggregateSource::Primary) => Donor::Source(cx.source(0)?),
            Some(AggregateSource::Secondary(index)) => Donor::Source(cx.source(index + 1)?),
        };
        let mut relation_node = relation(cx.region, RelationKind::OdmItems, &read.path, donor);
        let fields: &[usize] = match &read.scope {
            OdmScope::Keys => {
                for &column in cx.plan.keys() {
                    relation_node.current.push(Read::Column(cx.column(column)?));
                }
                &[]
            }
            OdmScope::Records => &crate::odm_source::HIERARCHY,
            OdmScope::Groups(fields) => fields,
            // Each template's scope lives in its binding's driver context.
            OdmScope::Templates(_) => &[],
        };
        for &column in fields {
            relation_node.current.push(Read::Field(field(column)?));
        }
        let relation_id = self.push_relation(relation_node)?;
        if let Some(filter) = &read.filter {
            let expr = self.donor_predicate(cx, relation_id, filter)?;
            self.out.relations[relation_id.index()].filter = Some(expr);
        }
        node.kind = ExprKind::Odm(relation_id);
        node.op = Op::Odm(Box::new(bound.clone()));
        node.reads.push(Read::Relation(relation_id, ODM_VALUE));
        Ok(())
    }

    /// A window over completed columns; a context window appends collected
    /// driver fields after the output columns.
    fn window(
        &mut self,
        cx: &Cx<'p>,
        path: &str,
        window: &'p Window,
        sources: &[(String, usize)],
    ) -> Result<WindowId, ResolutionError> {
        let slot = |column: usize| -> Result<Read, ResolutionError> {
            if column < cx.width {
                return Ok(Read::Column(cx.column(column)?));
            }
            let (_, source) = sources
                .get(column - cx.width)
                .ok_or_else(|| unsupported("window slot", column))?;
            Ok(Read::Field(field(*source)?))
        };
        let (function, operands) = match window.kind {
            WindowKind::RowNumber => (WindowFunction::RowNumber, Vec::new()),
            WindowKind::Competition => (WindowFunction::Competition, Vec::new()),
            WindowKind::Dense => (WindowFunction::Dense, Vec::new()),
            WindowKind::RowValue { column, offset } => {
                (WindowFunction::RowValue { offset }, vec![slot(column)?])
            }
            WindowKind::Locf {
                column,
                include_current,
            } => (
                WindowFunction::Locf { include_current },
                vec![slot(column)?],
            ),
            WindowKind::BaselineFlag {
                date,
                reference_date,
                value,
                inclusive,
            } => (
                WindowFunction::BaselineFlag { inclusive },
                vec![slot(date)?, slot(reference_date)?, slot(value)?],
            ),
        };
        let mut group_by = Vec::new();
        for &column in &window.group_by {
            group_by.push(slot(column)?);
        }
        let mut order_by = Vec::new();
        for term in &window.order_by {
            order_by.push(ReadOrder {
                read: slot(term.column)?,
                descending: term.descending,
                nulls_first: term.nulls_first,
            });
        }
        let filter = match &window.filter {
            Some(filter) => {
                let path = filter.plan().spec_path();
                let mut reads = Vec::new();
                for binding in filter.bindings() {
                    reads.push(match &binding.read {
                        BoundRead::Column(column) => slot(*column)?,
                        BoundRead::Source(column) => Read::Field(field(*column)?),
                        BoundRead::Intermediate { .. } | BoundRead::Joined(_) => {
                            return Err(ResolutionError::Unsupported(format!(
                                "named read inside the window filter at {path}"
                            )))
                        }
                    });
                }
                Some(self.push(Node {
                    kind: ExprKind::Predicate,
                    path: path.into(),
                    op: Op::Predicate(filter.clone()),
                    operands: Vec::new(),
                    reads,
                    sites: Vec::new(),
                })?)
            }
            None => None,
        };
        let id = WindowId::try_from(self.out.windows.len())?;
        self.out.windows.push(WindowNode {
            region: cx.region,
            function,
            path: path.into(),
            group_by,
            order_by,
            operands,
            filter,
        });
        Ok(id)
    }

    fn check(
        &mut self,
        declared: CheckDeclaration<'_>,
        reads: &[Read],
        predicates: Vec<ExprId>,
    ) -> Result<(), ResolutionError> {
        let CheckDeclaration {
            path,
            scope,
            family,
            severity,
            position,
            payload,
        } = declared;
        let start = self.out.reads.len();
        self.out.reads.extend_from_slice(reads);
        let reads = span(start, self.out.reads.len())?;
        self.out.checks.push(CheckNode {
            path: path.into(),
            scope,
            family,
            severity,
            // The schedule fixes the step once it exists.
            ready_at: StepId(u32::MAX),
            reads,
            predicates,
            position: field(position)?,
            payload,
        });
        Ok(())
    }

    /// Intermediate uniqueness, column check groups in declared column order,
    /// output keys, then dataset checks in declaration order (CHECK-1).
    fn lower_checks(&mut self, cx: &Cx<'p>) -> Result<(), ResolutionError> {
        use VerificationSeverity::Error;
        let plan = cx.plan;
        for (index, item) in plan.intermediates().iter().enumerate() {
            let scope = CheckScope::Intermediate(cx.named(index)?);
            for (position, check) in item.unique.iter().enumerate() {
                let family = CheckFamily::IntermediateUnique;
                let payload = CheckPayload::Unique(check.clone());
                self.check(
                    CheckDeclaration {
                        path: &check.path,
                        scope,
                        family,
                        severity: check.severity,
                        position,
                        payload,
                    },
                    &[],
                    Vec::new(),
                )?;
            }
        }
        for group in plan.column_verifications() {
            let column = cx.column(group.column)?;
            for (position, verification) in group.checks.iter().enumerate() {
                let mut reads = vec![Read::Column(column)];
                // A check waits for every completed column it selects its
                // rows by: a value-level check's where columns, a
                // `not_missing`'s `when` columns (CHECK-5).
                for read in where_columns(&verification.check)
                    .into_iter()
                    .chain(verification.check.when_columns())
                {
                    let read = Read::Column(cx.column(read)?);
                    if !reads.contains(&read) {
                        reads.push(read);
                    }
                }
                let severity = plan.verification_severity(&verification.path);
                let family = check_family(&verification.check);
                let scope = CheckScope::Column(column);
                self.check(
                    CheckDeclaration {
                        path: &verification.path,
                        scope,
                        family,
                        severity,
                        position,
                        payload: CheckPayload::Declared(verification.check.clone()),
                    },
                    &reads,
                    Vec::new(),
                )?;
            }
        }
        let mut keys = Vec::new();
        for (position, &key) in plan.keys().iter().enumerate() {
            let read = Read::Column(cx.column(key)?);
            keys.push(read);
            let path = format!("keys[{position}]");
            let family = CheckFamily::KeyMissing;
            self.check(
                CheckDeclaration {
                    path: &path,
                    scope: CheckScope::OutputKeys,
                    family,
                    severity: Error,
                    position,
                    payload: CheckPayload::Keys,
                },
                &[read],
                Vec::new(),
            )?;
        }
        let family = CheckFamily::KeyDuplicates;
        self.check(
            CheckDeclaration {
                path: "keys",
                scope: CheckScope::OutputKeys,
                family,
                severity: Error,
                position: 0,
                payload: CheckPayload::Keys,
            },
            &keys,
            Vec::new(),
        )?;
        for (position, verification) in plan.verifications().iter().enumerate() {
            let mut reads = Vec::new();
            let mut predicates = Vec::new();
            match &verification.check {
                Check::Unique(columns) | Check::AllOrNone(columns) => {
                    for &column in columns {
                        reads.push(Read::Column(cx.column(column)?));
                    }
                }
                Check::Assert { when, require } => {
                    for predicate in when.iter().chain([require]) {
                        predicates.push(self.predicate(cx, predicate, Fields::Driver)?);
                    }
                }
                Check::PredicateDeclaration(predicate) => {
                    predicates.push(self.predicate(cx, predicate, Fields::Driver)?);
                }
                Check::ScopedRowCount(count) => {
                    for &column in count.group_by.iter().flatten() {
                        reads.push(Read::Column(cx.column(column)?));
                    }
                    for predicate in count.filter.iter().chain(&count.when) {
                        predicates.push(self.predicate(cx, predicate, Fields::Driver)?);
                    }
                }
                _ => (),
            }
            let severity = plan.verification_severity(&verification.path);
            let family = check_family(&verification.check);
            let scope = CheckScope::Dataset;
            self.check(
                CheckDeclaration {
                    path: &verification.path,
                    scope,
                    family,
                    severity,
                    position,
                    payload: CheckPayload::Declared(verification.check.clone()),
                },
                &reads,
                predicates,
            )?;
        }
        Ok(())
    }
}

/// The ODM item value field.
const ODM_VALUE: u32 = 10;

/// The order in which one operation registers its own handler sites
/// (COLUMN-14): `missing` or `fallback`, `unmapped`, `invalid`, `not_after`,
/// `partial`, then a lookup's `multiple_matches` before its `no_match` or
/// `absent`.
fn registration_rank(kind: HandlerKind) -> u8 {
    match kind {
        HandlerKind::Missing => 0,
        HandlerKind::Unmapped => 1,
        HandlerKind::Invalid => 2,
        HandlerKind::CrossedBounds => 3,
        HandlerKind::Partial => 4,
        HandlerKind::MultipleMatches => 5,
        HandlerKind::NoMatch => 6,
        HandlerKind::Absent => 7,
        HandlerKind::Unconvertible => 8,
    }
}

fn template_driver(cx: &Cx<'_>, mode: &RowMode) -> Result<DriverContext, ResolutionError> {
    let named = match mode {
        RowMode::IntermediateDriver { index, .. } => Some(cx.named(*index)?),
        _ => None,
    };
    let grain = match mode.local_mode() {
        RowMode::Records => Grain::Records,
        RowMode::Groups(keys) => Grain::Groups(
            keys.iter()
                .map(|&key| field(key))
                .collect::<Result<_, _>>()?,
        ),
        RowMode::Keys => Grain::Keys,
        RowMode::Driver { .. } | RowMode::IntermediateDriver { .. } => {
            return Err(ResolutionError::Unsupported("nested row driver".into()))
        }
    };
    Ok(DriverContext {
        source: cx.source(mode.source_index())?,
        named,
        grain,
        odm: None,
    })
}

/// Deferred column-phase ODM reads carry one scope per template; every read
/// of one declaration shares the scopes binding derived once for its row
/// templates. The walk follows the ODM visitor of the bound plan.
fn deferred_odm(expression: &Expression) -> Option<&[OdmTemplateScope]> {
    use Expression as E;
    match expression {
        E::Odm(bound) => match &bound.read().scope {
            OdmScope::Templates(scopes) => Some(scopes),
            _ => None,
        },
        E::SourceBinding { source, .. }
        | E::MappingCut { source, .. }
        | E::StrExtract { source, .. }
        | E::StrReplace { source, .. }
        | E::StrCase { source, .. }
        | E::StrPad { source, .. }
        | E::StrSubstr { source, .. }
        | E::StrLength { source, .. }
        | E::StrDecimal { source, .. }
        | E::Round { source, .. } => deferred_odm(&source.expression),
        E::Extreme { operands, .. }
        | E::StrTemplate { operands, .. }
        | E::Imputation { operands, .. }
        | E::Temporal { operands, .. } => operands
            .iter()
            .find_map(|operand| deferred_odm(&operand.expression)),
        E::Case(branches) => branches
            .iter()
            .find_map(|branch| deferred_odm(&branch.assignment.expression)),
        E::FirstAvailable(selection) => {
            selection
                .sources()
                .iter()
                .find_map(|operand| match &operand.read {
                    SelectionRead::Expression(assignment) => deferred_odm(&assignment.expression),
                    _ => None,
                })
        }
        _ => None,
    }
}

fn selection_shape(
    selection: Option<&SourceSelection>,
) -> Result<Option<SelectionShape>, ResolutionError> {
    let Some(selection) = selection else {
        return Ok(None);
    };
    let mut order_by = Vec::new();
    for term in &selection.order_by {
        order_by.push(FieldOrder {
            field: field(term.column)?,
            descending: term.descending,
            nulls_first: term.nulls_first,
        });
    }
    Ok(Some(SelectionShape {
        order_by,
        keep: selection.keep,
    }))
}

fn range_node(
    cx: &Cx<'_>,
    range: &crate::dataset::RelationRange,
) -> Result<RangeNode, ResolutionError> {
    Ok(RangeNode {
        current: current_read(cx, range.current())?,
        lower: range.lower().map(|bound| field(bound.column)).transpose()?,
        upper: range.upper().map(|bound| field(bound.column)).transpose()?,
        closed: range.closed(),
    })
}

fn match_read(cx: &Cx<'_>, read: &MatchRead) -> Result<Read, ResolutionError> {
    Ok(match read {
        MatchRead::Output { column, .. } => Read::Column(cx.column(*column)?),
        MatchRead::Record { column, .. } | MatchRead::SecondaryRecord { column, .. } => {
            Read::Field(field(*column)?)
        }
    })
}

fn current_read(cx: &Cx<'_>, current: &CurrentField) -> Result<Read, ResolutionError> {
    Ok(match current.read {
        CurrentRead::Record { column, .. } => Read::Field(field(column)?),
        CurrentRead::Output(column) => Read::Column(cx.column(column)?),
        CurrentRead::Intermediate { index, column } => {
            Read::Relation(cx.named(index)?, field(column)?)
        }
        // Only a donor plan reads it, rebased onto the donor record.
        CurrentRead::Contextual => return Err(unsupported("contextual current read", 0)),
    })
}

/// The completed where columns a value-level check selects its rows by, each
/// once, in clause order.
fn where_columns(check: &Check) -> Vec<usize> {
    let mut columns = Vec::new();
    for column in check.clauses().iter().flat_map(|clause| clause.columns()) {
        if !columns.contains(&column) {
            columns.push(column);
        }
    }
    columns
}

fn check_family(check: &Check) -> CheckFamily {
    match check {
        Check::NotMissing { .. } => CheckFamily::NotMissing,
        Check::Mandatory(_) => CheckFamily::Mandatory,
        Check::DataType { .. } => CheckFamily::DataType,
        Check::AllowedValues(_) => CheckFamily::AllowedValues,
        Check::Codelist { .. } => CheckFamily::Codelist,
        Check::ValueCodelists { .. } => CheckFamily::ValueCodelists,
        Check::Inventory { .. } => CheckFamily::Inventory,
        Check::ValueMaxLength { .. } => CheckFamily::ValueMaxLength,
        Check::ValueEntries(_) => CheckFamily::ValueEntries,
        Check::ValueEntryMatch { .. } => CheckFamily::ValueEntryMatch,
        Check::Range { .. } => CheckFamily::Range,
        Check::MaxLength(_) => CheckFamily::MaxLength,
        Check::WideMaxLength(_) => CheckFamily::WideMaxLength,
        Check::Matches(_) => CheckFamily::Matches,
        Check::InvalidDiagnostic(_) => CheckFamily::InvalidDiagnostic,
        Check::InvalidDeclaration { .. } => CheckFamily::InvalidDeclaration,
        Check::Assert { .. } => CheckFamily::Assert,
        Check::PredicateDeclaration(_) => CheckFamily::PredicateDeclaration,
        Check::Unique(_) => CheckFamily::Unique,
        Check::AllOrNone(_) => CheckFamily::AllOrNone,
        Check::RowCount { .. } => CheckFamily::RowCount,
        Check::WideRowCount { .. } => CheckFamily::WideRowCount,
        Check::ScopedRowCount(_) => CheckFamily::ScopedRowCount,
    }
}

/// The bound reads of a function's arguments, in argument order.
fn argument_reads(arguments: &[FunctionArgument]) -> impl Iterator<Item = &BoundRead> {
    arguments
        .iter()
        .filter_map(|argument| match &argument.input {
            FunctionInput::Read(read) => Some(read),
            _ => None,
        })
}
