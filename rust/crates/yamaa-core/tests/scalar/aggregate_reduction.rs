use yamaa_core::{
    aggregate_parser::Reducer,
    aggregate_reduction::{reduce_values, AggregateReductionError},
    table::ValueRef,
    value::{Value, ValueType},
};

#[test]
fn extrema_keep_exact_values_types_missingness_and_text_order() {
    let values = [
        Value::Missing,
        Value::Int(9_007_199_254_740_993),
        Value::float(9_007_199_254_740_992.0),
    ];
    let borrowed = values.iter().map(ValueRef::from).collect::<Vec<_>>();
    assert_eq!(
        reduce_values(&borrowed, Reducer::Max, "MAX(D.V)"),
        Ok(ValueRef::Int(9_007_199_254_740_993))
    );
    assert_eq!(
        reduce_values(&borrowed, Reducer::Min, "MIN(D.V)"),
        Ok(ValueRef::from(&values[2]))
    );
    let text = [
        ValueRef::Str("é"),
        ValueRef::Str("😀"),
        ValueRef::Missing,
        ValueRef::Str(""),
    ];
    assert_eq!(
        reduce_values(&text, Reducer::Min, "MIN(D.V)"),
        Ok(ValueRef::Str(""))
    );
    assert_eq!(
        reduce_values(&text, Reducer::Max, "MAX(D.V)"),
        Ok(ValueRef::Str("😀"))
    );
    let equal = [Value::Int(2), Value::float(2.0)];
    for reducer in [Reducer::Min, Reducer::Max] {
        assert_eq!(
            reduce_values(
                &equal.iter().map(ValueRef::from).collect::<Vec<_>>(),
                reducer,
                "D.V"
            ),
            Ok(ValueRef::Int(2))
        );
        assert_eq!(reduce_values(&[], reducer, "D.V"), Ok(ValueRef::Missing));
        assert_eq!(
            reduce_values(&[ValueRef::Missing; 2], reducer, "D.V"),
            Ok(ValueRef::Missing)
        );
    }
    assert_eq!(
        reduce_values(
            &[ValueRef::Int(1), ValueRef::Str("1")],
            Reducer::Min,
            "MIN(D.V)"
        ),
        Err(AggregateReductionError::IncompatibleInputs {
            argument: 1,
            left: ValueType::Int,
            right: ValueType::Str
        })
    );
}

#[test]
fn only_counts_records_before_missing_handling_and_count_retains_empty_distinction() {
    assert_eq!(
        reduce_values(&[], Reducer::Only, "ONLY(D.V)"),
        Ok(ValueRef::Missing)
    );
    assert_eq!(
        reduce_values(&[ValueRef::Missing], Reducer::Only, "ONLY(D.V)"),
        Ok(ValueRef::Missing)
    );
    assert_eq!(
        reduce_values(&[ValueRef::Bool(false)], Reducer::Only, "ONLY(D.V)"),
        Ok(ValueRef::Bool(false))
    );
    assert_eq!(
        reduce_values(&[ValueRef::Missing; 2], Reducer::Only, "ONLY(D.V)"),
        Err(AggregateReductionError::MultipleRecords { count: 2 })
    );
    assert_eq!(
        reduce_values(&[], Reducer::Count, "COUNT(D.V)"),
        Ok(ValueRef::Missing)
    );
    assert_eq!(
        reduce_values(&[ValueRef::Missing; 2], Reducer::Count, "COUNT(D.V)"),
        Ok(ValueRef::Int(0))
    );
}
