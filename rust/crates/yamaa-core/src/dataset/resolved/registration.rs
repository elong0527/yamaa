//! What a run registers before reading any row: handler sites and callback
//! signatures.
//!
//! Every handler site is registered at zero before any row is read, so a site
//! that never fires reports zero (COLUMN-14). The handler table is the
//! executor's registration walk (`DatasetPlan::register_handlers`): the
//! plan's conversion handlers, those of each intermediate's donor plan, then
//! every site of each derivation in evaluation order (template assignments,
//! column assignments, intermediate key expressions), then every site of each
//! intermediate's derivations, intermediate by intermediate in declaration
//! order, each followed by the key expressions of the intermediates its
//! derivations read. Each derivation's sites come in expression order: a
//! node's own sites, then its operands' in written order. A site registers
//! once, where the walk first reaches it.
use super::build::Sites;
use super::*;

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
    let mut order = registration_order(plan, count);
    // A site the walk does not reach still registers up front; the check
    // against the executor's walk then names it.
    let mut placed = vec![false; count];
    for &site in &order {
        placed[site] = true;
    }
    order.extend((0..count).filter(|&site| !placed[site]));
    let mut remap = vec![None; count];
    let mut table = Vec::with_capacity(count);
    for site in order {
        remap[site] = Some(HandlerSiteId(table.len() as u32));
        let (path, kind) = &sites.entries[site];
        table.push(HandlerSite {
            path: path.clone(),
            kind: *kind,
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

/// The registration walk over the arena, in report order.
fn registration_order(plan: &ResolvedPlan, count: usize) -> Vec<usize> {
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
            walk.node(plan.binding(binding).expr);
        }
    }
    for &column in &plan.region(dataset).column_order {
        if let Some(binding) = plan.column_bindings(column).next() {
            walk.node(plan.binding(binding).expr);
        }
    }
    walk.keys(dataset);
    // Each intermediate's derivations, in declaration order, then the key
    // expressions of the intermediates they read, as bound for the donor
    // records. A nested donor plan is an earlier intermediate's, walked in its
    // turn.
    for &relation in &named {
        if let Some(donor) = plan.relation(relation).derivations {
            for &column in &plan.region(donor).column_order {
                if let Some(binding) = plan.column_bindings(column).next() {
                    walk.node(plan.binding(binding).expr);
                }
            }
            walk.keys(donor);
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
    /// The key expressions of a region's named intermediates, in order.
    fn keys(&mut self, region: RegionId) {
        let plan = self.plan;
        for relation in plan.region_relations(region) {
            let node = plan.relation(relation);
            if matches!(node.kind, RelationKind::Named { .. }) {
                for key in &node.keys {
                    if let KeyValue::Expr(expr) = key.value {
                        self.node(expr);
                    }
                }
            }
        }
    }
    /// A node's own sites in node order, then its operands' in operand order.
    /// Predicate operands carry no sites.
    fn node(&mut self, expr: ExprId) {
        for index in 0..self.plan.sites(expr).len() {
            let site = self.plan.sites(expr)[index];
            self.register(site.site);
        }
        for index in 0..self.plan.operands(expr).len() {
            let operand = self.plan.operands(expr)[index];
            self.node(operand.expr);
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
        | K::StrTemplate
        | K::Temporal
        | K::Imputation
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
