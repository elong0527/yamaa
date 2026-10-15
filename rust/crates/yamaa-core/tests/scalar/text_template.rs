use yamaa_core::{
    numeric_parser::SourceSpan,
    text_template::{AdmissionError, MaterializeError, Part, StringTemplate},
    value::Value,
};
#[test]
fn deferred_syntax_refusals_retain_original_utf8_spans() {
    for (text, start, end) in [("é{", 2, 3), ("é}", 2, 3), ("é{A + B}", 2, 9)] {
        let parsed = StringTemplate::new(text.into(), "path".into(), None).unwrap();
        assert_eq!(
            parsed.syntax_failure_span(),
            Some(SourceSpan { start, end })
        );
        assert!(parsed.declaration_diagnostic().is_some());
        assert!(parsed.parts().is_empty());
        assert!(parsed.identifiers().is_empty());
    }
    let parsed = StringTemplate::new("é{{}}{A}".into(), "path".into(), None).unwrap();
    assert_eq!(parsed.syntax_failure_span(), None);
    assert!(parsed.declaration_diagnostic().is_none());
}
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
        op.materialize(&[
            Value::Str("first".into()),
            Value::Str("second".into()),
            Value::Str("third".into())
        ])
        .unwrap(),
        "{first}:second/third"
    );
    assert_eq!(
        StringTemplate::new("".into(), "path".into(), None)
            .unwrap()
            .materialize(&[])
            .unwrap(),
        ""
    );
}
#[test]
fn unicode_scalars_empty_strings_and_nul_have_no_implicit_formatting() {
    let op = StringTemplate::new("\u{03a9}:{A}/{A}".into(), "path".into(), None).unwrap();
    let values = [
        Value::Str("e\u{0301}\u{1f642}\0".into()),
        Value::Str("".into()),
    ];
    let expected = "\u{03a9}:e\u{0301}\u{1f642}\0/";
    assert_eq!(op.output_bytes(&values).unwrap(), expected.len());
    assert_eq!(op.materialize(&values).unwrap(), expected);
    assert_eq!(
        op.materialize(&[Value::Int(7), Value::Str("".into())]),
        Err(MaterializeError::Shape)
    );
    assert_eq!(op.materialize(&[]), Err(MaterializeError::Shape));
    assert!(StringTemplate::new("{A:>10}".into(), "path".into(), None)
        .unwrap()
        .declaration_diagnostic()
        .is_some());
}
#[test]
fn public_constructor_preserves_complete_metadata_and_path_occurrences() {
    assert_eq!(
        StringTemplate::new("".into(), "".into(), None),
        Err(AdmissionError::Shape)
    );
    let literal = "x".repeat(100_000);
    assert_eq!(
        StringTemplate::new(literal.clone(), "path".into(), None)
            .unwrap()
            .materialize(&[])
            .unwrap(),
        literal
    );
    let repeated = StringTemplate::new("{A}".repeat(1000), "p".repeat(1000), None).unwrap();
    assert_eq!(repeated.parts().len(), 1000);
    assert_eq!(repeated.path().len(), 1000);
    let handler = Value::Str("x".repeat(100_000));
    assert_eq!(
        StringTemplate::new("".into(), "path".into(), Some(handler.clone()))
            .unwrap()
            .missing(),
        Some(&handler)
    );
    for value in [Value::Missing, Value::Int(7), Value::Str("MiXeD".into())] {
        let op = StringTemplate::new("{A}".into(), "path".into(), Some(value.clone())).unwrap();
        assert_eq!(op.missing(), Some(&value));
    }
}
#[path = "../diagnostics/diagnostic/text_template.rs"]
mod diagnostic;
#[test]
fn complete_canonical_grammar_causes_are_reached() {
    assert_eq!(diagnostic::reached().len(), 1);
}
#[test]
fn many_unique_names_retain_first_occurrence_indexes_without_a_work_quota() {
    let text = (0..7000)
        .map(|index| format!("{{N{index}}}"))
        .collect::<String>();
    let op = StringTemplate::new(text, "path".into(), None).unwrap();
    assert_eq!(op.identifiers().len(), 7000);
    assert_eq!(op.identifiers().first().unwrap(), "N0");
    assert_eq!(op.identifiers().last().unwrap(), "N6999");
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
