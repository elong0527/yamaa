//! Original string-case declarations bind named operands without coercion.
use super::*;
use crate::text_case::{self, Case, Delimiters};

/// The executor's case conversion for the verb at `e`; its source binds as
/// the scope decides.
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let operation = case(spec, e)?;
    Ok(Operation {
        operands: vec![Operand::name(
            operation.source(),
            format!("{}.source", operation.path()),
        )],
        kind: Kind::StrCase(operation),
    })
}

fn case(spec: &Spec, e: ExprId) -> Result<StringCase, Refusal> {
    let ExprKind::Scalar {
        op: ScalarOp::StrCase { to, delimiters },
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let path = path(spec, e);
    let mode = mode(*to, delimiters.as_deref(), &path)
        .map_err(|finding| Refusal::Findings(vec![finding]))?;
    StringCase::new(
        source.into(),
        path,
        mode,
        handler(spec, handlers.missing.as_ref())?,
    )
    .ok_or(Refusal::Internal)
}

/// The case `to` names, with the `delimiters` written beside it, of the
/// declaration at `path`: only `proper` takes a list, which names at least
/// one ASCII scalar, each once, and without one `proper` takes SAS
/// `PROPCASE`'s six (TEXT-5).
fn mode(to: ast::TextCase, delimiters: Option<&str>, path: &str) -> Result<Case, Diagnostic> {
    let mode = match to {
        ast::TextCase::Proper => {
            return match delimiters {
                None => Ok(Case::Proper(Delimiters::PROPCASE)),
                Some(list) => Delimiters::new(list)
                    .map(Case::Proper)
                    .ok_or_else(|| text_case::delimiters_invalid(path)),
            }
        }
        ast::TextCase::Upper => Case::Upper,
        ast::TextCase::Lower => Case::Lower,
        ast::TextCase::Sentence => Case::Sentence,
        ast::TextCase::Title => Case::Title,
    };
    match delimiters {
        None => Ok(mode),
        Some(_) => Err(text_case::delimiters_not_taken(path, to.name())),
    }
}

/// `str_case`'s declaration findings: its `delimiters` (TEXT-5), or else a
/// source that is not text (TEXT-1). With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here; binding checks a
/// row template's bare name on its captured type.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let operation = match case(spec, e) {
        Ok(operation) => operation,
        Err(Refusal::Findings(findings)) => return Ok(findings),
        Err(refusal) => return Err(refusal),
    };
    let source = operation.source();
    let kind = (source.contains('.') || !rows)
        .then(|| declared(source))
        .flatten();
    Ok(kind
        .filter(|kind| *kind != ColumnType::Str)
        .map(|kind| operation.input_diagnostic(kind.name()))
        .into_iter()
        .collect())
}
