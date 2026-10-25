//! Preflight over the typed tree (#2844, design section 3).
//!
//! The root checks read the tree's typed fields ([`root`]). Every other
//! check runs once per node kind: [`ast::Spec::sites`] names the roots, each
//! check keeps the site filter it has always had (rounding, for example,
//! skips a spec with row templates), a walk from each root finds the nodes
//! of its kind, and [`types::DeclaredTypes`] types what they read before any
//! schema is captured.
//!
//! `check()` and `run()` run the same checks: one that refuses a spec before
//! binding ([`verbs`], [`odm`], [`lookups`]) refuses it in both. What binding
//! decides comes after, from binding itself: `run()` binds the captured
//! schemas, and `check()` the declared ones (#3805).
//!
//! A check that needs an operation's executor type constructs it from the
//! tree, as binding does.

pub(super) mod lookups;
pub(super) mod odm;
pub(super) mod root;
pub(super) mod verbs;

use super::*;
use crate::ast::{self, ExprId, ExprKind, JoinKey, Spec};

/// A node's path, as the spec's diagnostics cite it.
pub(in crate::specification) fn path(spec: &Spec, e: ExprId) -> String {
    spec.cite(spec.expr(e).meta)
}

/// A node's written position, as the spec's diagnostics cite it.
pub(in crate::specification) fn render(spec: &Spec, meta: ast::Meta) -> String {
    spec.cite(meta)
}

/// The operand of a verb in `slot`.
pub(in crate::specification) fn operand(
    operands: &[ast::Operand],
    slot: ast::Slot,
) -> Option<ExprId> {
    operands
        .iter()
        .find(|operand| operand.slot == slot)
        .map(|operand| operand.expr)
}

/// The name a verb's operand in `slot` reads.
pub(in crate::specification) fn operand_name<'s>(
    spec: &'s Spec,
    operands: &[ast::Operand],
    slot: ast::Slot,
) -> Option<&'s str> {
    operand(operands, slot).and_then(|e| name(spec, e))
}

/// A written handler's value, refused at its own path as preparation
/// refuses any written value it cannot hold.
pub(in crate::specification) fn handler(
    spec: &Spec,
    handler: Option<&ast::Constant>,
) -> Result<Option<crate::value::Value>, PrepareError> {
    handler
        .map(|handler| constant(&handler.value, &render(spec, handler.meta)))
        .transpose()
}

/// Whether the node at `meta` is written below the spec's field `field`,
/// such as `rows` for a row template's own derivation.
pub(in crate::specification) fn under(spec: &Spec, meta: ast::Meta, field: &str) -> bool {
    let mut at = meta.path;
    while let Some(parent) = spec.paths.parent(at) {
        if spec.paths.parent(parent).is_none() {
            return matches!(spec.paths.segment(at), ast::Segment::Field(name) if *name == field);
        }
        at = parent;
    }
    false
}

/// The derivation positions at and below `root`: the root, then each case
/// result in written order, depth first. These are the only places a
/// verb, a window or an aggregate nests (EXPR-3).
pub(super) fn positions(spec: &Spec, root: ExprId) -> Vec<ExprId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        found.push(e);
        if let ExprKind::Case(arms) = &spec.expr(e).kind {
            stack.extend(arms.iter().rev().map(|arm| arm.result));
        }
    }
    found
}

/// The current-row values each aggregate at a position below `root` pairs
/// with a donor column.
pub(super) fn aggregate_key_values(spec: &Spec, root: ExprId) -> Vec<ExprId> {
    positions(spec, root)
        .into_iter()
        .filter_map(|e| match &spec.expr(e).kind {
            ExprKind::Aggregate(aggregate) => match &aggregate.key {
                Some(JoinKey::Pairs { pairs, .. }) => {
                    Some(pairs.iter().map(|&(_, value)| value).collect::<Vec<_>>())
                }
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .collect()
}

/// Every key value a verb check visits besides the column and row
/// derivations: the aggregates' key values in column defaults, then in row
/// overrides, then each intermediate's own key values and nearest reference
/// (LOOKUP-26) and those of the aggregates in its derivations.
pub(super) fn key_sites(spec: &Spec) -> Vec<ExprId> {
    let mut sites = Vec::new();
    for column in &spec.columns {
        if let Some(derivation) = &column.derivation {
            sites.extend(aggregate_key_values(spec, derivation.expr));
        }
    }
    for template in &spec.rows.templates {
        for (_, derivation) in &template.overrides {
            sites.extend(aggregate_key_values(spec, derivation.expr));
        }
    }
    for item in &spec.intermediates {
        sites.extend(item.match_values());
        for binding in &item.bindings {
            sites.extend(aggregate_key_values(spec, binding.derivation.expr));
        }
    }
    sites
}

/// Whether the spec builds its rows with row templates.
pub(super) fn has_rows(spec: &Spec) -> bool {
    !spec.rows.templates.is_empty()
}

/// The driver without row templates: `base`, or the one declared input.
pub(super) fn default_driver(spec: &Spec) -> Option<&str> {
    if let Some(base) = &spec.rows.base {
        return Some(&base.name);
    }
    match spec.inputs.as_slice() {
        [input] => Some(&input.name.name),
        _ => None,
    }
}

/// A row template's driver: its `dataset`, or the one declared input.
pub(super) fn template_driver<'a>(spec: &'a Spec, template: &'a ast::Template) -> Option<&'a str> {
    match &template.driver {
        Some(driver) => Some(&driver.name),
        None => match spec.inputs.as_slice() {
            [input] => Some(&input.name.name),
            _ => None,
        },
    }
}

/// The name a reference reads.
pub(super) fn name(spec: &Spec, e: ExprId) -> Option<&str> {
    match &spec.expr(e).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        _ => None,
    }
}

/// The variable a source operand reads: a reference, or a structured
/// source binding's variable.
pub(super) fn variable(spec: &Spec, e: ExprId) -> Option<&str> {
    match &spec.expr(e).kind {
        ExprKind::Ref(name) => Some(name.as_str()),
        ExprKind::Source(source) => name(spec, source.variable),
        _ => None,
    }
}
