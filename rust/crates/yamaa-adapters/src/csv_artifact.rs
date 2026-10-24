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
    render_partitions(table, projection, decimals, &[0..table.row_count()]).map(|(bytes, _)| bytes)
}

/// [`render`] one partition of rows at a time (D264): the header once, then
/// the records of each of `partitions`, the consecutive ranges that divide
/// the table's rows in order. Every byte is row-local and CSV-9's display
/// precision is per value, so the file equals the whole-table render's
/// whatever the partitions. Each partition is admitted on its own, the file
/// is checked before it grows, and the end of each partition's bytes is
/// returned with them, the last at the file's length.
pub fn render_partitions<T: TableAccess>(
    table: &T,
    projection: &[usize],
    decimals: Option<&str>,
    partitions: &[std::ops::Range<usize>],
) -> Result<(Vec<u8>, Vec<usize>), Error<T::Error>> {
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= table.schema().columns().len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    let mut writer = Writer { bytes: Vec::new() };
    for (index, &column) in projection.iter().enumerate() {
        if index > 0 {
            writer.write(",")?;
        }
        writer.field(&table.schema().columns()[column].name)?;
    }
    writer.write("\n")?;
    let floats = projection
        .iter()
        .any(|&c| table.schema().columns()[c].kind == ColumnType::Float);
    // The display precision is read with the first record, as a whole-table
    // render reads it once the table holds one.
    let mut precision = None;
    let mut ends = Vec::with_capacity(partitions.len());
    for rows in partitions {
        Limits::table(rows.len(), projection.len()).map_err(|error| Error::Limit {
            resource: error.resource,
            limit: error.limit,
            actual: error.actual,
            decimal_formatting: false,
        })?;
        if !rows.is_empty() && precision.is_none() {
            precision = Some(if floats {
                writer.precision(decimals)?
            } else {
                None
            });
        }
        let decimals = precision.flatten();
        for row in rows.clone() {
            writer.record(table, projection, decimals, row)?;
        }
        ends.push(writer.bytes.len());
    }
    Ok((writer.bytes, ends))
}

impl Writer {
    /// The display precision `decimals` asks for, read once the table holds a
    /// record and a float column: a count of digits whose narrowest value
    /// still fits in the file.
    fn precision<E>(&self, decimals: Option<&str>) -> Result<Option<usize>, Error<E>> {
        decimals
            .map(|text| {
                text.parse::<usize>()
                    .ok()
                    .filter(|&n| {
                        if n == 0 {
                            self.bytes.len() < Limits::OUTPUT_BYTES
                        } else {
                            n.checked_add(2).is_some_and(|width| {
                                width <= Limits::OUTPUT_BYTES - self.bytes.len()
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
            .transpose()
    }

    /// Write the projected cells of `row`, then its LF.
    fn record<T: TableAccess>(
        &mut self,
        table: &T,
        projection: &[usize],
        decimals: Option<usize>,
        row: usize,
    ) -> Result<(), Error<T::Error>> {
        for (index, &column) in projection.iter().enumerate() {
            if index > 0 {
                self.write(",")?;
            }
            if let Some(digits) = decimals {
                let minimum = if digits == 0 { 1 } else { digits + 2 };
                if table.schema().columns()[column].kind == ColumnType::Float
                    && minimum > Limits::OUTPUT_BYTES - self.bytes.len()
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
                ValueRef::Str(text) => self.field(text)?,
                ValueRef::Int(value) => self.field(&value.to_string())?,
                ValueRef::Float(value) => {
                    let text = match decimals {
                        Some(digits) => {
                            fixed_point(value, digits, Limits::OUTPUT_BYTES - self.bytes.len())
                                .map_err(|_| Error::Limit {
                                    resource: "output_bytes",
                                    limit: Limits::OUTPUT_BYTES,
                                    actual: None,
                                    decimal_formatting: true,
                                })?
                        }
                        None => float_text(value),
                    };
                    self.field(&text)?;
                }
                ValueRef::Date(value) => self.field(&value.to_string())?,
                ValueRef::DateTime(value) => self.field(&value.to_string())?,
                ValueRef::Bool(_) => return Err(Error::UnsupportedValue),
            }
        }
        self.write("\n")
    }
}
