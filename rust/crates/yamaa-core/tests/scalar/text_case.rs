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
fn original_ascii_casing_truth_preserves_range_endpoints_and_unicode_word_boundaries() {
    // Retain the original Python expression tests' inclusive-range mutation
    // controls and Unicode cases without consulting a host casing routine.
    for (source, mode, expected) in [
        ("abcZ", Case::Upper, "ABCZ"),
        ("ABCz", Case::Lower, "abcz"),
        ("a1!_", Case::Upper, "A1!_"),
        ("A1!_", Case::Lower, "a1!_"),
        ("z", Case::Upper, "Z"),
        (
            "abcdefghijklmnopqrstuvwxyz",
            Case::Upper,
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        ),
        ("Z", Case::Lower, "z"),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            Case::Lower,
            "abcdefghijklmnopqrstuvwxyz",
        ),
        ("WEEK 8", Case::Sentence, "Week 8"),
        ("WeEk 8", Case::Sentence, "Week 8"),
        ("8-WEEK", Case::Sentence, "8-week"),
        ("a", Case::Sentence, "A"),
        (
            "\u{00df}TRA\u{00df}e",
            Case::Sentence,
            "\u{00df}tra\u{00df}e",
        ),
        ("\u{0131}ABC", Case::Sentence, "\u{0131}abc"),
        ("\u{0130}ABC", Case::Sentence, "\u{0130}abc"),
        ("\u{212a}ELVIN", Case::Sentence, "\u{212a}elvin"),
        ("END OF TREATMENT", Case::Title, "End Of Treatment"),
        ("WEEK 8", Case::Title, "Week 8"),
        ("follow-up", Case::Title, "Follow-Up"),
        ("8-WEEK", Case::Title, "8-Week"),
        ("\u{00e9}COLE", Case::Title, "\u{00e9}Cole"),
        (
            "\u{00e9}cole \u{00c9}COLE",
            Case::Title,
            "\u{00e9}Cole \u{00c9}Cole",
        ),
        ("Stra\u{00df}e", Case::Upper, "STRA\u{00df}E"),
        ("Stra\u{00df}e", Case::Lower, "stra\u{00df}e"),
        ("apple", Case::Title, "Apple"),
        ("pizza", Case::Title, "Pizza"),
        ("ABC", Case::Title, "Abc"),
        ("aZ", Case::Title, "Az"),
    ] {
        let actual = transform(source, mode, source.len()).unwrap();
        assert_eq!(actual, expected, "{source:?}, {mode:?}");
        assert_eq!(actual.chars().count(), source.chars().count());
    }
    for source in [
        "\u{00df}",
        "\u{0130}",
        "\u{0131}",
        "\u{212a}",
        "\u{00e9}",
        "\u{00c9}",
    ] {
        for mode in [Case::Upper, Case::Lower, Case::Sentence, Case::Title] {
            assert_eq!(transform(source, mode, source.len()).unwrap(), source);
        }
    }
    for mode in [Case::Sentence, Case::Title] {
        assert_eq!(transform("", mode, 0).unwrap(), "");
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
#[path = "../diagnostics/diagnostic/text_case.rs"]
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
