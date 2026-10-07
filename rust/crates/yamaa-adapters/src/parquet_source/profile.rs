//! Closed physical/logical source mapping after complete container decoding.
//! Callers must not use these profile findings to mask unreadable page data.
use arrow_schema::{DataType, Schema, TimeUnit as ArrowUnit};
use parquet::{
    basic::{ConvertedType, LogicalType, TimeUnit, Type as Physical},
    schema::types::{ColumnDescriptor, SchemaDescriptor},
};
use std::collections::BTreeSet;
use yamaa_core::{
    table::{Column, TableSchema},
    value::ColumnType,
};

#[derive(Debug, PartialEq)]
pub(super) enum Error {
    Invalid,
    EmptyName {
        field: usize,
    },
    DuplicateName {
        field: String,
    },
    Unsupported {
        field: String,
        stored_type: String,
    },
    Value {
        field: String,
        row: usize,
        value: i64,
    },
}

/// Do not infer integers, timestamps, nested fields or dictionaries from values.
/// Legacy converted UTF8/DATE annotations denote the same closed logical pair.
pub(super) fn column_type(stored: &ColumnDescriptor, arrow: &DataType) -> Option<ColumnType> {
    let logical = stored.logical_type_ref();
    let converted = stored.converted_type();
    let string = matches!(logical, Some(LogicalType::String))
        || (logical.is_none() && converted == ConvertedType::UTF8);
    let date = matches!(logical, Some(LogicalType::Date))
        || (logical.is_none() && converted == ConvertedType::DATE);
    let none = logical.is_none() && converted == ConvertedType::NONE;
    let timestamp = matches!(logical, Some(LogicalType::Timestamp(t))
        if !t.is_adjusted_to_u_t_c && t.unit == TimeUnit::MICROS);
    match (stored.physical_type(), arrow) {
        (Physical::BYTE_ARRAY, DataType::Utf8 | DataType::LargeUtf8) if string => {
            Some(ColumnType::Str)
        }
        (Physical::INT64, DataType::Int64) if none => Some(ColumnType::Int),
        (Physical::DOUBLE, DataType::Float64) if none => Some(ColumnType::Float),
        (Physical::INT32, DataType::Date32) if date => Some(ColumnType::Date),
        (Physical::INT64, DataType::Timestamp(ArrowUnit::Microsecond, None)) if timestamp => {
            Some(ColumnType::DateTime)
        }
        _ => None,
    }
}

/// Pin zero-fields/alignment/name/type precedence in authored field order.
pub(super) fn columns(arrow: &Schema, stored: &SchemaDescriptor) -> Result<TableSchema, Error> {
    if arrow.fields().is_empty() {
        return Err(Error::Invalid);
    }
    let unsupported = |field: &arrow_schema::Field| Error::Unsupported {
        field: field.name().clone(),
        stored_type: super::type_name::field_type(field),
    };
    if stored.num_columns() != arrow.fields().len() {
        return Err(unsupported(&arrow.fields()[0]));
    }
    let mut names = BTreeSet::new();
    let mut columns = Vec::with_capacity(arrow.fields().len());
    for (index, (field, stored)) in arrow.fields().iter().zip(stored.columns()).enumerate() {
        if field.name().is_empty() {
            return Err(Error::EmptyName { field: index + 1 });
        }
        if !names.insert(field.name()) {
            return Err(Error::DuplicateName {
                field: field.name().clone(),
            });
        }
        let kind = if stored.max_rep_level() == 0 && stored.path().string() == *field.name() {
            column_type(stored, field.data_type())
        } else {
            None
        }
        .ok_or_else(|| unsupported(field))?;
        columns.push(Column {
            name: field.name().clone(),
            kind,
        });
    }
    TableSchema::new(columns).map_err(|_| Error::Invalid)
}

/// Check raw storage integers before any date conversion, rounding, or host
/// representation. Return raw values and one-based source rows in diagnostics.
pub(super) fn temporal(kind: ColumnType, value: i64, field: &str, row: usize) -> Result<(), Error> {
    const MIN_DAY: i64 = -719_162;
    const MAX_DAY: i64 = 2_932_896;
    let valid = match kind {
        ColumnType::Date => (MIN_DAY..=MAX_DAY).contains(&value),
        ColumnType::DateTime => {
            (MIN_DAY * 86_400 * 1_000_000..=(MAX_DAY * 86_400 + 86_399) * 1_000_000)
                .contains(&value)
                && value % 1_000_000 == 0
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::Value {
            field: field.to_owned(),
            row,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::{basic::Repetition, schema::types::Type};
    use std::sync::Arc;
    fn descriptor(
        physical: Physical,
        logical: Option<LogicalType>,
        converted: ConvertedType,
    ) -> SchemaDescriptor {
        let field = Arc::new(
            Type::primitive_type_builder("X", physical)
                .with_repetition(Repetition::OPTIONAL)
                .with_logical_type(logical)
                .with_converted_type(converted)
                .build()
                .unwrap(),
        );
        SchemaDescriptor::new(Arc::new(
            Type::group_type_builder("schema")
                .with_fields(vec![field])
                .build()
                .unwrap(),
        ))
    }
    #[test]
    fn only_the_closed_pairs_pass_including_legacy_string_date_and_large_offsets() {
        for (physical, logical, converted, arrow, expected) in [
            (
                Physical::BYTE_ARRAY,
                Some(LogicalType::String),
                ConvertedType::UTF8,
                DataType::Utf8,
                ColumnType::Str,
            ),
            (
                Physical::BYTE_ARRAY,
                None,
                ConvertedType::UTF8,
                DataType::LargeUtf8,
                ColumnType::Str,
            ),
            (
                Physical::INT64,
                None,
                ConvertedType::NONE,
                DataType::Int64,
                ColumnType::Int,
            ),
            (
                Physical::DOUBLE,
                None,
                ConvertedType::NONE,
                DataType::Float64,
                ColumnType::Float,
            ),
            (
                Physical::INT32,
                Some(LogicalType::Date),
                ConvertedType::DATE,
                DataType::Date32,
                ColumnType::Date,
            ),
            (
                Physical::INT32,
                None,
                ConvertedType::DATE,
                DataType::Date32,
                ColumnType::Date,
            ),
            (
                Physical::INT64,
                Some(LogicalType::timestamp(false, TimeUnit::MICROS)),
                ConvertedType::NONE,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
                ColumnType::DateTime,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored.column(0), &arrow), Some(expected));
        }
        for (physical, logical, converted, arrow) in [
            (
                Physical::INT64,
                Some(LogicalType::integer(64, true)),
                ConvertedType::INT_64,
                DataType::Int64,
            ),
            (
                Physical::INT64,
                Some(LogicalType::timestamp(true, TimeUnit::MICROS)),
                ConvertedType::TIMESTAMP_MICROS,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::INT64,
                Some(LogicalType::timestamp(false, TimeUnit::MILLIS)),
                ConvertedType::NONE,
                DataType::Timestamp(ArrowUnit::Millisecond, None),
            ),
            (
                Physical::INT64,
                None,
                ConvertedType::TIMESTAMP_MICROS,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::BYTE_ARRAY,
                None,
                ConvertedType::NONE,
                DataType::Binary,
            ),
            (
                Physical::BYTE_ARRAY,
                Some(LogicalType::String),
                ConvertedType::UTF8,
                DataType::Dictionary(Box::new(DataType::Int32), Box::new(DataType::Utf8)),
            ),
            (Physical::INT32, None, ConvertedType::NONE, DataType::Int32),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored.column(0), &arrow), None);
        }
    }
    #[test]
    fn field_names_and_physical_alignment_have_stable_precedence() {
        use arrow_schema::Field;
        let stored = descriptor(Physical::INT64, None, ConvertedType::NONE);
        assert_eq!(columns(&Schema::empty(), &stored), Err(Error::Invalid));
        let arrow = Schema::new(vec![Field::new("", DataType::Int64, true)]);
        assert_eq!(columns(&arrow, &stored), Err(Error::EmptyName { field: 1 }));
        let arrow = Schema::new(vec![Field::new("X", DataType::Int64, true)]);
        assert_eq!(
            columns(&arrow, &stored).unwrap().columns()[0].kind,
            ColumnType::Int
        );
        let arrow = Schema::new(vec![
            Field::new("", DataType::Int64, true),
            Field::new("Y", DataType::Int64, true),
        ]);
        assert!(
            matches!(columns(&arrow,&stored),Err(Error::Unsupported {field,..}) if field.is_empty())
        );
        let same = stored.root_schema().get_fields()[0].clone();
        let duplicate = SchemaDescriptor::new(Arc::new(
            Type::group_type_builder("schema")
                .with_fields(vec![same.clone(), same])
                .build()
                .unwrap(),
        ));
        let arrow = Schema::new(vec![
            Field::new("X", DataType::Int64, true),
            Field::new("X", DataType::Boolean, true),
        ]);
        assert_eq!(
            columns(&arrow, &duplicate),
            Err(Error::DuplicateName { field: "X".into() })
        );
    }
    #[test]
    fn temporal_edges_are_integral_and_never_rounded_into_range() {
        for value in [-719_162, 0, 2_932_896] {
            temporal(ColumnType::Date, value, "D", 1).unwrap();
        }
        for value in [-719_163, 2_932_897] {
            assert_eq!(
                temporal(ColumnType::Date, value, "D", 9),
                Err(Error::Value {
                    field: "D".into(),
                    row: 9,
                    value
                })
            );
        }
        let min = -719_162 * 86_400 * 1_000_000i64;
        let max = (2_932_896 * 86_400 + 86_399) * 1_000_000i64;
        for value in [min, -1_000_000, 0, max] {
            temporal(ColumnType::DateTime, value, "T", 1).unwrap();
        }
        for value in [min - 1, max + 1, -1, 1, i64::MIN, i64::MAX] {
            assert!(matches!(
                temporal(ColumnType::DateTime, value, "T", 7),
                Err(Error::Value { row: 7, .. })
            ));
        }
    }
}
