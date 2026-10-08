//! Copy shared typed columns into owned R vectors; no JSON or semantic parser.
use extendr_api::prelude::*;
use yamaa_adapters::public_table::{PublicTable, Values};

pub(super) fn frame(table: &PublicTable) -> std::result::Result<Robj, String> {
    let mut columns = Vec::new();
    for (name, values) in table.columns() {
        let mut value = match values {
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
        match values {
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
        columns.push((name.as_str(), value));
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
