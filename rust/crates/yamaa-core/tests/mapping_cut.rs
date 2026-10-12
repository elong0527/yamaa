use yamaa_core::{
    mapping_cut::{AdmissionError, Cut, Mapping, SelectedValue},
    table::ValueRef,
    value::Value,
};
#[path = "diagnostic/mapping_cut.rs"]
mod diagnostic;

#[test]
fn dictionary_results_retain_literal_types_and_missing_is_not_unmapped() {
    let mapping = Mapping::new(
        "SRC.V".into(),
        "columns.V.derivation.mapping".into(),
        vec![
            ("N".into(), Value::Missing),
            ("i".into(), Value::Int(7)),
            ("f".into(), Value::float(2.5)),
            ("b".into(), Value::Bool(false)),
            ("s".into(), Value::Str("".into())),
        ],
        true,
        Some(Value::Int(3)),
        Some(Value::Bool(true)),
    )
    .unwrap();
    for (input, expected) in [
        ("N", Value::Missing),
        ("i", Value::Int(7)),
        ("f", Value::float(2.5)),
        ("b", Value::Bool(false)),
        ("s", Value::Str("".into())),
    ] {
        let result = mapping.select(ValueRef::Str(input)).unwrap();
        assert_eq!(result.value, SelectedValue::Literal(&expected));
        assert_eq!(result.handler, None);
    }
    let missing = mapping.select(ValueRef::Missing).unwrap();
    assert_eq!(missing.value, SelectedValue::Literal(&Value::Int(3)));
    assert_eq!(missing.handler, Some("missing"));
    let unmapped = mapping.select(ValueRef::Str("missing")).unwrap();
    assert_eq!(unmapped.value, SelectedValue::Literal(&Value::Bool(true)));
    assert_eq!(unmapped.handler, Some("unmapped"));
}
#[test]
fn folding_is_ascii_only_and_includes_empty_and_nul_keys() {
    let map = Mapping::new(
        "V".into(),
        "mapping".into(),
        vec![
            ("\u{00e9}a\0Z".into(), Value::Int(1)),
            ("\u{00c9}a\0Z".into(), Value::Int(2)),
            ("".into(), Value::Int(3)),
            ("ss".into(), Value::Int(4)),
            ("\u{00df}".into(), Value::Int(5)),
        ],
        false,
        None,
        None,
    )
    .unwrap();
    for (input, expected) in [
        ("\u{00e9}A\0z", 1),
        ("\u{00c9}A\0z", 2),
        ("", 3),
        ("SS", 4),
        ("\u{00df}", 5),
    ] {
        assert_eq!(
            map.select(ValueRef::Str(input)).unwrap().value,
            SelectedValue::Literal(&Value::Int(expected))
        );
    }
    assert!(map.select(ValueRef::Str("\u{00e9}aZ")).is_err());
    let exact = Mapping::new(
        "V".into(),
        "mapping".into(),
        vec![("Y".into(), Value::Int(1)), ("y".into(), Value::Int(2))],
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        exact.select(ValueRef::Str("y")).unwrap().value,
        SelectedValue::Literal(&Value::Int(2))
    );
}
#[test]
fn interval_closure_and_unbounded_tails_follow_numeric_order() {
    for right in [false, true] {
        let cut = Cut::new(
            "AGE".into(),
            "cut".into(),
            vec![18.0, 65.0],
            vec!["child".into(), "adult".into(), "older".into()],
            right,
            None,
        )
        .unwrap();
        for (input, expected) in [
            (17, "child"),
            (18, if right { "child" } else { "adult" }),
            (19, "adult"),
            (64, "adult"),
            (65, if right { "adult" } else { "older" }),
            (66, "older"),
        ] {
            assert_eq!(
                cut.select(ValueRef::Int(input)).unwrap().value,
                SelectedValue::Label(expected)
            );
        }
    }
    let empty = Cut::new(
        "AGE".into(),
        "cut".into(),
        vec![],
        vec!["all".into()],
        false,
        Some(Value::Missing),
    )
    .unwrap();
    assert_eq!(
        empty.select(ValueRef::Int(i64::MIN)).unwrap().value,
        SelectedValue::Label("all")
    );
    assert_eq!(
        empty.select(ValueRef::Missing).unwrap().handler,
        Some("missing")
    );
    for boundary in [f64::NEG_INFINITY, f64::INFINITY] {
        assert!(matches!(
            Cut::new(
                "AGE".into(),
                "cut".into(),
                vec![boundary],
                vec!["low".into(), "high".into()],
                true,
                None
            ),
            Err(AdmissionError::Diagnostic(_))
        ));
    }
}
#[test]
fn interval_comparison_does_not_round_an_integer_into_a_binary64_boundary() {
    let cut = Cut::new(
        "N".into(),
        "cut".into(),
        vec![9_007_199_254_740_992.0],
        vec!["before".into(), "after".into()],
        true,
        None,
    )
    .unwrap();
    assert_eq!(
        cut.select(ValueRef::Int(9_007_199_254_740_992))
            .unwrap()
            .value,
        SelectedValue::Label("before")
    );
    assert_eq!(
        cut.select(ValueRef::Int(9_007_199_254_740_993))
            .unwrap()
            .value,
        SelectedValue::Label("after")
    );
}
#[test]
fn invalid_boundaries_and_label_counts_are_planning_findings() {
    for (breaks, labels) in [
        (vec![1.0, 1.0], vec!["a", "b", "c"]),
        (vec![2.0, 1.0], vec!["a", "b", "c"]),
        (vec![f64::NAN], vec!["a", "b"]),
        (vec![1.0], vec!["a"]),
    ] {
        assert!(matches!(
            Cut::new(
                "N".into(),
                "cut".into(),
                breaks,
                labels.into_iter().map(Into::into).collect(),
                false,
                None
            ),
            Err(AdmissionError::Diagnostic(_))
        ));
    }
}
