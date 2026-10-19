//! The dataset region of a resolved plan, read in the bound plan's terms.
//! Binding hands back the resolved plan (#2821). Its nodes carry the bound
//! plan's payloads: an expression node its operation, a named relation its
//! intermediate, a check its declaration.
use yamaa_core::{
    conversion::LiteralHandler,
    dataset::{
        resolved::{
            Binding, BindingOrigin, CheckPayload, CheckScope, ExprId, ExprKind, Grain, Op,
            OperandRole, Read, RegionId, TemplateNode,
        },
        Check, Intermediate, ResolvedPlan, SecondarySource,
    },
    table::TableSchema,
};

const DATASET: RegionId = RegionId::DATASET;

/// The dataset's driver schema.
pub fn source(plan: &ResolvedPlan) -> &TableSchema {
    let driver = plan.region_sources(DATASET).next().unwrap();
    &plan.sources()[driver.index()].schema
}

/// The dataset's secondary inputs, in plan order.
pub fn secondary(plan: &ResolvedPlan) -> &[SecondarySource] {
    &plan.region(DATASET).secondary
}

/// The dataset's output schema.
pub fn output(plan: &ResolvedPlan) -> &TableSchema {
    &plan.region(DATASET).output
}

/// The output key columns.
pub fn keys(plan: &ResolvedPlan) -> Vec<usize> {
    plan.keys().iter().map(|key| key.index()).collect()
}

/// The bound declaration of each named intermediate, in plan order.
pub fn intermediates(plan: &ResolvedPlan) -> Vec<&Intermediate> {
    (0..)
        .map_while(|index| plan.named(DATASET, index))
        .map(|relation| plan.relation(relation).named.as_deref().unwrap())
        .collect()
}

/// The dataset's row templates, in plan order.
pub fn templates(plan: &ResolvedPlan) -> Vec<&TemplateNode> {
    plan.region_templates(DATASET)
        .map(|template| plan.template(template))
        .collect()
}

/// The grain of a template driven by the dataset's own input, as the bound
/// plan's records, groups and keys modes are.
pub fn grain(template: &TemplateNode) -> &Grain {
    assert_eq!(template.driver.source.index(), 0, "a secondary driver");
    assert_eq!(template.driver.named, None, "a named driver");
    &template.driver.grain
}

/// Template `index`'s row bindings, in the bound plan's assignment order.
pub fn row(plan: &ResolvedPlan, index: usize) -> Vec<&Binding> {
    let template = plan.region_templates(DATASET).nth(index).unwrap();
    plan.row_bindings(template)
        .map(|binding| plan.binding(binding))
        .collect()
}

/// The column-phase bindings, one per binding of the bound plan and in its
/// order. A binding split by driver context shares one expression, so its
/// first part stands for it.
pub fn columns(plan: &ResolvedPlan) -> Vec<&Binding> {
    let mut previous = None;
    plan.region_bindings(DATASET)
        .map(|binding| plan.binding(binding))
        .filter(|binding| match binding.origin {
            BindingOrigin::Column { assignment } => {
                previous.replace(assignment) != Some(assignment)
            }
            BindingOrigin::Template { .. } => false,
        })
        .collect()
}

/// The column each binding completes.
pub fn completed(bindings: &[&Binding]) -> Vec<usize> {
    bindings
        .iter()
        .map(|binding| binding.column.index())
        .collect()
}

/// The operation of an expression node.
pub fn op(plan: &ResolvedPlan, expr: ExprId) -> &Op {
    &plan.expr(expr).op
}

/// Whether `expr` reads field `column` of the driver record, as a bound
/// `Source` read does.
pub fn reads_source(plan: &ResolvedPlan, expr: ExprId, column: u32) -> bool {
    plan.expr(expr).kind == ExprKind::Source && plan.reads(expr) == [Read::Field(column)]
}

/// Whether `expr` reads field `column` of named intermediate `index`, as a
/// bound `Intermediate` read does.
pub fn reads_intermediate(plan: &ResolvedPlan, expr: ExprId, index: usize, column: usize) -> bool {
    let relation = plan.named(DATASET, index).unwrap();
    plan.expr(expr).kind == ExprKind::Intermediate(relation)
        && plan.expr(expr).op == Op::Intermediate { column }
}

/// A node's operands in `role`, in authored order.
pub fn operands(plan: &ResolvedPlan, expr: ExprId, role: OperandRole) -> Vec<ExprId> {
    plan.operands(expr)
        .iter()
        .filter(|operand| operand.role == role)
        .map(|operand| operand.expr)
        .collect()
}

/// The declared dataset checks, in declaration order.
pub fn verifications(plan: &ResolvedPlan) -> Vec<&Check> {
    plan.checks()
        .iter()
        .filter(|check| check.scope == CheckScope::Dataset)
        .map(|check| match &check.payload {
            CheckPayload::Declared(declared) => declared,
            payload => panic!("dataset check holds {payload:?}"),
        })
        .collect()
}

/// The declared column checks, grouped by column in declared column order.
pub fn column_verifications(plan: &ResolvedPlan) -> Vec<(usize, Vec<&Check>)> {
    let mut groups: Vec<(usize, Vec<&Check>)> = Vec::new();
    for check in plan.checks() {
        let (CheckScope::Column(column), CheckPayload::Declared(declared)) =
            (check.scope, &check.payload)
        else {
            continue;
        };
        match groups.last_mut() {
            Some((last, checks)) if *last == column.index() => checks.push(declared),
            _ => groups.push((column.index(), vec![declared])),
        }
    }
    groups
}

/// The declared conversion handlers in the compiler's order, each with the
/// path of the derivation it replaces.
pub fn conversion_handlers(plan: &ResolvedPlan) -> Vec<(&str, &LiteralHandler)> {
    plan.conversions(DATASET)
        .iter()
        .map(|&site| {
            let binding = plan
                .region_bindings(DATASET)
                .map(|binding| plan.binding(binding))
                .find(|binding| binding.unconvertible == Some(site))
                .unwrap();
            (&*binding.path, binding.handler.as_ref().unwrap())
        })
        .collect()
}

/// The declared handler of the derivation at `path`.
pub fn conversion_handler<'p>(plan: &'p ResolvedPlan, path: &str) -> Option<&'p LiteralHandler> {
    plan.region_bindings(DATASET)
        .map(|binding| plan.binding(binding))
        .find(|binding| &*binding.path == path)
        .and_then(|binding| binding.handler.as_ref())
}
