//! Table contracts: parquet source.
//!
//! The seam is `parquet_source::parse` over bytes written or authored field by
//! field in the test, its typed `Error` and the Arrow layouts it admits, which
//! neither the benchmark reader nor the host fixtures can write.
mod table {
    #[cfg(feature = "parquet")]
    mod parquet_source {
        //! Parquet inputs written at test time exercise the held-byte input codec.
        mod unknown_fields {
            //! Independent Compact wire extensions of the uncompressed V1 input.
            //! No candidate writer supplies either the extension bytes or expected cells.
            use std::sync::LazyLock;
            use yamaa_adapters::parquet_source::{parse, Error};
            use yamaa_core::{
                table::{TableAccess, ValueRef},
                temporal::Precision,
                value::ColumnType,
            };

            static SOURCE: LazyLock<Vec<u8>> =
                LazyLock::new(|| crate::support::parquet_files::file("none-1-0"));

            fn extended(kind: u8, payload: &[u8]) -> Vec<u8> {
                // A known field follows the unknown value: the footer's last
                // field, its column orders, moved after it under an explicit
                // id 7. Admission refuses a repeated known field, and the
                // footer holds every known field once.
                let end = SOURCE.len() - 8;
                let length = u32::from_le_bytes(SOURCE[end..end + 4].try_into().unwrap()) as usize;
                let start = end - length;
                // created_by, then column_orders: a 16-byte list, then STOP.
                let orders = end - 18;
                assert_eq!(SOURCE[orders], 0x19);
                let mut bytes = SOURCE[..orders].to_vec();
                bytes.extend([kind, 0xe0, 0xd4, 3]); // explicit unknown field 30000
                bytes.extend(payload);
                bytes.extend([0x09, 14]); // explicit field 7, a list
                bytes.extend(&SOURCE[orders + 1..end]);
                let length = u32::try_from(bytes.len() - start).unwrap();
                bytes.extend(length.to_le_bytes());
                bytes.extend(b"PAR1");
                assert_eq!(&bytes[..start], &SOURCE[..start]); // preserve all data pages
                bytes
            }

            fn extended_with_suffix(kind: u8, payload: &[u8], suffix: &[u8]) -> Vec<u8> {
                let end = SOURCE.len() - 8;
                let length = u32::from_le_bytes(SOURCE[end..end + 4].try_into().unwrap()) as usize;
                let start = end - length;
                assert_eq!(SOURCE[end - 1], 0); // root FileMetaData STOP
                let mut bytes = SOURCE[..end - 1].to_vec();
                bytes.extend([kind, 0xe0, 0xd4, 3]); // explicit unknown field 30000
                bytes.extend(payload);
                bytes.extend(suffix);
                bytes.push(0);
                let length = u32::try_from(bytes.len() - start).unwrap();
                bytes.extend(length.to_le_bytes());
                bytes.extend(b"PAR1");
                assert_eq!(&bytes[..start], &SOURCE[..start]); // preserve all data pages
                bytes
            }

            #[test]
            fn exact_reported_boolean_list_footer_preserves_every_cell() {
                let mut list = vec![0xf1, 0x80, 0x80, 4];
                for _ in 0..32_768 {
                    list.extend([1, 2]);
                }
                let bytes = extended_with_suffix(9, &list, &[]);
                // The reported footer: the source's, with an unknown field of
                // 65,536 booleans before its STOP, eight header bytes and
                // one byte for each boolean.
                let footer = |bytes: &[u8]| {
                    let end = bytes.len() - 8;
                    u32::from_le_bytes(bytes[end..end + 4].try_into().unwrap()) as usize
                };
                assert_eq!(bytes.len(), SOURCE.len() + 65_544);
                assert_eq!(footer(&bytes), footer(&SOURCE) + 65_544);
                assert!(footer(&bytes) > 65_536);
                exact_cells(&bytes);
            }

            fn exact_cells(bytes: &[u8]) {
                let table = parse(bytes).unwrap();
                assert_eq!(table.row_count(), 3);
                assert_eq!(
                    table
                        .schema()
                        .columns()
                        .iter()
                        .map(|column| (column.name.as_str(), column.kind))
                        .collect::<Vec<_>>(),
                    [
                        ("S", ColumnType::Str),
                        ("I", ColumnType::Int),
                        ("F", ColumnType::Float),
                        ("D", ColumnType::Date),
                        ("T", ColumnType::DateTime),
                    ]
                );
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""));
                assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str("β\0z"));
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN));
                assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX));
                for column in 0..5 {
                    assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing);
                }
                for (row, bits) in [(0, (-0.0f64).to_bits()), (2, 1)] {
                    let ValueRef::Float(value) = table.cell(row, 2).unwrap() else {
                        panic!("float cell")
                    };
                    assert_eq!(value.get().to_bits(), bits);
                }
                for (row, text) in [(0, "0001-01-01"), (2, "9999-12-31")] {
                    let ValueRef::Date(value) = table.cell(row, 3).unwrap() else {
                        panic!("date cell")
                    };
                    assert_eq!(value.to_string(), text);
                    assert_eq!(value.collected_precision(), Precision::Day);
                }
                for (row, fields) in [
                    (0, (1969, 12, 31, 23, 59, 59)),
                    (2, (9999, 12, 31, 23, 59, 59)),
                ] {
                    let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                        panic!("datetime cell")
                    };
                    assert_eq!(value.fields(), fields);
                    assert_eq!(value.collected_precision(), Precision::Second);
                }
            }

            #[test]
            fn unknown_boolean_lists_sets_and_maps_preserve_all_fifteen_cells() {
                for count in [0, 1, 14, 15, 65_536] {
                    let mut list = if count < 15 {
                        vec![(count as u8) << 4 | 1]
                    } else if count == 15 {
                        vec![0xf1, 15]
                    } else {
                        vec![0xf1, 0x80, 0x80, 4]
                    };
                    list.extend((0..count).map(|index| 1 + (index % 2) as u8));
                    for kind in [9, 10] {
                        exact_cells(&extended(kind, &list));
                    }
                }
                for kinds in [0x11, 0x13, 0x31] {
                    // bool/bool, bool/byte, byte/bool
                    let mut map = vec![0x80, 0x80, 4, kinds]; // 65,536 pairs
                    for _ in 0..65_536 {
                        map.extend([1, 2]);
                    }
                    exact_cells(&extended(11, &map));
                }
                exact_cells(&extended(11, &[0])); // empty map has no type header
            }

            #[test]
            fn unknown_uuid_values_preserve_all_fifteen_cells() {
                exact_cells(&extended(13, &[7; 16]));
                let mut list = vec![0x2d]; // list<uuid> with two elements
                list.extend([7; 32]);
                exact_cells(&extended(9, &list));
            }

            #[test]
            fn nested_unknown_boolean_values_keep_inline_booleans_and_later_known_fields() {
                // list<struct> = [{true, false, map<bool,list<bool>>={false:[true,false]}}]
                exact_cells(&extended(
                    9,
                    &[0x1c, 0x11, 0x12, 0x1b, 1, 0x19, 2, 0x21, 1, 2, 0],
                ));
                // map<list<bool>,map<byte,bool>> = {[false,true]: {-1:false}}
                exact_cells(&extended(11, &[1, 0x9b, 0x21, 2, 1, 1, 0x31, 0xff, 2]));

                // Append an unknown bool list inside the known root SchemaElement, before
                // the next schema element. Check the written wire prefix before editing:
                // version 1, six schema elements, the root's name, five children, STOP.
                let end = SOURCE.len() - 8;
                let length = u32::from_le_bytes(SOURCE[end..end + 4].try_into().unwrap());
                let start = end - length as usize;
                assert_eq!(
                    &SOURCE[start..start + 15],
                    &[
                        0x15, 2, 0x19, 0x6c, 0x48, 6, b's', b'c', b'h', b'e', b'm', b'a', 0x15, 10,
                        0
                    ]
                );
                let mut bytes = SOURCE[..start + 14].to_vec();
                bytes.extend([9, 0xe0, 0xd4, 3, 0x21, 1, 2]);
                bytes.extend(&SOURCE[start + 14..end]);
                bytes.extend((length + 7).to_le_bytes());
                bytes.extend(b"PAR1");
                exact_cells(&bytes);
            }

            #[test]
            fn malformed_boolean_collections_depth_and_footer_extent_remain_refused() {
                for (kind, payload) in [
                    (9, &[0x21, 1, 3][..]),                   // invalid list boolean
                    (10, &[0x11, 0]),                         // invalid set boolean
                    (11, &[1, 0x11, 3, 2]),                   // invalid boolean key
                    (11, &[1, 0x11, 1, 3]),                   // invalid boolean value
                    (9, &[0x12, 1]),        // false is not a collection element type
                    (11, &[1, 0x21, 2, 1]), // invalid map key type
                    (9, &[0xf1, 0xff, 0xff, 0xff, 0xff, 7]), // count exceeds held bytes
                    (11, &[0xff, 0xff, 0xff, 0xff, 7, 0x11]), // map extent impossible
                    (9, &[0xf1, 0x80, 0x80, 0x80, 0x80, 8]), // count exceeds i32
                ] {
                    assert!(matches!(
                        parse(&extended(kind, payload)),
                        Err(Error::Malformed)
                    ));
                }
                let depth = yamaa_core::limits::Limits::SEMANTIC_DEPTH;
                let mut nested = vec![0x1c; depth];
                nested.extend([0x19, 0x11, 1]); // a bool collection beyond the depth limit
                nested.extend(vec![0; depth + 1]);
                assert!(matches!(
                    parse(&extended(12, &nested)),
                    Err(Error::Malformed)
                ));

                let valid = extended(9, &[0x21, 1, 2]);
                let mut impossible_page = valid.clone();
                // The first DATA_PAGE has a ten-byte body: extend both size claims
                // without changing its held chunk boundary or the valid unknown field.
                assert_eq!(&impossible_page[4..10], &[0x15, 0, 0x15, 20, 0x15, 20]);
                impossible_page[7] = 126;
                impossible_page[9] = 126;
                assert!(matches!(parse(&impossible_page), Err(Error::Malformed)));
                for removed in [1, 8, 9, 12] {
                    assert!(matches!(
                        parse(&valid[..valid.len() - removed]),
                        Err(Error::Malformed)
                    ));
                }
                let mut impossible_footer = valid;
                let end = impossible_footer.len() - 8;
                impossible_footer[end..end + 4].copy_from_slice(&u32::MAX.to_le_bytes());
                assert!(matches!(parse(&impossible_footer), Err(Error::Malformed)));
            }

            #[test]
            fn known_schema_and_row_group_boolean_lists_remain_type_errors() {
                for footer in [
                    // version=1; schema=[true,false]; num_rows=0; row_groups=[]
                    &[0x15, 2, 0x19, 0x21, 1, 2, 0x16, 0, 0x19, 0x0c, 0][..],
                    // schema=[{num_children:0}]; num_rows=0; row_groups=[true,false]
                    &[
                        0x15, 2, 0x19, 0x1c, 0x55, 0, 0, 0x16, 0, 0x19, 0x21, 1, 2, 0,
                    ],
                ] {
                    let mut bytes = b"PAR1".to_vec();
                    bytes.extend(footer);
                    bytes.extend((footer.len() as u32).to_le_bytes());
                    bytes.extend(b"PAR1");
                    assert!(matches!(parse(&bytes), Err(Error::Malformed)));
                }
            }
        }
        use crate::support::parquet_files::file as content;
        use yamaa_adapters::parquet_source::{parse, Error};
        use yamaa_core::{
            table::{TableAccess, ValueRef},
            temporal::Precision,
            value::ColumnType,
        };

        #[test]
        fn odm_metadata_admission_ignores_vendor_types_but_checks_every_physical_chunk() {
            use yamaa_adapters::parquet_source::parse_odm;
            use yamaa_core::odm_source::{SchemaError, FIELDS};
            let bytes = content("odm-vendor");
            let table = parse_odm(&bytes).unwrap();
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect::<Vec<_>>(),
                FIELDS.map(|name| (name, ColumnType::Str))
            );
            let expected = [
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
            for (column, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    table.cell(0, column).unwrap(),
                    expected.map_or(ValueRef::Missing, ValueRef::Str)
                );
            }
            // The ordinary closed profile retains its own independently qualified
            // unsupported vendor behavior; only the authored ODM interpretation projects.
            assert!(parse(&bytes).is_err());
            assert!(matches!(
                parse_odm(&content("odm-vendor-malformed")),
                Err(Error::Malformed)
            ));
            let expected = [
                (
                    "odm-wrong-type",
                    vec![SchemaError::Type {
                        field: "Value",
                        stored_field: "Value".into(),
                        kind: Some(ColumnType::Int),
                    }],
                ),
                (
                    "odm-ambiguous",
                    vec![SchemaError::Ambiguous {
                        field: "StudyOID",
                        stored: vec!["StudyOID".into(), "studyoid".into()],
                    }],
                ),
                (
                    "odm-missing",
                    vec![SchemaError::Missing {
                        fields: vec!["FormOID", "FormRepeatKey"],
                    }],
                ),
            ];
            for (name, expected) in expected {
                let Error::OdmFields(actual) = parse_odm(&content(name)).unwrap_err() else {
                    panic!("typed ODM field finding: {name}")
                };
                assert_eq!(actual, expected, "{name}");
            }
            let empty = parse_odm(&content("odm-empty")).unwrap();
            assert_eq!(empty.row_count(), 0);
            assert_eq!(empty.schema(), table.schema());
        }

        #[test]
        fn independent_codecs_page_versions_and_encodings_preserve_exact_values() {
            let mut cases = vec!["delta".to_string()];
            for codec in ["none", "snappy", "gzip", "brotli", "lz4", "zstd"] {
                for version in [1, 2] {
                    for dictionary in [0, 1] {
                        cases.push(format!("{codec}-{version}-{dictionary}"));
                    }
                }
            }
            for name in cases {
                let bytes = content(&name);
                let table = parse(&bytes).unwrap();
                drop(bytes);
                assert_eq!(table.row_count(), 3, "{name}");
                assert_eq!(
                    table
                        .schema()
                        .columns()
                        .iter()
                        .map(|c| (c.name.as_str(), c.kind))
                        .collect::<Vec<_>>(),
                    [
                        ("S", ColumnType::Str),
                        ("I", ColumnType::Int),
                        ("F", ColumnType::Float),
                        ("D", ColumnType::Date),
                        ("T", ColumnType::DateTime)
                    ]
                );
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""), "{name}");
                assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Str("β\0z"), "{name}");
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Int(i64::MIN), "{name}");
                assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Int(i64::MAX), "{name}");
                for column in 0..5 {
                    assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing, "{name}");
                }
                for (row, bits) in [(0, (-0.0f64).to_bits()), (2, 1)] {
                    let ValueRef::Float(value) = table.cell(row, 2).unwrap() else {
                        panic!("{name}")
                    };
                    assert_eq!(value.get().to_bits(), bits, "{name}");
                }
                for (row, text) in [(0, "0001-01-01"), (2, "9999-12-31")] {
                    let ValueRef::Date(value) = table.cell(row, 3).unwrap() else {
                        panic!("{name}")
                    };
                    assert_eq!(value.to_string(), text, "{name}");
                    assert_eq!(value.collected_precision(), Precision::Day);
                }
                for (row, fields) in [
                    (0, (1969, 12, 31, 23, 59, 59)),
                    (2, (9999, 12, 31, 23, 59, 59)),
                ] {
                    let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                        panic!("{name}")
                    };
                    assert_eq!(value.fields(), fields, "{name}");
                    assert_eq!(value.collected_precision(), Precision::Second);
                }
            }
        }

        #[test]
        fn empty_tables_and_nonfinite_inputs_keep_the_closed_schema_and_normalized_nulls() {
            let empty = parse(&content("empty")).unwrap();
            assert_eq!(empty.row_count(), 0);
            assert_eq!(empty.schema().columns().len(), 5);
            let table = parse(&content("nonfinite")).unwrap();
            for row in 0..3 {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
            }
            let ValueRef::Float(value) = table.cell(3, 0).unwrap() else {
                panic!()
            };
            assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
            // PyArrow restores the stored DATE as date32 even with an Arrow date64 hint.
            let date = parse(&content("date64")).unwrap();
            let ValueRef::Date(value) = date.cell(0, 0).unwrap() else {
                panic!()
            };
            assert_eq!(value.to_string(), "1970-01-01");
            assert_eq!(value.collected_precision(), Precision::Day);
        }

        #[test]
        fn unsupported_stored_types_retain_independent_arrow_type_spelling() {
            for (name, expected) in [
                ("uint64", "uint64"),
                ("float32", "float"),
                ("binary", "binary"),
                ("milliseconds", "timestamp[ms]"),
                ("timezone", "timestamp[us, tz=UTC]"),
                ("list", "list<element: int64>"),
                ("fixed-list", "fixed_size_list<element: int64>[2]"),
                ("struct", "struct<item: int64>"),
                ("decimal", "decimal128(10, 2)"),
                // Types that the pinned Arrow and Parquet releases do not
                // know, spelled as pyarrow 26 reads them (#3155).
                ("decimal32", "decimal32(9, 2)"),
                ("decimal64", "decimal64(18, 2)"),
                ("list-view", "list_view<element: int64>"),
                ("large-list-view", "large_list_view<element: int64>"),
                ("geometry", "binary"),
                ("geography", "binary"),
                (
                    "variant",
                    "struct<metadata: binary not null, value: binary not null>",
                ),
            ] {
                assert!(
                    matches!(parse(&content(&format!("unsupported-{name}"))),
                Err(Error::Unsupported {field, stored_type}) if field == "FIELD" && stored_type == expected),
                    "{name}"
                );
            }
        }

        #[test]
        fn the_pyarrow_int32_fixture_reads_as_int() {
            // An unannotated INT32, as pyarrow writes an int32 column, is a
            // signed 32-bit integer (PARQUET-7). The fixture keeps the name it
            // had while 0.4.0 refused it.
            let table = parse(&content("unsupported-int32")).unwrap();
            let column = &table.schema().columns()[0];
            assert_eq!(
                (column.name.as_str(), column.kind),
                ("FIELD", ColumnType::Int)
            );
            assert_eq!(table.row_count(), 1);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(1));
        }

        #[test]
        fn nested_roots_name_their_own_field_after_flat_columns_and_preserve_name_precedence() {
            for (name, field, stored_type) in [
                ("mixed-struct", "S", "struct<left: int64, right: string>"),
                ("mixed-map", "M", "map<string, int64 ('M')>"),
            ] {
                assert!(
                    matches!(parse(&content(name)),
            Err(Error::Unsupported { field: actual, stored_type: actual_type })
            if actual == field && actual_type == stored_type),
                    "{name}"
                );
            }
            assert!(matches!(
                parse(&content("mixed-empty")),
                Err(Error::EmptyName { field: 1 })
            ));
            assert!(
                matches!(parse(&content("mixed-duplicate")),Err(Error::DuplicateName{field}) if field=="I")
            );
        }

        #[test]
        fn field_and_temporal_diagnostics_precede_invalid_text() {
            for name in ["boolean", "utf8-bool"] {
                assert!(
                    matches!(parse(&content(name)), Err(Error::Unsupported { stored_type, .. }) if stored_type == "bool"),
                    "{name}"
                );
            }
            for name in ["fractional", "utf8-time"] {
                assert!(
                    matches!(
                        parse(&content(name)),
                        Err(Error::Value {
                            row: 1,
                            value: 1,
                            ..
                        })
                    ),
                    "{name}"
                );
            }
            assert!(matches!(
                parse(&content("bad-date")),
                Err(Error::Value {
                    row: 1,
                    value: -719_163,
                    ..
                })
            ));
            assert!(matches!(
                parse(&content("empty-name")),
                Err(Error::EmptyName { field: 1 })
            ));
            assert!(
                matches!(parse(&content("duplicate")), Err(Error::DuplicateName { field }) if field == "I")
            );
            assert!(matches!(parse(&content("utf8")), Err(Error::Malformed)));
            let mut broken = content("boolean");
            broken.truncate(broken.len() - 1);
            assert!(matches!(parse(&broken), Err(Error::Malformed)));
        }

        mod authored {
            //! Parquet metadata authored one Compact Protocol field at a time,
            //! for files the parquet crate's writers refuse to write.

            fn varint(mut value: u64, output: &mut Vec<u8>) {
                while value >= 0x80 {
                    output.push(value as u8 | 0x80);
                    value >>= 7;
                }
                output.push(value as u8);
            }

            pub(super) fn zigzag(value: i64) -> Vec<u8> {
                let mut bytes = Vec::new();
                varint(((value << 1) ^ (value >> 63)) as u64, &mut bytes);
                bytes
            }

            /// A Compact Protocol struct, written one field at a time.
            #[derive(Default)]
            pub(super) struct Fields {
                bytes: Vec<u8>,
                last: i16,
            }

            impl Fields {
                pub(super) fn raw(mut self, id: i16, kind: u8, value: &[u8]) -> Self {
                    match id - self.last {
                        delta @ 1..=15 => self.bytes.push((delta as u8) << 4 | kind),
                        _ => {
                            self.bytes.push(kind);
                            self.bytes.extend(zigzag(id.into()));
                        }
                    }
                    self.last = id;
                    self.bytes.extend(value);
                    self
                }

                pub(super) fn integer(self, id: i16, kind: u8, value: i64) -> Self {
                    self.raw(id, kind, &zigzag(value))
                }

                pub(super) fn binary(self, id: i16, value: &[u8]) -> Self {
                    let mut bytes = Vec::new();
                    varint(value.len() as u64, &mut bytes);
                    bytes.extend(value);
                    self.raw(id, 8, &bytes)
                }

                pub(super) fn boolean(self, id: i16, value: bool) -> Self {
                    self.raw(id, if value { 1 } else { 2 }, &[])
                }

                pub(super) fn strct(self, id: i16, value: Fields) -> Self {
                    self.raw(id, 12, &value.end())
                }

                /// A list of encoded elements of wire type `kind`.
                pub(super) fn list(self, id: i16, kind: u8, items: &[Vec<u8>]) -> Self {
                    let mut bytes = Vec::new();
                    if items.len() < 15 {
                        bytes.push((items.len() as u8) << 4 | kind);
                    } else {
                        bytes.push(0xf0 | kind);
                        varint(items.len() as u64, &mut bytes);
                    }
                    bytes.extend(items.concat());
                    self.raw(id, 9, &bytes)
                }

                pub(super) fn end(mut self) -> Vec<u8> {
                    self.bytes.push(0);
                    self.bytes
                }
            }
        }

        mod later_logical_types {
            //! The variant, geometry and geography logical types, which
            //! parquet 57 read and the pinned parquet 55 does not know, in the
            //! shapes beyond the one column of each `unsupported-*` input (#3155). No
            //! writer the tests can call writes them, so these files are
            //! authored here one Compact Protocol field at a time. Each field
            //! is refused as `source_field_type_unsupported` with the stored
            //! type that parquet 57 and pyarrow 26 give it, and each
            //! annotation that parquet 57 refused leaves the file invalid.
            use super::authored::{zigzag, Fields};
            use yamaa_adapters::parquet_source::{parse, Error};

            /// One REQUIRED leaf column of an authored file: its path, its
            /// physical type and the PLAIN bytes of its two values.
            struct Leaf {
                path: &'static [&'static str],
                physical: i64,
                body: Vec<u8>,
            }

            /// An INT64 leaf holding 1 and 2.
            fn int64(path: &'static [&'static str]) -> Leaf {
                let body = [1i64, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
                Leaf {
                    path,
                    physical: 2,
                    body,
                }
            }

            /// A BYTE_ARRAY leaf holding the well-known binary of the point
            /// (1 2) twice.
            fn points(path: &'static [&'static str]) -> Leaf {
                let point = b"\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\xf0?\x00\x00\x00\x00\x00\x00\x00@";
                let mut body = Vec::new();
                for _ in 0..2 {
                    body.extend((point.len() as u32).to_le_bytes());
                    body.extend(point);
                }
                Leaf {
                    path,
                    physical: 6,
                    body,
                }
            }

            /// A file of one row group of two rows of `leaves`, each in one
            /// uncompressed PLAIN V1 page, under the schema elements
            /// `schema`, the root first.
            fn container(schema: &[Vec<u8>], leaves: &[Leaf]) -> Vec<u8> {
                let mut bytes = b"PAR1".to_vec();
                let mut chunks = Vec::new();
                for leaf in leaves {
                    // Two values; PLAIN; RLE definition and repetition levels.
                    let data = Fields::default().integer(1, 5, 2).integer(2, 5, 0);
                    let data = data.integer(3, 5, 3).integer(4, 5, 3);
                    // DATA_PAGE; uncompressed and compressed sizes.
                    let size = leaf.body.len() as i64;
                    let page = Fields::default().integer(1, 5, 0).integer(2, 5, size);
                    let page = page.integer(3, 5, size).strct(5, data).end();
                    let offset = bytes.len() as i64;
                    let length = (page.len() + leaf.body.len()) as i64;
                    bytes.extend(page);
                    bytes.extend(&leaf.body);
                    let path: Vec<Vec<u8>> = leaf
                        .path
                        .iter()
                        .map(|name| [&[name.len() as u8][..], name.as_bytes()].concat())
                        .collect();
                    let meta = Fields::default()
                        .integer(1, 5, leaf.physical)
                        .list(2, 5, &[zigzag(0), zigzag(3)]) // PLAIN, RLE
                        .list(3, 8, &path)
                        .integer(4, 5, 0) // UNCOMPRESSED
                        .integer(5, 6, 2)
                        .integer(6, 6, length)
                        .integer(7, 6, length)
                        .integer(9, 6, offset);
                    let chunk = Fields::default().integer(2, 6, offset).strct(3, meta);
                    chunks.push(chunk.end());
                }
                let size = bytes.len() as i64 - 4;
                let group = Fields::default().list(1, 12, &chunks);
                let group = group.integer(2, 6, size).integer(3, 6, 2).end();
                let footer = Fields::default().integer(1, 5, 1).list(2, 12, schema);
                let footer = footer.integer(3, 6, 2).list(4, 12, &[group]).end();
                bytes.extend(&footer);
                bytes.extend((footer.len() as u32).to_le_bytes());
                bytes.extend(b"PAR1");
                bytes
            }

            fn root(children: i64) -> Vec<u8> {
                Fields::default()
                    .binary(4, b"schema")
                    .integer(5, 5, children)
                    .end()
            }

            /// A REQUIRED leaf element.
            fn leaf(name: &str, physical: i64) -> Fields {
                let leaf = Fields::default().integer(1, 5, physical).integer(3, 5, 0);
                leaf.binary(4, name.as_bytes())
            }

            /// A REQUIRED group element of `children` children.
            fn group(name: &str, children: i64) -> Fields {
                let group = Fields::default().integer(3, 5, 0);
                group.binary(4, name.as_bytes()).integer(5, 5, children)
            }

            /// A logical type union holding `variant`: 16 for VARIANT, 17 for
            /// GEOMETRY and 18 for GEOGRAPHY.
            fn logical(variant: i16, value: Fields) -> Fields {
                Fields::default().strct(variant, value)
            }

            fn geometry() -> Fields {
                logical(17, Fields::default().binary(1, b"OGC:CRS84"))
            }

            fn variant() -> Fields {
                logical(16, Fields::default().raw(1, 3, &[1]))
            }

            fn refused(bytes: &[u8]) -> (String, String) {
                match parse(bytes) {
                    Err(Error::Unsupported { field, stored_type }) => (field, stored_type),
                    other => panic!("{:?}", other.err()),
                }
            }

            #[test]
            fn each_annotated_field_is_refused_with_the_type_parquet_57_gave_it() {
                let metadata = || leaf("metadata", 6).end();
                let value = || leaf("value", 6).end();
                type Case = (
                    &'static str,
                    Vec<Vec<u8>>,
                    Vec<Leaf>,
                    &'static str,
                    &'static str,
                );
                let cases: [Case; 5] = [
                    (
                        "a geometry in a struct",
                        vec![
                            root(2),
                            leaf("V", 2).end(),
                            group("S", 1).end(),
                            leaf("g", 6).strct(10, geometry()).end(),
                        ],
                        vec![int64(&["V"]), points(&["S", "g"])],
                        "S",
                        "struct<g: binary not null>",
                    ),
                    (
                        "a geography",
                        vec![
                            root(2),
                            leaf("V", 2).end(),
                            leaf("G", 6)
                                .strct(10, logical(18, Fields::default().integer(2, 5, 0)))
                                .end(),
                        ],
                        vec![int64(&["V"]), points(&["G"])],
                        "G",
                        "binary",
                    ),
                    (
                        "a shredded variant",
                        vec![
                            root(2),
                            leaf("V", 2).end(),
                            group("X", 3).strct(10, variant()).end(),
                            metadata(),
                            value(),
                            leaf("typed_value", 2).end(),
                        ],
                        vec![
                            int64(&["V"]),
                            points(&["X", "metadata"]),
                            points(&["X", "value"]),
                            int64(&["X", "typed_value"]),
                        ],
                        "X",
                        "struct<metadata: binary not null, value: binary not null, \
                         typed_value: int64 not null>",
                    ),
                    // Parquet 57 checked nothing of a group's annotation.
                    (
                        "a geometry on a group",
                        vec![
                            root(1),
                            group("S", 1).strct(10, geometry()).end(),
                            leaf("g", 6).end(),
                        ],
                        vec![points(&["S", "g"])],
                        "S",
                        "struct<g: binary not null>",
                    ),
                    // The first field the closed mapping refuses is reported.
                    (
                        "a geometry before an unsigned field",
                        vec![
                            root(2),
                            leaf("G", 6).strct(10, geometry()).end(),
                            leaf("U", 2).integer(6, 5, 14).end(), // UINT_64
                        ],
                        vec![points(&["G"]), int64(&["U"])],
                        "G",
                        "binary",
                    ),
                ];
                for (name, schema, leaves, field, stored_type) in cases {
                    let bytes = container(&schema, &leaves);
                    assert_eq!(
                        refused(&bytes),
                        (field.into(), stored_type.into()),
                        "{name}"
                    );
                }
                // An earlier field's own finding still comes first.
                let schema = [
                    root(2),
                    leaf("", 2).end(),
                    leaf("G", 6).strct(10, geometry()).end(),
                ];
                let bytes = container(&schema, &[int64(&[""]), points(&["G"])]);
                assert!(matches!(parse(&bytes), Err(Error::EmptyName { field: 1 })));
            }

            #[test]
            fn an_annotation_parquet_57_refused_leaves_the_file_invalid() {
                type Invalid = (&'static str, Vec<Vec<u8>>, Vec<Leaf>);
                let cases: [Invalid; 4] = [
                    (
                        "a variant on a leaf",
                        vec![root(1), leaf("X", 6).strct(10, variant()).end()],
                        vec![points(&["X"])],
                    ),
                    (
                        "a geometry on an INT64",
                        vec![root(1), leaf("G", 2).strct(10, geometry()).end()],
                        vec![int64(&["G"])],
                    ),
                    (
                        "a geometry with a converted type",
                        vec![
                            root(1),
                            leaf("G", 6).integer(6, 5, 0).strct(10, geometry()).end(),
                        ],
                        vec![points(&["G"])],
                    ),
                    // A variant group whose converted type makes it a list
                    // that its two children cannot form.
                    (
                        "a variant marked as a list",
                        vec![
                            root(1),
                            group("X", 2).integer(6, 5, 3).strct(10, variant()).end(),
                            leaf("metadata", 6).end(),
                            leaf("value", 6).end(),
                        ],
                        vec![points(&["X", "metadata"]), points(&["X", "value"])],
                    ),
                ];
                for (name, schema, leaves) in cases {
                    let bytes = container(&schema, &leaves);
                    assert!(matches!(parse(&bytes), Err(Error::Malformed)), "{name}");
                }
            }
        }

        mod row_groups {
            //! More row groups than an `i16` ordinal numbers (#2940), one
            //! record per row group, as pyarrow and Polars write them with
            //! `row_group_size=1`. The parquet crate's writer refuses
            //! a 32,769th row group, so these files are authored here one
            //! Compact Protocol field at a time. Expected cells are the values
            //! the test writes, never read back from a candidate reader.
            use super::authored::{zigzag, Fields};
            use arrow_array::{ArrayRef, Int64Array, RecordBatch};
            use parquet::arrow::ArrowWriter;
            use std::sync::Arc;
            use yamaa_adapters::parquet_source::parse;
            use yamaa_core::{
                table::{TableAccess, ValueRef},
                value::ColumnType,
            };

            /// Which row groups hold their ordinal.
            #[derive(Clone, Copy, Debug)]
            enum Ordinals {
                /// None, as pyarrow writes them.
                Absent,
                /// The first 32,768, numbered 0 to 32,767, as Polars writes them.
                Polars,
            }

            /// The value of record `row`: its index, missing in every seventh
            /// record.
            fn value(row: usize) -> Option<i64> {
                (row % 7 != 6).then_some(row as i64)
            }

            /// `rows` records of one OPTIONAL INT64 field `RID` holding
            /// `value`, in one-row row groups. Each row group is one
            /// uncompressed PLAIN V1 page.
            fn file(rows: usize, ordinals: Ordinals) -> Vec<u8> {
                let mut bytes = b"PAR1".to_vec();
                let mut groups = Vec::with_capacity(rows);
                for row in 0..rows {
                    // Definition levels: their length, then one RLE run.
                    let value = value(row);
                    let mut body = vec![2, 0, 0, 0, 2, u8::from(value.is_some())];
                    body.extend(value.map(i64::to_le_bytes).unwrap_or_default());
                    // num_values; PLAIN; RLE definition and repetition levels.
                    let data = Fields::default().integer(1, 5, 1).integer(2, 5, 0);
                    let data = data.integer(3, 5, 3).integer(4, 5, 3);
                    // DATA_PAGE; uncompressed and compressed sizes.
                    let size = body.len() as i64;
                    let page = Fields::default().integer(1, 5, 0).integer(2, 5, size);
                    let page = page.integer(3, 5, size).strct(5, data).end();
                    let offset = bytes.len() as i64;
                    let length = (page.len() + body.len()) as i64;
                    bytes.extend(page);
                    bytes.extend(body);
                    let meta = Fields::default()
                        .integer(1, 5, 2) // INT64
                        .list(2, 5, &[zigzag(0), zigzag(3)]) // PLAIN, RLE
                        .list(3, 8, &[b"\x03RID".to_vec()]) // path_in_schema
                        .integer(4, 5, 0) // UNCOMPRESSED
                        .integer(5, 6, 1) // num_values
                        .integer(6, 6, length)
                        .integer(7, 6, length)
                        .integer(9, 6, offset); // data_page_offset
                    let chunk = Fields::default().integer(2, 6, 0).strct(3, meta);
                    let group = Fields::default().list(1, 12, &[chunk.end()]);
                    let group = group.integer(2, 6, length).integer(3, 6, 1);
                    groups.push(match ordinals {
                        Ordinals::Polars if row < 32_768 => group.integer(7, 4, row as i64).end(),
                        _ => group.end(),
                    });
                }
                let root = Fields::default().binary(4, b"schema").integer(5, 5, 1);
                // INT64; OPTIONAL; name.
                let field = Fields::default().integer(1, 5, 2).integer(3, 5, 1);
                let schema = [root.end(), field.binary(4, b"RID").end()];
                let footer = Fields::default()
                    .integer(1, 5, 2)
                    .list(2, 12, &schema)
                    .integer(3, 6, rows as i64)
                    .list(4, 12, &groups)
                    .binary(6, b"yamaa test")
                    .end();
                bytes.extend(&footer);
                bytes.extend((footer.len() as u32).to_le_bytes());
                bytes.extend(b"PAR1");
                bytes
            }

            /// The same records in one row group, written by the parquet
            /// crate's Arrow writer.
            fn one_row_group(rows: usize) -> Vec<u8> {
                let values: ArrayRef = Arc::new(Int64Array::from_iter((0..rows).map(value)));
                let batch = RecordBatch::try_from_iter([("RID", values)]).unwrap();
                let mut bytes = Vec::new();
                let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), None).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                bytes
            }

            #[test]
            fn every_record_reads_in_order_past_the_row_groups_one_footer_numbers() {
                // Row-group sizing does not change the delivered records
                // (PARQUET-10), whichever row groups hold an ordinal.
                for (rows, ordinals) in [
                    (32_768, Some(Ordinals::Polars)),
                    (32_769, Some(Ordinals::Absent)),
                    (40_000, Some(Ordinals::Polars)),
                    (40_000, None),
                ] {
                    let bytes = match ordinals {
                        Some(ordinals) => file(rows, ordinals),
                        None => one_row_group(rows),
                    };
                    let table = parse(&bytes)
                        .unwrap_or_else(|error| panic!("{rows} {ordinals:?}: {error:?}"));
                    assert_eq!(table.row_count(), rows, "{ordinals:?}");
                    let column = &table.schema().columns()[0];
                    assert_eq!(
                        (column.name.as_str(), column.kind),
                        ("RID", ColumnType::Int)
                    );
                    for row in 0..rows {
                        assert_eq!(
                            table.cell(row, 0).unwrap(),
                            value(row).map_or(ValueRef::Missing, ValueRef::Int),
                            "{rows} {ordinals:?} row {row}"
                        );
                    }
                }
            }
        }

        mod integer_annotations {
            //! Integer fields with the physical types and annotations DuckDB,
            //! pyarrow and Polars write (#2940), written here by the parquet
            //! crate's writers. A textual schema names a converted type alone,
            //! or a logical type with its converted type. A signed annotation
            //! whose width fits the physical type reads as `int`, as does an
            //! INT32 without an annotation; an unsigned one stays refused.
            //! Expected cells are the values the test writes, never read back
            //! from a candidate reader.
            use super::authored::{zigzag, Fields};
            use arrow_array::{ArrayRef, Int16Array, Int32Array, Int8Array, RecordBatch};
            use parquet::{
                arrow::ArrowWriter, basic::Repetition, column::writer::ColumnWriter,
                file::writer::SerializedFileWriter, schema::parser::parse_message_type,
            };
            use std::sync::Arc;
            use yamaa_adapters::{
                column_table::ColumnTable,
                parquet_source::{parse, Error},
            };
            use yamaa_core::{
                table::{TableAccess, ValueRef},
                value::ColumnType,
            };

            /// One row group holding `columns[i]` in field `i` of `schema`,
            /// with a null for each `None`.
            fn write(schema: &str, columns: &[Vec<Option<i64>>]) -> Vec<u8> {
                let schema = Arc::new(parse_message_type(schema).unwrap());
                let optional: Vec<_> = schema
                    .get_fields()
                    .iter()
                    .map(|field| field.get_basic_info().repetition() == Repetition::OPTIONAL)
                    .collect();
                let mut bytes = Vec::new();
                let mut writer =
                    SerializedFileWriter::new(&mut bytes, schema, Default::default()).unwrap();
                let mut group = writer.next_row_group().unwrap();
                for (values, optional) in columns.iter().zip(optional) {
                    let mut column = group.next_column().unwrap().unwrap();
                    let levels: Vec<_> = values.iter().map(|v| i16::from(v.is_some())).collect();
                    let levels = optional.then_some(levels.as_slice());
                    let present = values.iter().flatten().copied();
                    match column.untyped() {
                        ColumnWriter::Int32ColumnWriter(writer) => {
                            let present: Vec<_> =
                                present.map(|v| i32::try_from(v).unwrap()).collect();
                            writer.write_batch(&present, levels, None).unwrap()
                        }
                        ColumnWriter::Int64ColumnWriter(writer) => {
                            let present: Vec<_> = present.collect();
                            writer.write_batch(&present, levels, None).unwrap()
                        }
                        _ => unreachable!(),
                    };
                    column.close().unwrap();
                }
                group.close().unwrap();
                writer.close().unwrap();
                bytes
            }

            /// The least and greatest value of a signed width, a null and zero.
            fn extremes(bits: u32) -> Vec<Option<i64>> {
                let greatest = i64::MAX >> (64 - bits);
                vec![Some(-greatest - 1), None, Some(greatest), Some(0)]
            }

            /// Every field reads as `int` and holds the values written.
            fn assert_int_cells(table: &ColumnTable, columns: &[Vec<Option<i64>>]) {
                let kinds: Vec<_> = table.schema().columns().iter().map(|c| c.kind).collect();
                assert_eq!(kinds, vec![ColumnType::Int; columns.len()]);
                for (index, values) in columns.iter().enumerate() {
                    assert_eq!(table.row_count(), values.len());
                    for (row, value) in values.iter().enumerate() {
                        assert_eq!(
                            table.cell(row, index).unwrap(),
                            value.map_or(ValueRef::Missing, ValueRef::Int),
                            "field {index} row {row}"
                        );
                    }
                }
            }

            #[test]
            fn duckdb_signed_integer_fields_read_as_int() {
                // DuckDB 1.5.6 writes BIGINT, INTEGER, SMALLINT and TINYINT
                // fields OPTIONAL, with the converted type alone and no field
                // id, under a root named duckdb_schema.
                let schema = "message duckdb_schema {
                    OPTIONAL INT64 B (INT_64);
                    OPTIONAL INT32 I (INT_32);
                    OPTIONAL INT32 S (INT_16);
                    OPTIONAL INT32 T (INT_8);
                }";
                let columns = [64, 32, 16, 8].map(extremes);
                assert_int_cells(&parse(&write(schema, &columns)).unwrap(), &columns);
            }

            #[test]
            fn unannotated_int32_fields_read_as_int() {
                // pyarrow and Polars write int32 columns without an annotation.
                let schema = "message schema {
                    OPTIONAL INT32 P;
                    REQUIRED INT32 Q;
                }";
                let columns = [
                    extremes(32),
                    vec![Some(i64::from(i32::MIN)), Some(-1), Some(0), Some(1)],
                ];
                assert_int_cells(&parse(&write(schema, &columns)).unwrap(), &columns);
                // The Arrow writer stores an int32 array the same way, under an
                // Arrow schema hint naming int32.
                let int32: ArrayRef = Arc::new(Int32Array::from_iter(
                    columns[0]
                        .iter()
                        .map(|v| v.map(|v| i32::try_from(v).unwrap())),
                ));
                let batch = RecordBatch::try_from_iter([("P", int32)]).unwrap();
                let mut bytes = Vec::new();
                let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), None).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                assert_int_cells(&parse(&bytes).unwrap(), &columns[..1]);
            }

            #[test]
            fn logical_signed_integer_annotations_read_as_int() {
                // `Int(64, signed)` with `INT_64` is the DuckDB BIGINT field as
                // pyarrow reports it. A REQUIRED field reads the same.
                let schema = "message schema {
                    OPTIONAL INT64 L64 (INTEGER(64,true));
                    OPTIONAL INT32 L32 (INTEGER(32,true));
                    OPTIONAL INT32 L16 (INTEGER(16,true));
                    REQUIRED INT32 L8 (INTEGER(8,true));
                }";
                let mut columns = [64, 32, 16].map(extremes).to_vec();
                columns.push(vec![Some(-128), Some(127), Some(0), Some(1)]);
                assert_int_cells(&parse(&write(schema, &columns)).unwrap(), &columns);
                // The Arrow writer stores int8 and int16 arrays as pyarrow and
                // Polars do: INT32 annotated Int(8 | 16, signed), INT_8 | INT_16,
                // under an Arrow schema hint naming int8 and int16.
                let columns = [extremes(8), extremes(16)];
                let int8: ArrayRef = Arc::new(Int8Array::from_iter(
                    columns[0]
                        .iter()
                        .map(|v| v.map(|v| i8::try_from(v).unwrap())),
                ));
                let int16: ArrayRef = Arc::new(Int16Array::from_iter(
                    columns[1]
                        .iter()
                        .map(|v| v.map(|v| i16::try_from(v).unwrap())),
                ));
                let batch = RecordBatch::try_from_iter([("A", int8), ("B", int16)]).unwrap();
                let mut bytes = Vec::new();
                let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), None).unwrap();
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                assert_int_cells(&parse(&bytes).unwrap(), &columns);
            }

            #[test]
            fn unsigned_annotations_stay_refused() {
                // DuckDB writes UBIGINT through UTINYINT with the unsigned
                // converted types alone; pyarrow and Polars add the logical
                // type.
                for (field, expected) in [
                    ("OPTIONAL INT64 F (UINT_64);", "uint64"),
                    ("OPTIONAL INT32 F (UINT_32);", "uint32"),
                    ("OPTIONAL INT32 F (UINT_16);", "uint16"),
                    ("OPTIONAL INT32 F (UINT_8);", "uint8"),
                    ("OPTIONAL INT64 F (INTEGER(64,false));", "uint64"),
                    ("OPTIONAL INT32 F (INTEGER(8,false));", "uint8"),
                ] {
                    let bytes = write(&format!("message schema {{ {field} }}"), &[vec![Some(1)]]);
                    assert!(
                        matches!(
                            parse(&bytes),
                            Err(Error::Unsupported { field, stored_type })
                                if field == "F" && stored_type == expected
                        ),
                        "{field}"
                    );
                }
            }

            /// One REQUIRED field `F` of physical type `physical` (1 for INT32,
            /// 2 for INT64), annotated by the logical type `Int(bits, signed)`
            /// alone, holding `value` in one PLAIN page. The parquet crate's
            /// writer refuses an annotation the physical type cannot carry.
            fn annotated(physical: i64, bits: u8, value: &[u8]) -> Vec<u8> {
                // num_values; PLAIN; RLE definition and repetition levels.
                let data = Fields::default().integer(1, 5, 1).integer(2, 5, 0);
                let data = data.integer(3, 5, 3).integer(4, 5, 3);
                // DATA_PAGE; uncompressed and compressed sizes.
                let size = value.len() as i64;
                let page = Fields::default().integer(1, 5, 0).integer(2, 5, size);
                let page = page.integer(3, 5, size).strct(5, data).end();
                let length = (page.len() + value.len()) as i64;
                let meta = Fields::default()
                    .integer(1, 5, physical)
                    .list(2, 5, &[zigzag(0)]) // PLAIN
                    .list(3, 8, &[b"\x01F".to_vec()]) // path_in_schema
                    .integer(4, 5, 0) // UNCOMPRESSED
                    .integer(5, 6, 1) // num_values
                    .integer(6, 6, length)
                    .integer(7, 6, length)
                    .integer(9, 6, 4); // data_page_offset
                let chunk = Fields::default().integer(2, 6, 0).strct(3, meta);
                let group = Fields::default().list(1, 12, &[chunk.end()]);
                let group = group.integer(2, 6, length).integer(3, 6, 1).end();
                // The INTEGER variant of LogicalType: IntType(bitWidth, isSigned).
                let int = Fields::default().raw(1, 3, &[bits]).boolean(2, true);
                let logical = Fields::default().strct(10, int);
                let root = Fields::default().binary(4, b"schema").integer(5, 5, 1);
                let field = Fields::default().integer(1, 5, physical).integer(3, 5, 0);
                let field = field.binary(4, b"F").strct(10, logical);
                let footer = Fields::default()
                    .integer(1, 5, 1)
                    .list(2, 12, &[root.end(), field.end()])
                    .integer(3, 6, 1)
                    .list(4, 12, &[group])
                    .end();
                let mut bytes = b"PAR1".to_vec();
                bytes.extend(page);
                bytes.extend(value);
                bytes.extend(&footer);
                bytes.extend((footer.len() as u32).to_le_bytes());
                bytes.extend(b"PAR1");
                bytes
            }

            #[test]
            fn an_integer_annotation_its_physical_type_cannot_carry_is_invalid() {
                // The logical type alone reads as `int` where it fits.
                for (physical, bits, value) in [
                    (1, 32, (-7i32).to_le_bytes().to_vec()),
                    (1, 8, (-7i32).to_le_bytes().to_vec()),
                    (2, 64, (-7i64).to_le_bytes().to_vec()),
                ] {
                    let table = parse(&annotated(physical, bits, &value)).unwrap();
                    assert_eq!(table.schema().columns()[0].kind, ColumnType::Int);
                    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(-7));
                }
                for (physical, bits, value) in [
                    (1, 64, 7i32.to_le_bytes().to_vec()),
                    (2, 32, 7i64.to_le_bytes().to_vec()),
                    (2, 8, 7i64.to_le_bytes().to_vec()),
                ] {
                    assert!(
                        matches!(
                            parse(&annotated(physical, bits, &value)),
                            Err(Error::Malformed)
                        ),
                        "{physical} {bits}"
                    );
                }
            }

            #[test]
            fn a_stored_value_outside_its_declared_width_is_invalid() {
                for (annotation, outside) in [
                    ("INT_8", 128),
                    ("INT_8", -129),
                    ("INTEGER(8,true)", 255),
                    ("INT_16", 32_768),
                    ("INTEGER(16,true)", -32_769),
                ] {
                    let schema = format!("message schema {{ OPTIONAL INT32 F ({annotation}); }}");
                    let bytes = write(&schema, &[vec![Some(0), None, Some(outside)]]);
                    assert!(
                        matches!(parse(&bytes), Err(Error::Malformed)),
                        "{annotation} {outside}"
                    );
                }
            }
        }

        mod schema_layouts {
            //! Files written here by the parquet crate's Arrow writer, whose Arrow
            //! schema hint names a view or dictionary layout. The stored Parquet
            //! type still decides the column type (PARQUET-7). Expected cells are
            //! authored here, never read back from a candidate reader.
            use arrow_array::{
                types::{
                    ArrowDictionaryKeyType, Int16Type, Int32Type, Int64Type, Int8Type, UInt16Type,
                    UInt32Type, UInt64Type, UInt8Type,
                },
                Array, ArrayRef, BinaryArray, Date32Array, DictionaryArray, Float64Array,
                Int64Array, LargeStringArray, PrimitiveArray, RecordBatch, StringArray,
                StringViewArray, TimestampMicrosecondArray,
            };
            use arrow_buffer::ArrowNativeType;
            use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
            use parquet::{
                arrow::{
                    arrow_reader::ArrowReaderMetadata, arrow_writer::ArrowWriterOptions,
                    encode_arrow_schema, ArrowWriter,
                },
                file::{metadata::KeyValue, properties::WriterProperties},
            };
            use std::sync::Arc;
            use yamaa_adapters::parquet_source::{parse, parse_odm, Error};
            use yamaa_core::{
                table::{TableAccess, ValueRef},
                value::ColumnType,
            };

            /// Rows of every text column: a repeat, a null, an empty string and
            /// a non-ASCII value.
            const TEXT: [Option<&str>; 6] = [
                Some("b"),
                None,
                Some(""),
                Some("a"),
                Some("b"),
                Some("\u{3b2}"),
            ];

            /// One column: its name, the array the writer stores and the type
            /// the file's Arrow schema hint names.
            type Column = (&'static str, ArrayRef, DataType);

            /// Write one batch, optionally dictionary-encoded, in pages of at
            /// most two rows so a dictionary page serves several data pages.
            /// The writer embeds its batch schema as the hint, but it cannot
            /// store a dictionary of views, doubles, dates or timestamps. Where
            /// a hinted type differs from its stored array, the hinted schema
            /// replaces the writer's, through the encoder the writer uses itself.
            fn write(columns: &[Column], dictionary: bool) -> Vec<u8> {
                let schema = |kind: fn(&Column) -> DataType| {
                    Schema::new(
                        columns
                            .iter()
                            .map(|column| Field::new(column.0, kind(column), true))
                            .collect::<Vec<_>>(),
                    )
                };
                let stored = Arc::new(schema(|column| column.1.data_type().clone()));
                let hint = schema(|column| column.2.clone());
                let authored = hint != *stored;
                let batch = RecordBatch::try_new(
                    stored.clone(),
                    columns.iter().map(|column| column.1.clone()).collect(),
                )
                .unwrap();
                let properties = WriterProperties::builder()
                    .set_dictionary_enabled(dictionary)
                    .set_write_batch_size(2)
                    .set_data_page_row_count_limit(2)
                    .build();
                let options = ArrowWriterOptions::new()
                    .with_properties(properties)
                    .with_skip_arrow_metadata(authored);
                let mut bytes = Vec::new();
                let mut writer =
                    ArrowWriter::try_new_with_options(&mut bytes, stored, options).unwrap();
                if authored {
                    writer.append_key_value_metadata(KeyValue::new(
                        "ARROW:schema".into(),
                        encode_arrow_schema(&hint),
                    ));
                }
                writer.write(&batch).unwrap();
                writer.close().unwrap();
                bytes
            }

            /// The Arrow schema the parquet crate derives from the file, with
            /// its hint applied: proof that each case reaches the reader with
            /// the layout it names.
            fn hinted(bytes: &[u8]) -> SchemaRef {
                let bytes = bytes::Bytes::copy_from_slice(bytes);
                ArrowReaderMetadata::load(&bytes, Default::default())
                    .unwrap()
                    .schema()
                    .clone()
            }

            /// `values` indexed by `keys`; a null key is a null row.
            fn dictionary<K: ArrowDictionaryKeyType>(
                keys: &[Option<usize>],
                values: &ArrayRef,
            ) -> ArrayRef {
                let keys: PrimitiveArray<K> = keys
                    .iter()
                    .map(|key| key.map(|key| K::Native::from_usize(key).unwrap()))
                    .collect();
                Arc::new(DictionaryArray::<K>::try_new(keys, values.clone()).unwrap())
            }

            /// Dictionaries under index type `K` of plain, large and view
            /// strings, as (stored array, hinted type). A dictionary of views
            /// stores the dictionary of plain strings.
            fn keyed<K: ArrowDictionaryKeyType>(
                keys: &[Option<usize>],
                plain: &ArrayRef,
                large: &ArrayRef,
            ) -> [(ArrayRef, DataType); 3] {
                let view =
                    DataType::Dictionary(Box::new(K::DATA_TYPE), Box::new(DataType::Utf8View));
                let [plain, large] = [plain, large].map(|values| dictionary::<K>(keys, values));
                [
                    (plain.clone(), plain.data_type().clone()),
                    (large.clone(), large.data_type().clone()),
                    (plain, view),
                ]
            }

            /// Every Arrow layout of `TEXT`: the three string types, and a
            /// dictionary of each under each index width. The second dictionary
            /// entry is null, so a null row comes both from a null key and from
            /// a key that selects the null entry.
            fn text_layouts() -> Vec<(ArrayRef, DataType)> {
                let entries = [Some("b"), None, Some(""), Some("a"), Some("\u{3b2}")];
                let plain: ArrayRef = Arc::new(StringArray::from(entries.to_vec()));
                let large: ArrayRef = Arc::new(LargeStringArray::from(entries.to_vec()));
                let mut layouts: Vec<(ArrayRef, DataType)> = vec![
                    (Arc::new(StringArray::from(TEXT.to_vec())), DataType::Utf8),
                    (
                        Arc::new(LargeStringArray::from(TEXT.to_vec())),
                        DataType::LargeUtf8,
                    ),
                    (
                        Arc::new(StringViewArray::from(TEXT.to_vec())),
                        DataType::Utf8View,
                    ),
                ];
                for keys in [
                    [Some(0), None, Some(2), Some(3), Some(0), Some(4)],
                    [Some(0), Some(1), Some(2), Some(3), Some(0), Some(4)],
                ] {
                    layouts.extend(keyed::<Int8Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<Int16Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<Int32Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<Int64Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<UInt8Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<UInt16Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<UInt32Type>(&keys, &plain, &large));
                    layouts.extend(keyed::<UInt64Type>(&keys, &plain, &large));
                }
                layouts
            }

            fn kinds(table: &impl TableAccess) -> Vec<(&str, ColumnType)> {
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect()
            }

            #[test]
            fn view_and_dictionary_text_layouts_read_as_the_same_text() {
                let ids: ArrayRef = Arc::new(Int64Array::from_iter_values(1..=6));
                let layouts = text_layouts();
                // Three string types, then 2 key patterns x 8 index widths x 3.
                assert_eq!(layouts.len(), 51);
                for (stored, hint) in layouts {
                    for encoded in [false, true] {
                        let bytes = write(
                            &[
                                ("ID", ids.clone(), DataType::Int64),
                                ("S", stored.clone(), hint.clone()),
                            ],
                            encoded,
                        );
                        assert_eq!(hinted(&bytes).field(1).data_type(), &hint);
                        let table = parse(&bytes)
                            .unwrap_or_else(|error| panic!("{hint} ({encoded}): {error:?}"));
                        assert_eq!(
                            kinds(&table),
                            [("ID", ColumnType::Int), ("S", ColumnType::Str)],
                            "{hint}"
                        );
                        for (row, expected) in TEXT.into_iter().enumerate() {
                            assert_eq!(
                                table.cell(row, 1).unwrap(),
                                expected.map_or(ValueRef::Missing, ValueRef::Str),
                                "{hint} ({encoded}) row {row}"
                            );
                        }
                    }
                }
            }

            #[test]
            fn dictionaries_of_the_other_admitted_types_read_as_their_value_type() {
                let keys = [Some(0), None, Some(1), Some(0)];
                let int = dictionary::<Int32Type>(
                    &keys,
                    &(Arc::new(Int64Array::from(vec![i64::MIN, 7])) as ArrayRef),
                );
                let hint = |index: DataType, value: DataType| {
                    DataType::Dictionary(Box::new(index), Box::new(value))
                };
                // The writer stores a dictionary of int64 itself. Doubles, dates
                // and timestamps are stored decoded; only the hint names their
                // dictionary.
                let columns = [
                    ("I", int.clone(), int.data_type().clone()),
                    (
                        "F",
                        Arc::new(Float64Array::from(vec![
                            Some(-0.0),
                            None,
                            Some(f64::from_bits(1)),
                            Some(-0.0),
                        ])) as ArrayRef,
                        hint(DataType::Int8, DataType::Float64),
                    ),
                    (
                        "D",
                        Arc::new(Date32Array::from(vec![
                            Some(-719_162),
                            None,
                            Some(2_932_896),
                            Some(-719_162),
                        ])) as ArrayRef,
                        hint(DataType::UInt16, DataType::Date32),
                    ),
                    (
                        "T",
                        Arc::new(TimestampMicrosecondArray::from(vec![
                            Some(-1_000_000),
                            None,
                            Some(0),
                            Some(-1_000_000),
                        ])) as ArrayRef,
                        hint(
                            DataType::UInt32,
                            DataType::Timestamp(TimeUnit::Microsecond, None),
                        ),
                    ),
                    // A date64 hint names the stored DATE as it does outside a
                    // dictionary, where it already reads as a date.
                    (
                        "E",
                        Arc::new(Date32Array::from(vec![
                            Some(-719_162),
                            None,
                            Some(2_932_896),
                            Some(-719_162),
                        ])) as ArrayRef,
                        hint(DataType::Int16, DataType::Date64),
                    ),
                ];
                for encoded in [false, true] {
                    let bytes = write(&columns, encoded);
                    let restored = hinted(&bytes);
                    for (index, column) in columns.iter().enumerate() {
                        assert_eq!(restored.field(index).data_type(), &column.2);
                    }
                    let table = parse(&bytes).unwrap();
                    assert_eq!(
                        kinds(&table),
                        [
                            ("I", ColumnType::Int),
                            ("F", ColumnType::Float),
                            ("D", ColumnType::Date),
                            ("T", ColumnType::DateTime),
                            ("E", ColumnType::Date),
                        ]
                    );
                    for column in 0..5 {
                        assert_eq!(table.cell(1, column).unwrap(), ValueRef::Missing);
                    }
                    for row in [0, 3] {
                        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(i64::MIN));
                        let ValueRef::Float(value) = table.cell(row, 1).unwrap() else {
                            panic!("float")
                        };
                        assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
                        let ValueRef::Date(value) = table.cell(row, 2).unwrap() else {
                            panic!("date")
                        };
                        assert_eq!(value.to_string(), "0001-01-01");
                        let ValueRef::DateTime(value) = table.cell(row, 3).unwrap() else {
                            panic!("datetime")
                        };
                        assert_eq!(value.fields(), (1969, 12, 31, 23, 59, 59));
                        let ValueRef::Date(value) = table.cell(row, 4).unwrap() else {
                            panic!("date64 dictionary")
                        };
                        assert_eq!(value.to_string(), "0001-01-01");
                    }
                    assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Int(7));
                    let ValueRef::Float(value) = table.cell(2, 1).unwrap() else {
                        panic!("float")
                    };
                    assert_eq!(value.get().to_bits(), 1);
                    let ValueRef::Date(value) = table.cell(2, 2).unwrap() else {
                        panic!("date")
                    };
                    assert_eq!(value.to_string(), "9999-12-31");
                    let ValueRef::DateTime(value) = table.cell(2, 3).unwrap() else {
                        panic!("datetime")
                    };
                    assert_eq!(value.fields(), (1970, 1, 1, 0, 0, 0));
                    let ValueRef::Date(value) = table.cell(2, 4).unwrap() else {
                        panic!("date64 dictionary")
                    };
                    assert_eq!(value.to_string(), "9999-12-31");
                }
            }

            #[test]
            fn a_dictionary_of_a_refused_type_keeps_its_stored_type_spelling() {
                let values: ArrayRef = Arc::new(BinaryArray::from(vec![b"x".as_slice()]));
                let array = dictionary::<Int32Type>(&[Some(0)], &values);
                let bytes = write(&[("B", array.clone(), array.data_type().clone())], true);
                assert!(matches!(
                    parse(&bytes),
                    Err(Error::Unsupported { field, stored_type })
                        if field == "B"
                            && stored_type == "dictionary<values=binary, indices=int32, ordered=0>"
                ));
            }

            #[test]
            fn odm_fields_read_text_through_view_and_dictionary_layouts() {
                use yamaa_core::odm_source::FIELDS;
                let rows = |field: &str| [format!("{field}-1"), format!("{field}-2")];
                let columns: Vec<Column> = FIELDS
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        let [first, second] = rows(field);
                        let array: ArrayRef = match index % 3 {
                            0 => Arc::new(StringViewArray::from_iter_values([first, second])),
                            1 => dictionary::<UInt32Type>(
                                &[Some(1), Some(0)],
                                &(Arc::new(LargeStringArray::from_iter_values([second, first]))
                                    as ArrayRef),
                            ),
                            _ => dictionary::<Int8Type>(
                                &[Some(0), Some(1)],
                                &(Arc::new(StringArray::from_iter_values([first, second]))
                                    as ArrayRef),
                            ),
                        };
                        let kind = array.data_type().clone();
                        (*field, array, kind)
                    })
                    .collect();
                let table = parse_odm(&write(&columns, true)).unwrap();
                assert_eq!(
                    kinds(&table),
                    FIELDS
                        .iter()
                        .map(|field| (*field, ColumnType::Str))
                        .collect::<Vec<_>>()
                );
                for (column, field) in FIELDS.iter().enumerate() {
                    for (row, expected) in rows(field).iter().enumerate() {
                        assert_eq!(
                            table.cell(row, column).unwrap(),
                            ValueRef::Str(expected.as_str()),
                            "{field}"
                        );
                    }
                }
            }
        }
    }
}
