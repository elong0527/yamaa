//! Original string-case declarations bind named operands without coercion.
use super::*;
use crate::text_case::{Case, StringCase};

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    _driver: Option<&str>,
    _intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<StringCase, PrepareError> {
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !["source", "to", "missing"].contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = text(d, field(d, payload, "source")?)?;
    let missing = d.field(payload, "missing");
    let mode = match text(d, field(d, payload, "to")?)? {
        "upper" => Case::Upper,
        "lower" => Case::Lower,
        "sentence" => Case::Sentence,
        "title" => Case::Title,
        // The original schema owns enum rejection, before compiler admission.
        _ => return Err(PrepareError::Internal),
    };
    StringCase::new(
        source.into(),
        path.into(),
        mode,
        missing
            .map(|id| literal(d, id, &format!("{path}.missing")))
            .transpose()?,
    )
    .ok_or(PrepareError::Internal)
}

/// `str_case` reads text (TEXT-1). With `rows` a bare name's phase is
/// binding's to decide, so its type is not checked here.
pub(super) fn operand_findings(
    d: &Document,
    payload: usize,
    path: &str,
    declared: &types::DeclaredTypes<'_>,
    rows: bool,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    let operation = prepare(d, payload, path, None, None, &mut Vec::new())?;
    let source = operation.source();
    let kind = (source.contains('.') || !rows)
        .then(|| declared.name(source).column_type())
        .flatten();
    if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Str) {
        findings.push(PreflightFinding::TextCase(
            operation.input_diagnostic(checks::type_name(kind)),
        ));
    }
    Ok(())
}

/// The source binds as the scope decides. Preflight checks the declared type
/// of every name without `rows` and of a qualified name with them, so a row
/// template's bare name is checked here, on its captured type (TEXT-1).
pub(super) fn bind(
    operation: &StringCase,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    let path = format!("{}.source", operation.path());
    let source = cx.operand(operation.source(), &path)?;
    if source.is_some() && cx.implicit().is_none() {
        if let Some(kind) = cx
            .catalog
            .type_of(operation.source())
            .filter(|kind| *kind != ColumnType::Str)
        {
            cx.findings.push(BindFinding::Scalar(
                operation.input_diagnostic(checks::type_name(kind)),
            ));
            return Ok(None);
        }
    }
    Ok(source.map(|expression| Expression::StrCase {
        source: std::boxed::Box::new(Assignment {
            column,
            path,
            expression,
        }),
        operation: operation.clone(),
    }))
}
