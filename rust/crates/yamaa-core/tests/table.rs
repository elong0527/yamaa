use yamaa_core::{
    table::{Column, SchemaError, TableSchema, ValueRef},
    temporal::{Date, DatePrecision, DateTime, DateTimePrecision},
    value::{ColumnType, Value},
};

#[test]
/// Preserve empty-table schema identity and reject ambiguous column names.
fn schema_retains_order_and_all_closed_column_types() {
    let kinds = [
        ColumnType::DateTime,
        ColumnType::Str,
        ColumnType::Int,
        ColumnType::Date,
        ColumnType::Float,
    ];
    let columns: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, &kind)| Column {
            name: format!("C{i}"),
            kind,
        })
        .collect();
    let schema = TableSchema::new(columns.clone()).unwrap();
    assert_eq!(schema.columns(), columns);
    assert!(TableSchema::new(vec![]).unwrap().columns().is_empty());
    assert_eq!(
        TableSchema::new(vec![Column {
            name: "".into(),
            kind: ColumnType::Int
        }]),
        Err(SchemaError::EmptyName { column: 0 })
    );
    assert_eq!(
        TableSchema::new(vec![columns[0].clone(), columns[0].clone()]),
        Err(SchemaError::DuplicateName { column: 1 })
    );
}

#[test]
/// Prove borrowed text ownership and exact scalar/temporal metadata.
fn borrowing_retains_text_backing_exact_values_and_temporal_precision() {
    let date = Date::new(2025, 1, 1, DatePrecision::Year).unwrap();
    let datetime = DateTime::new(date, 0, 0, 0, DateTimePrecision::Day).unwrap();
    let values = [
        Value::Missing,
        Value::Str("text\0\u{1f980}".into()),
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::float(-0.0),
        Value::Bool(true),
        Value::Date(date),
        Value::DateTime(datetime),
    ];
    let views: Vec<_> = values.iter().map(ValueRef::from).collect();
    for (owned, view) in values.iter().zip(&views) {
        assert_eq!(owned.value_type(), view.value_type());
    }
    let (Value::Str(owned), ValueRef::Str(view)) = (&values[1], views[1]) else {
        panic!("text required")
    };
    assert_eq!(owned.as_ptr(), view.as_ptr());
    assert_eq!(owned.len(), view.len());
    assert_eq!(views[2], ValueRef::Int(i64::MIN));
    assert_eq!(views[3], ValueRef::Int(i64::MAX));
    let ValueRef::Float(zero) = views[4] else {
        panic!("float required")
    };
    assert_eq!(zero.get().to_bits(), 1 << 63);
    let ValueRef::Date(date) = views[6] else {
        panic!("date required")
    };
    assert_eq!(date.collected_precision(), DatePrecision::Year);
    let ValueRef::DateTime(datetime) = views[7] else {
        panic!("datetime required")
    };
    assert_eq!(datetime.collected_precision(), DateTimePrecision::Day);
}
