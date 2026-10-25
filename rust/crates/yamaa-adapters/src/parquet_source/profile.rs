//! A leaf's column type, read by the closed profile's column rows
//! (`closed_profile`), and the table schema of a file's fields; the findings
//! and their order are core-owned.
use super::closed_profile;
use super::footer::{Kind, Leaf, Schema as Stored};
use crate::storage_api::LaterType;
use arrow_schema::{DataType, Schema};
pub(super) use core_profile::{temporal, Error};
use yamaa_core::{parquet_source as core_profile, table::TableSchema, value::ColumnType};

/// The column row the leaf column `stored` reads by when the embedded Arrow
/// schema names it in `arrow` (PARQUET-7). A leaf whose variant, geometry or
/// geography annotation the footer reads as none, `annotated`
/// (`wire::Checked`), reads by none: that annotation is outside the mapping.
fn row(
    stored: &Leaf,
    arrow: &DataType,
    annotated: bool,
) -> Option<&'static closed_profile::Column> {
    if annotated {
        return None;
    }
    closed_profile::column(stored.physical, stored.logical, stored.converted, arrow)
}
/// The bits of the signed integer every stored value of the leaf `stored`
/// must fit, when the embedded schema names it in `arrow` (PARQUET-7).
pub(super) fn width(stored: &Leaf, arrow: &DataType) -> Option<u8> {
    row(stored, arrow, false).and_then(|row| row.width)
}
pub(super) fn column_type(stored: &Leaf, arrow: &DataType, annotated: bool) -> Option<ColumnType> {
    row(stored, arrow, annotated).map(|row| row.kind)
}
/// The table schema of the fields `arrow` of the schema `stored`, or the
/// first field's finding. A field has a column type when it is one leaf,
/// not repeated, whose path is its name, and a column row admits that leaf
/// (`column_type`). `annotated` lists the leaf columns whose variant,
/// geometry or geography annotation the footer reads as none, and `later`
/// the later types the embedded schema names (`storage_api::LaterTypes`),
/// which a refused field's stored type spells.
pub(super) fn columns(
    arrow: &Schema,
    stored: &Stored,
    annotated: &[usize],
    later: &[(Vec<usize>, LaterType)],
) -> Result<TableSchema, Error> {
    let leaves = &stored.leaves;
    let mut leaf = 0;
    let fields = arrow.fields().iter().enumerate().map(|(index, field)| {
        let first = leaf;
        while leaf < leaves.len() && leaves[leaf].root == index {
            leaf += 1;
        }
        let descriptor = (leaf - first == 1).then(|| &leaves[first]);
        let later: Vec<_> = later
            .iter()
            .filter_map(|(path, later)| match path.split_first() {
                Some((&root, below)) if root == index => Some((below, *later)),
                _ => None,
            })
            .collect();
        let kind = descriptor
            .filter(|column| column.max_repetition == 0 && column.path.join(".") == *field.name())
            .and_then(|column| column_type(column, field.data_type(), annotated.contains(&first)));
        core_profile::Field {
            name: field.name().clone(),
            kind,
            stored_type: super::type_name::stored_type(field, &later),
        }
    });
    let roots = match &stored.nodes[0].kind {
        Kind::Group(children) => children.len(),
        Kind::Primitive(_) => 0,
    };
    core_profile::columns(fields, roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parquet_source::footer::{Node, Repetition};
    use crate::parquet_source::format::{Converted, Logical, Physical, TimeUnit};
    use arrow_schema::TimeUnit as ArrowUnit;

    /// A message of one optional leaf named `X` of each of `leaves`: its
    /// physical type, logical type and converted type, as the footer holds
    /// them.
    fn stored(leaves: &[(Physical, Option<Logical>, Converted)]) -> Stored {
        let mut nodes = vec![Node {
            name: "schema".into(),
            repetition: None,
            converted: Converted::None,
            logical: None,
            kind: Kind::Group((1..=leaves.len()).collect()),
        }];
        let mut columns = Vec::new();
        for (index, &(physical, logical, converted)) in leaves.iter().enumerate() {
            nodes.push(Node {
                name: "X".into(),
                repetition: Some(Repetition::Optional),
                converted,
                logical,
                kind: Kind::Primitive(index),
            });
            columns.push(Leaf {
                node: index + 1,
                root: index,
                path: vec!["X".into()],
                physical,
                type_length: -1,
                precision: -1,
                scale: -1,
                converted,
                logical,
                max_definition: 1,
                max_repetition: 0,
            });
        }
        Stored {
            nodes,
            leaves: columns,
        }
    }
    fn descriptor(physical: Physical, logical: Option<Logical>, converted: Converted) -> Leaf {
        stored(&[(physical, logical, converted)]).leaves.remove(0)
    }
    fn timestamp(utc: bool, unit: TimeUnit) -> Option<Logical> {
        Some(Logical::Timestamp { utc, unit })
    }
    fn integer(bits: i8, signed: bool) -> Option<Logical> {
        Some(Logical::Integer { bits, signed })
    }
    #[test]
    fn only_the_closed_pairs_pass_including_legacy_string_date_and_large_offsets() {
        for (physical, logical, converted, arrow, expected) in [
            (
                Physical::ByteArray,
                Some(Logical::String),
                Converted::Utf8,
                DataType::Utf8,
                ColumnType::Str,
            ),
            (
                Physical::ByteArray,
                None,
                Converted::Utf8,
                DataType::LargeUtf8,
                ColumnType::Str,
            ),
            (
                Physical::Int64,
                None,
                Converted::None,
                DataType::Int64,
                ColumnType::Int,
            ),
            (
                Physical::Double,
                None,
                Converted::None,
                DataType::Float64,
                ColumnType::Float,
            ),
            (
                Physical::Int32,
                Some(Logical::Date),
                Converted::Date,
                DataType::Date32,
                ColumnType::Date,
            ),
            (
                Physical::Int32,
                None,
                Converted::Date,
                DataType::Date32,
                ColumnType::Date,
            ),
            (
                Physical::Int64,
                timestamp(false, TimeUnit::Micros),
                Converted::None,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
                ColumnType::DateTime,
            ),
            (
                Physical::Int32,
                None,
                Converted::None,
                DataType::Int32,
                ColumnType::Int,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored, &arrow, false), Some(expected));
        }
        for (physical, logical, converted, arrow) in [
            (
                Physical::Int64,
                integer(64, false),
                Converted::Uint64,
                DataType::UInt64,
            ),
            (Physical::Int64, None, Converted::Uint64, DataType::UInt64),
            (
                Physical::Int32,
                integer(8, false),
                Converted::Uint8,
                DataType::UInt8,
            ),
            (Physical::Int32, None, Converted::Uint32, DataType::UInt32),
            (
                Physical::Int64,
                integer(64, true),
                Converted::Int64,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::Int64,
                timestamp(true, TimeUnit::Micros),
                Converted::TimestampMicros,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (
                Physical::Int64,
                timestamp(false, TimeUnit::Millis),
                Converted::None,
                DataType::Timestamp(ArrowUnit::Millisecond, None),
            ),
            (
                Physical::Int64,
                None,
                Converted::TimestampMicros,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
            ),
            (Physical::ByteArray, None, Converted::None, DataType::Binary),
        ] {
            let stored = descriptor(physical, logical, converted);
            assert_eq!(column_type(&stored, &arrow, false), None);
        }
    }
    #[test]
    fn signed_integer_annotations_read_as_int_and_declare_their_width() {
        for (physical, bits, converted, arrow) in [
            (Physical::Int64, 64, Converted::Int64, DataType::Int64),
            (Physical::Int32, 32, Converted::Int32, DataType::Int32),
            (Physical::Int32, 16, Converted::Int16, DataType::Int16),
            (Physical::Int32, 8, Converted::Int8, DataType::Int8),
        ] {
            // Both annotations, as pyarrow and Polars write them, and the
            // converted type alone, as DuckDB writes it.
            let logical = integer(bits as i8, true);
            for logical in [logical, None] {
                let column = descriptor(physical, logical, converted);
                // A narrower annotation bounds every stored value; a 32- or
                // 64-bit one holds whatever its physical type stores.
                let bound = (bits < 32).then_some(bits);
                assert_eq!(width(&column, &arrow), bound, "{converted:?}");
                let dictionary =
                    DataType::Dictionary(Box::new(DataType::Int32), Box::new(arrow.clone()));
                for arrow in [&arrow, &dictionary] {
                    assert_eq!(
                        column_type(&column, arrow, false),
                        Some(ColumnType::Int),
                        "{converted:?} {logical:?} {arrow}"
                    );
                }
            }
        }
        for converted in [Converted::None, Converted::Uint8, Converted::Date] {
            let column = descriptor(Physical::Int32, None, converted);
            assert_eq!(width(&column, &DataType::Int32), None, "{converted:?}");
        }
    }
    #[test]
    fn view_and_dictionary_layouts_read_as_their_value_type() {
        fn dictionary(index: DataType, value: DataType) -> DataType {
            DataType::Dictionary(Box::new(index), Box::new(value))
        }
        let text = descriptor(Physical::ByteArray, Some(Logical::String), Converted::Utf8);
        let legacy = descriptor(Physical::ByteArray, None, Converted::Utf8);
        for stored in [&text, &legacy] {
            assert_eq!(
                column_type(stored, &DataType::Utf8View, false),
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
                        column_type(stored, &arrow, false),
                        Some(ColumnType::Str),
                        "{arrow}"
                    );
                }
            }
        }
        let micros = timestamp(false, TimeUnit::Micros);
        for (physical, logical, converted, value, expected) in [
            (
                Physical::Int64,
                None,
                Converted::None,
                DataType::Int64,
                ColumnType::Int,
            ),
            (
                Physical::Double,
                None,
                Converted::None,
                DataType::Float64,
                ColumnType::Float,
            ),
            (
                Physical::Int32,
                Some(Logical::Date),
                Converted::Date,
                DataType::Date32,
                ColumnType::Date,
            ),
            (
                Physical::Int64,
                micros,
                Converted::None,
                DataType::Timestamp(ArrowUnit::Microsecond, None),
                ColumnType::DateTime,
            ),
            (
                Physical::Int32,
                None,
                Converted::None,
                DataType::Int32,
                ColumnType::Int,
            ),
        ] {
            let stored = descriptor(physical, logical, converted);
            let arrow = dictionary(DataType::Int32, value);
            assert_eq!(column_type(&stored, &arrow, false), Some(expected));
        }
        // A layout never supplies a type the stored pair lacks, and a refused
        // value type stays refused inside a dictionary.
        let binary = descriptor(Physical::ByteArray, None, Converted::None);
        for (stored, arrow) in [
            (&binary, DataType::Utf8View),
            (&binary, dictionary(DataType::Int32, DataType::Utf8)),
            (&binary, dictionary(DataType::Int32, DataType::Binary)),
            (&text, dictionary(DataType::Int32, DataType::Binary)),
            (
                &descriptor(Physical::Int32, None, Converted::Uint8),
                dictionary(DataType::Int32, DataType::UInt8),
            ),
            (
                &descriptor(Physical::Int64, micros, Converted::None),
                dictionary(
                    DataType::Int32,
                    DataType::Timestamp(ArrowUnit::Microsecond, Some("UTC".into())),
                ),
            ),
        ] {
            assert_eq!(column_type(stored, &arrow, false), None, "{arrow}");
        }
    }
    #[test]
    fn field_names_and_physical_alignment_have_stable_precedence() {
        use arrow_schema::Field;
        let leaf = (Physical::Int64, None, Converted::None);
        let stored = stored(&[leaf]);
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
        let duplicate = super::tests::stored(&[leaf, leaf]);
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
