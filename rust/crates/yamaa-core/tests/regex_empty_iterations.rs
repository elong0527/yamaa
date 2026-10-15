//! Empty iteration admission and the public distinction between unset and empty captures.
use std::fmt::Write;
use yamaa_core::regex::{CompileError, Pattern};

fn references(count: usize) -> String {
    let mut source = String::new();
    for number in 1..=count {
        write!(source, r"\{number}").unwrap();
    }
    source
}

#[test]
fn independent_empty_variants_keep_every_reference_at_width_zero() {
    // Each capture is empty or belongs to a zero-consuming optional iteration
    // that cannot commit it. All references therefore consume zero, including
    // after repetition, a discarded body, and a negative assertion. Admission
    // remains structural even when the negative assertion cannot succeed.
    for count in [2, 4, 30] {
        for (unit, groups) in [
            (r"(?:()|())?", 2),
            (r"(()|())?", 3),
            (r"(?:()|())*", 2),
            (r"(?:()|())", 2),
            (r"(?:()|()){1,2}", 2),
            (r"(?:(?=())|(?=()))?", 2),
            (r"(?:((?=))|((?=)))?", 2),
            (r"(?:(?=(?:()|())))?", 2),
            (r"(?:(?=(a))|(?=(b)))?", 2),
            (r"(?:(())?|(()))?", 4),
            (r"(?:()|())??", 2),
        ] {
            let choices = unit.repeat(count);
            let all = references(groups * count);
            for source in [
                format!("{choices}(?<={all})"),
                format!(r"{choices}{all}(?<=\1)"),
                format!(r"(?:{choices}{all}){{0}}(?<=\1)"),
                format!("(?:{choices}){{2}}(?<={all})"),
                format!(r"{choices}(?!{all})(?<=\1)"),
            ] {
                let pattern =
                    Pattern::compile(&source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
                assert_eq!(pattern.group_count(), groups * count, "{source}");
            }
        }
    }
}

#[test]
fn optional_empty_branches_restore_captures_and_required_branches_participate() {
    for source in [r"^(?:()|())?(?<=\1\2)$", r"^(?:()|()){0,2}(?<=\1\2)$"] {
        assert_eq!(
            Pattern::compile(source)
                .unwrap()
                .full_match("")
                .unwrap()
                .unwrap()
                .groups,
            [Some(""), None, None],
            "{source}",
        );
    }
    for source in [r"^(?:()|()){1,2}(?<=\1\2)$", r"^(?:()|()){2}(?<=\1\2)$"] {
        assert_eq!(
            Pattern::compile(source)
                .unwrap()
                .full_match("")
                .unwrap()
                .unwrap()
                .groups,
            [Some(""), Some(""), None],
            "{source}",
        );
    }

    // Capturing text inside a lookahead does not make its iteration consuming.
    // A required iteration keeps the first arm's capture; an optional one
    // restores both alternatives to their unentered state.
    for (source, expected) in [
        (r"^(?:(?=(a))|(?=(b)))?(?<=\1\2)a$", [Some("a"), None, None]),
        (
            r"^(?:(?=(a))|(?=(b))){1,2}a(?<=\1\2)$",
            [Some("a"), Some("a"), None],
        ),
    ] {
        assert_eq!(
            Pattern::compile(source)
                .unwrap()
                .full_match("a")
                .unwrap()
                .unwrap()
                .groups,
            expected,
            "{source}",
        );
    }
}

#[test]
fn unavailable_iterations_still_validate_inner_assertions_and_arithmetic() {
    assert_eq!(
        Pattern::compile(r"(?:(?<=a*)())?(?<=\1)"),
        Err(CompileError::Invalid {
            byte: 3,
            reason: "variable-length lookbehind",
        })
    );
    for assertion in ["=", "!"] {
        let source = format!(r"(?:(?{assertion}(aa)\1{{{}}}))?(?<=\1)", usize::MAX);
        assert_eq!(
            Pattern::compile(&source),
            Err(CompileError::Unsupported {
                byte: source.find(r"\1").unwrap(),
                feature: "pattern width exceeds addressable range",
            }),
            "{source}",
        );
    }
}

#[test]
fn nullable_consuming_branches_still_make_reference_widths_variable() {
    // Stopping leaves every capture unentered; a consuming iteration can enter
    // either one-scalar capture. Their sum can therefore be zero or positive.
    for count in [2, 4, 8] {
        let choices = r"(?:(a?)|(b?))?".repeat(count);
        let source = format!("{choices}(?<={})", references(2 * count));
        assert_eq!(
            Pattern::compile(&source),
            Err(CompileError::Invalid {
                byte: choices.len(),
                reason: "variable-length lookbehind",
            }),
            "{source}",
        );
    }
}
