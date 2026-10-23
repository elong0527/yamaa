//! Copy an engine table's shared Arrow columns, read from a fresh stream,
//! into plain owned R vectors.
//!
//! Every value either keeps its exact value in R or the conversion fails:
//! integers become doubles up to 2^53 in magnitude, text becomes character
//! unless it contains NUL, dates become `Date` and zone-free datetimes
//! `POSIXct` in UTC. List columns, which only engine-built tables such as
//! issues carry, become lists of such vectors, with `NULL` for a missing list.
use arrow_array::ffi_stream::{ArrowArrayStreamReader, FFI_ArrowArrayStream};
use arrow_array::{
    Array, Date32Array, Float64Array, Int64Array, LargeStringArray, ListArray, RecordBatch,
    RecordBatchReader, StringArray, StringViewArray, TimestampSecondArray,
};
use arrow_schema::{DataType, TimeUnit};
use extendr_api::prelude::*;
use yamaa_boundary::client::{Owned, Refused};

/// The largest magnitude every integer below it holds exactly in binary64.
const EXACT_DOUBLE_MAGNITUDE: u64 = 1 << 53;

enum Column {
    Text(Vec<Option<String>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    Date(Vec<Option<i32>>),
    DateTime(Vec<Option<i64>>),
    List(Vec<Option<Robj>>),
}

/// The data frame of an engine table: its columns read whole from a fresh
/// stream, which a failed conversion releases with every batch it read.
pub(super) fn frame(table: &Owned) -> std::result::Result<Robj, String> {
    let stream = table.engine().export(table.handle());
    let mut stream = stream
        .map_err(|refused| match refused {
            Refused::Engine { message, .. } | Refused::Boundary(message) => message,
            Refused::Cancelled => "the call was cancelled".to_owned(),
        })?
        .into_raw();
    // SAFETY: the engine's stream has the C stream interface's layout;
    // `from_raw` moves it out and leaves the source released.
    let stream = unsafe { FFI_ArrowArrayStream::from_raw((&raw mut stream).cast()) };
    let reader = ArrowArrayStreamReader::try_new(stream).map_err(|error| error.to_string())?;
    let schema = reader.schema();
    let batches = reader
        .collect::<std::result::Result<Vec<RecordBatch>, _>>()
        .map_err(|error| error.to_string())?;
    let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
    let mut columns = Vec::with_capacity(schema.fields().len());
    for (index, field) in schema.fields().iter().enumerate() {
        let mut column = empty(field.data_type(), rows)?;
        for batch in &batches {
            extend(&mut column, batch.column(index).as_ref())?;
        }
        columns.push((field.name().as_str(), finish(column)?));
    }
    let mut result = List::from_pairs(columns).into_robj();
    result
        .set_class(["data.frame"])
        .map_err(|_| "data-frame class")?;
    let rows = i32::try_from(rows).map_err(|_| "R data frames hold at most 2^31 - 1 rows")?;
    result
        .set_attrib(
            "row.names",
            if rows == 0 {
                Integers::new(0)
            } else {
                Integers::from_values([Rint::na(), Rint::from(-rows)])
            },
        )
        .map_err(|_| "data-frame row names")?;
    Ok(result)
}

fn empty(data_type: &DataType, rows: usize) -> std::result::Result<Column, String> {
    Ok(match data_type {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            Column::Text(Vec::with_capacity(rows))
        }
        DataType::Int64 => Column::Int(Vec::with_capacity(rows)),
        DataType::Float64 => Column::Float(Vec::with_capacity(rows)),
        DataType::Date32 => Column::Date(Vec::with_capacity(rows)),
        DataType::Timestamp(TimeUnit::Second, None) => Column::DateTime(Vec::with_capacity(rows)),
        DataType::List(_) => Column::List(Vec::with_capacity(rows)),
        other => return Err(format!("unsupported Arrow column type {other}")),
    })
}

fn extend(column: &mut Column, array: &dyn Array) -> std::result::Result<(), String> {
    fn cast<T: 'static>(array: &dyn Array) -> std::result::Result<&T, String> {
        array
            .as_any()
            .downcast_ref::<T>()
            .ok_or_else(|| format!("Arrow array does not match its {} field", array.data_type()))
    }
    match column {
        Column::Text(values) => match array.data_type() {
            DataType::Utf8 => values.extend(
                cast::<StringArray>(array)?
                    .iter()
                    .map(|v| v.map(str::to_owned)),
            ),
            DataType::LargeUtf8 => values.extend(
                cast::<LargeStringArray>(array)?
                    .iter()
                    .map(|v| v.map(str::to_owned)),
            ),
            _ => values.extend(
                cast::<StringViewArray>(array)?
                    .iter()
                    .map(|v| v.map(str::to_owned)),
            ),
        },
        Column::Int(values) => values.extend(cast::<Int64Array>(array)?.iter()),
        Column::Float(values) => values.extend(cast::<Float64Array>(array)?.iter()),
        Column::Date(values) => values.extend(cast::<Date32Array>(array)?.iter()),
        Column::DateTime(values) => values.extend(cast::<TimestampSecondArray>(array)?.iter()),
        Column::List(values) => {
            let list = cast::<ListArray>(array)?;
            for row in 0..list.len() {
                values.push(if list.is_null(row) {
                    None
                } else {
                    let items = list.value(row);
                    let mut column = empty(items.data_type(), items.len())?;
                    extend(&mut column, items.as_ref())?;
                    Some(finish(column)?)
                });
            }
        }
    }
    Ok(())
}

fn finish(column: Column) -> std::result::Result<Robj, String> {
    let doubles = |values: Vec<Option<f64>>| {
        Doubles::from_values(
            values
                .into_iter()
                .map(|v| v.map(Rfloat::from).unwrap_or(Rfloat::na())),
        )
        .into_robj()
    };
    Ok(match column {
        Column::Text(values) => {
            if values.iter().flatten().any(|v| v.contains('\0')) {
                return Err("text containing NUL cannot be held by an R character vector".into());
            }
            Strings::from_values(values.iter().map(|v| v.as_deref().unwrap_or(<&str>::na())))
                .into_robj()
        }
        Column::Int(values) => doubles(
            values
                .into_iter()
                .map(|v| v.map(exact_double).transpose())
                .collect::<std::result::Result<_, _>>()?,
        ),
        Column::Float(values) => doubles(values),
        Column::Date(values) => {
            let mut value = doubles(values.into_iter().map(|v| v.map(f64::from)).collect());
            value.set_class(["Date"]).map_err(|_| "date class")?;
            value
        }
        Column::DateTime(values) => {
            // Zone-free seconds are at most about 2^38 in magnitude for the
            // admitted civil range, so every value is exact in binary64.
            let mut value = doubles(values.into_iter().map(|v| v.map(|v| v as f64)).collect());
            value
                .set_class(["POSIXct", "POSIXt"])
                .map_err(|_| "datetime class")?;
            value
                .set_attrib("tzone", "UTC")
                .map_err(|_| "datetime timezone")?;
            value
        }
        Column::List(values) => List::from_values(
            values
                .into_iter()
                .map(|v| v.unwrap_or_else(|| NULL.into_robj())),
        )
        .into_robj(),
    })
}

pub(crate) fn exact_double(value: i64) -> std::result::Result<f64, String> {
    if value.unsigned_abs() <= EXACT_DOUBLE_MAGNITUDE {
        Ok(value as f64)
    } else {
        Err(format!(
            "integer {value} is beyond 2^53 and cannot be held exactly by an R double"
        ))
    }
}
