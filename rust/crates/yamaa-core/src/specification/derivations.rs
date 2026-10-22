//! What preparation refuses in a derivation before any source is read, over
//! the tree: an operation outside the language, a formula beyond a compiler
//! limit, a written value its type cannot hold, and each verb's declaration
//! findings. Every derivation site admits its derivations here, in the
//! order preparation has always reported them; binding builds each
//! operation from the tree again.
use super::*;

fn unsupported(path: &str, operation: &str) -> PrepareError {
    PrepareError::Unsupported(vec![UnsupportedFeature {
        operation: operation.into(),
        path: path.into(),
    }])
}

/// What a derivation's admission reads besides the tree.
#[derive(Clone, Copy)]
pub(super) struct Admission<'a> {
    /// The project calls, each admitted with its definition.
    pub project: Option<&'a functions::Prepared>,
    /// The named intermediates a structured source may read, whose own
    /// records a filter does not select among.
    pub intermediates: &'a intermediates::Declarations,
}

impl Admission<'_> {
    /// Admit the derivation at `e`, nested `depth` operations deep.
    pub(super) fn admit(
        &self,
        spec: &ast::Spec,
        e: ExprId,
        depth: usize,
    ) -> Result<(), PrepareError> {
        let path = checks::path(spec, e);
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            let site = spec.paths.parent(spec.expr(e).meta.path);
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                site.map(|site| spec.render(site, spec.notation()))
                    .as_deref(),
            ));
        }
        match &spec.expr(e).kind {
            ExprKind::Odm(read) => {
                predicate_limits(spec, read.filter)?;
                odm::Declaration::from_tree(spec, e)?;
            }
            ExprKind::Aggregate(_) => aggregates::admit(spec, e, self, depth)?,
            ExprKind::Flag { .. } => {
                flags::Declaration::from_tree(spec, e)?;
            }
            ExprKind::Literal(value) => {
                constant(value, &path)?;
            }
            ExprKind::Call { .. } => {
                // The tree types the call by the return its definition declares.
                let project = self.project.ok_or(PrepareError::Internal)?;
                project.call(e).ok_or(PrepareError::Internal)?;
            }
            ExprKind::Case(arms) => cases::admit(spec, e, arms, self, depth + 1)?,
            ExprKind::Scalar { op, .. } => self.scalar(spec, e, op, depth)?,
            ExprKind::FirstAvailable { sources, missing } => {
                for &source in sources {
                    self.source(spec, source)?;
                }
                checks::handler(spec, missing.as_ref())?;
            }
            ExprKind::Window(_) => windows::admit(spec, e)?,
            ExprKind::Formula(formula) => match &formula.compiled {
                Ok(_) | Err(CompileError::Invalid { .. }) => {}
                Err(error) => {
                    return Err(PrepareError::Numeric {
                        path: checks::render(spec, formula.text.meta),
                        error: error.clone(),
                    })
                }
            },
            ExprKind::Ref(_) | ExprKind::Source(_) => self.source(spec, e)?,
            ExprKind::Unsupported(operation) => return Err(unsupported(&path, operation)),
        }
        Ok(())
    }

    /// A scalar verb: within the depth it adds for its operands, each source
    /// binding it reads admitted as any source is, then its operation built
    /// from the tree.
    fn scalar(
        &self,
        spec: &ast::Spec,
        e: ExprId,
        op: &ast::ScalarOp,
        depth: usize,
    ) -> Result<(), PrepareError> {
        let nested = depth + op.operand_depth();
        if nested > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(nested),
                None,
            ));
        }
        let ExprKind::Scalar { operands, .. } = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        for operand in operands {
            if let ExprKind::Source(_) = &spec.expr(operand.expr).kind {
                self.source(spec, operand.expr)?;
            }
        }
        crate::scalar_verb::Operation::from_tree(spec, e).map_err(refused_verb)?;
        Ok(())
    }

    /// A structured source: its filter within the compiler's limits unless
    /// it names a named intermediate's own record, and its written `absent`.
    pub(super) fn source(&self, spec: &ast::Spec, e: ExprId) -> Result<(), PrepareError> {
        self.source_filter(spec, e)?;
        source_expressions::Declaration::from_tree(spec, e)?;
        Ok(())
    }

    fn source_filter(&self, spec: &ast::Spec, e: ExprId) -> Result<(), PrepareError> {
        let ExprKind::Source(read) = &spec.expr(e).kind else {
            return Ok(());
        };
        let Some(variable) = checks::name(spec, read.variable) else {
            return Err(PrepareError::Internal);
        };
        if variable
            .split_once('.')
            .is_some_and(|(name, _)| !self.intermediates.contains(name))
        {
            predicate_limits(spec, read.filter)?;
        }
        Ok(())
    }
}

/// A predicate beyond the expression compiler's limits is refused at its
/// field; a grammar finding is binding's to report.
pub(super) fn predicate_limits(spec: &ast::Spec, e: Option<ExprId>) -> Result<(), PrepareError> {
    let Some(ExprKind::Formula(formula)) = e.map(|e| &spec.expr(e).kind) else {
        return Ok(());
    };
    match &formula.compiled {
        Ok(_) | Err(CompileError::Invalid { .. }) => Ok(()),
        Err(error) => Err(PrepareError::expression(
            error.clone(),
            Some(&checks::render(spec, formula.text.meta)),
        )),
    }
}

/// The declaration defects a derivation reports before any of its
/// references binds, in written order: each `case` predicate's grammar
/// finding, a flag's condition and its `false_value` without `missing`, a
/// formula's grammar finding, and a scalar verb's defect and the written
/// operands it does not take (a padding width, a whole number a temporal
/// verb counts written as a fraction).
pub(super) fn declaration_findings(
    spec: &ast::Spec,
    e: ExprId,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match &spec.expr(e).kind {
        ExprKind::Flag { .. } => flags::Declaration::from_tree(spec, e)
            .map_err(|_| BindError::Internal)?
            .findings(findings),
        ExprKind::Scalar { .. } => {
            let operation = crate::scalar_verb::Operation::from_tree(spec, e)
                .map_err(|_| BindError::Internal)?;
            findings.extend(operation.defect().cloned().map(BindFinding::Scalar));
            findings.extend(
                operation
                    .literal_findings()
                    .into_iter()
                    .map(BindFinding::Scalar),
            );
        }
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
                if let Some(invalid) = cases::invalid_predicate(spec, arm) {
                    findings.push(BindFinding::Predicate(invalid));
                }
                declaration_findings(spec, arm.result, findings)?;
            }
        }
        _ => (),
    }
    Ok(())
}
