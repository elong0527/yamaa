//! Data frames to owned Arrow frames; all R text arrives as raw UTF-8.
use crate::api_entry::Refusal;
use crate::function_callback::inherits;
use arrow_array::{
    builder::StringBuilder, BooleanArray, NullArray, RecordBatchIterator, StructArray,
};
use extendr_api::prelude::*;
use std::{borrow::Cow, collections::BTreeMap, result::Result, sync::Arc};
use yamaa::{
    Array, Date32Array, Float64Array, Int64Array, Time32SecondArray, TimestampSecondArray,
};

fn list(value: &Robj, size: Option<usize>) -> Result<List, Refusal> {
    value
        .as_list()
        .filter(|value| size.is_none_or(|size| value.len() == size))
        .ok_or_else(|| "invalid input frame transport".into())
}
fn item(value: &List, index: usize) -> Result<Robj, Refusal> {
    value
        .elt(index)
        .map_err(|_| "invalid input frame transport".into())
}
fn text(value: &Robj) -> Result<String, Refusal> {
    let bytes = value.as_raw().ok_or("input text must be UTF-8 bytes")?;
    std::str::from_utf8(bytes.as_slice())
        .map(str::to_owned)
        .map_err(|_| "input text is not valid UTF-8".into())
}

pub(crate) fn import(
    specification: &str,
    inputs: &Robj,
) -> Result<BTreeMap<String, yamaa::InputFrame>, Refusal> {
    if inputs.is_null() {
        return Ok(BTreeMap::new());
    }
    let inputs = list(inputs, None)?;
    let mut frames = BTreeMap::new();
    for index in 0..inputs.len() {
        let entry = list(&item(&inputs, index)?, Some(2))?;
        let alias = text(&item(&entry, 0)?)?;
        if alias.is_empty() || frames.contains_key(&alias) {
            return Err("input aliases must be unique nonempty names".into());
        }
        let frame = list(&item(&entry, 1)?, Some(3))?;
        let names = list(&item(&frame, 0)?, None)?;
        let values = list(&item(&frame, 1)?, Some(names.len()))?;
        let rows = item(&frame, 2)?
            .as_real()
            .filter(|rows| {
                rows.is_finite() && *rows >= 0.0 && rows.fract() == 0.0 && *rows < usize::MAX as f64
            })
            .ok_or("invalid input frame row count")? as usize;
        let mut fields = Vec::with_capacity(names.len());
        let mut columns = Vec::with_capacity(names.len());
        for column in 0..names.len() {
            let name = text(&item(&names, column)?)?;
            let values = array(&name, &item(&values, column)?)?;
            fields.push(yamaa::Field::new(name, values.data_type().clone(), true));
            columns.push(values);
        }
        let batch = StructArray::try_new_with_length(fields.into(), columns, None, rows)
            .map(yamaa::RecordBatch::from)
            .map_err(|_| "input columns must have the same number of rows")?;
        let schema = batch.schema();
        let stream = yamaa::FFI_ArrowArrayStream::new(Box::new(RecordBatchIterator::new(
            vec![Ok(batch)],
            schema,
        )));
        let frame = yamaa::InputFrame::import(stream)
            .map_err(|error| error.at_input(specification, &alias))?;
        frames.insert(alias, frame);
    }
    Ok(frames)
}

fn array(name: &str, column: &Robj) -> Result<Arc<dyn Array>, Refusal> {
    let column = list(column, Some(2))?;
    let kind = text(&item(&column, 0)?)?;
    let value = item(&column, 1)?;
    if kind != "native" {
        return Err("unknown input column transport".into());
    }
    if inherits(&value, "factor") {
        return factor(name, &value);
    }
    if value.rtype() == Rtype::Strings {
        return strings(name, &value);
    }
    if inherits(&value, "Date") {
        let days = numbers(&value)?
            .into_iter()
            .map(|value| {
                value
                    .map(|value| {
                        if !value.is_finite()
                            || value.fract() != 0.0
                            || value < i32::MIN as f64
                            || value > i32::MAX as f64
                        {
                            return Err(Refusal::from(
                                "input Date values must be whole representable days",
                            ));
                        }
                        Ok(value as i32)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Arc::new(Date32Array::from(days)));
    }
    if inherits(&value, "POSIXct") {
        // POSIXct stores epoch seconds; its tzone controls display. In UTC
        // (or GMT) the epoch second is the clock time the user sees; in any
        // other zone, or the session's zone an empty or absent tzone means,
        // it is not, so the column is refused rather than shifted (#4882).
        utc_zone(name, &value)?;
        let seconds = numbers(&value)?
            .into_iter()
            .map(|value| {
                value
                    .map(|value| {
                        if !value.is_finite()
                            || value.fract() != 0.0
                            || value < i64::MIN as f64
                            || value >= i64::MAX as f64
                        {
                            return Err(Refusal::from(
                                "input POSIXct values must be whole representable seconds",
                            ));
                        }
                        Ok(value as i64)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Arc::new(TimestampSecondArray::from(seconds)));
    }
    if inherits(&value, "hms") {
        // An hms counts seconds since midnight; one of other units or past
        // one day is no time of day.
        if !crate::function_callback::seconds_unit(&value) {
            return Err("input hms values must count seconds".into());
        }
        let seconds = numbers(&value)?
            .into_iter()
            .map(|value| {
                value
                    .map(|value| {
                        if !value.is_finite()
                            || value.fract() != 0.0
                            || !(0.0..86_400.0).contains(&value)
                        {
                            return Err(Refusal::from(
                                "input hms values must be whole seconds within one day",
                            ));
                        }
                        Ok(value as i32)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Arc::new(Time32SecondArray::from(seconds)));
    }
    if value.class().is_some() {
        return Err("unsupported class on an input column".into());
    }
    if let Some(values) = value.as_integer_slice() {
        return Ok(Arc::new(Int64Array::from(
            values
                .iter()
                .map(|value| (!value.is_na()).then_some(i64::from(*value)))
                .collect::<Vec<_>>(),
        )));
    }
    if let Some(values) = value.as_real_slice() {
        return Ok(Arc::new(Float64Array::from(values.to_vec())));
    }
    if let Some(values) = value.as_logical_slice() {
        // A logical vector of NA alone, such as `data.frame(x = NA)` or an
        // empty column `read.csv` reads, holds no value: Arrow's null type,
        // which a declared type retypes. One that holds a value is refused.
        if values.iter().all(|value| value.is_na()) {
            return Ok(Arc::new(NullArray::new(values.len())));
        }
        return Ok(Arc::new(BooleanArray::from(
            values
                .iter()
                .map(|value| {
                    if value.is_na() {
                        None
                    } else {
                        Some(value.is_true())
                    }
                })
                .collect::<Vec<_>>(),
        )));
    }
    Err(
        "input columns must be character, factor, integer, numeric, Date, POSIXct or hms vectors"
            .into(),
    )
}
/// A character column, each element checked as strict UTF-8 in Rust with no
/// R-level loop (#4883); a refused element is named by row and bytes.
fn strings(name: &str, value: &Robj) -> Result<Arc<dyn Array>, Refusal> {
    let mut builder = StringBuilder::with_capacity(value.len(), 0);
    crate::function_callback::each_text(value, |text| builder.append_option(text))
        .map_err(|(index, bytes, reason)| refused_text(name, index, &bytes, reason))?;
    Ok(Arc::new(builder.finish()))
}

/// A factor column: each level is decoded on its first use and kept, and
/// every row takes its level's text, so an unused level is never read; an
/// `NA` code or an `NA` level is missing (#4883).
fn factor(name: &str, value: &Robj) -> Result<Arc<dyn Array>, Refusal> {
    let codes = value
        .as_integer_slice()
        .ok_or("input factor codes must be integers")?;
    let levels = value
        .get_attrib("levels")
        .filter(|levels| levels.rtype() == Rtype::Strings)
        .ok_or("input factor levels must be a character vector")?;
    // Outer `None`: not yet decoded; inner `None`: an `NA` level.
    let mut decoded: Vec<Option<Option<String>>> = vec![None; levels.len()];
    let mut builder = StringBuilder::with_capacity(codes.len(), 0);
    for (row, code) in codes.iter().enumerate() {
        if code.is_na() {
            builder.append_null();
            continue;
        }
        let level = usize::try_from(*code)
            .ok()
            .and_then(|code| code.checked_sub(1))
            .filter(|level| *level < levels.len())
            .ok_or("input factor codes must name a level")?;
        if decoded[level].is_none() {
            let text = crate::function_callback::optional_text_at(&levels, level)
                .map_err(|(bytes, reason)| refused_text(name, row, &bytes, reason))?;
            decoded[level] = Some(text.map(Cow::into_owned));
        }
        builder.append_option(decoded[level].as_ref().and_then(Option::as_deref));
    }
    Ok(Arc::new(builder.finish()))
}

/// The refusal of one input text value: its column, its 1-based row and its
/// held bytes, escaped, since they are not text the engine can hold.
fn refused_text(name: &str, index: usize, bytes: &[u8], reason: &str) -> Refusal {
    format!(
        "input column {name:?} row {} holds \"{}\", which is refused: {reason}",
        index + 1,
        bytes.escape_ascii()
    )
    .into()
}

/// Refuse a POSIXct column whose tzone is not UTC or GMT, naming the column
/// and its zone, with the conversion that keeps the clock time.
fn utc_zone(name: &str, value: &Robj) -> Result<(), Refusal> {
    crate::function_callback::utc_tzone(value).map_err(|zone| {
        format!(
            "input column {name:?} is a POSIXct in {zone}, not UTC or GMT; its clock times \
             would change, so convert it first, keeping them, with \
             lubridate::force_tz(x, \"UTC\")"
        )
        .into()
    })
}

fn numbers(value: &Robj) -> Result<Vec<Option<f64>>, Refusal> {
    if let Some(values) = value.as_real_slice() {
        return Ok(values
            .iter()
            .map(|value| (!value.is_nan()).then_some(*value))
            .collect());
    }
    if let Some(values) = value.as_integer_slice() {
        return Ok(values
            .iter()
            .map(|value| (!value.is_na()).then_some(f64::from(*value)))
            .collect());
    }
    Err("input temporal values must be numeric".into())
}
