use yamaa_core::text_case::{transform, Case, OutputLimit};

#[test]
fn normative_ascii_modes_preserve_foreign_scalars_and_first_scalar_boundaries() {
    for (source, mode, expected) in [
        ("FOLLOW-UP", Case::Sentence, "Follow-up"),
        ("FOLLOW-UP", Case::Title, "Follow-Up"),
        (" END OF TREATMENT", Case::Sentence, " end of treatment"),
        (" END OF TREATMENT", Case::Title, " End Of Treatment"),
        ("abc_123DEF", Case::Title, "Abc_123Def"),
        (
            "\u{00df}\u{0130}\u{00e9}\u{1f642}AbC",
            Case::Upper,
            "\u{00df}\u{0130}\u{00e9}\u{1f642}ABC",
        ),
        (
            "\u{00df}\u{0130}\u{00e9}\u{1f642}AbC",
            Case::Lower,
            "\u{00df}\u{0130}\u{00e9}\u{1f642}abc",
        ),
        ("\u{00e9}ABC", Case::Sentence, "\u{00e9}abc"),
        ("abc\u{00e9}DEF", Case::Title, "Abc\u{00e9}Def"),
        ("a\0BC", Case::Title, "A\0Bc"),
        ("", Case::Upper, ""),
    ] {
        let actual = transform(source, mode, source.len()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.chars().count(), source.chars().count());
        assert_eq!(actual.len(), source.len());
    }
}
#[test]
fn output_quota_is_checked_on_utf8_bytes_before_substitution() {
    assert_eq!(
        transform("\u{00e9}A", Case::Upper, 2),
        Err(OutputLimit { required: 3 })
    );
    assert_eq!(transform("\u{00e9}A", Case::Upper, 3).unwrap(), "\u{00e9}A");
    assert_eq!(transform("", Case::Title, 0).unwrap(), "");
}
#[path = "diagnostic/text_case.rs"]
mod diagnostic;
#[test]
fn owned_declarations_preserve_missing_literal_presence_and_semantic_truth() {
    use yamaa_core::{
        table::ValueRef,
        text_case::{Step, StringCase},
        value::Value,
    };
    assert_eq!(diagnostic::reached().len(), 2);
    for handler in [Value::Missing, Value::Str("MiXeD".into()), Value::Int(7)] {
        let operation = StringCase::new(
            "SRC.V".into(),
            "case".into(),
            Case::Lower,
            Some(handler.clone()),
        )
        .unwrap();
        assert_eq!(operation.apply(ValueRef::Missing, 0), Ok(Step::Recovered));
        assert_eq!(operation.missing(), Some(&handler));
        assert_eq!(
            operation.apply(ValueRef::Str("ABC"), 3),
            Ok(Step::Value(Value::Str("abc".into())))
        );
    }
}
#[test]
fn owned_metadata_shape_and_whole_copy_size_refuse_before_future_evaluation() {
    use yamaa_core::{
        text_case::{StringCase, MAX_DECLARATION_BYTES},
        value::Value,
    };
    assert!(StringCase::new(String::new(), "path".into(), Case::Upper, None).is_none());
    assert!(StringCase::new("SRC.V".into(), String::new(), Case::Upper, None).is_none());
    let oversized = "x".repeat(MAX_DECLARATION_BYTES / 2);
    assert!(StringCase::new(oversized.clone(), "path".into(), Case::Upper, None).is_none());
    assert!(StringCase::new("SRC.V".into(), oversized.clone(), Case::Upper, None).is_none());
    assert!(StringCase::new(
        "SRC.V".into(),
        "path".into(),
        Case::Upper,
        Some(Value::Str(oversized.clone()))
    )
    .is_none());
    assert!(StringCase::new(
        oversized[..oversized.len() / 2].into(),
        oversized[..oversized.len() / 2].into(),
        Case::Upper,
        None
    )
    .is_none());
}
