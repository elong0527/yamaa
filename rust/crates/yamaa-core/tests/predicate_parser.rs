use yamaa_core::predicate::Comparison;
use yamaa_core::predicate_parser::{
    parse_predicate, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind,
    TemporalKind,
};
use yamaa_core::{regex, temporal::TemporalError};

/// Syntax association is independent of evaluator truth and preserves written names.
#[test]
fn precedence_spelling_and_names() {
    let p = parse_predicate(
        "NOT A = +0001 OR B < -2.50e+3 AND (A IS NULL)",
        ParseLimits::default(),
    )
    .unwrap();
    assert_eq!(p.identifiers().collect::<Vec<_>>(), ["A", "B"]);
    let nodes = p.nodes();
    let ParsedKind::Or(left, right) = nodes[p.root()].kind else {
        panic!()
    };
    assert!(matches!(nodes[left].kind, ParsedKind::Not(_)));
    let ParsedKind::And(_, group) = nodes[right].kind else {
        panic!()
    };
    assert!(matches!(nodes[group].kind, ParsedKind::Group(_)));
    let numbers: Vec<_> = nodes
        .iter()
        .filter_map(|n| match n.kind {
            ParsedKind::Number { fractional } => {
                Some((&p.expression()[n.span.start..n.span.end], fractional))
            }
            _ => None,
        })
        .collect();
    assert_eq!(numbers, [("+0001", false), ("-2.50e+3", true)]);
    let p = parse_predicate("FALSE OR TRUE OR FALSE", ParseLimits::default()).unwrap();
    let ParsedKind::Or(left, _) = p.nodes()[p.root()].kind else {
        panic!()
    };
    assert!(matches!(p.nodes()[left].kind, ParsedKind::Or(..)));
}

/// Every closed comparison and compound production admits only operand syntax.
#[test]
fn closed_vocabulary() {
    for (op, expected) in [
        ("=", Comparison::Equal),
        ("<>", Comparison::NotEqual),
        ("<", Comparison::Less),
        ("<=", Comparison::LessEqual),
        (">", Comparison::Greater),
        (">=", Comparison::GreaterEqual),
    ] {
        let p = parse_predicate(&format!("A {op} NULL"), ParseLimits::default()).unwrap();
        assert!(
            matches!(p.nodes()[p.root()].kind,ParsedKind::Compare{operator,..} if operator==expected)
        );
    }
    for text in [
        "A NOT IN (1, 2, NULL)",
        "A BETWEEN -1 AND 1 OR FALSE",
        "A NOT BETWEEN B AND C",
        "A NOT LIKE '%' ESCAPE '!'",
        "A IS NOT NULL",
        "str_contains = 'column'",
        "D.TRUE = 1",
        "STR_CONTAINS(NULL, '^$')",
    ] {
        parse_predicate(text, ParseLimits::default()).unwrap();
    }
    for text in [
        "",
        " ",
        "A",
        "NULL",
        "A != 1",
        "A == 1",
        "A IN ()",
        "A IS TRUE",
        "A IN ((1))",
        "A BETWEEN 1 OR 2",
        "A NOT = 1",
        "A LIKE '%' ESCAPE 1",
        "A+1 = 2",
        "TRUE()",
        "str_detect(A, 'a')",
        "str_contains(A, B)",
        "str_contains(A)",
        "str_contains(A, 'a', 'b')",
        "NOT NULL",
        "(A) = 1",
        "A. = 1",
        "A.B.C = 1",
        "A = 1.",
        "A = .1",
        "A = - 1",
        "A = 1e+",
        "A = 0x10",
        "A = 'open",
        "A = TRUE",
    ] {
        assert!(
            matches!(
                parse_predicate(text, ParseLimits::default()),
                Err(ParseError::Grammar { .. })
            ),
            "{text}"
        );
    }
    for keyword in [
        "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE", "NOT", "NULL",
        "OR", "TRUE",
    ] {
        parse_predicate(&format!("D.{keyword} = 1"), ParseLimits::default()).unwrap();
    }
}

/// Doubled quotes decode exactly once; other Unicode, backslash and NUL are literal.
#[test]
fn text_temporal_and_unbounded_number_syntax() {
    let text = "'it''s\\\0\u{1f600}' = 'x'";
    let p = parse_predicate(text, ParseLimits::default()).unwrap();
    assert_eq!(
        p.nodes()[0].kind,
        ParsedKind::String("it's\\\0\u{1f600}".into())
    );
    for text in [
        "A = DATE '2024-02-29'",
        "A = DATETIME '2024-02-29T12:34'",
        "A = DATETIME '2024-02-29T12:34:56'",
        "A = 9999999999999999999999999999999",
        "A = 1e9999",
    ] {
        parse_predicate(text, ParseLimits::default()).unwrap();
    }
    assert!(matches!(
        parse_predicate("A = DATE '2025-02-30'", ParseLimits::default()),
        Err(ParseError::Grammar {
            failure: GrammarFailure::InvalidTemporal {
                kind: TemporalKind::Date,
                error: TemporalError::InvalidDate
            },
            ..
        })
    ));
    for text in [
        "A = DATE '2024'",
        "A = DATETIME '2024-02-29T12:34Z'",
        "A = DATETIME '2024-02-29T24:00'",
        "A = DATE '0000-01-01'",
    ] {
        assert!(matches!(
            parse_predicate(text, ParseLimits::default()),
            Err(ParseError::Grammar {
                failure: GrammarFailure::InvalidTemporal { .. },
                ..
            })
        ));
    }
}

/// Regex admission uses portable grammar, including features rejected by the reference host.
#[test]
fn portable_regex_literals_are_checked_before_closing_token() {
    for pattern in [
        r"[]",
        r"[^]",
        r"\1(a)",
        r"(a)(?<=\1)",
        r"(?<\u0061>a)\k<a>",
        r"[\S]",
    ] {
        let text = format!("str_contains(A, '{pattern}')");
        let p = parse_predicate(&text, ParseLimits::default()).unwrap();
        assert!(matches!(
            p.nodes()[p.root()].kind,
            ParsedKind::Contains { .. }
        ));
    }
    let text = "str_contains('\u{1f600}', '('";
    let Err(ParseError::Grammar { position, failure }) =
        parse_predicate(text, ParseLimits::default())
    else {
        panic!()
    };
    assert_eq!(position.byte, 21);
    assert_eq!(position.character, 18);
    assert!(matches!(
        failure,
        GrammarFailure::InvalidRegex { byte: 1, .. }
    ));
    assert_eq!(failure.requirement(), "REQ-1244");
    // Whole-input tokenization still precedes invalid regex admission.
    assert!(matches!(
        parse_predicate("str_contains(A, '(') @", ParseLimits::default()),
        Err(ParseError::Grammar {
            failure: GrammarFailure::InvalidExpression,
            ..
        })
    ));
    assert!(matches!(
        parse_predicate("str_contains(A, 'a{1000001}')", ParseLimits::default()),
        Err(ParseError::RegexLimit {
            resource: regex::Resource::Repetition,
            limit: 1_000_000,
            ..
        })
    ));
}

/// Static ESCAPE defects have their owning requirement and original literal site.
#[test]
fn escape_admission_and_unicode_positions() {
    for text in [
        "A LIKE 'a!' ESCAPE '!'",
        "A LIKE 'a' ESCAPE ''",
        "A LIKE 'a' ESCAPE 'ab'",
    ] {
        let Err(ParseError::Grammar { failure, .. }) =
            parse_predicate(text, ParseLimits::default())
        else {
            panic!("{text}")
        };
        assert_eq!(failure, GrammarFailure::InvalidEscape);
        assert_eq!(failure.requirement(), "REQ-0191");
    }
    for text in [
        "A LIKE 'a!!' ESCAPE '!'",
        "A LIKE 'a!!!b' ESCAPE '!'",
        "A LIKE B ESCAPE '!'",
        "A LIKE 'a\u{1f600}%' ESCAPE '\u{1f600}'",
    ] {
        parse_predicate(text, ParseLimits::default()).unwrap();
    }
    let Err(ParseError::Grammar { position, .. }) =
        parse_predicate("'\u{1f600}' = @", ParseLimits::default())
    else {
        panic!()
    };
    assert_eq!((position.byte, position.character), (9, 6));
}

/// Finite policies cannot be defeated by flat chains, repeated NOT or nested groups.
#[test]
fn resource_limits_and_fresh_retry() {
    let defaults = ParseLimits::default();
    let cases = [
        (
            "TRUE".into(),
            ParseLimits {
                bytes: 3,
                ..defaults
            },
            ParseResource::Bytes,
        ),
        (
            "A = 1".into(),
            ParseLimits {
                tokens: 2,
                ..defaults
            },
            ParseResource::Tokens,
        ),
        (
            "A = 1".into(),
            ParseLimits {
                nodes: 2,
                ..defaults
            },
            ParseResource::Nodes,
        ),
        (
            "TRUE AND ".repeat(70) + "TRUE",
            ParseLimits {
                depth: usize::MAX,
                ..defaults
            },
            ParseResource::Depth,
        ),
        (
            "NOT ".repeat(70) + "TRUE",
            ParseLimits {
                depth: usize::MAX,
                ..defaults
            },
            ParseResource::Depth,
        ),
        (
            format!("{}TRUE{}", "(".repeat(70), ")".repeat(70)),
            ParseLimits {
                depth: usize::MAX,
                ..defaults
            },
            ParseResource::Depth,
        ),
    ];
    for (text, limits, resource) in cases {
        assert!(
            matches!(parse_predicate(&text,limits),Err(ParseError::Limit{resource:r,..}) if r==resource)
        );
        parse_predicate("TRUE", defaults).unwrap();
    }
    let p = parse_predicate(&format!("A IN ({})", vec!["1"; 1000].join(",")), defaults).unwrap();
    assert_eq!(p.nodes().len(), 1002);
}
