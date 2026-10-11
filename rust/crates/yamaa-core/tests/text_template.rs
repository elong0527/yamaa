use yamaa_core::{
    text_template::{
        AdmissionError, MaterializeError, Part, StringTemplate, MAX_DECLARATION_BYTES,
    },
    value::Value,
};
#[test]
fn literal_escapes_unique_bindings_and_ordered_occurrences_are_exact() {
    let op = StringTemplate::new(
        "{{{A}}}:{A}/{B}".into(),
        "columns.X.str_template".into(),
        None,
    )
    .unwrap();
    assert_eq!(op.identifiers(), ["A", "B"]);
    assert_eq!(
        op.parts()
            .iter()
            .filter_map(|p| if let Part::Placeholder(i) = p {
                Some(*i)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(
        op.materialize(
            &[
                Value::Str("first".into()),
                Value::Str("second".into()),
                Value::Str("third".into())
            ],
            100
        )
        .unwrap(),
        "{first}:second/third"
    );
    assert_eq!(
        StringTemplate::new("".into(), "path".into(), None)
            .unwrap()
            .materialize(&[], 0)
            .unwrap(),
        ""
    );
}
#[test]
fn unicode_scalars_empty_strings_nul_and_utf8_byte_limits_have_no_formatting() {
    let op = StringTemplate::new("\u{03a9}:{A}/{A}".into(), "path".into(), None).unwrap();
    let values = [
        Value::Str("e\u{0301}\u{1f642}\0".into()),
        Value::Str("".into()),
    ];
    let expected = "\u{03a9}:e\u{0301}\u{1f642}\0/";
    assert_eq!(op.output_bytes(&values).unwrap(), expected.len());
    assert_eq!(
        op.materialize(&values, expected.len() - 1),
        Err(MaterializeError::Limit)
    );
    assert_eq!(op.materialize(&values, expected.len()).unwrap(), expected);
    assert_eq!(
        op.materialize(&[Value::Int(7), Value::Str("".into())], 100),
        Err(MaterializeError::Shape)
    );
    assert_eq!(op.materialize(&[], 100), Err(MaterializeError::Shape));
    assert!(StringTemplate::new("{A:>10}".into(), "path".into(), None)
        .unwrap()
        .declaration_diagnostic()
        .is_some());
}
#[test]
fn public_constructor_bounds_all_metadata_and_retained_path_multiplicity() {
    assert_eq!(
        StringTemplate::new("".into(), "".into(), None),
        Err(AdmissionError::Shape)
    );
    assert_eq!(
        StringTemplate::new("x".repeat(MAX_DECLARATION_BYTES / 100), "path".into(), None),
        Err(AdmissionError::Limit)
    );
    assert_eq!(
        StringTemplate::new("{A}".repeat(1000), "p".repeat(1000), None),
        Err(AdmissionError::Limit)
    );
    assert_eq!(
        StringTemplate::new(
            "".into(),
            "path".into(),
            Some(Value::Str("x".repeat(MAX_DECLARATION_BYTES / 8)))
        ),
        Err(AdmissionError::Limit)
    );
    for value in [Value::Missing, Value::Int(7), Value::Str("MiXeD".into())] {
        let op = StringTemplate::new("{A}".into(), "path".into(), Some(value.clone())).unwrap();
        assert_eq!(op.missing(), Some(&value));
    }
}
#[path = "diagnostic/text_template.rs"]
mod diagnostic;
#[test]
fn complete_canonical_grammar_causes_are_reached() {
    assert_eq!(diagnostic::reached().len(), 1);
}
#[test]
fn unique_name_comparison_work_refuses_before_unbounded_dedup_and_resets_per_request() {
    let text = (0..7000)
        .map(|index| format!("{{N{index}}}"))
        .collect::<String>();
    assert!(StringTemplate::declaration_bytes(&text, "path", 0).unwrap() < MAX_DECLARATION_BYTES);
    assert_eq!(
        StringTemplate::new(text, "path".into(), None),
        Err(AdmissionError::Limit)
    );
    for _ in 0..2 {
        assert!(
            StringTemplate::new("{A}-{A}-{B}".into(), "path".into(), None)
                .unwrap()
                .declaration_diagnostic()
                .is_none()
        );
    }
    let op = StringTemplate::new("{A}".into(), "path".into(), None).unwrap();
    assert!(op.input_diagnostic(0, "not a closed type").is_none());
    assert!(op.input_diagnostic(usize::MAX, "int").is_none());
    assert!(op.unknown_diagnostic(usize::MAX).is_none());
}
