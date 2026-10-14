use arrow_array::{
    Array, ArrayRef, Date32Array, Float64Array, Int64Array, RecordBatch, RecordBatchOptions,
    StringArray, StructArray, TimestampSecondArray, UInt8Array,
};
use arrow_buffer::NullBuffer;
use arrow_schema::{DataType, Field, Schema};
use std::sync::Arc;
use yamaa_adapters::arrow_table::{
    date_array, datetime_array, physical_schema, ArrowTable, TableError, TableLimits,
    TemporalCellError,
};
use yamaa_core::{
    numeric::{ArithmeticErrorKind, Number},
    reduction::{NumericReducer, ReductionError},
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    temporal::{Date, DatePrecision as DP, DateTime, DateTimePrecision as TP},
    value::ColumnType as CT,
};
use yamaa_engine::table_reduction::{reduce_column, TableReductionError};

/// Build a closed schema without changing supplied declaration order.
fn logical_schema(columns: &[(&str, CT)]) -> TableSchema {
    TableSchema::new(
        columns
            .iter()
            .map(|&(name, kind)| Column {
                name: name.into(),
                kind,
            })
            .collect(),
    )
    .unwrap()
}

/// Tests use an explicit small policy; production callers must choose their own.
fn limits() -> TableLimits {
    TableLimits {
        max_rows: 200_000,
        max_columns: 10,
        max_batches: 20,
        max_cells: 1_000_000,
    }
}

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
    let table = ArrowTable::try_new(schema, vec![original.clone()], limits()).unwrap();
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
        limits(),
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
    let dates = Arc::new(date_array(&[None, Some(year), Some(month), Some(day), None]).unwrap());
    let times = Arc::new(datetime_array(&[None, Some(imputed), Some(time), None, None]).unwrap());
    let original = batch(&schema, vec![dates, times]);
    let table = ArrowTable::try_new(schema, vec![original.slice(1, 4)], limits()).unwrap();
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
    let no_batches = ArrowTable::try_new(schema.clone(), vec![], limits()).unwrap();
    assert_eq!(no_batches.schema(), &schema);
    assert_eq!(no_batches.row_count(), 0);
    let empty = batch(
        &schema,
        vec![
            Arc::new(date_array(&[]).unwrap()),
            Arc::new(StringArray::from(Vec::<Option<&str>>::new())),
        ],
    );
    let table = ArrowTable::try_new(schema, vec![empty], limits()).unwrap();
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
    let table = ArrowTable::try_new(empty_schema, vec![rows], limits()).unwrap();
    assert_eq!(table.row_count(), 3);
    assert_eq!(
        table.cell(0, 0),
        Err(CellError::OutOfBounds { row: 0, column: 0 })
    );
}

/// Construct canonical date storage directly to test independent bad payloads.
fn raw_dates(values: Vec<i32>, precision: Vec<u8>, valid: Option<Vec<bool>>) -> StructArray {
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
            matches!(ArrowTable::try_new(schema,vec![input],limits()),Err(TableError::Temporal { batch:0,column:0,row:1,reason: actual }) if actual == reason)
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
    let table = ArrowTable::try_new(schema, vec![input], limits()).unwrap();
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
    let table = ArrowTable::try_new(
        schema.clone(),
        vec![batch(&schema, vec![Arc::new(masked)])],
        limits(),
    )
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
            ArrowTable::try_new(schema.clone(), vec![input], limits()),
            Err(TableError::SchemaMismatch { batch: 0 })
        ));
    }
}

#[test]
/// Reject excess work before inspecting malformed temporal payloads.
fn schema_mismatches_and_shape_limits_precede_payload_scans() {
    let schema = logical_schema(&[("D", CT::Date)]);
    let invalid = batch(&schema, vec![Arc::new(raw_dates(vec![0], vec![255], None))]);
    for (policy, resource) in [
        (
            TableLimits {
                max_rows: 0,
                ..limits()
            },
            "rows",
        ),
        (
            TableLimits {
                max_columns: 0,
                ..limits()
            },
            "columns",
        ),
        (
            TableLimits {
                max_batches: 0,
                ..limits()
            },
            "batches",
        ),
        (
            TableLimits {
                max_cells: 0,
                ..limits()
            },
            "cells",
        ),
    ] {
        assert!(
            matches!(ArrowTable::try_new(schema.clone(),vec![invalid.clone()],policy),Err(TableError::Limit { resource:actual,.. }) if actual == resource)
        );
    }
    for foreign in [
        Arc::new(Schema::new(vec![Field::new(
            "wrong",
            DataType::Int64,
            true,
        )])),
        Arc::new(Schema::new(vec![Field::new("D", DataType::Int64, true)])),
    ] {
        let input =
            RecordBatch::try_new(foreign, vec![Arc::new(Int64Array::from(vec![1]))]).unwrap();
        assert!(matches!(
            ArrowTable::try_new(schema.clone(), vec![input], limits()),
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
    let policy = TableLimits {
        max_rows: usize::MAX,
        ..limits()
    };
    assert!(matches!(
        ArrowTable::try_new(no_columns, vec![huge.clone(), huge], policy),
        Err(TableError::Limit {
            resource: "rows",
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
    let table = ArrowTable::try_new(schema, chunks, limits()).unwrap();
    assert_eq!(
        reduce_column(
            &table,
            0,
            &[0, 1, 2, 3, 4],
            5,
            NumericReducer::Sum,
            "SUM(A)"
        ),
        Ok(Number::float(1.0))
    );
    assert_eq!(
        reduce_column(
            &table,
            0,
            &[0, 1, 2, 3, 4],
            5,
            NumericReducer::Mean,
            "MEAN(A)"
        ),
        Ok(Number::float(0.25))
    );
    let schema = logical_schema(&[("A", CT::Int)]);
    let chunks = [i64::MAX, 1, -1]
        .map(|value| batch(&schema, vec![Arc::new(Int64Array::from(vec![value]))]));
    let table = ArrowTable::try_new(schema, chunks.into(), limits()).unwrap();
    let result = reduce_column(&table, 0, &[0, 1, 2], 3, NumericReducer::Sum, "SUM(A)");
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
    let table = ArrowTable::try_new(
        schema.clone(),
        vec![batch(&schema, vec![Arc::new(raw)])],
        limits(),
    )
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
    let table = ArrowTable::try_new(
        schema.clone(),
        vec![batch(&schema, vec![Arc::new(array)])],
        limits(),
    )
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
            matches!(ArrowTable::try_new(schema.clone(),vec![input],limits()),Err(TableError::Temporal { reason:actual,.. }) if actual == reason)
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
    let table = ArrowTable::try_new(
        schema.clone(),
        vec![batch(&schema, vec![Arc::new(array)])],
        limits(),
    )
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
        ArrowTable::try_new(schema, vec![input], limits()),
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
    let table = ArrowTable::try_new(schema, vec![input], limits()).unwrap();
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
