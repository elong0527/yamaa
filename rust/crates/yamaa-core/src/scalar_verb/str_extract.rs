//! Whole-request text declaration admission, then reference-owned extraction binding.
//! `str_replace` shares the extraction declaration and its source binding; only
//! the bound expression differs.
use super::*;
use crate::text_extract::AdmissionError;

/// The `str_extract` or `str_replace` at `e`, checked before binding, with
/// whether a bare name it reads is left to binding (`rows`). Without `rows`
/// a declared column or stored field it reads must be text, and only then
/// is its pattern and group checked.
pub(super) fn preflight(
    spec: &Spec,
    e: ExprId,
    declared: &dyn Fn(&str) -> Option<ColumnType>,
    rows: bool,
) -> Result<Vec<Diagnostic>, Refusal> {
    let operation = from_tree(spec, e)?;
    let search = match &operation.kind {
        Kind::StrExtract(operation) => operation,
        Kind::StrReplace(operation) => operation.search(),
        _ => return Err(Refusal::Internal),
    };
    // Row availability/group scope belongs to binding. Declared columns
    // and stored fields have their declared types in the no-rows model.
    let kind = (!rows).then(|| declared(search.source())).flatten();
    Ok(match kind {
        Some(kind) if kind != ColumnType::Str => vec![search.input_diagnostic(kind.name())],
        Some(_) => search
            .declaration_diagnostic()
            .cloned()
            .into_iter()
            .collect(),
        None => Vec::new(),
    })
}

/// The executor's extraction or replacement for the verb at `e`. A pattern
/// beyond a regular-expression limit is refused at its own path; an invalid
/// one is the operation's deferred finding ([`Operation::late_refusal`]).
/// The source binds as the scope decides; an unknown source is the text
/// operation's own finding ([`Operation::unknown`]).
pub(super) fn from_tree(spec: &Spec, e: ExprId) -> Result<Operation, Refusal> {
    let ExprKind::Scalar {
        op,
        operands,
        handlers,
    } = &spec.expr(e).kind
    else {
        return Err(Refusal::Internal);
    };
    let source = operand_name(spec, operands, Slot::Source).ok_or(Refusal::Internal)?;
    let path = path(spec, e);
    let missing = handler(spec, handlers.missing.as_ref())?;
    let kind = match op {
        ScalarOp::StrExtract { pattern, group } => StringExtract::new(
            source.into(),
            path.clone(),
            pattern.clone(),
            group.clone(),
            missing,
            handler(spec, handlers.no_match.as_ref())?,
        )
        .map(Kind::StrExtract),
        ScalarOp::StrReplace {
            pattern,
            replacement,
        } => StringReplace::new(
            source.into(),
            path.clone(),
            pattern.clone(),
            replacement.clone(),
            missing,
        )
        .map(Kind::StrReplace),
        _ => return Err(Refusal::Internal),
    }
    .map_err(|error| admission_error(error, &path))?;
    Ok(Operation {
        operands: vec![Operand::name(source, format!("{path}.source"))],
        kind,
    })
}
fn admission_error(error: AdmissionError, path: &str) -> Refusal {
    match error {
        AdmissionError::Shape => Refusal::Internal,
        AdmissionError::Regex(crate::portable_regex::CompileError::Limit { resource, limit }) => {
            Refusal::Pattern {
                resource,
                limit,
                path: format!("{path}.pattern"),
            }
        }
        // An invalid pattern is a deferred finding, never an admission error.
        AdmissionError::Regex(crate::portable_regex::CompileError::Invalid { .. }) => {
            Refusal::Internal
        }
    }
}
