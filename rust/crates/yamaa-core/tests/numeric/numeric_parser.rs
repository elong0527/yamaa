use yamaa_core::numeric_parser::{
    parse_numeric, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind,
    SourcePosition,
};

/// Read grammar errors independently of their human-readable formatting.
fn failure(text: &str) -> (SourcePosition, GrammarFailure) {
    match parse_numeric(text, ParseLimits::default()).unwrap_err() {
        ParseError::Grammar { position, failure } => (position, failure),
        other => panic!("expected grammar error, got {other:?}"),
    }
}

/// Assert which explicit budget halted parsing.
fn limited(text: &str, limits: ParseLimits, expected: ParseResource, maximum: usize) {
    match parse_numeric(text, limits).unwrap_err() {
        ParseError::Limit {
            resource, limit, ..
        } => {
            assert_eq!(resource, expected);
            assert_eq!(limit, maximum);
        }
        other => panic!("expected resource limit, got {other:?}"),
    }
}

/// Group spans must not overwrite the exact identifier or function-name extent.
#[test]
fn spans_and_identifier_order_preserve_original_text() {
    let text = "\u{2003}-(a + ABS(B.x)) + a";
    let parsed = parse_numeric(text, ParseLimits::default()).unwrap();
    assert_eq!(parsed.expression(), text);
    assert_eq!(parsed.identifiers().collect::<Vec<_>>(), ["a", "B.x"]);
    let mut slices = Vec::new();
    for (index, node) in parsed.nodes().iter().enumerate() {
        let source = &text[node.span.start..node.span.end];
        slices.push(source);
        match &node.kind {
            ParsedKind::Group { operand } | ParsedKind::Unary { operand, .. } => {
                assert!(*operand < index);
            }
            ParsedKind::Binary { left, right, .. } => {
                assert!(*left < index && *right < index);
            }
            ParsedKind::Call {
                name, arguments, ..
            } => {
                assert_eq!(&text[name.start..name.end], "ABS");
                assert!(arguments.iter().all(|&child| child < index));
            }
            _ => {}
        }
    }
    assert_eq!(
        slices,
        [
            "a",
            "B.x",
            "ABS(B.x)",
            "a + ABS(B.x)",
            "(a + ABS(B.x))",
            "-(a + ABS(B.x))",
            "a",
            "-(a + ABS(B.x)) + a"
        ]
    );
    assert_eq!(parsed.root(), parsed.nodes().len() - 1);
}

/// Unicode scalar offsets match Python, while spans remain valid UTF-8 bytes.
#[test]
fn grammar_failures_preserve_context_and_both_coordinates() {
    let (position, kind) = failure("\u{2003}\u{a0}A + #");
    assert_eq!(
        position,
        SourcePosition {
            byte: 9,
            character: 6
        }
    );
    assert_eq!(kind.condition(), "invalid_numeric_expression");
    assert_eq!(kind.requirement(), "REQ-0439");
    let (position, kind) = failure("\u{2003}aNd");
    assert_eq!(
        position,
        SourcePosition {
            byte: 3,
            character: 1
        }
    );
    assert_eq!(
        kind,
        GrammarFailure::ProhibitedConstruct {
            construct: "boolean"
        }
    );
    assert_eq!(kind.requirement(), "REQ-0441");
    for (text, count) in [("abS(1, 2)", Some(2)), ("foo(1)", None)] {
        let (_, kind) = failure(text);
        assert_eq!(kind.condition(), "prohibited_function");
        assert_eq!(kind.requirement(), "REQ-0440");
        match kind {
            GrammarFailure::ProhibitedFunction {
                name,
                argument_count,
            } => {
                assert_eq!(&text[name.start..name.end], &text[..3]);
                assert_eq!(argument_count, count);
            }
            _ => panic!("expected function context"),
        }
    }
}

/// Late lexical errors precede syntax; call closure precedes function validation.
#[test]
fn failure_priority_matches_reference() {
    for (text, condition, byte) in [
        ("--A > B", "prohibited_construct", 4),
        ("foo(1", "invalid_numeric_expression", 5),
        ("ABS(1,)", "invalid_numeric_expression", 6),
        ("foo(ABS())", "prohibited_function", 4),
        ("A.1", "invalid_numeric_expression", 0),
        ("A.B.C", "invalid_numeric_expression", 3),
        ("1e+", "invalid_numeric_expression", 1),
        ("--A", "invalid_numeric_expression", 1),
        ("NULL()", "prohibited_function", 0),
        (" ", "invalid_numeric_expression", 1),
    ] {
        let (position, kind) = failure(text);
        assert_eq!(
            (kind.condition(), position.byte),
            (condition, byte),
            "{text}"
        );
    }
}

/// Range and nonfinite-value checks belong to later evaluation, not parsing.
#[test]
fn literals_remain_exact_text_without_host_integer_or_float_conversion() {
    for (text, fractional) in [
        ("0009223372036854775808", false),
        ("1e9999", true),
        ("01.00E+2", true),
        (&"9".repeat(5000), false),
    ] {
        let parsed = parse_numeric(text, ParseLimits::default()).unwrap();
        assert_eq!(
            parsed.nodes()[parsed.root()].kind,
            ParsedKind::Number { fractional }
        );
        assert_eq!(parsed.expression(), text);
    }
    for text in ["A.TRUE", "NULL.x", "_A._B", "A\u{1c}+\u{1f}B"] {
        assert!(
            parse_numeric(text, ParseLimits::default()).is_ok(),
            "{text:?}"
        );
    }
}

/// Exact budget boundaries are accepted, and each next unit is rejected.
#[test]
fn byte_token_node_and_depth_budgets_are_independent() {
    let limits = ParseLimits {
        bytes: 3,
        tokens: 3,
        nodes: 3,
        depth: 2,
    };
    assert!(parse_numeric("A+B", limits).is_ok());
    limited("A+B ", limits, ParseResource::Bytes, 3);
    limited(
        "A+B",
        ParseLimits {
            tokens: 2,
            ..limits
        },
        ParseResource::Tokens,
        2,
    );
    limited(
        "A+B",
        ParseLimits { nodes: 2, ..limits },
        ParseResource::Nodes,
        2,
    );
    limited(
        "A+B",
        ParseLimits { depth: 1, ..limits },
        ParseResource::Depth,
        1,
    );
    limited(
        "A",
        ParseLimits { depth: 0, ..limits },
        ParseResource::Depth,
        0,
    );
    limited(
        "A",
        ParseLimits { nodes: 0, ..limits },
        ParseResource::Nodes,
        0,
    );
    limited(
        "A",
        ParseLimits {
            tokens: 0,
            ..limits
        },
        ParseResource::Tokens,
        0,
    );
    // Byte rejection reports the input as a whole; it must not slice inside UTF-8.
    limited(
        "\u{2003}A",
        ParseLimits { bytes: 1, ..limits },
        ParseResource::Bytes,
        1,
    );
}

/// Both recursion before node creation and left-associated tree depth are bounded.
#[test]
fn adversarial_nesting_chains_and_variadic_calls_are_bounded() {
    let defaults = ParseLimits::default();
    for text in [
        format!("{}A{}", "(".repeat(63), ")".repeat(63)),
        vec!["A"; 64].join("+"),
    ] {
        assert!(parse_numeric(&text, defaults).is_ok());
    }
    for text in [
        format!("{}A{}", "(".repeat(2000), ")".repeat(2000)),
        format!("{}A{}", "ABS(".repeat(1000), ")".repeat(1000)),
        vec!["A"; 1000].join("+"),
    ] {
        limited(
            &text,
            ParseLimits {
                depth: usize::MAX,
                ..defaults
            },
            ParseResource::Depth,
            64,
        );
    }
    // Variadic breadth consumes nodes, even when its tree has only two levels.
    let call = format!("COALESCE({})", vec!["A"; 100].join(","));
    assert!(parse_numeric(&call, defaults).is_ok());
    limited(
        &call,
        ParseLimits {
            nodes: 100,
            ..defaults
        },
        ParseResource::Nodes,
        100,
    );
    limited(&"(".repeat(9000), defaults, ParseResource::Tokens, 8192);
    limited(&" ".repeat(65_537), defaults, ParseResource::Bytes, 65_536);
}
