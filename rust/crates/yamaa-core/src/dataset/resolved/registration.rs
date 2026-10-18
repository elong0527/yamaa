//! What a run registers before reading any row: handler sites and callback
//! signatures.
//!
//! The handler table reproduces the registration walk at the top of
//! `DatasetPlan::execute`: the plan's conversion handlers, the conversion
//! handlers of each intermediate's donor plan, then the sites of the six
//! walkers, in their sequence, for every template assignment, column
//! assignment and intermediate key expression. Sites no walker reaches
//! register when they first fire; sites inside donor derivations are counted
//! locally and never reported.
use super::build::Sites;
use super::*;

const WALKERS: [Walker; 6] = [
    Walker::Scalar,
    Walker::Imputation,
    Walker::StrCase,
    Walker::StrExtract,
    Walker::StrReplace,
    Walker::StrTemplate,
];

/// One callback site found while lowering, in lowering order.
pub(super) struct Call {
    pub(super) expr: ExprId,
    pub(super) slot: usize,
    pub(super) signature: FunctionSignature,
}

pub(super) fn build(
    plan: &mut ResolvedPlan,
    sites: Sites,
    calls: Vec<Call>,
) -> Result<(), ResolutionError> {
    let count = sites.entries.len();
    let eager = eager_order(plan, count);
    let mut placed = vec![false; count];
    let mut order: Vec<(usize, Registration)> = Vec::with_capacity(count);
    for site in eager {
        placed[site] = true;
        order.push((site, Registration::Eager));
    }
    for (site, done) in placed.iter_mut().enumerate() {
        if !*done && sites.reported[site] {
            *done = true;
            order.push((site, Registration::OnFirstFire));
        }
    }
    for (site, done) in placed.iter().enumerate() {
        if !*done {
            order.push((site, Registration::Unreported));
        }
    }
    let mut remap = vec![None; count];
    let mut table = Vec::with_capacity(count);
    for (site, registration) in order {
        remap[site] = Some(HandlerSiteId(table.len() as u32));
        let (path, kind) = &sites.entries[site];
        table.push(HandlerSite {
            path: path.clone(),
            kind: *kind,
            registration,
        });
    }
    let renumber = |site: HandlerSiteId| remap[site.index()].ok_or(ResolutionError::Capacity);
    for entry in &mut plan.node_sites {
        entry.site = renumber(entry.site)?;
    }
    for binding in &mut plan.bindings {
        binding.unconvertible = binding.unconvertible.map(renumber).transpose()?;
    }
    for region in &mut plan.regions {
        for site in &mut region.conversions {
            *site = renumber(*site)?;
        }
    }
    plan.handlers = table;
    plan.functions = functions(plan, calls);
    Ok(())
}

/// The registration walk over the arena, in today's order.
fn eager_order(plan: &ResolvedPlan, count: usize) -> Vec<usize> {
    let dataset = RegionId::DATASET;
    let mut walk = Walk {
        plan,
        seen: vec![false; count],
        order: Vec::new(),
    };
    for &site in plan.conversions(dataset) {
        walk.register(site);
    }
    let named: Vec<_> = plan
        .region_relations(dataset)
        .filter(|&relation| matches!(plan.relation(relation).kind, RelationKind::Named { .. }))
        .collect();
    for &relation in &named {
        if let Some(donor) = plan.relation(relation).derivations {
            for &site in plan.conversions(donor) {
                walk.register(site);
            }
        }
    }
    for template in plan.region_templates(dataset) {
        for binding in plan.row_bindings(template) {
            walk.six(plan.binding(binding).expr);
        }
    }
    for &column in &plan.region(dataset).column_order {
        if let Some(binding) = plan.column_bindings(column).next() {
            walk.six(plan.binding(binding).expr);
        }
    }
    for &relation in &named {
        for key in &plan.relation(relation).keys {
            if let KeyValue::Expr(expr) = key.value {
                walk.six(expr);
            }
        }
    }
    walk.order
}

struct Walk<'a> {
    plan: &'a ResolvedPlan,
    seen: Vec<bool>,
    order: Vec<usize>,
}
impl Walk<'_> {
    fn register(&mut self, site: HandlerSiteId) {
        if !std::mem::replace(&mut self.seen[site.index()], true) {
            self.order.push(site.index());
        }
    }
    /// Register the sites this walker owns on one node, in node order.
    fn tagged(&mut self, expr: ExprId, walker: Walker) {
        for index in 0..self.plan.sites(expr).len() {
            let site = self.plan.sites(expr)[index];
            if site.walker == Some(walker) {
                self.register(site.site);
            }
        }
    }
    fn six(&mut self, expr: ExprId) {
        for walker in WALKERS {
            self.walk(walker, expr);
        }
    }
    fn recurse(&mut self, walker: Walker, expr: ExprId, role: OperandRole) {
        for index in 0..self.plan.operands(expr).len() {
            let operand = self.plan.operands(expr)[index];
            if operand.role == role {
                self.walk(walker, operand.expr);
            }
        }
    }
    /// Each walker's recursion follows its legacy counterpart exactly,
    /// including the operation families it does not descend into.
    fn walk(&mut self, walker: Walker, expr: ExprId) {
        use ExprKind as K;
        use OperandRole::{Branch, Selection, Value};
        let kind = self.plan.expr(expr).kind;
        match walker {
            Walker::Scalar => match kind {
                K::Aggregate(_) => {
                    for index in 0..self.plan.operands(expr).len() {
                        let operand = self.plan.operands(expr)[index];
                        match operand.role {
                            OperandRole::Key => self.six(operand.expr),
                            OperandRole::Derivation => {
                                self.tagged(operand.expr, Walker::Scalar);
                                for inner in 0..self.plan.operands(operand.expr).len() {
                                    let inner = self.plan.operands(operand.expr)[inner];
                                    self.six(inner.expr);
                                }
                            }
                            _ => (),
                        }
                    }
                }
                K::MappingCut
                | K::StrContains
                | K::StrConcat
                | K::SourceBinding
                | K::SourceLookup(_)
                | K::JoinedSource(_) => {
                    self.tagged(expr, Walker::Scalar);
                    self.recurse(walker, expr, Value);
                }
                K::FirstAvailable => self.recurse(walker, expr, Selection),
                K::Extreme | K::Imputation | K::Temporal | K::StrTemplate => {
                    self.recurse(walker, expr, Value)
                }
                K::Case => self.recurse(walker, expr, Branch),
                K::StrCase | K::StrExtract | K::StrReplace | K::StrPad | K::Round => {
                    self.recurse(walker, expr, Value)
                }
                _ => (),
            },
            Walker::Imputation => match kind {
                K::Imputation | K::Temporal => {
                    self.tagged(expr, Walker::Imputation);
                    self.recurse(walker, expr, Value);
                }
                K::StrCase
                | K::StrPad
                | K::StrContains
                | K::StrExtract
                | K::StrReplace
                | K::MappingCut
                | K::Round
                | K::Extreme
                | K::StrConcat
                | K::StrTemplate => self.recurse(walker, expr, Value),
                K::Case => self.recurse(walker, expr, Branch),
                _ => (),
            },
            Walker::StrCase => match kind {
                K::StrCase => {
                    self.tagged(expr, Walker::StrCase);
                    self.recurse(walker, expr, Value);
                }
                K::Extreme
                | K::StrConcat
                | K::Imputation
                | K::StrTemplate
                | K::Temporal
                | K::StrContains
                | K::StrExtract
                | K::StrReplace
                | K::StrPad
                | K::MappingCut
                | K::Round => self.recurse(walker, expr, Value),
                K::Case => self.recurse(walker, expr, Branch),
                _ => (),
            },
            // The extraction walker does not descend into `str_contains`.
            Walker::StrExtract => match kind {
                K::StrExtract => {
                    self.tagged(expr, Walker::StrExtract);
                    self.recurse(walker, expr, Value);
                }
                K::Extreme
                | K::StrConcat
                | K::Imputation
                | K::StrTemplate
                | K::Temporal
                | K::StrCase
                | K::StrPad
                | K::StrReplace
                | K::MappingCut
                | K::Round => self.recurse(walker, expr, Value),
                K::Case => self.recurse(walker, expr, Branch),
                _ => (),
            },
            Walker::StrReplace => match kind {
                K::StrReplace => {
                    self.tagged(expr, Walker::StrReplace);
                    self.recurse(walker, expr, Value);
                }
                K::Extreme
                | K::StrConcat
                | K::Imputation
                | K::StrTemplate
                | K::Temporal
                | K::StrCase
                | K::StrPad
                | K::StrContains
                | K::StrExtract
                | K::MappingCut
                | K::Round => self.recurse(walker, expr, Value),
                K::Case => self.recurse(walker, expr, Branch),
                _ => (),
            },
            Walker::StrTemplate => match kind {
                K::StrTemplate => {
                    self.tagged(expr, Walker::StrTemplate);
                    self.recurse(walker, expr, Value);
                }
                K::Extreme
                | K::StrConcat
                | K::Imputation
                | K::Temporal
                | K::StrCase
                | K::StrPad
                | K::StrContains
                | K::StrExtract
                | K::StrReplace
                | K::MappingCut
                | K::Round => self.recurse(walker, expr, Value),
                K::Case => self.recurse(walker, expr, Branch),
                _ => (),
            },
        }
    }
}

/// Callback sites in the order the function port admits them: each plan's
/// template and column assignments, then its donor plans, last found first.
fn functions(plan: &ResolvedPlan, calls: Vec<Call>) -> Vec<FunctionSlot> {
    let mut found = Vec::new();
    let mut pending = vec![RegionId::DATASET];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(region) = pending.pop() {
        if !seen.insert(region) {
            continue;
        }
        for template in plan.region_templates(region) {
            for binding in plan.row_bindings(template) {
                admit(plan, plan.binding(binding).expr, &mut found);
            }
        }
        for &column in &plan.region(region).column_order {
            if let Some(binding) = plan.column_bindings(column).next() {
                admit(plan, plan.binding(binding).expr, &mut found);
            }
        }
        for relation in plan.region_relations(region) {
            let node = plan.relation(relation);
            if let (RelationKind::Named { .. }, Some(donor)) = (node.kind, node.derivations) {
                pending.push(donor);
            }
        }
    }
    found
        .into_iter()
        .filter_map(|expr| {
            calls
                .iter()
                .find(|call| call.expr == expr)
                .map(|call| FunctionSlot {
                    expr,
                    slot: call.slot,
                    signature: call.signature.clone(),
                })
        })
        .collect()
}

fn admit(plan: &ResolvedPlan, expr: ExprId, found: &mut Vec<ExprId>) {
    use ExprKind as K;
    let follow = |role: OperandRole, found: &mut Vec<ExprId>| {
        for operand in plan.operands(expr) {
            if operand.role == role {
                admit(plan, operand.expr, found);
            }
        }
    };
    match plan.expr(expr).kind {
        K::Aggregate(_) => {
            for operand in plan.operands(expr) {
                if operand.role == OperandRole::Derivation {
                    for inner in plan.operands(operand.expr) {
                        admit(plan, inner.expr, found);
                    }
                }
            }
        }
        K::Function | K::ProjectFunction => found.push(expr),
        K::Case => follow(OperandRole::Branch, found),
        K::FirstAvailable => follow(OperandRole::Selection, found),
        K::Extreme
        | K::StrConcat
        | K::StrTemplate
        | K::Temporal
        | K::Imputation
        | K::StrContains
        | K::StrExtract
        | K::StrReplace
        | K::StrCase
        | K::StrPad
        | K::MappingCut
        | K::Round
        | K::SourceBinding => follow(OperandRole::Value, found),
        _ => (),
    }
}
