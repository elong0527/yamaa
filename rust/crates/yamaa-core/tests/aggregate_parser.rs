use yamaa_core::aggregate_parser::{
    parse_aggregate, GrammarFailure, ParseError, ParseLimits, ParseResource, ParsedKind, Reducer,
};

/// Read the structured grammar outcome without relying on diagnostic prose.
fn failure(text: &str) -> (usize, usize, GrammarFailure) {
    match parse_aggregate(text, ParseLimits::default()).unwrap_err() {
        ParseError::Grammar { position, failure } => (position.byte, position.character, failure),
        error => panic!("unexpected limit: {error:?}"),
    }
}

/// Distinguish fieldless record dependencies and ungrouped fields in written order.
#[test]
fn references_and_spans_preserve_written_context() {
    let text = "\u{2003}COALESCE(sUm( D.A + D.B ), COUNT(D.*)) + D.A + X + COUNT(E.*) + X";
    let parsed = parse_aggregate(text, ParseLimits::default()).unwrap();
    assert_eq!(parsed.expression(), text);
    assert_eq!(
        parsed.identifiers().collect::<Vec<_>>(),
        ["D.A", "D.B", "X"]
    );
    assert_eq!(
        parsed.ungrouped_identifiers().collect::<Vec<_>>(),
        ["D.A", "X"]
    );
    assert_eq!(parsed.star_datasets().collect::<Vec<_>>(), ["D", "E"]);
    let mut reductions = Vec::new();
    for (id, node) in parsed.nodes().iter().enumerate() {
        let mut children = Vec::new();
        match &node.kind {
            ParsedKind::Reduction {
                reducer,
                name,
                operand,
            } => {
                reductions.push((
                    *reducer,
                    &text[node.span.start..node.span.end],
                    &text[name.start..name.end],
                ));
                children.push(*operand);
            }
            ParsedKind::Group { operand } | ParsedKind::Unary { operand, .. } => {
                children.push(*operand)
            }
            ParsedKind::Binary { left, right, .. } => children.extend([*left, *right]),
            ParsedKind::Call { arguments, .. } => children.extend(arguments),
            _ => {}
        }
        assert!(children.into_iter().all(|child| child < id));
    }
    assert_eq!(
        reductions,
        [
            (Reducer::Sum, "sUm( D.A + D.B )", "sUm"),
            (Reducer::Count, "COUNT(D.*)", "COUNT"),
            (Reducer::Count, "COUNT(E.*)", "COUNT")
        ]
    );
}

/// Closing syntax and complete lexing retain priority over semantic call checks.
#[test]
fn diagnostics_have_aggregate_requirements_and_exact_priority() {
    for (text, byte, condition, requirement) in [
        ("SUM(A", 5, "invalid_aggregate_expression", "REQ-0499"),
        ("SUM(A,B)", 5, "invalid_aggregate_expression", "REQ-0499"),
        ("SUM(D.*)", 4, "invalid_aggregate_expression", "REQ-0499"),
        ("COUNT(*)", 6, "invalid_aggregate_expression", "REQ-0499"),
        (
            "COUNT((D.*))",
            7,
            "invalid_aggregate_expression",
            "REQ-0499",
        ),
        (
            "COUNT(D.*,A)",
            9,
            "invalid_aggregate_expression",
            "REQ-0499",
        ),
        ("D.*", 0, "invalid_aggregate_expression", "REQ-0499"),
        ("SUM(MIN(A))", 0, "nested_reduction", "REQ-0502"),
        ("SUM(MIN(A)", 10, "invalid_aggregate_expression", "REQ-0499"),
        ("SUM(MIN(A)) > X", 12, "prohibited_construct", "REQ-0512"),
        ("SUM(ABS())", 4, "prohibited_function", "REQ-0500"),
        ("--A AND B", 4, "prohibited_construct", "REQ-0512"),
        ("AVG(A)", 0, "prohibited_function", "REQ-0500"),
        ("AVG(A", 5, "invalid_aggregate_expression", "REQ-0499"),
        ("COUNT(D .*)", 8, "invalid_aggregate_expression", "REQ-0499"),
    ] {
        let (actual, _, failure) = failure(text);
        assert_eq!(
            (actual, failure.condition(), failure.requirement()),
            (byte, condition, requirement),
            "{text}"
        );
    }
    assert_eq!(
        failure("\u{2003}SUM(COALESCE(MIN(A), MAX(B)))"),
        (
            3,
            1,
            GrammarFailure::NestedReduction {
                outer: Reducer::Sum,
                inner: Reducer::Min
            }
        )
    );
    assert_eq!(
        failure("SUM(MAX(MIN(A)))").2,
        GrammarFailure::NestedReduction {
            outer: Reducer::Max,
            inner: Reducer::Min
        }
    );
    let (_, _, error) = failure("abS(1,2)");
    assert!(matches!(
        error,
        GrammarFailure::ProhibitedFunction {
            argument_count: Some(2),
            ..
        }
    ));
}

/// Literal range and missingness are evaluation concerns, never parser checks.
#[test]
fn literal_spelling_and_identifier_vocabulary_are_not_evaluated() {
    for text in [
        "SUM(0009223372036854775808)",
        "SUM(1e9999)",
        "NULL",
        "SUM + COUNT",
        "COUNT(TRUE.*)",
        "SUM(A.NULL)",
        "ONLY(NULL)",
    ] {
        assert!(
            parse_aggregate(text, ParseLimits::default()).is_ok(),
            "{text}"
        );
    }
    let text = format!("SUM({})", "9".repeat(5000));
    let parsed = parse_aggregate(&text, ParseLimits::default()).unwrap();
    let literal = &parsed.nodes()[0];
    assert_eq!(literal.kind, ParsedKind::Number { fractional: false });
    assert_eq!(
        &text[literal.span.start..literal.span.end],
        "9".repeat(5000)
    );
}

/// Independent limits bound source, token allocations, arena size and recursion.
#[test]
fn resource_limits_and_adversarial_inputs_are_bounded() {
    let defaults = ParseLimits::default();
    for (limits, resource, limit) in [
        (
            ParseLimits {
                bytes: 3,
                ..defaults
            },
            ParseResource::Bytes,
            3,
        ),
        (
            ParseLimits {
                tokens: 3,
                ..defaults
            },
            ParseResource::Tokens,
            3,
        ),
        (
            ParseLimits {
                nodes: 1,
                ..defaults
            },
            ParseResource::Nodes,
            1,
        ),
        (
            ParseLimits {
                depth: 1,
                ..defaults
            },
            ParseResource::Depth,
            1,
        ),
    ] {
        assert!(
            matches!(parse_aggregate("SUM(A)",limits),Err(ParseError::Limit {resource:r,limit:l,..}) if r==resource && l==limit)
        );
    }
    for text in [
        format!("{}A{}", "SUM(".repeat(10000), ")".repeat(10000)),
        "A+".repeat(10000),
        format!("COALESCE({})", vec!["SUM(A)"; 3000].join(",")),
    ] {
        assert!(matches!(
            parse_aggregate(&text, defaults),
            Err(ParseError::Limit { .. })
        ));
    }
    let text = format!("{}A{}", "(".repeat(65), ")".repeat(65));
    assert!(matches!(
        parse_aggregate(
            &text,
            ParseLimits {
                depth: usize::MAX,
                ..defaults
            }
        ),
        Err(ParseError::Limit {
            resource: ParseResource::Depth,
            limit: 64,
            ..
        })
    ));
}
