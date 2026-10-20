//! Bounded ordered case lowering. Unselected branches have no runtime effects.
use super::*;
use crate::{dataset::CaseBranch, expression::Failure};

#[derive(Clone, Debug)]
pub(super) struct Branch {
    when: Option<std::boxed::Box<Result<CompiledExpression, InvalidPredicate>>>,
    pub(super) path: String,
    /// The document node of the operation, which the tree types.
    pub(super) value: usize,
    pub(super) operation: std::boxed::Box<Operation>,
}
impl Branch {
    pub(super) fn value(operation: Operation, path: String, value: usize) -> Self {
        Self {
            when: None,
            path,
            value,
            operation: std::boxed::Box::new(operation),
        }
    }
    pub(super) fn predicate(&self) -> Option<&Result<CompiledExpression, InvalidPredicate>> {
        self.when.as_deref()
    }
}
/// Keep the complete parser cause on the prepared branch; document reporting
/// projects it only after source ingestion reaches predicate binding.
#[derive(Clone, Debug)]
pub(super) struct InvalidPredicate {
    path: String,
    expression: String,
    position: usize,
    failure: Failure,
}
impl InvalidPredicate {
    pub(super) fn diagnostic(&self) -> Result<crate::diagnostic::Diagnostic, BindError> {
        Ok(self.failure.diagnostic(
            ExpressionContext::Predicate,
            &self.path,
            &self.expression,
            self.position,
        ))
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    d: &Document,
    node: usize,
    path: &str,
    project: Option<&functions::Prepared>,
    intermediates: &intermediates::Declarations,
    record: bool,
    depth: usize,
    extraction: &str_extract::Prepared,
    templates: &str_template::Prepared,
) -> Result<Vec<Branch>, PrepareError> {
    if depth > crate::limits::Limits::SEMANTIC_DEPTH {
        return Err(PrepareError::limit(
            "semantic_depth",
            crate::limits::Limits::SEMANTIC_DEPTH,
            Some(depth),
            Some(path),
        ));
    }

    let items = sequence(d, node)?;

    // The schema owns EXPR-7 shape failures. These branches defend the
    // normalized-document contract rather than declare a language subset.
    if items.is_empty() {
        return Err(PrepareError::Internal);
    }
    let mut branches = Vec::new();
    for (index, &item) in items.iter().enumerate() {
        let prefix = format!("{path}[{index}]");
        let (when, value, result_path) = if let Some(value) = d.field(item, "otherwise") {
            if index == 0
                || index + 1 != items.len()
                || d.field(item, "when").is_some()
                || d.field(item, "then").is_some()
            {
                return Err(PrepareError::Internal);
            }
            (None, value, format!("{prefix}.otherwise"))
        } else {
            let when = text(d, field(d, item, "when")?)?;
            let compiled = compile(
                when,
                ExpressionContext::Predicate,
                &format!("{prefix}.when"),
            );
            let compiled = match compiled {
                Ok(plan) => Ok(plan),
                Err(CompileError::Invalid {
                    position, failure, ..
                }) => Err(InvalidPredicate {
                    path: format!("{prefix}.when"),
                    expression: when.into(),
                    position: position.character,
                    failure,
                }),
                Err(error) => {
                    return Err(PrepareError::expression(
                        error,
                        Some(&format!("{prefix}.when")),
                    ))
                }
            };
            (
                Some(std::boxed::Box::new(compiled)),
                field(d, item, "then")?,
                format!("{prefix}.then"),
            )
        };
        let mut branch = derivations::prepare_value(
            d,
            value,
            &result_path,
            project,
            intermediates,
            record,
            depth,
            extraction,
            templates,
        )?;
        branch.when = when;
        branches.push(branch);
    }
    Ok(branches)
}

/// A width defect shares its derivation's authored declaration-validation order
/// with the expression defects of its predicates and formulas, before any
/// reference binding. Consumers without a width defect retain their existing
/// binding checkpoint.
pub(super) fn padding_findings(
    operation: &Operation,
    findings: &mut Vec<BindFinding>,
) -> Result<bool, BindError> {
    fn has_width_error(operation: &Operation) -> bool {
        match operation {
            Operation::StrPad(operation) => operation.has_width_error(),
            Operation::Flag(operation) => operation.has_findings(),
            Operation::Case(branches) => branches
                .iter()
                .any(|branch| has_width_error(&branch.operation)),
            _ => false,
        }
    }
    fn append(
        operation: &Operation,
        path: &str,
        findings: &mut Vec<BindFinding>,
    ) -> Result<(), BindError> {
        match operation {
            Operation::StrPad(operation) => str_pad::declaration_findings(operation, findings),
            Operation::Flag(operation) => operation.findings(findings),
            Operation::InvalidNumeric { expression, error } => {
                findings.push(BindFinding::Numeric {
                    path: format!("{path}.expr"),
                    expression: expression.clone(),
                    error: error.clone(),
                })
            }
            Operation::Case(branches) => {
                for branch in branches {
                    if let Some(Err(invalid)) = branch.when.as_deref() {
                        findings.push(BindFinding::Predicate(invalid.diagnostic()?));
                    }
                    append(&branch.operation, &branch.path, findings)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    if !has_width_error(operation) {
        return Ok(false);
    }
    // A top-level numeric declaration cannot also contain padding. Its path is
    // only needed for the branch-owned InvalidNumeric cases visited by append.
    append(operation, "", findings)?;
    Ok(true)
}

/// Ordered branches, each a predicate and a result bound in this scope. In
/// the implicit template a result's formula and `first_available` bind as
/// its case branches always have: a formula reads only lookups and columns,
/// and a source of `first_available` binds as a row template's would.
pub(super) fn bind(
    cx: &mut BindContext<'_, '_>,
    branches: &[Branch],
    column: usize,
) -> Result<Option<Expression>, BindError> {
    let before = cx.findings.len();
    let mut result = Vec::new();
    for branch in branches {
        let when = match branch.predicate() {
            Some(Err(invalid)) => {
                if !cx.reported {
                    cx.findings
                        .push(BindFinding::Predicate(invalid.diagnostic()?));
                }
                None
            }
            Some(Ok(plan)) => case_predicates::bind(cx, plan, before)?,
            None => None,
        };
        let implicit = cx.implicit().is_some();
        let expression = match branch.operation.as_ref() {
            Operation::Compute(compiled) if implicit => {
                cx.branch_formula(compiled, &branch.path)?
            }
            Operation::FirstAvailable(selection) if implicit => {
                cx.first_available(selection, &branch.path, column)?
            }
            operation => cx.bind(operation, &branch.path, column)?,
        };
        if let Some(expression) = expression {
            result.push(CaseBranch {
                when,
                assignment: std::boxed::Box::new(Assignment {
                    column,
                    expression,
                    path: branch.path.clone(),
                }),
            });
        }
    }
    Ok((cx.findings.len() == before).then_some(Expression::Case(result)))
}
