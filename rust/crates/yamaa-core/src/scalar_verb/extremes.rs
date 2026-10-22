//! Greatest and least, lowered without converting their intermediate values.
use super::*;

/// The operation at `e`: each source a name, at the verb's path. Each
/// source binds as the scope decides; binding then compares the captured
/// types of them all ([`Operation::incomparable`]).
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
        return Err(Refusal::Internal);
    };
    let sources = operands
        .iter()
        .map(|operand| {
            name(spec, operand.expr)
                .map(String::from)
                .ok_or(Refusal::Internal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let path = path(spec, e);
    Ok(Operation {
        operands: sources
            .iter()
            .enumerate()
            .map(|(index, source)| Operand::name(source, format!("{path}.sources[{index}]")))
            .collect(),
        kind: Kind::Extreme {
            largest: matches!(op, ScalarOp::Greatest),
            sources,
            path,
        },
    })
}

/// VALUE-19: every operand of `greatest` or `least` with a known declared
/// type, and all of them typed, compare with the first.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
) -> Vec<Diagnostic> {
    let ExprKind::Scalar { operands, .. } = &spec.expr(e).kind else {
        return Vec::new();
    };
    let sources = operands
        .iter()
        .map(|operand| name(spec, operand.expr).unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    let types = sources
        .iter()
        .map(|name| declared(name))
        .collect::<Option<Vec<_>>>();
    types
        .and_then(|types| incomparable(&sources, &types, &path(spec, e)))
        .into_iter()
        .collect()
}

/// VALUE-19's refusal of operands of `types` that do not all compare with
/// the first.
pub(super) fn incomparable(
    sources: &[String],
    types: &[ColumnType],
    path: &str,
) -> Option<Diagnostic> {
    let comparable = |a: ColumnType, b: ColumnType| ResultType::from(a).comparable(b.into());
    types
        .first()
        .is_some_and(|&first| types.iter().any(|&kind| !comparable(first, kind)))
        .then(|| crate::scalar_language::incomparable_diagnostic(sources, types, path))
}
