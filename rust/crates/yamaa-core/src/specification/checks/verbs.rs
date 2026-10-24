//! The scalar verbs' declaration and operand checks, one per verb kind, in
//! the order preparation has always reported them ([`Preflight::ORDER`]):
//! rounding, temporal verbs, imputation, `str_case`, the extremes, text
//! extraction and templates, the positional text verbs, then fixed-decimal
//! text. Each verb names its check and makes its own findings
//! ([`crate::scalar_verb::preflight`]).
//!
//! Each check keeps its site filter ([`Preflight::sites`]). Every check
//! visits the column defaults and the key values of aggregates and
//! intermediates; imputation, `str_case` and the extremes also visit row
//! overrides, and the extraction, template, positional and decimal text
//! verbs every derivation and key value. With row templates a bare name's
//! phase is binding's to decide, so the checks that type operands leave bare
//! names untyped there, and rounding, which types only bare names, checks
//! nothing.
use super::*;
use crate::scalar_verb::{Preflight, Sites};

/// Every verb's findings, appended to `findings` in the established order.
pub(in crate::specification) fn findings(
    spec: &Spec,
    declared: &types::DeclaredTypes<'_>,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let rows = has_rows(spec);
    let defaults = spec
        .columns
        .iter()
        .filter_map(|column| column.derivation.as_ref().map(|derivation| derivation.expr))
        .collect::<Vec<_>>();
    let overrides = spec
        .rows
        .templates
        .iter()
        .flat_map(|template| &template.overrides)
        .map(|(_, derivation)| derivation.expr)
        .collect::<Vec<_>>();
    let keys = key_sites(spec);
    // Each site with whether a bare name it reads is left to binding.
    let sites = |overrides_too: bool| {
        defaults
            .iter()
            .map(|&root| (root, rows))
            .chain(
                overrides
                    .iter()
                    .filter(move |_| overrides_too)
                    .map(|&root| (root, true)),
            )
            .chain(keys.iter().map(|&root| (root, rows)))
            .flat_map(|(root, rows)| {
                positions(spec, root)
                    .into_iter()
                    .map(move |position| (position, rows))
            })
            .collect::<Vec<_>>()
    };
    let typed = |name: &str| declared.name(name).column_type();
    for check in Preflight::ORDER {
        if rows && !check.with_row_templates() {
            continue;
        }
        let sites = match check.sites() {
            Sites::Defaults => sites(false),
            Sites::Derivations => sites(true),
            Sites::Everywhere => text_sites(spec),
        };
        for (e, rows) in sites {
            let ExprKind::Scalar { op, .. } = &spec.expr(e).kind else {
                continue;
            };
            if op.preflight() == Some(check) {
                findings.extend(
                    crate::scalar_verb::preflight(spec, e, &typed, rows)
                        .map_err(refused_verb)?
                        .into_iter()
                        .map(PreflightFinding::Scalar),
                );
            }
        }
    }
    Ok(())
}

/// Where the text verbs are checked, each position with whether a bare name
/// it reads is left to binding: every column default, row override,
/// intermediate key value, nearest reference and intermediate derivation,
/// and below each the case results, aggregate key values and aggregate
/// `derive` bindings.
fn text_sites(spec: &Spec) -> Vec<(ExprId, bool)> {
    let rows = has_rows(spec);
    let mut roots = Vec::new();
    for column in &spec.columns {
        if let Some(derivation) = &column.derivation {
            roots.push((derivation.expr, rows));
        }
    }
    for template in &spec.rows.templates {
        for (_, derivation) in &template.overrides {
            roots.push((derivation.expr, true));
        }
    }
    for item in &spec.intermediates {
        roots.extend(item.match_values().map(|value| (value, rows)));
        for binding in &item.bindings {
            roots.push((binding.derivation.expr, true));
        }
    }
    let mut sites = Vec::new();
    for root in roots {
        let mut stack = vec![root];
        while let Some((e, rows)) = stack.pop() {
            sites.push((e, rows));
            let mut children = Vec::new();
            match &spec.expr(e).kind {
                ExprKind::Aggregate(aggregate) => {
                    if let Some(JoinKey::Pairs { pairs, .. }) = &aggregate.key {
                        children.extend(pairs.iter().map(|&(_, value)| (value, rows)));
                    }
                    children.extend(
                        aggregate
                            .bindings
                            .iter()
                            .map(|binding| (binding.derivation.expr, true)),
                    );
                }
                ExprKind::Case(arms) => children.extend(arms.iter().map(|arm| (arm.result, rows))),
                _ => {}
            }
            stack.extend(children.into_iter().rev());
        }
    }
    sites
}
