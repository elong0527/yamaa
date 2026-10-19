//! Table contracts.
#[allow(dead_code)]
mod support;
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
        use yamaa_core::table_reduction::{reduce_column, TableReductionError};
        use yamaa_core::{
            numeric::{ArithmeticErrorKind, Number},
            reduction::{NumericReducer, ReductionError},
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
        /// Run the engine consumer across chunks, preserving rounding and overflow order.
        fn actual_engine_reductions_cross_chunks_without_partial_sums() {
            let schema = logical_schema(&[("A", CT::Float)]);
            let chunks = vec![
                batch(
                    &schema,
                    vec![Arc::new(Float64Array::from(vec![Some(1e16)]))],
                ),
                batch(
                    &schema,
                    vec![Arc::new(Float64Array::from(vec![
                        Some(1.0),
                        None,
                        Some(-1e16),
                        Some(1.0),
                    ]))],
                ),
            ];
            let table = ArrowTable::try_new(schema, chunks).unwrap();
            assert_eq!(
                reduce_column(&table, 0, &[0, 1, 2, 3, 4], NumericReducer::Sum, "SUM(A)"),
                Ok(Number::float(1.0))
            );
            assert_eq!(
                reduce_column(&table, 0, &[0, 1, 2, 3, 4], NumericReducer::Mean, "MEAN(A)"),
                Ok(Number::float(0.25))
            );
            let schema = logical_schema(&[("A", CT::Int)]);
            let chunks = [i64::MAX, 1, -1]
                .map(|value| batch(&schema, vec![Arc::new(Int64Array::from(vec![value]))]));
            let table = ArrowTable::try_new(schema, chunks.into()).unwrap();
            let result = reduce_column(&table, 0, &[0, 1, 2], NumericReducer::Sum, "SUM(A)");
            assert!(
                matches!(result,Err(TableReductionError::Reduction(ReductionError::Arithmetic { argument:1,error })) if error.kind == ArithmeticErrorKind::IntegerOverflow { value:i64::MAX as i128+1 })
            );
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
        use crate::support::ipc_snapshot;
        use std::{convert::Infallible, sync::Arc};
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::specification::{PreparedSpecification, SourceDeclaration};
        use yamaa_core::{
            specification_output::SaveError, specification_run::SpecificationSourcePort,
        };

        const SPEC: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {VALUE: float}}}\noutput: {path: output.csv, columns: [ID, VALUE, DOUBLE], decimals: PRECISION}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VALUE, type: float, derivation: SRC.VALUE}\n  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}\n";
        fn prepared(precision: &str, path: &str, expression: &str) -> PreparedRun {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let bytes = SPEC
                .replace("PRECISION", precision)
                .replace("output.csv", path)
                .replace("VALUE + VALUE", expression);
            let document = schema
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: bytes.into_bytes(),
                })
                .unwrap();
            PreparedRun::prepare(document).unwrap()
        }
        struct Study(usize);
        impl SpecificationSourcePort for Study {
            fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = Infallible;
            fn capture_reads(&self) -> usize {
                self.0
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, Infallible> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                assert_eq!(source.path, "input.csv");
                self.0 += 1;
                Ok(Arc::from(
                    b"ID,VALUE\n1,0.125\n2,1.234\n3,2.345\n".as_slice(),
                ))
            }
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
        #[derive(Default)]
        struct Publisher {
            bytes: Vec<u8>,
            calls: usize,
            reject: bool,
        }
        impl specification_report::ArtifactPort for Publisher {
            type Error = &'static str;
            fn publish(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
                self.calls += 1;
                assert_eq!(path, "output.csv");
                if self.reject {
                    return Err("original publication failure");
                }
                self.bytes = content.into();
                Ok(())
            }
        }

        #[test]
        fn whole_result_keeps_unrounded_derivation_and_prepares_exact_saved_bytes_once() {
            let run = prepared("2", "output.csv", "VALUE + VALUE");
            let mut study = Study(0);
            let attempt = run.execute_with_port(&mut study, Scope::default());
            let table = attempt.result.as_ref().unwrap().table.as_ref().unwrap();
            let snapshot: yamaa_core::json::Value =
                yamaa_adapters::json_wire::decode(&ipc_snapshot::table_snapshot(table)).unwrap();
            assert_eq!(
                snapshot["rows"][1][1],
                yamaa_adapters::json!({"float": "3ff3be76c8b43958"})
            );
            assert_eq!(
                snapshot["rows"][1][2],
                yamaa_adapters::json!({"float": "4003be76c8b43958"})
            );
            let result = specification_report::build_result(&run, &attempt, identity()).unwrap();
            assert_eq!(
                result.observations()["artifacts"],
                yamaa_adapters::json!([])
            );
            let mut publisher = Publisher {
                reject: true,
                ..Default::default()
            };
            assert!(matches!(
                result.save(&mut publisher),
                Err(SaveError::Publish("original publication failure"))
            ));
            assert!(publisher.bytes.is_empty());
            publisher.reject = false;
            drop(attempt);
            drop(run);
            let report = result.save(&mut publisher).unwrap();
            let expected = b"ID,VALUE,DOUBLE\n1,0.13,0.25\n2,1.23,2.47\n3,2.35,4.69\n";
            assert_eq!(publisher.bytes, expected);
            assert_eq!(
                report["artifacts"][0]["content"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                expected
            );
            result.save(&mut publisher).unwrap();
            assert_eq!(publisher.calls, 3);
            assert_eq!(study.0, 1);
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
                let attempt = run.execute_with_port(&mut Study(0), Scope::default());
                let result =
                    specification_report::build_result(&run, &attempt, identity()).unwrap();
                let report = result.observations();
                assert!(result.output().is_none());
                assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
                assert_eq!(report["diagnostics"][0]["condition"], condition);
                let mut publisher = Publisher::default();
                assert!(matches!(
                    result.save(&mut publisher),
                    Err(SaveError::FailedBuild)
                ));
                assert_eq!(publisher.calls, 0);
            }
        }

        #[test]
        fn compiler_keeps_arbitrary_width_precision_without_a_metadata_copy_allowance() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let precision = "9".repeat(1000);
            let document = schema
                .prepare_standalone(Source {
                    identity: "spec.yaml".into(),
                    bytes: SPEC.replace("PRECISION", &precision).into_bytes(),
                })
                .unwrap();
            let compiled = PreparedSpecification::prepare(document.model()).unwrap();
            assert_eq!(compiled.output_decimals(), Some(precision.as_str()));
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
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
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
        struct Source(PathBuf);
        impl Source {
            fn new(bytes: &[u8]) -> Self {
                static NEXT: AtomicUsize = AtomicUsize::new(0);
                let path = std::env::temp_dir().join(format!(
                    "yamaa-parquet-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
                Self(path)
            }
            fn reader(&self) -> SerializedFileReader<File> {
                SerializedFileReader::new(File::open(&self.0).unwrap()).unwrap()
            }
        }
        impl Drop for Source {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
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
                    timestamp.get_fields()[0]
                        .get_basic_info()
                        .logical_type_ref()
                        .cloned(),
                ),
            ];
            for (i, (physical, logical)) in expected.into_iter().enumerate() {
                let field = schema.column(i);
                assert_eq!(field.physical_type(), physical);
                assert_eq!(field.logical_type_ref(), logical.as_ref());
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
            // REQ-0570: precision is internal provenance. Even imputed values carry
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
    #[cfg(feature = "parquet")]
    mod parquet_source {
        //! Independently authored PyArrow containers exercise the held-byte input codec.
        mod unknown_fields {
            //! Independent Compact wire extensions of the authored PyArrow fixture.
            //! No candidate writer supplies either the extension bytes or expected cells.
            use yamaa_adapters::parquet_source::{parse, Error};
            use yamaa_core::{
                table::{TableAccess, ValueRef},
                temporal::Precision,
                value::ColumnType,
            };

            const SOURCE: &[u8] = include_bytes!("fixtures/pq/none-1-0.parquet");

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
                assert_eq!(bytes.len(), 67_625);
                assert_eq!(
                    u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap()),
                    66_951
                );
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
                // the next schema element. Check the authored wire prefix before editing.
                let end = SOURCE.len() - 8;
                let length = u32::from_le_bytes(SOURCE[end..end + 4].try_into().unwrap());
                let start = end - length as usize;
                assert_eq!(
                    &SOURCE[start..start + 17],
                    &[
                        0x15, 4, 0x19, 0x6c, 0x35, 0, 0x18, 6, b's', b'c', b'h', b'e', b'm', b'a',
                        0x15, 10, 0
                    ]
                );
                let mut bytes = SOURCE[..start + 16].to_vec();
                bytes.extend([9, 0xe0, 0xd4, 3, 0x21, 1, 2]);
                bytes.extend(&SOURCE[start + 16..end]);
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
                // Authored first DATA_PAGE has a ten-byte body: extend both size claims
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
        use std::path::Path;
        use yamaa_adapters::parquet_source::{parse, Error};
        use yamaa_core::{
            table::{TableAccess, ValueRef},
            temporal::Precision,
            value::ColumnType,
        };

        fn content(name: &str) -> Vec<u8> {
            std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/pq")
                    .join(format!("{name}.parquet")),
            )
            .unwrap()
        }

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
            // signed 32-bit integer (REQ-1032). The fixture keeps the name it
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

        mod row_groups {
            //! More row groups than parquet 57.3.0 decodes from one footer
            //! (#2940), one record per row group, as pyarrow and Polars write
            //! them with `row_group_size=1`. The parquet crate's writer refuses
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
                // (REQ-1037), whichever row groups hold an ordinal.
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
                arrow_table::ArrowTable,
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
            fn assert_int_cells(table: &ArrowTable, columns: &[Vec<Option<i64>>]) {
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
            //! type still decides the column type (REQ-1032). Expected cells are
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
    #[cfg(feature = "yaml")]
    mod predicate_diagnostics {
        //! Complete original-document outcomes captured independently from Python.
        use std::sync::Arc;
        use yamaa_adapters::{
            specification_report::{self, ArtifactPort, Identity},
            specification_run::{PreparedRun, SpecificationSourcePort},
            specification_source::Source,
        };
        use yamaa_core::application_observer::Scope;
        use yamaa_core::specification::SourceDeclaration;

        struct Port {
            captures: usize,
        }
        impl SpecificationSourcePort for Port {
            fn inspect(&mut self, _source: &SourceDeclaration) -> Result<(), Self::Error> {
                Ok(())
            }
            fn resource_failure(
                &self,
                _error: &Self::Error,
            ) -> Option<yamaa_core::resource::ResourceFailure> {
                None
            }
            type Error = ();
            fn capture_reads(&self) -> usize {
                self.captures
            }
            fn capture(
                &mut self,
                source: &SourceDeclaration,
                origin: &mut Option<yamaa_core::specification_run::SourceOrigin>,
            ) -> Result<Arc<[u8]>, ()> {
                *origin = Some(yamaa_core::specification_run::SourceOrigin::Resource);
                assert_eq!((&*source.name, &*source.path), ("SRC", "source.csv"));
                self.captures += 1;
                Ok(Arc::from(b"ID,V\n1,2\n".as_slice()))
            }
        }
        impl ArtifactPort for Port {
            type Error = ();
            fn publish(&mut self, _: &str, _: &[u8]) -> Result<(), ()> {
                panic!("failed predicate attempt cannot publish")
            }
        }
        #[test]
        fn original_regex_and_temporal_reports_match_independently_captured_reference() {
            let schema = yamaa_adapters::shipped_schema::capture().unwrap();
            let mut count = 0;
            for row in include_str!("fixtures/predicate_context_diagnostics.tsv")
                .lines()
                .skip(1)
            {
                let fields = row.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 3);
                let prepared = schema
                    .prepare_standalone(Source {
                        identity: "spec.yaml".into(),
                        bytes: fields[1].as_bytes().to_vec(),
                    })
                    .unwrap();
                let run = PreparedRun::prepare(prepared).unwrap();
                let mut port = Port { captures: 0 };
                let attempt = run.execute_with_port(&mut port, Scope::default());
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
                )
                .unwrap();
                let expected: yamaa_core::json::Value =
                    yamaa_adapters::json_wire::decode(fields[2]).unwrap();
                assert_eq!(result.observations(), expected, "{}", fields[0]);
                assert!(result.output().is_none());
                drop(attempt);
                drop(run);
                for _ in 0..2 {
                    assert!(matches!(
                        result.save(&mut port),
                        Err(yamaa_core::specification_output::SaveError::FailedBuild)
                    ));
                    assert_eq!(result.observations(), expected);
                }
                assert_eq!(port.captures, 1);
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
