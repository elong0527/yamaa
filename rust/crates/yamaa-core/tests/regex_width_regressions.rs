//! Public width controls retained from the shared-expression implementation.
use std::fmt::Write;
use yamaa_core::regex::{CompileError, Pattern};

fn references(count: usize) -> String {
    let mut source = String::new();
    for number in 1..=count {
        write!(source, r"\{number}").unwrap();
    }
    source
}

fn assert_variable(source: &str) {
    assert_eq!(
        Pattern::compile(source).unwrap_err(),
        CompileError::Invalid {
            byte: source.find("(?<=").unwrap(),
            reason: "variable-length lookbehind",
        },
        "{source}",
    );
}

fn dense_reference_choices(count: usize) -> String {
    let mut source = r"(?:(a)()|()(a))".repeat(count);
    source.push_str("(?<=");
    for left in 0..count {
        for right in left + 1..count {
            write!(
                source,
                r"(?:\{}\{}|\{}\{})",
                4 * left + 1,
                4 * right + 4,
                4 * left + 4,
                4 * right + 1,
            )
            .unwrap();
        }
    }
    source.push(')');
    source
}

#[test]
fn dense_reference_alternatives_reject_witnessed_variation() {
    for count in [4, 8, 16] {
        let source = dense_reference_choices(count);
        // Give two blocks opposite capture participation. The alternative
        // linking those blocks then consumes zero or two, proving variation
        // without requiring the exact extrema of every dense combination.
        assert_variable(&source);
    }
}

#[test]
fn overflow_after_a_dense_assertion_checks_consumption_without_its_width() {
    let mut source = dense_reference_choices(16);
    let byte = source.len();
    write!(source, r"\1{{{}}}", usize::MAX).unwrap();
    // The prefix consumes sixteen scalars and the assertion consumes zero.
    // When capture one participates, its maximum-count repetition adds
    // usize::MAX, so addressability fails at that later operation.
    assert_eq!(
        Pattern::compile(&source),
        Err(CompileError::Unsupported {
            byte,
            feature: "pattern width exceeds addressable range",
        })
    );
}

#[test]
fn dense_assertion_sums_keep_reachable_huge_term_overflow() {
    for count in [4, 8, 16] {
        let dense = dense_reference_choices(count);
        let byte = dense.find("(?<=").unwrap() + 4;
        for term in [
            "a".to_owned(),
            format!(r"\{}", 4 * count - 3),
            format!(r"\{}", 4 * count),
        ] {
            let source = dense.replacen("(?<=", &format!(r"(?<={term}{{{}}}", usize::MAX), 1);
            // At least two outer blocks have the same participation state,
            // so their linking term consumes one and the dense sum is positive.
            // The two final captures require opposite states to contribute MAX.
            // Lookbehind evaluates the dense sum first, then the huge term.
            assert_eq!(
                Pattern::compile(&source),
                Err(CompileError::Unsupported {
                    byte,
                    feature: "pattern width exceeds addressable range",
                }),
                "{source}",
            );
        }
    }
}

#[test]
fn opposite_maximum_branches_require_complete_correlated_width_proofs() {
    let maximum = usize::MAX;
    // Exactly one of captures one and four participates. Their repeated
    // references total MAX in either branch; the relaxed two-MAX bound is false.
    let source = format!(r"(?:(a)()|()(a))(?<=\1{{{maximum}}}\4{{{maximum}}})");
    assert_eq!(Pattern::compile(&source).unwrap().group_count(), 4);

    // A safe MAX branch does not make the other width, two, equal to it.
    let source = format!(r"(?:(a)()|()(aa))(?<=\1{{{maximum}}}\4)");
    assert_variable(&source);

    // The first outer branch still totals MAX, but the second enters both
    // captures five and six. Reverse traversal reaches capture six last and
    // overflows there, after the earlier terms have accumulated at most MAX.
    let source = format!(
        r"(?:(?:(a)()|()(a))|(a)(a))(?<=\6{{{maximum}}}\5{{{maximum}}}\1{{{maximum}}}\4{{{maximum}}})"
    );
    assert_eq!(
        Pattern::compile(&source),
        Err(CompileError::Unsupported {
            byte: source.find(r"\6").unwrap(),
            feature: "pattern width exceeds addressable range",
        })
    );
}

#[test]
fn enclosing_captures_preserve_independent_and_correlated_widths() {
    // Group one captures the thirty-to-sixty scalar sum, and the remaining
    // references reproduce it. Their combined width still varies.
    let source = "(".to_owned() + &"(a|bb)".repeat(30) + ")(?<=" + &references(31) + ")";
    assert_variable(&source);

    // Every inner block totals three. Group one captures ninety scalars and
    // the other 120 references also total ninety, giving fixed width 180.
    let source =
        "(".to_owned() + &r"(?:(a)(bc)|(ab)(c))".repeat(30) + ")(?<=" + &references(121) + ")";
    assert_eq!(Pattern::compile(&source).unwrap().group_count(), 121);
}

#[test]
fn repeated_captures_keep_the_last_iteration_and_incoming_choice() {
    for source in [
        r"(?:(a)()|()(b))(\1){2}(?<=\5\4)",
        r"(?:(a)()|()(b))(\1){1,3}(?<=\5\4)",
        r"(?:(a)|(b)){1000000}(?<=\1\2)",
    ] {
        Pattern::compile(source).unwrap();
    }
    // Capturing the complete repeat doubles the first outer branch's width.
    // Allowing zero iterations also adds an all-unentered capture state.
    for source in [
        r"(?:(a)()|()(b))((?:\1){2})(?<=\5\4)",
        r"(?:(a)|(b)){0,1000000}(?<=\1\2)",
    ] {
        assert_variable(source);
    }
}

fn doubling_capture(initial: &str, count: usize) -> String {
    let mut source = format!("({initial})");
    for number in 1..count {
        write!(source, r"(\{number}\{number})").unwrap();
    }
    write!(source, r"(?<=\{count})").unwrap();
    source
}

#[test]
fn shared_capture_sums_keep_exact_width_and_addressability() {
    // Thirty authored groups produce final width 2^29; repeated references
    // share prior expressions rather than expanding their textual histories.
    assert_eq!(
        Pattern::compile(&doubling_capture("a", 30))
            .unwrap()
            .group_count(),
        30
    );
    assert!(matches!(
        Pattern::compile(&doubling_capture("a", usize::BITS as usize + 1)),
        Err(CompileError::Unsupported {
            feature: "pattern width exceeds addressable range",
            ..
        })
    ));
    // Multiplying an unbounded maximum leaves it unbounded; this is a
    // variable lookbehind rather than a finite width overflow.
    assert_variable(&doubling_capture("a*", 30));
}

#[test]
fn width_overflow_keeps_discarded_body_and_source_ownership() {
    let maximum = usize::MAX;
    for (source, byte) in [
        (format!(r"(aa)\1{{{maximum}}}(?<=\1)"), 4),
        (format!(r"(aa)(?:\1{{{maximum}}}){{0}}(?<=\1)"), 7),
        (format!(r"(a*)(aa)\2{{{maximum}}}(?<=\2)"), 8),
    ] {
        assert_eq!(
            Pattern::compile(&source),
            Err(CompileError::Unsupported {
                byte,
                feature: "pattern width exceeds addressable range",
            }),
            "{source}",
        );
    }
    // Marginalizing the earlier iteration intentionally keeps its unbounded
    // interval, even though one alternative has a finite upper bound.
    assert_variable(&format!(r"(a*|aa)\1{{{maximum}}}(?<=\1)"));
}

#[test]
fn capability_overflow_uses_deterministic_operation_bytes() {
    for prefix in ["", "λ", "λ*"] {
        let body = format!(
            r"(?:(a)(bb)|(aa)(b))(?:\3{{{maximum}}}\2{{{maximum}}})",
            maximum = usize::MAX,
        );
        for consumed in [body.clone(), format!("(?:{body}){{0}}")] {
            let source = format!(r"{prefix}{consumed}(?<=\1)");
            assert_eq!(
                Pattern::compile(&source),
                Err(CompileError::Unsupported {
                    byte: source.find(r"\3").unwrap(),
                    feature: "pattern width exceeds addressable range",
                }),
                "{source}",
            );
        }
    }
}

#[test]
fn a_middle_repeat_clears_only_its_own_capture_interval() {
    let pattern = Pattern::compile(r"^(a)(?:(a)|(b)){2}(d)(?<=\1\2\3\4)e$").unwrap();
    assert_eq!(
        pattern.full_match("aabde").unwrap().unwrap().groups,
        [Some("aabde"), Some("a"), None, Some("b"), Some("d")]
    );
}

#[test]
fn earlier_iteration_projection_retains_its_unbounded_interval() {
    let source = format!(r"(?:(a*)|a{{{}}}){{2}}()(?<=\2)", usize::MAX);
    assert_eq!(Pattern::compile(&source).unwrap().group_count(), 2);
}
