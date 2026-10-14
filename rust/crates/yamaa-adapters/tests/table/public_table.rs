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
    let mut stream = StreamReader::try_new(Cursor::new(table.ipc().unwrap()), None).unwrap();
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
    let mut stream = StreamReader::try_new(Cursor::new(table.ipc().unwrap()), None).unwrap();
    let batch = stream.next().unwrap().unwrap();
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.num_columns(), 5);
    assert!(batch.column(0).as_any().is::<StringArray>());
    assert!(batch.column(1).as_any().is::<Int64Array>());
    assert!(batch.column(2).as_any().is::<Float64Array>());
    assert!(batch.column(3).as_any().is::<Date32Array>());
    assert!(batch.column(4).as_any().is::<TimestampSecondArray>());
}
