//! A leaf's column type, read by the closed profile's column rows
//! (`closed_profile`), and the table schema of a file's fields; the findings
//! and their order are core-owned.
use super::closed_profile;
use super::format::{Converted, Logical, Physical};
use crate::storage_api::LaterType;
use arrow_schema::{DataType, Schema};
pub(super) use core_profile::{temporal, Error};
use parquet::{
    basic::{ConvertedType, Type as StoredPhysical},
    schema::types::{ColumnDescriptor, SchemaDescriptor},
};
use yamaa_core::{parquet_source as core_profile, table::TableSchema, value::ColumnType};

fn physical(stored: &ColumnDescriptor) -> Physical {
    physical_type(stored.physical_type())
}
/// A physical type as the reader names it (`format`).
pub(super) fn physical_type(stored: StoredPhysical) -> Physical {
    match stored {
        StoredPhysical::BOOLEAN => Physical::Boolean,
        StoredPhysical::INT32 => Physical::Int32,
        StoredPhysical::INT64 => Physical::Int64,
        StoredPhysical::INT96 => Physical::Int96,
        StoredPhysical::FLOAT => Physical::Float,
        StoredPhysical::DOUBLE => Physical::Double,
        StoredPhysical::BYTE_ARRAY => Physical::ByteArray,
        StoredPhysical::FIXED_LEN_BYTE_ARRAY => Physical::FixedLenByteArray,
    }
}
fn logical(stored: &ColumnDescriptor) -> Option<Logical> {
    crate::storage_api::column_logical(stored)
}
fn converted(stored: &ColumnDescriptor) -> Converted {
    converted_type(stored.converted_type())
}
/// A converted type as the reader names it (`format`).
pub(super) fn converted_type(stored: ConvertedType) -> Converted {
    match stored {
        ConvertedType::NONE => Converted::None,
        ConvertedType::UTF8 => Converted::Utf8,
        ConvertedType::MAP => Converted::Map,
        ConvertedType::MAP_KEY_VALUE => Converted::MapKeyValue,
        ConvertedType::LIST => Converted::List,
        ConvertedType::ENUM => Converted::Enum,
        ConvertedType::DECIMAL => Converted::Decimal,
        ConvertedType::DATE => Converted::Date,
        ConvertedType::TIME_MILLIS => Converted::TimeMillis,
        ConvertedType::TIME_MICROS => Converted::TimeMicros,
        ConvertedType::TIMESTAMP_MILLIS => Converted::TimestampMillis,
        ConvertedType::TIMESTAMP_MICROS => Converted::TimestampMicros,
        ConvertedType::UINT_8 => Converted::Uint8,
        ConvertedType::UINT_16 => Converted::Uint16,
        ConvertedType::UINT_32 => Converted::Uint32,
        ConvertedType::UINT_64 => Converted::Uint64,
        ConvertedType::INT_8 => Converted::Int8,
        ConvertedType::INT_16 => Converted::Int16,
        ConvertedType::INT_32 => Converted::Int32,
        ConvertedType::INT_64 => Converted::Int64,
        ConvertedType::JSON => Converted::Json,
        ConvertedType::BSON => Converted::Bson,
        ConvertedType::INTERVAL => Converted::Interval,
    }
}
/// The column row the leaf column `stored` reads by when the embedded Arrow
/// schema names it in `arrow` (PARQUET-7). A leaf whose variant, geometry or
/// geography annotation the native decoder was not shown, `annotated`
/// (`wire::Checked`), reads by none: that annotation is outside the mapping.
fn row(
    stored: &ColumnDescriptor,
    arrow: &DataType,
    annotated: bool,
) -> Option<&'static closed_profile::Column> {
    if annotated {
        return None;
    }
    closed_profile::column(physical(stored), logical(stored), converted(stored), arrow)
}
/// The bits of the signed integer every stored value of the leaf `stored`
/// must fit, when the embedded schema names it in `arrow` (PARQUET-7).
pub(super) fn width(stored: &ColumnDescriptor, arrow: &DataType) -> Option<u8> {
    row(stored, arrow, false).and_then(|row| row.width)
}
pub(super) fn column_type(
    stored: &ColumnDescriptor,
    arrow: &DataType,
    annotated: bool,
) -> Option<ColumnType> {
    row(stored, arrow, annotated).map(|row| row.kind)
}
/// The table schema of the fields `arrow` of the leaf columns `stored`, or
/// the first field's finding. A field has a column type when it is one leaf,
/// not repeated, whose path is its name, and a column row admits that leaf
/// (`column_type`). `annotated` lists the leaf columns whose variant,
/// geometry or geography annotation the native decoder was not shown, and
/// `later` the later types the embedded schema names
/// (`storage_api::LaterTypes`), which a refused field's stored type spells.
pub(super) fn columns(
    arrow: &Schema,
    stored: &SchemaDescriptor,
    annotated: &[usize],
    later: &[(Vec<usize>, LaterType)],
) -> Result<TableSchema, Error> {
    let mut leaf = 0;
    let fields = arrow.fields().iter().enumerate().map(|(index, field)| {
        let first = leaf;
        while leaf < stored.num_columns() && stored.get_column_root_idx(leaf) == index {
            leaf += 1;
        }
        let descriptor = (leaf - first == 1).then(|| stored.column(first));
        let later: Vec<_> = later
            .iter()
            .filter_map(|(path, later)| match path.split_first() {
                Some((&root, below)) if root == index => Some((below, *later)),
                _ => None,
            })
            .collect();
        let kind = descriptor
            .filter(|column| column.max_rep_level() == 0 && column.path().string() == *field.name())
            .and_then(|column| column_type(&column, field.data_type(), annotated.contains(&first)));
        core_profile::Field {
            name: field.name().clone(),
            kind,
            stored_type: super::type_name::stored_type(field, &later),
        }
    });
    core_profile::columns(fields, stored.root_schema().get_fields().len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_schema::TimeUnit as ArrowUnit;
    use parquet::basic::{LogicalType, Type as Physical};
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
            (
                Physical::INT32,
                None,
                ConvertedType::NONE,
                DataType::Int32,
                ColumnType::Int,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(
                column_type(&stored.column(0), &arrow, false),
                Some(expected)
            );
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
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored.column(0), &arrow, false), None);
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
                // A narrower annotation bounds every stored value; a 32- or
                // 64-bit one holds whatever its physical type stores.
                let bound = (bits < 32).then_some(bits);
                assert_eq!(width(&column, &arrow), bound, "{converted}");
                let dictionary =
                    DataType::Dictionary(Box::new(DataType::Int32), Box::new(arrow.clone()));
                for arrow in [&arrow, &dictionary] {
                    assert_eq!(
                        column_type(&column, arrow, false),
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
            let column = stored.column(0);
            assert_eq!(width(&column, &DataType::Int32), None, "{converted}");
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
                column_type(&stored.column(0), &DataType::Utf8View, false),
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
                        column_type(&stored.column(0), &arrow, false),
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
            (
                Physical::INT32,
                None,
                ConvertedType::NONE,
                DataType::Int32,
                ColumnType::Int,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            let arrow = dictionary(DataType::Int32, value);
            assert_eq!(
                column_type(&stored.column(0), &arrow, false),
                Some(expected)
            );
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
                &descriptor(Physical::INT32, None, ConvertedType::UINT_8),
                dictionary(DataType::Int32, DataType::UInt8),
            ),
            (
                &descriptor(Physical::INT64, timestamp, ConvertedType::NONE),
                dictionary(
                    DataType::Int32,
                    DataType::Timestamp(ArrowUnit::Microsecond, Some("UTC".into())),
                ),
            ),
        ] {
            assert_eq!(
                column_type(&stored.column(0), &arrow, false),
                None,
                "{arrow}"
            );
        }
    }
    #[test]
    fn field_names_and_physical_alignment_have_stable_precedence() {
        use arrow_schema::Field;
        let stored = descriptor(Physical::INT64, None, ConvertedType::NONE);
        assert_eq!(
            columns(&Schema::empty(), &stored, &[], &[]),
            Err(Error::Invalid)
        );
        let arrow = Schema::new(vec![Field::new("", DataType::Int64, true)]);
        assert_eq!(
            columns(&arrow, &stored, &[], &[]),
            Err(Error::EmptyName { field: 1 })
        );
        let arrow = Schema::new(vec![Field::new("X", DataType::Int64, true)]);
        assert_eq!(
            columns(&arrow, &stored, &[], &[]).unwrap().columns()[0].kind,
            ColumnType::Int
        );
        let arrow = Schema::new(vec![
            Field::new("", DataType::Int64, true),
            Field::new("Y", DataType::Int64, true),
        ]);
        assert_eq!(columns(&arrow, &stored, &[], &[]), Err(Error::Invalid));
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
            columns(&arrow, &duplicate, &[], &[]),
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
