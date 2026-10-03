use yamaa_core::conversion::{convert, float_text, ConversionValue};
use yamaa_core::temporal::{Date, DatePrecision, DateTime, DateTimePrecision};
use yamaa_core::value::{ColumnType, FiniteFloat, Value, ValueType};

/// Apply the numeric grammar without inheriting Python's configurable digit cap.
#[test]
fn long_integer_text_preserves_value_or_exact_range_diagnostic() {
    let leading_zeros = Value::Str("0".repeat(5000) + "1");
    assert_eq!(convert(&leading_zeros, ColumnType::Int), Ok(Value::Int(1)));
    let integer = "9".repeat(5000);
    let error = convert(&Value::Str(integer.clone()), ColumnType::Int).unwrap_err();
    assert_eq!(error.requirement(), "REQ-0021");
    assert_eq!(error.value, ConversionValue::Integer(integer));
}

/// Decode shared typed inputs, allowing explicit bits for decimal halfway cases.
fn decode(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, text) = token.split_once(':').unwrap();
    match kind {
        "str" => Value::Str(text.into()),
        "int" => Value::Int(text.parse().unwrap()),
        "float" => Value::float(text.parse().unwrap()),
        "bits" => Value::float(f64::from_bits(u64::from_str_radix(text, 16).unwrap())),
        "bool" => Value::Bool(text.parse().unwrap()),
        "date" => Value::Date(text.parse().unwrap()),
        "datetime" => Value::DateTime(text.parse().unwrap()),
        _ => panic!("unknown fixture type {kind}"),
    }
}

/// Encode observed values without rounding floats or discarding runtime types.
fn encode(value: &Value) -> String {
    match value {
        Value::Missing => "missing".into(),
        Value::Str(text) => format!("str:{text}"),
        Value::Int(number) => format!("int:{number}"),
        Value::Float(number) => format!("float:{:016x}", number.get().to_bits()),
        Value::Bool(boolean) => format!("bool:{boolean}"),
        Value::Date(date) => format!("date:{date}"),
        Value::DateTime(date) => format!("datetime:{date}"),
    }
}

/// Compare all successful matrix cells and portable failure data to hand-written truth.
#[test]
fn shared_conversion_vectors() {
    for row in include_str!("fixtures/conversion.tsv").lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 4, "{row}");
        let target = match fields[2] {
            "str" => ColumnType::Str,
            "int" => ColumnType::Int,
            "float" => ColumnType::Float,
            "date" => ColumnType::Date,
            "datetime" => ColumnType::DateTime,
            _ => panic!("bad target"),
        };
        let result = convert(&decode(fields[1]), target);
        let actual = match result {
            Ok(value) => encode(&value),
            Err(error) => {
                assert_eq!(error.target, target);
                assert_eq!(error.phase(), "convert");
                assert_eq!(error.condition(), "conversion_failed");
                assert_eq!(error.applicable_handler(), "unconvertible");
                let context = match &error.value {
                    ConversionValue::Runtime(value) => {
                        assert_eq!(error.source_type(), value.value_type());
                        encode(value)
                    }
                    ConversionValue::Integer(text) => {
                        assert_eq!(error.source_type(), Some(ValueType::Int));
                        format!("int:{text}")
                    }
                };
                format!("error:{}:{context}", error.requirement())
            }
        };
        assert_eq!(actual, fields[3], "{}", fields[0]);
    }
}

/// Every scalar type/destination pairing has a declared result or a typed failure.
#[test]
fn complete_conversion_matrix() {
    let sources = [
        decode("str:42"),
        decode("int:42"),
        decode("float:42.0"),
        decode("date:2025-01-02"),
        decode("datetime:2025-01-02T00:00"),
        decode("bool:true"),
        Value::Missing,
    ];
    let targets = [
        ColumnType::Str,
        ColumnType::Int,
        ColumnType::Float,
        ColumnType::Date,
        ColumnType::DateTime,
    ];
    let allowed = [
        [true, true, true, false, false],
        [true, true, true, false, false],
        [true, true, true, false, false],
        [true, false, false, true, false],
        [true, false, false, false, true],
        [false, false, false, false, false],
        [true, true, true, true, true],
    ];
    for (source, outcomes) in sources.iter().zip(allowed) {
        for (target, succeeds) in targets.into_iter().zip(outcomes) {
            assert_eq!(
                convert(source, target).is_ok(),
                succeeds,
                "{source:?} -> {target:?}"
            );
        }
    }
}

/// Identity retains collected precision while text round trips lose that provenance.
#[test]
fn temporal_precision_at_conversion_boundaries() {
    let date = Date::new(2025, 1, 2, DatePrecision::Year).unwrap();
    let Value::Date(identity) = convert(&Value::Date(date), ColumnType::Date).unwrap() else {
        panic!("date");
    };
    assert_eq!(identity.collected_precision(), DatePrecision::Year);
    let text = convert(&Value::Date(date), ColumnType::Str).unwrap();
    let Value::Date(parsed) = convert(&text, ColumnType::Date).unwrap() else {
        panic!("date");
    };
    assert_eq!(parsed.collected_precision(), DatePrecision::Day);
    let datetime = DateTime::new(date, 0, 0, 0, DateTimePrecision::Day).unwrap();
    let Value::DateTime(identity) =
        convert(&Value::DateTime(datetime), ColumnType::DateTime).unwrap()
    else {
        panic!("datetime");
    };
    assert_eq!(identity.collected_precision(), DateTimePrecision::Day);
    let text = convert(&Value::DateTime(datetime), ColumnType::Str).unwrap();
    let Value::DateTime(parsed) = convert(&text, ColumnType::DateTime).unwrap() else {
        panic!("datetime");
    };
    assert_eq!(parsed.collected_precision(), DateTimePrecision::Second);
}

/// Probe the extreme binary64 exponent range without using either engine as truth.
#[test]
fn extreme_float_text_is_positional_and_shortest() {
    assert_eq!(
        float_text(FiniteFloat::new(f64::MAX).unwrap()),
        "17976931348623157".to_owned() + &"0".repeat(292)
    );
    assert_eq!(
        float_text(FiniteFloat::new(f64::from_bits(1)).unwrap()),
        "0.".to_owned() + &"0".repeat(323) + "5"
    );
    assert_eq!(float_text(FiniteFloat::new(-0.0).unwrap()), "-0");
}

/// Check bit-preserving text round trips across a deterministic exponent/sign sample.
#[test]
fn sampled_floats_round_trip_without_exponent_notation() {
    let mut bits = 0x123456789abcdef0_u64;
    for _ in 0..20000 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        if let Some(value) = FiniteFloat::new(f64::from_bits(bits)) {
            let text = float_text(value);
            assert!(!text.contains(['e', 'E']));
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), bits, "{text}");
        }
    }
}

/// Reject host-friendly numeric spellings that are outside the specification grammar.
#[test]
fn numeric_text_rejects_whitespace_unicode_and_incomplete_forms() {
    for text in [
        "1 ", "\t1", "1\n", "1\r", "１", "١", "+", "-", "1e", "1.e2", "1e.2", "+-1", ".Nan",
        "-.nan",
    ] {
        for target in [ColumnType::Int, ColumnType::Float] {
            let error = convert(&Value::Str(text.into()), target).unwrap_err();
            assert_eq!(error.requirement(), "REQ-0013", "{text}");
        }
    }
}
