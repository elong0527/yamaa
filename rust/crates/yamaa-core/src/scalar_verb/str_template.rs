//! Whole-occurrence template admission followed by shared reference binding.
use super::*;

/// The `str_template` at `e`, checked before binding, with whether a bare
/// name it reads is left to binding (`rows`): its syntax, and without `rows`
/// each placeholder a declared column or a stored field of type `str`.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let operation = template(spec, e)?;
    if let Some(error) = operation.declaration_diagnostic() {
        return Ok(vec![error.clone()]);
    }
    if rows {
        return Ok(Vec::new());
    }
    let mut findings = Vec::new();
    for (index, source) in operation.identifiers().iter().enumerate() {
        if !source.contains('.')
            && !spec
                .columns
                .iter()
                .any(|column| column.name.name == *source)
        {
            findings.extend(operation.unknown_diagnostic(index));
        } else if let Some(kind) = declared(source).filter(|kind| *kind != ColumnType::Str) {
            findings.extend(operation.input_diagnostic(index, kind.name()));
        }
    }
    Ok(findings)
}

/// The executor's template for the verb at `e`. Each placeholder binds as
/// the scope decides, at the template's path.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let operation = template(spec, e)?;
    let at = format!("{}.template", operation.path());
    let operands = operation
        .identifiers()
        .iter()
        .map(|source| Operand::name(source, at.clone()))
        .collect();
    Ok(Operation {
        kind: Kind::StrTemplate(operation),
        operands,
    })
}

fn template(spec: &Spec, e: ExprId) -> Result<StringTemplate, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::StrTemplate { template },
        handlers,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    StringTemplate::new(
        template.clone(),
        path(spec, e),
        handler(spec, handlers.missing.as_ref())?,
    )
    .map_err(|_| Refusal::Internal)
}
