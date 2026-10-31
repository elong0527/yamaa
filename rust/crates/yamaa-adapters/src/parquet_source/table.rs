//! Convert admitted stored columns into the engine's precision-bearing table.
use super::reader::{Converted, Decoded};
use crate::{
    column_table::{Column, ColumnTable, TableError},
    epoch::{date_from_days, datetime_from_seconds},
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
    yamaa_core::limits::Limits::table(decoded.rows, columns).map_err(|error| {
        Error::Table(TableError::Limit {
            resource: error.resource,
            limit: error.limit,
            required: error.actual,
        })
    })?;
    for column in &decoded.columns {
        if let Converted::Column(Column::Str(text)) = column {
            // Each decoded text column fits Arrow Utf8's offsets.
            if !crate::column_table::fits_utf8_offsets([text.arena().len()]) {
                return Err(Error::Representation);
            }
        }
    }
    Ok(decoded.rows)
}

/// Convert an admitted decoded table of `rows` records: each temporal
/// column's counts to civil values, a field the stored columns ended before
/// empty, then the table.
pub(super) fn build(decoded: Decoded, rows: usize) -> Result<ColumnTable, Error> {
    let mut converted = decoded.columns.into_iter();
    let mut values = Vec::with_capacity(decoded.schema.columns().len());
    for field in decoded.schema.columns() {
        values.push(match (field.kind, converted.next()) {
            (kind, None) => Column::empty(kind),
            (ColumnType::Date, Some(Converted::Days(days))) => Column::Date(dates(&days)?),
            (ColumnType::DateTime, Some(Converted::Micros(micros))) => Column::DateTime(
                (0..micros.len())
                    .map(|row| {
                        micros
                            .get(row)
                            .map(|micros| {
                                if micros % 1_000_000 != 0 {
                                    return Err(Error::Invalid);
                                }
                                datetime_from_seconds(micros / 1_000_000, Precision::Second)
                                    .ok_or(Error::Invalid)
                            })
                            .transpose()
                    })
                    .collect::<Result<_, _>>()?,
            ),
            (kind, Some(Converted::Column(column))) if column.kind() == kind => column,
            _ => return Err(Error::Invalid),
        });
    }
    ColumnTable::new(decoded.schema, values, rows).map_err(Error::Table)
}

/// Each present day's date, with day precision. A column repeats its days,
/// such as a subject's visit dates, so the last 64 days converted are kept
/// by their low bits and converted once.
fn dates(
    days: &super::columns::Stored<i32>,
) -> Result<Vec<Option<yamaa_core::temporal::Date>>, Error> {
    let mut dates = Vec::with_capacity(days.len());
    let mut seen: [Option<(i32, yamaa_core::temporal::Date)>; 64] = [None; 64];
    for row in 0..days.len() {
        let Some(day) = days.get(row) else {
            dates.push(None);
            continue;
        };
        let slot = &mut seen[day.rem_euclid(64) as usize];
        dates.push(Some(match *slot {
            Some((at, date)) if at == day => date,
            _ => {
                let date = date_from_days(i64::from(day), Precision::Day).ok_or(Error::Invalid)?;
                *slot = Some((day, date));
                date
            }
        }));
    }
    Ok(dates)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{Text, Values};
    use yamaa_core::table::{Column as Field, TableAccess, TableSchema, ValueRef};

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
            .map(|(name, kind)| Field {
                name: name.into(),
                kind,
            })
            .collect(),
        )
        .unwrap();
        let columns = vec![
            Converted::Column(Column::Str(
                [Some(""), None, Some("β")].into_iter().collect::<Text>(),
            )),
            Converted::Column(Column::Int(
                [Some(i64::MIN), None, Some(i64::MAX)].into_iter().collect(),
            )),
            Converted::Column(Column::Float(
                [Some(-0.0), None, Some(f64::from_bits(1))]
                    .into_iter()
                    .collect::<Values<f64>>(),
            )),
            Converted::Days(
                [Some(-719_162), None, Some(2_932_896)]
                    .into_iter()
                    .collect(),
            ),
            Converted::Micros(
                [Some(-1_000_000), None, Some(253_402_300_799_000_000)]
                    .into_iter()
                    .collect(),
            ),
        ];
        Decoded {
            schema,
            columns,
            rows: 3,
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
