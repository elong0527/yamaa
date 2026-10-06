use super::*;

fn values(source: &str) -> Vec<SourceText> {
    scan(source.as_bytes(), ScanLimits::default())
        .unwrap()
        .into_iter()
        .filter_map(|event| match event.event {
            SourceEvent::Scalar(value, ..) => Some(value),
            _ => None,
        })
        .collect()
}

fn unicode(text: &str) -> SourceText {
    SourceText::Unicode(text.into())
}

#[test]
fn preserves_lexemes_styles_duplicates_and_document_boundaries() {
    let events = scan(
        b"x: 9007199254740993\nx: 9223372036854775808\ny: 'true'\n---\nfalse\n",
        ScanLimits::default(),
    )
    .unwrap();
    let scalars: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.event {
            SourceEvent::Scalar(value, style, ..) => Some((value.clone(), *style)),
            _ => None,
        })
        .collect();
    assert_eq!(
        scalars,
        vec![
            (unicode("x"), ScalarStyle::Plain),
            (unicode("9007199254740993"), ScalarStyle::Plain),
            (unicode("x"), ScalarStyle::Plain),
            (unicode("9223372036854775808"), ScalarStyle::Plain),
            (unicode("y"), ScalarStyle::Plain),
            (unicode("true"), ScalarStyle::SingleQuoted),
            (unicode("false"), ScalarStyle::Plain),
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.event, SourceEvent::DocumentStart(_)))
            .count(),
        2
    );
    assert_eq!(
        events[3].start,
        SourcePosition {
            offset: 0,
            line: 1,
            column: 1
        }
    );
    assert_eq!(
        events[5].start,
        SourcePosition {
            offset: 20,
            line: 2,
            column: 1
        }
    );
}

#[test]
fn retains_forbidden_metadata_for_document_admission() {
    let events = scan(
        b"x: &label !!str 1\ny: *label\nz: {<<: 2, '<<': 3}\n",
        ScanLimits::default(),
    )
    .unwrap();
    assert!(events.iter().any(|e| matches!(&e.event,
        SourceEvent::Scalar(_, _, 1, Some(tag)) if tag.handle == "tag:yaml.org,2002:" && tag.suffix == "str"
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e.event, SourceEvent::Alias(1))));
    assert!(events.iter().any(|e| matches!(&e.event,
        SourceEvent::Scalar(SourceText::Unicode(text), ScalarStyle::Plain, ..) if text == "<<"
    )));
    assert!(events.iter().any(|e| matches!(&e.event,
        SourceEvent::Scalar(SourceText::Unicode(text), ScalarStyle::SingleQuoted, ..) if text == "<<"
    )));
}

#[test]
fn reconstructs_surrogates_without_reserving_private_use_characters() {
    assert_eq!(
        values(r#"["\uE000\uD800\uE800\uDFFF\uE7FF\uEFFF", "\U0000d801"]"#),
        vec![
            SourceText::Invalid(vec![0xe000, 0xd800, 0xe800, 0xdfff, 0xe7ff, 0xefff]),
            SourceText::Invalid(vec![0xd801]),
        ]
    );
}

#[test]
fn every_surrogate_recovers_with_both_escape_widths() {
    for point in 0xd800..=0xdfff {
        let source = format!(r#"["\u{point:04X}", "\U{point:08x}"]"#);
        assert_eq!(
            values(&source),
            vec![
                SourceText::Invalid(vec![point]),
                SourceText::Invalid(vec![point])
            ]
        );
    }
}

#[test]
fn preserves_folding_escapes_and_scalar_offsets() {
    // The folded newline becomes one space; the escaped newline disappears.
    let source = "x: [\"a\n  b\\n\\U0001F600\\\n  \\uD800\\t\\uDFFF\"]\n";
    assert_eq!(
        values(source),
        vec![
            unicode("x"),
            SourceText::Invalid(vec![0x61, 0x20, 0x62, 0x0a, 0x1f600, 0xd800, 0x09, 0xdfff,])
        ]
    );
}

#[test]
fn only_effective_escapes_in_double_quotes_are_repaired() {
    let source = "# \\uD800\nplain: \\uD800\nsingle: '\\uD800'\nblock: |\n  \\uD800\ndouble: \"\\\\uD800 \\\" \\uD800\"\n";
    assert_eq!(
        values(source),
        vec![
            unicode("plain"),
            unicode("\\uD800"),
            unicode("single"),
            unicode("\\uD800"),
            unicode("block"),
            unicode("\\uD800\n"),
            unicode("double"),
            SourceText::Invalid(vec![
                0x5c, 0x75, 0x44, 0x38, 0x30, 0x30, 0x20, 0x22, 0x20, 0xd800,
            ]),
        ]
    );
}

#[test]
fn repaired_keys_remain_distinct_from_actual_private_use_keys() {
    assert_eq!(
        values(r#"{"\uD800": 1, "\uE000": 2, "\U0000D800": 3}"#),
        vec![
            SourceText::Invalid(vec![0xd800]),
            unicode("1"),
            unicode("\u{e000}"),
            unicode("2"),
            SourceText::Invalid(vec![0xd800]),
            unicode("3"),
        ]
    );
}

#[test]
fn later_syntax_error_wins_over_earlier_surrogate() {
    for source in [
        r#"["\uD800", [}"#,
        r#"["\uD800", "\q"]"#,
        r#"["\uD800", "\u123"]"#,
        r#"["\uD800", "unterminated"#,
        r#"["\uD800", "\U00110000"]"#,
        r#"["\uD800\q"]"#,
        r#"["\uD800\U00110000"]"#,
    ] {
        assert!(
            matches!(
                scan(source.as_bytes(), ScanLimits::default()),
                Err(ScanFailure::Syntax { .. })
            ),
            "{source}"
        );
    }
}

#[test]
fn non_surrogate_bad_escapes_remain_syntax_errors() {
    for source in [r#""\UFFFFFFFF""#, r#""\uZZZZ""#, r#""\U00110000""#] {
        assert!(matches!(
            scan(source.as_bytes(), ScanLimits::default()),
            Err(ScanFailure::Syntax { .. })
        ));
    }
}

#[test]
fn ascii_rejection_precedes_yaml_and_has_original_coordinates() {
    assert_eq!(
        scan(b"x: [\n y: \xff", ScanLimits::default()),
        Err(ScanFailure::NonAscii(SourcePosition {
            offset: 9,
            line: 2,
            column: 5
        },))
    );
}

#[test]
fn failed_recovery_never_returns_a_partial_or_repaired_document() {
    let source = br#"["\uD800", "\uDFFF"]"#;
    for passes in 0..4 {
        let limits = ScanLimits {
            parse_bytes: passes * source.len(),
            ..ScanLimits::default()
        };
        assert_eq!(scan(source, limits), Err(ScanFailure::Limit("parse_bytes")));
    }
    assert!(scan(
        source,
        ScanLimits {
            parse_bytes: 4 * source.len(),
            ..ScanLimits::default()
        }
    )
    .is_ok());
}

#[test]
fn enforces_source_event_depth_and_decoded_limits() {
    assert_eq!(
        scan(
            b"abc",
            ScanLimits {
                source_bytes: 2,
                ..ScanLimits::default()
            }
        ),
        Err(ScanFailure::Limit("source_bytes"))
    );
    assert_eq!(
        scan(
            b"abc",
            ScanLimits {
                events: 1,
                ..ScanLimits::default()
            }
        ),
        Err(ScanFailure::Limit("events"))
    );
    assert_eq!(
        scan(
            b"[[[1]]]",
            ScanLimits {
                depth: 2,
                ..ScanLimits::default()
            }
        ),
        Err(ScanFailure::Limit("depth"))
    );
    assert_eq!(
        scan(
            b"abc",
            ScanLimits {
                decoded_bytes: 2,
                ..ScanLimits::default()
            }
        ),
        Err(ScanFailure::Limit("decoded_bytes"))
    );
    assert!(scan(
        b"[[1]]",
        ScanLimits {
            depth: 2,
            ..ScanLimits::default()
        }
    )
    .is_ok());
    assert_eq!(
        scan(
            b"",
            ScanLimits {
                parse_bytes: 0,
                ..ScanLimits::default()
            }
        ),
        Err(ScanFailure::Limit("parse_bytes"))
    );
}
