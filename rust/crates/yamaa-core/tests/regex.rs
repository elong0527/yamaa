use yamaa_core::regex::{CompileError, CompileLimits, MatchError, MatchLimits, Pattern, Resource};

/// Compile a test pattern under production defaults.
fn pattern(text: &str) -> Pattern {
    Pattern::compile(text, CompileLimits::default()).unwrap()
}
/// Observe borrowed capture text without converting missing groups to empty strings.
fn captures<'a>(text: &str, subject: &'a str) -> Option<Vec<Option<&'a str>>> {
    pattern(text)
        .search(subject, MatchLimits::default())
        .unwrap()
        .map(|m| m.groups)
}

/// Written truth covers leftmost order, greedy/lazy branching and full-match continuation.
#[test]
fn backtracking_order_and_empty_matches() {
    assert_eq!(captures("a|ab", "ab"), Some(vec![Some("a")]));
    assert_eq!(
        pattern("a|ab")
            .full_match("ab", MatchLimits::default())
            .unwrap()
            .unwrap()
            .groups,
        vec![Some("ab")]
    );
    assert_eq!(captures("a+?", "aaa"), Some(vec![Some("a")]));
    assert_eq!(captures("a+", "aaa"), Some(vec![Some("aaa")]));
    assert_eq!(captures("a*", "bbb"), Some(vec![Some("")]));
    assert_eq!(captures("(a?)*", ""), Some(vec![Some(""), None]));
    assert_eq!(captures("(a?)+", ""), Some(vec![Some(""), Some("")]));
    assert_eq!(captures("a{2,3}?a", "aaaa"), Some(vec![Some("aaa")]));
    assert_eq!(captures("a{2,3}a", "aaaa"), Some(vec![Some("aaaa")]));
}

/// Repetition clears descendant capture registers; unset and empty remain distinct.
#[test]
fn backreferences_and_capture_lifecycle() {
    assert_eq!(
        captures(r"(a|(b))+", "aba"),
        Some(vec![Some("aba"), Some("a"), None])
    );
    assert_eq!(
        captures(r"^(a|(b))\2c$", "ac"),
        Some(vec![Some("ac"), Some("a"), None])
    );
    assert_eq!(captures(r"^\1(a)$", "a"), Some(vec![Some("a"), Some("a")]));
    assert_eq!(captures(r"^(a\1)$", "a"), Some(vec![Some("a"), Some("a")]));
    assert_eq!(
        captures(r"^(?<x>a)\k<x>$", "aa"),
        Some(vec![Some("aa"), Some("a")])
    );
    assert_eq!(captures(r"^(a)?\1$", ""), Some(vec![Some(""), None]));
    assert_eq!(captures(r"^(a*)\1$", ""), Some(vec![Some(""), Some("")]));
}

/// Positive assertions are atomic; lookbehind traverses a fixed-width sequence backwards.
#[test]
fn lookaround_order_and_isolation() {
    assert_eq!(
        captures(r"(?=(a+))a*b\1", "baaabac"),
        Some(vec![Some("aba"), Some("a")])
    );
    assert_eq!(
        captures(r"(?<=(a)(b))c", "abc"),
        Some(vec![Some("c"), Some("a"), Some("b")])
    );
    assert_eq!(captures(r"(?<=a|b)c", "ac"), Some(vec![Some("c")]));
    assert_eq!(captures(r"(?<!a)b", "ab"), None);
    assert_eq!(captures(r"(?<!a)b", "cb"), Some(vec![Some("b")]));
    assert_eq!(captures(r"(?!(a))\1b", "b"), Some(vec![Some("b"), None]));
    assert_eq!(
        captures(r"(?<=((a)|(b)){2})c", "abc"),
        Some(vec![Some("c"), Some("a"), Some("a"), None])
    );
}

/// Scalar classes have the repository's exact whitespace/ASCII rules in both polarities.
#[test]
fn unicode_classes_and_escape_grammar() {
    for c in [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}',
        '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
    ] {
        let s = c.to_string();
        assert!(captures(r"^[\s]$", &s).is_some());
        assert!(captures(r"^[\S]$", &s).is_none());
        assert!(captures(r"^[^\S]$", &s).is_some());
    }
    assert!(captures(r"^\s$", "\u{85}").is_none());
    assert!(captures(r"^[\S]$", "\u{85}").is_some());
    assert!(captures(r"^\d$", "\u{665}").is_none());
    assert!(captures(r"^\w$", "\u{e9}").is_none());
    assert_eq!(
        captures(r"^\u{1D400}$", "\u{1d400}"),
        Some(vec![Some("\u{1d400}")])
    );
    assert_eq!(
        captures(r"^\uD835\uDC00$", "\u{1d400}"),
        Some(vec![Some("\u{1d400}")])
    );
    assert_eq!(captures(r"^\cA$", "\u{1}"), Some(vec![Some("\u{1}")]));
    assert_eq!(captures(r"^\0$", "\0"), Some(vec![Some("\0")]));
    assert_eq!(captures(r"[]", "x"), None);
    assert_eq!(captures(r"[^]", "\n"), Some(vec![Some("\n")]));
    assert_eq!(captures(r"\B", ""), Some(vec![Some("")]));
    assert_eq!(captures(r"a$", "a\n"), None);
}

/// Invalid grammar never becomes ordinary no-match or a silently selected host dialect.
#[test]
fn invalid_patterns_and_explicit_remaining_features() {
    for text in [
        r"\01",
        r"\a",
        r"\!",
        r"[\B]",
        r"[\d-a]",
        r"[z-a]",
        r"a{2,1}",
        r"(?P<x>a)",
        r"(?i)a",
        r"\p{L}",
        r"(?<=a+)b",
        r"(?<=a|bc)d",
        r"a++",
        r"a???",
        r"(?=a)*",
        r"(?<x>a)(?<x>b)",
        r"\2(a)",
        r"\k<absent>",
        "(",
        "[",
        r"\",
    ] {
        assert!(
            matches!(
                Pattern::compile(text, CompileLimits::default()),
                Err(CompileError::Invalid { .. })
            ),
            "{text}"
        );
    }
    // These are declared capability gaps, not invalid patterns in the language.
    assert!(matches!(
        Pattern::compile(r"(?<\u0061>x)", CompileLimits::default()),
        Err(CompileError::Unsupported { .. })
    ));
    assert!(matches!(
        Pattern::compile(r"(a)(?<=\1)b", CompileLimits::default()),
        Err(CompileError::Unsupported { .. })
    ));
}

/// Compilation and matching enforce independent budgets; failure cannot poison the next run.
#[test]
fn resource_refusal_and_retry() {
    for (source, limits, resource) in [
        (
            "a",
            CompileLimits {
                bytes: 0,
                ..CompileLimits::default()
            },
            Resource::PatternBytes,
        ),
        (
            "ab",
            CompileLimits {
                nodes: 1,
                ..CompileLimits::default()
            },
            Resource::Nodes,
        ),
        (
            "(a)",
            CompileLimits {
                groups: 0,
                ..CompileLimits::default()
            },
            Resource::Groups,
        ),
        (
            "((a))",
            CompileLimits {
                depth: 1,
                ..CompileLimits::default()
            },
            Resource::Depth,
        ),
        (
            "a{10}",
            CompileLimits {
                repetition: 9,
                ..CompileLimits::default()
            },
            Resource::Repetition,
        ),
    ] {
        assert!(
            matches!(Pattern::compile(source,limits),Err(CompileError::Limit {resource:r,..}) if r==resource)
        );
    }
    let deep = "(".repeat(65) + "a" + &")".repeat(65);
    assert!(matches!(
        Pattern::compile(
            &deep,
            CompileLimits {
                depth: usize::MAX,
                ..CompileLimits::default()
            }
        ),
        Err(CompileError::Limit {
            resource: Resource::Depth,
            limit: 64
        })
    ));
    let compiled = pattern("(a+)+$");
    for (limits, resource) in [
        (
            MatchLimits {
                subject_bytes: 0,
                ..MatchLimits::default()
            },
            Resource::SubjectBytes,
        ),
        (
            MatchLimits {
                work: 8,
                ..MatchLimits::default()
            },
            Resource::Work,
        ),
        (
            MatchLimits {
                state_cells: 2,
                ..MatchLimits::default()
            },
            Resource::StateCells,
        ),
    ] {
        assert!(
            matches!(compiled.search("aaaaaaaaaaaaab",limits),Err(MatchError {resource:r,..}) if r==resource)
        );
    }
    assert_eq!(
        compiled
            .search("a", MatchLimits::default())
            .unwrap()
            .unwrap()
            .groups,
        vec![Some("a"), Some("a")]
    );
}

/// Width arithmetic must refuse by policy rather than misclassify a fixed-width pattern.
#[test]
fn fixed_width_zero_repetition_and_overflow() {
    assert_eq!(captures(r"(?<=(?:a|bc){0})x", "x"), Some(vec![Some("x")]));
    let limits = CompileLimits {
        width: 8,
        ..CompileLimits::default()
    };
    assert!(matches!(
        Pattern::compile(r"(?:a{3}){3}", limits),
        Err(CompileError::Limit {
            resource: Resource::Width,
            limit: 8
        })
    ));
    let large = format!("(?:a{{{}}}){{2}}", usize::MAX);
    assert!(matches!(
        Pattern::compile(
            &large,
            CompileLimits {
                repetition: usize::MAX,
                width: usize::MAX,
                ..CompileLimits::default()
            }
        ),
        Err(CompileError::Limit {
            resource: Resource::Width,
            ..
        })
    ));
}

/// Flat patterns and nested assertions spend one shared budget without native-stack growth per row.
#[test]
fn large_flat_patterns_and_cumulative_assertion_work() {
    let source = "a".repeat(3000);
    let compiled = pattern(&source);
    assert!(compiled
        .full_match(&source, MatchLimits::default())
        .unwrap()
        .is_some());
    let nested = "(?=".repeat(60) + "a" + &")".repeat(60) + "a";
    assert_eq!(captures(&nested, "a"), Some(vec![Some("a")]));
    assert!(matches!(
        pattern(&nested).search(
            "a",
            MatchLimits {
                work: 30,
                ..MatchLimits::default()
            }
        ),
        Err(MatchError {
            resource: Resource::Work,
            ..
        })
    ));
    assert!(matches!(
        pattern("z").search(
            &"a".repeat(100),
            MatchLimits {
                work: 30,
                ..MatchLimits::default()
            }
        ),
        Err(MatchError {
            resource: Resource::Work,
            ..
        })
    ));
}

/// Byte admission never promises sufficient storage; callers can raise a separate cap.
#[test]
fn independent_subject_and_storage_ceilings() {
    let limits = MatchLimits::default();
    let subject = "a".repeat(limits.subject_bytes);
    let compiled = pattern("a");
    assert_eq!(
        compiled.search(&subject, limits),
        Err(MatchError {
            resource: Resource::StateCells,
            limit: limits.state_cells,
        })
    );
    let sufficient = MatchLimits {
        state_cells: 3 * limits.subject_bytes,
        ..limits
    };
    assert_eq!(
        compiled
            .search(&subject, sufficient)
            .unwrap()
            .unwrap()
            .groups,
        vec![Some("a")]
    );
    let oversized = subject + "a";
    assert_eq!(
        compiled.search(&oversized, sufficient),
        Err(MatchError {
            resource: Resource::SubjectBytes,
            limit: limits.subject_bytes,
        })
    );
}

/// REQ-0825 expands braced escapes to REQ-0022 scalars, excluding surrogates.
#[test]
fn braced_code_point_expansion_requires_a_scalar() {
    for value in 0xd800..=0xdfff {
        for source in [format!(r"\u{{{value:X}}}"), format!(r"[\u{{{value:x}}}]")] {
            assert!(
                matches!(
                    Pattern::compile(&source, CompileLimits::default()),
                    Err(CompileError::Invalid { .. })
                ),
                "{source}"
            );
        }
    }
    // The neighboring scalars, unassigned scalars and U+10FFFF stay valid.
    for value in [0xd7ff, 0xe000, 0x378, 0x10ffff] {
        let source = format!(r"^\u{{{value:X}}}$");
        let subject = char::from_u32(value).unwrap().to_string();
        assert_eq!(
            captures(&source, &subject),
            Some(vec![Some(subject.as_str())])
        );
    }
}
