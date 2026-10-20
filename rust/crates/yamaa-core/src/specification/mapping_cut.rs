//! One mapping/cut declaration and binder shared by root, row and case dispatch.
use super::*;
use crate::{
    mapping_cut::{AdmissionError, Cut, Mapping, MappingCut},
    value::ColumnType,
};

#[derive(Clone, Debug)]
pub(super) struct Declaration {
    pub source: source_expressions::Declaration,
    pub operation: MappingCut,
}
fn admission<T>(result: Result<T, AdmissionError>) -> Result<T, PrepareError> {
    result.map_err(|error| match error {
        AdmissionError::Shape => PrepareError::Internal,
        AdmissionError::Diagnostic(finding) => {
            PrepareError::Invalid(vec![PreflightFinding::Scalar(finding)])
        }
    })
}
/// The mapping or cut at `e`: its source binding and the executor's
/// operation. A dictionary file is refused before binding.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Declaration, PrepareError> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    let source = source_expressions::Declaration::from_tree(
        spec,
        checks::operand(operands, ast::Slot::Source).ok_or(PrepareError::Internal)?,
    )?;
    let missing = checks::handler(spec, handlers.missing.as_ref())?;
    let operation = match op {
        ast::ScalarOp::Mapping {
            dict,
            case_sensitive,
        } => {
            let entries = match dict {
                ast::Dictionary::Inline(entries) => entries
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.text.clone(),
                            constant(&value.value, &checks::render(spec, value.meta))?,
                        ))
                    })
                    .collect::<Result<Vec<_>, PrepareError>>()?,
                ast::Dictionary::File(_) => Vec::new(),
            };
            let unmapped = checks::handler(spec, handlers.unmapped.as_ref())?;
            MappingCut::Mapping(admission(Mapping::new(
                source.variable.clone(),
                path,
                entries,
                *case_sensitive,
                missing,
                unmapped,
            ))?)
        }
        ast::ScalarOp::Cut {
            breaks,
            labels,
            closed,
        } => {
            let findings = Cut::declaration_findings(&path, breaks, labels);
            if !findings.is_empty() {
                return Err(PrepareError::Invalid(
                    findings.into_iter().map(PreflightFinding::Scalar).collect(),
                ));
            }
            MappingCut::Cut(admission(Cut::new(
                source.variable.clone(),
                path,
                breaks.clone(),
                labels.clone(),
                *closed == ast::Closed::Right,
                missing,
            ))?)
        }
        _ => return Err(PrepareError::Internal),
    };
    Ok(Declaration { source, operation })
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
fn checked_wrap(
    declaration: &Declaration,
    column: usize,
    expression: Expression,
    kind: Option<ColumnType>,
    findings: &mut Vec<BindFinding>,
) -> Option<Expression> {
    if let Some(kind) = kind.filter(|&kind| !declaration.operation.accepts(kind)) {
        findings.push(BindFinding::Scalar(
            declaration.operation.input_diagnostic(kind_name(kind)),
        ));
        return None;
    }
    Some(Expression::MappingCut {
        source: std::boxed::Box::new(Assignment {
            column,
            // Collection multiplicity belongs to the enclosing mapping/cut operation.
            // Static source/type binding still uses the authored `.source` leaf.
            path: declaration.operation.path().into(),
            expression,
        }),
        operation: declaration.operation.clone(),
    })
}
/// The structured source binds as the scope decides; its input type is a
/// lookup read's donor type or the catalog's type of the name.
pub(super) fn bind(
    declaration: &Declaration,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", declaration.operation.path());
    let name = &declaration.source.variable;
    let expression = source_expressions::bind(cx, &declaration.source, column, &path)?;
    let kind = if let Some((index, field)) = cx.donors.named.reference(name) {
        cx.donors
            .items
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|item| {
                let column = item.source_column(cx.donors.secondary, field)?;
                item.source_kind(cx.donors.secondary, column)
            })
    } else {
        cx.catalog.type_of(name)
    };
    Ok(expression
        .and_then(|expression| checked_wrap(declaration, column, expression, kind, cx.findings)))
}
