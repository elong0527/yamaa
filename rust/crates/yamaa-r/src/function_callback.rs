//! Plain R values for project function calls; all R access stays on the
//! calling thread.
//!
//! An argument becomes an ordinary R value: missing is `NA`, `str` a
//! character string, `int` and `float` a double, `bool` a logical, `date` a
//! `Date` and `datetime` a `POSIXct` in UTC. A value R cannot hold exactly is
//! refused, never changed. A result converts by the declared column type, and
//! R text is copied as checked UTF-8, never borrowed unchecked.
use extendr_api::prelude::*;
use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use yamaa::{ColumnType, Date, DateTime, Scalar, ScalarRef};

/// The largest magnitude up to which binary64 holds every integer exactly.
const EXACT_DOUBLE_MAGNITUDE: f64 = 9_007_199_254_740_992.0;

// R's C API for the bytes and declared encoding of one string; extendr's own
// string views assume valid UTF-8 without checking it. A `SEXP` is an opaque
// pointer, declared here as `void *`.
extern "C" {
    fn STRING_ELT(x: *mut c_void, i: isize) -> *mut c_void;
    fn R_CHAR(x: *mut c_void) -> *const c_char;
    fn Rf_xlength(x: *mut c_void) -> isize;
    fn Rf_getCharCE(x: *mut c_void) -> c_int;
}

/// The `cetype_t` encodings of R's `Rinternals.h` that need their own rule.
const CE_LATIN1: c_int = 2;
const CE_BYTES: c_int = 3;

/// Decode exact host detail bytes strictly.
/// Invalid encodings retain a stable fallback; raw invalid text is never repaired.
pub(super) fn detail(value: &Robj, fallback: &str) -> std::result::Result<String, String> {
    let bytes = value
        .as_raw()
        .ok_or_else(|| "invalid host detail".to_owned())?;
    let text = match std::str::from_utf8(bytes.as_slice()) {
        Ok(text) => text,
        Err(_) => return Ok(fallback.into()),
    };
    Ok(text.into())
}

/// Convert one engine argument to the plain R value a project function
/// receives, or say why R cannot hold it exactly.
pub(crate) fn argument(value: ScalarRef<'_>) -> std::result::Result<Robj, String> {
    Ok(match value {
        ScalarRef::Missing => Robj::from(Rbool::na()),
        ScalarRef::Str(text) => {
            if text.contains('\0') {
                return Err("text containing NUL cannot be held by an R character vector".into());
            }
            if i32::try_from(text.len()).is_err() {
                return Err("text longer than 2^31 - 1 bytes cannot be held by an R string".into());
            }
            Robj::from(text)
        }
        ScalarRef::Int(value) => Robj::from(crate::public_table::exact_double(value)?),
        ScalarRef::Float(value) => Robj::from(value),
        ScalarRef::Bool(value) => Robj::from(value),
        ScalarRef::Date(date) => {
            let mut value = Robj::from(f64::from(date.epoch_days()));
            value.set_class(["Date"]).map_err(|_| "date class")?;
            value
        }
        ScalarRef::DateTime(datetime) => {
            // Whole seconds of the civil years 1 to 9999 are exact in binary64.
            let mut value = Robj::from(datetime.epoch_seconds() as f64);
            value
                .set_class(["POSIXct", "POSIXt"])
                .map_err(|_| "datetime class")?;
            value
                .set_attrib("tzone", "UTC")
                .map_err(|_| "datetime timezone")?;
            value
        }
        // The facade enum is non-exhaustive; a new variant must not reach R
        // as some other value.
        _ => return Err("unsupported argument value".into()),
    })
}

/// Convert one plain R result to the declared column type. `NULL`, a
/// length-one `NA` and a non-finite double are missing; the engine still
/// checks whether the function may return missing. Anything else must be one
/// value of the declared type, or the reason it is not is returned.
pub(crate) fn result(value: &Robj, returns: ColumnType) -> std::result::Result<Scalar, String> {
    if value.is_null() || missing(value) {
        return Ok(Scalar::Missing);
    }
    let class = classes(value);
    match returns {
        ColumnType::Str => {
            if value.rtype() != Rtype::Strings || value.len() != 1 || class.is_some() {
                return Err(format!(
                    "a str result must be one character string, not {}",
                    describe(value, class.as_deref())
                ));
            }
            let text = text_at(value, 0)?;
            if text.contains('\0') {
                return Err("a str result cannot contain NUL".into());
            }
            Ok(Scalar::Str(text))
        }
        ColumnType::Int => {
            let number = plain_number(value, class.as_deref()).ok_or_else(|| {
                format!(
                    "an int result must be one number, not {}",
                    describe(value, class.as_deref())
                )
            })?;
            if number.fract() != 0.0 || number.abs() > EXACT_DOUBLE_MAGNITUDE {
                return Err(format!(
                    "an int result must be a whole number within 2^53 in magnitude, not {}",
                    shown(number)
                ));
            }
            Ok(Scalar::Int(number as i64))
        }
        ColumnType::Float => plain_number(value, class.as_deref())
            .map(Scalar::float)
            .ok_or_else(|| {
                format!(
                    "a float result must be one number, not {}",
                    describe(value, class.as_deref())
                )
            }),
        ColumnType::Date => {
            let days = number(value)
                .filter(|_| class_is(class.as_deref(), &["Date"]))
                .ok_or_else(|| {
                    format!(
                        "a date result must be one Date, not {}",
                        describe(value, class.as_deref())
                    )
                })?;
            if days.fract() != 0.0 {
                return Err(format!(
                    "a date result must be a whole number of days, not {}",
                    shown(days)
                ));
            }
            Date::from_epoch_days(days as i64)
                .map(Scalar::Date)
                .ok_or_else(|| {
                    format!(
                        "a date result must lie within the years 1 to 9999, not {} days from 1970-01-01",
                        shown(days)
                    )
                })
        }
        ColumnType::DateTime => {
            let seconds = number(value)
                .filter(|_| class_is(class.as_deref(), &["POSIXct", "POSIXt"]))
                .ok_or_else(|| {
                    format!(
                        "a datetime result must be one POSIXct, not {}",
                        describe(value, class.as_deref())
                    )
                })?;
            let zone = value.get_attrib("tzone");
            let utc = zone.as_ref().is_some_and(|zone| {
                zone.len() == 1 && text_at(zone, 0).is_ok_and(|zone| zone == "UTC")
            });
            if !utc {
                return Err(format!(
                    "a datetime result must be a POSIXct with tzone UTC, not {}",
                    match zone.as_ref().map(|zone| text_at(zone, 0)) {
                        Some(Ok(zone)) => format!("tzone {zone:?}"),
                        Some(Err(_)) => "an unreadable tzone".to_owned(),
                        None => "one without a tzone".to_owned(),
                    }
                ));
            }
            if seconds.fract() != 0.0 {
                return Err(format!(
                    "a datetime result must be a whole number of seconds, not {}",
                    shown(seconds)
                ));
            }
            DateTime::from_epoch_seconds(seconds as i64)
                .map(Scalar::DateTime)
                .ok_or_else(|| {
                    format!(
                        "a datetime result must lie within the years 1 to 9999, not {} seconds from 1970-01-01T00:00:00",
                        shown(seconds)
                    )
                })
        }
        // The facade enum is non-exhaustive; no other declared type exists.
        _ => Err("unsupported declared result type".into()),
    }
}

/// A length-one `NA` of a plain atomic type, or a non-finite double, which
/// the engine normalizes to missing before it checks the result.
fn missing(value: &Robj) -> bool {
    if value.len() != 1 {
        return false;
    }
    match value.rtype() {
        Rtype::Logicals => value
            .as_logical_slice()
            .is_some_and(|value| value[0].is_na()),
        Rtype::Integers => value
            .as_integer_slice()
            .is_some_and(|value| value[0].is_na()),
        Rtype::Doubles => value
            .as_real_slice()
            .is_some_and(|value| !value[0].is_finite()),
        Rtype::Strings => value.is_na(),
        _ => false,
    }
}

/// One integer or double value, whatever its class.
fn number(value: &Robj) -> Option<f64> {
    if value.len() != 1 {
        return None;
    }
    match value.rtype() {
        Rtype::Integers => value.as_integer_slice().map(|value| f64::from(value[0])),
        Rtype::Doubles => value.as_real_slice().map(|value| value[0]),
        _ => None,
    }
}

/// One unclassed integer or double value: a factor or another classed
/// number is not a plain number.
fn plain_number(value: &Robj, class: Option<&[String]>) -> Option<f64> {
    class.is_none().then(|| number(value)).flatten()
}

/// Whether the class attribute is exactly `expected`, in order.
fn class_is(class: Option<&[String]>, expected: &[&str]) -> bool {
    class.is_some_and(|class| {
        class
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
    })
}

/// The class attribute, or `None` for an unclassed value. A class name that
/// is not readable text is kept as empty, so it matches no expected class.
fn classes(value: &Robj) -> Option<Vec<String>> {
    let class = value.get_attrib(class_symbol())?;
    Some(
        (0..class.len())
            .map(|index| text_at(&class, index).unwrap_or_default())
            .collect(),
    )
}

/// Show a refused number in a message: plain decimal digits, or an exponent
/// once the digits would be unreadably long.
fn shown(value: f64) -> String {
    if value.abs() < 1e21 {
        value.to_string()
    } else {
        format!("{value:e}")
    }
}

/// Name a refused R value in a result message.
fn describe(value: &Robj, class: Option<&[String]>) -> String {
    if let Some(class) = class {
        let name = class.first().map(String::as_str).unwrap_or_default();
        return format!("an object of class {name:?}");
    }
    let kind = match value.rtype() {
        Rtype::Logicals => "a logical vector",
        Rtype::Integers => "an integer vector",
        Rtype::Doubles => "a double vector",
        Rtype::Strings => "a character vector",
        Rtype::Complexes => "a complex vector",
        Rtype::Raw => "a raw vector",
        Rtype::List => "a list",
        Rtype::Function | Rtype::Builtin | Rtype::Special => return "a function".into(),
        Rtype::Environment => return "an environment".into(),
        _ => return "an unsupported R object".into(),
    };
    format!("{kind} of length {}", value.len())
}

/// Copy one element of a character vector under the strict host text rules:
/// byte-marked text has no declared Unicode encoding, Latin-1 has one defined
/// decoding, and any other text must already be valid UTF-8. Nothing is
/// repaired.
fn text_at(value: &Robj, index: usize) -> std::result::Result<String, String> {
    if value.rtype() != Rtype::Strings || index >= value.len() {
        return Err("expected an R character value".into());
    }
    let index = isize::try_from(index).map_err(|_| "invalid R character index")?;
    // SAFETY: `value` is a live, protected character vector and `index` is in
    // bounds, so the element is a CHARSXP that stays reachable from `value`.
    // `R_CHAR` points at its `Rf_xlength` bytes, and nothing between reading
    // that pointer and copying the bytes below allocates on the R heap.
    let (encoding, bytes) = unsafe {
        let element = STRING_ELT(value.get().cast::<c_void>(), index);
        let length = usize::try_from(Rf_xlength(element)).map_err(|_| "invalid R text length")?;
        (
            Rf_getCharCE(element),
            std::slice::from_raw_parts(R_CHAR(element).cast::<u8>(), length),
        )
    };
    match encoding {
        CE_BYTES => Err("byte-marked R text has no declared Unicode encoding".into()),
        CE_LATIN1 => Ok(bytes.iter().copied().map(char::from).collect()),
        _ => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "R text is not valid UTF-8".into()),
    }
}
