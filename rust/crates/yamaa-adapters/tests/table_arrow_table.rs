//! Table contracts: Arrow table, CSV artifact, CSV precision, CSV source,
//! parquet artifact, predicate diagnostics and public table.
mod table {
    mod arrow_table {
        use crate::support::table_schema as logical_schema;
        use arrow_array::{
            Array, ArrayRef, Date32Array, Float64Array, Int64Array, RecordBatch,
            RecordBatchOptions, StringArray, StructArray, TimestampSecondArray, UInt8Array,
        };
        use arrow_buffer::NullBuffer;
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc;
        use yamaa_adapters::arrow_table::{
            date_array, datetime_array, physical_schema, ArrowTable, TableError,
        };
        use yamaa_core::{
            table::{CellError, TableAccess, TableSchema, ValueRef},
            temporal::{Date, DateTime, Precision as DP, Precision as TP},
            value::ColumnType as CT,
        };

        /// Create a batch with the canonical physical schema.
        fn batch(schema: &TableSchema, arrays: Vec<ArrayRef>) -> RecordBatch {
            RecordBatch::try_new(physical_schema(schema), arrays).unwrap()
        }

        /// Read and require an actual date, preserving its separate precision metadata.
        fn date(table: &ArrowTable, row: usize, column: usize) -> Date {
            let ValueRef::Date(value) = table.cell(row, column).unwrap() else {
                panic!("date required")
            };
            value
        }

        /// Read and require an actual datetime, preserving its separate precision metadata.
        fn datetime(table: &ArrowTable, row: usize, column: usize) -> DateTime {
            let ValueRef::DateTime(value) = table.cell(row, column).unwrap() else {
                panic!("datetime required")
            };
            value
        }

        #[test]
        /// Retain full-range integers, borrowed text and finite bits after inputs drop.
        fn exact_scalars_normalization_and_buffer_ownership() {
            let schema = logical_schema(&[
                ("text", CT::Str),
                ("integer", CT::Int),
                ("float", CT::Float),
            ]);
            let text = Arc::new(StringArray::from(vec![
                Some(""),
                Some("a\0\u{1f980}"),
                None,
                Some("last"),
            ]));
            let pointer = text.value(1).as_ptr();
            let original = batch(
                &schema,
                vec![
                    text.clone(),
                    Arc::new(Int64Array::from(vec![
                        i64::MIN,
                        i64::MAX,
                        9_007_199_254_740_993,
                        0,
                    ])),
                    Arc::new(Float64Array::from(vec![
                        -0.0,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NAN,
                    ])),
                ],
            );
            let table = ArrowTable::try_new(schema, vec![original.clone()]).unwrap();
            drop(original);
            drop(text);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str(""));
            let ValueRef::Str(text) = table.cell(1, 0).unwrap() else {
                panic!("text required")
            };
            assert_eq!(text, "a\0\u{1f980}");
            assert_eq!(text.as_ptr(), pointer);
            assert_eq!(table.cell(2, 0).unwrap(), ValueRef::Missing);
            for (row, value) in [i64::MIN, i64::MAX, 9_007_199_254_740_993, 0]
                .into_iter()
                .enumerate()
            {
                assert_eq!(table.cell(row, 1).unwrap(), ValueRef::Int(value));
            }
            let ValueRef::Float(zero) = table.cell(0, 2).unwrap() else {
                panic!("float required")
            };
            assert_eq!(zero.get().to_bits(), 1 << 63);
            for row in 1..4 {
                assert_eq!(table.cell(row, 2).unwrap(), ValueRef::Missing);
                assert!(table.batches()[0].column(2).is_null(row));
            }
            for _ in 0..100 {
                assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Str("last"));
            }
            assert_eq!(
                table.cell(4, 0),
                Err(CellError::OutOfBounds { row: 4, column: 0 })
            );
            assert_eq!(
                table.cell(0, 3),
                Err(CellError::OutOfBounds { row: 0, column: 3 })
            );
        }

        #[test]
        /// Navigate empty chunks and nonzero slice offsets without changing order.
        fn chunk_and_slice_offsets_keep_original_record_and_column_order() {
            let schema = logical_schema(&[("B", CT::Str), ("A", CT::Int)]);
            let original = batch(
                &schema,
                vec![
                    Arc::new(StringArray::from(vec!["skip", "first", "second", "third"])),
                    Arc::new(Int64Array::from(vec![-1, 10, 20, 30])),
                ],
            );
            let empty = original.slice(0, 0);
            let table = ArrowTable::try_new(
                schema,
                vec![
                    empty.clone(),
                    original.slice(1, 2),
                    empty.clone(),
                    original.slice(3, 1),
                    empty,
                ],
            )
            .unwrap();
            assert_eq!(table.row_count(), 3);
            assert_eq!(table.batches().len(), 5);
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|col| col.name.as_str())
                    .collect::<Vec<_>>(),
                ["B", "A"]
            );
            for (row, (name, value)) in [("first", 10), ("second", 20), ("third", 30)]
                .into_iter()
                .enumerate()
            {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Str(name));
                assert_eq!(table.cell(row, 1).unwrap(), ValueRef::Int(value));
            }
        }

        #[test]
        /// Keep imputed fields and precision independently through temporal slicing.
        fn temporal_precision_and_negative_epoch_survive_sliced_structs() {
            let schema = logical_schema(&[("date", CT::Date), ("datetime", CT::DateTime)]);
            let year = Date::new(2025, 7, 19, DP::Year).unwrap();
            let month = Date::new(1969, 12, 31, DP::Month).unwrap();
            let day = Date::new(1, 1, 1, DP::Day).unwrap();
            let time = DateTime::new(month, 23, 59, 59, TP::Second).unwrap();
            // Day precision still retains supplied/imputed time fields.
            let imputed = DateTime::new(year, 12, 34, 56, TP::Day).unwrap();
            let dates =
                Arc::new(date_array(&[None, Some(year), Some(month), Some(day), None]).unwrap());
            let times =
                Arc::new(datetime_array(&[None, Some(imputed), Some(time), None, None]).unwrap());
            let original = batch(&schema, vec![dates, times]);
            let table = ArrowTable::try_new(schema, vec![original.slice(1, 4)]).unwrap();
            drop(original);
            for (row, expected) in [year, month, day].into_iter().enumerate() {
                assert_eq!(date(&table, row, 0).fields(), expected.fields());
                assert_eq!(
                    date(&table, row, 0).collected_precision(),
                    expected.collected_precision()
                );
            }
            assert_eq!(datetime(&table, 0, 1).fields(), imputed.fields());
            assert_eq!(datetime(&table, 0, 1).collected_precision(), TP::Day);
            assert_eq!(datetime(&table, 1, 1).fields(), (1969, 12, 31, 23, 59, 59));
            assert_eq!(datetime(&table, 1, 1).collected_precision(), TP::Second);
            assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(3, 0).unwrap(), ValueRef::Missing);
        }

        #[test]
        /// Keep every rung of the one collected-precision ladder beside the
        /// completed fields, for dates and datetimes alike.
        fn every_collected_precision_survives_encoding() {
            let schema = logical_schema(&[("date", CT::Date)]);
            let dates = [DP::None, DP::Year, DP::Month, DP::Day]
                .map(|precision| Date::new(2025, 3, 4, precision).unwrap());
            let encoded = Arc::new(date_array(&dates.map(Some)).unwrap());
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![encoded])]).unwrap();
            for (row, expected) in dates.into_iter().enumerate() {
                let ValueRef::Date(value) = table.cell(row, 0).unwrap() else {
                    panic!("date")
                };
                assert_eq!(value.collected_precision(), expected.collected_precision());
            }
            let schema = logical_schema(&[("datetime", CT::DateTime)]);
            let day = Date::new(2025, 3, 4, DP::Day).unwrap();
            let values = [
                DateTime::new(day, 0, 0, 0, TP::None).unwrap(),
                DateTime::new(day, 0, 0, 0, TP::Year).unwrap(),
                DateTime::new(day, 0, 0, 0, TP::Month).unwrap(),
                DateTime::new(day, 0, 0, 0, TP::Day).unwrap(),
                DateTime::new(day, 8, 59, 59, TP::Hour).unwrap(),
                DateTime::new(day, 8, 30, 0, TP::Minute).unwrap(),
                DateTime::new(day, 8, 30, 15, TP::Second).unwrap(),
            ];
            let times = Arc::new(datetime_array(&values.map(Some)).unwrap());
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![times])]).unwrap();
            for (row, expected) in values.into_iter().enumerate() {
                assert_eq!(datetime(&table, row, 0).fields(), expected.fields());
                assert_eq!(
                    datetime(&table, row, 0).collected_precision(),
                    expected.collected_precision()
                );
            }
        }

        #[test]
        /// Retain declared schema with no records and record counts with no columns.
        fn zero_rows_and_zero_columns_retain_independent_shapes() {
            let schema = logical_schema(&[("D", CT::Date), ("T", CT::Str)]);
            let no_batches = ArrowTable::try_new(schema.clone(), vec![]).unwrap();
            assert_eq!(no_batches.schema(), &schema);
            assert_eq!(no_batches.row_count(), 0);
            let empty = batch(
                &schema,
                vec![
                    Arc::new(date_array(&[]).unwrap()),
                    Arc::new(StringArray::from(Vec::<Option<&str>>::new())),
                ],
            );
            let table = ArrowTable::try_new(schema, vec![empty]).unwrap();
            assert_eq!(table.batches().len(), 1);
            assert_eq!(table.schema().columns().len(), 2);
            assert_eq!(
                table.cell(0, 0),
                Err(CellError::OutOfBounds { row: 0, column: 0 })
            );
            let empty_schema = TableSchema::new(vec![]).unwrap();
            let rows = RecordBatch::try_new_with_options(
                physical_schema(&empty_schema),
                vec![],
                &RecordBatchOptions::new().with_row_count(Some(3)),
            )
            .unwrap();
            let table = ArrowTable::try_new(empty_schema, vec![rows]).unwrap();
            assert_eq!(table.row_count(), 3);
            assert_eq!(
                table.cell(0, 0),
                Err(CellError::OutOfBounds { row: 0, column: 0 })
            );
        }

        /// Construct canonical date storage directly to test independent bad payloads.
        fn raw_dates(
            values: Vec<i32>,
            precision: Vec<u8>,
            valid: Option<Vec<bool>>,
        ) -> StructArray {
            let schema = logical_schema(&[("D", CT::Date)]);
            let physical = physical_schema(&schema);
            let DataType::Struct(fields) = physical.field(0).data_type() else {
                panic!("struct required")
            };
            StructArray::try_new(
                fields.clone(),
                vec![
                    Arc::new(Date32Array::from(values)),
                    Arc::new(UInt8Array::from(precision)),
                ],
                valid.map(NullBuffer::from),
            )
            .unwrap()
        }

        #[test]
        /// Reject malformed visible data but honor parent-null masking.
        fn visible_temporal_payloads_are_validated_before_any_use() {
            // Codes 0 to 3 name year, month, day and none; 4 is the first unknown.
            for (values, precisions, precision) in [
                (vec![0, 0], vec![2, 4], true),
                (vec![0, i32::MIN], vec![2, 2], false),
                (vec![0, 2932897], vec![2, 2], false),
            ] {
                let schema = logical_schema(&[("D", CT::Date)]);
                let input = batch(&schema, vec![Arc::new(raw_dates(values, precisions, None))]);
                let result = ArrowTable::try_new(schema, vec![input]);
                if precision {
                    assert!(matches!(
                        result,
                        Err(TableError::TemporalPrecision {
                            batch: 0,
                            column: 0,
                            row: 1
                        })
                    ));
                } else {
                    assert!(matches!(
                        result,
                        Err(TableError::TemporalRange {
                            batch: 0,
                            column: 0,
                            row: 1
                        })
                    ));
                }
            }
            // Null parent masks even an invalid raw day and unknown precision.
            let schema = logical_schema(&[("D", CT::Date)]);
            let input = batch(
                &schema,
                vec![Arc::new(raw_dates(
                    vec![i32::MIN, 0],
                    vec![255, 2],
                    Some(vec![false, true]),
                ))],
            );
            let table = ArrowTable::try_new(schema, vec![input]).unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
            assert_eq!(date(&table, 1, 0).fields(), (1970, 1, 1));
        }

        #[test]
        /// Require both temporal children and their exact ordered non-null schema.
        fn temporal_child_validity_and_physical_schema_cannot_drift() {
            let schema = logical_schema(&[("D", CT::Date)]);
            let physical = physical_schema(&schema);
            let DataType::Struct(fields) = physical.field(0).data_type() else {
                panic!("struct required")
            };
            let null_children: Vec<ArrayRef> = vec![
                Arc::new(Date32Array::from(vec![None, Some(0)])),
                Arc::new(UInt8Array::from(vec![None, Some(2)])),
            ];
            assert!(StructArray::try_new(fields.clone(), null_children.clone(), None).is_err());
            let masked = StructArray::try_new(
                fields.clone(),
                null_children,
                Some(NullBuffer::from(vec![false, true])),
            )
            .unwrap();
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![Arc::new(masked)])])
                    .unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
            for fields in [
                vec![
                    Field::new("precision", DataType::UInt8, false),
                    Field::new("value", DataType::Date32, false),
                ],
                vec![
                    Field::new("value", DataType::Date32, true),
                    Field::new("precision", DataType::UInt8, false),
                ],
            ] {
                let arrays: Vec<ArrayRef> = if fields[0].name() == "precision" {
                    vec![
                        Arc::new(UInt8Array::from(vec![2])),
                        Arc::new(Date32Array::from(vec![0])),
                    ]
                } else {
                    vec![
                        Arc::new(Date32Array::from(vec![0])),
                        Arc::new(UInt8Array::from(vec![2])),
                    ]
                };
                let array = StructArray::try_new(fields.into(), arrays, None).unwrap();
                let foreign = Arc::new(Schema::new(vec![Field::new(
                    "D",
                    array.data_type().clone(),
                    true,
                )]));
                let input = RecordBatch::try_new(foreign, vec![Arc::new(array)]).unwrap();
                assert!(matches!(
                    ArrowTable::try_new(schema.clone(), vec![input]),
                    Err(TableError::SchemaMismatch { batch: 0 })
                ));
            }
        }

        #[test]
        /// Fixed geometry precedes malformed temporal payloads; schema errors remain distinct.
        fn schema_mismatches_and_fixed_geometry_precede_payload_scans() {
            let schema = logical_schema(&[("D", CT::Date)]);
            let invalid = batch(&schema, vec![Arc::new(raw_dates(vec![0], vec![255], None))]);
            let wide_schema = TableSchema::new(
                (0..1_025)
                    .map(|column| yamaa_core::table::Column {
                        name: if column == 0 {
                            "D".into()
                        } else {
                            format!("C{column}")
                        },
                        kind: if column == 0 { CT::Date } else { CT::Int },
                    })
                    .collect(),
            )
            .unwrap();
            let number = Arc::new(Int64Array::from(vec![1])) as ArrayRef;
            let arrays = std::iter::once(invalid.column(0).clone())
                .chain(std::iter::repeat_n(number, 1_024))
                .collect();
            let wide = batch(&wide_schema, arrays);
            assert!(matches!(
                ArrowTable::try_new(wide_schema, vec![wide]),
                Err(TableError::Limit {
                    resource: "columns",
                    limit: 1_024,
                    required: Some(1_025),
                })
            ));
            for foreign in [
                Arc::new(Schema::new(vec![Field::new(
                    "wrong",
                    DataType::Int64,
                    true,
                )])),
                Arc::new(Schema::new(vec![Field::new("D", DataType::Int64, true)])),
            ] {
                let input =
                    RecordBatch::try_new(foreign, vec![Arc::new(Int64Array::from(vec![1]))])
                        .unwrap();
                assert!(matches!(
                    ArrowTable::try_new(schema.clone(), vec![input]),
                    Err(TableError::SchemaMismatch { batch: 0 })
                ));
            }
            let no_columns = TableSchema::new(vec![]).unwrap();
            let huge = RecordBatch::try_new_with_options(
                physical_schema(&no_columns),
                vec![],
                &RecordBatchOptions::new().with_row_count(Some(usize::MAX)),
            )
            .unwrap();
            assert!(matches!(
                ArrowTable::try_new(no_columns, vec![huge.clone(), huge]),
                Err(TableError::Limit {
                    resource: "table_rows",
                    required: None,
                    ..
                })
            ));
        }

        #[test]
        /// Compare independent epoch anchors and every day of a Gregorian cycle.
        fn epoch_calendar_anchors_and_full_gregorian_cycle() {
            let schema = logical_schema(&[("D", CT::Date)]);
            let anchors = [
                (-719162, (1, 1, 1)),
                (-135140, (1600, 1, 1)),
                (-25508, (1900, 3, 1)),
                (-1, (1969, 12, 31)),
                (0, (1970, 1, 1)),
                (11016, (2000, 2, 29)),
                (20089, (2025, 1, 1)),
                (2932896, (9999, 12, 31)),
            ];
            let raw = raw_dates(
                anchors.iter().map(|(days, _)| *days).collect(),
                vec![2; anchors.len()],
                None,
            );
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![Arc::new(raw)])])
                    .unwrap();
            for (row, (_, fields)) in anchors.iter().enumerate() {
                assert_eq!(date(&table, row, 0).fields(), *fields);
            }
            let mut civil = vec![];
            for year in 1600..2000 {
                for month in 1..=12 {
                    for day in 1..=31 {
                        if let Ok(value) = Date::new(year, month, day, DP::Day) {
                            civil.push(Some(value));
                        }
                    }
                }
            }
            assert_eq!(civil.len(), 146097);
            let array = date_array(&civil).unwrap();
            let days = array
                .column(0)
                .as_any()
                .downcast_ref::<Date32Array>()
                .unwrap();
            for row in 0..civil.len() {
                assert_eq!(days.value(row), -135140 + row as i32);
            }
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![Arc::new(array)])])
                    .unwrap();
            for (row, expected) in civil.into_iter().enumerate() {
                assert_eq!(date(&table, row, 0).fields(), expected.unwrap().fields());
            }
        }

        #[test]
        /// Check negative epochs, range endpoints, precision and timezone rejection.
        fn datetime_range_precision_and_unit_are_strict() {
            let schema = logical_schema(&[("T", CT::DateTime)]);
            let physical = physical_schema(&schema);
            let DataType::Struct(fields) = physical.field(0).data_type() else {
                panic!("struct required")
            };
            for (seconds, code, precision) in [
                (i64::MIN, 1, false),
                (i64::MAX, 1, false),
                (-62135596801, 1, false),
                (253402300800, 1, false),
                // Codes 0 to 6 name day, second, hour, minute, none, year and month.
                (0, 7, true),
            ] {
                let array = StructArray::try_new(
                    fields.clone(),
                    vec![
                        Arc::new(TimestampSecondArray::from(vec![seconds])),
                        Arc::new(UInt8Array::from(vec![code])),
                    ],
                    None,
                )
                .unwrap();
                let input = batch(&schema, vec![Arc::new(array)]);
                let result = ArrowTable::try_new(schema.clone(), vec![input]);
                if precision {
                    assert!(matches!(result, Err(TableError::TemporalPrecision { .. })));
                } else {
                    assert!(matches!(result, Err(TableError::TemporalRange { .. })));
                }
            }
            let seconds = [-62135596800, -86401, -86400, -1, 0, 253402300799];
            let expected = [
                (1, 1, 1, 0, 0, 0),
                (1969, 12, 30, 23, 59, 59),
                (1969, 12, 31, 0, 0, 0),
                (1969, 12, 31, 23, 59, 59),
                (1970, 1, 1, 0, 0, 0),
                (9999, 12, 31, 23, 59, 59),
            ];
            let array = StructArray::try_new(
                fields.clone(),
                vec![
                    Arc::new(TimestampSecondArray::from(seconds.to_vec())),
                    Arc::new(UInt8Array::from(vec![1; 6])),
                ],
                None,
            )
            .unwrap();
            let table =
                ArrowTable::try_new(schema.clone(), vec![batch(&schema, vec![Arc::new(array)])])
                    .unwrap();
            for (row, fields) in expected.into_iter().enumerate() {
                assert_eq!(datetime(&table, row, 0).fields(), fields);
            }
            let encoded = datetime_array(
                &(0..6)
                    .map(|row| Some(datetime(&table, row, 0)))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            assert_eq!(
                encoded
                    .column(0)
                    .as_any()
                    .downcast_ref::<TimestampSecondArray>()
                    .unwrap()
                    .values()
                    .as_ref(),
                seconds
            );
            // A timezone-bearing timestamp is not the civil representation.
            let zoned = TimestampSecondArray::from(vec![0]).with_timezone("UTC");
            let foreign_fields = vec![
                Field::new("value", zoned.data_type().clone(), false),
                Field::new("precision", DataType::UInt8, false),
            ];
            let array = StructArray::try_new(
                foreign_fields.into(),
                vec![Arc::new(zoned), Arc::new(UInt8Array::from(vec![1]))],
                None,
            )
            .unwrap();
            let input = RecordBatch::try_new(
                Arc::new(Schema::new(vec![Field::new(
                    "T",
                    array.data_type().clone(),
                    true,
                )])),
                vec![Arc::new(array)],
            )
            .unwrap();
            assert!(matches!(
                ArrowTable::try_new(schema, vec![input]),
                Err(TableError::SchemaMismatch { batch: 0 })
            ));
        }

        #[test]
        /// Keep explicit null validity separate from i64 sentinels and finite float bits.
        fn null_validity_never_collides_with_exact_scalar_payloads() {
            let schema = logical_schema(&[("I", CT::Int), ("F", CT::Float)]);
            let bits = [1, 0x7fefffffffffffff, 0xffefffffffffffff];
            let floats: Vec<_> = bits
                .into_iter()
                .map(|bits| Some(f64::from_bits(bits)))
                .chain([None])
                .collect();
            let input = batch(
                &schema,
                vec![
                    Arc::new(Int64Array::from(vec![
                        Some(i64::MIN),
                        None,
                        Some(i64::MAX),
                        Some(0),
                    ])),
                    Arc::new(Float64Array::from(floats)),
                ],
            );
            let table = ArrowTable::try_new(schema, vec![input]).unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(i64::MIN));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
            for (row, bits) in bits.into_iter().enumerate() {
                let ValueRef::Float(value) = table.cell(row, 1).unwrap() else {
                    panic!("finite float required")
                };
                assert_eq!(value.get().to_bits(), bits);
            }
            assert_eq!(table.cell(3, 1).unwrap(), ValueRef::Missing);
        }
    }
    mod csv_artifact {
        use std::convert::Infallible;
        use yamaa_adapters::csv_artifact::{self, Error};
        use yamaa_core::{
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };
        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
        }
        impl TableAccess for Table {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows.len()
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.rows
                    .get(row)
                    .and_then(|r| r.get(column))
                    .map(ValueRef::from)
                    .ok_or(CellError::OutOfBounds { row, column })
            }
        }
        fn text_table(names: &[&str], rows: Vec<Vec<Value>>) -> Table {
            Table {
                schema: TableSchema::new(
                    names
                        .iter()
                        .map(|name| Column {
                            name: (*name).into(),
                            kind: ColumnType::Str,
                        })
                        .collect(),
                )
                .unwrap(),
                rows,
            }
        }
        #[test]
        fn exact_quoting_preserves_empty_missing_whitespace_unicode_and_line_characters() {
            let table = text_table(
                &["A", "B", "C", "D"],
                vec![
                    vec![
                        Value::Missing,
                        Value::Str("".into()),
                        Value::Str("  text\t".into()),
                        Value::Str("é漢".into()),
                    ],
                    vec![
                        Value::Str("a,b".into()),
                        Value::Str("a\"b".into()),
                        Value::Str("a\nb".into()),
                        Value::Str("a\rb".into()),
                    ],
                ],
            );
            let expected = "A,B,C,D\n,\"\",  text\t,é漢\n\"a,b\",\"a\"\"b\",\"a\nb\",\"a\rb\"\n";
            assert_eq!(
                csv_artifact::render(&table, &[0, 1, 2, 3], None).unwrap(),
                expected.as_bytes()
            );
        }
        #[test]
        fn header_only_and_projection_order_are_exact() {
            let table = text_table(&["FIRST", "LAST"], vec![]);
            assert_eq!(
                csv_artifact::render(&table, &[1, 0], None).unwrap(),
                b"LAST,FIRST\n"
            );
            for columns in [vec![], vec![0, 0], vec![2]] {
                assert!(matches!(
                    csv_artifact::render(&table, &columns, None),
                    Err(Error::Projection)
                ));
            }
        }
        #[test]
        fn scalar_output_uses_exact_integer_and_shortest_positional_float_text() {
            let table = Table {
                schema: TableSchema::new(vec![
                    Column {
                        name: "I".into(),
                        kind: ColumnType::Int,
                    },
                    Column {
                        name: "F".into(),
                        kind: ColumnType::Float,
                    },
                ])
                .unwrap(),
                rows: vec![
                    vec![Value::Int(i64::MAX), Value::float(-0.0)],
                    vec![Value::Int(9007199254740993), Value::float(1e-7)],
                ],
            };
            assert_eq!(
                csv_artifact::render(&table, &[0, 1], None).unwrap(),
                b"I,F\n9223372036854775807,-0\n9007199254740993,0.0000001\n"
            );
        }

        #[test]
        fn precision_serializes_float_projection_only_and_preserves_missing_and_empty_text() {
            let table = Table {
                schema: TableSchema::new(vec![
                    Column {
                        name: "I".into(),
                        kind: ColumnType::Int,
                    },
                    Column {
                        name: "F".into(),
                        kind: ColumnType::Float,
                    },
                    Column {
                        name: "S".into(),
                        kind: ColumnType::Str,
                    },
                ])
                .unwrap(),
                rows: vec![
                    vec![Value::Int(1), Value::float(0.125), Value::Str("".into())],
                    vec![Value::Int(2), Value::float(2.675), Value::Str("a,b".into())],
                    vec![Value::Int(3), Value::Missing, Value::Missing],
                    vec![Value::Int(4), Value::float(-0.004), Value::Str("z".into())],
                ],
            };
            let expected = b"S,F,I\n\"\",0.13,1\n\"a,b\",2.68,2\n,,3\nz,0.00,4\n";
            assert_eq!(
                csv_artifact::render(&table, &[2, 1, 0], Some("2")).unwrap(),
                expected
            );
            assert_eq!(table.rows[0][1], Value::float(0.125));
            assert_eq!(
                csv_artifact::render(&table, &[0], Some("999999999999999999999999999999")).unwrap(),
                b"I\n1\n2\n3\n4\n"
            );
        }

        #[test]
        fn excessive_precision_refuses_before_the_first_projected_cell_read() {
            struct NoCells(TableSchema);
            impl TableAccess for NoCells {
                type Error = Infallible;
                fn schema(&self) -> &TableSchema {
                    &self.0
                }
                fn row_count(&self) -> usize {
                    1
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
                    panic!("impossible output width must precede cell authority")
                }
            }
            let table = NoCells(
                TableSchema::new(vec![
                    Column {
                        name: "I".into(),
                        kind: ColumnType::Int,
                    },
                    Column {
                        name: "F".into(),
                        kind: ColumnType::Float,
                    },
                ])
                .unwrap(),
            );
            for precision in ["10737418241", "999999999999999999999999999999"] {
                assert!(
                    matches!(csv_artifact::render(&table, &[0, 1], Some(precision)),
            Err(Error::Limit { resource: "output_bytes", limit, actual: None, decimal_formatting: true }) if limit == yamaa_core::limits::Limits::OUTPUT_BYTES)
                );
            }
        }

        #[test]
        fn retired_python_scalar_text_and_calendar_edges_keep_exact_csv_bytes() {
            let table = Table {
                schema: TableSchema::new(
                    [
                        ("ID", ColumnType::Str),
                        ("COUNT", ColumnType::Int),
                        ("WHEN", ColumnType::Date),
                        ("AT", ColumnType::DateTime),
                        ("TEXT", ColumnType::Str),
                    ]
                    .into_iter()
                    .map(|(name, kind)| Column {
                        name: name.into(),
                        kind,
                    })
                    .collect(),
                )
                .unwrap(),
                rows: vec![
                    vec![
                        Value::Str("1".into()),
                        Value::Int(-42),
                        Value::Date("2020-01-02".parse().unwrap()),
                        Value::DateTime("2020-01-02T03:04:05".parse().unwrap()),
                        Value::Str("\u{e9}\u{1d400}".into()),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Int(0),
                        Value::Missing,
                        Value::Missing,
                        Value::Missing,
                    ],
                ],
            };
            assert_eq!(
        csv_artifact::render(&table, &[0, 1, 2, 3, 4], None).unwrap(),
        "ID,COUNT,WHEN,AT,TEXT\n1,-42,2020-01-02,2020-01-02T03:04:05,\u{e9}\u{1d400}\n2,0,,,\n"
            .as_bytes()
    );
            let edges = Table {
                schema: TableSchema::new(vec![
                    Column {
                        name: "ID".into(),
                        kind: ColumnType::Str,
                    },
                    table.schema.columns()[2].clone(),
                    table.schema.columns()[3].clone(),
                ])
                .unwrap(),
                rows: vec![
                    vec![
                        Value::Str("1".into()),
                        Value::Date("0001-01-01".parse().unwrap()),
                        Value::DateTime("0001-01-01T00:00:00".parse().unwrap()),
                    ],
                    vec![
                        Value::Str("2".into()),
                        Value::Date("9999-12-31".parse().unwrap()),
                        Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
                    ],
                ],
            };
            assert_eq!(
                csv_artifact::render(&edges, &[0, 1, 2], None).unwrap(),
                b"ID,WHEN,AT\n1,0001-01-01,0001-01-01T00:00:00\n2,9999-12-31,9999-12-31T23:59:59\n"
            );
        }
    }
    #[cfg(feature = "yaml")]
    mod csv_precision {
        use crate::support::ports::Sources;
        use crate::support::Edit as _;
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::value::Value;

        const SPEC: &str = "yamaa: '1.0'\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: output.csv, columns: [ID, VALUE, DOUBLE], decimals: PRECISION}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: DOUBLE, type: float, derivation: 'VALUE + VALUE'}\n";
        fn prepared(precision: &str, path: &str, expression: &str) -> PreparedRun {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let bytes = SPEC
                .edit("PRECISION", precision)
                .edit("output.csv", path)
                .edit("VALUE + VALUE", expression);
            let document = schema
                .prepare_document(
                    Source {
                        identity: "spec.yaml".into(),
                        bytes: bytes.into_bytes(),
                    },
                    "spec.yaml".into(),
                    &mut crate::support::inheritance::Files,
                )
                .unwrap();
            PreparedRun::prepare(document).unwrap()
        }
        /// The one CSV input of the csv-precision cases, served at `input.csv`.
        fn study() -> Sources {
            Sources::by_path([(
                "input.csv",
                b"ID,VALUE\n1,0.125\n2,1.234\n3,2.345\n".as_slice(),
            )])
        }
        fn identity() -> Identity<'static> {
            Identity {
                runtime: "python",
                runtime_version: "fixture",
                engine_version: "fixture",
                example: "csv-precision",
                specification: "spec.yaml",
                base_directory: ".",
            }
        }

        #[test]
        fn whole_result_keeps_unrounded_derivation_and_prepares_exact_saved_bytes_once() {
            let run = prepared("2", "output.csv", "VALUE + VALUE");
            let mut study = study();
            let attempt = run.execute(&mut study, Scope::default());
            // The attempt's own checked dataset keeps the unrounded
            // derivation; only the saved bytes round.
            let dataset = &attempt
                .dataset
                .result
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .dataset;
            let bits = |column: usize| match &dataset.rows()[1][column] {
                Value::Float(value) => value.get().to_bits(),
                other => panic!("a float cell, not {other:?}"),
            };
            assert_eq!(bits(1), 0x3ff3_be76_c8b4_3958);
            assert_eq!(bits(2), 0x4003_be76_c8b4_3958);
            let result =
                specification_report::build_result(&run, &attempt, identity(), &[]).unwrap();
            assert_eq!(
                result.observations()["artifacts"],
                yamaa_adapters::json!([])
            );
            drop(attempt);
            drop(run);
            // The saved bytes belong to the result, not to the run.
            let expected = b"ID,VALUE,DOUBLE\n1,0.13,0.25\n2,1.23,2.47\n3,2.35,4.69\n";
            assert_eq!(result.files(), [("output.csv", expected.as_slice())]);
            let report = result.saved_observations().unwrap();
            assert_eq!(
                report["artifacts"][0]["content"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                expected
            );
            assert_eq!(study.reads(), 1);
        }

        #[test]
        fn output_precision_rejection_follows_original_execution_failure_and_denies_publication() {
            for (precision, path, expression, condition) in [
                ("-1", "output.csv", "VALUE + VALUE", "invalid_field_type"),
                ("19", "output.csv", "VALUE + VALUE", "invalid_field_type"),
                (
                    "999999999999999999999999999999",
                    "output.csv",
                    "VALUE + VALUE",
                    "invalid_field_type",
                ),
                (
                    "2",
                    "output.parquet",
                    "VALUE + VALUE",
                    "decimals_not_applicable",
                ),
                ("-1", "output.csv", "1 / 0", "division_by_zero"),
                ("19", "output.csv", "1 / 0", "division_by_zero"),
            ] {
                let run = prepared(precision, path, expression);
                let attempt = run.execute(&mut study(), Scope::default());
                let result =
                    specification_report::build_result(&run, &attempt, identity(), &[]).unwrap();
                let report = result.observations();
                assert!(result.output().is_none());
                assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
                assert_eq!(report["diagnostics"][0]["condition"], condition);
                assert!(result.files().is_empty());
            }
        }

        #[test]
        fn compiler_keeps_arbitrary_width_precision_without_a_metadata_copy_allowance() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let precision = "9".repeat(1000);
            let document = schema
                .prepare_document(
                    Source {
                        identity: "spec.yaml".into(),
                        bytes: SPEC.edit("PRECISION", &precision).into_bytes(),
                    },
                    "spec.yaml".into(),
                    &mut crate::support::inheritance::Files,
                )
                .unwrap();
            let run = PreparedRun::prepare(document).unwrap();
            assert_eq!(run.compiled().output_decimals(), Some(precision.as_str()));
        }
    }
    mod csv_source {
        #[test]
        fn untyped_arrow_snapshot_owns_exact_source_cells_and_limits() {
            use yamaa_adapters::csv_source::{parse_text_table, TextTableError};
            use yamaa_core::table::{TableAccess, ValueRef};
            let mut input = b"A,B\n9007199254740993,\"\"\nx\0y,NA".to_vec();
            let table = parse_text_table(&input).unwrap();
            input.fill(0);
            drop(input);
            assert_eq!(table.row_count(), 2);
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Str("9007199254740993"));
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Str("x\0y"));
            assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("NA"));
            assert!(table.cell(2, 0).is_err());
            assert_eq!(parse_text_table(b"A,B").unwrap().row_count(), 0);
            let wide = (0..1_025)
                .map(|i| format!("C{i}"))
                .collect::<Vec<_>>()
                .join(",");
            assert!(matches!(
                parse_text_table(wide.as_bytes()),
                Err(TextTableError::Csv(
                    yamaa_adapters::csv_source::Error::Limit {
                        resource: "columns",
                        limit: 1_024,
                        actual: Some(1_025)
                    }
                ))
            ));
        }
    }
    #[cfg(feature = "parquet")]
    mod parquet_artifact {
        use parquet::{
            basic::{Compression, LogicalType, Repetition, Type},
            file::{reader::FileReader, serialized_reader::SerializedFileReader},
            record::{Field, RowAccessor},
        };
        use std::{
            convert::Infallible,
            fs::{File, OpenOptions},
            io::Write,
        };
        use yamaa_adapters::parquet_artifact::{render, Error};
        use yamaa_core::{
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        struct Table {
            schema: TableSchema,
            rows: Vec<Vec<Value>>,
        }
        impl TableAccess for Table {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows.len()
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                self.rows
                    .get(row)
                    .and_then(|r| r.get(column))
                    .map(ValueRef::from)
                    .ok_or(CellError::OutOfBounds { row, column })
            }
        }

        fn table(columns: &[(&str, ColumnType)], rows: Vec<Vec<Value>>) -> Table {
            Table {
                schema: TableSchema::new(
                    columns
                        .iter()
                        .map(|&(name, kind)| Column {
                            name: name.into(),
                            kind,
                        })
                        .collect(),
                )
                .unwrap(),
                rows,
            }
        }
        /// A Parquet file in its own scratch directory.
        struct Source(crate::support::Scratch);
        impl Source {
            fn new(bytes: &[u8]) -> Self {
                let directory = crate::support::Scratch::new("parquet-test");
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("source.parquet"))
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
                Self(directory)
            }
            fn reader(&self) -> SerializedFileReader<File> {
                SerializedFileReader::new(File::open(self.0.join("source.parquet")).unwrap())
                    .unwrap()
            }
        }

        #[test]
        fn closed_schema_and_exact_values_include_full_integers_temporals_nulls_and_zero_sign() {
            use ColumnType::*;
            let table = table(
                &[
                    ("S", Str),
                    ("I", Int),
                    ("F", Float),
                    ("D", Date),
                    ("T", DateTime),
                ],
                vec![
                    vec![
                        Value::Str("".into()),
                        Value::Int(i64::MIN),
                        Value::float(-0.0),
                        Value::Date("0001-01-01".parse().unwrap()),
                        Value::DateTime("1969-12-31T23:59:59".parse().unwrap()),
                    ],
                    vec![
                        Value::Missing,
                        Value::Int(i64::MAX),
                        Value::float(f64::from_bits(1)),
                        Value::Date("9999-12-31".parse().unwrap()),
                        Value::DateTime("9999-12-31T23:59:59".parse().unwrap()),
                    ],
                    vec![
                        Value::Str("a\u{e9}\n".into()),
                        Value::Int(9_007_199_254_740_993),
                        Value::float(1.2345678901234567),
                        Value::Missing,
                        Value::Missing,
                    ],
                ],
            );
            let bytes = render(&table, &[0, 1, 2, 3, 4]).unwrap();
            let source = Source::new(&bytes);
            let reader = source.reader();
            let meta = reader.metadata().file_metadata();
            assert!(meta.key_value_metadata().is_none());
            let schema = meta.schema_descr();
            // Textual schema truth avoids a release-specific LogicalType constructor.
            let timestamp = parquet::schema::parser::parse_message_type(
                "message expected { OPTIONAL INT64 at (TIMESTAMP(MICROS,false)); }",
            )
            .unwrap();
            let expected = [
                (Type::BYTE_ARRAY, Some(LogicalType::String)),
                (Type::INT64, None),
                (Type::DOUBLE, None),
                (Type::INT32, Some(LogicalType::Date)),
                (
                    Type::INT64,
                    timestamp.get_fields()[0].get_basic_info().logical_type(),
                ),
            ];
            for (i, (physical, logical)) in expected.into_iter().enumerate() {
                let field = schema.column(i);
                assert_eq!(field.physical_type(), physical);
                assert_eq!(field.logical_type(), logical);
                assert_eq!(
                    field.self_type().get_basic_info().repetition(),
                    Repetition::OPTIONAL
                );
                assert_eq!(
                    reader.metadata().row_group(0).column(i).compression(),
                    Compression::UNCOMPRESSED
                );
            }
            let rows = reader
                .get_row_iter(None)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            assert_eq!(rows[0].get_string(0).unwrap(), "");
            assert_eq!(rows[1].get_column_iter().next().unwrap().1, &Field::Null);
            assert_eq!(rows[2].get_string(0).unwrap(), "a\u{e9}\n");
            assert_eq!(rows[0].get_long(1).unwrap(), i64::MIN);
            assert_eq!(rows[1].get_long(1).unwrap(), i64::MAX);
            assert_eq!(rows[2].get_long(1).unwrap(), 9_007_199_254_740_993);
            assert_eq!(
                rows[0].get_double(2).unwrap().to_bits(),
                (-0.0f64).to_bits()
            );
            assert_eq!(rows[1].get_double(2).unwrap().to_bits(), 1);
            assert_eq!(
                rows[2].get_double(2).unwrap().to_bits(),
                1.2345678901234567f64.to_bits()
            );
            assert_eq!(
                rows[0].get_column_iter().nth(3).unwrap().1,
                &Field::Date(-719_162)
            );
            assert_eq!(
                rows[1].get_column_iter().nth(3).unwrap().1,
                &Field::Date(2_932_896)
            );
            assert_eq!(rows[0].get_timestamp_micros(4).unwrap(), -1_000_000);
            assert_eq!(
                rows[1].get_timestamp_micros(4).unwrap(),
                253_402_300_799_000_000
            );
            assert_eq!(rows[2].get_column_iter().nth(3).unwrap().1, &Field::Null);
            assert_eq!(rows[2].get_column_iter().nth(4).unwrap().1, &Field::Null);
        }

        #[test]
        fn opaque_cell_failures_retain_the_original_payload() {
            struct Failed {
                schema: TableSchema,
                token: std::sync::Arc<()>,
            }
            impl TableAccess for Failed {
                type Error = std::sync::Arc<()>;
                fn schema(&self) -> &TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    1
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                    Err(CellError::Access(self.token.clone()))
                }
            }
            let failed = Failed {
                schema: table(&[("S", ColumnType::Str)], vec![]).schema,
                token: std::sync::Arc::new(()),
            };
            let Err(Error::Cell(CellError::Access(original))) = render(&failed, &[0]) else {
                panic!("opaque cell error must survive encoding");
            };
            assert!(std::sync::Arc::ptr_eq(&original, &failed.token));
        }

        #[test]
        fn logical_records_preserve_json_spelling_without_rounding_or_partial_dates() {
            let table = table(
                &[
                    ("S", ColumnType::Str),
                    ("F", ColumnType::Float),
                    ("I", ColumnType::Int),
                ],
                vec![
                    vec![
                        Value::Str("\u{1f600}\u{7f}\n\"".into()),
                        Value::float(-0.0),
                        Value::Int(i64::MIN),
                    ],
                    vec![
                        Value::Missing,
                        Value::float(1e-7),
                        Value::Int(9_007_199_254_740_993),
                    ],
                ],
            );
            let rows = yamaa_adapters::parquet_artifact::records(&table, &[0, 1, 2]).unwrap();
            assert_eq!(
                rows,
                [
                    r#"["S", "F", "I"]"#,
                    r#"["\ud83d\ude00\u007f\n\"", -0.0, -9223372036854775808]"#,
                    r#"[null, 1e-07, 9007199254740993]"#,
                ]
            );
        }

        #[test]
        fn ordered_row_groups_and_projection_survive_an_empty_source() {
            let table = table(
                &[("A", ColumnType::Int), ("B", ColumnType::Int)],
                (0..2051)
                    .map(|i| vec![Value::Int(i), Value::Int(-i)])
                    .collect(),
            );
            let source = Source::new(&render(&table, &[1, 0]).unwrap());
            let reader = source.reader();
            assert_eq!(
                reader
                    .metadata()
                    .row_groups()
                    .iter()
                    .map(|g| g.num_rows())
                    .collect::<Vec<_>>(),
                [1024, 1024, 3]
            );
            assert_eq!(
                reader
                    .metadata()
                    .file_metadata()
                    .schema_descr()
                    .column(0)
                    .name(),
                "B"
            );
            for (i, row) in reader.get_row_iter(None).unwrap().enumerate() {
                let row = row.unwrap();
                assert_eq!(row.get_long(0).unwrap(), -(i as i64));
                assert_eq!(row.get_long(1).unwrap(), i as i64);
            }
            let empty = Table {
                schema: table.schema,
                rows: vec![],
            };
            let source = Source::new(&render(&empty, &[1]).unwrap());
            let reader = source.reader();
            assert_eq!(reader.metadata().file_metadata().num_rows(), 0);
            assert_eq!(
                reader
                    .metadata()
                    .file_metadata()
                    .schema_descr()
                    .column(0)
                    .name(),
                "B"
            );
        }

        #[test]
        fn invalid_projection_and_cell_types_produce_no_artifact() {
            let table = table(
                &[("S", ColumnType::Str)],
                vec![vec![Value::Str("x".repeat(4096))]],
            );
            for projection in [&[][..], &[0, 0], &[1]] {
                assert!(matches!(render(&table, projection), Err(Error::Projection)));
            }
            let wrong = Table {
                schema: table.schema,
                rows: vec![vec![Value::Int(1)]],
            };
            assert!(matches!(render(&wrong, &[0]), Err(Error::ValueType)));
        }

        #[test]
        fn fixed_geometry_is_admitted_before_output_cell_access() {
            use yamaa_adapters::csv_artifact::{self, Error as CsvError};
            struct Geometry {
                schema: TableSchema,
                rows: usize,
            }
            impl TableAccess for Geometry {
                type Error = &'static str;
                fn schema(&self) -> &TableSchema {
                    &self.schema
                }
                fn row_count(&self) -> usize {
                    self.rows
                }
                fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
                    Err(CellError::Access("admitted table"))
                }
            }
            for (rows, columns, resource, actual, exceeds) in [
                (9_999_999, 1, "table_rows", 9_999_999, false),
                (10_000_001, 1, "table_rows", 10_000_001, true),
                (1, 1023, "columns", 1023, false),
                (1, 1025, "columns", 1025, true),
                (3_575_418, 179, "table_cells", 639_999_822, false),
                (3_575_419, 179, "table_cells", 640_000_001, true),
            ] {
                let schema = TableSchema::new(
                    (0..columns)
                        .map(|column| Column {
                            name: format!("F{column}"),
                            kind: ColumnType::Int,
                        })
                        .collect(),
                )
                .unwrap();
                let table = Geometry { schema, rows };
                let projection = (0..columns).collect::<Vec<_>>();
                macro_rules! check {
                    ($result:expr, $error:ident) => {
                        match ($result, exceeds) {
                            (
                                Err($error::Limit {
                                    resource: observed,
                                    actual: Some(required),
                                    ..
                                }),
                                true,
                            ) => assert_eq!((observed, required), (resource, actual)),
                            (Err($error::Cell(CellError::Access("admitted table"))), false) => {}
                            (other, _) => panic!("unexpected {resource} admission: {other:?}"),
                        }
                    };
                }
                check!(csv_artifact::render(&table, &projection, None), CsvError);
                check!(render(&table, &projection), Error);
                check!(
                    yamaa_adapters::parquet_artifact::records(&table, &projection),
                    Error
                );
            }
        }

        #[test]
        fn artifacts_keep_complete_temporal_fields_and_drop_collected_precision() {
            use yamaa_core::temporal::{Date, DateTime, Precision};
            // VALUE-17: precision is internal provenance. Even imputed values carry
            // complete calendar fields; artifacts deliberately retain only those fields.
            let table = table(
                &[("D", ColumnType::Date), ("T", ColumnType::DateTime)],
                [
                    (Precision::Year, Precision::Day),
                    (Precision::Month, Precision::Day),
                    (Precision::Day, Precision::Second),
                ]
                .into_iter()
                .map(|(date_precision, time_precision)| {
                    vec![
                        Value::Date(Date::new(1970, 2, 28, date_precision).unwrap()),
                        Value::DateTime(
                            DateTime::new(
                                Date::new(1970, 1, 1, Precision::Day).unwrap(),
                                23,
                                59,
                                59,
                                time_precision,
                            )
                            .unwrap(),
                        ),
                    ]
                })
                .collect(),
            );
            let bytes = render(&table, &[0, 1]).unwrap();
            let source = Source::new(&bytes);
            let reader = source.reader();
            assert!(reader
                .metadata()
                .file_metadata()
                .key_value_metadata()
                .is_none());
            let rows = reader
                .get_row_iter(None)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 3);
            for row in &rows {
                assert_eq!(row.get_column_iter().next().unwrap().1, &Field::Date(58));
                assert_eq!(row.get_timestamp_micros(1).unwrap(), 86_399_000_000);
            }
            assert_eq!(
                yamaa_adapters::parquet_artifact::records(&table, &[0, 1]).unwrap(),
                [
                    r#"["D", "T"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                    r#"["1970-02-28", "1970-01-01T23:59:59"]"#,
                ],
            );
        }

        #[test]
        fn retired_python_double_operands_round_trip_without_display_rounding() {
            let values = [0.1, 1.0 / 3.0, f64::from_bits(1), 1e308, -0.0, 2.675];
            let table = table(
                &[("USUBJID", ColumnType::Str), ("VALUE", ColumnType::Float)],
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        vec![Value::Str(format!("S-{index}")), Value::float(value)]
                    })
                    .collect(),
            );
            let source = Source::new(&render(&table, &[0, 1]).unwrap());
            let reader = source.reader();
            let rows = reader
                .get_row_iter(None)
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), values.len());
            for (row, expected) in rows.iter().zip(values) {
                assert_eq!(row.get_double(1).unwrap().to_bits(), expected.to_bits());
            }
        }
    }
    #[cfg(feature = "yaml")]
    mod predicate_diagnostics {
        //! Complete original-document outcomes captured independently from Python.
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;

        #[test]
        fn original_regex_and_temporal_reports_match_independently_captured_reference() {
            use crate::support::ports::Sources;
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let mut count = 0;
            for row in include_str!("fixtures/predicate_context_diagnostics.tsv")
                .lines()
                .skip(1)
            {
                let fields = row.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 3);
                let prepared = schema
                    .prepare_document(
                        Source {
                            identity: "spec.yaml".into(),
                            bytes: fields[1].as_bytes().to_vec(),
                        },
                        "spec.yaml".into(),
                        &mut crate::support::inheritance::Files,
                    )
                    .unwrap();
                let run = PreparedRun::prepare(prepared).unwrap();
                let mut port = Sources::by_path([("source.csv", b"ID,V\n1,2\n".as_slice())]);
                let attempt = run.execute(&mut port, Scope::default());
                let example = format!("predicate-context-{}", fields[0]);
                let result = specification_report::build_result(
                    &run,
                    &attempt,
                    Identity {
                        runtime: "python",
                        runtime_version: "fixture-runtime",
                        engine_version: "fixture-engine",
                        example: &example,
                        specification: "spec.yaml",
                        base_directory: ".",
                    },
                    &[],
                )
                .unwrap();
                let expected: yamaa_core::json::Value =
                    yamaa_adapters::json_wire::decode(fields[2]).unwrap();
                assert_eq!(result.observations(), expected, "{}", fields[0]);
                assert!(result.output().is_none());
                drop(attempt);
                drop(run);
                // A failed predicate attempt has no file to save.
                assert!(result.files().is_empty(), "{}", fields[0]);
                assert_eq!(result.observations(), expected);
                assert_eq!(port.captured(), ["SRC"]);
                count += 1;
            }
            assert_eq!(count, 2);
        }
    }
    mod public_table {
        use arrow_array::{
            Array, ArrayRef, Date32Array, Float64Array, Int64Array, RecordBatch, StringArray,
            TimestampSecondArray,
        };
        use arrow_ipc::reader::StreamReader;
        use std::{io::Cursor, sync::Arc};
        use yamaa_adapters::{
            arrow_table::{date_array, datetime_array, physical_schema, ArrowTable},
            public_table::PublicTable,
        };
        use yamaa_core::{
            table::{Column, TableSchema},
            temporal::{Date, DateTime, Precision},
            value::ColumnType,
        };

        fn source(rows: usize) -> ArrowTable {
            let schema = TableSchema::new(vec![
                Column {
                    name: "S".into(),
                    kind: ColumnType::Str,
                },
                Column {
                    name: "I".into(),
                    kind: ColumnType::Int,
                },
                Column {
                    name: "F".into(),
                    kind: ColumnType::Float,
                },
                Column {
                    name: "D".into(),
                    kind: ColumnType::Date,
                },
                Column {
                    name: "T".into(),
                    kind: ColumnType::DateTime,
                },
            ])
            .unwrap();
            let date = Date::new(1970, 1, 1, Precision::Year).unwrap();
            let time = DateTime::new(date, 0, 0, 0, Precision::Day).unwrap();
            let strings = [Some("nul\0text"), None, Some("")];
            let integers = [Some(i64::MIN), None, Some(i64::MAX)];
            let floats = [Some(-0.0), None, Some(1.25)];
            let dates = [
                Some(date),
                None,
                Some(Date::new(1969, 12, 31, Precision::Day).unwrap()),
            ];
            let times = [
                Some(time),
                None,
                Some(DateTime::new(date, 0, 0, 1, Precision::Second).unwrap()),
            ];
            let arrays: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(strings[..rows].to_vec())),
                Arc::new(Int64Array::from(integers[..rows].to_vec())),
                Arc::new(Float64Array::from(floats[..rows].to_vec())),
                Arc::new(date_array(&dates[..rows]).unwrap()),
                Arc::new(datetime_array(&times[..rows]).unwrap()),
            ];
            let batch = RecordBatch::try_new(physical_schema(&schema), arrays).unwrap();
            ArrowTable::try_new(schema, vec![batch]).unwrap()
        }
        #[test]
        fn public_columns_preserve_exact_values_missing_and_temporal_null_masks() {
            let table = PublicTable::from_snapshot(&source(3)).unwrap();
            assert_eq!(table.rows(), 3);
            assert_eq!(
                table.batches()[0]
                    .column(1)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>(),
                vec![Some(i64::MIN), None, Some(i64::MAX)]
            );
            let mut stream =
                StreamReader::try_new(Cursor::new(table.ipc().unwrap()), None).unwrap();
            let batch = stream.next().unwrap().unwrap();
            let strings = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(strings.value(0), "nul\0text");
            assert!(strings.is_null(1));
            assert_eq!(strings.value(2), "");
            let integers = batch
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            assert_eq!(integers.value(0), i64::MIN);
            assert!(integers.is_null(1));
            assert_eq!(integers.value(2), i64::MAX);
            let floats = batch
                .column(2)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap();
            assert_eq!(floats.value(0).to_bits(), (-0.0f64).to_bits());
            assert!(floats.is_null(1));
            let dates = batch
                .column(3)
                .as_any()
                .downcast_ref::<Date32Array>()
                .unwrap();
            assert_eq!(dates.value(0), 0);
            assert!(dates.is_null(1));
            assert_eq!(dates.value(2), -1);
            let times = batch
                .column(4)
                .as_any()
                .downcast_ref::<TimestampSecondArray>()
                .unwrap();
            assert_eq!(times.value(0), 0);
            assert!(times.is_null(1));
            assert_eq!(times.value(2), 1);
            assert!(stream.next().is_none());
        }
        #[test]
        fn empty_public_table_retains_every_declared_column_type() {
            let table = PublicTable::from_snapshot(&source(0)).unwrap();
            assert_eq!(table.rows(), 0);
            let mut stream =
                StreamReader::try_new(Cursor::new(table.ipc().unwrap()), None).unwrap();
            let batch = stream.next().unwrap().unwrap();
            assert_eq!(batch.num_rows(), 0);
            assert_eq!(batch.num_columns(), 5);
            assert!(batch.column(0).as_any().is::<StringArray>());
            assert!(batch.column(1).as_any().is::<Int64Array>());
            assert!(batch.column(2).as_any().is::<Float64Array>());
            assert!(batch.column(3).as_any().is::<Date32Array>());
            assert!(batch.column(4).as_any().is::<TimestampSecondArray>());
        }
    }
}
