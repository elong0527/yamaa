//! Original YAML rounding is one admitted scalar operation, not a formula rewrite.
use super::*;

/// The executor's rounding for the verb at `e`; its source binds as the
/// scope decides.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::Round { digits },
        operands,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let rounding = Rounding::new(source, &path(spec, e), digits).map_err(|_| Refusal::Internal)?;
    Ok(Operation {
        operands: vec![Operand::name(source, format!("{}.source", rounding.path()))],
        kind: Kind::Round(rounding),
    })
}

/// A rounded output column must be numeric. Only a bare output column has a
/// type before binding here: stored fields are binding's to type.
pub(super) fn preflight(spec: &Spec, e: ExprId) -> Vec<Diagnostic> {
    let ExprKind::Scalar { operands, .. } = &spec.expr(e).kind else {
        return Vec::new();
    };
    let Some(source) = operand_name(spec, operands, Slot::Source) else {
        return Vec::new();
    };
    let Some(column) = spec
        .columns
        .iter()
        .find(|column| column.name.name == source)
    else {
        return Vec::new();
    };
    if matches!(column.kind, ColumnType::Int | ColumnType::Float) {
        return Vec::new();
    }
    vec![Rounding::input_diagnostic(
        source,
        &path(spec, e),
        column.kind.name(),
    )]
}
