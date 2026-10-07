use yamaa_adapters::{
    arrow_table::TableLimits,
    csv_source,
    typed_csv::{self, Error},
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    value::ColumnType as T,
};
fn limits() -> TableLimits {
    TableLimits {
        max_rows: 100,
        max_columns: 16,
        max_batches: 1,
        max_cells: 1600,
    }
}
#[test]
fn declared_types_keep_exact_i64_float_bits_temporal_precision_and_undeclared_text() {
    let data=b"I,F,D,T,RAW\n9007199254740993,-0.0,2024-03-02,2024-03-02T00:00:00,0012\n,1.5,2024-01-01,2024-03-02T01:02:03,NA\n";
    let table = typed_csv::parse(
        data,
        &[
            ("I".into(), T::Int),
            ("F".into(), T::Float),
            ("D".into(), T::Date),
            ("T".into(), T::DateTime),
        ],
        Default::default(),
        limits(),
    )
    .unwrap();
    assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(9007199254740993));
    let ValueRef::Float(value) = table.cell(0, 1).unwrap() else {
        panic!()
    };
    assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
    let ValueRef::Date(value) = table.cell(0, 2).unwrap() else {
        panic!()
    };
    assert_eq!(value.to_string(), "2024-03-02");
    assert_eq!(
        value.collected_precision(),
        yamaa_core::temporal::DatePrecision::Day
    );
    let ValueRef::DateTime(value) = table.cell(0, 3).unwrap() else {
        panic!()
    };
    assert_eq!(
        value.collected_precision(),
        yamaa_core::temporal::DateTimePrecision::Second
    );
    assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Str("0012"));
    assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
    assert_eq!(table.cell(1, 4).unwrap(), ValueRef::Str("NA"));
}
#[test]
fn unknown_fields_follow_written_declaration_order_before_conversion() {
    let error = typed_csv::parse(
        b"X\nnot-an-int",
        &[
            ("X".into(), T::Int),
            ("Z".into(), T::Float),
            ("A".into(), T::Int),
        ],
        Default::default(),
        limits(),
    )
    .unwrap_err();
    assert!(matches!(error,Error::UnknownField{field} if field=="Z"));
}
#[test]
fn conversion_failure_follows_rows_then_stored_fields_not_declaration_order() {
    let error = typed_csv::parse(
        b"A,B\n1,bad\nworse,2",
        &[("B".into(), T::Int), ("A".into(), T::Int)],
        Default::default(),
        limits(),
    )
    .unwrap_err();
    assert!(
        matches!(error,Error::FieldParse{field,target:T::Int,value} if field=="B"&&value=="bad")
    );
    let error = typed_csv::parse(
        b"A,B\nfirst,second",
        &[("B".into(), T::Int), ("A".into(), T::Int)],
        Default::default(),
        limits(),
    )
    .unwrap_err();
    assert!(matches!(error,Error::FieldParse{field,value,..} if field=="A"&&value=="first"));
}
#[test]
fn profile_failure_precedes_unknown_fields_and_nonfinite_spellings_normalize() {
    let error = typed_csv::parse(
        b"X\n\"bad",
        &[("Z".into(), T::Int)],
        Default::default(),
        limits(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        Error::Csv(csv_source::Error::Profile {
            condition: "source_quote_unterminated",
            ..
        })
    ));
    let table = typed_csv::parse(
        b"F\n.nan\n+.inf\n-.inf",
        &[("F".into(), T::Float)],
        Default::default(),
        limits(),
    )
    .unwrap();
    for row in 0..3 {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
    }
}

#[test]
fn incomplete_temporal_text_is_not_implicitly_imputed() {
    for (kind, text) in [(T::Date, "2024-03"), (T::DateTime, "2024-03-02")] {
        let input = format!("X\n{text}");
        assert!(
            matches!(typed_csv::parse(input.as_bytes(),&[("X".into(),kind)],Default::default(),limits()),Err(Error::FieldParse {field,value,..}) if field=="X"&&value==text)
        );
    }
}
