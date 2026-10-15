use super::*;
use DocumentNode as N;

fn decode(source: &str) -> DecodedYaml {
    decode_yaml(source.as_bytes(), DecodeLimits::default()).unwrap()
}

fn scalar(source: &str) -> N {
    let result = decode(source);
    result.document.nodes()[result.document.root()].clone()
}

fn error(source: &str) -> DecodeFailure {
    decode_yaml(source.as_bytes(), DecodeLimits::default()).unwrap_err()
}

fn yaml_error(source: &str, line: usize, column: usize, reason: &str) {
    match error(source) {
        DecodeFailure::InvalidYaml {
            position,
            reason: actual,
        } => {
            assert_eq!(
                (position.line, position.column),
                (line, column),
                "{source:?}"
            );
            assert_eq!(actual, reason, "{source:?}");
        }
        other => panic!("{source:?}: {other:?}"),
    }
}

#[test]
fn core_booleans_nulls_and_text_do_not_use_yaml11_resolvers() {
    for spelling in ["true", "True", "TRUE"] {
        assert_eq!(scalar(spelling), N::Boolean(true));
    }
    for spelling in ["false", "False", "FALSE"] {
        assert_eq!(scalar(spelling), N::Boolean(false));
    }
    for spelling in [
        "",
        "# empty\n",
        "---\n",
        "null",
        "Null",
        "NULL",
        "~",
        ".inf",
        "-.Inf",
        "+.INF",
        ".NaN",
        "1e9999",
    ] {
        assert_eq!(scalar(spelling), N::Null, "{spelling}");
    }
    for spelling in [
        "yes",
        "No",
        "ON",
        "off",
        "y",
        "N",
        "TrUe",
        "nUlL",
        "2026-10-06",
        "2026-10-06T12:34:56Z",
        "inf",
        "NaN",
        "+.nan",
        "0b101",
        "0XFF",
        "+0x10",
        "-0o10",
        "1_000",
        "1:20",
        "1e",
        ".",
        "1.2.3",
    ] {
        assert_eq!(scalar(spelling), N::Text(spelling.into()), "{spelling}");
    }
    assert_eq!(scalar("'true'"), N::Text("true".into()));
    assert_eq!(scalar("\"null\""), N::Text("null".into()));
    assert_eq!(scalar("|\n  12\n"), N::Text("12\n".into()));
}

#[test]
fn implicit_empty_nodes_are_null_while_quoted_empty_scalars_are_text() {
    assert_eq!(scalar("''"), N::Text(String::new()));
    assert_eq!(scalar("\"\""), N::Text(String::new()));
    assert_eq!(
        decode("empty:\nquoted: ''\nexplicit: null\n")
            .document
            .nodes(),
        &[
            N::Text("empty".into()),
            N::Null,
            N::Text("quoted".into()),
            N::Text(String::new()),
            N::Text("explicit".into()),
            N::Null,
            N::Mapping(vec![(0, 1), (2, 3), (4, 5)]),
        ]
    );
    assert_eq!(
        decode("-\n- ''\n- null\n").document.nodes(),
        &[
            N::Null,
            N::Text(String::new()),
            N::Null,
            N::Sequence(vec![0, 1, 2]),
        ]
    );
}

#[test]
fn implicit_empty_mapping_keys_share_null_duplicate_identity() {
    yaml_error(
        ": first\nnull: second\n",
        2,
        1,
        "found duplicate key 'None'",
    );
    yaml_error(": first\n~: second\n", 2, 1, "found duplicate key 'None'");
    assert!(decode_yaml(b"'': text\n: empty\n", DecodeLimits::default()).is_ok());
}

#[test]
fn comments_cannot_interrupt_multiline_plain_scalars() {
    for (source, column) in [
        ("first  # comment\nsecond\n", 8),
        ("key: first  # comment\n  second\n", 13),
    ] {
        yaml_error(source, 1, column, "comment intercepting the multiline text");
    }
    assert_eq!(scalar("first\nsecond\n"), N::Text("first second".into()));
    assert_eq!(
        decode("key: first\n  second\n").document.nodes(),
        &[
            N::Text("key".into()),
            N::Text("first second".into()),
            N::Mapping(vec![(0, 1)]),
        ]
    );
}

#[test]
fn integers_keep_exact_identity_before_runtime_range_admission() {
    for (source, expected) in [
        ("+00077", "77"),
        ("077", "77"),
        ("-00077", "-77"),
        ("-0", "0"),
        ("0o777", "511"),
        ("0xFF", "255"),
        ("9007199254740993", "9007199254740993"),
        ("9223372036854775807", "9223372036854775807"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("9223372036854775808", "9223372036854775808"),
        ("9223372036854775809", "9223372036854775809"),
        ("0x8000000000000000", "9223372036854775808"),
        ("0xffffffffffffffff", "18446744073709551615"),
        ("0o2000000000000000000000", "18446744073709551616"),
    ] {
        assert_eq!(scalar(source), N::Integer(expected.into()), "{source}");
    }
}

#[test]
fn finite_float_identity_and_signed_zero_are_retained() {
    for (source, expected) in [
        ("1.", 1.0_f64),
        (".25", 0.25),
        ("-1e2", -100.0),
        ("2E-2", 0.02),
        ("-0.0", -0.0),
        ("-1e-9999", -0.0),
    ] {
        let N::Float(value) = scalar(source) else {
            panic!("{source}")
        };
        assert_eq!(value.to_bits(), expected.to_bits(), "{source}");
    }
}

#[test]
fn document_order_locations_and_unique_occurrences_survive_decoding() {
    let result = decode("z: [1, 1]\na: {b: null}\n");
    assert_eq!(
        result.document.nodes(),
        &[
            N::Text("z".into()),
            N::Integer("1".into()),
            N::Integer("1".into()),
            N::Sequence(vec![1, 2]),
            N::Text("a".into()),
            N::Text("b".into()),
            N::Null,
            N::Mapping(vec![(5, 6)]),
            N::Mapping(vec![(0, 3), (4, 7)]),
        ]
    );
    assert_eq!(result.document.root(), 8);
    assert_eq!(
        result.locations[4],
        SourcePosition {
            offset: 10,
            line: 2,
            column: 1
        }
    );
    assert_eq!(result.locations.len(), 9);
}

#[test]
fn different_large_integer_keys_are_never_rounded_or_collapsed() {
    let result = decode("{9223372036854775808: a, 9223372036854775809: b, 9007199254740993: c, 9007199254740992.0: d}");
    let nodes = result.document.nodes();
    assert_eq!(nodes[0], N::Integer("9223372036854775808".into()));
    assert_eq!(nodes[2], N::Integer("9223372036854775809".into()));
    assert_eq!(nodes[4], N::Integer("9007199254740993".into()));
    assert_eq!(nodes[6], N::Float(9007199254740992.0));
    assert_eq!(nodes[8], N::Mapping(vec![(0, 1), (2, 3), (4, 5), (6, 7)]));
}

#[test]
fn numeric_and_nonfinite_duplicate_identity_is_shared_with_core() {
    for (left, right, label) in [
        ("true", "1", "1"),
        ("false", "-0.0", "-0.0"),
        ("1", "1.0", "1.0"),
        ("1", "01", "1"),
        ("0x10", "16", "16"),
        ("null", ".nan", "None"),
        (".inf", "1e9999", "None"),
        (
            "18446744073709551616",
            "18446744073709551616.0",
            "1.8446744073709552e+19",
        ),
    ] {
        yaml_error(
            &format!("{left}: a\n{right}: b\n"),
            2,
            1,
            &format!("found duplicate key '{label}'"),
        );
    }
    assert!(decode_yaml(b"{'1': a, 1: b, .5: c, -.5: d}", DecodeLimits::default()).is_ok());
}

#[test]
fn forbidden_metadata_reports_its_prefix_and_precedes_later_errors() {
    for (source, line, column, reason) in [
        ("x: &v 1\n", 1, 4, "YAML anchors are not allowed"),
        ("x: !!str 1\n", 1, 4, "explicit YAML tags are not allowed"),
        (
            "x: !<tag:yaml.org,2002:str> 1\n",
            1,
            4,
            "explicit YAML tags are not allowed",
        ),
        (
            "x: !<tag:yaml.org,2002:seq> []\n",
            1,
            4,
            "explicit YAML tags are not allowed",
        ),
        (
            "x: !<tag:yaml.org,2002:map> {}\n",
            1,
            4,
            "explicit YAML tags are not allowed",
        ),
        ("x: !!str &v 1\n", 1, 4, "YAML anchors are not allowed"),
        ("x: &v !!str 1\n", 1, 4, "YAML anchors are not allowed"),
        ("x: [ &v 1 ]\n", 1, 6, "YAML anchors are not allowed"),
        ("x: {a: &v 1}\n", 1, 8, "YAML anchors are not allowed"),
        (
            "---\n# !decoy &decoy\nx:\n  &v 1\n",
            4,
            3,
            "YAML anchors are not allowed",
        ),
        (
            "# !decoy &decoy\n!!str 1\n",
            2,
            1,
            "explicit YAML tags are not allowed",
        ),
        ("[&v 1, \"\\uD800\"]", 1, 2, "YAML anchors are not allowed"),
        (
            "---\n1\n---\n2\n",
            3,
            1,
            "expected a single document in the stream",
        ),
    ] {
        yaml_error(source, line, column, reason);
    }
    assert!(matches!(
        error("x: *unknown"),
        DecodeFailure::InvalidYaml { .. }
    ));
    // Both reference and native scanners may look ahead before emitting a
    // tagged/anchored node. A lexical defect in that lookahead wins.
    assert!(
        matches!(error("[&v 1, \"\\q\"]"), DecodeFailure::InvalidYaml { reason, .. }
        if reason.contains("unknown escape"))
    );
}

#[test]
fn merge_key_style_and_non_scalar_keys_are_checked() {
    yaml_error("x: {<<: 1}\n", 1, 5, "YAML merge keys are not allowed");
    assert!(decode_yaml(b"x: {'<<': 1}\ny: <<\n", DecodeLimits::default()).is_ok());
    yaml_error("? [a]\n: 1\n", 1, 3, "a mapping key must be a scalar");
    yaml_error("? {a: 1}\n: 2\n", 1, 3, "a mapping key must be a scalar");
}

#[test]
fn deferred_mapping_error_order_matches_reference_construction() {
    yaml_error(
        "a:\n  x: 1\n  x: 2\na: 3\n",
        4,
        1,
        "found duplicate key 'a'",
    );
    yaml_error(
        "- child:\n    x: 1\n    x: 2\n- y: 1\n  y: 2\n",
        5,
        3,
        "found duplicate key 'y'",
    );
    yaml_error(
        "a: 1\na: 2\n<<: 3\n",
        3,
        1,
        "YAML merge keys are not allowed",
    );
    yaml_error(
        "x: \"\\uD800\"\ny: 1\ny: 2\n",
        3,
        1,
        "found duplicate key 'y'",
    );
    yaml_error(
        "\"\\uD800\": 1\n\"\\U0000D800\": 2\n",
        2,
        1,
        "found duplicate key '\\ud800'",
    );
    assert!(
        matches!(error("{a: 1, a: 2, z: [}"), DecodeFailure::InvalidYaml { reason, .. } if !reason.starts_with("found duplicate key"))
    );
}

#[test]
fn unicode_findings_use_original_paths_and_scalar_offsets() {
    let source = r#"{"\uD800": "\uDFFF", "\u00e9": ["a\uD801\uD802"], 2.0: "\uD803", true: "\uD804", null: "\uD805"}"#;
    let expected = [
        ("$.<key>", 0xd800, 0),
        ("$.\\ud800", 0xdfff, 0),
        ("$.\\xe9[0]", 0xd801, 1),
        ("$.2.0", 0xd803, 0),
        ("$.True", 0xd804, 0),
        ("$.None", 0xd805, 0),
    ]
    .into_iter()
    .map(|(path, code_point, offset)| UnicodeIssue {
        path: path.into(),
        code_point,
        offset,
    })
    .collect();
    assert_eq!(error(source), DecodeFailure::InvalidText(expected));
}

#[test]
fn private_use_keys_remain_distinct_from_recovered_invalid_keys() {
    let expected = vec![UnicodeIssue {
        path: "$.<key>".into(),
        code_point: 0xd800,
        offset: 0,
    }];
    assert_eq!(
        error(r#"{"\uD800": 1, "\uE000": 2}"#),
        DecodeFailure::InvalidText(expected)
    );
}

#[test]
fn document_and_numeric_quotas_fail_explicitly_and_fresh_retry_succeeds() {
    let limits = DecodeLimits {
        numeric_digits: 3,
        ..DecodeLimits::default()
    };
    for source in ["1234", "0xffff", "0o7777", "1.234", "1e9999"] {
        assert_decode_limit(
            decode_yaml(source.as_bytes(), limits).unwrap_err(),
            "numeric_digits",
            limits.numeric_digits,
        );
    }
    assert!(decode_yaml(b"'1234'", limits).is_ok());
    for (source, document, resource) in [
        (
            "x",
            DocumentLimits {
                nodes: 0,
                ..DocumentLimits::default()
            },
            "nodes",
        ),
        (
            "text",
            DocumentLimits {
                text_bytes: 3,
                ..DocumentLimits::default()
            },
            "text_bytes",
        ),
        (
            "[1, 2]",
            DocumentLimits {
                edges: 1,
                ..DocumentLimits::default()
            },
            "edges",
        ),
        (
            "[[1]]",
            DocumentLimits {
                depth: 2,
                ..DocumentLimits::default()
            },
            "depth",
        ),
        (
            r#"[["\uD800"]]"#,
            DocumentLimits {
                depth: 2,
                ..DocumentLimits::default()
            },
            "depth",
        ),
    ] {
        assert_decode_limit(
            decode_yaml(
                source.as_bytes(),
                DecodeLimits {
                    document,
                    ..DecodeLimits::default()
                },
            )
            .unwrap_err(),
            resource,
            match resource {
                "nodes" => document.nodes,
                "text_bytes" => document.text_bytes,
                "edges" => document.edges,
                "depth" => document.depth.min(64),
                _ => unreachable!(),
            },
        );
    }
    assert_decode_limit(
        decode_yaml(
            br#"["\uD800"]"#,
            DecodeLimits {
                diagnostic_bytes: 1,
                ..DecodeLimits::default()
            },
        )
        .unwrap_err(),
        "diagnostic_bytes",
        1,
    );
    assert_eq!(scalar("1234"), N::Integer("1234".into()));
}

#[test]
fn duplicate_key_reasons_identify_text_and_respect_diagnostic_limits() {
    yaml_error(
        "{F: Female, M: Male, F: Other}",
        1,
        22,
        "found duplicate key 'F'",
    );
    yaml_error(
        "{?a: x, b: y, \"?a\": z}",
        1,
        15,
        "found duplicate key '?a'",
    );
    yaml_error(
        "{\"a\\nb\": 1, \"a\\nb\": 2}",
        1,
        13,
        "found duplicate key 'a\\nb'",
    );
    let source = b"{long: 1, long: 2}";
    let reason = "found duplicate key 'long'";
    for limit in [reason.len() - 1, reason.len()] {
        let result = decode_yaml(
            source,
            DecodeLimits {
                diagnostic_bytes: limit,
                ..DecodeLimits::default()
            },
        );
        if limit < reason.len() {
            assert_decode_limit(result.unwrap_err(), "diagnostic_bytes", limit);
        } else {
            assert!(
                matches!(result.unwrap_err(), DecodeFailure::InvalidYaml { reason: actual, .. } if actual == reason)
            );
        }
    }
}

fn assert_decode_limit(error: DecodeFailure, resource: &'static str, expected: usize) {
    let DecodeFailure::Limit(limit) = error else {
        panic!("expected decoder quota refusal")
    };
    assert_eq!(limit.resource, resource);
    assert_eq!(limit.limit, expected);
    if let Some(actual) = limit.actual {
        assert!(actual > limit.limit);
    }
}
