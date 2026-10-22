//! Original string-case declarations bind named operands without coercion.
use super::*;
use crate::text_case::Case;

/// The executor's case conversion for the verb at `e`; its source binds as
/// the scope decides.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let operation = case(spec, e)?;
    Ok(Operation {
        operands: vec![Operand::name(
            operation.source(),
            format!("{}.source", operation.path()),
        )],
        kind: Kind::StrCase(operation),
    })
}

fn case(spec: &Spec, e: ExprId) -> Result<StringCase, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::StrCase { to },
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let mode = match to {
        ast::TextCase::Upper => Case::Upper,
        ast::TextCase::Lower => Case::Lower,
        ast::TextCase::Sentence => Case::Sentence,
        ast::TextCase::Title => Case::Title,
    };
    StringCase::new(
        source.into(),
        path(spec, e),
        mode,
        handler(spec, handlers.missing.as_ref())?,
    )
    .ok_or(Refusal::Internal)
}

/// `str_case` reads text (TEXT-1). With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here; binding checks a
/// row template's bare name on its captured type.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let operation = case(spec, e)?;
    let source = operation.source();
    let kind = (source.contains('.') || !rows)
        .then(|| declared(source))
        .flatten();
    Ok(kind
        .filter(|kind| *kind != ColumnType::Str)
        .map(|kind| operation.input_diagnostic(kind.name()))
        .into_iter()
        .collect())
}
