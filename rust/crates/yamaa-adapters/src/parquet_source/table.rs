//! Convert admitted storage arrays into the engine's precision-bearing snapshot.
use super::reader::Decoded;
use crate::{
    arrow_table::{
        date_array, datetime_array, physical_schema, ArrowTable, TableError, TableLimits,
    },
    arrow_temporal::{date_from_days, datetime_from_seconds},
};
use arrow_array::{
    Array, ArrayRef, Date32Array, LargeStringArray, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use std::sync::Arc;
use yamaa_core::{
    temporal::{DatePrecision, DateTimePrecision},
    value::ColumnType,
};

#[derive(Debug)]
pub(super) enum Error {
    Limit,
    Invalid,
    Table(TableError),
}

/// The staging charge bounds admitted cells and copied string payloads. It is
/// not an allocator/RSS limit; the caller also bounds source and decoded arrays.
pub(super) fn convert(
    decoded: Decoded,
    staged_bytes: usize,
    limits: TableLimits,
) -> Result<ArrowTable, Error> {
    let mut rows = 0usize;
    let columns = decoded.schema.columns().len();
    if columns > limits.max_columns || decoded.batches.len() > limits.max_batches {
        return Err(Error::Limit);
    }
    let mut remaining = staged_bytes;
    // Admit the complete shape before constructing any converted column.
    for batch in &decoded.batches {
        rows = rows
            .checked_add(batch.num_rows())
            .filter(|n| *n <= limits.max_rows)
            .ok_or(Error::Limit)?;
        let cells = batch.num_rows().checked_mul(columns).ok_or(Error::Limit)?;
        remaining = remaining
            .checked_sub(cells.checked_mul(32).ok_or(Error::Limit)?)
            .ok_or(Error::Limit)?;
        for array in batch.columns() {
            if let Some(strings) = array.as_any().downcast_ref::<LargeStringArray>() {
                let mut bytes = 0usize;
                for value in strings.iter().flatten() {
                    bytes = bytes.checked_add(value.len()).ok_or(Error::Limit)?;
                }
                // The canonical representation uses signed 32-bit offsets.
                if bytes > i32::MAX as usize {
                    return Err(Error::Limit);
                }
                remaining = remaining.checked_sub(bytes).ok_or(Error::Limit)?;
            }
        }
    }
    if rows
        .checked_mul(columns)
        .is_none_or(|n| n > limits.max_cells)
    {
        return Err(Error::Limit);
    }
    let physical = physical_schema(&decoded.schema);
    let mut batches = Vec::with_capacity(decoded.batches.len());
    for batch in decoded.batches {
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns);
        for (field, array) in decoded.schema.columns().iter().zip(batch.columns()) {
            let converted: ArrayRef = match field.kind {
                ColumnType::Str => {
                    if array.as_any().is::<StringArray>() {
                        array.clone()
                    } else {
                        let strings = array
                            .as_any()
                            .downcast_ref::<LargeStringArray>()
                            .ok_or(Error::Invalid)?;
                        Arc::new(StringArray::from_iter(strings.iter()))
                    }
                }
                ColumnType::Date => {
                    let dates = array
                        .as_any()
                        .downcast_ref::<Date32Array>()
                        .ok_or(Error::Invalid)?;
                    let values = dates
                        .iter()
                        .map(|v| {
                            v.map(|day| {
                                date_from_days(i64::from(day), DatePrecision::Day)
                                    .ok_or(Error::Invalid)
                            })
                            .transpose()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Arc::new(date_array(&values).map_err(|_| Error::Invalid)?)
                }
                ColumnType::DateTime => {
                    let dates = array
                        .as_any()
                        .downcast_ref::<TimestampMicrosecondArray>()
                        .ok_or(Error::Invalid)?;
                    let values = dates
                        .iter()
                        .map(|v| {
                            v.map(|micros| {
                                if micros % 1_000_000 != 0 {
                                    return Err(Error::Invalid);
                                }
                                datetime_from_seconds(micros / 1_000_000, DateTimePrecision::Second)
                                    .ok_or(Error::Invalid)
                            })
                            .transpose()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    Arc::new(datetime_array(&values).map_err(|_| Error::Invalid)?)
                }
                ColumnType::Int | ColumnType::Float => array.clone(),
            };
            arrays.push(converted);
        }
        batches.push(RecordBatch::try_new(physical.clone(), arrays).map_err(|_| Error::Invalid)?);
    }
    ArrowTable::try_new(decoded.schema, batches, limits).map_err(Error::Table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{Float64Array, Int64Array};
    use arrow_schema::{Field, Schema, TimeUnit};
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
    fn limits() -> TableLimits {
        TableLimits {
            max_rows: 3,
            max_columns: 5,
            max_batches: 1,
            max_cells: 15,
        }
    }

    #[test]
    fn canonical_snapshot_keeps_empty_null_exact_bits_and_complete_civil_precision() {
        let table = convert(source(), 482, limits()).unwrap();
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
            assert_eq!(value.collected_precision(), DatePrecision::Day);
        }
        for (row, fields) in [
            (0, (1969, 12, 31, 23, 59, 59)),
            (2, (9999, 12, 31, 23, 59, 59)),
        ] {
            let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                panic!()
            };
            assert_eq!(value.fields(), fields);
            assert_eq!(value.collected_precision(), DateTimePrecision::Second);
        }
    }

    #[test]
    fn shape_and_string_copy_charges_are_enforced_before_conversion() {
        assert!(matches!(
            convert(source(), 481, limits()),
            Err(Error::Limit)
        ));
        for limits in [
            TableLimits {
                max_rows: 2,
                ..limits()
            },
            TableLimits {
                max_columns: 4,
                ..limits()
            },
            TableLimits {
                max_cells: 14,
                ..limits()
            },
            TableLimits {
                max_batches: 0,
                ..limits()
            },
        ] {
            assert!(matches!(convert(source(), 482, limits), Err(Error::Limit)));
        }
    }
}
