//! Greatest and least, lowered without converting their intermediate values.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    sources: Vec<String>,
    largest: bool,
    path: String,
}
impl Declaration {
    /// The operation at `e`: each source a name, at the verb's path.
    pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Self, PrepareError> {
        let ExprKind::Scalar { op, operands, .. } = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        Ok(Declaration {
            sources: operands
                .iter()
                .map(|operand| {
                    checks::name(spec, operand.expr)
                        .map(String::from)
                        .ok_or(PrepareError::Internal)
                })
                .collect::<Result<_, _>>()?,
            largest: matches!(op, ast::ScalarOp::Greatest),
            path: checks::path(spec, e),
        })
    }
}
/// VALUE-19's comparability of two declared operand types.
fn comparable(a: ColumnType, b: ColumnType) -> bool {
    crate::result_type::ResultType::from(a).comparable(b.into())
}
fn bound_types(
    sources: &[String],
    catalog: &Catalog,
) -> Result<Vec<Option<ColumnType>>, BindError> {
    Ok(sources.iter().map(|name| catalog.type_of(name)).collect())
}
fn finish_extreme(
    sources: &[String],
    path: &str,
    largest: bool,
    types: Vec<Option<ColumnType>>,
    findings: &mut Vec<BindFinding>,
    operands: Vec<Assignment>,
    start: usize,
) -> Result<Option<Expression>, BindError> {
    let types = types.into_iter().collect::<Option<Vec<_>>>();
    if let Some(types) = types {
        if types
            .first()
            .is_some_and(|&first| types.iter().any(|&kind| !comparable(first, kind)))
        {
            findings.push(BindFinding::Scalar(
                crate::scalar_language::incomparable_diagnostic(sources, &types, path),
            ));
        }
    }
    Ok((findings.len() == start).then_some(Expression::Extreme { operands, largest }))
}
/// Each source binds as the scope decides; a lookup read takes the type its
/// selector's donor captured, any other read the catalog's (VALUE-19).
pub(super) fn bind(
    operation: &Declaration,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let start = cx.findings.len();
    let Declaration {
        sources,
        largest,
        path,
    } = operation;
    let mut operands = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let path = format!("{path}.sources[{index}]");
        if let Some(expression) = cx.operand(source, &path)? {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    let types = sources
        .iter()
        .map(|name| {
            if let Some((index, field)) = cx.donors.named.reference(name) {
                Ok(cx.donors.items[index].as_ref().and_then(|item| {
                    item.source_column(cx.donors.secondary, field)
                        .and_then(|column| item.source_kind(cx.donors.secondary, column))
                }))
            } else {
                bound_types(std::slice::from_ref(name), cx.catalog).map(|types| types[0])
            }
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    finish_extreme(sources, path, *largest, types, cx.findings, operands, start)
}
