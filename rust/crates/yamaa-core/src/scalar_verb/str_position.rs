//! The positional text verbs, `str_substr` and `str_length`, each reading one
//! text source by scalar position (TEXT-13, TEXT-14).
use super::*;
use crate::text_position::{DeclarationError, Fields, StringLength, StringSubstr};

/// The executor's operation for the verb at `e`; its source binds as the
/// scope decides. A `str_substr` whose fields TEXT-13 or TEXT-16 refuses has
/// no operation, only its declaration defect, which binding reports at the
/// derivation's checkpoint ([`Operation::defect`]).
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let path = path(spec, e);
    let missing = handler(spec, handlers.missing.as_ref())?;
    let kind = match op {
        ScalarOp::StrSubstr {
            start,
            length,
            piece,
        } => Kind::StrSubstr(
            match StringSubstr::new(
                source.into(),
                path.clone(),
                Fields {
                    start: start.as_deref(),
                    length: length.as_deref(),
                    piece: piece.as_deref(),
                },
                missing,
            ) {
                Ok(operation) => Ok(operation),
                Err(DeclarationError::Shape) => return Err(Refusal::Internal),
                Err(defect) => Err(defect.diagnostic(&path).ok_or(Refusal::Internal)?),
            },
        ),
        ScalarOp::StrLength => Kind::StrLength(
            StringLength::new(source.into(), path.clone(), missing).ok_or(Refusal::Internal)?,
        ),
        _ => return Err(Refusal::Internal),
    };
    Ok(Operation {
        operands: vec![Operand::name(source, format!("{path}.source"))],
        kind,
    })
}

/// A positional verb reads text (TEXT-1). Without `rows` a declared column
/// or stored field it reads must be text; with `rows` a bare name's phase is
/// binding's to decide, and binding types the source
/// ([`Typing::Catalog`]).
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
    Ok((!rows)
        .then(|| declared(source))
        .flatten()
        .filter(|kind| *kind != ColumnType::Str)
        .map(|kind| crate::text_position::input_diagnostic(source, &path(spec, e), kind.name()))
        .into_iter()
        .collect())
}
