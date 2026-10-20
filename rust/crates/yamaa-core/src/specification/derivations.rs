//! What preparation refuses in a derivation before any source is read, over
//! the tree: an operation outside the language, a formula beyond a compiler
//! limit, a written value its type cannot hold, and each verb's declaration
//! findings. Every derivation site admits its derivations here, in the
//! order preparation has always reported them; binding builds each
//! operation from the tree again.
use super::*;
use crate::ast::{ScalarOp, Slot};

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

    fn scalar(
        &self,
        spec: &ast::Spec,
        e: ExprId,
        op: &ScalarOp,
        depth: usize,
    ) -> Result<(), PrepareError> {
        match op {
            ScalarOp::StrTemplate { .. } => {
                str_template::from_tree(spec, e)?;
            }
            ScalarOp::StrExtract { .. } | ScalarOp::StrReplace { .. } => {
                str_extract::from_tree(spec, e)?;
            }
            ScalarOp::Greatest | ScalarOp::Least => {
                if depth + 1 > crate::limits::Limits::SEMANTIC_DEPTH {
                    return Err(PrepareError::limit(
                        "semantic_depth",
                        crate::limits::Limits::SEMANTIC_DEPTH,
                        Some(depth + 1),
                        None,
                    ));
                }
                scalar::Declaration::from_tree(spec, e)?;
            }
            ScalarOp::Mapping { dict, .. } => {
                self.mapping_source(spec, e)?;
                mapping_cut::from_tree(spec, e)?;
                if let ast::Dictionary::File(file) = dict {
                    return Err(unsupported(
                        &checks::render(spec, file.meta),
                        "mapping_dictionary_resource",
                    ));
                }
            }
            ScalarOp::Cut { .. } => {
                self.mapping_source(spec, e)?;
                mapping_cut::from_tree(spec, e)?;
            }
            ScalarOp::StrCase { .. } => {
                str_case::from_tree(spec, e)?;
            }
            ScalarOp::StrPad { .. } => {
                str_pad::from_tree(spec, e)?;
            }
            ScalarOp::Impute { .. }
            | ScalarOp::Precision { .. }
            | ScalarOp::ImputationFlag { .. }
            | ScalarOp::ToDate => {
                imputation::from_tree(spec, e)?;
            }
            ScalarOp::Round { .. } => {
                rounding::from_tree(spec, e)?;
            }
            ScalarOp::Diff { .. }
            | ScalarOp::StudyDay
            | ScalarOp::ToEpochDay
            | ScalarOp::Shift { .. }
            | ScalarOp::DurationValue { .. }
            | ScalarOp::Dtc
            | ScalarOp::TimeOfDay => {
                temporal::from_tree(spec, e)?;
            }
        }
        Ok(())
    }

    /// A mapping's or cut's structured source, admitted as any source is.
    fn mapping_source(&self, spec: &ast::Spec, e: ExprId) -> Result<(), PrepareError> {
        let ExprKind::Scalar { operands, .. } = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        self.source_filter(
            spec,
            checks::operand(operands, Slot::Source).ok_or(PrepareError::Internal)?,
        )
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
/// formula's grammar finding, a padding width and a whole number a
/// temporal verb counts written as a fraction.
pub(super) fn declaration_findings(
    spec: &ast::Spec,
    e: ExprId,
    findings: &mut Vec<BindFinding>,
) -> Result<(), BindError> {
    match &spec.expr(e).kind {
        ExprKind::Flag { .. } => flags::Declaration::from_tree(spec, e)
            .map_err(|_| BindError::Internal)?
            .findings(findings),
        ExprKind::Scalar {
            op: ScalarOp::StrPad { .. },
            ..
        } => str_pad::declaration_findings(
            &str_pad::from_tree(spec, e).map_err(|_| BindError::Internal)?,
            findings,
        ),
        ExprKind::Scalar { op, .. } if checks::verbs::temporal(op) => {
            let operation = temporal::from_tree(spec, e).map_err(|_| BindError::Internal)?;
            for index in 0..operation.sources().len() {
                if matches!(
                    operation.literal(index),
                    Some(crate::value::Value::Float(_))
                ) && operation.expected(index) == "int"
                {
                    findings.push(BindFinding::Temporal(
                        operation.operand_diagnostic(index, "float"),
                    ));
                }
            }
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
