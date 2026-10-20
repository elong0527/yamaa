//! A root predicate reads only the base driver, before key construction (ROW-9).
use super::*;
use crate::json::Value as JsonValue;
use crate::{
    bound_expression::BoundPredicate,
    diagnostic::{ConditionCode, Diagnostic},
};

/// The root filter at `filter`, a formula beyond the expression compiler's
/// limits refused before any source is read.
pub(super) fn admit(spec: &ast::Spec, filter: Option<ExprId>) -> Result<(), PrepareError> {
    let Some(filter) = filter else {
        return Ok(());
    };
    let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
        return Err(PrepareError::Internal);
    };
    if let Err(error @ CompileError::Limit { .. }) = &formula.compiled {
        return Err(PrepareError::expression(
            error.clone(),
            Some(&checks::render(spec, formula.text.meta)),
        ));
    }
    Ok(())
}

/// Bind the root filter at `filter` over the base driver's records.
pub(super) fn bind(
    spec: &ast::Spec,
    filter: ExprId,
    driver: &str,
    source: &TableSchema,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<BoundPredicate>, BindError> {
    {
        let ExprKind::Formula(formula) = &spec.expr(filter).kind else {
            return Err(BindError::Internal);
        };
        let path = checks::render(spec, formula.text.meta);
        let plan = match &formula.compiled {
            Ok(plan) => plan,
            Err(CompileError::Invalid {
                position, failure, ..
            }) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    &path,
                    &formula.text.text,
                    position.character,
                    failure,
                )));
                return Ok(None);
            }
            Err(error) => return Err(BindError::ExpressionPolicy(error.clone())),
        };
        let start = findings.len();
        let mut seen = std::collections::BTreeSet::new();
        let mut bindings = Vec::new();
        for identifier in plan.identifiers() {
            if !seen.insert(identifier) {
                continue;
            }
            let Some((dataset, field)) = identifier.split_once('.') else {
                findings.push(BindFinding::Predicate(Diagnostic {
                    code: ConditionCode::RootFilterPhaseBoundary,
                    spec_paths: vec![path.clone()],
                    context: [
                        ("identifier", identifier),
                        ("available_phase", "column_derivation"),
                        ("required_phase", "row_filter"),
                    ]
                    .into_iter()
                    .map(|(name, text)| (name.into(), JsonValue::String(text.into())))
                    .collect(),
                    source_span: None,
                    operand_route: None,
                }));
                continue;
            };
            let selected = (dataset == driver)
                .then(|| {
                    source
                        .columns()
                        .iter()
                        .position(|column| column.name == field)
                })
                .flatten();
            if let Some(column) = selected {
                bindings.push(Binding {
                    name: identifier.into(),
                    read: Read::Source(column),
                });
            } else {
                findings.push(BindFinding::UnknownReference {
                    path: path.clone(),
                    name: identifier.into(),
                });
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        let typed = plan.typed(|name| {
            crate::result_type::ResultType::declared(
                name.split_once('.')
                    .filter(|(dataset, _)| *dataset == driver)
                    .and_then(|(_, field)| {
                        source.columns().iter().find(|column| column.name == field)
                    })
                    .map(|column| column.kind),
            )
        });
        BoundPredicate::new(typed, bindings)
            .map(Some)
            .map_err(BindError::InvalidPredicateBinding)
    }
}
