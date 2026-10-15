//! R020 CSV rendering over an already admitted typed table and projection.
//! Filesystem publication and output declaration diagnostics belong to the runner.
use yamaa_core::{
    conversion::float_text,
    csv_precision::fixed_point,
    limits::Limits,
    table::{CellError, TableAccess, ValueRef},
    value::ColumnType,
};

#[derive(Debug)]
pub enum Error<E> {
    Projection,
    Limit {
        resource: &'static str,
        limit: usize,
        actual: Option<usize>,
        decimal_formatting: bool,
    },
    Cell(CellError<E>),
    UnsupportedValue,
}
struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    fn write<E>(&mut self, text: &str) -> Result<(), Error<E>> {
        self.bytes
            .len()
            .checked_add(text.len())
            .filter(|&n| n <= Limits::OUTPUT_BYTES)
            .ok_or(Error::Limit {
                resource: "output_bytes",
                limit: Limits::OUTPUT_BYTES,
                actual: self.bytes.len().checked_add(text.len()),
                decimal_formatting: false,
            })?;
        self.bytes.extend_from_slice(text.as_bytes());
        Ok(())
    }
    fn field<E>(&mut self, text: &str) -> Result<(), Error<E>> {
        if text.is_empty()
            || text
                .bytes()
                .any(|b| matches!(b, b'"' | b',' | b'\r' | b'\n'))
        {
            self.write("\"")?;
            // Repeated quotes are streamed; no escaping-size allocation occurs.
            let mut start = 0;
            for (index, _) in text.match_indices('"') {
                self.write(&text[start..index])?;
                self.write("\"\"")?;
                start = index + 1;
            }
            self.write(&text[start..])?;
            self.write("\"")
        } else {
            self.write(text)
        }
    }
}
/// Missing and collected empty text remain distinct. Every record ends in LF;
/// no BOM, exponent spelling, whitespace normalization, or host CSV library.
/// Precision affects floats only. Its physical file width is checked before
/// formatting or asking the table for the next projected cell.
pub fn render<T: TableAccess>(
    table: &T,
    projection: &[usize],
    decimals: Option<&str>,
) -> Result<Vec<u8>, Error<T::Error>> {
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= table.schema().columns().len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    Limits::table(table.row_count(), projection.len()).map_err(|error| Error::Limit {
        resource: error.resource,
        limit: error.limit,
        actual: error.actual,
        decimal_formatting: false,
    })?;
    let mut writer = Writer { bytes: Vec::new() };
    for (index, &column) in projection.iter().enumerate() {
        if index > 0 {
            writer.write(",")?;
        }
        writer.field(&table.schema().columns()[column].name)?;
    }
    writer.write("\n")?;
    let decimals = if table.row_count() > 0
        && projection
            .iter()
            .any(|&c| table.schema().columns()[c].kind == ColumnType::Float)
    {
        decimals
            .map(|text| {
                text.parse::<usize>()
                    .ok()
                    .filter(|&n| {
                        if n == 0 {
                            writer.bytes.len() < Limits::OUTPUT_BYTES
                        } else {
                            n.checked_add(2).is_some_and(|width| {
                                width <= Limits::OUTPUT_BYTES - writer.bytes.len()
                            })
                        }
                    })
                    .ok_or(Error::Limit {
                        resource: "output_bytes",
                        limit: Limits::OUTPUT_BYTES,
                        actual: None,
                        decimal_formatting: true,
                    })
            })
            .transpose()?
    } else {
        None
    };
    for row in 0..table.row_count() {
        for (index, &column) in projection.iter().enumerate() {
            if index > 0 {
                writer.write(",")?;
            }
            if let Some(digits) = decimals {
                let minimum = if digits == 0 { 1 } else { digits + 2 };
                if table.schema().columns()[column].kind == ColumnType::Float
                    && minimum > Limits::OUTPUT_BYTES - writer.bytes.len()
                {
                    return Err(Error::Limit {
                        resource: "output_bytes",
                        limit: Limits::OUTPUT_BYTES,
                        actual: None,
                        decimal_formatting: true,
                    });
                }
            }
            match table.cell(row, column).map_err(Error::Cell)? {
                ValueRef::Missing => {}
                ValueRef::Str(text) => writer.field(text)?,
                ValueRef::Int(value) => writer.field(&value.to_string())?,
                ValueRef::Float(value) => {
                    let text = match decimals {
                        Some(digits) => {
                            fixed_point(value, digits, Limits::OUTPUT_BYTES - writer.bytes.len())
                                .map_err(|_| Error::Limit {
                                    resource: "output_bytes",
                                    limit: Limits::OUTPUT_BYTES,
                                    actual: None,
                                    decimal_formatting: true,
                                })?
                        }
                        None => float_text(value),
                    };
                    writer.field(&text)?;
                }
                ValueRef::Date(value) => writer.field(&value.to_string())?,
                ValueRef::DateTime(value) => writer.field(&value.to_string())?,
                ValueRef::Bool(_) => return Err(Error::UnsupportedValue),
            }
        }
        writer.write("\n")?;
    }
    Ok(writer.bytes)
}
