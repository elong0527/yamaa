//! The scalar verbs' declaration and operand checks, one per verb kind, in
//! the order preparation has always reported them: rounding, temporal
//! verbs, imputation, `str_case`, the extremes, then text extraction and
//! templates.
//!
//! Each verb keeps its site filter. Every verb checks the column defaults
//! and the key values of aggregates and intermediates; imputation,
//! `str_case` and the extremes also check row overrides, and text verbs
//! every derivation and key value. With row templates a bare name's phase is
//! binding's to decide, so the verbs that type operands leave bare names
//! untyped there, and rounding, which types only bare names, checks nothing.
use super::*;
use crate::ast::{ScalarOp, Slot};

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
    let scalar = |e: ExprId| match &spec.expr(e).kind {
        ExprKind::Scalar { op, operands, .. } => Some((op, operands)),
        _ => None,
    };
    if !rows {
        for (e, _) in sites(false) {
            if let Some((ScalarOp::Round { .. }, operands)) = scalar(e) {
                rounding(spec, e, operands, findings);
            }
        }
    }
    for (e, rows) in sites(false) {
        if let Some((op, _)) = scalar(e) {
            if temporal(op) {
                temporal::operand_findings(spec, e, declared, rows, findings)?;
            }
        }
    }
    for (e, rows) in sites(true) {
        if let Some((
            ScalarOp::Impute { .. }
            | ScalarOp::Precision { .. }
            | ScalarOp::ImputationFlag { .. }
            | ScalarOp::ToDate,
            _,
        )) = scalar(e)
        {
            imputation::operand_findings(spec, e, declared, rows, findings)?;
        }
    }
    for (e, rows) in sites(true) {
        if let Some((ScalarOp::StrCase { .. }, _)) = scalar(e) {
            str_case::operand_findings(spec, e, declared, rows, findings)?;
        }
    }
    for (e, _) in sites(true) {
        if let Some((ScalarOp::Greatest | ScalarOp::Least, operands)) = scalar(e) {
            extremes(spec, e, operands, declared, findings);
        }
    }
    let mut extraction = Vec::new();
    let mut templates = Vec::new();
    for (e, rows) in text_sites(spec) {
        match scalar(e) {
            Some((ScalarOp::StrExtract { .. } | ScalarOp::StrReplace { .. }, _)) => {
                extraction.push((e, rows));
            }
            Some((ScalarOp::StrTemplate { .. }, _)) => {
                templates.push((e, rows));
            }
            _ => {}
        }
    }
    str_extract::findings(spec, extraction, declared, findings)?;
    let columns = spec
        .columns
        .iter()
        .map(|column| column.name.name.as_str())
        .collect::<Vec<_>>();
    str_template::findings(spec, &columns, templates, declared, findings)
}

pub(in crate::specification) fn temporal(op: &ScalarOp) -> bool {
    matches!(
        op,
        ScalarOp::Diff { .. }
            | ScalarOp::StudyDay
            | ScalarOp::ToEpochDay
            | ScalarOp::Shift { .. }
            | ScalarOp::DurationValue { .. }
            | ScalarOp::Dtc
            | ScalarOp::TimeOfDay
    )
}

/// A rounded output column must be numeric. Only a bare output column has a
/// type before binding here: stored fields are binding's to type.
fn rounding(
    spec: &Spec,
    e: ExprId,
    operands: &[ast::Operand],
    findings: &mut Vec<PreflightFinding>,
) {
    let Some(source) = operands
        .iter()
        .find(|operand| operand.slot == Slot::Source)
        .and_then(|operand| name(spec, operand.expr))
    else {
        return;
    };
    let Some(column) = spec
        .columns
        .iter()
        .find(|column| column.name.name == source)
    else {
        return;
    };
    if !matches!(column.kind, ColumnType::Int | ColumnType::Float) {
        findings.push(PreflightFinding::RoundingInput {
            path: path(spec, e),
            source: source.into(),
            actual: type_name(column.kind),
        });
    }
}

/// VALUE-19: every operand of `greatest` or `least` with a known type, and
/// all of them typed, compare with the first.
fn extremes(
    spec: &Spec,
    e: ExprId,
    operands: &[ast::Operand],
    declared: &types::DeclaredTypes<'_>,
    findings: &mut Vec<PreflightFinding>,
) {
    let sources = operands
        .iter()
        .map(|operand| name(spec, operand.expr).unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    let types = sources
        .iter()
        .map(|name| declared.name(name).column_type())
        .collect::<Vec<_>>();
    let known = types.iter().flatten().copied().collect::<Vec<_>>();
    let comparable =
        |a: ColumnType, b: ColumnType| crate::result_type::ResultType::from(a).comparable(b.into());
    if known
        .first()
        .is_some_and(|&first| known.iter().any(|&kind| !comparable(first, kind)))
        && known.len() == types.len()
    {
        findings.push(PreflightFinding::Scalar(
            crate::scalar_language::incomparable_diagnostic(&sources, &known, &path(spec, e)),
        ));
    }
}

/// Where the text verbs are checked, each position with whether a bare name
/// it reads is left to binding: every column default, row override,
/// intermediate key value and intermediate derivation, and below each the
/// case results, aggregate key values and aggregate `derive` bindings.
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
        if let Some(JoinKey::Pairs { pairs, .. }) = &item.key {
            roots.extend(pairs.iter().map(|&(_, value)| (value, rows)));
        }
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
