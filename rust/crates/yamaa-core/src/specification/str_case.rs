//! Original string-case declarations bind named operands without coercion.
use super::*;
use crate::text_case::{Case, StringCase};

/// The executor's case conversion for the verb at `e`.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<StringCase, PrepareError> {
    let ExprKind::Scalar {
        op: ast::ScalarOp::StrCase { to },
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let source =
        checks::operand_name(spec, operands, ast::Slot::Source).ok_or(PrepareError::Internal)?;
    let mode = match to {
        ast::TextCase::Upper => Case::Upper,
        ast::TextCase::Lower => Case::Lower,
        ast::TextCase::Sentence => Case::Sentence,
        ast::TextCase::Title => Case::Title,
    };
    StringCase::new(
        source.into(),
        checks::path(spec, e),
        mode,
        checks::handler(spec, handlers.missing.as_ref())?,
    )
    .ok_or(PrepareError::Internal)
}

/// `str_case` reads text (TEXT-1). With `rows` a bare name's phase is
/// `str_case` reads text (TEXT-1). With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here.
pub(super) fn operand_findings(
    spec: &ast::Spec,
    e: ExprId,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let operation = from_tree(spec, e)?;
    let source = operation.source();
    let kind = (source.contains('.') || !rows)
        .then(|| declared.name(source).column_type())
        .flatten();
    if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Str) {
        findings.push(PreflightFinding::TextCase(
            operation.input_diagnostic(checks::type_name(kind)),
        ));
    }
    Ok(())
}

/// The source binds as the scope decides. Preflight checks the declared type
/// of every name without `rows` and of a qualified name with them, so a row
/// template's bare name is checked here, on its captured type (TEXT-1).
pub(super) fn bind(
    operation: &StringCase,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", operation.path());
    let source = cx.operand(operation.source(), &path)?;
    if source.is_some() && cx.implicit().is_none() {
        if let Some(kind) = cx
            .catalog
            .type_of(operation.source())
            .filter(|kind| *kind != ColumnType::Str)
        {
            cx.findings.push(BindFinding::Scalar(
                operation.input_diagnostic(checks::type_name(kind)),
            ));
            return Ok(None);
        }
    }
    Ok(source.map(|expression| Expression::StrCase {
        source: std::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
