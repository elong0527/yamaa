//! Convert owned Rust diagnostics to R records without a host JSON decoder.
//!
//! One record per diagnostic, built from the same report as the issues data
//! frame: `phase`, `condition`, `requirement`, `spec_paths`, `context`,
//! `source_span` and `operand_route`. Absent values are `NULL`, never a zero
//! or empty default: a present zero-width span is `list(start = 0, end = 0)`
//! and a present empty route is `character(0)`. Context numbers: an integer
//! token whose magnitude is at most 2^53 becomes a double; every other
//! integer token keeps its exact decimal spelling as a character scalar, so
//! binary64 never narrows it; fraction and exponent tokens are doubles.
//! Objects are named lists, arrays unnamed lists, and text containing NUL is
//! a `yamaa_utf8` scalar, as in output columns.
use extendr_api::prelude::*;
use yamaa::{ContextValue, Diagnostic};

/// The largest magnitude every integer below it holds exactly in binary64.
const EXACT_DOUBLE_MAGNITUDE: u128 = 1 << 53;

pub(super) fn records(diagnostics: &[Diagnostic]) -> std::result::Result<Robj, String> {
    let records = diagnostics
        .iter()
        .map(record)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(List::from_values(records).into_robj())
}

fn record(diagnostic: &Diagnostic) -> std::result::Result<Robj, String> {
    let context = diagnostic
        .context
        .iter()
        .map(|(key, value)| Ok((key.clone(), context_value(value)?)))
        .collect::<std::result::Result<Vec<_>, String>>()?;
    let requirement = match &diagnostic.requirement {
        Some(requirement) => r!(requirement.as_str()),
        None => NULL.into_robj(),
    };
    let source_span = match diagnostic.source_span {
        Some(span) => list!(start = span.start as f64, end = span.end as f64).into_robj(),
        None => NULL.into_robj(),
    };
    let operand_route = match &diagnostic.operand_route {
        Some(route) => Strings::from_values(route).into_robj(),
        None => NULL.into_robj(),
    };
    Ok(list!(
        phase = diagnostic.phase.as_str(),
        condition = diagnostic.code.as_str(),
        requirement = requirement,
        spec_paths = Strings::from_values(&diagnostic.spec_path),
        context = named_list(context)?,
        source_span = source_span,
        operand_route = operand_route
    )
    .into_robj())
}

/// A named list keeps its names attribute even when empty, so an empty
/// object stays distinct from an empty array.
fn named_list(pairs: Vec<(String, Robj)>) -> std::result::Result<Robj, String> {
    let (names, values): (Vec<String>, Vec<Robj>) = pairs.into_iter().unzip();
    let mut list = List::from_values(values).into_robj();
    list.set_attrib("names", Strings::from_values(names))
        .map_err(|_| "diagnostic context names")?;
    Ok(list)
}

pub(crate) fn context_value(value: &ContextValue) -> std::result::Result<Robj, String> {
    Ok(match value {
        ContextValue::Null => NULL.into_robj(),
        ContextValue::Bool(value) => r!(*value),
        ContextValue::Int(value) => integer(i128::from(*value), || value.to_string()),
        ContextValue::Uint(value) => integer(i128::from(*value), || value.to_string()),
        ContextValue::Wide(token) => r!(token.as_str()),
        ContextValue::Float(value) => r!(*value),
        ContextValue::Text(value) => text(value)?,
        ContextValue::List(values) => List::from_values(
            values
                .iter()
                .map(context_value)
                .collect::<std::result::Result<Vec<_>, _>>()?,
        )
        .into_robj(),
        ContextValue::Map(values) => named_list(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), context_value(value)?)))
                .collect::<std::result::Result<Vec<_>, String>>()?,
        )?,
        // The facade enum is non-exhaustive; uncovered variants cannot occur.
        _ => return Err("unsupported diagnostic context value".into()),
    })
}

fn integer(value: i128, token: impl FnOnce() -> String) -> Robj {
    if value.unsigned_abs() <= EXACT_DOUBLE_MAGNITUDE {
        r!(value as f64)
    } else {
        r!(token())
    }
}

fn text(value: &str) -> std::result::Result<Robj, String> {
    if value.contains('\0') {
        return Err("text containing NUL cannot be held by an R character vector".into());
    }
    Ok(r!(value))
}
