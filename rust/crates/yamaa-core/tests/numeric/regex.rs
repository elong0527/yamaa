use yamaa_core::regex::{
    CompileError, CompileLimits, MatchBudget, MatchError, MatchLimits, Pattern, Resource,
};

/// Successful matches cannot repeatedly reset the surrounding work/storage scope.
#[test]
fn cumulative_match_work_storage_and_fresh_retry() {
    let compiled = pattern("a+");
    let limits = MatchLimits::default();
    let mut measured = MatchBudget::new(limits);
    assert_eq!(
        compiled
            .search_with_budget("aaaa", limits, &mut measured)
            .unwrap()
            .unwrap()
            .groups,
        [Some("aaaa")]
    );
    let cost = measured.used();
    assert!(cost.work > 0 && cost.state_cells > 0);
    for resource in [Resource::Work, Resource::StateCells] {
        let mut shared_limits = limits;
        let ceiling = if resource == Resource::Work {
            shared_limits.work = 2 * cost.work - 1;
            shared_limits.work
        } else {
            shared_limits.state_cells = 2 * cost.state_cells - 1;
            shared_limits.state_cells
        };
        let mut shared = MatchBudget::new(shared_limits);
        assert!(compiled
            .search_with_budget("aaaa", limits, &mut shared)
            .unwrap()
            .is_some());
        assert_eq!(
            compiled
                .search_with_budget("aaaa", limits, &mut shared)
                .unwrap_err(),
            MatchError {
                resource,
                limit: ceiling
            }
        );
        let used = shared.used();
        assert!(used.work >= cost.work && used.state_cells >= cost.state_cells);
        assert!(used.work <= shared_limits.work && used.state_cells <= shared_limits.state_cells);
        let mut retry = MatchBudget::new(limits);
        assert!(compiled
            .search_with_budget("aaaa", limits, &mut retry)
            .unwrap()
            .is_some());
        assert_eq!(retry.used(), cost);
    }
}

/// Subject quotas use UTF-8 bytes across both modes; local limits and no-match stay distinct.
#[test]
fn cumulative_subject_bytes_and_failure_prefixes() {
    let compiled = pattern(".");
    let limits = MatchLimits::default();
    let mut shared = MatchBudget::new(MatchLimits {
        subject_bytes: 7,
        ..limits
    });
    assert_eq!(
        compiled
            .search_with_budget("😀", limits, &mut shared)
            .unwrap()
            .unwrap()
            .groups,
        [Some("😀")]
    );
    let before = shared.used();
    assert_eq!(before.subject_bytes, 4);
    assert_eq!(
        compiled
            .full_match_with_budget("😀", limits, &mut shared)
            .unwrap_err(),
        MatchError {
            resource: Resource::SubjectBytes,
            limit: 7
        }
    );
    assert_eq!(shared.used(), before);
    assert_eq!(
        compiled
            .search_with_budget(
                "x",
                MatchLimits {
                    subject_bytes: 0,
                    ..limits
                },
                &mut shared
            )
            .unwrap_err(),
        MatchError {
            resource: Resource::SubjectBytes,
            limit: 0
        }
    );
    assert_eq!(shared.used(), before);
    let mut fresh = MatchBudget::new(MatchLimits {
        subject_bytes: 8,
        ..limits
    });
    assert!(compiled
        .search_with_budget("😀", limits, &mut fresh)
        .unwrap()
        .is_some());
    assert!(compiled
        .full_match_with_budget("😀", limits, &mut fresh)
        .unwrap()
        .is_some());
    assert_eq!(fresh.used().subject_bytes, 8);

    let mut failed = MatchBudget::new(MatchLimits { work: 0, ..limits });
    assert_eq!(
        compiled
            .search_with_budget("x", limits, &mut failed)
            .unwrap_err(),
        MatchError {
            resource: Resource::Work,
            limit: 0
        }
    );
    assert_eq!(failed.used().subject_bytes, 1);
    assert_eq!(failed.used().work, 0);
    assert!(failed.used().state_cells > 0);
    let mut fresh = MatchBudget::new(limits);
    assert!(pattern("a")
        .search_with_budget("b", limits, &mut fresh)
        .unwrap()
        .is_none());
    assert!(fresh.used().work > 0);
    assert_eq!(fresh.used().subject_bytes, 1);
}

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

/// Width admission follows capture participation and reverse evaluation order.
#[test]
fn lookbehind_backreference_width_uses_capture_state() {
    for (source, subject, expected) in [
        (r"^(a)(?<=\1)b$", "ab", vec![Some("ab"), Some("a")]),
        (r"^(?<x>a)(?<=\k<x>)b$", "ab", vec![Some("ab"), Some("a")]),
        (r"(?<=\1(a))b", "aab", vec![Some("b"), Some("a")]),
        (r"(?<=(a)\1)b", "ab", vec![Some("b"), Some("a")]),
        (r"^(?<=\1)(a*)$", "aaa", vec![Some("aaa"), Some("aaa")]),
        (r"^()(?:a)(?<=\1+)b$", "ab", vec![Some("ab"), Some("")]),
        (r"^(?=(a))a(?<=\1)b$", "ab", vec![Some("ab"), Some("a")]),
        (r"^(?!(a))(?<=\1)b$", "b", vec![Some("b"), None]),
    ] {
        assert_eq!(captures(source, subject), Some(expected), "{source}");
    }
    // Alternative capture states must stay correlated: either pair totals three.
    assert_eq!(
        captures(r"^(?:(a)(bc)|(ab)(c))(?<=\1\2\3\4)d$", "abcd"),
        Some(vec![Some("abcd"), Some("a"), Some("bc"), None, None])
    );
}

/// Prior variable/optional captures can make an otherwise plain reference vary.
#[test]
fn lookbehind_backreference_width_rejects_proven_variation() {
    for source in [
        r"(a+)(?<=\1)b",
        r"(a)?(?<=\1)b",
        r"(?:a|(b))(?<=\1)c",
        r"(a)(?<=\1+)b",
    ] {
        assert!(
            matches!(
                Pattern::compile(source, CompileLimits::default()),
                Err(CompileError::Invalid {
                    reason: "variable-length lookbehind",
                    ..
                })
            ),
            "{source}"
        );
    }
}

/// Final repeat captures belong to one cleared iteration, in either direction.
#[test]
fn lookbehind_width_preserves_repeat_capture_lifecycle() {
    for (source, subject, expected) in [
        (
            r"^(?:(a)|(b)){2}(?<=\1\2)c$",
            "abc",
            vec![Some("abc"), None, Some("b")],
        ),
        (r"^(?:(a)(?<=\1)){2}b$", "aab", vec![Some("aab"), Some("a")]),
        (r"(?<=(?:\1(a)){2})b", "aaaab", vec![Some("b"), Some("a")]),
        (r"(?<=(?:(a)\1){2})b", "aab", vec![Some("b"), Some("a")]),
        (
            r"^((?:a|b){2})(?<=\1)c$",
            "abc",
            vec![Some("abc"), Some("ab")],
        ),
        (r"^(a){1,3}(?<=\1)b$", "aaab", vec![Some("aaab"), Some("a")]),
        (r"^(){0,3}(?<=\1)b$", "b", vec![Some("b"), None]),
        (r"^(){2,3}(?<=\1)b$", "b", vec![Some("b"), Some("")]),
        (r"^(a){0}(?<=\1)b$", "b", vec![Some("b"), None]),
    ] {
        assert_eq!(captures(source, subject), Some(expected), "{source}");
    }
    for source in [
        r"(a|bb){2}(?<=\1)c",
        r"((?:a|bb){2})(?<=\1)c",
        r"(a){0,2}(?<=\1)b",
        r"(a?){1,2}(?<=\1)b",
        r"(?<=(?:\1(a)){1,2})b",
    ] {
        assert!(
            matches!(
                Pattern::compile(source, CompileLimits::default()),
                Err(CompileError::Invalid {
                    reason: "variable-length lookbehind",
                    ..
                })
            ),
            "{source}"
        );
    }
}

/// References can compensate unequal literal alternatives in reverse traversal.
#[test]
fn lookbehind_width_preserves_reverse_alternative_correlation() {
    let source = r"(?<=\1\2(?:(a)bb|(aa)))c";
    assert_eq!(
        captures(source, "aabbc"),
        Some(vec![Some("c"), Some("a"), None])
    );
    assert_eq!(
        captures(source, "aaaac"),
        Some(vec![Some("c"), None, Some("aa")])
    );
    assert_eq!(captures(source, "aaac"), None);
    // A reference outside its own open capture is unset until that capture closes.
    assert_eq!(
        captures(r"^(\1a)(?<=\1)b$", "ab"),
        Some(vec![Some("ab"), Some("a")])
    );
    assert_eq!(
        captures(r"^(\2a)(\1b)(?<=\1\2)c$", "aabc"),
        Some(vec![Some("aabc"), Some("a"), Some("ab")])
    );
}

/// Assertion state commits only on positive assertions; syntax checks remain structural.
#[test]
fn lookbehind_width_validates_nested_and_unexecuted_assertions() {
    assert_eq!(
        captures(r"^(a)(?<!(?<=\1)b)c$", "ac"),
        Some(vec![Some("ac"), Some("a")])
    );
    assert_eq!(
        captures(r"^(?!(a+))(?<=\1)b$", "b"),
        Some(vec![Some("b"), None])
    );
    assert_eq!(
        captures(r"^(?=(a))(?<=(?=\1))a$", "a"),
        Some(vec![Some("a"), Some("a")])
    );
    for source in [
        r"(?:(?<=a+)){0}(a)\1",
        r"(?!(?<=a+))(a)\1",
        r"(?<=(?=a+)(?<=b+))(a)\1",
        r"(?=(a+))(?<=\1)b",
        r"(?:(a+)(?<=\1)){0}",
    ] {
        assert!(
            matches!(
                Pattern::compile(source, CompileLimits::default()),
                Err(CompileError::Invalid {
                    reason: "variable-length lookbehind",
                    ..
                })
            ),
            "{source}"
        );
    }
    assert_eq!(
        Pattern::compile("\u{e9}(a+)(?<=\\1)b", CompileLimits::default()).unwrap_err(),
        CompileError::Invalid {
            byte: 6,
            reason: "variable-length lookbehind"
        }
    );
}

/// Exponential capture choices stop at caller budgets, and a fresh compile can retry.
#[test]
fn lookbehind_width_analysis_budgets_and_retry() {
    let source = r"(a)(?<=\1)b";
    for (limits, expected) in [
        (
            CompileLimits {
                width_work: 0,
                ..CompileLimits::default()
            },
            Resource::WidthWork,
        ),
        (
            CompileLimits {
                width_cells: 0,
                ..CompileLimits::default()
            },
            Resource::WidthCells,
        ),
    ] {
        assert!(matches!(Pattern::compile(source, limits),
            Err(CompileError::Limit { resource, limit: 0 }) if resource == expected));
        assert!(Pattern::compile(source, CompileLimits::default()).is_ok());
    }
    let choices = "(?:(a)|b)".repeat(16) + r"(?<=\1)";
    for (limits, expected) in [
        (
            CompileLimits {
                width_work: 1000,
                ..CompileLimits::default()
            },
            Resource::WidthWork,
        ),
        (
            CompileLimits {
                width_cells: 1000,
                ..CompileLimits::default()
            },
            Resource::WidthCells,
        ),
    ] {
        assert!(matches!(Pattern::compile(&choices, limits),
            Err(CompileError::Limit { resource, limit: 1000 }) if resource == expected));
    }
    // Symbolic count handling must not unroll a million repetitions.
    assert!(Pattern::compile(r"(a){1000000}(?<=\1)", CompileLimits::default()).is_ok());
    assert!(Pattern::compile(r"(a)(?<=\1{1000000})", CompileLimits::default()).is_ok());
    assert!(matches!(
        Pattern::compile(r"(aa)(?<=\1{1000000})", CompileLimits::default()),
        Err(CompileError::Limit {
            resource: Resource::Width,
            ..
        })
    ));
    let flat = "a".repeat(3000) + r"(a)(?<=\1)";
    assert!(Pattern::compile(&flat, CompileLimits::default()).is_ok());
}

/// An unrelated reference must not make a statically fixed assertion need analysis.
#[test]
fn fixed_lookbehind_does_not_analyze_unrelated_capture_paths() {
    let choices = "(?:(a)|b)".repeat(16) + r"(?<=c)\1";
    for source in [&choices, r"(a{1000})\1{2000}(?<=b)", r"(a)(?<=(?=\1))a"] {
        assert!(
            Pattern::compile(source, CompileLimits::default()).is_ok(),
            "{source}"
        );
        assert!(
            Pattern::compile(
                source,
                CompileLimits {
                    width_work: 0,
                    width_cells: 0,
                    ..CompileLimits::default()
                }
            )
            .is_ok(),
            "{source}"
        );
    }
    // A fixed outer assertion must not hide a dependent nested assertion.
    assert!(matches!(
        Pattern::compile(r"(a+)(?<=(?<=\1))b", CompileLimits::default()),
        Err(CompileError::Invalid {
            reason: "variable-length lookbehind",
            ..
        })
    ));
}

/// Integer count truth distinguishes a final capture from a whole repeated group.
#[test]
fn lookbehind_width_agrees_with_independent_count_truth() {
    let assert_fixed = |source: &str, fixed: bool| {
        let compiled = Pattern::compile(source, CompileLimits::default());
        if fixed {
            assert!(compiled.is_ok(), "{source}: {compiled:?}");
        } else {
            assert!(
                matches!(
                    compiled,
                    Err(CompileError::Invalid {
                        reason: "variable-length lookbehind",
                        ..
                    })
                ),
                "{source}: {compiled:?}"
            );
        }
    };
    for width in 0..5 {
        for min in 0..5 {
            for max in min..5 {
                assert_fixed(
                    &format!(r"(a{{{width}}}){{{min},{max}}}(?<=\1)"),
                    width == 0 || min > 0 || max == 0,
                );
                assert_fixed(
                    &format!(r"((?:a{{{width}}}){{{min},{max}}})(?<=\1)"),
                    width == 0 || min == max,
                );
            }
        }
    }
    for left in 0..5 {
        for right in 0..5 {
            for count in 0..4 {
                let fixed = count == 0 || left == right;
                assert_fixed(
                    &format!(r"(?:(a{{{left}}})|(a{{{right}}})){{{count}}}(?<=\1\2)"),
                    fixed,
                );
                assert_fixed(
                    &format!(r"(?<=(?:\1\2(?:(a{{{left}}})|(a{{{right}}}))){{{count}}})x"),
                    fixed,
                );
            }
        }
    }
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
fn invalid_patterns() {
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
}

/// Names use decoded ID properties, not host identifiers, XID or normalization.
#[test]
fn unicode_group_names_preserve_decoded_identity() {
    for name in [
        "x",
        "$",
        "_",
        "\u{e9}",
        "\u{37a}",
        "\u{1d400}",
        "a\u{301}",
        "a\u{200c}\u{200d}",
        "a\u{30fb}\u{ff65}",
        "a0",
    ] {
        let escaped: String = name
            .chars()
            .map(|c| format!(r"\u{{{:X}}}", c as u32))
            .collect();
        for (definition, reference) in [(name, escaped.as_str()), (escaped.as_str(), name)] {
            assert_eq!(
                captures(&format!(r"^(?<{definition}>a)\k<{reference}>$"), "aa"),
                Some(vec![Some("aa"), Some("a")])
            );
            assert_eq!(
                captures(&format!(r"^\k<{reference}>(?<{definition}>a)$"), "a"),
                Some(vec![Some("a"), Some("a")])
            );
        }
    }
    assert_eq!(
        captures(r"^(?<\uD835\uDC00>a)\k<\u{1D400}>$", "aa"),
        Some(vec![Some("aa"), Some("a")])
    );
    assert_eq!(
        captures(
            "^(?<\u{e9}>a)(?<e\u{301}>b)\\k<\u{e9}>\\k<e\u{301}>$",
            "abab"
        ),
        Some(vec![Some("abab"), Some("a"), Some("b")])
    );
}

/// Invalid name scalars and escapes own syntax errors at their source positions.
#[test]
fn unicode_group_names_reject_invalid_positions_and_aliases() {
    for name in [
        "",
        "0x",
        "\u{301}a",
        "\u{200c}",
        "\u{1f600}",
        "a-",
        r"\x61",
        r"\cA",
        r"\uD800",
        r"\uDC00",
        r"\u{D800}",
        r"\u{110000}",
        r"\u{}",
        r"\u12",
        r"\u0030x",
    ] {
        for source in [format!("(?<{name}>a)"), format!(r"\k<{name}>(?<x>a)")] {
            assert!(
                matches!(
                    Pattern::compile(&source, CompileLimits::default()),
                    Err(CompileError::Invalid { .. })
                ),
                "{source}"
            );
        }
    }
    for source in [r"(?<x>a)(?<\u0078>b)", "(?<\u{e9}>a)(?<\\u00E9>b)"] {
        assert!(matches!(
            Pattern::compile(source, CompileLimits::default()),
            Err(CompileError::Invalid {
                reason: "duplicate group name",
                ..
            })
        ));
    }
    assert_eq!(
        Pattern::compile("(?<\u{e9}\\u0030->a)", CompileLimits::default()).unwrap_err(),
        CompileError::Invalid {
            byte: 11,
            reason: "invalid group name"
        }
    );
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

/// Known variable consumption stays invalid even when a backreference is also present.
#[test]
fn lookbehind_width_distinguishes_variable_from_backreference_dependent() {
    for source in [
        r"(a)(?<=a+\1)b",
        r"(a)(?<=\1a+)b",
        r"(a)(?<=a+|\1)b",
        r"(a)(?<=\1|a+)b",
        r"(a)(?<=a|\1|bb)b",
        r"(a)(?<=(?=\1)a+)b",
        r"(a)(?<=(?:a+\1){2})b",
        r"(a)(?<=(?:b\1)+)c",
    ] {
        assert!(
            matches!(
                Pattern::compile(source, CompileLimits::default()),
                Err(CompileError::Invalid { .. })
            ),
            "{source}"
        );
    }
    // A backreference can be empty, so repetition alone does not prove variability.
    for source in [r"(a)(?<=\1)b", r"()(?<=\1+)b", r"()(?<=\1{1,2})b"] {
        assert!(
            Pattern::compile(source, CompileLimits::default()).is_ok(),
            "{source}"
        );
    }
    // Zero repetitions and zero-width assertions do not consume their descendants.
    assert_eq!(
        captures(r"(a)(?<=(?:a+\1){0})b", "ab"),
        Some(vec![Some("ab"), Some("a")])
    );
    assert_eq!(
        captures(r"(?<=((?=a))*?)a", "a"),
        Some(vec![Some("a"), None])
    );
    assert_eq!(
        captures(r"(a)(?<=(?=\1))a", "aa"),
        Some(vec![Some("aa"), Some("a")])
    );
}
