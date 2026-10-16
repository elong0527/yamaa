//! Arrow conversion for the core batch invocation contract.
//!
//! Batch arguments cross the adapter boundary as Arrow arrays; the core
//! contract itself is logical. This module materializes one logical
//! column slice from an admitted `ArrowTable` (temporal values keep their
//! collected precision, exactly as scalar cell access returns them) and
//! converts an accepted logical result column back to the canonical
//! physical Arrow representation. Conversion back rejects any value whose
//! exact type differs from the declared column type, so a wrong-type host
//! result can never be silently coerced into an output array; the core
//! batch service performs the same check before output is accepted.

use crate::arrow_table::{date_array, datetime_array, string_array, ArrowTable};
use arrow_array::{ArrayRef, Float64Array, Int64Array};
use arrow_schema::ArrowError;
use std::{convert::Infallible, sync::Arc};
use yamaa_core::{
    table::{CellError, TableAccess, ValueRef},
    value::{ColumnType, Value},
};

/// Materialize owned logical values for `rows` of one declared column, in
/// row order. Missing cells stay `Value::Missing`, distinct from an empty
/// string; temporal cells retain their collected precision.
pub fn column_values(
    table: &ArrowTable,
    column: usize,
    rows: std::ops::Range<usize>,
) -> Result<Vec<Value>, CellError<Infallible>> {
    rows.map(|row| match table.cell(row, column)? {
        ValueRef::Missing => Ok(Value::Missing),
        ValueRef::Str(value) => Ok(Value::Str(value.to_owned())),
        ValueRef::Int(value) => Ok(Value::Int(value)),
        ValueRef::Float(value) => Ok(Value::Float(value)),
        ValueRef::Bool(value) => Ok(Value::Bool(value)),
        ValueRef::Date(value) => Ok(Value::Date(value)),
        ValueRef::DateTime(value) => Ok(Value::DateTime(value)),
    })
    .collect()
}

/// Convert accepted logical results to the canonical physical array for
/// `kind`. Every present value must have exactly the declared type;
/// missing values become Arrow nulls. The temporal struct keeps the
/// `{value, precision}` representation until the host scalar boundary.
pub fn values_array(kind: ColumnType, values: &[Value]) -> Result<ArrayRef, ArrowError> {
    let mismatch = || ArrowError::CastError("batch result type mismatch".into());
    Ok(match kind {
        ColumnType::Int => Arc::new(Int64Array::from(
            values
                .iter()
                .map(|value| match value {
                    Value::Missing => Ok(None),
                    Value::Int(value) => Ok(Some(*value)),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        ColumnType::Float => Arc::new(Float64Array::from(
            values
                .iter()
                .map(|value| match value {
                    Value::Missing => Ok(None),
                    Value::Float(value) => Ok(Some(value.get())),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        ColumnType::Str => Arc::new(string_array(
            values
                .iter()
                .map(|value| match value {
                    Value::Missing => Ok(None),
                    Value::Str(value) => Ok(Some(value.as_str())),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?),
        ColumnType::Date => Arc::new(date_array(
            &values
                .iter()
                .map(|value| match value {
                    Value::Missing => Ok(None),
                    Value::Date(value) => Ok(Some(*value)),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?),
        ColumnType::DateTime => Arc::new(datetime_array(
            &values
                .iter()
                .map(|value| match value {
                    Value::Missing => Ok(None),
                    Value::DateTime(value) => Ok(Some(*value)),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::RecordBatch;
    use yamaa_core::{
        table::{Column, TableSchema},
        temporal::{Date, DatePrecision},
    };

    fn make_table(kind: ColumnType, array: ArrayRef) -> ArrowTable {
        let schema = TableSchema::new(vec![Column {
            name: "X".into(),
            kind,
        }])
        .unwrap();
        let physical = crate::arrow_table::physical_schema(&schema);
        let batch = RecordBatch::try_new(physical, vec![array]).unwrap();
        ArrowTable::try_new(schema, vec![batch]).unwrap()
    }

    #[test]
    fn column_values_retain_precision_missing_and_empty_string() {
        let dates = date_array(&[
            Some(Date::new(2025, 1, 1, DatePrecision::Year).unwrap()),
            None,
            Some(Date::new(2024, 2, 3, DatePrecision::Day).unwrap()),
        ])
        .unwrap();
        let table = make_table(ColumnType::Date, Arc::new(dates));
        let values = column_values(&table, 0, 0..3).unwrap();
        assert_eq!(
            values,
            [
                Value::Date(Date::new(2025, 1, 1, DatePrecision::Year).unwrap()),
                Value::Missing,
                Value::Date(Date::new(2024, 2, 3, DatePrecision::Day).unwrap()),
            ]
        );
        if let Value::Date(date) = values[0] {
            assert_eq!(date.collected_precision(), DatePrecision::Year);
        } else {
            panic!("date")
        }

        let strings = string_array([Some(""), None, Some("é")]).unwrap();
        let table = make_table(ColumnType::Str, Arc::new(strings));
        assert_eq!(
            column_values(&table, 0, 0..3).unwrap(),
            [
                Value::Str(String::new()),
                Value::Missing,
                Value::Str("é".into())
            ]
        );
    }

    #[test]
    fn values_array_round_trips_and_rejects_wrong_types() {
        let values = [Value::Int(i64::MIN), Value::Missing, Value::Int(i64::MAX)];
        let array = values_array(ColumnType::Int, &values).unwrap();
        let table = make_table(ColumnType::Int, array);
        assert_eq!(column_values(&table, 0, 0..3).unwrap(), values);
        assert!(values_array(ColumnType::Int, &[Value::Str("1".into())]).is_err());
        assert!(values_array(ColumnType::Int, &[Value::Bool(true)]).is_err());
        assert!(values_array(ColumnType::Float, &[Value::Int(1)]).is_err());
        let empty = values_array(ColumnType::Str, &[]).unwrap();
        assert_eq!(empty.len(), 0);
    }
}
