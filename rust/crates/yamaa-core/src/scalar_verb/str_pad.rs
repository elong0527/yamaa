//! Original scalar padding shares named-reference and phase binding authority.
use super::*;
use crate::text_pad::DeclarationError;

/// The executor's padding for the verb at `e`, or its width defect, which
/// binding reports at the derivation's checkpoint ([`Operation::defect`]).
/// Its source binds as the scope decides; a defect leaves nothing to wrap.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::StrPad { width },
        operands,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let path = path(spec, e);
    let operation = match StringPad::new(source, &path, width) {
        Ok(pad) => Ok(pad),
        Err(DeclarationError::Width) => Err(DeclarationError::Width
            .diagnostic(&path)
            .ok_or(Refusal::Internal)?),
        Err(DeclarationError::Shape) => return Err(Refusal::Internal),
    };
    Ok(Operation {
        operands: vec![Operand::name(source, format!("{path}.source"))],
        kind: Kind::StrPad(operation),
    })
}
