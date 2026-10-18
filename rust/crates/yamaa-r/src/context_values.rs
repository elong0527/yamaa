//! Convert owned Rust structured values to R without a host JSON decoder.
//!
//! Objects are named lists (an empty object keeps its empty `names`), arrays
//! unnamed lists, text character and `null` is `NULL`. An integer token whose
//! magnitude is at most 2^53 becomes a double; every other integer token keeps
//! its exact decimal spelling as a character scalar, so binary64 never narrows
//! it. Fraction and exponent tokens are doubles. Text containing NUL is refused.
use extendr_api::prelude::*;
use yamaa::ContextValue;

/// The largest magnitude every integer below it holds exactly in binary64.
const EXACT_DOUBLE_MAGNITUDE: u128 = 1 << 53;

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
