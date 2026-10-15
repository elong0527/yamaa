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
            date_array, datetime_array, physical_schema, ArrowTable, TableError, TemporalCellError,
        };
        use yamaa_core::table_reduction::{reduce_column, TableReductionError};
        use yamaa_core::{
            numeric::{ArithmeticErrorKind, Number},
            reduction::{NumericReducer, ReductionError},
            table::{CellError, TableAccess, TableSchema, ValueRef},
            temporal::{Date, DatePrecision as DP, DateTime, DateTimePrecision as TP},
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
            for (values, precisions, reason) in [
                (vec![0, 0], vec![2, 3], TemporalCellError::Precision),
                (vec![0, i32::MIN], vec![2, 2], TemporalCellError::Range),
                (vec![0, 2932897], vec![2, 2], TemporalCellError::Range),
            ] {
                let schema = logical_schema(&[("D", CT::Date)]);
                let input = batch(&schema, vec![Arc::new(raw_dates(values, precisions, None))]);
                assert!(
                    matches!(ArrowTable::try_new(schema,vec![input]),Err(TableError::Temporal { batch:0,column:0,row:1,reason: actual }) if actual == reason)
                );
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
            for (seconds, code, reason) in [
                (i64::MIN, 1, TemporalCellError::Range),
                (i64::MAX, 1, TemporalCellError::Range),
                (-62135596801, 1, TemporalCellError::Range),
                (253402300800, 1, TemporalCellError::Range),
                (0, 2, TemporalCellError::Precision),
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
                assert!(
                    matches!(ArrowTable::try_new(schema.clone(),vec![input]),Err(TableError::Temporal { reason:actual,.. }) if actual == reason)
                );
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
            let expected = b"S,F,I\n\"\",0.13,1\n\"a,b\",2.67,2\n,,3\nz,0.00,4\n";
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
    mod csv_precision {
        use crate::support::ipc_snapshot;
        use std::{convert::Infallible, sync::Arc};
        use yamaa_adapters::{
            specification_report::{self, Identity},
            specification_run::PreparedRun,
            specification_source::Source,
        };
        use yamaa_core::specification::{PreparedSpecification, SourceDeclaration};
        use yamaa_core::{specification_output::SaveError, specification_run::SourcePort};

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
        impl SourcePort for Study {
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
            let attempt = run.execute_with_port(&mut study);
            let table = attempt.result.as_ref().unwrap().table.as_ref().unwrap();
            let snapshot: serde_json::Value =
                serde_json::from_str(&ipc_snapshot::table_snapshot(table).unwrap()).unwrap();
            assert_eq!(
                snapshot["rows"][1][1],
                serde_json::json!({"float": "3ff3be76c8b43958"})
            );
            assert_eq!(
                snapshot["rows"][1][2],
                serde_json::json!({"float": "4003be76c8b43958"})
            );
            let result = specification_report::build_result(&run, &attempt, identity()).unwrap();
            assert_eq!(result.observations()["artifacts"], serde_json::json!([]));
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
                let attempt = run.execute_with_port(&mut Study(0));
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
    mod ipc_table {
        use crate::support::ipc_snapshot;
        use arrow_array::{
            Array, Date32Array, Int64Array, RecordBatch, RecordBatchOptions, StringArray,
            StructArray, UInt8Array,
        };
        use arrow_ipc::{reader::StreamReader, writer::StreamWriter};
        use arrow_schema::{DataType, Field, Schema};
        use ipc_snapshot::table_snapshot;
        use std::{io::Cursor, sync::Arc};
        use yamaa_adapters::ipc_table::{table_round_trip, TableError as E};

        /// Replay independent truth using PyArrow-authored input, not candidate output.
        #[test]
        fn shared_installed_table_truth_and_repeated_ownership() {
            let expected: serde_json::Value =
                serde_json::from_str(include_str!("fixtures/tables/expected.json")).unwrap();
            for (name, input) in [
                (
                    "mixed",
                    include_bytes!("fixtures/tables/mixed.arrow").as_slice(),
                ),
                (
                    "empty",
                    include_bytes!("fixtures/tables/empty.arrow").as_slice(),
                ),
                (
                    "schema_only",
                    include_bytes!("fixtures/tables/schema_only.arrow").as_slice(),
                ),
                (
                    "zero_columns",
                    include_bytes!("fixtures/tables/zero_columns.arrow").as_slice(),
                ),
            ] {
                let before: serde_json::Value =
                    serde_json::from_str(&table_snapshot(input).unwrap()).unwrap();
                assert_eq!(before, expected[name], "{name}");
                let mut owned = table_round_trip(input).unwrap();
                for _ in 0..5 {
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&table_snapshot(&owned).unwrap())
                            .unwrap(),
                        expected[name],
                        "{name}"
                    );
                    owned = table_round_trip(&owned).unwrap();
                }
            }
        }

        /// Reject truncation and arbitrary framing without a panic/internal outcome.
        #[test]
        fn every_truncation_and_bad_frame_is_rejected_before_use() {
            let input = include_bytes!("fixtures/tables/mixed.arrow");
            for end in 0..input.len() {
                assert!(
                    matches!(
                        table_snapshot(&input[..end]),
                        Err(E::InvalidStream | E::InvalidTable | E::Limit { .. })
                    ),
                    "prefix {end}"
                );
            }
            for input in [
                vec![],
                vec![255; 8],
                vec![0; 8],
                vec![1, 0, 0, 128],
                b"ARROW1".to_vec(),
            ] {
                assert!(matches!(
                    table_round_trip(&input),
                    Err(E::InvalidStream | E::Limit { .. })
                ));
            }
            let mut trailing = input.to_vec();
            trailing.push(0);
            assert_eq!(table_snapshot(&trailing), Err(E::InvalidStream));
            assert!(table_snapshot(input).is_ok());
        }

        /// Encode a safe in-memory batch for deliberately unsupported/oversized requests.
        fn stream(batch: &RecordBatch) -> Vec<u8> {
            let mut output = vec![];
            let mut writer = StreamWriter::try_new(&mut output, &batch.schema()).unwrap();
            writer.write(batch).unwrap();
            writer.finish().unwrap();
            output
        }

        /// Read normalized public output through the independent Arrow decoder.
        fn read(bytes: Vec<u8>) -> Vec<RecordBatch> {
            StreamReader::try_new(Cursor::new(bytes), None)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        }

        /// Null exports must erase hidden scalar bytes, temporal children and text data.
        #[test]
        fn public_export_sanitizes_hidden_payloads() {
            use arrow_buffer::{NullBuffer, ScalarBuffer};
            use yamaa_adapters::arrow_table::physical_schema;
            use yamaa_core::{
                table::{Column, TableSchema},
                value::ColumnType as CT,
            };
            let schema = TableSchema::new(vec![
                Column {
                    name: "I".into(),
                    kind: CT::Int,
                },
                Column {
                    name: "S".into(),
                    kind: CT::Str,
                },
                Column {
                    name: "D".into(),
                    kind: CT::Date,
                },
            ])
            .unwrap();
            let hidden = Int64Array::new(
                ScalarBuffer::from(vec![424242, 7]),
                Some(NullBuffer::from(vec![false, true])),
            );
            let strings = StringArray::new(
                arrow_buffer::OffsetBuffer::new(ScalarBuffer::from(vec![0, 6, 8])),
                arrow_buffer::Buffer::from(b"SECRETok".as_slice()),
                Some(NullBuffer::from(vec![false, true])),
            );
            let physical = physical_schema(&schema);
            let DataType::Struct(fields) = physical.field(2).data_type() else {
                panic!("struct")
            };
            let dates = StructArray::try_new(
                fields.clone(),
                vec![
                    Arc::new(Date32Array::from(vec![i32::MIN, 0])),
                    Arc::new(UInt8Array::from(vec![255, 2])),
                ],
                Some(NullBuffer::from(vec![false, true])),
            )
            .unwrap();
            let batch = RecordBatch::try_new(
                physical,
                vec![Arc::new(hidden), Arc::new(strings), Arc::new(dates)],
            )
            .unwrap();
            let output = table_round_trip(&stream(&batch)).unwrap();
            assert!(!output.windows(6).any(|bytes| bytes == b"SECRET"));
            let batches = read(output);
            let batch = &batches[0];
            assert_eq!(
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .unwrap()
                    .values()
                    .as_ref(),
                [0, 7]
            );
            let date = batch
                .column(2)
                .as_any()
                .downcast_ref::<StructArray>()
                .unwrap();
            assert_eq!(
                date.column(0)
                    .as_any()
                    .downcast_ref::<Date32Array>()
                    .unwrap()
                    .value(0),
                0
            );
            assert_eq!(
                date.column(1)
                    .as_any()
                    .downcast_ref::<UInt8Array>()
                    .unwrap()
                    .value(0),
                0
            );
            assert!(date.is_null(0));
        }

        /// Oversized declared rows and noncanonical types never enter evaluation.
        #[test]
        fn shape_and_schema_policy_are_enforced() {
            let empty = Arc::new(Schema::empty());
            let batch = RecordBatch::try_new_with_options(
                empty,
                vec![],
                &RecordBatchOptions::new().with_row_count(Some(10_000_001)),
            )
            .unwrap();
            assert_eq!(
                table_snapshot(&stream(&batch)),
                Err(E::Limit {
                    resource: "table_rows",
                    limit: 10_000_000,
                    required: Some(10_000_001)
                })
            );
            let schema = Arc::new(Schema::new(vec![Field::new("I", DataType::Int64, false)]));
            let batch =
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![1]))]).unwrap();
            assert_eq!(table_snapshot(&stream(&batch)), Err(E::InvalidTable));
            let fields = (0..65)
                .map(|i| Field::new(format!("C{i}"), DataType::Int64, true))
                .collect::<Vec<_>>();
            let arrays = (0..65)
                .map(|_| Arc::new(Int64Array::from(Vec::<i64>::new())) as arrow_array::ArrayRef)
                .collect();
            let batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).unwrap();
            assert!(table_snapshot(&stream(&batch)).is_ok());
        }

        /// Canonical temporal fields exercise the largest supported scalar IPC schema.
        #[test]
        fn wide_temporal_ipc_admits_1024_and_refuses_1025_and_oversized_metadata() {
            use yamaa_core::{
                table::{Column, TableSchema},
                value::ColumnType,
            };
            for width in [1_024, 1_025] {
                let logical = TableSchema::new(
                    (0..width)
                        .map(|i| Column {
                            name: format!("T{i:04}"),
                            kind: ColumnType::DateTime,
                        })
                        .collect(),
                )
                .unwrap();
                let physical = yamaa_adapters::arrow_table::physical_schema(&logical);
                let arrays = physical
                    .fields()
                    .iter()
                    .map(|field| arrow_array::new_empty_array(field.data_type()))
                    .collect();
                let batch = RecordBatch::try_new(physical, arrays).unwrap();
                let result = table_round_trip(&stream(&batch));
                if width == 1_024 {
                    assert_eq!(read(result.unwrap())[0].num_columns(), 1_024);
                } else {
                    assert_eq!(
                        result,
                        Err(E::Limit {
                            resource: "columns",
                            limit: 1_024,
                            required: Some(1_025)
                        })
                    );
                }
            }
            // A truncated declared metadata message is malformed, independently of its size.
            let oversized = (1_048_576_u32 + 8).to_le_bytes();
            assert_eq!(table_round_trip(&oversized), Err(E::InvalidStream));
        }

        /// Build adversarial metadata independently of Arrow's validating record writer.
        fn declared_stream(
            schema: &Schema,
            rows: i64,
            nodes: &[arrow_ipc::FieldNode],
            buffers: &[arrow_ipc::Buffer],
            body: &[u8],
            declared_body: i64,
            compressed: bool,
        ) -> Vec<u8> {
            let mut result = vec![];
            let mut writer = StreamWriter::try_new(&mut result, schema).unwrap();
            writer.finish().unwrap();
            result.truncate(result.len() - 8);
            let mut builder = flatbuffers::FlatBufferBuilder::new();
            let nodes = builder.create_vector(nodes);
            let buffers = builder.create_vector(buffers);
            let compression = compressed
                .then(|| arrow_ipc::BodyCompression::create(&mut builder, &Default::default()));
            let batch = arrow_ipc::RecordBatch::create(
                &mut builder,
                &arrow_ipc::RecordBatchArgs {
                    length: rows,
                    nodes: Some(nodes),
                    buffers: Some(buffers),
                    compression,
                    ..Default::default()
                },
            );
            let message = arrow_ipc::Message::create(
                &mut builder,
                &arrow_ipc::MessageArgs {
                    version: arrow_ipc::MetadataVersion::V5,
                    header_type: arrow_ipc::MessageHeader::RecordBatch,
                    header: Some(batch.as_union_value()),
                    bodyLength: declared_body,
                    ..Default::default()
                },
            );
            builder.finish(message, None);
            let metadata = builder.finished_data();
            let padded = metadata.len().div_ceil(8) * 8;
            result.extend_from_slice(&u32::MAX.to_le_bytes());
            result.extend_from_slice(&(padded as u32).to_le_bytes());
            result.extend_from_slice(metadata);
            result.resize(result.len() + padded - metadata.len(), 0);
            result.extend_from_slice(body);
            result.extend_from_slice(&[255, 255, 255, 255, 0, 0, 0, 0]);
            result
        }

        /// Untrusted declarations cannot allocate oversized bodies or bypass buffer bounds.
        #[test]
        fn adversarial_declared_lengths_nodes_buffers_and_compression() {
            use arrow_ipc::{Buffer as B, FieldNode as N};
            let schema = Schema::new(vec![Field::new("I", DataType::Int64, true)]);
            let valid_nodes = [N::new(1, 0)];
            let valid_buffers = [B::new(0, 0), B::new(0, 8)];
            for body in [-1, i64::MAX, 1024 * 1024 * 1024] {
                assert_eq!(
                    table_snapshot(&declared_stream(
                        &schema,
                        1,
                        &valid_nodes,
                        &valid_buffers,
                        &[],
                        body,
                        false
                    )),
                    Err(E::InvalidStream)
                );
            }
            for nodes in [
                [N::new(-1, 0)],
                [N::new(1, -1)],
                [N::new(1, 2)],
                [N::new(2, 0)],
            ] {
                assert_eq!(
                    table_snapshot(&declared_stream(
                        &schema,
                        1,
                        &nodes,
                        &valid_buffers,
                        &[0; 8],
                        8,
                        false
                    )),
                    Err(E::InvalidStream)
                );
            }
            for buffers in [
                [B::new(0, 0), B::new(-1, 8)],
                [B::new(0, 0), B::new(0, -1)],
                [B::new(0, 0), B::new(1, 8)],
                [B::new(0, 0), B::new(i64::MAX, i64::MAX)],
            ] {
                assert_eq!(
                    table_snapshot(&declared_stream(
                        &schema,
                        1,
                        &valid_nodes,
                        &buffers,
                        &[0; 8],
                        8,
                        false
                    )),
                    Err(E::InvalidStream)
                );
            }
            assert_eq!(
                table_snapshot(&declared_stream(
                    &schema,
                    1,
                    &valid_nodes,
                    &valid_buffers,
                    &[0; 8],
                    8,
                    true
                )),
                Err(E::InvalidStream)
            );
            // Correct declarations with too-short value storage are rejected by Arrow validation.
            assert_eq!(
                table_snapshot(&declared_stream(
                    &schema,
                    1,
                    &valid_nodes,
                    &[B::new(0, 0), B::new(0, 0)],
                    &[],
                    0,
                    false
                )),
                Err(E::InvalidStream)
            );
        }

        /// Aliased buffers and JSON escaping retain all logical text without copy quotas.
        #[test]
        fn aliased_text_and_json_expansion_remain_complete() {
            use arrow_ipc::{Buffer as B, FieldNode as N};
            let schema = Schema::new(
                (0..64)
                    .map(|i| Field::new(format!("S{i}"), DataType::Utf8, true))
                    .collect::<Vec<_>>(),
            );
            let length = 150_000_i32;
            let mut body = vec![];
            body.extend_from_slice(&0_i32.to_le_bytes());
            body.extend_from_slice(&length.to_le_bytes());
            body.resize(8 + length as usize, b'x');
            let nodes = vec![N::new(1, 0); 64];
            let buffers = (0..64)
                .flat_map(|_| [B::new(0, 0), B::new(0, 8), B::new(8, i64::from(length))])
                .collect::<Vec<_>>();
            let input = declared_stream(
                &schema,
                1,
                &nodes,
                &buffers,
                &body,
                body.len() as i64,
                false,
            );
            let output = table_round_trip(&input).unwrap();
            let table = yamaa_adapters::ipc_table::decode_snapshot(&output).unwrap();
            use yamaa_core::table::{TableAccess, ValueRef};
            assert_eq!(table.row_count(), 1);
            for column in 0..64 {
                assert_eq!(
                    table.cell(0, column).unwrap(),
                    ValueRef::Str(&"x".repeat(150_000))
                );
            }
            let schema = Arc::new(Schema::new(vec![Field::new("S", DataType::Utf8, true)]));
            let text = "\0".repeat(3 * 1024 * 1024);
            let batch = RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec![text]))])
                .unwrap();
            let input = stream(&batch);
            let snapshot = table_snapshot(&input).unwrap();
            assert!(snapshot.len() > 16 * 1024 * 1024);
            let snapshot: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
            assert_eq!(snapshot["rows"][0][0]["str"], "\0".repeat(3 * 1024 * 1024));
            assert!(table_round_trip(&input).is_ok());
        }

        /// Distinct chunks retain every aliased field without a shared copy allowance.
        #[test]
        fn aliased_text_remains_complete_across_batches() {
            use arrow_ipc::{Buffer as B, FieldNode as N};
            use yamaa_adapters::ipc_table::snapshot_batches;

            let schema = Schema::new(vec![
                Field::new("A", DataType::Utf8, true),
                Field::new("B", DataType::Utf8, true),
            ]);
            let length = 4096_i32;
            let mut body = vec![];
            body.extend_from_slice(&0_i32.to_le_bytes());
            body.extend_from_slice(&length.to_le_bytes());
            body.resize(8 + length as usize, b'x');
            let input = declared_stream(
                &schema,
                1,
                &[N::new(1, 0); 2],
                &[
                    B::new(0, 0),
                    B::new(0, 8),
                    B::new(8, i64::from(length)),
                    B::new(0, 0),
                    B::new(0, 8),
                    B::new(8, i64::from(length)),
                ],
                &body,
                body.len() as i64,
                false,
            );
            let record_start = 8 + u32::from_le_bytes(input[4..8].try_into().unwrap()) as usize;
            let mut two_batches = input[..input.len() - 8].to_vec();
            two_batches.extend_from_slice(&input[record_start..]);
            let mut batches = snapshot_batches(&two_batches).unwrap();
            for _ in 0..2 {
                let table = batches.next().unwrap().unwrap();
                assert_eq!(table.num_rows(), 1);
                for column in 0..2 {
                    let values = table
                        .column(column)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap();
                    assert_eq!(values.value(0), "x".repeat(4096));
                }
            }
            assert!(batches.next().is_none());
        }

        /// Validate UTF-8 and reject unsupported dictionaries, nesting and IPC versions.
        #[test]
        fn closed_stream_forms_and_utf8_are_enforced() {
            use arrow_ipc::{Buffer as B, FieldNode as N};
            let schema = Schema::new(vec![Field::new("S", DataType::Utf8, true)]);
            let mut body = vec![];
            body.extend_from_slice(&0_i32.to_le_bytes());
            body.extend_from_slice(&1_i32.to_le_bytes());
            body.push(255);
            assert_eq!(
                table_snapshot(&declared_stream(
                    &schema,
                    1,
                    &[N::new(1, 0)],
                    &[B::new(0, 0), B::new(0, 8), B::new(8, 1)],
                    &body,
                    9,
                    false
                )),
                Err(E::InvalidStream)
            );
            let unsupported = [
                DataType::Dictionary(Box::new(DataType::Int8), Box::new(DataType::Utf8)),
                DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
            ];
            for kind in unsupported {
                let schema = Schema::new(vec![Field::new("A", kind, true)]);
                let mut bytes = vec![];
                let mut writer = StreamWriter::try_new(&mut bytes, &schema).unwrap();
                writer.finish().unwrap();
                assert_eq!(table_snapshot(&bytes), Err(E::InvalidTable));
            }
            let options = arrow_ipc::writer::IpcWriteOptions::try_new(
                8,
                false,
                arrow_ipc::MetadataVersion::V4,
            )
            .unwrap();
            let mut bytes = vec![];
            let mut writer =
                StreamWriter::try_new_with_options(&mut bytes, &schema, options).unwrap();
            writer.finish().unwrap();
            assert_eq!(table_snapshot(&bytes), Err(E::InvalidStream));
        }
    }
    mod parquet_artifact {
        use parquet::{
            basic::{Compression, LogicalType, Repetition, TimeUnit, Type},
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
            let expected = [
                (Type::BYTE_ARRAY, Some(LogicalType::String)),
                (Type::INT64, None),
                (Type::DOUBLE, None),
                (Type::INT32, Some(LogicalType::Date)),
                (
                    Type::INT64,
                    Some(LogicalType::timestamp(false, TimeUnit::MICROS)),
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
            assert!(rows[1].is_null(0).unwrap());
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
            assert!(rows[2].is_null(3).unwrap() && rows[2].is_null(4).unwrap());
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
            use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
            // REQ-0570: precision is internal provenance. Even imputed values carry
            // complete calendar fields; artifacts deliberately retain only those fields.
            let table = table(
                &[("D", ColumnType::Date), ("T", ColumnType::DateTime)],
                [
                    (DatePrecision::Year, DateTimePrecision::Day),
                    (DatePrecision::Month, DateTimePrecision::Day),
                    (DatePrecision::Day, DateTimePrecision::Second),
                ]
                .into_iter()
                .map(|(date_precision, time_precision)| {
                    vec![
                        Value::Date(Date::new(1970, 2, 28, date_precision).unwrap()),
                        Value::DateTime(
                            DateTime::new(
                                Date::new(1970, 1, 1, DatePrecision::Day).unwrap(),
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
    mod parquet_source {
        //! Independently authored PyArrow containers exercise the held-byte input codec.
        use std::path::Path;
        use yamaa_adapters::parquet_source::{parse, Error};
        use yamaa_core::{
            table::{TableAccess, ValueRef},
            temporal::{DatePrecision, DateTimePrecision},
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
                    assert_eq!(value.collected_precision(), DatePrecision::Day);
                }
                for (row, fields) in [
                    (0, (1969, 12, 31, 23, 59, 59)),
                    (2, (9999, 12, 31, 23, 59, 59)),
                ] {
                    let ValueRef::DateTime(value) = table.cell(row, 4).unwrap() else {
                        panic!("{name}")
                    };
                    assert_eq!(value.fields(), fields, "{name}");
                    assert_eq!(value.collected_precision(), DateTimePrecision::Second);
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
            assert_eq!(value.collected_precision(), DatePrecision::Day);
        }

        #[test]
        fn unsupported_stored_types_retain_independent_arrow_type_spelling() {
            for (name, expected) in [
                ("int32", "int32"),
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
    }
    mod predicate_diagnostics {
        //! Complete original-document outcomes captured independently from Python.
        use std::sync::Arc;
        use yamaa_adapters::{
            specification_report::{self, ArtifactPort, Identity},
            specification_run::{PreparedRun, SourcePort},
            specification_source::Source,
        };
        use yamaa_core::specification::SourceDeclaration;

        struct Port {
            captures: usize,
        }
        impl SourcePort for Port {
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
                let attempt = run.execute_with_port(&mut port);
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
                let expected: serde_json::Value = serde_json::from_str(fields[2]).unwrap();
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
        use arrow_ipc::{reader::StreamReader, writer::StreamWriter};
        use std::{io::Cursor, sync::Arc};
        use yamaa_adapters::{
            arrow_table::{date_array, datetime_array, physical_schema},
            public_table::{PublicTable, Values},
        };
        use yamaa_core::{
            table::{Column, TableSchema},
            temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
            value::ColumnType,
        };

        fn source(rows: usize) -> Vec<u8> {
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
            let date = Date::new(1970, 1, 1, DatePrecision::Year).unwrap();
            let time = DateTime::new(date, 0, 0, 0, DateTimePrecision::Day).unwrap();
            let strings = [Some("nul\0text"), None, Some("")];
            let integers = [Some(i64::MIN), None, Some(i64::MAX)];
            let floats = [Some(-0.0), None, Some(1.25)];
            let dates = [
                Some(date),
                None,
                Some(Date::new(1969, 12, 31, DatePrecision::Day).unwrap()),
            ];
            let times = [
                Some(time),
                None,
                Some(DateTime::new(date, 0, 0, 1, DateTimePrecision::Second).unwrap()),
            ];
            let arrays: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(strings[..rows].to_vec())),
                Arc::new(Int64Array::from(integers[..rows].to_vec())),
                Arc::new(Float64Array::from(floats[..rows].to_vec())),
                Arc::new(date_array(&dates[..rows]).unwrap()),
                Arc::new(datetime_array(&times[..rows]).unwrap()),
            ];
            let batch = RecordBatch::try_new(physical_schema(&schema), arrays).unwrap();
            let mut bytes = Vec::new();
            let mut writer = StreamWriter::try_new(&mut bytes, &batch.schema()).unwrap();
            writer.write(&batch).unwrap();
            writer.finish().unwrap();
            drop(writer);
            bytes
        }
        #[test]
        fn public_columns_preserve_exact_values_missing_and_temporal_null_masks() {
            let table = PublicTable::decode(&source(3)).unwrap();
            assert_eq!(table.rows(), 3);
            assert!(
                matches!(&table.columns()[1].1,Values::Int(v) if v==&vec![Some(i64::MIN),None,Some(i64::MAX)])
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
            let table = PublicTable::decode(&source(0)).unwrap();
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
