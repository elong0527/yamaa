//! Represent physical codec metadata; closed source type admission is core-owned.
use arrow_schema::{DataType, Schema, TimeUnit as ArrowUnit};
pub(super) use core_profile::{temporal, Error};
use parquet::{
    basic::{ConvertedType, Type as Physical},
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
    crate::storage_api::column_logical(stored)
}
fn converted(stored: &ColumnDescriptor) -> core_profile::Converted {
    use core_profile::Converted as C;
    let integer = |bits, signed| C::Integer { bits, signed };
    match stored.converted_type() {
        ConvertedType::NONE => C::None,
        ConvertedType::UTF8 => C::Utf8,
        ConvertedType::DATE => C::Date,
        ConvertedType::INT_8 => integer(8, true),
        ConvertedType::INT_16 => integer(16, true),
        ConvertedType::INT_32 => integer(32, true),
        ConvertedType::INT_64 => integer(64, true),
        ConvertedType::UINT_8 => integer(8, false),
        ConvertedType::UINT_16 => integer(16, false),
        ConvertedType::UINT_32 => integer(32, false),
        ConvertedType::UINT_64 => integer(64, false),
        _ => C::Other,
    }
}
/// The width a field's signed integer annotation declares, logical or legacy.
/// Every stored value of an `INT32` field that reads as `int` must fit it
/// (REQ-1032).
pub(super) fn declared_width(stored: &ColumnDescriptor) -> Option<u8> {
    use core_profile::{Converted as C, Logical as L};
    match (logical(stored), converted(stored)) {
        (L::Integer { bits, signed: true }, _) | (L::None, C::Integer { bits, signed: true }) => {
            Some(bits)
        }
        _ => None,
    }
}
/// The embedded Arrow schema chooses a layout, never a type (REQ-1032): every
/// string layout is text, and a dictionary reads as its value type whatever
/// its index type.
fn representation(arrow: &DataType) -> core_profile::Representation {
    use core_profile::{Representation as R, Unit};
    match arrow {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => R::Text,
        DataType::Dictionary(_, value) => representation(value),
        DataType::Int8 => R::Int8,
        DataType::Int16 => R::Int16,
        DataType::Int32 => R::Int32,
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
    use parquet::basic::LogicalType;
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
                Some(crate::storage_api::timestamp_micros(false)),
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
                Some(crate::storage_api::integer(64, false)),
                ConvertedType::UINT_64,
                DataType::UInt64,
            ),
            (
                Physical::INT64,
                None,
                ConvertedType::UINT_64,
                DataType::UInt64,
            ),
            (
                Physical::INT32,
                Some(crate::storage_api::integer(8, false)),
                ConvertedType::UINT_8,
                DataType::UInt8,
            ),
            (
                Physical::INT32,
                None,
                ConvertedType::UINT_32,
                DataType::UInt32,
            ),
            (
                Physical::INT64,
                Some(crate::storage_api::integer(64, true)),
                ConvertedType::INT_64,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::INT64,
                Some(crate::storage_api::timestamp_micros(true)),
                ConvertedType::TIMESTAMP_MICROS,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::INT64,
                Some(crate::storage_api::timestamp_millis(false)),
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
            (Physical::INT32, None, ConvertedType::NONE, DataType::Int32),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored.column(0), &arrow), None);
        }
    }
    #[test]
    fn signed_integer_annotations_read_as_int_and_declare_their_width() {
        for (physical, bits, converted, arrow) in [
            (Physical::INT64, 64, ConvertedType::INT_64, DataType::Int64),
            (Physical::INT32, 32, ConvertedType::INT_32, DataType::Int32),
            (Physical::INT32, 16, ConvertedType::INT_16, DataType::Int16),
            (Physical::INT32, 8, ConvertedType::INT_8, DataType::Int8),
        ] {
            // Both annotations, as pyarrow and Polars write them, and the
            // converted type alone, as DuckDB writes it.
            let logical = Some(crate::storage_api::integer(bits as i8, true));
            for logical in [logical, None] {
                let stored = descriptor(physical, logical.clone(), converted);
                let column = stored.column(0);
                assert_eq!(declared_width(&column), Some(bits), "{converted}");
                let dictionary =
                    DataType::Dictionary(Box::new(DataType::Int32), Box::new(arrow.clone()));
                for arrow in [&arrow, &dictionary] {
                    assert_eq!(
                        column_type(&column, arrow),
                        Some(ColumnType::Int),
                        "{converted} {logical:?} {arrow}"
                    );
                }
            }
        }
        for converted in [
            ConvertedType::NONE,
            ConvertedType::UINT_8,
            ConvertedType::DATE,
        ] {
            let stored = descriptor(Physical::INT32, None, converted);
            assert_eq!(declared_width(&stored.column(0)), None, "{converted}");
        }
    }
    #[test]
    fn view_and_dictionary_layouts_read_as_their_value_type() {
        fn dictionary(index: DataType, value: DataType) -> DataType {
            DataType::Dictionary(Box::new(index), Box::new(value))
        }
        let text = descriptor(
            Physical::BYTE_ARRAY,
            Some(LogicalType::String),
            ConvertedType::UTF8,
        );
        let legacy = descriptor(Physical::BYTE_ARRAY, None, ConvertedType::UTF8);
        for stored in [&text, &legacy] {
            assert_eq!(
                column_type(&stored.column(0), &DataType::Utf8View),
                Some(ColumnType::Str)
            );
            for index in [
                DataType::Int8,
                DataType::Int16,
                DataType::Int32,
                DataType::Int64,
                DataType::UInt8,
                DataType::UInt16,
                DataType::UInt32,
                DataType::UInt64,
            ] {
                for value in [DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View] {
                    let arrow = dictionary(index.clone(), value);
                    assert_eq!(
                        column_type(&stored.column(0), &arrow),
                        Some(ColumnType::Str),
                        "{arrow}"
                    );
                }
            }
        }
        let timestamp = Some(crate::storage_api::timestamp_micros(false));
        for (physical, logical, converted, value, expected) in [
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
                Physical::INT64,
                timestamp.clone(),
                ConvertedType::NONE,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
                ColumnType::DateTime,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            let arrow = dictionary(DataType::Int32, value);
            assert_eq!(column_type(&stored.column(0), &arrow), Some(expected));
        }
        // A layout never supplies a type the stored pair lacks, and a refused
        // value type stays refused inside a dictionary.
        let binary = descriptor(Physical::BYTE_ARRAY, None, ConvertedType::NONE);
        for (stored, arrow) in [
            (&binary, DataType::Utf8View),
            (&binary, dictionary(DataType::Int32, DataType::Utf8)),
            (&binary, dictionary(DataType::Int32, DataType::Binary)),
            (&text, dictionary(DataType::Int32, DataType::Binary)),
            (
                &descriptor(Physical::INT32, None, ConvertedType::NONE),
                dictionary(DataType::Int32, DataType::Int32),
            ),
            (
                &descriptor(Physical::INT64, timestamp, ConvertedType::NONE),
                dictionary(
                    DataType::Int32,
                    DataType::Timestamp(ArrowUnit::Microsecond, Some("UTC".into())),
                ),
            ),
        ] {
            assert_eq!(column_type(&stored.column(0), &arrow), None, "{arrow}");
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
