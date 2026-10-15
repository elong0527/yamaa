use yamaa_adapters::{
    arrow_table::TableLimits,
    csv_source,
    typed_csv::{self, Error},
};
use yamaa_core::{
    table::{TableAccess, ValueRef},
    typed_csv::Error as TypingError,
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
    assert!(matches!(error,Error::Typing(TypingError::UnknownField{field}) if field=="Z"));
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
        matches!(error,Error::Typing(TypingError::FieldParse{field,target:T::Int,value}) if field=="B"&&value=="bad")
    );
    let error = typed_csv::parse(
        b"A,B\nfirst,second",
        &[("B".into(), T::Int), ("A".into(), T::Int)],
        Default::default(),
        limits(),
    )
    .unwrap_err();
    assert!(
        matches!(error,Error::Typing(TypingError::FieldParse{field,value,..}) if field=="A"&&value=="first")
    );
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
            cause: csv_source::ProfileCause::QuoteUnterminated,
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
            matches!(typed_csv::parse(input.as_bytes(),&[("X".into(),kind)],Default::default(),limits()),Err(Error::Typing(TypingError::FieldParse {field,value,..})) if field=="X"&&value==text)
        );
    }
}

#[test]
fn retired_python_integer_ingestion_keeps_every_i64_through_long_zero_prefixes() {
    // Original test_source.py operands: CSV admission must not inherit a host
    // integer-string digit ceiling or round the values through binary64.
    let expected = [0_i64, 1, 9_007_199_254_740_993, i64::MAX, i64::MIN];
    let mut input = String::from("VALUE\n");
    for value in expected {
        input.push(if value < 0 { '-' } else { '+' });
        input.push_str(&"0".repeat(5000));
        input.push_str(&value.unsigned_abs().to_string());
        input.push('\n');
    }
    let table = typed_csv::parse(
        input.as_bytes(),
        &[("VALUE".into(), T::Int)],
        Default::default(),
        limits(),
    )
    .unwrap();
    assert_eq!(table.row_count(), expected.len());
    for (row, value) in expected.into_iter().enumerate() {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(value));
    }
}

#[test]
fn retired_python_typed_csv_keeps_original_field_order_missingness_and_civil_time() {
    let declared = [
        ("AGE".into(), T::Int),
        ("SCORE".into(), T::Float),
        ("DATE".into(), T::Date),
        ("MOMENT".into(), T::DateTime),
    ];
    let table = typed_csv::parse(
        b"ID,AGE,SCORE,DATE,MOMENT,EMPTY,TEXT\n\
007,42,1.5,2025-01-02,2025-01-02T03:04,,\"\"\n\
008,,.Inf,2025-02-03,2025-02-03T04:05:06,NA,unknown\n",
        &declared,
        Default::default(),
        limits(),
    )
    .unwrap();
    assert_eq!(
        table
            .schema()
            .columns()
            .iter()
            .map(|column| (column.name.as_str(), column.kind))
            .collect::<Vec<_>>(),
        [
            ("ID", T::Str),
            ("AGE", T::Int),
            ("SCORE", T::Float),
            ("DATE", T::Date),
            ("MOMENT", T::DateTime),
            ("EMPTY", T::Str),
            ("TEXT", T::Str),
        ]
    );
    let expected = [
        vec![
            ValueRef::Str("007"),
            ValueRef::Int(42),
            ValueRef::Float(yamaa_core::value::FiniteFloat::new(1.5).unwrap()),
            ValueRef::Date("2025-01-02".parse().unwrap()),
            ValueRef::DateTime("2025-01-02T03:04".parse().unwrap()),
            ValueRef::Missing,
            ValueRef::Missing,
        ],
        vec![
            ValueRef::Str("008"),
            ValueRef::Missing,
            ValueRef::Missing,
            ValueRef::Date("2025-02-03".parse().unwrap()),
            ValueRef::DateTime("2025-02-03T04:05:06".parse().unwrap()),
            ValueRef::Str("NA"),
            ValueRef::Str("unknown"),
        ],
    ];
    assert_eq!(table.row_count(), expected.len());
    for (row, values) in expected.into_iter().enumerate() {
        for (column, value) in values.into_iter().enumerate() {
            assert_eq!(table.cell(row, column).unwrap(), value, "{row}/{column}");
        }
    }

    let empty =
        typed_csv::parse(b"ID,AGE\n", &declared[..1], Default::default(), limits()).unwrap();
    assert_eq!(empty.row_count(), 0);
    assert_eq!(empty.schema().columns()[0].kind, T::Str);
    assert_eq!(empty.schema().columns()[1].kind, T::Int);
    let quoted =
        typed_csv::parse(b"AGE\n\"\"\n", &declared[..1], Default::default(), limits()).unwrap();
    assert_eq!(quoted.cell(0, 0).unwrap(), ValueRef::Missing);
}

#[test]
fn retired_python_missing_looking_text_is_never_inferred_as_missing() {
    let table = typed_csv::parse(
        b"VALUE\nNA\nNULL\n.\nunknown\n",
        &[],
        Default::default(),
        limits(),
    )
    .unwrap();
    for (row, expected) in ["NA", "NULL", ".", "unknown"].into_iter().enumerate() {
        assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Str(expected));
    }
}
