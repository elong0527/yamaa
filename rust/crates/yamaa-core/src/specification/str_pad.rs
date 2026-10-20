//! Original scalar padding shares named-reference and phase binding authority.
use super::*;
use crate::text_pad::{DeclarationError, StringPad};
#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Valid(StringPad),
    WidthError { source: String, path: String },
}
impl Declaration {
    pub(super) fn has_width_error(&self) -> bool {
        matches!(self, Self::WidthError { .. })
    }
    pub(super) fn source(&self) -> &str {
        match self {
            Self::Valid(pad) => pad.source(),
            Self::WidthError { source, .. } => source,
        }
    }
    fn path(&self) -> &str {
        match self {
            Self::Valid(pad) => pad.path(),
            Self::WidthError { path, .. } => path,
        }
    }
    fn diagnostic(&self) -> Option<crate::diagnostic::Diagnostic> {
        match self {
            Self::Valid(_) => None,
            Self::WidthError { path, .. } => DeclarationError::Width.diagnostic(path),
        }
    }
}

/// The executor's padding for the verb at `e`, or its width defect.
pub(super) fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Declaration, PrepareError> {
    let ExprKind::Scalar {
        op: ast::ScalarOp::StrPad { width },
        operands,
        ..
    } = &spec.expr(e).kind
    else {
        return Err(PrepareError::Internal);
    };
    let source =
        checks::operand_name(spec, operands, ast::Slot::Source).ok_or(PrepareError::Internal)?;
    let path = checks::path(spec, e);
    match StringPad::new(source, &path, width) {
        Ok(pad) => Ok(Declaration::Valid(pad)),
        Err(DeclarationError::Width) => Ok(Declaration::WidthError {
            source: source.into(),
            path,
        }),
        Err(DeclarationError::Shape) => Err(PrepareError::Internal),
    }
}

/// Report the operation's width defect at its enclosing derivation checkpoint,
/// before that derivation binds any predicate or source references. The complete
/// occurrence metadata was admitted before this declaration was owned.
pub(super) fn declaration_findings(operation: &Declaration, findings: &mut Vec<BindFinding>) {
    if let Some(diagnostic) = operation.diagnostic() {
        findings.push(BindFinding::Padding(diagnostic));
    }
}

/// The source binds as the scope decides; a width defect, reported with the
/// derivation's declaration defects, leaves nothing to wrap.
pub(super) fn bind(
    operation: &Declaration,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", operation.path());
    let bound = cx.operand(operation.source(), &path)?;
    if operation.has_width_error() {
        return Ok(None);
    }
    let Declaration::Valid(operation) = operation else {
        return Err(BindError::Internal);
    };
    Ok(bound.map(|expression| Expression::StrPad {
        source: std::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
