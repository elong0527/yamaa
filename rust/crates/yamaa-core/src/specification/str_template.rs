//! Whole-occurrence template admission followed by shared reference binding.
use super::*;
use crate::text_template::StringTemplate;
/// Admit each `str_template` at `sites`, in order, with whether a bare name
/// it reads is left to binding (`rows`): its syntax, and without `rows` each
/// placeholder a declared column or a stored field of type `str`. `columns`
/// are the declared output columns.
pub(super) fn findings(
    spec: &ast::Spec,
    columns: &[&str],
    sites: Vec<(ExprId, bool)>,
    declared: &types::DeclaredTypes<'_>,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    for (e, rows) in sites {
        let operation = from_tree(spec, e)?;
        if let Some(error) = operation.declaration_diagnostic() {
            findings.push(PreflightFinding::TextTemplate(error.clone()));
        } else if !rows {
            for (index, source) in operation.identifiers().iter().enumerate() {
                if !source.contains('.') && !columns.contains(&source.as_str()) {
                    findings.push(PreflightFinding::TextTemplate(
                        operation.unknown_diagnostic(index).expect("admitted index"),
                    ));
                } else if let Some(kind) = declared
                    .name(source)
                    .column_type()
                    .filter(|kind| *kind != ColumnType::Str)
                {
                    findings.push(PreflightFinding::TextTemplate(
                        operation
                            .input_diagnostic(index, kind_name(kind))
                            .expect("admitted index"),
                    ));
                }
            }
        }
    }
    Ok(())
}
/// The executor's template for the verb at `e`.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<StringTemplate, PrepareError> {
    let ExprKind::Scalar {
        op: ast::ScalarOp::StrTemplate { template },
        handlers,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    StringTemplate::new(
        template.clone(),
        checks::path(spec, e),
        checks::handler(spec, handlers.missing.as_ref())?,
    )
    .map_err(|_| PrepareError::Internal)
}
fn kind_name(kind: ColumnType) -> &'static str {
    match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}

fn finish(
    operation: &StringTemplate,
    source: &str,
    index: usize,
    expression: Option<Expression>,
    start: usize,
    catalog: &Catalog,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    if expression.is_none() {
        // Bare output names have the text-operation rule; qualified names
        // retain their original resource/reference-scope owner and complete facts.
        if !source.contains('.') {
            for finding in &mut findings[start..] {
                if matches!(
                    finding,
                    BindFinding::UnknownReference { .. }
                        | BindFinding::OutputReference {
                            finding: reference_binding::Diagnostic::UnknownField,
                            ..
                        }
                ) {
                    *finding = BindFinding::TextTemplate(
                        operation.unknown_diagnostic(index).expect("admitted index"),
                    );
                }
            }
        }
        return Ok(None);
    }
    if let Some(kind) = catalog.type_of(source) {
        if kind != ColumnType::Str {
            findings.push(BindFinding::TextTemplate(
                operation
                    .input_diagnostic(index, kind_name(kind))
                    .expect("admitted index"),
            ));
            return Ok(None);
        }
    }
    Ok(expression)
}
/// Each placeholder binds as the scope decides, at the template's path.
pub(super) fn bind(
    operation: &StringTemplate,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let start = cx.findings.len();
    let mut operands = Vec::new();
    if let Some(diagnostic) = operation.declaration_diagnostic() {
        cx.findings
            .push(BindFinding::TextTemplate(diagnostic.clone()));
        return Ok(None);
    }
    for (index, source) in operation.identifiers().iter().enumerate() {
        let at = cx.findings.len();
        let path = format!("{}.template", operation.path());
        let expression = cx.operand(source, &path)?;
        if let Some(expression) = finish(
            operation,
            source,
            index,
            expression,
            at,
            cx.catalog,
            cx.findings,
        )? {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    if cx.findings.len() != start {
        Ok(None)
    } else {
        Ok(Some(Expression::StrTemplate {
            operands,
            operation: operation.clone(),
        }))
    }
}
