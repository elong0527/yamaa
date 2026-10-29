//! Data frames to owned Arrow frames; all R text arrives as raw UTF-8.
use crate::api_entry::Refusal;
use arrow_array::{BooleanArray, RecordBatchIterator, StructArray};
use extendr_api::prelude::*;
use std::{collections::BTreeMap, result::Result, sync::Arc};
use yamaa::{Array, Date32Array, Float64Array, Int64Array, StringArray, TimestampSecondArray};

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
            let values = array(&item(&values, column)?)?;
            fields.push(yamaa::Field::new(
                text(&item(&names, column)?)?,
                values.data_type().clone(),
                true,
            ));
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

fn array(column: &Robj) -> Result<Arc<dyn Array>, Refusal> {
    let column = list(column, Some(2))?;
    let kind = text(&item(&column, 0)?)?;
    let value = item(&column, 1)?;
    if kind == "str" {
        let values = list(&value, None)?;
        let mut strings = Vec::with_capacity(values.len());
        for index in 0..values.len() {
            let value = item(&values, index)?;
            strings.push(if value.is_null() {
                None
            } else {
                Some(text(&value)?)
            });
        }
        return Ok(Arc::new(StringArray::from(strings)));
    }
    if kind != "native" {
        return Err("unknown input column transport".into());
    }
    if value.inherits("Date") {
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
    if value.inherits("POSIXct") {
        // POSIXct stores epoch seconds; its timezone controls display. Keep
        // the instant as a whole UTC second, without truncating fractions.
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
    Err("input columns must be character, factor, integer, numeric, Date or POSIXct vectors".into())
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
