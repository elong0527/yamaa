//! Parquet test inputs written at test time.
//!
//! Each named input holds the schema, the values and the stored types of the
//! file that pyarrow wrote for that name, which the repository once held
//! under `tests/fixtures/pq` (#3496): binary files live only under
//! `benchmarks/`. The parquet crate's `ArrowWriter` writes most inputs, with
//! the Arrow schema embedded and list and map fields named as pyarrow names
//! them, in the codecs, page versions, encodings and row groups that pyarrow
//! used. Its `SerializedFileWriter` writes a decimal in the fixed-length
//! bytes that pyarrow stores it in, though without the dictionary that the
//! parquet crate never writes for such bytes in V1 pages, and a row group of
//! no rows. A few files are edited after writing: an annotation that the
//! pinned parquet release cannot write (GEOMETRY, GEOGRAPHY, VARIANT), or a
//! corrupt payload. Where the embedded schema names a type that the pinned
//! Arrow release does not have, it is authored here. Column chunks carry
//! statistics and no page index, as pyarrow's did; page headers carry no
//! statistics. Nothing here reads a file with yamaa, so expected values stay
//! independent of the reader. `tools/staging/build_parquet_source_inputs.py`
//! writes the inputs that the installed host suites read.
use arrow_array::{
    builder::{Int64Builder, MapBuilder, MapFieldNames, StringBuilder},
    ArrayRef, BinaryArray, BooleanArray, Date32Array, Date64Array, FixedSizeListArray,
    Float32Array, Float64Array, Int32Array, Int64Array, LargeStringArray, ListArray, RecordBatch,
    StringArray, StructArray, TimestampMicrosecondArray, TimestampMillisecondArray, UInt64Array,
};
use arrow_buffer::OffsetBuffer;
use arrow_schema::{DataType, Field, Schema};
use base64::{prelude::BASE64_STANDARD, Engine};
use parquet::{
    arrow::{
        add_encoded_arrow_schema_to_metadata, arrow_writer::ArrowWriterOptions,
        ArrowSchemaConverter, ArrowWriter,
    },
    basic::{BrotliLevel, Compression, Encoding, GzipLevel, ZstdLevel},
    data_type::FixedLenByteArrayType,
    file::{
        metadata::KeyValue,
        properties::{EnabledStatistics, WriterProperties, WriterPropertiesBuilder, WriterVersion},
        writer::SerializedFileWriter,
    },
    schema::{parser::parse_message_type, types::ColumnPath},
};
use std::{collections::HashMap, sync::Arc};

/// The bytes of the input named `name`.
pub fn file(name: &str) -> Vec<u8> {
    if let Some(bytes) = ordinary(name) {
        return bytes;
    }
    if let Some(bytes) = unsupported(name) {
        return bytes;
    }
    let none = || base().set_compression(Compression::UNCOMPRESSED);
    let zstd = || base().set_compression(Compression::ZSTD(ZstdLevel::default()));
    match name {
        "delta" => {
            // Delta integers and strings and byte-stream-split doubles in
            // one row group of V2 pages.
            let mut properties = zstd()
                .set_writer_version(WriterVersion::PARQUET_2_0)
                .set_dictionary_enabled(false);
            for (column, encoding) in [
                ("S", Encoding::DELTA_BYTE_ARRAY),
                ("I", Encoding::DELTA_BINARY_PACKED),
                ("F", Encoding::BYTE_STREAM_SPLIT),
                ("D", Encoding::DELTA_BINARY_PACKED),
                ("T", Encoding::DELTA_BINARY_PACKED),
            ] {
                properties = properties.set_column_encoding(ColumnPath::from(column), encoding);
            }
            write(&ordinary_batch(), properties)
        }
        // pyarrow writes a table of no rows as one row group whose column
        // chunks each hold an empty dictionary page, no data page and no
        // statistics.
        "empty" => no_rows(
            &ordinary_batch().schema(),
            none().set_statistics_enabled(EnabledStatistics::None),
        ),
        "nonfinite" => {
            let values = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0];
            write(
                &batch(vec![("F", array(Float64Array::from(values.to_vec())))]),
                none(),
            )
        }
        "text" => write(
            &batch(vec![
                ("I", array(Int64Array::from(vec![1, 2]))),
                ("S", array(StringArray::from(vec!["", "X"]))),
            ]),
            none(),
        ),
        "boolean" => write(
            &batch(vec![("FLAG", array(BooleanArray::from(vec![true])))]),
            none(),
        ),
        "fractional" => write(
            &batch(vec![(
                "AT",
                array(TimestampMicrosecondArray::from(vec![1])),
            )]),
            none(),
        ),
        "bad-date" => write(
            &batch(vec![("D", array(Date32Array::from(vec![-719_163])))]),
            none(),
        ),
        "empty-name" => write(&batch(vec![("", array(Int64Array::from(vec![1])))]), none()),
        "duplicate" => write(
            &batch(vec![
                ("I", array(Int64Array::from(vec![1]))),
                ("I", array(Int64Array::from(vec![2]))),
            ]),
            none(),
        ),
        // The DATE that pyarrow stores for a date64, which the embedded
        // schema names.
        "date64" => write(
            &batch(vec![("D", array(Date64Array::from(vec![0])))]),
            none(),
        ),
        "mixed-struct" | "mixed-map" | "mixed-empty" | "mixed-duplicate" => {
            let integer = || array(Int64Array::from(vec![1]));
            let columns = match name {
                "mixed-struct" => {
                    let structure = StructArray::from(vec![
                        (
                            Arc::new(Field::new("left", DataType::Int64, true)),
                            array(Int64Array::from(vec![2])),
                        ),
                        (
                            Arc::new(Field::new("right", DataType::Utf8, true)),
                            array(StringArray::from(vec!["text"])),
                        ),
                    ]);
                    vec![
                        ("I", integer()),
                        ("S", array(structure)),
                        ("AFTER", integer()),
                    ]
                }
                "mixed-map" => vec![("I", integer()), ("M", map()), ("AFTER", integer())],
                "mixed-empty" => vec![("", integer()), ("M", map())],
                _ => vec![("I", integer()), ("I", integer()), ("M", map())],
            };
            write(&batch(columns), zstd())
        }
        // One string payload of each is replaced with bytes that are not
        // UTF-8, so neither a dictionary nor statistics may repeat it.
        "utf8" | "utf8-bool" | "utf8-time" => {
            let mut columns = match name {
                "utf8-bool" => vec![("OTHER", array(BooleanArray::from(vec![true])))],
                "utf8-time" => vec![("OTHER", array(TimestampMicrosecondArray::from(vec![1])))],
                _ => Vec::new(),
            };
            columns.push(("TEXT", array(StringArray::from(vec!["ok"]))));
            let properties = none()
                .set_dictionary_enabled(false)
                .set_statistics_enabled(EnabledStatistics::None);
            let bytes = write(&batch(columns), properties);
            replace(bytes, b"\x02\x00\x00\x00ok", b"\x02\x00\x00\x00\xff\xff")
        }
        "odm-vendor" => odm(&odm_vendor()),
        // The vendor binary value claims more bytes than its page holds.
        "odm-vendor-malformed" => replace(
            odm(&odm_vendor()),
            b"\x10\x00\x00\x00\xff\xfeVENDORPHYSICAL",
            b"\xff\xff\xff\xff\xff\xfeVENDORPHYSICAL",
        ),
        "odm-missing" => {
            let mut columns = odm_columns();
            columns.retain(|(name, _)| !name.starts_with("Form"));
            odm(&batch(columns))
        }
        "odm-wrong-type" => {
            let mut columns = odm_columns();
            columns[10].1 = array(Int64Array::from(vec![34]));
            odm(&batch(columns))
        }
        "odm-ambiguous" => {
            let mut columns = odm_columns();
            columns.push(("studyoid", array(StringArray::from(vec!["S"]))));
            odm(&batch(columns))
        }
        "odm-empty" => {
            let columns = ODM_FIELDS
                .iter()
                .map(|name| Field::new(*name, DataType::Utf8, true))
                .collect::<Vec<_>>();
            no_rows(&Arc::new(Schema::new(columns)), odm_properties())
        }
        _ => panic!("no Parquet test input is named {name}"),
    }
}

/// `{codec}-{page version}-{dictionary}`: the five ordinary columns in row
/// groups of two rows, each column with one page per row group.
fn ordinary(name: &str) -> Option<Vec<u8>> {
    let mut parts = name.split('-');
    let compression = match parts.next()? {
        "none" => Compression::UNCOMPRESSED,
        "snappy" => Compression::SNAPPY,
        "gzip" => Compression::GZIP(GzipLevel::default()),
        "brotli" => Compression::BROTLI(BrotliLevel::default()),
        // pyarrow's LZ4 is the raw block format.
        "lz4" => Compression::LZ4_RAW,
        "zstd" => Compression::ZSTD(ZstdLevel::default()),
        _ => return None,
    };
    let version = match parts.next()? {
        "1" => WriterVersion::PARQUET_1_0,
        "2" => WriterVersion::PARQUET_2_0,
        _ => return None,
    };
    let dictionary = match parts.next()? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    let properties = base()
        .set_compression(compression)
        .set_writer_version(version)
        .set_dictionary_enabled(dictionary)
        // pyarrow encodes values PLAIN in either page version.
        .set_encoding(Encoding::PLAIN)
        .set_max_row_group_size(2);
    Some(write(&ordinary_batch(), properties))
}

/// Full i64, signed zero, the smallest binary64 subnormal, empty, NUL and
/// non-ASCII text, a missing row and both civil calendar bounds.
fn ordinary_batch() -> RecordBatch {
    batch(vec![
        (
            "S",
            array(LargeStringArray::from(vec![
                Some(""),
                None,
                Some("\u{3b2}\0z"),
            ])),
        ),
        (
            "I",
            array(Int64Array::from(vec![Some(i64::MIN), None, Some(i64::MAX)])),
        ),
        (
            "F",
            array(Float64Array::from(vec![
                Some(-0.0),
                None,
                Some(f64::from_bits(1)),
            ])),
        ),
        (
            "D",
            array(Date32Array::from(vec![
                Some(-719_162),
                None,
                Some(2_932_896),
            ])),
        ),
        (
            "T",
            array(TimestampMicrosecondArray::from(vec![
                Some(-1_000_000),
                None,
                Some(253_402_300_799_000_000),
            ])),
        ),
    ])
}

/// `unsupported-{name}`: one row of one field, `FIELD`, of a type that the
/// closed mapping refuses, or of an unannotated INT32.
fn unsupported(name: &str) -> Option<Vec<u8>> {
    let name = name.strip_prefix("unsupported-")?;
    let properties = || base().set_compression(Compression::ZSTD(ZstdLevel::default()));
    let one = |column: ArrayRef| write(&batch(vec![("FIELD", column)]), properties());
    let item = || Arc::new(Field::new("item", DataType::Int64, true));
    let list = || {
        let values = array(Int64Array::from(vec![1]));
        array(ListArray::new(
            item(),
            OffsetBuffer::from_lengths([1]),
            values,
            None,
        ))
    };
    // The well-known binary of the point (1 2), as a GeoArrow WKB value,
    // with no statistics, as pyarrow writes a geospatial column.
    let point = |metadata: &str| {
        let point =
            b"\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\xf0?\x00\x00\x00\x00\x00\x00\x00@";
        let field = Field::new("FIELD", DataType::Binary, true).with_metadata(HashMap::from([
            (
                "ARROW:extension:name".to_string(),
                "geoarrow.wkb".to_string(),
            ),
            ("ARROW:extension:metadata".to_string(), metadata.to_string()),
        ]));
        let schema = Arc::new(Schema::new(vec![field]));
        let column = array(BinaryArray::from(vec![&point[..]]));
        let batch = RecordBatch::try_new(schema, vec![column]).unwrap();
        let properties = properties().set_statistics_enabled(EnabledStatistics::None);
        write(&batch, properties)
    };
    Some(match name {
        "int32" => one(array(Int32Array::from(vec![1]))),
        "uint64" => one(array(UInt64Array::from(vec![1]))),
        "float32" => one(array(Float32Array::from(vec![1.25]))),
        "binary" => one(array(BinaryArray::from(vec![&b"x"[..]]))),
        "milliseconds" => one(array(TimestampMillisecondArray::from(vec![1000]))),
        "timezone" => one(array(
            TimestampMicrosecondArray::from(vec![1_000_000]).with_timezone("UTC"),
        )),
        "list" => one(list()),
        "fixed-list" => {
            let values = array(Int64Array::from(vec![1, 2]));
            one(array(FixedSizeListArray::new(item(), 2, values, None)))
        }
        "struct" => one(array(StructArray::from(vec![(
            item(),
            array(Int64Array::from(vec![1])),
        )]))),
        // pyarrow stores each decimal in the fewest bytes that hold its
        // precision; the parquet crate would store these as integers.
        "decimal" => {
            let field = Field::new("FIELD", DataType::Decimal128(10, 2), true);
            let mut properties = properties().build();
            add_encoded_arrow_schema_to_metadata(&Schema::new(vec![field]), &mut properties);
            decimal(5, 10, properties)
        }
        // Types that the pinned Arrow release does not have, named only in
        // the embedded schema, as pyarrow 26 writes them (#3155).
        "decimal32" | "decimal64" => {
            let (width, precision, bits) = if name == "decimal32" {
                (4, 9, 32)
            } else {
                (8, 18, 64)
            };
            let hint = later(Later::Decimal { bits, precision });
            let properties = properties().set_key_value_metadata(Some(vec![hint]));
            decimal(width, precision, properties.build())
        }
        "list-view" | "large-list-view" => {
            let kind = if name == "list-view" {
                Later::ListView
            } else {
                Later::LargeListView
            };
            let properties = properties().set_key_value_metadata(Some(vec![later(kind)]));
            let options = ArrowWriterOptions::new().with_skip_arrow_metadata(true);
            write_with(&batch(vec![("FIELD", list())]), properties, options)
        }
        // Logical types that the pinned parquet release does not know
        // (#3155), annotated in the footer after writing.
        "geometry" => annotate(point("{}"), b"\x18\x05FIELD\x00", &[0x6c, 0x0c, 0x22, 0, 0]),
        "geography" => annotate(
            point(r#"{"edges":"spherical"}"#),
            b"\x18\x05FIELD\x00",
            &[0x6c, 0x0c, 0x24, 0, 0],
        ),
        // A VARIANT group: the struct of its two binary fields, the group
        // annotated in the footer with specification version 1.
        "variant" => {
            let structure = StructArray::from(vec![
                (
                    Arc::new(Field::new("metadata", DataType::Binary, false)),
                    array(BinaryArray::from(vec![&b"\x01\x00\x00"[..]])),
                ),
                (
                    Arc::new(Field::new("value", DataType::Binary, false)),
                    array(BinaryArray::from(vec![&b"\x0c\x01"[..]])),
                ),
            ]);
            annotate(
                one(array(structure)),
                b"\x18\x05FIELD\x15\x04\x00",
                &[0x5c, 0x0c, 0x20, 0x13, 1, 0, 0],
            )
        }
        _ => return None,
    })
}

/// pyarrow's choices where they differ from the parquet crate's: list and
/// map fields named `element` and `key_value`, a date64 stored as DATE, a
/// root named `schema`, and statistics for each column chunk alone.
fn base() -> WriterPropertiesBuilder {
    WriterProperties::builder()
        .set_coerce_types(true)
        .set_statistics_enabled(EnabledStatistics::Chunk)
        .set_offset_index_disabled(true)
}

fn array(array: impl arrow_array::Array + 'static) -> ArrayRef {
    Arc::new(array)
}

/// A batch of nullable columns, in order; names may repeat or be empty.
fn batch(columns: Vec<(&str, ArrayRef)>) -> RecordBatch {
    let columns = columns
        .into_iter()
        .map(|(name, column)| (name, column, true));
    RecordBatch::try_from_iter_with_nullable(columns).unwrap()
}

fn write(batch: &RecordBatch, properties: WriterPropertiesBuilder) -> Vec<u8> {
    write_with(batch, properties, ArrowWriterOptions::new())
}

fn write_with(
    batch: &RecordBatch,
    properties: WriterPropertiesBuilder,
    options: ArrowWriterOptions,
) -> Vec<u8> {
    let options = options
        .with_properties(properties.build())
        .with_schema_root("schema".into());
    let mut bytes = Vec::new();
    let mut writer =
        ArrowWriter::try_new_with_options(&mut bytes, batch.schema(), options).unwrap();
    writer.write(batch).unwrap();
    writer.close().unwrap();
    bytes
}

/// One row group of no rows under `schema`, its column chunks closed with
/// nothing written.
fn no_rows(schema: &Schema, properties: WriterPropertiesBuilder) -> Vec<u8> {
    let parquet = ArrowSchemaConverter::new()
        .with_coerce_types(true)
        .schema_root("schema")
        .convert(schema)
        .unwrap();
    let mut properties = properties.build();
    add_encoded_arrow_schema_to_metadata(schema, &mut properties);
    let root = parquet.root_schema_ptr();
    let mut writer = SerializedFileWriter::new(Vec::new(), root, Arc::new(properties)).unwrap();
    let mut group = writer.next_row_group().unwrap();
    while let Some(column) = group.next_column().unwrap() {
        column.close().unwrap();
    }
    group.close().unwrap();
    writer.into_inner().unwrap()
}

/// One row of `FIELD`, the decimal 1.23 in `width` bytes of precision
/// `precision` and scale 2.
fn decimal(width: usize, precision: i32, properties: WriterProperties) -> Vec<u8> {
    let schema = format!(
        "message schema {{ optional fixed_len_byte_array({width}) FIELD (DECIMAL({precision},2)); }}"
    );
    let schema = Arc::new(parse_message_type(&schema).unwrap());
    let mut writer = SerializedFileWriter::new(Vec::new(), schema, Arc::new(properties)).unwrap();
    let mut group = writer.next_row_group().unwrap();
    let mut column = group.next_column().unwrap().unwrap();
    let value = 123i128.to_be_bytes()[16 - width..].to_vec();
    column
        .typed::<FixedLenByteArrayType>()
        .write_batch(&[value.into()], Some(&[1]), None)
        .unwrap();
    column.close().unwrap();
    group.close().unwrap();
    writer.into_inner().unwrap()
}

/// A type of `FIELD` that the pinned Arrow release does not have.
enum Later {
    Decimal { bits: i32, precision: i32 },
    ListView,
    LargeListView,
}

/// Key-value metadata embedding a schema of one nullable field, `FIELD`, of
/// type `kind`, as writers embed one: an encapsulated IPC message, its
/// continuation marker, length and flatbuffer padded to eight bytes.
fn later(kind: Later) -> KeyValue {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let children = match kind {
        Later::Decimal { .. } => Vec::new(),
        Later::ListView | Later::LargeListView => {
            let args = arrow_ipc::IntArgs {
                bitWidth: 64,
                is_signed: true,
            };
            let int = arrow_ipc::Int::create(&mut builder, &args);
            let children = builder.create_vector::<flatbuffers::WIPOffset<arrow_ipc::Field>>(&[]);
            let name = builder.create_string("item");
            let args = arrow_ipc::FieldArgs {
                name: Some(name),
                nullable: true,
                type_type: arrow_ipc::Type::Int,
                type_: Some(int.as_union_value()),
                children: Some(children),
                ..Default::default()
            };
            vec![arrow_ipc::Field::create(&mut builder, &args)]
        }
    };
    let children = builder.create_vector(&children);
    let (type_type, type_) = match kind {
        Later::Decimal { bits, precision } => {
            let args = arrow_ipc::DecimalArgs {
                precision,
                scale: 2,
                bitWidth: bits,
            };
            let decimal = arrow_ipc::Decimal::create(&mut builder, &args);
            (arrow_ipc::Type::Decimal, decimal.as_union_value())
        }
        Later::ListView => {
            let view = arrow_ipc::ListView::create(&mut builder, &Default::default());
            (arrow_ipc::Type::ListView, view.as_union_value())
        }
        Later::LargeListView => {
            let view = arrow_ipc::LargeListView::create(&mut builder, &Default::default());
            (arrow_ipc::Type::LargeListView, view.as_union_value())
        }
    };
    let name = builder.create_string("FIELD");
    let args = arrow_ipc::FieldArgs {
        name: Some(name),
        nullable: true,
        type_type,
        type_: Some(type_),
        children: Some(children),
        ..Default::default()
    };
    let field = arrow_ipc::Field::create(&mut builder, &args);
    let fields = builder.create_vector(&[field]);
    let args = arrow_ipc::SchemaArgs {
        endianness: arrow_ipc::Endianness::Little,
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
    KeyValue::new("ARROW:schema".into(), BASE64_STANDARD.encode(encapsulated))
}

/// `bytes` with `annotation` inserted before the STOP byte that ends
/// `element`, the one schema element of the footer that ends so.
fn annotate(bytes: Vec<u8>, element: &[u8], annotation: &[u8]) -> Vec<u8> {
    let end = bytes.len() - 8;
    let length = u32::from_le_bytes(bytes[end..end + 4].try_into().unwrap()) as usize;
    let start = end - length;
    let found: Vec<usize> = bytes[start..end]
        .windows(element.len())
        .enumerate()
        .filter(|(_, window)| *window == element)
        .map(|(index, _)| start + index + element.len() - 1)
        .collect();
    let [stop] = found[..] else {
        panic!("{} schema elements end {element:?}", found.len())
    };
    let mut edited = bytes[..stop].to_vec();
    edited.extend(annotation);
    edited.extend(&bytes[stop..end]);
    edited.extend(
        u32::try_from(length + annotation.len())
            .unwrap()
            .to_le_bytes(),
    );
    edited.extend(b"PAR1");
    edited
}

/// `bytes` with the one occurrence of `from` replaced by `to`, as long.
fn replace(mut bytes: Vec<u8>, from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let found: Vec<usize> = bytes
        .windows(from.len())
        .enumerate()
        .filter(|(_, window)| *window == from)
        .map(|(index, _)| index)
        .collect();
    let [at] = found[..] else {
        panic!("{} occurrences of {from:?}", found.len())
    };
    bytes[at..at + to.len()].copy_from_slice(to);
    bytes
}

/// A map of one row holding the one entry `key: 2`, its fields named as
/// pyarrow names them.
fn map() -> ArrayRef {
    let names = MapFieldNames {
        entry: "entries".into(),
        key: "key".into(),
        value: "value".into(),
    };
    let mut map = MapBuilder::new(Some(names), StringBuilder::new(), Int64Builder::new());
    map.keys().append_value("key");
    map.values().append_value(2);
    map.append(true).unwrap();
    array(map.finish())
}

/// The eleven ODM fields, in their order.
const ODM_FIELDS: [&str; 11] = [
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "FormOID",
    "FormRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
    "ItemOID",
    "Value",
];

/// One item's values, the three repeat keys missing.
const ODM_VALUES: [Option<&str>; 11] = [
    Some("S"),
    Some("M"),
    Some("P"),
    Some("E"),
    None,
    Some("F"),
    None,
    Some("G"),
    None,
    Some("IT.TEST.OID"),
    Some("Y"),
];

fn large_text(value: Option<&str>) -> ArrayRef {
    array(LargeStringArray::from(vec![value]))
}

fn odm_columns() -> Vec<(&'static str, ArrayRef)> {
    ODM_FIELDS
        .iter()
        .zip(ODM_VALUES)
        .map(|(name, value)| (*name, large_text(value)))
        .collect()
}

/// The ODM fields under names of inverted case, then three vendor fields: a
/// boolean, a list of integers and binary bytes that are not UTF-8.
fn odm_vendor() -> RecordBatch {
    let names: Vec<String> = ODM_FIELDS
        .iter()
        .map(|name| {
            name.chars()
                .map(|letter| {
                    if letter.is_ascii_uppercase() {
                        letter.to_ascii_lowercase()
                    } else {
                        letter.to_ascii_uppercase()
                    }
                })
                .collect()
        })
        .collect();
    let mut columns: Vec<(&str, ArrayRef)> = names
        .iter()
        .zip(ODM_VALUES)
        .map(|(name, value)| (name.as_str(), large_text(value)))
        .collect();
    let item = Arc::new(Field::new("item", DataType::Int32, true));
    let values = array(Int32Array::from(vec![1, 2]));
    let list = ListArray::new(item, OffsetBuffer::from_lengths([2]), values, None);
    columns.extend([
        ("VendorBool", array(BooleanArray::from(vec![false]))),
        ("VendorList", array(list)),
        (
            "VendorBinary",
            array(BinaryArray::from(vec![&b"\xff\xfeVENDORPHYSICAL"[..]])),
        ),
    ]);
    batch(columns)
}

/// Uncompressed V2 pages of PLAIN values, booleans RLE, with neither a
/// dictionary nor statistics.
fn odm_properties() -> WriterPropertiesBuilder {
    base()
        .set_compression(Compression::UNCOMPRESSED)
        .set_writer_version(WriterVersion::PARQUET_2_0)
        .set_dictionary_enabled(false)
        .set_statistics_enabled(EnabledStatistics::None)
        .set_encoding(Encoding::PLAIN)
        .set_column_encoding(ColumnPath::from("VendorBool"), Encoding::RLE)
}

fn odm(batch: &RecordBatch) -> Vec<u8> {
    write(batch, odm_properties())
}
