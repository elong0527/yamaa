//! Bounded R020 CSV rendering over an already admitted typed table and projection.
//! Filesystem publication and output declaration diagnostics belong to the runner.
use yamaa_core::{
    conversion::float_text,
    table::{CellError, TableAccess, ValueRef},
};

#[derive(Debug)]
pub enum Error<E> {
    Projection,
    Limit,
    Cell(CellError<E>),
    UnsupportedValue,
}
struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Writer {
    fn write<E>(&mut self, text: &str) -> Result<(), Error<E>> {
        self.bytes
            .len()
            .checked_add(text.len())
            .filter(|&n| n <= self.maximum)
            .ok_or(Error::Limit)?;
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
pub fn render<T: TableAccess>(
    table: &T,
    projection: &[usize],
    maximum: usize,
) -> Result<Vec<u8>, Error<T::Error>> {
    if projection.is_empty()
        || projection
            .iter()
            .enumerate()
            .any(|(i, &c)| c >= table.schema().columns().len() || projection[..i].contains(&c))
    {
        return Err(Error::Projection);
    }
    let mut writer = Writer {
        bytes: Vec::new(),
        maximum,
    };
    for (index, &column) in projection.iter().enumerate() {
        if index > 0 {
            writer.write(",")?;
        }
        writer.field(&table.schema().columns()[column].name)?;
    }
    writer.write("\n")?;
    for row in 0..table.row_count() {
        for (index, &column) in projection.iter().enumerate() {
            if index > 0 {
                writer.write(",")?;
            }
            match table.cell(row, column).map_err(Error::Cell)? {
                ValueRef::Missing => {}
                ValueRef::Str(text) => writer.field(text)?,
                ValueRef::Int(value) => writer.field(&value.to_string())?,
                ValueRef::Float(value) => writer.field(&float_text(value))?,
                ValueRef::Date(value) => writer.field(&value.to_string())?,
                ValueRef::DateTime(value) => writer.field(&value.to_string())?,
                ValueRef::Bool(_) => return Err(Error::UnsupportedValue),
            }
        }
        writer.write("\n")?;
    }
    Ok(writer.bytes)
}
