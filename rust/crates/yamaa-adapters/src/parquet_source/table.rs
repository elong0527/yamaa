//! Convert admitted storage arrays into the engine's precision-bearing table.
use super::reader::Decoded;
use crate::{
    column_table::{Column, ColumnTable, TableError},
    epoch::{date_from_days, datetime_from_seconds},
};
use arrow_array::{
    Array, Date32Array, Float64Array, Int64Array, LargeStringArray, StringArray,
    TimestampMicrosecondArray,
};
use yamaa_core::{temporal::Precision, value::ColumnType};

#[derive(Debug)]
pub(super) enum Error {
    Invalid,
    Representation,
    Table(TableError),
}

/// Preserve canonical scalar values at the decoded table boundary: a date
/// keeps day and a datetime second precision, and text keeps the empty
/// string apart from a missing value.
pub(super) fn convert(decoded: Decoded) -> Result<ColumnTable, Error> {
    let rows = admit(&decoded)?;
    build(decoded, rows)
}

/// Admit the decoded table's geometry within the fixed limits and each text
/// column's offsets, before any value converts: its records.
pub(super) fn admit(decoded: &Decoded) -> Result<usize, Error> {
    let columns = decoded.schema.columns().len();
    let mut rows = 0usize;
    for batch in &decoded.batches {
        rows = rows.checked_add(batch.num_rows()).ok_or(Error::Invalid)?;
        yamaa_core::limits::Limits::table(rows, columns).map_err(|error| {
            Error::Table(TableError::Limit {
                resource: error.resource,
                limit: error.limit,
                required: error.actual,
            })
        })?;
        for array in batch.columns() {
            if let Some(strings) = array.as_any().downcast_ref::<LargeStringArray>() {
                // Each decoded text column fits Arrow Utf8's offsets.
                if !crate::column_table::fits_utf8_offsets(
                    strings.iter().flatten().map(|text| text.len()),
                ) {
                    return Err(Error::Representation);
                }
            }
        }
    }
    Ok(rows)
}

/// Convert an admitted decoded table of `rows` records.
pub(super) fn build(decoded: Decoded, rows: usize) -> Result<ColumnTable, Error> {
    let mut values = decoded
        .schema
        .columns()
        .iter()
        .map(|field| Column::empty(field.kind))
        .collect::<Vec<_>>();
    for batch in &decoded.batches {
        for ((field, array), column) in decoded
            .schema
            .columns()
            .iter()
            .zip(batch.columns())
            .zip(&mut values)
        {
            fn cast<T: 'static>(array: &dyn Array) -> Result<&T, Error> {
                array.as_any().downcast_ref::<T>().ok_or(Error::Invalid)
            }
            let array = array.as_ref();
            let present = |row| !array.is_null(row);
            match (field.kind, column) {
                (ColumnType::Str, Column::Str(text)) => {
                    if let Some(strings) = array.as_any().downcast_ref::<StringArray>() {
                        for row in 0..array.len() {
                            text.push(present(row).then(|| strings.value(row)));
                        }
                    } else {
                        let strings = cast::<LargeStringArray>(array)?;
                        for row in 0..array.len() {
                            text.push(present(row).then(|| strings.value(row)));
                        }
                    }
                }
                (ColumnType::Int, Column::Int(integers)) => {
                    let array = cast::<Int64Array>(array)?;
                    for row in 0..array.len() {
                        integers.push(present(row).then(|| array.value(row)));
                    }
                }
                (ColumnType::Float, Column::Float(floats)) => {
                    let array = cast::<Float64Array>(array)?;
                    for row in 0..array.len() {
                        floats.push(present(row).then(|| array.value(row)));
                    }
                }
                (ColumnType::Date, Column::Date(dates)) => {
                    let days = cast::<Date32Array>(array)?;
                    for row in 0..array.len() {
                        dates.push(if present(row) {
                            let day = i64::from(days.value(row));
                            Some(date_from_days(day, Precision::Day).ok_or(Error::Invalid)?)
                        } else {
                            None
                        });
                    }
                }
                (ColumnType::DateTime, Column::DateTime(datetimes)) => {
                    let micros = cast::<TimestampMicrosecondArray>(array)?;
                    for row in 0..array.len() {
                        datetimes.push(if present(row) {
                            let micros = micros.value(row);
                            if micros % 1_000_000 != 0 {
                                return Err(Error::Invalid);
                            }
                            Some(
                                datetime_from_seconds(micros / 1_000_000, Precision::Second)
                                    .ok_or(Error::Invalid)?,
                            )
                        } else {
                            None
                        });
                    }
                }
                _ => return Err(Error::Invalid),
            }
        }
    }
    ColumnTable::new(decoded.schema, values, rows).map_err(Error::Table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{ArrayRef, RecordBatch};
    use arrow_schema::{Field, Schema, TimeUnit};
    use std::sync::Arc;
    use yamaa_core::table::{Column, TableAccess, TableSchema, ValueRef};

    fn source() -> Decoded {
        let schema = TableSchema::new(
            [
                ("S", ColumnType::Str),
                ("I", ColumnType::Int),
                ("F", ColumnType::Float),
                ("D", ColumnType::Date),
                ("T", ColumnType::DateTime),
            ]
            .into_iter()
            .map(|(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
        )
        .unwrap();
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(LargeStringArray::from(vec![Some(""), None, Some("β")])),
            Arc::new(Int64Array::from(vec![Some(i64::MIN), None, Some(i64::MAX)])),
            Arc::new(Float64Array::from(vec![
                Some(-0.0),
                None,
                Some(f64::from_bits(1)),
            ])),
            Arc::new(Date32Array::from(vec![
                Some(-719_162),
                None,
                Some(2_932_896),
            ])),
            Arc::new(TimestampMicrosecondArray::from(vec![
                Some(-1_000_000),
                None,
                Some(253_402_300_799_000_000),
            ])),
        ];
        let stored = Arc::new(Schema::new(vec![
            Field::new("S", arrow_schema::DataType::LargeUtf8, true),
            Field::new("I", arrow_schema::DataType::Int64, true),
            Field::new("F", arrow_schema::DataType::Float64, true),
            Field::new("D", arrow_schema::DataType::Date32, true),
            Field::new(
                "T",
                arrow_schema::DataType::Timestamp(TimeUnit::Microsecond, None),
                true,
            ),
        ]));
        Decoded {
            schema,
            batches: vec![RecordBatch::try_new(stored, arrays).unwrap()],
        }
    }

    #[test]
    fn canonical_snapshot_keeps_empty_null_exact_bits_and_complete_civil_precision() {
        let table = convert(source()).unwrap();
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""));
        assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str("β"));
        for column in 0..5 {
            assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing);
        }
        assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN));
        assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX));
        for (row, bits) in [(0, (-0.0f64).to_bits()), (2, 1)] {
            let ValueRef::Float(value) = table.cell(row, 2).unwrap() else {
                panic!()
            };
            assert_eq!(value.get().to_bits(), bits);
        }
        for (row, text) in [(0, "0001-01-01"), (2, "9999-12-31")] {
            let ValueRef::Date(value) = table.cell(row, 3).unwrap() else {
                panic!()
            };
            assert_eq!(value.to_string(), text);
            assert_eq!(value.collected_precision(), Precision::Day);
        }
        for (row, fields) in [
            (0, (1969, 12, 31, 23, 59, 59)),
            (2, (9999, 12, 31, 23, 59, 59)),
        ] {
            let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                panic!()
            };
            assert_eq!(value.fields(), fields);
            assert_eq!(value.collected_precision(), Precision::Second);
        }
    }
}
