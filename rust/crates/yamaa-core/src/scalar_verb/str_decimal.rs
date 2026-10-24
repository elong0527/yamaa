//! A number written as fixed-decimal text reads one numeric source (TEXT-15).
use super::*;
use crate::text_decimal::Admission;

/// The executor's decimal writing for the verb at `e`, or the declaration
/// findings that refuse it before any source is read; its source binds as
/// the scope decides.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let operation = decimal(spec, e)?;
    Ok(Operation {
        operands: vec![Operand::name(
            operation.source(),
            format!("{}.source", operation.path()),
        )],
        kind: Kind::StrDecimal(operation),
    })
}

fn decimal(spec: &Spec, e: ExprId) -> Result<StringDecimal, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::StrDecimal { decimals, width },
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    StringDecimal::new(
        source.into(),
        path(spec, e),
        decimals,
        width.as_deref(),
        handler(spec, handlers.missing.as_ref())?,
    )
    .map_err(|admission| match admission {
        Admission::Findings(findings) => Refusal::Findings(findings),
        Admission::Shape => Refusal::Internal,
    })
}

/// `str_decimal` reads a number: a source whose declared type is another,
/// then a `decimals` below zero and a `width` below one, in field order.
/// With `rows` a bare name's phase is binding's to decide, so its type is
/// left to binding, which checks it on the type it binds.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let ExprKind::Scalar { operands, .. } = &spec.expr(e).kind else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let kind = (source.contains('.') || !rows)
        .then(|| declared(source))
        .flatten();
    let mut findings = kind
        .filter(|kind| !matches!(kind, ColumnType::Int | ColumnType::Float))
        .map(|kind| StringDecimal::input_diagnostic(source, &path(spec, e), kind.name()))
        .into_iter()
        .collect::<Vec<_>>();
    match decimal(spec, e) {
        Ok(_) => {}
        Err(Refusal::Findings(declaration)) => findings.extend(declaration),
        Err(refusal) => return Err(refusal),
    }
    Ok(findings)
}
