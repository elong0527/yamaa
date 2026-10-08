//! Represent physical codec metadata; closed source type admission is core-owned.
use arrow_schema::{DataType, Schema, TimeUnit as ArrowUnit};
pub(super) use core_profile::{temporal, Error};
use parquet::{
    basic::{ConvertedType, LogicalType, TimeUnit, Type as Physical},
    schema::types::{ColumnDescriptor, SchemaDescriptor},
};
use yamaa_core::{parquet_source as core_profile, table::TableSchema, value::ColumnType};

fn physical(stored: &ColumnDescriptor) -> core_profile::Physical {
    use core_profile::Physical as P;
    match stored.physical_type() {
        Physical::BYTE_ARRAY => P::ByteArray,
        Physical::INT32 => P::Int32,
        Physical::INT64 => P::Int64,
        Physical::DOUBLE => P::Double,
        _ => P::Other,
    }
}
fn logical(stored: &ColumnDescriptor) -> core_profile::Logical {
    use core_profile::{Logical as L, Unit};
    match stored.logical_type_ref() {
        None => L::None,
        Some(LogicalType::String) => L::String,
        Some(LogicalType::Date) => L::Date,
        Some(LogicalType::Timestamp(t)) => L::Timestamp {
            unit: if t.unit == TimeUnit::MICROS {
                Unit::Microsecond
            } else if t.unit == TimeUnit::MILLIS {
                Unit::Millisecond
            } else {
                Unit::Nanosecond
            },
            utc: t.is_adjusted_to_u_t_c,
        },
        _ => L::Other,
    }
}
fn converted(stored: &ColumnDescriptor) -> core_profile::Converted {
    use core_profile::Converted as C;
    match stored.converted_type() {
        ConvertedType::NONE => C::None,
        ConvertedType::UTF8 => C::Utf8,
        ConvertedType::DATE => C::Date,
        _ => C::Other,
    }
}
fn representation(arrow: &DataType) -> core_profile::Representation {
    use core_profile::{Representation as R, Unit};
    match arrow {
        DataType::Utf8 | DataType::LargeUtf8 => R::Text,
        DataType::Int64 => R::Int64,
        DataType::Float64 => R::Float64,
        DataType::Date32 => R::Date32,
        DataType::Timestamp(unit, timezone) => R::Timestamp {
            unit: match unit {
                ArrowUnit::Second => Unit::Second,
                ArrowUnit::Millisecond => Unit::Millisecond,
                ArrowUnit::Microsecond => Unit::Microsecond,
                ArrowUnit::Nanosecond => Unit::Nanosecond,
            },
            timezone: timezone.is_some(),
        },
        _ => R::Other,
    }
}
pub(super) fn column_type(stored: &ColumnDescriptor, arrow: &DataType) -> Option<ColumnType> {
    core_profile::column_type(
        physical(stored),
        logical(stored),
        converted(stored),
        representation(arrow),
    )
}
pub(super) fn columns(arrow: &Schema, stored: &SchemaDescriptor) -> Result<TableSchema, Error> {
    let mut leaf = 0;
    let fields = arrow.fields().iter().enumerate().map(|(index, field)| {
        let first = leaf;
        while leaf < stored.num_columns() && stored.get_column_root_idx(leaf) == index {
            leaf += 1;
        }
        let descriptor = (leaf - first == 1).then(|| stored.column(first));
        core_profile::Field {
            name: field.name().clone(),
            leaves: leaf - first,
            path: descriptor
                .as_ref()
                .map_or_else(String::new, |column| column.path().string()),
            repetition: descriptor
                .as_ref()
                .map_or(0, |column| column.max_rep_level()),
            physical: descriptor
                .as_ref()
                .map_or(core_profile::Physical::Other, |column| physical(column)),
            logical: descriptor
                .as_ref()
                .map_or(core_profile::Logical::Other, |column| logical(column)),
            converted: descriptor
                .as_ref()
                .map_or(core_profile::Converted::Other, |column| converted(column)),
            representation: representation(field.data_type()),
            stored_type: super::type_name::field_type(field),
        }
    });
    core_profile::columns(fields, stored.root_schema().get_fields().len())
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
        assert_eq!(columns(&arrow, &stored), Err(Error::Invalid));
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
