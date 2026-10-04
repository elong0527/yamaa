use yamaa_core::{
    numeric::{ArithmeticErrorKind, Number},
    reduction::{reduce_numeric, NumericReducer, ReductionError},
    table::ValueRef,
    value::Value,
};

/// Decode hand-written fixture inputs without consulting either evaluator.
fn value(token: &str) -> Value {
    if token == "missing" {
        return Value::Missing;
    }
    let (kind, value) = token.split_once(':').unwrap();
    match kind {
        "int" => Value::Int(value.parse().unwrap()),
        "float" => Value::float(value.parse().unwrap()),
        "str" => Value::Str(value.into()),
        "bool" => Value::Bool(value.parse().unwrap()),
        _ => panic!("unknown value {token}"),
    }
}

#[test]
/// Pin all shared values, exact float bits and portable failure facts.
fn independent_ordered_reduction_truth_shared_with_python() {
    let mut count = 0;
    for row in include_str!("fixtures/reduction.tsv").lines().skip(1) {
        let columns: Vec<_> = row.split('\t').collect();
        assert_eq!(columns.len(), 4);
        let (id, name, tokens, expected) = (columns[0], columns[1], columns[2], columns[3]);
        let reducer = match name {
            "SUM" => NumericReducer::Sum,
            "MEAN" => NumericReducer::Mean,
            _ => panic!("bad reducer"),
        };
        assert_eq!(reducer.name(), name);
        let values: Vec<_> = if tokens == "-" {
            vec![]
        } else {
            tokens.split(',').map(value).collect()
        };
        let borrowed: Vec<_> = values.iter().map(ValueRef::from).collect();
        let expression = format!("{name}(A)");
        let actual = match reduce_numeric(&borrowed, reducer, &expression) {
            Ok(Number::Missing) => "missing".into(),
            Ok(Number::Int(value)) => format!("int:{value}"),
            Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
            Err(ReductionError::IncompatibleInputType { argument, actual }) => {
                assert_eq!(borrowed[argument].value_type(), Some(actual));
                format!(
                    "error:incompatible_input_type:REQ-0510:{}",
                    format!("{actual:?}").to_lowercase()
                )
            }
            Err(ReductionError::Arithmetic { argument, error }) => {
                assert_eq!(argument, 1, "{id}");
                assert_eq!(error.expression, expression);
                assert_eq!(error.phase(), "derivation");
                let ArithmeticErrorKind::IntegerOverflow { value } = error.kind else {
                    panic!("unexpected failure")
                };
                format!(
                    "error:{}:{}:{value}",
                    error.condition(),
                    error.requirement()
                )
            }
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(actual, expected, "{id}");
        count += 1;
    }
    assert_eq!(count, 32);
}

#[test]
/// Reject each nonnumeric type at its original position including missing slots.
fn nonnumeric_arguments_keep_original_index_and_type() {
    use yamaa_core::temporal::{Date, DateTime};
    let date: Date = "2025-01-01".parse().unwrap();
    let datetime: DateTime = "2025-01-01T00:00:00".parse().unwrap();
    for value in [
        ValueRef::Str(""),
        ValueRef::Bool(false),
        ValueRef::Date(date),
        ValueRef::DateTime(datetime),
    ] {
        for reducer in [NumericReducer::Sum, NumericReducer::Mean] {
            assert_eq!(
                reduce_numeric(
                    &[ValueRef::Missing, ValueRef::Int(1), value],
                    reducer,
                    "reduce(A)"
                ),
                Err(ReductionError::IncompatibleInputType {
                    argument: 2,
                    actual: value.value_type().unwrap()
                })
            );
        }
    }
}
