use yamaa_core::{
    predicate, regex,
    table::ValueRef,
    text_extract::{AdmissionError, Error, Step, StringExtract, MAX_DECLARATION_BYTES},
    value::Value,
};
fn operation(
    pattern: &str,
    group: &str,
    missing: Option<Value>,
    no_match: Option<Value>,
) -> StringExtract {
    StringExtract::new(
        "SRC.TEXT".into(),
        "columns.TEXT.derivation.str_extract".into(),
        pattern.into(),
        group.into(),
        missing,
        no_match,
    )
    .unwrap()
}
fn budget() -> predicate::Budget {
    predicate::Budget::new(predicate::Limits::default().into())
}
#[test]
fn shared_search_capture_truth_preserves_absence_empty_unicode_and_nul() {
    for (pattern, group, subject, expected) in [
        ("(a)?b", "1", "b", Step::Capture(None)),
        ("()b", "1", "b", Step::Capture(Some(""))),
        ("()", "0", "abc", Step::Capture(Some(""))),
        ("a", "-0", "abc", Step::Capture(Some("a"))),
        ("(a|ab)", "1", "zab", Step::Capture(Some("a"))),
        ("(a|b)+", "1", "ab", Step::Capture(Some("b"))),
        ("x(.)", "1", "x\u{1f642}", Step::Capture(Some("\u{1f642}"))),
        ("x(.)", "1", "x\0", Step::Capture(Some("\0"))),
        ("(a)", "1", "xyz", Step::NoMatch("xyz")),
    ] {
        assert_eq!(
            operation(pattern, group, None, None).apply(
                ValueRef::Str(subject),
                Default::default(),
                &mut budget()
            ),
            Ok(expected)
        );
    }
}
#[test]
fn omitted_handlers_null_literals_and_nonconversion_are_distinct() {
    for handler in [Value::Missing, Value::Str("12".into()), Value::Int(12)] {
        let declared = operation("(a)", "1", Some(handler.clone()), Some(handler.clone()));
        assert_eq!(
            declared.apply(ValueRef::Missing, Default::default(), &mut budget()),
            Ok(Step::Recovered("missing"))
        );
        assert_eq!(
            declared.apply(ValueRef::Str("x"), Default::default(), &mut budget()),
            Ok(Step::Recovered("no_match"))
        );
        assert_eq!(declared.handler("missing"), Some(&handler));
        assert_eq!(
            declared.apply(ValueRef::Str("a"), Default::default(), &mut budget()),
            Ok(Step::Capture(Some("a")))
        );
    }
    assert_eq!(
        operation("()", "0", None, None).apply(
            ValueRef::Missing,
            Default::default(),
            &mut budget()
        ),
        Ok(Step::Missing)
    );
}
#[test]
fn matcher_limits_and_shared_consumer_usage_never_become_no_match() {
    let op = operation("(a)", "1", None, Some(Value::Missing));
    let mut usage = predicate::Usage::from(predicate::Limits::default());
    usage.regex.subject_bytes = 3;
    let mut shared = predicate::Budget::new(usage);
    assert_eq!(
        op.apply(ValueRef::Str("aa"), Default::default(), &mut shared),
        Ok(Step::Capture(Some("a")))
    );
    let other = regex::Pattern::compile("a", Default::default()).unwrap();
    assert!(shared
        .search_pattern(&other, "aa", Default::default())
        .is_err());
    assert_eq!(shared.used().regex.subject_bytes, 2);
    let mut shared = budget();
    let limits = regex::MatchLimits {
        work: 0,
        ..Default::default()
    };
    assert!(matches!(
        op.apply(ValueRef::Str("x"), limits, &mut shared),
        Err(Error::Limit(_))
    ));
}
#[test]
fn constructor_and_failure_context_are_admitted_before_copying_or_parsing() {
    assert_eq!(
        StringExtract::new("".into(), "p".into(), "[".into(), "0".into(), None, None),
        Err(AdmissionError::Shape)
    );
    assert_eq!(
        StringExtract::new("s".into(), "p".into(), "[".into(), "+1".into(), None, None),
        Err(AdmissionError::Shape)
    );
    assert_eq!(
        StringExtract::new(
            "s".into(),
            "p".into(),
            "[".repeat(MAX_DECLARATION_BYTES / 512),
            "0".into(),
            None,
            None
        ),
        Err(AdmissionError::Limit)
    );
    assert_eq!(
        StringExtract::new(
            "s".into(),
            "p".into(),
            "a".into(),
            "0".into(),
            Some(Value::Str("x".repeat(MAX_DECLARATION_BYTES / 4))),
            None
        ),
        Err(AdmissionError::Limit)
    );
    let op = operation("(a)", "1", None, None);
    assert!(op.failure_diagnostic(Step::NoMatch("x"), 0).is_none());
    let size = op.failure_text_bytes(Step::NoMatch("x")).unwrap();
    assert!(op
        .failure_diagnostic(Step::NoMatch("x"), size - 1)
        .is_none());
    assert!(op.failure_diagnostic(Step::NoMatch("x"), size).is_some());
}
#[path = "diagnostic/text_extract.rs"]
mod diagnostic;
#[test]
fn all_extraction_semantic_causes_preserve_independent_complete_facts() {
    assert_eq!(diagnostic::reached().len(), 6);
}
