//! Bound the embedded Arrow schema before the generic Parquet schema conversion.
use super::compact::Error;
use crate::storage_api::LaterTypes;
use base64::{prelude::BASE64_STANDARD, Engine};

/// A file's embedded Arrow schema, in Arrow 55's types.
#[derive(Debug, PartialEq)]
pub(super) struct Hint {
    /// The schema, with each later type it names held as its nearest Arrow
    /// 55 type (`storage_api::ipc_schema`).
    pub schema: arrow_schema::Schema,
    /// Where `schema` holds a later type, and which; empty for most files.
    pub later: LaterTypes,
    /// Whether the message verifies within the FlatBuffers verifier's
    /// default depth, as the Arrow schema conversion verified it again: a
    /// file whose hint nests deeper reads to no Arrow schema
    /// (`conversion`).
    pub shallow: bool,
}

/// The embedded Arrow schema of the key-value metadata `metadata`, if it
/// has one.
pub(super) fn admit(metadata: &[(String, Option<String>)]) -> Result<Option<Hint>, Error> {
    // The general reader uses the last non-null value for each metadata key.
    let value = metadata.iter().rev().find_map(|(key, value)| {
        if key == "ARROW:schema" {
            value.as_deref()
        } else {
            None
        }
    });
    let Some(value) = value else {
        return Ok(None);
    };

    let bytes = BASE64_STANDARD
        .decode(value)
        .map_err(|_| Error::Malformed)?;
    let message = if bytes.len() > 8 && bytes[..4] == [255; 4] {
        &bytes[8..]
    } else {
        &bytes
    };
    let options = flatbuffers::VerifierOptions {
        max_depth: yamaa_core::limits::Limits::SEMANTIC_DEPTH,
        ..Default::default()
    };
    let shallow = arrow_ipc::root_as_message(message).is_ok();
    let message =
        arrow_ipc::root_as_message_with_opts(&options, message).map_err(|_| Error::Malformed)?;
    let schema = message.header_as_schema().ok_or(Error::Malformed)?;
    let (schema, later) = crate::storage_api::ipc_schema(schema).map_err(|_| Error::Malformed)?;
    Ok(Some(Hint {
        schema,
        later,
        shallow,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::file::metadata::KeyValue;

    /// Key-value metadata as the footer holds it.
    fn pairs(metadata: &[KeyValue]) -> Vec<(String, Option<String>)> {
        metadata
            .iter()
            .map(|item| (item.key.clone(), item.value.clone()))
            .collect()
    }

    // FlatBuffers permits many vector entries to share one table and string.
    // Verification must account for each traversal, not only the stored bytes.
    fn shared_metadata(repetitions: usize, text_bytes: usize) -> Vec<KeyValue> {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let key = builder.create_string("shared");
        let value = builder.create_string(&"x".repeat(text_bytes));
        let item = arrow_ipc::KeyValue::create(
            &mut builder,
            &arrow_ipc::KeyValueArgs {
                key: Some(key),
                value: Some(value),
            },
        );
        let metadata = builder.create_vector(&vec![item; repetitions]);
        let fields = builder.create_vector::<flatbuffers::WIPOffset<arrow_ipc::Field<'_>>>(&[]);
        let schema = arrow_ipc::Schema::create(
            &mut builder,
            &arrow_ipc::SchemaArgs {
                fields: Some(fields),
                ..Default::default()
            },
        );
        let message = arrow_ipc::Message::create(
            &mut builder,
            &arrow_ipc::MessageArgs {
                header_type: arrow_ipc::MessageHeader::Schema,
                header: Some(schema.as_union_value()),
                custom_metadata: Some(metadata),
                ..Default::default()
            },
        );
        builder.finish(message, None);
        vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some(BASE64_STANDARD.encode(builder.finished_data())),
        }]
    }

    #[test]
    fn shared_tables_and_strings_retain_flatbuffer_verifier_limits() {
        assert!(admit(&pairs(&shared_metadata(4, 8))).unwrap().is_some());
        let defaults = flatbuffers::VerifierOptions::default();
        // A four-MiB vector exceeds the default table-visit limit without a
        // deep tree or a correspondingly large collection of stored tables.
        assert_eq!(
            admit(&pairs(&shared_metadata(defaults.max_tables, 8))),
            Err(Error::Malformed)
        );
        // The same stored string can also multiply apparent traversal bytes.
        let text_bytes = 1024 * 1024;
        assert_eq!(
            admit(&pairs(&shared_metadata(
                defaults.max_apparent_size / text_bytes + 1,
                text_bytes,
            ))),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn shared_schema_tables_keep_the_flatbuffer_verifiers_default_work_bounds() {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let name = builder.create_string("shared");
        let null = arrow_ipc::Null::create(&mut builder, &Default::default());
        let mut field = arrow_ipc::Field::create(
            &mut builder,
            &arrow_ipc::FieldArgs {
                name: Some(name),
                type_type: arrow_ipc::Type::Null,
                type_: Some(null.as_union_value()),
                ..Default::default()
            },
        );
        // This DAG occupies less than 2 KiB but expands to over two million
        // table visits, well below the fixed semantic nesting boundary.
        for _ in 0..20 {
            let children = builder.create_vector(&[field, field]);
            let structure = arrow_ipc::Struct_::create(&mut builder, &Default::default());
            field = arrow_ipc::Field::create(
                &mut builder,
                &arrow_ipc::FieldArgs {
                    name: Some(name),
                    type_type: arrow_ipc::Type::Struct_,
                    type_: Some(structure.as_union_value()),
                    children: Some(children),
                    ..Default::default()
                },
            );
        }
        let fields = builder.create_vector(&[field]);
        let schema = arrow_ipc::Schema::create(
            &mut builder,
            &arrow_ipc::SchemaArgs {
                fields: Some(fields),
                ..Default::default()
            },
        );
        let message = arrow_ipc::Message::create(
            &mut builder,
            &arrow_ipc::MessageArgs {
                header_type: arrow_ipc::MessageHeader::Schema,
                header: Some(schema.as_union_value()),
                ..Default::default()
            },
        );
        builder.finish(message, None);
        let bytes = builder.finished_data();
        assert!(bytes.len() < 2048);
        assert!(matches!(
            arrow_ipc::root_as_message(bytes),
            Err(flatbuffers::InvalidFlatbuffer::TooManyTables)
        ));
        let metadata = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some(BASE64_STANDARD.encode(bytes)),
        }];
        assert_eq!(admit(&pairs(&metadata)), Err(Error::Malformed));
    }

    #[test]
    fn invalid_schema_hints_and_encoding_are_not_ignored() {
        admit(&[]).unwrap();
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("!".into()),
        }];
        assert_eq!(admit(&pairs(&data)), Err(Error::Malformed));
        let data = vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some("AAAA".into()),
        }];
        assert_eq!(admit(&pairs(&data)), Err(Error::Malformed));
    }

    /// The type of one field of an authored IPC schema; a child is named
    /// `item`, the children of a struct `c0`, `c1` and so on, and a map's
    /// `entries`, `key` and `value`, as the parquet crate's writer names them.
    enum Ipc {
        Int64,
        Utf8,
        /// A decimal of precision 9 and scale 2, of this bit width.
        Decimal(i32),
        List(Box<Ipc>),
        ListView(Box<Ipc>),
        LargeListView(Box<Ipc>),
        /// A list of this many values in every row.
        FixedSizeList(Box<Ipc>, i32),
        Struct(Vec<Ipc>),
        /// A map from text keys to values of this type.
        Map(Box<Ipc>),
        /// A sparse union of one child.
        Union(Box<Ipc>),
        /// Dictionary-encoded, with 32-bit indices.
        Dictionary(Box<Ipc>),
    }

    fn ipc_field<'a>(
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
        name: &str,
        ipc: &Ipc,
    ) -> flatbuffers::WIPOffset<arrow_ipc::Field<'a>> {
        use arrow_ipc::Type;
        let (encoded, ipc) = match ipc {
            Ipc::Dictionary(value) => (true, &**value),
            ipc => (false, ipc),
        };
        let children: Vec<_> = match ipc {
            Ipc::List(child)
            | Ipc::ListView(child)
            | Ipc::LargeListView(child)
            | Ipc::FixedSizeList(child, _)
            | Ipc::Union(child) => {
                vec![ipc_field(builder, "item", child)]
            }
            Ipc::Map(value) => {
                let key = ipc_field(builder, "key", &Ipc::Utf8);
                let value = ipc_field(builder, "value", value);
                vec![ipc_entries(builder, &[key, value])]
            }
            Ipc::Struct(children) => children
                .iter()
                .enumerate()
                .map(|(index, child)| ipc_field(builder, &format!("c{index}"), child))
                .collect(),
            _ => Vec::new(),
        };
        let children = builder.create_vector(&children);
        let (type_type, type_) = match ipc {
            Ipc::Int64 => {
                let args = arrow_ipc::IntArgs {
                    bitWidth: 64,
                    is_signed: true,
                };
                let int = arrow_ipc::Int::create(builder, &args);
                (Type::Int, int.as_union_value())
            }
            Ipc::Utf8 => {
                let utf8 = arrow_ipc::Utf8::create(builder, &Default::default());
                (Type::Utf8, utf8.as_union_value())
            }
            Ipc::Decimal(bits) => {
                let args = arrow_ipc::DecimalArgs {
                    precision: 9,
                    scale: 2,
                    bitWidth: *bits,
                };
                let decimal = arrow_ipc::Decimal::create(builder, &args);
                (Type::Decimal, decimal.as_union_value())
            }
            Ipc::List(_) => {
                let list = arrow_ipc::List::create(builder, &Default::default());
                (Type::List, list.as_union_value())
            }
            Ipc::ListView(_) => {
                let view = arrow_ipc::ListView::create(builder, &Default::default());
                (Type::ListView, view.as_union_value())
            }
            Ipc::LargeListView(_) => {
                let view = arrow_ipc::LargeListView::create(builder, &Default::default());
                (Type::LargeListView, view.as_union_value())
            }
            Ipc::FixedSizeList(_, size) => {
                let args = arrow_ipc::FixedSizeListArgs { listSize: *size };
                let list = arrow_ipc::FixedSizeList::create(builder, &args);
                (Type::FixedSizeList, list.as_union_value())
            }
            Ipc::Map(_) => {
                let map = arrow_ipc::Map::create(builder, &Default::default());
                (Type::Map, map.as_union_value())
            }
            Ipc::Struct(_) => {
                let structure = arrow_ipc::Struct_::create(builder, &Default::default());
                (Type::Struct_, structure.as_union_value())
            }
            Ipc::Union(_) => {
                let args = arrow_ipc::UnionArgs {
                    mode: arrow_ipc::UnionMode::Sparse,
                    typeIds: None,
                };
                let union = arrow_ipc::Union::create(builder, &args);
                (Type::Union, union.as_union_value())
            }
            Ipc::Dictionary(_) => unreachable!("a dictionary of a dictionary"),
        };
        let dictionary = encoded.then(|| {
            let args = arrow_ipc::IntArgs {
                bitWidth: 32,
                is_signed: true,
            };
            let index = arrow_ipc::Int::create(builder, &args);
            let args = arrow_ipc::DictionaryEncodingArgs {
                indexType: Some(index),
                ..Default::default()
            };
            arrow_ipc::DictionaryEncoding::create(builder, &args)
        });
        let name = builder.create_string(name);
        let args = arrow_ipc::FieldArgs {
            name: Some(name),
            // A map's key, and its entries, are never null.
            nullable: !matches!(ipc, Ipc::Utf8),
            type_type,
            type_: Some(type_),
            dictionary,
            children: Some(children),
            custom_metadata: None,
        };
        arrow_ipc::Field::create(builder, &args)
    }

    /// A map's `entries`: a struct, never null, of `children`.
    fn ipc_entries<'a>(
        builder: &mut flatbuffers::FlatBufferBuilder<'a>,
        children: &[flatbuffers::WIPOffset<arrow_ipc::Field<'a>>],
    ) -> flatbuffers::WIPOffset<arrow_ipc::Field<'a>> {
        let children = builder.create_vector(children);
        let structure = arrow_ipc::Struct_::create(builder, &Default::default());
        let name = builder.create_string("entries");
        let args = arrow_ipc::FieldArgs {
            name: Some(name),
            nullable: false,
            type_type: arrow_ipc::Type::Struct_,
            type_: Some(structure.as_union_value()),
            children: Some(children),
            ..Default::default()
        };
        arrow_ipc::Field::create(builder, &args)
    }

    /// Key-value metadata embedding an IPC schema of `fields`, as writers
    /// embed one: an encapsulated message, its continuation marker, length
    /// and flatbuffer padded to eight bytes.
    fn hint(fields: &[(&str, Ipc)], endianness: arrow_ipc::Endianness) -> Vec<KeyValue> {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let fields: Vec<_> = fields
            .iter()
            .map(|(name, ipc)| ipc_field(&mut builder, name, ipc))
            .collect();
        let fields = builder.create_vector(&fields);
        let args = arrow_ipc::SchemaArgs {
            endianness,
            fields: Some(fields),
            ..Default::default()
        };
        let schema = arrow_ipc::Schema::create(&mut builder, &args);
        let args = arrow_ipc::MessageArgs {
            version: arrow_ipc::MetadataVersion::V5,
            header_type: arrow_ipc::MessageHeader::Schema,
            header: Some(schema.as_union_value()),
            ..Default::default()
        };
        let message = arrow_ipc::Message::create(&mut builder, &args);
        builder.finish(message, None);
        let mut message = builder.finished_data().to_vec();
        message.resize(message.len().next_multiple_of(8), 0);
        let mut encapsulated = vec![0xff; 4];
        encapsulated.extend((message.len() as u32).to_le_bytes());
        encapsulated.extend(message);
        vec![KeyValue {
            key: "ARROW:schema".into(),
            value: Some(BASE64_STANDARD.encode(encapsulated)),
        }]
    }

    /// Arrow 55 has no 32- or 64-bit decimal and its IPC conversion reads no
    /// list view, so each is held as its nearest Arrow 55 type, and the hint
    /// says where. Below a union or in a dictionary, where no writer puts
    /// one, and in a big-endian schema, the hint stays unreadable.
    #[test]
    fn later_types_are_held_as_their_nearest_arrow_55_type() {
        use crate::storage_api::LaterType;
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc;
        let little = arrow_ipc::Endianness::Little;
        let fields = [
            ("V", Ipc::Int64),
            ("D", Ipc::Decimal(32)),
            (
                "S",
                Ipc::Struct(vec![Ipc::ListView(Box::new(Ipc::Decimal(64)))]),
            ),
            ("L", Ipc::List(Box::new(Ipc::Decimal(32)))),
        ];
        let held = admit(&pairs(&hint(&fields, little))).unwrap().unwrap();
        let item = || Arc::new(Field::new("item", DataType::Decimal128(9, 2), true));
        let list = Field::new("c0", DataType::List(item()), true);
        let schema = Schema::new(vec![
            Field::new("V", DataType::Int64, true),
            Field::new("D", DataType::Decimal128(9, 2), true),
            Field::new("S", DataType::Struct(vec![list].into()), true),
            Field::new("L", DataType::List(item()), true),
        ]);
        assert_eq!(held.schema, schema);
        let decimal = |bits| LaterType::Decimal {
            bits,
            precision: 9,
            scale: 2,
        };
        assert_eq!(
            held.later,
            [
                (vec![1], decimal(32)),
                (vec![2, 0, 0], decimal(64)),
                (vec![2, 0], LaterType::ListView),
                (vec![3, 0], decimal(32)),
            ]
        );
        // An ordinary hint holds no later type.
        let ordinary = admit(&pairs(&hint(&[("V", Ipc::Int64)], little)));
        let ordinary = ordinary.unwrap().unwrap();
        assert!(ordinary.later.is_empty());
        for (name, fields, endianness) in [
            (
                "union",
                [("U", Ipc::Union(Box::new(Ipc::Decimal(32))))],
                little,
            ),
            (
                "dictionary",
                [("D", Ipc::Dictionary(Box::new(Ipc::Decimal(32))))],
                little,
            ),
            (
                "big-endian",
                [("D", Ipc::Decimal(32))],
                arrow_ipc::Endianness::Big,
            ),
            // A decimal width that no Arrow release has.
            ("16-bit", [("D", Ipc::Decimal(16))], little),
        ] {
            let refused = admit(&pairs(&hint(&fields, endianness)));
            assert_eq!(refused, Err(Error::Malformed), "{name}");
        }
    }

    /// One row group of two rows holding `columns`, written by the parquet
    /// crate's writer, whose embedded Arrow schema is `fields` in place of
    /// its own.
    fn hinted(columns: Vec<(&str, arrow_array::ArrayRef)>, fields: &[(&str, Ipc)]) -> Vec<u8> {
        use parquet::{
            arrow::{arrow_writer::ArrowWriterOptions, ArrowWriter},
            file::properties::WriterProperties,
        };
        let columns = columns.into_iter().map(|(name, array)| (name, array, true));
        let batch = arrow_array::RecordBatch::try_from_iter_with_nullable(columns).unwrap();
        let metadata = hint(fields, arrow_ipc::Endianness::Little);
        let properties = WriterProperties::builder()
            .set_key_value_metadata(Some(metadata))
            .build();
        let options = ArrowWriterOptions::new()
            .with_properties(properties)
            .with_skip_arrow_metadata(true);
        let mut bytes = Vec::new();
        let mut writer =
            ArrowWriter::try_new_with_options(&mut bytes, batch.schema(), options).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        bytes
    }

    /// A later type in the embedded schema spells a refused field's stored
    /// type where the native conversion applies it, as parquet 57 and
    /// pyarrow 26 apply a decimal and pyarrow 26 a list view, and never
    /// refuses a field that its Parquet type admits (PARQUET-7, PARQUET-10).
    #[test]
    fn a_later_type_spells_a_refused_field_and_refuses_no_other() {
        use super::super::{parse, Error as Parsed};
        use arrow_array::{
            builder::{MapBuilder, MapFieldNames, StringBuilder},
            types::Decimal128Type,
            ArrayRef, Decimal128Array, Int32Array, Int64Array, LargeListArray, ListArray,
            StructArray, UInt64Array,
        };
        use arrow_buffer::OffsetBuffer;
        use arrow_schema::{DataType, Field};
        use std::sync::Arc;
        use yamaa_core::{table::TableAccess, value::ColumnType};
        let decimals = || {
            let values = Decimal128Array::from(vec![125, 250]);
            Arc::new(values.with_precision_and_scale(9, 2).unwrap())
        };
        let item = |kind| Arc::new(Field::new("item", kind, true));
        let decimal = DataType::Decimal128(9, 2);
        let list: ArrayRef = Arc::new(ListArray::new(
            item(decimal.clone()),
            OffsetBuffer::from_lengths([1, 1]),
            decimals(),
            None,
        ));
        let large: ArrayRef = Arc::new(LargeListArray::new(
            item(DataType::Int64),
            OffsetBuffer::from_lengths([1, 1]),
            Arc::new(Int64Array::from(vec![1, 2])),
            None,
        ));
        let structure: ArrayRef = Arc::new(StructArray::from(vec![(
            Arc::new(Field::new("c0", decimal.clone(), true)),
            decimals() as ArrayRef,
        )]));
        let value = arrow_array::builder::PrimitiveBuilder::<Decimal128Type>::new()
            .with_precision_and_scale(9, 2)
            .unwrap();
        // The names pyarrow gives a map's fields, as the hint does.
        let names = MapFieldNames {
            entry: "entries".into(),
            key: "key".into(),
            value: "value".into(),
        };
        let mut map = MapBuilder::new(Some(names), StringBuilder::new(), value);
        for (key, amount) in [("a", 125), ("b", 250)] {
            map.keys().append_value(key);
            map.values().append_value(amount);
            map.append(true).unwrap();
        }
        let map: ArrayRef = Arc::new(map.finish());
        let decimal32 = || Box::new(Ipc::Decimal(32));
        type Refused = (
            Vec<(&'static str, ArrayRef)>,
            Vec<(&'static str, Ipc)>,
            &'static str,
            &'static str,
        );
        let refused: [Refused; 6] = [
            (
                vec![("S", structure)],
                vec![("S", Ipc::Struct(vec![Ipc::Decimal(32)]))],
                "S",
                "struct<c0: decimal32(9, 2)>",
            ),
            (
                vec![("L", list.clone())],
                vec![("L", Ipc::List(decimal32()))],
                "L",
                "list<item: decimal32(9, 2)>",
            ),
            (
                vec![("L", list)],
                vec![("L", Ipc::ListView(decimal32()))],
                "L",
                "list_view<item: decimal32(9, 2)>",
            ),
            (
                vec![("L", large)],
                vec![("L", Ipc::LargeListView(Box::new(Ipc::Int64)))],
                "L",
                "large_list_view<item: int64>",
            ),
            (
                vec![("M", map)],
                vec![("M", Ipc::Map(Box::new(Ipc::Decimal(64))))],
                "M",
                "map<string, decimal64(9, 2) ('M')>",
            ),
            // An earlier refused field is still the one reported.
            (
                vec![
                    ("U", Arc::new(UInt64Array::from(vec![1, 2]))),
                    ("D", decimals()),
                ],
                vec![("U", Ipc::Int64), ("D", Ipc::Decimal(32))],
                "U",
                "uint64",
            ),
        ];
        for (columns, fields, field, stored_type) in refused {
            let result = parse(&hinted(columns, &fields));
            assert!(
                matches!(&result, Err(Parsed::Unsupported { field: f, stored_type: t })
                    if f == field && t == stored_type),
                "{stored_type}: {:?}",
                result.err()
            );
        }
        // A later type over a Parquet type the mapping admits does not apply.
        let int32: ArrayRef = Arc::new(Int32Array::from(vec![3, 4]));
        let int64: ArrayRef = Arc::new(Int64Array::from(vec![3, 4]));
        for (column, ipc) in [
            (int32, Ipc::Decimal(32)),
            (int64, Ipc::ListView(Box::new(Ipc::Int64))),
        ] {
            let table = parse(&hinted(vec![("X", column)], &[("X", ipc)])).unwrap();
            assert_eq!(table.schema().columns()[0].kind, ColumnType::Int);
            assert_eq!(table.row_count(), 2);
        }
    }

    /// A null fixed-size list has no size to check, so its field reaches the
    /// closed mapping and is refused with its stored type, as it is with no
    /// null: of integers or of a later decimal, held in a list, or in a
    /// struct that is null itself (PARQUET-7, #3366). An ODM input leaves it
    /// out as a vendor field (NAME-8). A list row of another size, an empty
    /// one too, is not valid Parquet (PARQUET-5). pyarrow 26 reads and
    /// refuses the same rows. Values are named `element`, as pyarrow names
    /// them.
    #[test]
    fn a_null_fixed_size_list_row_has_no_size_and_another_size_is_invalid() {
        use super::super::{parse, parse_odm, Error as Parsed};
        use arrow_array::{
            Array, ArrayRef, Decimal128Array, FixedSizeListArray, Int64Array, ListArray,
            RecordBatch, StringArray, StructArray,
        };
        use arrow_buffer::{NullBuffer, OffsetBuffer};
        use arrow_schema::{DataType, Field};
        use parquet::arrow::ArrowWriter;
        use std::sync::Arc;
        use yamaa_core::{odm_source::FIELDS, table::TableAccess};
        let element = |kind| Arc::new(Field::new("element", kind, true));
        // Two rows of two values; a null row keeps its two slots.
        let pairs = |values: ArrayRef, second: bool| -> ArrayRef {
            let field = element(values.data_type().clone());
            let nulls = NullBuffer::from(vec![true, second]);
            Arc::new(FixedSizeListArray::new(field, 2, values, Some(nulls)))
        };
        let integers = |second| pairs(Arc::new(Int64Array::from(vec![1, 2, 3, 4])), second);
        let decimals = Decimal128Array::from(vec![125, 250, 0, 0])
            .with_precision_and_scale(9, 2)
            .unwrap();
        let decimals = pairs(Arc::new(decimals), false);
        let pair = |ipc| Ipc::FixedSizeList(Box::new(ipc), 2);
        let kind = integers(true).data_type().clone();
        // A list whose first row holds a null fixed-size list, and a struct
        // of one; both are null in their second row.
        let outer: ArrayRef = Arc::new(ListArray::new(
            element(kind.clone()),
            OffsetBuffer::from_lengths([2, 0]),
            integers(false),
            Some(NullBuffer::from(vec![true, false])),
        ));
        let structure: ArrayRef = Arc::new(StructArray::new(
            vec![Field::new("c0", kind, true)].into(),
            vec![integers(true)],
            Some(NullBuffer::from(vec![true, false])),
        ));
        let refused: [(ArrayRef, Ipc, &str); 4] = [
            (
                integers(false),
                pair(Ipc::Int64),
                "fixed_size_list<element: int64>[2]",
            ),
            (
                decimals,
                pair(Ipc::Decimal(64)),
                "fixed_size_list<element: decimal64(9, 2)>[2]",
            ),
            (
                outer,
                Ipc::List(Box::new(pair(Ipc::Int64))),
                "list<element: fixed_size_list<element: int64>[2]>",
            ),
            (
                structure,
                Ipc::Struct(vec![pair(Ipc::Int64)]),
                "struct<c0: fixed_size_list<element: int64>[2]>",
            ),
        ];
        for (column, ipc, stored_type) in refused {
            let result = parse(&hinted(vec![("F", column)], &[("F", ipc)]));
            assert!(
                matches!(&result, Err(Parsed::Unsupported { field, stored_type: t })
                    if field == "F" && t == stored_type),
                "{stored_type}: {:?}",
                result.err()
            );
        }
        // The eleven ODM fields and the fixed-size list, with the writer's
        // own embedded schema.
        let text = || Arc::new(StringArray::from(vec!["a", "b"])) as ArrayRef;
        let fields = FIELDS.iter().map(|field| (*field, text(), true));
        let odm = fields.chain([("F", integers(false), true)]);
        let batch = RecordBatch::try_from_iter_with_nullable(odm).unwrap();
        let mut bytes = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
        let table = parse_odm(&bytes).unwrap();
        assert_eq!(table.schema().columns().len(), FIELDS.len());
        assert_eq!(table.row_count(), 2);
        // A list column whose embedded schema says that each row holds two
        // values: a null second row is refused by type, and an empty, short
        // or long one is invalid.
        let rows = |lengths: [usize; 2], second: bool| -> ArrayRef {
            let count = i64::try_from(lengths.iter().sum::<usize>()).unwrap();
            Arc::new(ListArray::new(
                element(DataType::Int64),
                OffsetBuffer::from_lengths(lengths),
                Arc::new(Int64Array::from_iter_values(1..=count)),
                Some(NullBuffer::from(vec![true, second])),
            ))
        };
        let hint = [("F", pair(Ipc::Int64))];
        let null = parse(&hinted(vec![("F", rows([2, 0], false))], &hint));
        assert!(
            matches!(&null, Err(Parsed::Unsupported { stored_type, .. })
                if stored_type == "fixed_size_list<element: int64>[2]"),
            "{:?}",
            null.err()
        );
        for lengths in [[2, 0], [2, 1], [2, 3]] {
            let result = parse(&hinted(vec![("F", rows(lengths, true))], &hint));
            assert!(
                matches!(&result, Err(Parsed::Malformed)),
                "{lengths:?}: {:?}",
                result.err()
            );
        }
    }
}
