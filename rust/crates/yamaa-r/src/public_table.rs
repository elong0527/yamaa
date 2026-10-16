//! Explicit copying view of shared Arrow columns into exact owned R vectors.
use extendr_api::prelude::*;
use yamaa::{
    Array, DataType, Date32Array, Float64Array, Int64Array, LargeStringArray, StringArray,
    StringViewArray, Table, TimeUnit, TimestampSecondArray,
};

enum Values {
    Str(Vec<Option<String>>),
    Int(Vec<Option<i64>>),
    Float(Vec<Option<f64>>),
    Date(Vec<Option<i32>>),
    DateTime(Vec<Option<i64>>),
}

pub(super) fn frame(table: &Table) -> std::result::Result<Robj, String> {
    let mut columns = Vec::new();
    for (index, field) in table.columns().iter().enumerate() {
        macro_rules! collect {
            ($kind:ty) => {{
                let mut values = Vec::with_capacity(table.rows());
                for batch in table.batches() {
                    let array = batch
                        .column(index)
                        .as_any()
                        .downcast_ref::<$kind>()
                        .ok_or("public Arrow column type")?;
                    values.extend(array.iter());
                }
                values
            }};
        }
        macro_rules! strings {
            ($kind:ty) => {{
                let mut values = Vec::with_capacity(table.rows());
                for batch in table.batches() {
                    let array = batch
                        .column(index)
                        .as_any()
                        .downcast_ref::<$kind>()
                        .ok_or("public Arrow string type")?;
                    values.extend(array.iter().map(|value| value.map(str::to_owned)));
                }
                values
            }};
        }
        let values = match field.data_type() {
            DataType::Utf8 => Values::Str(strings!(StringArray)),
            DataType::LargeUtf8 => Values::Str(strings!(LargeStringArray)),
            DataType::Utf8View => Values::Str(strings!(StringViewArray)),
            DataType::Int64 => Values::Int(collect!(Int64Array)),
            DataType::Float64 => Values::Float(collect!(Float64Array)),
            DataType::Date32 => Values::Date(collect!(Date32Array)),
            DataType::Timestamp(TimeUnit::Second, None) => {
                Values::DateTime(collect!(TimestampSecondArray))
            }
            _ => return Err("unsupported public Arrow column type".into()),
        };
        let mut value = match &values {
            Values::Str(values) if values.iter().flatten().any(|v| v.contains('\0')) => {
                let mut list = List::from_values(values.iter().map(|v| match v {
                    None => NULL.into_robj(),
                    Some(v) => Raw::from_bytes(v.as_bytes()).into_robj(),
                }))
                .into_robj();
                list.set_class(["yamaa_utf8_vector"])
                    .map_err(|_| "text vector class")?;
                list
            }
            Values::Str(values) => {
                Strings::from_values(values.iter().map(|v| v.as_deref().unwrap_or(<&str>::na())))
                    .into_robj()
            }
            Values::Int(values) => {
                let owned: Vec<Option<String>> =
                    values.iter().map(|v| v.map(|v| v.to_string())).collect();
                let mut value = Strings::from_values(
                    owned.iter().map(|v| v.as_deref().unwrap_or(<&str>::na())),
                )
                .into_robj();
                value
                    .set_class(["yamaa_int64_vector"])
                    .map_err(|_| "integer vector class")?;
                value
            }
            Values::Float(values) => Doubles::from_values(
                values
                    .iter()
                    .map(|v| v.map(Rfloat::from).unwrap_or(Rfloat::na())),
            )
            .into_robj(),
            Values::Date(values) => Doubles::from_values(values.iter().map(|v| {
                v.map(|v| Rfloat::from(f64::from(v)))
                    .unwrap_or(Rfloat::na())
            }))
            .into_robj(),
            Values::DateTime(values) => Doubles::from_values(
                values
                    .iter()
                    .map(|v| v.map(|v| Rfloat::from(v as f64)).unwrap_or(Rfloat::na())),
            )
            .into_robj(),
        };
        match &values {
            Values::Date(_) => {
                value.set_class(["Date"]).map_err(|_| "date vector class")?;
            }
            Values::DateTime(_) => {
                value
                    .set_class(["POSIXct", "POSIXt"])
                    .map_err(|_| "datetime vector class")?;
                value
                    .set_attrib("tzone", "UTC")
                    .map_err(|_| "datetime timezone")?;
            }
            _ => {}
        }
        columns.push((field.name().as_str(), value));
    }
    let mut result = List::from_pairs(columns).into_robj();
    result
        .set_class(["data.frame"])
        .map_err(|_| "output data-frame class")?;
    let rows = i32::try_from(table.rows()).map_err(|_| "output row limit")?;
    result
        .set_attrib(
            "row.names",
            if rows == 0 {
                Integers::new(0)
            } else {
                Integers::from_values([Rint::na(), Rint::from(-rows)])
            },
        )
        .map_err(|_| "output row names")?;
    Ok(result)
}
