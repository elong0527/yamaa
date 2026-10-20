//! Original YAML rounding is one admitted scalar operation, not a formula rewrite.
use super::*;
use crate::dataset::Rounding;

/// The executor's rounding for the verb at `e`.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Rounding, PrepareError> {
    let ExprKind::Scalar {
        op: ast::ScalarOp::Round { digits },
        operands,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let source =
        checks::operand_name(spec, operands, ast::Slot::Source).ok_or(PrepareError::Internal)?;
    Rounding::new(source, &checks::path(spec, e), digits).map_err(|_| PrepareError::Internal)
}

/// The source operand binds as the scope decides ([`BindContext::operand`]).
pub(super) fn bind(
    rounding: &Rounding,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    cx.operand(rounding.source(), &format!("{}.source", rounding.path()))
        .map(|source| source.map(|source| wrap(rounding, column, source)))
}
fn wrap(rounding: &Rounding, column: usize, source: Expression) -> Expression {
    Expression::Round {
        source: std::boxed::Box::new(Assignment {
            column,
            expression: source,
            path: format!("{}.source", rounding.path()),
        }),
        rounding: rounding.clone(),
    }
}
