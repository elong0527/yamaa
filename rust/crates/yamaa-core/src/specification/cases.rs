//! Bounded ordered `case`. Unselected branches have no runtime effects.
use super::*;
use crate::dataset::CaseBranch;

/// The grammar finding of an arm's `when`, which binding reports when it
/// reaches the arm.
pub(super) fn invalid_predicate(
    spec: &ast::Spec,
    arm: &ast::Arm,
) -> Option<crate::diagnostic::Diagnostic> {
    let ExprKind::Formula(formula) = &spec.expr(arm.condition?).kind else {
        return None;
    };
    match &formula.compiled {
        Err(CompileError::Invalid {
            position, failure, ..
        }) => Some(failure.diagnostic(
            ExpressionContext::Predicate,
            &checks::render(spec, formula.text.meta),
            &formula.text.text,
            position.character,
        )),
        _ => None,
    }
}

/// Admit the `case` at `e`, `depth` operations deep: each arm's `when`
/// within the compiler's limits, then its result.
pub(super) fn admit(
    spec: &ast::Spec,
    e: ExprId,
    arms: &[ast::Arm],
    admission: &derivations::Admission<'_>,
    depth: usize,
) -> Result<(), PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            Some(&checks::path(spec, e)),
        ));
    }
    // The schema owns EXPR-7 shape failures.
    if arms.is_empty() {
        return Err(PrepareError::Internal);
    }
    for arm in arms {
        derivations::predicate_limits(spec, arm.condition)?;
        admission.admit(spec, arm.result, depth)?;
    }
    Ok(())
}

/// A verb's declaration defect (a `str_pad` width) shares its derivation's
/// authored declaration-validation order with the expression defects of its
/// predicates and formulas, before any reference binding: when the
/// derivation at `e` holds one (or a flag's defect), its declaration findings
/// are reported now, and the binding that follows reports none of them
/// again. Other derivations keep their binding checkpoint.
pub(super) fn padding_findings(
    spec: &ast::Spec,
    e: ExprId,
    findings: &mut Vec<BindFinding>,
) -> Result<bool, BindError> {
    fn width_error(spec: &ast::Spec, e: ExprId) -> Result<bool, BindError> {
        Ok(match &spec.expr(e).kind {
            ExprKind::Scalar { .. } => crate::scalar_verb::Operation::from_tree(spec, e)
                .map_err(|_| BindError::Internal)?
                .defect()
                .is_some(),
            ExprKind::Flag { .. } => flags::Declaration::from_tree(spec, e)
                .map_err(|_| BindError::Internal)?
                .has_findings(),
            ExprKind::Case(arms) => {
                for arm in arms {
                    if width_error(spec, arm.result)? {
                        return Ok(true);
                    }
                }
                false
            }
            _ => false,
        })
    }
    fn append(
        spec: &ast::Spec,
        e: ExprId,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        match &spec.expr(e).kind {
            ExprKind::Scalar { .. } => findings.extend(
                crate::scalar_verb::Operation::from_tree(spec, e)
                    .map_err(|_| BindError::Internal)?
                    .defect()
                    .cloned()
                    .map(BindFinding::Scalar),
            ),
            ExprKind::Flag { .. } => flags::Declaration::from_tree(spec, e)
                .map_err(|_| BindError::Internal)?
                .findings(findings),
            ExprKind::Formula(formula) => {
                if let Err(error @ CompileError::Invalid { .. }) = &formula.compiled {
                    findings.push(BindFinding::Numeric {
                        path: checks::render(spec, formula.text.meta),
                        expression: formula.text.text.clone(),
                        error: error.clone(),
                    })
                }
            }
            ExprKind::Case(arms) => {
                for arm in arms {
                    if let Some(invalid) = invalid_predicate(spec, arm) {
                        findings.push(BindFinding::Predicate(invalid));
                    }
                    append(spec, arm.result, findings)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    if !width_error(spec, e)? {
        return Ok(false);
    }
    append(spec, e, findings)?;
    Ok(true)
}

/// Ordered branches, each a predicate and a result bound in this scope. In
/// the implicit template a result's formula and `first_available` bind as
/// its case branches always have: a formula reads only lookups and columns,
/// and a source of `first_available` binds as a row template's would.
pub(super) fn bind(
    cx: &mut BindContext<'_, '_>,
    arms: &[ast::Arm],
    column: usize,
) -> Result<Option<Expression>, BindError> {
    let spec = cx.spec();
    let before = cx.findings.len();
    let mut result = Vec::new();
    for arm in arms {
        let when = match arm.condition.map(|condition| &spec.expr(condition).kind) {
            None => None,
            Some(ExprKind::Formula(formula)) => match &formula.compiled {
                Ok(plan) => case_predicates::bind(cx, plan, before)?,
                Err(CompileError::Invalid { .. }) => {
                    if !cx.reported {
                        cx.findings.push(BindFinding::Predicate(
                            invalid_predicate(spec, arm).ok_or(BindError::Internal)?,
                        ));
                    }
                    None
                }
                Err(_) => return Err(BindError::Internal),
            },
            Some(_) => return Err(BindError::Internal),
        };
        let implicit = cx.implicit().is_some();
        let path = checks::path(spec, arm.result);
        let expression = match &spec.expr(arm.result).kind {
            ExprKind::Formula(ast::Formula {
                compiled: Ok(compiled),
                text,
                ..
            }) if implicit => cx.branch_formula(compiled, &checks::render(spec, text.meta))?,
            ExprKind::FirstAvailable { sources, missing } if implicit => {
                let selection =
                    source_expressions::FirstAvailable::from_tree(spec, sources, missing.as_ref())
                        .map_err(|_| BindError::Internal)?;
                cx.first_available(&selection, &path, column)?
            }
            _ => cx.bind(arm.result, column)?,
        };
        if let Some(expression) = expression {
            result.push(CaseBranch {
                when,
                assignment: std::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path,
                }),
            });
        }
    }
    Ok((cx.findings.len() == before).then_some(Expression::Case(result)))
}
